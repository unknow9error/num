use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleasePlan {
    pub changelog_path: PathBuf,
    pub current_version: Semver,
    pub next_version: Semver,
    pub bump: SemverBump,
    pub sections: Vec<ReleaseSection>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SemverBump {
    None,
    Patch,
    Minor,
    Major,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseSection {
    pub bump: SemverBump,
    pub entries: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Semver {
    major: u32,
    minor: u32,
    patch: u32,
}

impl ReleasePlan {
    pub fn to_json(&self) -> Value {
        json!({
            "changelog": self.changelog_path.display().to_string(),
            "current_version": self.current_version.render(),
            "next_version": self.next_version.render(),
            "bump": self.bump.label(),
            "sections": self.sections.iter().map(ReleaseSection::to_json).collect::<Vec<_>>(),
        })
    }

    pub fn render_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "Release plan: {} -> {} ({})\n",
            self.current_version.render(),
            self.next_version.render(),
            self.bump.label()
        ));
        out.push_str(&format!("Changelog: {}\n", self.changelog_path.display()));
        for section in &self.sections {
            out.push_str(&format!(
                "  {}: {} entries\n",
                section.bump.label(),
                section.entries.len()
            ));
        }
        out
    }
}

impl ReleaseSection {
    fn to_json(&self) -> Value {
        json!({
            "bump": self.bump.label(),
            "entries": self.entries,
        })
    }
}

impl SemverBump {
    fn parse_heading(heading: &str) -> Option<Self> {
        match heading.trim().to_ascii_lowercase().as_str() {
            "major" | "breaking" | "breaking changes" => Some(Self::Major),
            "minor" | "added" | "changed" => Some(Self::Minor),
            "patch" | "fixed" | "fixes" | "security" | "docs" | "documentation" => {
                Some(Self::Patch)
            }
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Patch => "patch",
            Self::Minor => "minor",
            Self::Major => "major",
        }
    }
}

impl Semver {
    fn parse(value: &str) -> Option<Self> {
        let mut parts = value.trim().split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        let patch = parts.next()?.parse().ok()?;
        if parts.next().is_some() {
            return None;
        }
        Some(Self {
            major,
            minor,
            patch,
        })
    }

    fn bump(self, bump: SemverBump) -> Self {
        match bump {
            SemverBump::None => self,
            SemverBump::Patch => Self {
                patch: self.patch.saturating_add(1),
                ..self
            },
            SemverBump::Minor => Self {
                major: self.major,
                minor: self.minor.saturating_add(1),
                patch: 0,
            },
            SemverBump::Major => Self {
                major: self.major.saturating_add(1),
                minor: 0,
                patch: 0,
            },
        }
    }

    fn render(&self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }
}

pub fn plan_from_changelog(
    changelog_path: &Path,
    current_version: &str,
) -> Result<ReleasePlan, String> {
    let current_version = Semver::parse(current_version)
        .ok_or_else(|| format!("invalid current version `{current_version}`; expected x.y.z"))?;
    let source = fs::read_to_string(changelog_path)
        .map_err(|err| format!("failed to read {}: {err}", changelog_path.display()))?;
    let sections = parse_unreleased_sections(&source)?;
    let bump = sections
        .iter()
        .map(|section| section.bump)
        .max()
        .unwrap_or(SemverBump::None);
    if bump == SemverBump::None {
        return Err(
            "CHANGELOG.md Unreleased must include Major, Minor, or Patch entries".to_string(),
        );
    }
    Ok(ReleasePlan {
        changelog_path: changelog_path.to_path_buf(),
        current_version,
        next_version: current_version.bump(bump),
        bump,
        sections,
    })
}

fn parse_unreleased_sections(source: &str) -> Result<Vec<ReleaseSection>, String> {
    let mut in_unreleased = false;
    let mut current_section = None::<ReleaseSection>;
    let mut sections = Vec::new();

    for line in source.lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            if in_unreleased {
                break;
            }
            in_unreleased = heading.trim() == "Unreleased";
            continue;
        }
        if !in_unreleased {
            continue;
        }
        if let Some(heading) = line.strip_prefix("### ") {
            if let Some(section) = current_section.take() {
                if !section.entries.is_empty() {
                    sections.push(section);
                }
            }
            current_section = SemverBump::parse_heading(heading).map(|bump| ReleaseSection {
                bump,
                entries: Vec::new(),
            });
            continue;
        }
        let Some(section) = current_section.as_mut() else {
            continue;
        };
        if line.starts_with("- ") {
            section
                .entries
                .push(line.trim_start_matches("- ").trim().to_string());
        } else if line.starts_with("  ") {
            if let Some(entry) = section.entries.last_mut() {
                entry.push(' ');
                entry.push_str(line.trim());
            }
        }
    }

    if let Some(section) = current_section.take() {
        if !section.entries.is_empty() {
            sections.push(section);
        }
    }
    if !in_unreleased {
        return Err("CHANGELOG.md must include an `## Unreleased` section".to_string());
    }
    Ok(sections)
}

#[cfg(test)]
mod tests {
    use super::{plan_from_changelog, SemverBump};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn release_plan_detects_highest_semver_bump() {
        let path = temp_changelog(
            r#"# Changelog

## Unreleased

### Patch

- Fix CLI help.

### Minor

- Add release-plan command.

## 0.1.1 - 2026-06-07
"#,
        );

        let plan = plan_from_changelog(&path, "0.1.1").unwrap();

        assert_eq!(plan.bump, SemverBump::Minor);
        assert_eq!(plan.next_version.render(), "0.2.0");
        assert_eq!(plan.sections.len(), 2);
    }

    #[test]
    fn release_plan_requires_unreleased_semver_entries() {
        let path = temp_changelog(
            r#"# Changelog

## Unreleased

### Notes

- Missing classification.
"#,
        );

        let err = plan_from_changelog(&path, "0.1.1").unwrap_err();

        assert!(err.contains("Unreleased must include Major, Minor, or Patch"));
    }

    fn temp_changelog(source: &str) -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("num_release_plan_{stamp}"));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("CHANGELOG.md");
        fs::write(&path, source).unwrap();
        path
    }
}
