use crate::compatibility::{validate_manifest, CURRENT_LANGUAGE_VERSION, CURRENT_MANIFEST_SCHEMA};
use crate::package::PackageManifest;
use num_compiler::{
    check_program,
    diagnostic::{Diagnostic, Severity},
    lexer,
    token::{Keyword, Symbol, Token, TokenKind},
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
    if write && !diagnostics.is_empty() {
        return Err("refusing to apply source migrations while blocking diagnostics exist; run `num migrate --source` for the diagnostic report".to_string());
    }
    let file_reports = files
        .iter()
        .map(plan_source_file_migration)
        .collect::<Vec<_>>();
    let changed = file_reports.iter().any(|file| file.changed);
    if write {
        for (file, report) in files.iter().zip(file_reports.iter()) {
            if !report.changed {
                continue;
            }
            let migrated = rewrite_source_file(file);
            fs::write(&file.path, migrated)
                .map_err(|err| format!("failed to write {}: {err}", file.path.display()))?;
        }
    }
    let mut actions = Vec::new();
    if file_reports.is_empty() {
        actions.push("no .num source files discovered".to_string());
    } else if changed {
        if file_reports.iter().any(|file| {
            file.actions
                .iter()
                .any(|action| action.starts_with("insert explicit module declaration"))
        }) {
            actions.push("insert missing explicit module declarations".to_string());
        }
        if file_reports.iter().any(|file| {
            file.actions.iter().any(|action| {
                action == "normalize legacy `rate_limit` metadata spelling to `rate limit`"
            })
        }) {
            actions.push("normalize legacy rate_limit metadata spelling".to_string());
        }
    } else {
        actions.push(format!(
            "no source rewrites required for language {CURRENT_LANGUAGE_VERSION}"
        ));
    }

    Ok(SourceMigrationReport {
        root,
        changed,
        applied: write && changed,
        actions,
        files: file_reports,
        diagnostics,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DiscoveredSourceFile {
    path: PathBuf,
    source: String,
    module_path: String,
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
            module_path: source_module_path(path, path.parent().unwrap_or_else(|| Path::new("."))),
        }],
    ))
}

fn collect_num_sources(root: &Path) -> Result<Vec<DiscoveredSourceFile>, String> {
    let mut files = Vec::new();
    collect_num_sources_inner(root, root, &mut files)?;
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(files)
}

fn collect_num_sources_inner(
    base: &Path,
    dir: &Path,
    files: &mut Vec<DiscoveredSourceFile>,
) -> Result<(), String> {
    if !dir.exists() {
        return Ok(());
    }
    let mut entries = fs::read_dir(dir)
        .map_err(|err| format!("failed to read source directory {}: {err}", dir.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("failed to read source directory entry: {err}"))?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect_num_sources_inner(base, &path, files)?;
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("num") {
            continue;
        }
        let source = fs::read_to_string(&path)
            .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
        files.push(DiscoveredSourceFile {
            module_path: source_module_path(&path, base),
            path,
            source,
        });
    }
    Ok(())
}

fn plan_source_file_migration(file: &DiscoveredSourceFile) -> SourceMigrationFileReport {
    let mut actions = Vec::new();
    if !has_explicit_module_declaration(&file.source) {
        actions.push(format!(
            "insert explicit module declaration `module {}`",
            file.module_path
        ));
    }
    if has_legacy_rate_limit_metadata(&file.path, &file.source) {
        actions.push("normalize legacy `rate_limit` metadata spelling to `rate limit`".to_string());
    }

    let changed = !actions.is_empty();
    if !changed {
        actions.push(format!(
            "no source rewrites required for language {CURRENT_LANGUAGE_VERSION}"
        ));
    }
    SourceMigrationFileReport {
        path: file.path.clone(),
        changed,
        actions,
    }
}

fn rewrite_source_file(file: &DiscoveredSourceFile) -> String {
    let mut source = replace_legacy_rate_limit_metadata(&file.path, &file.source);
    if !has_explicit_module_declaration(&source) {
        source = insert_module_declaration(&source, &file.module_path);
    } else {
        source = ensure_trailing_newline(&source);
    }
    source
}

fn has_explicit_module_declaration(source: &str) -> bool {
    source
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("//"))
        .is_some_and(|line| {
            line.strip_prefix("module")
                .is_some_and(module_prefix_boundary)
        })
}

