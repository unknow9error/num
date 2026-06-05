use crate::compatibility::{validate_manifest, CURRENT_LANGUAGE_VERSION, CURRENT_MANIFEST_SCHEMA};
use crate::package::PackageManifest;
use num_compiler::{
    check_program,
    diagnostic::{Diagnostic, Severity},
    SourceFile,
};
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceMigrationReport {
    pub root: PathBuf,
    pub changed: bool,
    pub applied: bool,
    pub actions: Vec<String>,
    pub files: Vec<SourceMigrationFileReport>,
    pub diagnostics: Vec<String>,
}

impl SourceMigrationReport {
    pub fn to_json(&self) -> Value {
        json!({
            "root": self.root.display().to_string(),
            "changed": self.changed,
            "applied": self.applied,
            "actions": self.actions,
            "files": self
                .files
                .iter()
                .map(SourceMigrationFileReport::to_json)
                .collect::<Vec<_>>(),
            "diagnostics": self.diagnostics,
        })
    }

    pub fn render_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "Source migration plan for {}\n",
            self.root.display()
        ));
        if self.changed {
            out.push_str(if self.applied {
                "Status: applied\n"
            } else {
                "Status: pending\n"
            });
        } else {
            out.push_str("Status: up to date\n");
        }
        for action in &self.actions {
            out.push_str(&format!("  - {action}\n"));
        }
        if !self.diagnostics.is_empty() {
            out.push_str("Blocking diagnostics:\n");
            for diagnostic in &self.diagnostics {
                out.push_str(&format!("  - {diagnostic}\n"));
            }
        }
        out.push_str("Files:\n");
        for file in &self.files {
            out.push_str(&format!(
                "  - {}: {}\n",
                file.path.display(),
                if file.changed {
                    "pending"
                } else {
                    "up to date"
                }
            ));
            for action in &file.actions {
                out.push_str(&format!("      - {action}\n"));
            }
        }
        out
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceMigrationFileReport {
    pub path: PathBuf,
    pub changed: bool,
    pub actions: Vec<String>,
}

impl SourceMigrationFileReport {
    fn to_json(&self) -> Value {
        json!({
            "path": self.path.display().to_string(),
            "changed": self.changed,
            "actions": self.actions,
        })
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

pub fn plan_source_migrations(path: &Path, write: bool) -> Result<SourceMigrationReport, String> {
    if write {
        return Err(
            "`num migrate --source` is planning-only until source rewrite rules are versioned"
                .to_string(),
        );
    }

    let (root, files) = discover_source_files(path)?;
    let program = check_program(
        files
            .iter()
            .map(|file| SourceFile {
                name: file.path.display().to_string(),
                source: file.source.clone(),
            })
            .collect(),
    );
    let diagnostics = blocking_diagnostics(&program.diagnostics);
    let file_reports = files
        .iter()
        .map(|file| plan_source_file_migration(&file.path, &file.source))
        .collect::<Vec<_>>();
    let changed = file_reports.iter().any(|file| file.changed);
    let mut actions = Vec::new();
    if file_reports.is_empty() {
        actions.push("no .num source files discovered".to_string());
    } else if changed {
        actions.push("source rewrite rules require manual review".to_string());
    } else {
        actions.push(format!(
            "no source rewrites required for language {CURRENT_LANGUAGE_VERSION}"
        ));
    }

    Ok(SourceMigrationReport {
        root,
        changed,
        applied: false,
        actions,
        files: file_reports,
        diagnostics,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DiscoveredSourceFile {
    path: PathBuf,
    source: String,
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

fn discover_source_files(path: &Path) -> Result<(PathBuf, Vec<DiscoveredSourceFile>), String> {
    if let Some(manifest) = PackageManifest::discover(path)? {
        let source_dir = manifest.source_dir();
        return Ok((source_dir.clone(), collect_num_sources(&source_dir)?));
    }

    if path.is_dir() {
        return Ok((path.to_path_buf(), collect_num_sources(path)?));
    }

    if !path.is_file() {
        return Err(format!(
            "{} is not a .num file or directory",
            path.display()
        ));
    }
    let source = fs::read_to_string(path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    Ok((
        path.parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf(),
        vec![DiscoveredSourceFile {
            path: path.to_path_buf(),
            source,
        }],
    ))
}

fn collect_num_sources(root: &Path) -> Result<Vec<DiscoveredSourceFile>, String> {
    let mut files = Vec::new();
    collect_num_sources_inner(root, &mut files)?;
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(files)
}

fn collect_num_sources_inner(
    root: &Path,
    files: &mut Vec<DiscoveredSourceFile>,
) -> Result<(), String> {
    if !root.exists() {
        return Ok(());
    }
    let mut entries = fs::read_dir(root)
        .map_err(|err| format!("failed to read source directory {}: {err}", root.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("failed to read source directory entry: {err}"))?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect_num_sources_inner(&path, files)?;
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("num") {
            continue;
        }
        let source = fs::read_to_string(&path)
            .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
        files.push(DiscoveredSourceFile { path, source });
    }
    Ok(())
}

fn plan_source_file_migration(path: &Path, _source: &str) -> SourceMigrationFileReport {
    SourceMigrationFileReport {
        path: path.to_path_buf(),
        changed: false,
        actions: vec![format!(
            "no source rewrites required for language {CURRENT_LANGUAGE_VERSION}"
        )],
    }
}

fn blocking_diagnostics(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .map(|diagnostic| {
            format!(
                "{}: {} at {}:{}:{}",
                diagnostic.code,
                diagnostic.message,
                diagnostic.span.source,
                diagnostic.span.line,
                diagnostic.span.column
            )
        })
        .collect()
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

    #[test]
    fn source_migration_reports_clean_project_sources() {
        let root = temp_project_dir("source_clean");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("num.toml"),
            r#"[project]
name = "app"
version = "0.1.0"
source = "src"
"#,
        )
        .unwrap();
        fs::write(
            root.join("src/main.num"),
            r#"module app.main

fn ok() -> Bool {
    return true
}
"#,
        )
        .unwrap();

        let report = plan_source_migrations(&root, false).unwrap();

        assert!(!report.changed);
        assert!(!report.applied);
        assert_eq!(report.files.len(), 1);
        assert!(report.diagnostics.is_empty());
        assert!(report.actions.iter().any(|action| {
            action
                == &format!("no source rewrites required for language {CURRENT_LANGUAGE_VERSION}")
        }));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn source_migration_reports_blocking_diagnostics() {
        let root = temp_project_dir("source_diagnostics");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("num.toml"),
            r#"[project]
name = "app"
version = "0.1.0"
source = "src"
"#,
        )
        .unwrap();
        fs::write(root.join("src/main.num"), "module app.main\nfn broken(").unwrap();

        let report = plan_source_migrations(&root, false).unwrap();

        assert!(!report.diagnostics.is_empty());
        assert!(report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("N")));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn source_migration_write_is_rejected_until_rewrite_rules_exist() {
        let root = temp_project_dir("source_write");

        let err = plan_source_migrations(&root, true).unwrap_err();

        assert!(err.contains("planning-only"));
        fs::remove_dir_all(root).unwrap();
    }
}
