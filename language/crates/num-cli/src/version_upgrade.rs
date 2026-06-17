use crate::compatibility::{CURRENT_LANGUAGE_VERSION, CURRENT_MANIFEST_SCHEMA};
use crate::package::{DependencySource, PackageManifest};
use crate::registry::LocalRegistry;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionUpgradeOptions {
    pub target_language_version: String,
    pub target_project_version: Option<String>,
    pub write: bool,
    pub include_dependencies: bool,
    pub write_dependencies: bool,
}

impl Default for VersionUpgradeOptions {
    fn default() -> Self {
        Self {
            target_language_version: CURRENT_LANGUAGE_VERSION.to_string(),
            target_project_version: None,
            write: false,
            include_dependencies: false,
            write_dependencies: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionUpgradeReport {
    pub manifest_path: PathBuf,
    pub package_name: String,
    pub package_version: String,
    pub changed: bool,
    pub applied: bool,
    pub actions: Vec<String>,
    pub language: VersionChange,
    pub project: Option<VersionChange>,
}

impl VersionUpgradeReport {
    pub fn to_json(&self) -> Value {
        json!({
            "manifest": self.manifest_path.display().to_string(),
            "package": {
                "name": self.package_name,
                "version": self.package_version,
            },
            "changed": self.changed,
            "applied": self.applied,
            "actions": self.actions,
            "language": self.language.to_json(),
            "project": self.project.as_ref().map(VersionChange::to_json),
        })
    }

    pub fn render_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "Version upgrade plan for {}\n",
            self.manifest_path.display()
        ));
        out.push_str(&format!(
            "  package: {} {}\n",
            self.package_name, self.package_version
        ));
        out.push_str(if self.changed {
            if self.applied {
                "Status: applied\n"
            } else {
                "Status: pending\n"
            }
        } else {
            "Status: up to date\n"
        });
        out.push_str(&format!(
            "  language: {} -> {}\n",
            self.language.from, self.language.to
        ));
        if let Some(project) = &self.project {
            out.push_str(&format!("  project: {} -> {}\n", project.from, project.to));
        }
        for action in &self.actions {
            out.push_str(&format!("  - {action}\n"));
        }
        out
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionUpgradeGraphReport {
    pub root: VersionUpgradeReport,
    pub dependencies: Vec<VersionUpgradeDependencyReport>,
    pub changed: bool,
    pub applied: bool,
}

impl VersionUpgradeGraphReport {
    pub fn to_json(&self) -> Value {
        json!({
            "changed": self.changed,
            "applied": self.applied,
            "root": self.root.to_json(),
            "dependencies": self
                .dependencies
                .iter()
                .map(VersionUpgradeDependencyReport::to_json)
                .collect::<Vec<_>>(),
        })
    }

    pub fn render_text(&self) -> String {
        let mut out = String::new();
        out.push_str("Version upgrade graph plan\n");
        out.push_str(if self.changed {
            if self.applied {
                "Status: applied\n\n"
            } else {
                "Status: pending\n\n"
            }
        } else {
            "Status: up to date\n\n"
        });
        out.push_str("Workspace:\n");
        out.push_str(&indent_text(&self.root.render_text()));
        if self.dependencies.is_empty() {
            out.push_str("\nDependencies: none\n");
            return out;
        }
        out.push_str("\nDependencies:\n");
        for dependency in &self.dependencies {
            out.push_str(&format!(
                "  [{}] {} {} ({})\n",
                dependency.source,
                dependency.report.package_name,
                dependency.report.package_version,
                dependency.report.manifest_path.display()
            ));
            out.push_str(&indent_text(&dependency.report.render_text()));
        }
        out
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionUpgradeDependencyReport {
    pub source: String,
    pub report: VersionUpgradeReport,
}

impl VersionUpgradeDependencyReport {
    fn to_json(&self) -> Value {
        json!({
            "source": self.source,
            "report": self.report.to_json(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionChange {
    pub from: String,
    pub to: String,
    pub changed: bool,
}

impl VersionChange {
    fn new(from: impl Into<String>, to: impl Into<String>) -> Self {
        let from = from.into();
        let to = to.into();
        Self {
            changed: from != to,
            from,
            to,
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "from": self.from,
            "to": self.to,
            "changed": self.changed,
        })
    }
}

pub fn upgrade_manifest_versions(
    path: &Path,
    options: &VersionUpgradeOptions,
) -> Result<VersionUpgradeReport, String> {
    validate_version_options(options)?;

    let (root, manifest_path, source) = discover_manifest_source(path)?;
    let manifest = PackageManifest::parse(&root, &manifest_path, &source);
    upgrade_manifest(&manifest, options, options.write)
}

pub fn upgrade_manifest_graph_versions(
    path: &Path,
    options: &VersionUpgradeOptions,
) -> Result<VersionUpgradeGraphReport, String> {
    validate_version_options(options)?;
    if options.write_dependencies && !options.write {
        return Err("--write-dependencies requires --write".to_string());
    }

    let (root, manifest_path, source) = discover_manifest_source(path)?;
    let root_manifest = PackageManifest::parse(&root, &manifest_path, &source);
    let manifests = collect_upgrade_manifest_graph(&root_manifest)?;
    let mut root_report = None;
    let mut dependencies = Vec::new();

    for graph_manifest in manifests {
        let mut package_options = options.clone();
        if graph_manifest.source != "workspace" {
            package_options.target_project_version = None;
        }
        let should_write = if graph_manifest.source == "workspace" {
            options.write
        } else {
            options.write_dependencies
        };
        let report = upgrade_manifest(&graph_manifest.manifest, &package_options, should_write)?;
        if graph_manifest.source == "workspace" {
            root_report = Some(report);
        } else {
            dependencies.push(VersionUpgradeDependencyReport {
                source: graph_manifest.source,
                report,
            });
        }
    }

    dependencies.sort_by(|left, right| {
        left.report
            .manifest_path
            .cmp(&right.report.manifest_path)
            .then_with(|| left.source.cmp(&right.source))
    });

    let root = root_report.ok_or_else(|| "upgrade graph did not include workspace".to_string())?;
    let changed = root.changed
        || dependencies
            .iter()
            .any(|dependency| dependency.report.changed);
    let pending = (root.changed && !root.applied)
        || dependencies
            .iter()
            .any(|dependency| dependency.report.changed && !dependency.report.applied);
    let applied = changed && !pending;
    Ok(VersionUpgradeGraphReport {
        root,
        dependencies,
        changed,
        applied,
    })
}

fn upgrade_manifest(
    manifest: &PackageManifest,
    options: &VersionUpgradeOptions,
    write: bool,
) -> Result<VersionUpgradeReport, String> {
    let source = fs::read_to_string(&manifest.path)
        .map_err(|err| format!("failed to read {}: {err}", manifest.path.display()))?;
    let manifest = PackageManifest::parse(&manifest.root, &manifest.path, &source);
    let planned = plan_version_upgrade(&source, &manifest, options)?;

    if write && planned.changed {
        fs::write(&manifest.path, &planned.source)
            .map_err(|err| format!("failed to write {}: {err}", manifest.path.display()))?;
    }

    Ok(VersionUpgradeReport {
        manifest_path: manifest.path,
        package_name: manifest.project.name,
        package_version: manifest.project.version,
        changed: planned.changed,
        applied: write && planned.changed,
        actions: planned.actions,
        language: planned.language,
        project: planned.project,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PlannedVersionUpgrade {
    source: String,
    changed: bool,
    actions: Vec<String>,
    language: VersionChange,
    project: Option<VersionChange>,
}

fn plan_version_upgrade(
    source: &str,
    manifest: &PackageManifest,
    options: &VersionUpgradeOptions,
) -> Result<PlannedVersionUpgrade, String> {
    ensure_not_downgrade(
        &manifest.language.version,
        &options.target_language_version,
        "[language].version",
    )?;
    if let Some(project_version) = &options.target_project_version {
        ensure_not_downgrade(
            &manifest.project.version,
            project_version,
            "[project].version",
        )?;
    }

    let mut lines = source.lines().map(str::to_string).collect::<Vec<_>>();
    let mut actions = Vec::new();
    let mut changed = false;

    ensure_language_metadata(&mut lines, &mut actions)?;

    let language = VersionChange::new(
        manifest.language.version.clone(),
        options.target_language_version.clone(),
    );
    if language.changed {
        changed = true;
        upsert_section_key(
            &mut lines,
            "language",
            "version",
            &format!("\"{}\"", options.target_language_version),
            &mut actions,
        )?;
        actions.push(format!(
            "upgrade [language].version from {} to {}",
            language.from, language.to
        ));
    }
    let project = if let Some(project_version) = &options.target_project_version {
        let project = VersionChange::new(manifest.project.version.clone(), project_version.clone());
        if project.changed {
            changed = true;
            upsert_section_key(
                &mut lines,
                "project",
                "version",
                &format!("\"{project_version}\""),
                &mut actions,
            )?;
            actions.push(format!(
                "upgrade [project].version from {} to {}",
                project.from, project.to
            ));
        }
        Some(project)
    } else {
        None
    };

    let source = if changed || !actions.is_empty() {
        format!("{}\n", lines.join("\n"))
    } else {
        ensure_trailing_newline(source)
    };
    Ok(PlannedVersionUpgrade {
        source,
        changed: changed || !actions.is_empty(),
        actions,
        language,
        project,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UpgradeGraphManifest {
    source: String,
    manifest: PackageManifest,
}

fn collect_upgrade_manifest_graph(
    manifest: &PackageManifest,
) -> Result<Vec<UpgradeGraphManifest>, String> {
    let mut manifests = Vec::new();
    let mut visited = BTreeSet::new();
    collect_upgrade_manifest_graph_inner(
        manifest,
        "workspace".to_string(),
        &mut visited,
        &mut manifests,
    )?;
    Ok(manifests)
}

fn collect_upgrade_manifest_graph_inner(
    manifest: &PackageManifest,
    source: String,
    visited: &mut BTreeSet<PathBuf>,
    manifests: &mut Vec<UpgradeGraphManifest>,
) -> Result<(), String> {
    let key = fs::canonicalize(&manifest.path).unwrap_or_else(|_| manifest.path.clone());
    if !visited.insert(key) {
        return Ok(());
    }

    manifests.push(UpgradeGraphManifest {
        source,
        manifest: manifest.clone(),
    });

    let mut dependencies = manifest.path_dependency_manifests()?;
    let registry = LocalRegistry::discover_for(manifest);
    for dependency in &manifest.dependencies {
        let DependencySource::Registry = dependency.source else {
            continue;
        };
        let registry = registry.as_ref().ok_or_else(|| {
            format!(
                "registry dependency `{}` requires [registry].path or NUM_REGISTRY_PATH",
                dependency.name
            )
        })?;
        if let Some(resolved) = registry.resolve(dependency)? {
            dependencies.push(resolved);
        }
    }
    dependencies.sort_by(|left, right| {
        left.project
            .name
            .cmp(&right.project.name)
            .then_with(|| left.path.cmp(&right.path))
    });

    for dependency_manifest in dependencies {
        let source = dependency_source_label(manifest, &dependency_manifest);
        collect_upgrade_manifest_graph_inner(&dependency_manifest, source, visited, manifests)?;
    }

    Ok(())
}

fn dependency_source_label(parent: &PackageManifest, dependency: &PackageManifest) -> String {
    for direct in &parent.dependencies {
        match &direct.source {
            DependencySource::Path(path)
                if direct.name == dependency.project.name
                    && direct.version == dependency.project.version =>
            {
                return format!("path:{path}");
            }
            DependencySource::Registry
                if direct.name == dependency.project.name
                    && direct.version == dependency.project.version =>
            {
                return "registry".to_string();
            }
            DependencySource::Git(_) => {}
            DependencySource::Registry => {}
            DependencySource::Path(_) => {}
        }
    }
    "dependency".to_string()
}

fn validate_version_options(options: &VersionUpgradeOptions) -> Result<(), String> {
    validate_semver(&options.target_language_version, "target language version")?;
    if let Some(project_version) = &options.target_project_version {
        validate_semver(project_version, "target project version")?;
    }
    Ok(())
}

fn indent_text(text: &str) -> String {
    text.lines()
        .map(|line| format!("  {line}\n"))
        .collect::<String>()
}

fn ensure_language_metadata(
    lines: &mut Vec<String>,
    actions: &mut Vec<String>,
) -> Result<(), String> {
    if find_section(lines, "language").is_none() {
        let insert = vec![
            "[language]".to_string(),
            format!("version = \"{CURRENT_LANGUAGE_VERSION}\""),
            "compatibility = \"minor\"".to_string(),
            format!("manifest_schema = {CURRENT_MANIFEST_SCHEMA}"),
            String::new(),
        ];
        lines.splice(0..0, insert);
        actions.push("insert [language] section with current language/schema metadata".to_string());
        return Ok(());
    }
    if section_key(lines, "language", "version")?.is_none() {
        upsert_section_key(
            lines,
            "language",
            "version",
            &format!("\"{CURRENT_LANGUAGE_VERSION}\""),
            actions,
        )?;
        actions.push("add [language].version".to_string());
    }
    if section_key(lines, "language", "compatibility")?.is_none() {
        upsert_section_key(lines, "language", "compatibility", "\"minor\"", actions)?;
        actions.push("add [language].compatibility".to_string());
    }
    if section_key(lines, "language", "manifest_schema")?.is_none() {
        upsert_section_key(
            lines,
            "language",
            "manifest_schema",
            &CURRENT_MANIFEST_SCHEMA.to_string(),
            actions,
        )?;
        actions.push("add [language].manifest_schema".to_string());
    }
    Ok(())
}

fn upsert_section_key(
    lines: &mut Vec<String>,
    section: &str,
    key: &str,
    value: &str,
    _actions: &mut Vec<String>,
) -> Result<(), String> {
    let Some((start, end)) = find_section(lines, section) else {
        return Err(format!("missing [{section}] section"));
    };
    if let Some(index) = section_key(lines, section, key)? {
        lines[index] = format!("{key} = {value}");
        return Ok(());
    }
    lines.insert(start + 1, format!("{key} = {value}"));
    let _ = end;
    Ok(())
}

fn section_key(lines: &[String], section: &str, key: &str) -> Result<Option<usize>, String> {
    let Some((start, end)) = find_section(lines, section) else {
        return Ok(None);
    };
    for (index, line) in lines.iter().enumerate().take(end).skip(start + 1) {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let Some((found, _)) = line.split_once('=') else {
            continue;
        };
        if normalize_key(found) == key {
            return Ok(Some(index));
        }
    }
    Ok(None)
}

fn find_section(lines: &[String], name: &str) -> Option<(usize, usize)> {
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

fn section_name(line: &str) -> Option<&str> {
    let line = line.split('#').next().unwrap_or("").trim();
    Some(line.strip_prefix('[')?.strip_suffix(']')?.trim())
}

fn normalize_key(key: &str) -> String {
    key.trim().trim_matches('"').trim_matches('\'').to_string()
}

fn ensure_not_downgrade(from: &str, to: &str, field: &str) -> Result<(), String> {
    let from = Semver::parse(from).ok_or_else(|| format!("invalid current {field} `{from}`"))?;
    let to = Semver::parse(to).ok_or_else(|| format!("invalid target {field} `{to}`"))?;
    if to < from {
        return Err(format!(
            "refusing to downgrade {field} from {} to {}",
            from.render(),
            to.render()
        ));
    }
    Ok(())
}

fn validate_semver(value: &str, label: &str) -> Result<(), String> {
    Semver::parse(value)
        .map(|_| ())
        .ok_or_else(|| format!("invalid {label} `{value}`; expected x.y.z"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Semver {
    major: u32,
    minor: u32,
    patch: u32,
}

impl Semver {
    fn parse(value: &str) -> Option<Self> {
        let mut parts = value.split('.');
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

    fn render(self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }
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
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn plans_language_version_upgrade() {
        let source = r#"[language]
version = "0.0.1"
compatibility = "minor"
manifest_schema = 1

[project]
name = "app"
version = "0.1.0"
"#;
        let manifest = PackageManifest::parse(
            Path::new("/tmp/app"),
            Path::new("/tmp/app/num.toml"),
            source,
        );
        let plan =
            plan_version_upgrade(source, &manifest, &VersionUpgradeOptions::default()).unwrap();

        assert!(plan.changed);
        assert!(plan
            .source
            .contains(&format!("version = \"{CURRENT_LANGUAGE_VERSION}\"")));
        assert!(plan
            .actions
            .iter()
            .any(|action| action.contains("upgrade [language].version")));
    }

    #[test]
    fn plans_project_version_upgrade_when_requested() {
        let source = r#"[language]
version = "0.1.0"
compatibility = "minor"
manifest_schema = 1

[project]
name = "app"
version = "0.1.0"
"#;
        let manifest = PackageManifest::parse(
            Path::new("/tmp/app"),
            Path::new("/tmp/app/num.toml"),
            source,
        );
        let options = VersionUpgradeOptions {
            target_project_version: Some("0.3.0".to_string()),
            ..VersionUpgradeOptions::default()
        };

        let plan = plan_version_upgrade(source, &manifest, &options).unwrap();

        assert!(plan.changed);
        assert!(plan
            .source
            .contains("[project]\nname = \"app\"\nversion = \"0.3.0\""));
        assert_eq!(plan.project.unwrap().to, "0.3.0");
    }

    #[test]
    fn inserts_missing_language_metadata() {
        let source = r#"[project]
name = "app"
version = "0.1.0"
"#;
        let manifest = PackageManifest::parse(
            Path::new("/tmp/app"),
            Path::new("/tmp/app/num.toml"),
            source,
        );
        let plan =
            plan_version_upgrade(source, &manifest, &VersionUpgradeOptions::default()).unwrap();

        assert!(plan.source.starts_with("[language]\n"));
        assert!(plan.source.contains("compatibility = \"minor\""));
        assert!(plan.source.contains("manifest_schema = 1"));
    }

    #[test]
    fn fills_partial_language_section_without_version() {
        let source = r#"[language]
compatibility = "minor"

[project]
name = "app"
version = "0.1.0"
"#;
        let manifest = PackageManifest::parse(
            Path::new("/tmp/app"),
            Path::new("/tmp/app/num.toml"),
            source,
        );
        let plan =
            plan_version_upgrade(source, &manifest, &VersionUpgradeOptions::default()).unwrap();

        assert!(plan
            .source
            .contains(&format!("version = \"{CURRENT_LANGUAGE_VERSION}\"")));
        assert!(plan.source.contains("manifest_schema = 1"));
    }

    #[test]
    fn rejects_version_downgrade() {
        let source = r#"[language]
version = "0.4.0"
compatibility = "minor"
manifest_schema = 1

[project]
name = "app"
version = "0.1.0"
"#;
        let manifest = PackageManifest::parse(
            Path::new("/tmp/app"),
            Path::new("/tmp/app/num.toml"),
            source,
        );
        let err =
            plan_version_upgrade(source, &manifest, &VersionUpgradeOptions::default()).unwrap_err();

        assert!(err.contains("refusing to downgrade [language].version"));
    }

    #[test]
    fn writes_upgrade_when_requested() {
        let root = temp_dir("write");
        fs::create_dir_all(&root).unwrap();
        let manifest_path = root.join("num.toml");
        fs::write(
            &manifest_path,
            r#"[language]
version = "0.0.1"
compatibility = "minor"
manifest_schema = 1

[project]
name = "app"
version = "0.1.0"
"#,
        )
        .unwrap();

        let report = upgrade_manifest_versions(
            &root,
            &VersionUpgradeOptions {
                write: true,
                ..VersionUpgradeOptions::default()
            },
        )
        .unwrap();
        let contents = fs::read_to_string(&manifest_path).unwrap();

        assert!(report.applied);
        assert!(contents.contains(&format!("version = \"{CURRENT_LANGUAGE_VERSION}\"")));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn graph_upgrade_reports_transitive_path_dependencies() {
        let root = temp_dir("graph_root");
        let shared = sibling_dir(&root, "shared");
        let core = sibling_dir(&root, "core");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&shared).unwrap();
        fs::create_dir_all(&core).unwrap();

        write_manifest(
            &root,
            &format!(
                r#"[project]
name = "app"
version = "0.1.0"

[dependencies]
shared = {{ path = "{}", version = "0.3.0" }}
"#,
                toml_path(&shared)
            ),
        );
        write_manifest(
            &shared,
            &format!(
                r#"[language]
version = "0.0.1"
compatibility = "minor"
manifest_schema = 1

[project]
name = "shared"
version = "0.3.0"

[dependencies]
core = {{ path = "{}", version = "1.0.0" }}
"#,
                toml_path(&core)
            ),
        );
        write_manifest(
            &core,
            r#"[project]
name = "core"
version = "1.0.0"
"#,
        );

        let report = upgrade_manifest_graph_versions(
            &root,
            &VersionUpgradeOptions {
                include_dependencies: true,
                ..VersionUpgradeOptions::default()
            },
        )
        .unwrap();

        assert!(report.changed);
        assert!(!report.applied);
        assert_eq!(report.dependencies.len(), 2);
        assert!(report
            .dependencies
            .iter()
            .any(
                |dependency| dependency.report.manifest_path == shared.join("num.toml")
                    && dependency.report.language.from == "0.0.1"
            ));
        assert!(report
            .dependencies
            .iter()
            .any(
                |dependency| dependency.report.manifest_path == core.join("num.toml")
                    && dependency.report.changed
            ));

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(shared).unwrap();
        fs::remove_dir_all(core).unwrap();
    }

    #[test]
    fn graph_write_keeps_dependencies_pending_without_write_dependencies() {
        let root = temp_dir("graph_write_root");
        let shared = sibling_dir(&root, "shared");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&shared).unwrap();

        write_manifest(
            &root,
            &format!(
                r#"[language]
version = "0.0.1"
compatibility = "minor"
manifest_schema = 1

[project]
name = "app"
version = "0.1.0"

[dependencies]
shared = {{ path = "{}", version = "0.3.0" }}
"#,
                toml_path(&shared)
            ),
        );
        write_manifest(
            &shared,
            r#"[language]
version = "0.0.1"
compatibility = "minor"
manifest_schema = 1

[project]
name = "shared"
version = "0.3.0"
"#,
        );

        let report = upgrade_manifest_graph_versions(
            &root,
            &VersionUpgradeOptions {
                write: true,
                include_dependencies: true,
                ..VersionUpgradeOptions::default()
            },
        )
        .unwrap();
        let root_source = fs::read_to_string(root.join("num.toml")).unwrap();
        let shared_source = fs::read_to_string(shared.join("num.toml")).unwrap();

        assert!(report.root.applied);
        assert!(!report.applied);
        assert!(report.dependencies[0].report.changed);
        assert!(!report.dependencies[0].report.applied);
        assert!(root_source.contains(&format!("version = \"{CURRENT_LANGUAGE_VERSION}\"")));
        assert!(shared_source.contains("version = \"0.0.1\""));

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(shared).unwrap();
    }

    #[test]
    fn graph_write_applies_dependency_manifests_when_requested() {
        let root = temp_dir("graph_write_deps_root");
        let shared = sibling_dir(&root, "shared");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&shared).unwrap();

        write_manifest(
            &root,
            &format!(
                r#"[project]
name = "app"
version = "0.1.0"

[dependencies]
shared = {{ path = "{}", version = "0.3.0" }}
"#,
                toml_path(&shared)
            ),
        );
        write_manifest(
            &shared,
            r#"[language]
version = "0.0.1"
compatibility = "minor"
manifest_schema = 1

[project]
name = "shared"
version = "0.3.0"
"#,
        );

        let report = upgrade_manifest_graph_versions(
            &root,
            &VersionUpgradeOptions {
                write: true,
                include_dependencies: true,
                write_dependencies: true,
                target_project_version: Some("0.4.0".to_string()),
                ..VersionUpgradeOptions::default()
            },
        )
        .unwrap();
        let root_source = fs::read_to_string(root.join("num.toml")).unwrap();
        let shared_source = fs::read_to_string(shared.join("num.toml")).unwrap();

        assert!(report.root.applied);
        assert!(report.applied);
        assert!(report.dependencies[0].report.applied);
        assert_eq!(report.root.project.unwrap().to, "0.4.0");
        assert!(report.dependencies[0].report.project.is_none());
        assert!(root_source.contains("version = \"0.4.0\""));
        assert!(shared_source.contains(&format!("version = \"{CURRENT_LANGUAGE_VERSION}\"")));
        assert!(!shared_source.contains("version = \"0.4.0\""));

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(shared).unwrap();
    }

    #[test]
    fn graph_write_dependencies_requires_write() {
        let root = temp_dir("graph_write_flag_root");
        fs::create_dir_all(&root).unwrap();
        write_manifest(
            &root,
            r#"[project]
name = "app"
version = "0.1.0"
"#,
        );

        let err = upgrade_manifest_graph_versions(
            &root,
            &VersionUpgradeOptions {
                include_dependencies: true,
                write_dependencies: true,
                ..VersionUpgradeOptions::default()
            },
        )
        .unwrap_err();

        assert_eq!(err, "--write-dependencies requires --write");
        fs::remove_dir_all(root).unwrap();
    }

    fn temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "num_version_upgrade_{name}_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn sibling_dir(root: &Path, suffix: &str) -> PathBuf {
        root.with_file_name(format!(
            "{}_{suffix}",
            root.file_name().unwrap().to_string_lossy()
        ))
    }

    fn write_manifest(root: &Path, source: &str) {
        fs::write(root.join("num.toml"), source).unwrap();
    }

    fn toml_path(path: &Path) -> String {
        path.display()
            .to_string()
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
    }
}