fn module_prefix_boundary(rest: &str) -> bool {
    rest.chars()
        .next()
        .is_some_and(|ch| ch.is_whitespace() || ch == '.')
}

fn insert_module_declaration(source: &str, module_path: &str) -> String {
    let lines = source.lines().collect::<Vec<_>>();
    let insert_at = lines
        .iter()
        .position(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with("//")
        })
        .unwrap_or(lines.len());
    let mut out = String::new();
    for line in lines.iter().take(insert_at) {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str("module ");
    out.push_str(module_path);
    out.push('\n');
    if insert_at < lines.len() {
        out.push('\n');
    }
    for line in lines.iter().skip(insert_at) {
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn has_legacy_rate_limit_metadata(path: &Path, source: &str) -> bool {
    !legacy_rate_limit_replacements(path, source).is_empty()
}

fn replace_legacy_rate_limit_metadata(path: &Path, source: &str) -> String {
    let replacements = legacy_rate_limit_replacements(path, source);
    if replacements.is_empty() {
        return source.to_string();
    }

    let mut out = String::with_capacity(source.len() + replacements.len());
    let mut cursor = 0;
    for (start, end) in replacements {
        out.push_str(&source[cursor..start]);
        out.push_str("rate limit");
        cursor = end;
    }
    out.push_str(&source[cursor..]);
    out
}

fn legacy_rate_limit_replacements(path: &Path, source: &str) -> Vec<(usize, usize)> {
    let lexed = lexer::lex(&path.display().to_string(), source);
    if !lexed.diagnostics.is_empty() {
        return Vec::new();
    }

    let tokens = lexed.tokens;
    let mut replacements = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        match tokens[index].kind {
            TokenKind::Keyword(Keyword::Workflow) => {
                if let Some(header_start) = workflow_metadata_start(&tokens, index + 1) {
                    scan_legacy_rate_limit_header(&tokens, header_start, &mut replacements);
                }
            }
            TokenKind::Keyword(Keyword::Service) => {
                scan_legacy_rate_limit_header(&tokens, index + 2, &mut replacements);
            }
            _ => {}
        }
        index += 1;
    }
    replacements
}

fn workflow_metadata_start(tokens: &[Token], mut index: usize) -> Option<usize> {
    while index < tokens.len() {
        match tokens[index].kind {
            TokenKind::Symbol(Symbol::LParen) => break,
            TokenKind::Symbol(Symbol::LBrace) | TokenKind::Eof => return None,
            _ => index += 1,
        }
    }
    if index >= tokens.len() {
        return None;
    }

    let mut depth = 0;
    while index < tokens.len() {
        match tokens[index].kind {
            TokenKind::Symbol(Symbol::LParen) => depth += 1,
            TokenKind::Symbol(Symbol::RParen) => {
                depth -= 1;
                if depth == 0 {
                    return Some(index + 1);
                }
            }
            TokenKind::Eof => return None,
            _ => {}
        }
        index += 1;
    }
    None
}

fn scan_legacy_rate_limit_header(
    tokens: &[Token],
    mut index: usize,
    replacements: &mut Vec<(usize, usize)>,
) {
    while index < tokens.len() {
        match &tokens[index].kind {
            TokenKind::Symbol(Symbol::LBrace) | TokenKind::Eof => break,
            TokenKind::Ident(text) if text == "rate_limit" => {
                replacements.push((tokens[index].span.start, tokens[index].span.end));
            }
            _ => {}
        }
        index += 1;
    }
}

fn source_module_path(path: &Path, root: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    let mut components = relative
        .with_extension("")
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .map(sanitize_module_component)
        .filter(|component| !component.is_empty())
        .collect::<Vec<_>>();
    if components.is_empty() {
        components.push("main".to_string());
    }
    components.join(".")
}

fn sanitize_module_component(component: &str) -> String {
    let mut out = String::new();
    for ch in component.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    while out.contains("__") {
        out = out.replace("__", "_");
    }
    let out = out.trim_matches('_').to_string();
    if out.is_empty() {
        return "module".to_string();
    }
    if out
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
    {
        out
    } else {
        format!("m_{out}")
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
    fn source_migration_plans_missing_module_declaration() {
        let root = temp_project_dir("source_missing_module");
        fs::create_dir_all(root.join("src/workflows")).unwrap();
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
            root.join("src/workflows/refund-flow.num"),
            r#"// preserved header

fn ok() -> Bool {
    return true
}
"#,
        )
        .unwrap();

        let report = plan_source_migrations(&root, false).unwrap();

        assert!(report.changed);
        assert!(!report.applied);
        assert_eq!(report.files.len(), 1);
        assert_eq!(
            report.files[0].actions,
            vec!["insert explicit module declaration `module workflows.refund_flow`".to_string()]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn source_migration_writes_missing_module_declaration() {
        let root = temp_project_dir("source_write_module");
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
            r#"// preserved header
fn ok() -> Bool {
    return true
}
"#,
        )
        .unwrap();

        let report = plan_source_migrations(&root, true).unwrap();
        let migrated = fs::read_to_string(root.join("src/main.num")).unwrap();

        assert!(report.changed);
        assert!(report.applied);
        assert_eq!(
            migrated,
            r#"// preserved header
module main

fn ok() -> Bool {
    return true
}
"#
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn source_migration_plans_legacy_rate_limit_metadata() {
        let root = temp_project_dir("source_legacy_rate_limit_plan");
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
            r#"module main

workflow nightly(rate_limit: Int) rate_limit 2 per 1m {
}

service Billing rate_limit 5 per 10s {
}
"#,
        )
        .unwrap();

        let report = plan_source_migrations(&root, false).unwrap();

        assert!(report.changed);
        assert!(!report.applied);
        assert!(report
            .actions
            .iter()
            .any(|action| { action == "normalize legacy rate_limit metadata spelling" }));
        assert_eq!(
            report.files[0].actions,
            vec!["normalize legacy `rate_limit` metadata spelling to `rate limit`".to_string()]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn source_migration_writes_legacy_rate_limit_metadata_idempotently() {
        let root = temp_project_dir("source_legacy_rate_limit_write");
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
            r#"module main

workflow nightly() rate_limit 2 per 1m {
    let text = "rate_limit stays literal"
}

service Billing rate_limit 5 per 10s {
}
"#,
        )
        .unwrap();

        let report = plan_source_migrations(&root, true).unwrap();
        let migrated = fs::read_to_string(root.join("src/main.num")).unwrap();
        let second_report = plan_source_migrations(&root, false).unwrap();

        assert!(report.changed);
        assert!(report.applied);
        assert_eq!(
            migrated,
            r#"module main

workflow nightly() rate limit 2 per 1m {
    let text = "rate_limit stays literal"
}

service Billing rate limit 5 per 10s {
}
"#
        );
        assert!(!second_report.changed);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn source_migration_rate_limit_fixture_matches_expected() {
        let before = include_str!("../tests/fixtures/migration/source_rate_limit.before.num");
        let expected = include_str!("../tests/fixtures/migration/source_rate_limit.after.num");
        let path = Path::new("source_rate_limit.before.num");

        assert_eq!(replace_legacy_rate_limit_metadata(path, before), expected);
        assert_eq!(replace_legacy_rate_limit_metadata(path, expected), expected);
    }

    #[test]
    fn source_migration_combines_module_and_rate_limit_rewrites() {
        let root = temp_project_dir("source_combined_rewrites");
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
            r#"// preserved header
workflow nightly() rate_limit 2 per 1m {
}
"#,
        )
        .unwrap();

        let report = plan_source_migrations(&root, true).unwrap();
        let migrated = fs::read_to_string(root.join("src/main.num")).unwrap();

        assert!(report.changed);
        assert_eq!(
            report.files[0].actions,
            vec![
                "insert explicit module declaration `module main`".to_string(),
                "normalize legacy `rate_limit` metadata spelling to `rate limit`".to_string()
            ]
        );
        assert_eq!(
            migrated,
            r#"// preserved header
module main

workflow nightly() rate limit 2 per 1m {
}
"#
        );
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
    fn source_migration_write_is_rejected_when_diagnostics_block_rewrites() {
        let root = temp_project_dir("source_write");
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

        let err = plan_source_migrations(&root, true).unwrap_err();

        assert!(err.contains("blocking diagnostics"));
        fs::remove_dir_all(root).unwrap();
    }
}
