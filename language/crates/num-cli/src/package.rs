use crate::compatibility;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageManifest {
    pub root: PathBuf,
    pub path: PathBuf,
    pub language: PackageLanguage,
    pub project: ProjectPackage,
    pub registry: PackageRegistry,
    pub runtime: PackageRuntime,
    pub deployment: PackageDeployment,
    pub security: PackageSecurity,
    pub connectors: Vec<PackageConnectorProcess>,
    pub dependencies: Vec<PackageDependency>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageLanguage {
    pub version: String,
    pub compatibility: String,
    pub manifest_schema: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectPackage {
    pub name: String,
    pub version: String,
    pub source: String,
    pub entry: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageSecurity {
    pub policy_mode: String,
    pub tenant_isolation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PackageRegistry {
    pub path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageRuntime {
    pub workflow_store: String,
    pub audit_store: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageDeployment {
    pub target: String,
    pub service: Option<String>,
    pub region: Option<String>,
    pub artifact: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageDependency {
    pub name: String,
    pub version: String,
    pub source: DependencySource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageConnectorProcess {
    pub method: String,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencySource {
    Registry,
    Path(String),
    Git(String),
}

impl DependencySource {
    fn lock_source(&self) -> String {
        match self {
            DependencySource::Registry => "registry".to_string(),
            DependencySource::Path(path) => format!("path:{path}"),
            DependencySource::Git(url) => format!("git:{url}"),
        }
    }
}

impl PackageManifest {
    pub fn discover(path: &Path) -> Result<Option<Self>, String> {
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
                return Ok(Some(Self::parse(&dir, &manifest_path, &source)));
            }
            cursor = dir.parent().map(Path::to_path_buf);
        }

        Ok(None)
    }

    pub fn parse(root: &Path, path: &Path, source: &str) -> Self {
        let mut section = "";
        let mut language_version = None;
        let mut language_compatibility = None;
        let mut manifest_schema = None;
        let mut project_name = None;
        let mut project_version = None;
        let mut project_source = None;
        let mut project_entry = None;
        let mut registry_path = None;
        let mut workflow_store = None;
        let mut audit_store = None;
        let mut deployment_target = None;
        let mut deployment_service = None;
        let mut deployment_region = None;
        let mut deployment_artifact = None;
        let mut policy_mode = None;
        let mut tenant_isolation = None;
        let mut connectors = Vec::new();
        let mut dependencies = Vec::new();

        for raw_line in source.lines() {
            let line = raw_line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                section = line.trim_start_matches('[').trim_end_matches(']').trim();
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = normalize_toml_key(key.trim());
            let value = value.trim();

            match section {
                "language" => match key.as_str() {
                    "version" => language_version = parse_toml_string(value),
                    "compatibility" => language_compatibility = parse_toml_string(value),
                    "manifest_schema" => manifest_schema = parse_toml_u32(value),
                    _ => {}
                },
                "project" => match key.as_str() {
                    "name" => project_name = parse_toml_string(value),
                    "version" => project_version = parse_toml_string(value),
                    "source" | "source_dir" => project_source = parse_toml_string(value),
                    "entry" => project_entry = parse_toml_string(value),
                    _ => {}
                },
                "dependencies" => {
                    if let Some(dependency) = parse_dependency(&key, value) {
                        dependencies.push(dependency);
                    }
                }
                "registry" => match key.as_str() {
                    "path" => registry_path = parse_toml_string(value),
                    _ => {}
                },
                "runtime" => match key.as_str() {
                    "workflow_store" => workflow_store = parse_toml_string(value),
                    "audit_store" => audit_store = parse_toml_string(value),
                    _ => {}
                },
                "deployment" => match key.as_str() {
                    "target" => deployment_target = parse_toml_string(value),
                    "service" => deployment_service = parse_toml_string(value),
                    "region" => deployment_region = parse_toml_string(value),
                    "artifact" => deployment_artifact = parse_toml_string(value),
                    _ => {}
                },
                "connectors" => {
                    if let Some(connector) = parse_connector_process(&key, value) {
                        connectors.push(connector);
                    }
                }
                "security" => match key.as_str() {
                    "policy_mode" => policy_mode = parse_toml_string(value),
                    "tenant_isolation" => tenant_isolation = parse_toml_bool(value),
                    _ => {}
                },
                _ => {}
            }
        }

        connectors.sort_by(|left, right| left.method.cmp(&right.method));
        dependencies.sort_by(|left, right| left.name.cmp(&right.name));

        Self {
            root: root.to_path_buf(),
            path: path.to_path_buf(),
            language: PackageLanguage {
                version: language_version.unwrap_or_else(|| "0.1.0".to_string()),
                compatibility: language_compatibility.unwrap_or_else(|| "minor".to_string()),
                manifest_schema: manifest_schema.unwrap_or(1),
            },
            project: ProjectPackage {
                name: project_name.unwrap_or_else(|| {
                    root.file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("num-package")
                        .to_string()
                }),
                version: project_version.unwrap_or_else(|| "0.1.0".to_string()),
                source: project_source.unwrap_or_else(|| "src".to_string()),
                entry: project_entry.unwrap_or_else(|| "src/main.num".to_string()),
            },
            registry: PackageRegistry {
                path: registry_path,
            },
            runtime: PackageRuntime {
                workflow_store: workflow_store.unwrap_or_else(|| "memory".to_string()),
                audit_store: audit_store.unwrap_or_else(|| "stdout".to_string()),
            },
            deployment: PackageDeployment {
                target: deployment_target.unwrap_or_else(|| "local".to_string()),
                service: deployment_service,
                region: deployment_region,
                artifact: deployment_artifact.unwrap_or_else(|| "num-deploy.json".to_string()),
            },
            security: PackageSecurity {
                policy_mode: policy_mode.unwrap_or_else(|| "strict".to_string()),
                tenant_isolation: tenant_isolation.unwrap_or(false),
            },
            connectors,
            dependencies,
        }
    }

    pub fn source_dir(&self) -> PathBuf {
        self.root.join(&self.project.source)
    }

    pub fn entry_path(&self) -> PathBuf {
        self.root.join(&self.project.entry)
    }

    pub fn lock_path(&self) -> PathBuf {
        self.root.join("num.lock")
    }

    pub fn path_dependency_manifests(&self) -> Result<Vec<PackageManifest>, String> {
        let mut manifests = Vec::new();
        for dependency in &self.dependencies {
            let DependencySource::Path(path) = &dependency.source else {
                continue;
            };
            let dependency_root = self.root.join(path);
            let manifest_path = dependency_root.join("num.toml");
            if !manifest_path.is_file() {
                return Err(format!(
                    "path dependency `{}` has no num.toml at {}",
                    dependency.name,
                    manifest_path.display()
                ));
            }
            let source = fs::read_to_string(&manifest_path)
                .map_err(|err| format!("failed to read {}: {err}", manifest_path.display()))?;
            manifests.push(PackageManifest::parse(
                &dependency_root,
                &manifest_path,
                &source,
            ));
        }
        Ok(manifests)
    }
}

pub fn write_lockfile(path: &Path) -> Result<PathBuf, String> {
    let manifest = PackageManifest::discover(path)?
        .ok_or_else(|| format!("no num.toml found for {}", path.display()))?;
    compatibility::validate_manifest(&manifest)?;
    let lock_path = manifest.lock_path();
    fs::write(&lock_path, render_lockfile(&manifest))
        .map_err(|err| format!("failed to write {}: {err}", lock_path.display()))?;
    Ok(lock_path)
}

pub fn render_lockfile(manifest: &PackageManifest) -> String {
    let mut out = String::new();
    out.push_str("# This file is generated by `num lock`. Do not edit by hand.\n\n");
    out.push_str("version = 1\n\n");
    out.push_str("[[package]]\n");
    push_lock_field(&mut out, "name", &manifest.project.name);
    push_lock_field(&mut out, "version", &manifest.project.version);
    push_lock_field(&mut out, "source", "workspace");
    push_lock_field(&mut out, "language", &manifest.language.version);
    push_lock_field(&mut out, "compatibility", &manifest.language.compatibility);
    push_lock_u32_field(
        &mut out,
        "manifest_schema",
        manifest.language.manifest_schema,
    );
    out.push('\n');

    for dependency in &manifest.dependencies {
        out.push_str("[[package]]\n");
        push_lock_field(&mut out, "name", &dependency.name);
        push_lock_field(&mut out, "version", &dependency.version);
        push_lock_field(&mut out, "source", &dependency.source.lock_source());
        out.push('\n');
    }

    out
}

fn parse_connector_process(method: &str, value: &str) -> Option<PackageConnectorProcess> {
    if let Some(command_line) = parse_toml_string(value) {
        let command_parts = split_command_line(&command_line).ok()?;
        return connector_from_parts(method, command_parts, None);
    }

    let fields = parse_inline_table(value)?;
    let command = fields.get("command")?.clone();
    let mut parts = vec![command];
    if let Some(args) = fields.get("args") {
        parts.extend(split_command_line(args).ok()?);
    }
    connector_from_parts(method, parts, fields.get("cwd").cloned())
}

fn connector_from_parts(
    method: &str,
    mut parts: Vec<String>,
    cwd: Option<String>,
) -> Option<PackageConnectorProcess> {
    if method.trim().is_empty() || parts.is_empty() {
        return None;
    }
    let command = parts.remove(0);
    if command.trim().is_empty() {
        return None;
    }
    Some(PackageConnectorProcess {
        method: method.to_string(),
        command,
        args: parts,
        cwd,
    })
}

fn parse_dependency(name: &str, value: &str) -> Option<PackageDependency> {
    if let Some(version) = parse_toml_string(value) {
        return Some(PackageDependency {
            name: name.to_string(),
            version,
            source: DependencySource::Registry,
        });
    }

    let fields = parse_inline_table(value)?;
    let version = fields
        .get("version")
        .cloned()
        .unwrap_or_else(|| "0.0.0".to_string());
    let source = if let Some(path) = fields.get("path") {
        DependencySource::Path(path.clone())
    } else if let Some(git) = fields.get("git") {
        DependencySource::Git(git.clone())
    } else {
        DependencySource::Registry
    };

    Some(PackageDependency {
        name: name.to_string(),
        version,
        source,
    })
}

fn split_command_line(input: &str) -> Result<Vec<String>, String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut escaped = false;

    for ch in input.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(active_quote) = quote {
            if ch == active_quote {
                quote = None;
            } else {
                current.push(ch);
            }
            continue;
        }
        match ch {
            '\'' | '"' => quote = Some(ch),
            ch if ch.is_whitespace() => {
                if !current.is_empty() {
                    args.push(current);
                    current = String::new();
                }
            }
            _ => current.push(ch),
        }
    }

    if escaped {
        current.push('\\');
    }
    if quote.is_some() {
        return Err("unterminated quote in command line".to_string());
    }
    if !current.is_empty() {
        args.push(current);
    }
    Ok(args)
}

fn normalize_toml_key(key: &str) -> String {
    key.trim().trim_matches('"').trim_matches('\'').to_string()
}

fn parse_inline_table(value: &str) -> Option<BTreeMap<String, String>> {
    let value = value.trim();
    if !value.starts_with('{') || !value.ends_with('}') {
        return None;
    }
    let inner = &value[1..value.len() - 1];
    let mut fields = BTreeMap::new();
    for part in split_inline_table_fields(inner) {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        let Some(value) = parse_toml_string(value.trim()) else {
            continue;
        };
        fields.insert(normalize_toml_key(key.trim()), value);
    }
    Some(fields)
}

fn split_inline_table_fields(inner: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut escaped = false;

    for ch in inner.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' && in_string {
            current.push(ch);
            escaped = true;
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
            current.push(ch);
            continue;
        }
        if ch == ',' && !in_string {
            let field = current.trim();
            if !field.is_empty() {
                fields.push(field.to_string());
            }
            current.clear();
            continue;
        }
        current.push(ch);
    }

    let field = current.trim();
    if !field.is_empty() {
        fields.push(field.to_string());
    }

    fields
}

fn parse_toml_string(value: &str) -> Option<String> {
    let value = value.trim();
    if value.len() < 2 || !value.starts_with('"') || !value.ends_with('"') {
        return None;
    }

    Some(value[1..value.len() - 1].to_string())
}

fn parse_toml_bool(value: &str) -> Option<bool> {
    match value.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn parse_toml_u32(value: &str) -> Option<u32> {
    value.trim().parse().ok()
}

fn push_lock_field(out: &mut String, key: &str, value: &str) {
    out.push_str(key);
    out.push_str(" = \"");
    out.push_str(&escape_lock_string(value));
    out.push_str("\"\n");
}

fn push_lock_u32_field(out: &mut String, key: &str, value: u32) {
    out.push_str(key);
    out.push_str(" = ");
    out.push_str(&value.to_string());
    out.push('\n');
}

fn escape_lock_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_package_dir(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = env::temp_dir().join(format!("num_package_{name}_{stamp}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn manifest_defaults_to_src_main() {
        let root = Path::new("/workspace/app");
        let manifest = PackageManifest::parse(
            root,
            &root.join("num.toml"),
            r#"
[project]
name = "app"
version = "0.1.0"
"#,
        );

        assert_eq!(manifest.source_dir(), root.join("src"));
        assert_eq!(manifest.entry_path(), root.join("src/main.num"));
        assert_eq!(manifest.language.version, "0.1.0");
        assert_eq!(manifest.language.compatibility, "minor");
        assert_eq!(manifest.language.manifest_schema, 1);
    }

    #[test]
    fn manifest_reads_language_metadata() {
        let root = Path::new("/workspace/app");
        let manifest = PackageManifest::parse(
            root,
            &root.join("num.toml"),
            r#"
[language]
version = "0.1.0"
compatibility = "exact"
manifest_schema = 1

[project]
name = "app"
version = "0.1.0"
"#,
        );

        assert_eq!(manifest.language.version, "0.1.0");
        assert_eq!(manifest.language.compatibility, "exact");
        assert_eq!(manifest.language.manifest_schema, 1);
    }

    #[test]
    fn manifest_reads_dependencies() {
        let root = Path::new("/workspace/app");
        let manifest = PackageManifest::parse(
            root,
            &root.join("num.toml"),
            r#"
[project]
name = "app"
version = "0.1.0"
source = "source"
entry = "source/app.num"

[dependencies]
std = "0.1.0"
shared = { path = "../shared", version = "0.2.0" }
banking = { git = "https://example.com/banking.num.git", version = "1.4.0" }
"#,
        );

        assert_eq!(manifest.source_dir(), root.join("source"));
        assert_eq!(manifest.entry_path(), root.join("source/app.num"));
        assert_eq!(manifest.dependencies.len(), 3);
        assert_eq!(manifest.dependencies[0].name, "banking");
        assert_eq!(
            manifest.dependencies[0].source,
            DependencySource::Git("https://example.com/banking.num.git".to_string())
        );
        assert_eq!(manifest.dependencies[1].name, "shared");
        assert_eq!(
            manifest.dependencies[1].source,
            DependencySource::Path("../shared".to_string())
        );
        assert_eq!(manifest.dependencies[2].name, "std");
        assert_eq!(manifest.dependencies[2].source, DependencySource::Registry);
    }

    #[test]
    fn manifest_reads_process_connectors() {
        let root = Path::new("/workspace/app");
        let manifest = PackageManifest::parse(
            root,
            &root.join("num.toml"),
            r#"
[project]
name = "app"
version = "0.1.0"

[connectors]
"payments.find" = { command = "node", args = "connectors/payments-find.js --mode real", cwd = "ops" }
"mailer.send" = "node connectors/mailer-send.js"
"#,
        );

        assert_eq!(manifest.connectors.len(), 2);
        assert_eq!(manifest.connectors[0].method, "mailer.send");
        assert_eq!(manifest.connectors[0].command, "node");
        assert_eq!(
            manifest.connectors[0].args,
            vec!["connectors/mailer-send.js".to_string()]
        );
        assert_eq!(manifest.connectors[1].method, "payments.find");
        assert_eq!(
            manifest.connectors[1].args,
            vec![
                "connectors/payments-find.js".to_string(),
                "--mode".to_string(),
                "real".to_string()
            ]
        );
        assert_eq!(manifest.connectors[1].cwd, Some("ops".to_string()));
    }

    #[test]
    fn manifest_reads_security_metadata() {
        let root = Path::new("/workspace/app");
        let manifest = PackageManifest::parse(
            root,
            &root.join("num.toml"),
            r#"
[project]
name = "app"
version = "0.1.0"

[security]
policy_mode = "strict"
tenant_isolation = true
"#,
        );

        assert_eq!(manifest.security.policy_mode, "strict");
        assert!(manifest.security.tenant_isolation);
    }

    #[test]
    fn manifest_reads_registry_metadata() {
        let root = Path::new("/workspace/app");
        let manifest = PackageManifest::parse(
            root,
            &root.join("num.toml"),
            r#"
[project]
name = "app"
version = "0.1.0"

[registry]
path = "../num-registry"
"#,
        );

        assert_eq!(manifest.registry.path, Some("../num-registry".to_string()));
    }

    #[test]
    fn manifest_reads_runtime_and_deployment_metadata() {
        let root = Path::new("/workspace/app");
        let manifest = PackageManifest::parse(
            root,
            &root.join("num.toml"),
            r#"
[project]
name = "app"
version = "0.1.0"

[runtime]
workflow_store = "file:.num-state"
audit_store = "file:audit/events.jsonl"

[deployment]
target = "container"
service = "BillingApi"
region = "eu-west-1"
artifact = "dist/deploy.json"
"#,
        );

        assert_eq!(manifest.runtime.workflow_store, "file:.num-state");
        assert_eq!(manifest.runtime.audit_store, "file:audit/events.jsonl");
        assert_eq!(manifest.deployment.target, "container");
        assert_eq!(manifest.deployment.service, Some("BillingApi".to_string()));
        assert_eq!(manifest.deployment.region, Some("eu-west-1".to_string()));
        assert_eq!(manifest.deployment.artifact, "dist/deploy.json");
    }

    #[test]
    fn lockfile_is_deterministic() {
        let root = Path::new("/workspace/app");
        let manifest = PackageManifest::parse(
            root,
            &root.join("num.toml"),
            r#"
[project]
name = "app"
version = "0.1.0"

[dependencies]
zeta = "2.0.0"
alpha = { path = "../alpha", version = "1.0.0" }
"#,
        );

        let lockfile = render_lockfile(&manifest);

        assert!(lockfile.contains("name = \"app\""));
        assert!(lockfile.contains("language = \"0.1.0\""));
        assert!(lockfile.contains("manifest_schema = 1"));
        assert!(lockfile.find("name = \"alpha\"") < lockfile.find("name = \"zeta\""));
        assert!(lockfile.contains("source = \"path:../alpha\""));
    }

    #[test]
    fn writes_lockfile_next_to_manifest() {
        let root = temp_package_dir("write_lock");
        fs::write(
            root.join("num.toml"),
            r#"
[project]
name = "app"
version = "0.1.0"

[dependencies]
std = "0.1.0"
"#,
        )
        .unwrap();

        let path = write_lockfile(&root).unwrap();
        let contents = fs::read_to_string(&path).unwrap();

        assert_eq!(path, root.join("num.lock"));
        assert!(contents.contains("name = \"std\""));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn discovers_path_dependency_manifests() {
        let root = temp_package_dir("path_dependency");
        let shared = root.with_file_name(format!(
            "{}_shared",
            root.file_name().unwrap().to_string_lossy()
        ));
        fs::create_dir_all(&shared).unwrap();
        fs::write(
            root.join("num.toml"),
            format!(
                r#"
[project]
name = "app"
version = "0.1.0"

[dependencies]
shared = {{ path = "{}", version = "0.2.0" }}
"#,
                path_to_toml_string(shared.display().to_string())
            ),
        )
        .unwrap();
        fs::write(
            shared.join("num.toml"),
            r#"
[project]
name = "shared"
version = "0.2.0"
source = "src"
entry = "src/lib.num"
"#,
        )
        .unwrap();

        let manifest = PackageManifest::discover(&root).unwrap().unwrap();
        let dependencies = manifest.path_dependency_manifests().unwrap();

        assert_eq!(dependencies.len(), 1);
        assert_eq!(dependencies[0].project.name, "shared");
        assert_eq!(dependencies[0].entry_path(), shared.join("src/lib.num"));
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(shared).unwrap();
    }

    fn path_to_toml_string(path: String) -> String {
        path.replace('\\', "\\\\").replace('"', "\\\"")
    }
}
