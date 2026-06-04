use crate::compatibility::{validate_manifest, CURRENT_LANGUAGE_VERSION, CURRENT_MANIFEST_SCHEMA};
use crate::package::PackageManifest;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationReport {
    pub manifest_path: PathBuf,
    pub changed: bool,
    pub applied: bool,
    pub actions: Vec<String>,
}

impl MigrationReport {
    pub fn to_json(&self) -> Value {
        json!({
            "manifest": self.manifest_path.display().to_string(),
            "changed": self.changed,
            "applied": self.applied,
            "actions": self.actions,
        })
    }

    pub fn render_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "Migration plan for {}\n",
            self.manifest_path.display()
        ));
        if self.changed {
            out.push_str(if self.applied {
                "Status: applied\n"
            } else {
                "Status: pending\n"
            });
            for action in &self.actions {
                out.push_str(&format!("  - {action}\n"));
            }
        } else {
            out.push_str("Status: up to date\n");
        }
        out
    }
}

pub fn migrate_manifest(path: &Path, write: bool) -> Result<MigrationReport, String> {
    let (root, manifest_path, source) = discover_manifest_source(path)?;
    let planned = plan_manifest_source(&source)?;
    let manifest_source = planned.source;
    let manifest = PackageManifest::parse(&root, &manifest_path, &manifest_source);
    validate_manifest(&manifest)?;

    if write && planned.changed {
        fs::write(&manifest_path, &manifest_source)
            .map_err(|err| format!("failed to write {}: {err}", manifest_path.display()))?;
    }

    Ok(MigrationReport {
        manifest_path,
        changed: planned.changed,
        applied: write && planned.changed,
        actions: planned.actions,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PlannedManifest {
    source: String,
    changed: bool,
    actions: Vec<String>,
}

fn discover_manifest_source(path: &Path) -> Result<(PathBuf, PathBuf, String), String> {
    let mut cursor = if path.is_file() {
        path.parent().map(Path::to_path_buf)
    } else {
        Some(path.to_path_buf())
    };

    while let Some(dir) = cursor {
        let manifest_path = dir.join("num.toml");
        if manifest_path.is_file() {
            let source = fs::read_to_string(&manifest_path)
                .map_err(|err| format!("failed to read {}: {err}", manifest_path.display()))?;
            return Ok((dir, manifest_path, source));
        }
        cursor = dir.parent().map(Path::to_path_buf);
    }

    Err(format!("no num.toml found for {}", path.display()))
}

fn plan_manifest_source(source: &str) -> Result<PlannedManifest, String> {
    let language_section = find_section(source, "language");
    let Some((start, end)) = language_section else {
        let mut migrated = String::new();
        migrated.push_str(&language_section_header());
        migrated.push('\n');
        migrated.push_str(source);
        if !migrated.ends_with('\n') {
            migrated.push('\n');
        }
        return Ok(PlannedManifest {
            source: migrated,
            changed: true,
            actions: vec![
                "insert [language] section with current language/schema metadata".to_string(),
            ],
        });
    };

    let mut lines = source.lines().map(str::to_string).collect::<Vec<_>>();
    let keys = section_keys(&lines, start + 1, end)?;
    let mut actions = Vec::new();

    let mut insert = Vec::new();
    if !keys.iter().any(|(key, _)| key == "version") {
        insert.push(format!("version = \"{CURRENT_LANGUAGE_VERSION}\""));
        actions.push("add [language].version".to_string());
    }
    if !keys.iter().any(|(key, _)| key == "compatibility") {
        insert.push("compatibility = \"minor\"".to_string());
        actions.push("add [language].compatibility".to_string());
    }
    if let Some((_, index)) = keys.iter().find(|(key, _)| key == "manifest_schema") {
        let schema = manifest_schema_value(&lines[*index])?;
        if schema == 0 {
            lines[*index] = format!("manifest_schema = {CURRENT_MANIFEST_SCHEMA}");
            actions.push("upgrade [language].manifest_schema from 0 to 1".to_string());
        } else if schema > CURRENT_MANIFEST_SCHEMA {
            return Err(format!(
                "cannot migrate manifest schema {schema}; this num CLI supports schema {CURRENT_MANIFEST_SCHEMA}"
            ));
        }
    } else {
        insert.push(format!("manifest_schema = {CURRENT_MANIFEST_SCHEMA}"));
        actions.push("add [language].manifest_schema".to_string());
    }

    if !insert.is_empty() {
        lines.splice(start + 1..start + 1, insert);
    }

    if actions.is_empty() {
        return Ok(PlannedManifest {
            source: ensure_trailing_newline(source),
            changed: false,
            actions,
        });
    }

    Ok(PlannedManifest {
        source: format!("{}\n", lines.join("\n")),
        changed: true,
        actions,
    })
}

fn language_section_header() -> String {
    format!(
        "[language]\nversion = \"{CURRENT_LANGUAGE_VERSION}\"\ncompatibility = \"minor\"\nmanifest_schema = {CURRENT_MANIFEST_SCHEMA}\n"
    )
}

fn find_section(source: &str, name: &str) -> Option<(usize, usize)> {
    let lines = source.lines().collect::<Vec<_>>();
    let start = lines
        .iter()
        .position(|line| section_name(line) == Some(name))?;
    let end = lines
        .iter()
        .enumerate()
        .skip(start + 1)
        .find_map(|(index, line)| section_name(line).map(|_| index))
        .unwrap_or(lines.len());
    Some((start, end))
}

fn section_name(line: &str) -> Option<&str> {
    let line = line.split('#').next().unwrap_or("").trim();
    Some(line.strip_prefix('[')?.strip_suffix(']')?.trim())
}

fn section_keys(
    lines: &[String],
    start: usize,
    end: usize,
) -> Result<Vec<(String, usize)>, String> {
    let mut keys = Vec::new();
    for (index, line) in lines.iter().enumerate().take(end).skip(start) {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, _)) = line.split_once('=') else {
            continue;
        };
        keys.push((normalize_key(key), index));
    }
    Ok(keys)
}

fn manifest_schema_value(line: &str) -> Result<u32, String> {
    let value = line
        .split('#')
        .next()
        .unwrap_or("")
        .split_once('=')
        .map(|(_, value)| value.trim())
        .ok_or_else(|| "invalid [language].manifest_schema entry".to_string())?;
    value
        .parse()
        .map_err(|_| format!("invalid [language].manifest_schema `{value}`"))
}

fn normalize_key(key: &str) -> String {
    key.trim().trim_matches('"').trim_matches('\'').to_string()
}

fn ensure_trailing_newline(source: &str) -> String {
    if source.ends_with('\n') {
        source.to_string()
    } else {
        format!("{source}\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_project_dir(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = env::temp_dir().join(format!("num_migration_{name}_{stamp}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn plans_legacy_manifest_language_section() {
        let source = r#"[project]
name = "app"
version = "0.1.0"
"#;

        let plan = plan_manifest_source(source).unwrap();

        assert!(plan.changed);
        assert!(plan.source.starts_with("[language]\n"));
        assert!(plan.source.contains("manifest_schema = 1"));
        assert_eq!(plan.actions.len(), 1);
    }

    #[test]
    fn fills_partial_language_section() {
        let source = r#"[language]
version = "0.1.0"

[project]
name = "app"
version = "0.1.0"
"#;

        let plan = plan_manifest_source(source).unwrap();

        assert!(plan.changed);
        assert!(plan.source.contains("compatibility = \"minor\""));
        assert!(plan.source.contains("manifest_schema = 1"));
    }

    #[test]
    fn writes_manifest_when_requested() {
        let root = temp_project_dir("write");
        let manifest_path = root.join("num.toml");
        fs::write(
            &manifest_path,
            r#"[project]
name = "app"
version = "0.1.0"
"#,
        )
        .unwrap();

        let report = migrate_manifest(&root, true).unwrap();
        let contents = fs::read_to_string(&manifest_path).unwrap();

        assert!(report.changed);
        assert!(report.applied);
        assert!(contents.starts_with("[language]\n"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_future_manifest_schema() {
        let source = r#"[language]
version = "0.1.0"
manifest_schema = 2

[project]
name = "app"
version = "0.1.0"
"#;

        assert!(plan_manifest_source(source)
            .unwrap_err()
            .contains("cannot migrate manifest schema 2"));
    }
}
