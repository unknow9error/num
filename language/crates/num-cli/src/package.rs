use crate::compatibility;
use crate::registry::LocalRegistry;
use num_runtime::sanitization::{SanitizerPack, TextCharClass, TextSanitizationPolicy};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const CURRENT_LOCKFILE_SCHEMA: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageManifest {
    pub root: PathBuf,
    pub path: PathBuf,
    pub language: PackageLanguage,
    pub project: ProjectPackage,
    pub registry: PackageRegistry,
    pub runtime: PackageRuntime,
    pub secrets: Vec<PackageSecretBackend>,
    pub ai: PackageAiConfig,
    pub environment: PackageEnvironment,
    pub deployment: PackageDeployment,
    pub security: PackageSecurity,
    pub connectors: Vec<PackageConnectorProcess>,
    pub javascript: Vec<PackageJavaScriptModule>,
    pub sanitizer_packs: Vec<PackageSanitizerPack>,
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
    pub jwt: Option<PackageJwtSecurity>,
    pub session: Option<PackageSessionSecurity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageJwtSecurity {
    pub issuer: String,
    pub audience: String,
    pub algorithms: Vec<String>,
    pub secret_env: String,
    pub leeway_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageSessionSecurity {
    pub cookie_name: String,
    pub secret_env: String,
    pub leeway_seconds: i64,
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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PackageSecretBackend {
    pub id: String,
    pub provider: String,
    pub address: Option<String>,
    pub mount: Option<String>,
    pub path_prefix: Option<String>,
    pub auth_method: String,
    pub token_env: Option<String>,
    pub credential_env: Vec<String>,
    pub optional: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PackageAiConfig {
    pub default_model: Option<String>,
    pub models: Vec<PackageAiModel>,
    pub scanners: Vec<PackageAiScanner>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PackageAiModel {
    pub alias: String,
    pub provider: String,
    pub model: String,
    pub credential_env: Vec<String>,
    pub timeout_ms: Option<u64>,
    pub max_cost: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PackageAiScanner {
    pub alias: String,
    pub provider: String,
    pub mode: String,
    pub block_threshold: Option<String>,
    pub audit_redaction: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PackageEnvironment {
    pub required: Vec<String>,
    pub optional: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageDeployment {
    pub target: String,
    pub service: Option<String>,
    pub region: Option<String>,
    pub artifact: String,
    pub registry: Option<String>,
    pub image: Option<String>,
    pub tag_strategy: Option<String>,
    pub credentials_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageDependency {
    pub name: String,
    pub version: String,
    pub source: DependencySource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageGitDependency {
    pub url: String,
    pub rev: Option<String>,
    pub tag: Option<String>,
    pub branch: Option<String>,
    pub reference: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageConnectorProcess {
    pub method: String,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageJavaScriptModule {
    pub method: String,
    pub module: String,
    pub export: String,
    pub cwd: Option<String>,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PackageSanitizerPack {
    pub name: String,
    pub extends: Vec<String>,
    pub trim: Option<bool>,
    pub strip_control_chars: Option<bool>,
    pub max_chars: Option<usize>,
    pub lowercase: Option<bool>,
    pub collapse_whitespace: Option<bool>,
    pub allowed_chars: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LockfileMigrationReport {
    pub lockfile: PathBuf,
    pub schema: Option<u32>,
    pub target_schema: u32,
    pub actions: Vec<String>,
    pub changed: bool,
    pub applied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencySource {
    Registry,
    Path(String),
    Git(PackageGitDependency),
}

impl DependencySource {
    pub(crate) fn lock_source(&self) -> String {
        match self {
            DependencySource::Registry => "registry".to_string(),
            DependencySource::Path(path) => format!("path:{path}"),
            DependencySource::Git(git) => git.lock_source(),
        }
    }
}

impl PackageGitDependency {
    fn lock_source(&self) -> String {
        let mut source = format!("git:{}", self.url);
        if let Some(rev) = &self.rev {
            source.push_str(&format!("#rev:{rev}"));
        } else if let Some(tag) = &self.tag {
            source.push_str(&format!("#tag:{tag}"));
        } else if let Some(branch) = &self.branch {
            source.push_str(&format!("#branch:{branch}"));
        } else if let Some(reference) = &self.reference {
            source.push_str(&format!("#ref:{reference}"));
        }
        source
    }

    fn lock_source_with_rev(&self, rev: &str) -> String {
        format!("git:{}#rev:{rev}", self.url)
    }

    fn checkout_selector(&self) -> Option<&str> {
        self.rev
            .as_deref()
            .or(self.tag.as_deref())
            .or(self.branch.as_deref())
            .or(self.reference.as_deref())
    }

    fn pinned_rev(&self) -> Option<&str> {
        self.rev.as_deref()
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
        let mut secret_backends = Vec::new();
        let mut ai_default_model = None;
        let mut ai_models = Vec::new();
        let mut ai_scanners = Vec::new();
        let mut environment_required = Vec::new();
        let mut environment_optional = Vec::new();
        let mut deployment_target = None;
        let mut deployment_service = None;
        let mut deployment_region = None;
        let mut deployment_artifact = None;
        let mut deployment_registry = None;
        let mut deployment_image = None;
        let mut deployment_tag_strategy = None;
        let mut deployment_credentials_ref = None;
        let mut policy_mode = None;
        let mut tenant_isolation = None;
        let mut jwt_issuer = None;
        let mut jwt_audience = None;
        let mut jwt_algorithms = Vec::new();
        let mut jwt_secret_env = None;
        let mut jwt_leeway_seconds = None;
        let mut session_cookie_name = None;
        let mut session_secret_env = None;
        let mut session_leeway_seconds = None;
        let mut connectors = Vec::new();
        let mut javascript = Vec::new();
        let mut sanitizer_packs = Vec::new();
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
                section if section.starts_with("secrets.") => {
                    let id = section.strip_prefix("secrets.").unwrap_or_default().trim();
                    upsert_secret_backend(&mut secret_backends, id, &key, value);
                }
                "ai" => match key.as_str() {
                    "default_model" | "default" => ai_default_model = parse_toml_string(value),
                    _ => {}
                },
                section if section.starts_with("ai.models.") => {
                    let alias = section
                        .strip_prefix("ai.models.")
                        .unwrap_or_default()
                        .trim();
                    upsert_ai_model(&mut ai_models, alias, &key, value);
                }
                section if section.starts_with("ai.scanners.") => {
                    let alias = section
                        .strip_prefix("ai.scanners.")
                        .unwrap_or_default()
                        .trim();
                    upsert_ai_scanner(&mut ai_scanners, alias, &key, value);
                }
                "environment" => match key.as_str() {
                    "required" => environment_required.extend(parse_toml_string_array(value)),
                    "optional" => environment_optional.extend(parse_toml_string_array(value)),
                    _ => {}
                },
                "deployment" => match key.as_str() {
                    "target" => deployment_target = parse_toml_string(value),
                    "service" => deployment_service = parse_toml_string(value),
                    "region" => deployment_region = parse_toml_string(value),
                    "artifact" => deployment_artifact = parse_toml_string(value),
                    "registry" | "registry_url" => deployment_registry = parse_toml_string(value),
                    "image" | "image_name" => deployment_image = parse_toml_string(value),
                    "tag_strategy" | "image_tag_strategy" => {
                        deployment_tag_strategy = parse_toml_string(value)
                    }
                    "credentials_ref" | "registry_credentials_ref" => {
                        deployment_credentials_ref = parse_toml_string(value)
                    }
                    _ => {}
                },
                "connectors" => {
                    if let Some(connector) = parse_connector_process(&key, value) {
                        connectors.push(connector);
                    }
                }
                "javascript" => {
                    if let Some(module) = parse_javascript_module(&key, value) {
                        javascript.push(module);
                    }
                }
                section if section.starts_with("sanitizer_packs.") => {
                    let name = section
                        .strip_prefix("sanitizer_packs.")
                        .unwrap_or_default()
                        .trim();
                    upsert_sanitizer_pack(&mut sanitizer_packs, name, &key, value);
                }
                "security" => match key.as_str() {
                    "policy_mode" => policy_mode = parse_toml_string(value),
                    "tenant_isolation" => tenant_isolation = parse_toml_bool(value),
                    _ => {}
                },
                "security.jwt" => match key.as_str() {
                    "issuer" => jwt_issuer = parse_toml_string(value),
                    "audience" => jwt_audience = parse_toml_string(value),
                    "algorithms" | "allowed_algorithms" => {
                        jwt_algorithms.extend(parse_toml_string_array(value))
                    }
                    "secret_env" => jwt_secret_env = parse_toml_string(value),
                    "leeway_seconds" => jwt_leeway_seconds = parse_toml_i64(value),
                    _ => {}
                },
                "security.session" => match key.as_str() {
                    "cookie_name" => session_cookie_name = parse_toml_string(value),
                    "secret_env" => session_secret_env = parse_toml_string(value),
                    "leeway_seconds" => session_leeway_seconds = parse_toml_i64(value),
                    _ => {}
                },
                _ => {}
            }
        }

        connectors.sort_by(|left, right| left.method.cmp(&right.method));
        javascript.sort_by(|left, right| left.method.cmp(&right.method));
        secret_backends.sort_by(|left, right| left.id.cmp(&right.id));
        ai_models.sort_by(|left, right| left.alias.cmp(&right.alias));
        ai_scanners.sort_by(|left, right| left.alias.cmp(&right.alias));
        sanitizer_packs.sort_by(|left, right| left.name.cmp(&right.name));
        dependencies.sort_by(|left, right| left.name.cmp(&right.name));

        Self {
            root: root.to_path_buf(),
            path: path.to_path_buf(),
            language: PackageLanguage {
                version: language_version
                    .unwrap_or_else(|| compatibility::CURRENT_LANGUAGE_VERSION.to_string()),
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
            secrets: secret_backends,
            ai: PackageAiConfig {
                default_model: ai_default_model,
                models: ai_models,
                scanners: ai_scanners,
            },
            environment: PackageEnvironment {
                required: normalize_env_names(environment_required),
                optional: normalize_env_names(environment_optional),
            },
            deployment: PackageDeployment {
                target: deployment_target.unwrap_or_else(|| "local".to_string()),
                service: deployment_service,
                region: deployment_region,
                artifact: deployment_artifact.unwrap_or_else(|| "num-deploy.json".to_string()),
                registry: deployment_registry,
                image: deployment_image,
                tag_strategy: deployment_tag_strategy,
                credentials_ref: deployment_credentials_ref,
            },
            security: PackageSecurity {
                policy_mode: policy_mode.unwrap_or_else(|| "strict".to_string()),
                tenant_isolation: tenant_isolation.unwrap_or(false),
                jwt: match (jwt_issuer, jwt_audience, jwt_secret_env) {
                    (Some(issuer), Some(audience), Some(secret_env)) => Some(PackageJwtSecurity {
                        issuer,
                        audience,
                        algorithms: normalize_env_names(if jwt_algorithms.is_empty() {
                            vec!["HS256".to_string()]
                        } else {
                            jwt_algorithms
                        }),
                        secret_env,
                        leeway_seconds: jwt_leeway_seconds.unwrap_or(0).max(0),
                    }),
                    _ => None,
                },
                session: session_secret_env.map(|secret_env| PackageSessionSecurity {
                    cookie_name: session_cookie_name.unwrap_or_else(|| "num_session".to_string()),
                    secret_env,
                    leeway_seconds: session_leeway_seconds.unwrap_or(0).max(0),
                }),
            },
            connectors,
            javascript,
            sanitizer_packs,
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

    pub fn sanitizer_pack_policies(&self) -> Result<Vec<(String, TextSanitizationPolicy)>, String> {
        let mut out = Vec::new();
        let mut resolving = BTreeSet::new();
        for pack in &self.sanitizer_packs {
            let policy = self.resolve_sanitizer_pack_policy(&pack.name, &mut resolving)?;
            out.push((pack.name.clone(), policy));
        }
        Ok(out)
    }

    fn resolve_sanitizer_pack_policy(
        &self,
        name: &str,
        resolving: &mut BTreeSet<String>,
    ) -> Result<TextSanitizationPolicy, String> {
        if !resolving.insert(name.to_string()) {
            return Err(format!(
                "sanitizer pack `{name}` extends itself recursively"
            ));
        }
        let Some(pack) = self.sanitizer_packs.iter().find(|pack| pack.name == name) else {
            resolving.remove(name);
            return SanitizerPack::named(name)
                .map(|pack| pack.policy())
                .map_err(|_| format!("unknown sanitizer pack `{name}`"));
        };
        let mut policy = TextSanitizationPolicy::default();
        for parent in &pack.extends {
            let parent_policy = self.resolve_sanitizer_pack_policy(parent, resolving)?;
            policy = policy.compose(&parent_policy);
        }
        policy = policy.compose(&pack_policy(pack)?);
        resolving.remove(name);
        Ok(policy)
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

    pub(crate) fn resolved_dependency_manifests(&self) -> Result<Vec<PackageManifest>, String> {
        let mut manifests = Vec::new();
        let registry = LocalRegistry::discover_for(self);

        for dependency in &self.dependencies {
            let resolved = match &dependency.source {
                DependencySource::Path(_) => Some(load_path_dependency_manifest(self, dependency)?),
                DependencySource::Registry => {
                    let registry = registry.as_ref().ok_or_else(|| {
                        format!(
                            "registry dependency `{}` requires [registry].path or NUM_REGISTRY_PATH",
                            dependency.name
                        )
                    })?;
                    registry.resolve(dependency)?
                }
                DependencySource::Git(_) => Some(load_git_dependency_manifest(self, dependency)?.0),
            };

            if let Some(resolved) = resolved {
                validate_dependency_manifest_identity(dependency, &resolved)?;
                manifests.push(resolved);
            }
        }

        manifests.sort_by(|left, right| left.project.name.cmp(&right.project.name));
        Ok(manifests)
    }
}

pub fn write_lockfile(path: &Path) -> Result<PathBuf, String> {
    let manifest = PackageManifest::discover(path)?
        .ok_or_else(|| format!("no num.toml found for {}", path.display()))?;
    compatibility::validate_manifest(&manifest)?;
    let lock_path = manifest.lock_path();
    fs::write(&lock_path, render_lockfile_graph(&manifest)?)
        .map_err(|err| format!("failed to write {}: {err}", lock_path.display()))?;
    Ok(lock_path)
}

pub fn validate_project_lockfile(path: &Path) -> Result<PathBuf, String> {
    let manifest = PackageManifest::discover(path)?
        .ok_or_else(|| format!("no num.toml found for {}", path.display()))?;
    compatibility::validate_manifest(&manifest)?;
    let lock_path = manifest.lock_path();
    if !lock_path.is_file() {
        return Err(format!("no num.lock found at {}", lock_path.display()));
    }
    let source = fs::read_to_string(&lock_path)
        .map_err(|err| format!("failed to read {}: {err}", lock_path.display()))?;
    validate_lockfile_source(&lock_path, &source)?;
    validate_lockfile_registry_pins(&manifest, &lock_path, &source)?;
    Ok(lock_path)
}

pub fn migrate_lockfile(path: &Path, write: bool) -> Result<LockfileMigrationReport, String> {
    let manifest = PackageManifest::discover(path)?
        .ok_or_else(|| format!("no num.toml found for {}", path.display()))?;
    compatibility::validate_manifest(&manifest)?;
    let lock_path = manifest.lock_path();
    if !lock_path.is_file() {
        return Err(format!("no num.lock found at {}", lock_path.display()));
    }
    let source = fs::read_to_string(&lock_path)
        .map_err(|err| format!("failed to read {}: {err}", lock_path.display()))?;
    let (schema, actions, migrated_source) = plan_lockfile_source_migration(&source)?;
    let changed = !actions.is_empty();
    if changed && write {
        fs::write(&lock_path, migrated_source)
            .map_err(|err| format!("failed to write {}: {err}", lock_path.display()))?;
    }
    Ok(LockfileMigrationReport {
        lockfile: lock_path,
        schema,
        target_schema: CURRENT_LOCKFILE_SCHEMA,
        actions,
        changed,
        applied: changed && write,
    })
}

pub fn validate_lockfile(path: &Path) -> Result<(), String> {
    let source = fs::read_to_string(path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    validate_lockfile_source(path, &source)
}

impl LockfileMigrationReport {
    pub fn to_json(&self) -> Value {
        json!({
            "lockfile": self.lockfile.display().to_string(),
            "schema": self.schema,
            "target_schema": self.target_schema,
            "actions": self.actions,
            "changed": self.changed,
            "applied": self.applied,
        })
    }

    pub fn render_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "Lockfile migration plan for {}\n",
            self.lockfile.display()
        ));
        out.push_str(&format!("Target schema: {}\n", self.target_schema));
        if self.actions.is_empty() {
            out.push_str("Actions: none\n");
        } else {
            out.push_str("Actions:\n");
            for action in &self.actions {
                out.push_str(&format!("- {action}\n"));
            }
        }
        out.push_str(&format!("Changed: {}\n", self.changed));
        out.push_str(&format!("Applied: {}\n", self.applied));
        out
    }
}

fn validate_lockfile_source(path: &Path, source: &str) -> Result<(), String> {
    let schema = parse_lockfile_schema(source)
        .ok_or_else(|| format!("{} is missing lockfile `version`", path.display()))?;
    if schema == 0 {
        return Err(format!(
            "{} declares invalid lockfile version 0; expected {CURRENT_LOCKFILE_SCHEMA}",
            path.display()
        ));
    }
    if schema > CURRENT_LOCKFILE_SCHEMA {
        return Err(format!(
            "{} requires lockfile version {schema}; this num CLI supports {CURRENT_LOCKFILE_SCHEMA}",
            path.display()
        ));
    }
    Ok(())
}

fn plan_lockfile_source_migration(
    source: &str,
) -> Result<(Option<u32>, Vec<String>, String), String> {
    let schema_line = find_lockfile_schema_line(source);
    let mut lines = source.lines().map(str::to_string).collect::<Vec<_>>();
    let mut actions = Vec::new();

    match schema_line {
        Some((index, 0)) => {
            lines[index] = format!("version = {CURRENT_LOCKFILE_SCHEMA}");
            actions.push(format!(
                "upgrade lockfile schema from 0 to {CURRENT_LOCKFILE_SCHEMA}"
            ));
            Ok((Some(0), actions, render_lockfile_lines(&lines)))
        }
        Some((_, schema)) if schema > CURRENT_LOCKFILE_SCHEMA => Err(format!(
            "cannot migrate lockfile schema {schema}; this num CLI supports {CURRENT_LOCKFILE_SCHEMA}"
        )),
        Some((_, schema)) => Ok((Some(schema), actions, render_lockfile_lines(&lines))),
        None => {
            insert_lockfile_schema_header(&mut lines);
            actions.push(format!("add lockfile schema version {CURRENT_LOCKFILE_SCHEMA}"));
            Ok((None, actions, render_lockfile_lines(&lines)))
        }
    }
}

fn find_lockfile_schema_line(source: &str) -> Option<(usize, u32)> {
    for (index, raw_line) in source.lines().enumerate() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("[[package]]") {
            return None;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if normalize_toml_key(key.trim()) == "version" {
            return parse_toml_u32(value).map(|schema| (index, schema));
        }
    }
    None
}

fn insert_lockfile_schema_header(lines: &mut Vec<String>) {
    let insert_at = lines
        .iter()
        .position(|line| line.trim_start().starts_with("[[package]]"))
        .unwrap_or(lines.len());
    lines.insert(insert_at, String::new());
    lines.insert(insert_at, format!("version = {CURRENT_LOCKFILE_SCHEMA}"));
}

fn render_lockfile_lines(lines: &[String]) -> String {
    let mut source = lines.join("\n");
    if !source.ends_with('\n') {
        source.push('\n');
    }
    source
}

fn parse_lockfile_schema(source: &str) -> Option<u32> {
    find_lockfile_schema_line(source).map(|(_, schema)| schema)
}

#[cfg(test)]
pub fn render_lockfile(manifest: &PackageManifest) -> String {
    render_lock_entries(&direct_lock_entries(manifest))
}

fn render_lockfile_graph(manifest: &PackageManifest) -> Result<String, String> {
    let entries = resolve_lock_entries(manifest)?;
    Ok(render_lock_entries(&entries))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LockPackage {
    name: String,
    version: String,
    source: String,
    content_hash: Option<String>,
    language: Option<String>,
    compatibility: Option<String>,
    manifest_schema: Option<u32>,
    dependencies: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LockPackagePin {
    name: String,
    version: String,
    source: String,
    content_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RegistryLockPin {
    name: String,
    version: String,
    source: String,
    content_hash: String,
}

fn validate_lockfile_registry_pins(
    manifest: &PackageManifest,
    lock_path: &Path,
    source: &str,
) -> Result<(), String> {
    let lock_pins = parse_lockfile_packages(source);
    let mut by_identity = BTreeMap::new();
    for package in lock_pins {
        by_identity.insert(
            (package.name, package.version, package.source),
            package.content_hash,
        );
    }

    let mut expected = Vec::new();
    let mut visited = BTreeSet::new();
    collect_registry_lock_pins(manifest, &mut visited, &mut expected)?;

    for pin in expected {
        let key = (pin.name.clone(), pin.version.clone(), pin.source.clone());
        let Some(recorded) = by_identity.get(&key) else {
            return Err(format!(
                "{} is missing registry lock entry for {} {}",
                lock_path.display(),
                pin.name,
                pin.version
            ));
        };
        let Some(recorded_hash) = recorded else {
            return Err(format!(
                "{} registry package {} {} is missing required content_hash pin",
                lock_path.display(),
                pin.name,
                pin.version
            ));
        };
        if recorded_hash != &pin.content_hash {
            return Err(format!(
                "{} registry package {} {} content_hash mismatch: lockfile has {}, resolved package has {}",
                lock_path.display(),
                pin.name,
                pin.version,
                recorded_hash,
                pin.content_hash
            ));
        }
    }

    Ok(())
}

fn collect_registry_lock_pins(
    manifest: &PackageManifest,
    visited: &mut BTreeSet<String>,
    pins: &mut Vec<RegistryLockPin>,
) -> Result<(), String> {
    let key = format!("{}@{}", manifest.project.name, manifest.project.version);
    if !visited.insert(key) {
        return Ok(());
    }

    for dependency in &manifest.dependencies {
        match dependency.source {
            DependencySource::Path(_) => {
                let dependency_manifest = load_path_dependency_manifest(manifest, dependency)?;
                validate_dependency_manifest_identity(dependency, &dependency_manifest)?;
                collect_registry_lock_pins(&dependency_manifest, visited, pins)?;
            }
            DependencySource::Registry => {
                let Some(registry) = LocalRegistry::discover_for(manifest) else {
                    continue;
                };
                let Some(resolved) = registry.resolve_with_metadata(dependency)? else {
                    continue;
                };
                validate_dependency_manifest_identity(dependency, &resolved.manifest)?;
                pins.push(RegistryLockPin {
                    name: resolved.manifest.project.name.clone(),
                    version: resolved.manifest.project.version.clone(),
                    source: dependency.source.lock_source(),
                    content_hash: resolved.content_hash,
                });
                collect_registry_lock_pins(&resolved.manifest, visited, pins)?;
            }
            DependencySource::Git(_) => {}
        }
    }

    Ok(())
}

fn parse_lockfile_packages(source: &str) -> Vec<LockPackagePin> {
    let mut packages = Vec::new();
    let mut current: Option<LockPackagePin> = None;

    for raw_line in source.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line == "[[package]]" {
            if let Some(package) = current.take() {
                packages.push(package);
            }
            current = Some(LockPackagePin {
                name: String::new(),
                version: String::new(),
                source: String::new(),
                content_hash: None,
            });
            continue;
        }

        let Some(package) = current.as_mut() else {
            continue;
        };
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = normalize_toml_key(key.trim());
        let value = value.trim();
        match key.as_str() {
            "name" => {
                if let Some(name) = parse_toml_string(value) {
                    package.name = name;
                }
            }
            "version" => {
                if let Some(version) = parse_toml_string(value) {
                    package.version = version;
                }
            }
            "source" => {
                if let Some(source) = parse_toml_string(value) {
                    package.source = source;
                }
            }
            "content_hash" => package.content_hash = parse_toml_string(value),
            _ => {}
        }
    }

    if let Some(package) = current {
        packages.push(package);
    }

    packages
}

#[cfg(test)]
fn direct_lock_entries(manifest: &PackageManifest) -> Vec<LockPackage> {
    let mut entries = vec![workspace_lock_package(manifest)];
    entries.extend(manifest.dependencies.iter().map(|dependency| LockPackage {
        name: dependency.name.clone(),
        version: dependency.version.clone(),
        source: dependency.source.lock_source(),
        content_hash: None,
        language: None,
        compatibility: None,
        manifest_schema: None,
        dependencies: Vec::new(),
    }));
    entries.sort_by(lock_package_order);
    if let Some(workspace_index) = entries.iter().position(|entry| entry.source == "workspace") {
        let workspace = entries.remove(workspace_index);
        entries.insert(0, workspace);
    }
    entries
}

fn resolve_lock_entries(manifest: &PackageManifest) -> Result<Vec<LockPackage>, String> {
    let mut packages = Vec::new();
    let mut visited = BTreeSet::new();
    resolve_lock_manifest(
        manifest,
        "workspace".to_string(),
        None,
        &mut visited,
        &mut packages,
    )?;

    if packages.len() > 1 {
        packages[1..].sort_by(lock_package_order);
    }
    Ok(packages)
}

fn resolve_lock_manifest(
    manifest: &PackageManifest,
    source: String,
    content_hash: Option<String>,
    visited: &mut BTreeSet<String>,
    packages: &mut Vec<LockPackage>,
) -> Result<(), String> {
    compatibility::validate_manifest(manifest)?;
    let key = format!(
        "{}@{} {source} {}",
        manifest.project.name,
        manifest.project.version,
        content_hash.as_deref().unwrap_or("")
    );
    if !visited.insert(key) {
        return Ok(());
    }

    let mut dependencies = manifest
        .dependencies
        .iter()
        .map(lock_dependency_label)
        .collect::<Vec<_>>();
    dependencies.sort();
    packages.push(LockPackage {
        name: manifest.project.name.clone(),
        version: manifest.project.version.clone(),
        source,
        content_hash,
        language: Some(manifest.language.version.clone()),
        compatibility: Some(manifest.language.compatibility.clone()),
        manifest_schema: Some(manifest.language.manifest_schema),
        dependencies,
    });

    for dependency in &manifest.dependencies {
        match dependency.source {
            DependencySource::Path(_) => {
                let dependency_manifest = load_path_dependency_manifest(manifest, dependency)?;
                validate_dependency_manifest_identity(dependency, &dependency_manifest)?;
                resolve_lock_manifest(
                    &dependency_manifest,
                    dependency.source.lock_source(),
                    None,
                    visited,
                    packages,
                )?;
            }
            DependencySource::Registry => {
                let Some(registry) = LocalRegistry::discover_for(manifest) else {
                    add_unresolved_lock_dependency(dependency, visited, packages);
                    continue;
                };
                let Some(resolved) = registry.resolve_with_metadata(dependency)? else {
                    add_unresolved_lock_dependency(dependency, visited, packages);
                    continue;
                };
                validate_dependency_manifest_identity(dependency, &resolved.manifest)?;
                resolve_lock_manifest(
                    &resolved.manifest,
                    dependency.source.lock_source(),
                    Some(resolved.content_hash),
                    visited,
                    packages,
                )?;
            }
            DependencySource::Git(_) => {
                let (dependency_manifest, resolved_source) =
                    load_git_dependency_manifest(manifest, dependency)?;
                validate_dependency_manifest_identity(dependency, &dependency_manifest)?;
                resolve_lock_manifest(
                    &dependency_manifest,
                    resolved_source,
                    None,
                    visited,
                    packages,
                )?;
            }
        }
    }

    Ok(())
}

fn add_unresolved_lock_dependency(
    dependency: &PackageDependency,
    visited: &mut BTreeSet<String>,
    packages: &mut Vec<LockPackage>,
) {
    let source = dependency.source.lock_source();
    let key = format!("{}@{} {source}", dependency.name, dependency.version);
    if !visited.insert(key) {
        return;
    }
    packages.push(LockPackage {
        name: dependency.name.clone(),
        version: dependency.version.clone(),
        source,
        content_hash: None,
        language: None,
        compatibility: None,
        manifest_schema: None,
        dependencies: Vec::new(),
    });
}

fn load_path_dependency_manifest(
    manifest: &PackageManifest,
    dependency: &PackageDependency,
) -> Result<PackageManifest, String> {
    let DependencySource::Path(path) = &dependency.source else {
        return Err(format!(
            "dependency `{}` is not a path dependency",
            dependency.name
        ));
    };
    let dependency_root = manifest.root.join(path);
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
    Ok(PackageManifest::parse(
        &dependency_root,
        &manifest_path,
        &source,
    ))
}

fn load_git_dependency_manifest(
    manifest: &PackageManifest,
    dependency: &PackageDependency,
) -> Result<(PackageManifest, String), String> {
    let DependencySource::Git(git) = &dependency.source else {
        return Err(format!(
            "dependency `{}` is not a git dependency",
            dependency.name
        ));
    };
    let checkout_root = git_dependency_checkout_root(manifest, dependency, git);
    checkout_git_dependency(git, &checkout_root)?;
    let manifest_path = checkout_root.join("num.toml");
    if !manifest_path.is_file() {
        return Err(format!(
            "git dependency `{}` has no num.toml at {}",
            dependency.name,
            manifest_path.display()
        ));
    }
    let source = fs::read_to_string(&manifest_path)
        .map_err(|err| format!("failed to read {}: {err}", manifest_path.display()))?;
    let resolved_rev = git_head_rev(&checkout_root)?;
    Ok((
        PackageManifest::parse(&checkout_root, &manifest_path, &source),
        git.lock_source_with_rev(&resolved_rev),
    ))
}

fn git_dependency_checkout_root(
    manifest: &PackageManifest,
    dependency: &PackageDependency,
    git: &PackageGitDependency,
) -> PathBuf {
    let key = format!(
        "{}@{} {}",
        dependency.name,
        dependency.version,
        git.lock_source()
    );
    manifest
        .root
        .join(".num-git")
        .join(format!("{}-{}", dependency.name, stable_hex_hash(&key)))
}

fn checkout_git_dependency(git: &PackageGitDependency, checkout_root: &Path) -> Result<(), String> {
    if checkout_root.join(".git").is_dir() {
        if let Some(rev) = git.pinned_rev() {
            if checkout_git_selector(checkout_root, rev).is_ok() {
                return Ok(());
            }
        }
        run_git(
            ["fetch", "--quiet", "--tags", "origin"],
            Some(checkout_root),
        )?;
    } else {
        if let Some(parent) = checkout_root.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
        }
        let checkout_path = checkout_root.display().to_string();
        run_git(["clone", "--quiet", &git.url, &checkout_path], None)?;
    }

    if let Some(selector) = git.checkout_selector() {
        checkout_git_selector(checkout_root, &selector)?;
    }

    Ok(())
}

fn checkout_git_selector(checkout_root: &Path, selector: &str) -> Result<(), String> {
    let checkout = Command::new("git")
        .arg("checkout")
        .arg("--quiet")
        .arg(selector)
        .current_dir(checkout_root)
        .output()
        .map_err(|err| format!("failed to run git checkout {selector}: {err}"))?;
    if checkout.status.success() {
        return Ok(());
    }

    let origin_selector = format!("origin/{selector}");
    let origin_checkout = Command::new("git")
        .arg("checkout")
        .arg("--quiet")
        .arg(&origin_selector)
        .current_dir(checkout_root)
        .output()
        .map_err(|err| format!("failed to run git checkout {origin_selector}: {err}"))?;
    if origin_checkout.status.success() {
        return Ok(());
    }

    Err(format!(
        "failed to checkout git selector `{selector}`\nstderr:\n{}",
        String::from_utf8_lossy(&checkout.stderr)
    ))
}

fn git_head_rev(checkout_root: &Path) -> Result<String, String> {
    let output = run_git(["rev-parse", "HEAD"], Some(checkout_root))?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn run_git<const N: usize>(
    args: [&str; N],
    cwd: Option<&Path>,
) -> Result<std::process::Output, String> {
    let mut command = Command::new("git");
    command.args(args);
    apply_git_dependency_env(&mut command);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let output = command
        .output()
        .map_err(|err| format!("failed to run git: {err}"))?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(format!(
            "git command failed\nstdout:\n{}\nstderr:\n{}\nGit dependency policy: non-interactive authentication is enforced; use preconfigured Git credentials or SSH agent access. Existing .num-git checkouts are reused offline only for explicit rev pins that are already present in the cache.",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn apply_git_dependency_env(command: &mut Command) {
    command.env("GIT_TERMINAL_PROMPT", "0");
    command.env("GIT_ASKPASS", "true");
    command.env("SSH_ASKPASS", "true");
    if std::env::var_os("GIT_SSH_COMMAND").is_none() {
        command.env("GIT_SSH_COMMAND", "ssh -oBatchMode=yes");
    }
}

fn stable_hex_hash(value: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in value.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn validate_dependency_manifest_identity(
    dependency: &PackageDependency,
    dependency_manifest: &PackageManifest,
) -> Result<(), String> {
    if dependency_manifest.project.name != dependency.name
        || dependency_manifest.project.version != dependency.version
    {
        return Err(format!(
            "dependency `{}` version `{}` resolved to package `{}` version `{}`",
            dependency.name,
            dependency.version,
            dependency_manifest.project.name,
            dependency_manifest.project.version
        ));
    }
    Ok(())
}

#[cfg(test)]
fn workspace_lock_package(manifest: &PackageManifest) -> LockPackage {
    let mut dependencies = manifest
        .dependencies
        .iter()
        .map(lock_dependency_label)
        .collect::<Vec<_>>();
    dependencies.sort();
    LockPackage {
        name: manifest.project.name.clone(),
        version: manifest.project.version.clone(),
        source: "workspace".to_string(),
        content_hash: None,
        language: Some(manifest.language.version.clone()),
        compatibility: Some(manifest.language.compatibility.clone()),
        manifest_schema: Some(manifest.language.manifest_schema),
        dependencies,
    }
}

fn lock_package_order(left: &LockPackage, right: &LockPackage) -> std::cmp::Ordering {
    left.name
        .cmp(&right.name)
        .then_with(|| left.version.cmp(&right.version))
        .then_with(|| left.source.cmp(&right.source))
        .then_with(|| left.content_hash.cmp(&right.content_hash))
}

fn lock_dependency_label(dependency: &PackageDependency) -> String {
    format!(
        "{}@{} {}",
        dependency.name,
        dependency.version,
        dependency.source.lock_source()
    )
}

fn render_lock_entries(entries: &[LockPackage]) -> String {
    let mut out = String::new();
    out.push_str("# This file is generated by `num lock`. Do not edit by hand.\n\n");
    out.push_str(&format!("version = {CURRENT_LOCKFILE_SCHEMA}\n\n"));
    for package in entries {
        out.push_str("[[package]]\n");
        push_lock_field(&mut out, "name", &package.name);
        push_lock_field(&mut out, "version", &package.version);
        push_lock_field(&mut out, "source", &package.source);
        if let Some(content_hash) = &package.content_hash {
            push_lock_field(&mut out, "content_hash", content_hash);
        }
        if let Some(language) = &package.language {
            push_lock_field(&mut out, "language", language);
        }
        if let Some(compatibility) = &package.compatibility {
            push_lock_field(&mut out, "compatibility", compatibility);
        }
        if let Some(manifest_schema) = package.manifest_schema {
            push_lock_u32_field(&mut out, "manifest_schema", manifest_schema);
        }
        push_lock_array_field(&mut out, "dependencies", &package.dependencies);
        out.push('\n');
    }

    out
}

fn parse_connector_process(method: &str, value: &str) -> Option<PackageConnectorProcess> {
    if let Some(command_line) = parse_toml_string(value) {
        let command_parts = split_command_line(&command_line).ok()?;
        return connector_from_parts(method, command_parts, None, None);
    }

    let fields = parse_inline_table(value)?;
    let command = fields.get("command")?.clone();
    let mut parts = vec![command];
    if let Some(args) = fields.get("args") {
        parts.extend(split_command_line(args).ok()?);
    }
    let timeout_ms = fields
        .get("timeout_ms")
        .and_then(|value| value.parse::<u64>().ok());
    connector_from_parts(method, parts, fields.get("cwd").cloned(), timeout_ms)
}

fn connector_from_parts(
    method: &str,
    mut parts: Vec<String>,
    cwd: Option<String>,
    timeout_ms: Option<u64>,
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
        timeout_ms,
    })
}

fn parse_javascript_module(method: &str, value: &str) -> Option<PackageJavaScriptModule> {
    let fields = parse_inline_table(value)?;
    let module = fields.get("module")?.clone();
    if method.trim().is_empty() || module.trim().is_empty() {
        return None;
    }
    let timeout_ms = fields
        .get("timeout_ms")
        .and_then(|value| value.parse::<u64>().ok());
    Some(PackageJavaScriptModule {
        method: method.to_string(),
        module,
        export: fields
            .get("export")
            .cloned()
            .unwrap_or_else(|| "default".to_string()),
        cwd: fields.get("cwd").cloned(),
        timeout_ms,
    })
}

fn upsert_sanitizer_pack(
    packs: &mut Vec<PackageSanitizerPack>,
    name: &str,
    key: &str,
    value: &str,
) {
    let name = name.trim();
    if name.is_empty() {
        return;
    }
    let index = packs.iter().position(|pack| pack.name == name);
    let index = index.unwrap_or_else(|| {
        packs.push(PackageSanitizerPack {
            name: name.to_string(),
            ..PackageSanitizerPack::default()
        });
        packs.len() - 1
    });
    let pack = &mut packs[index];
    match key {
        "extends" => pack.extends = parse_toml_string_array(value),
        "trim" => pack.trim = parse_toml_bool(value),
        "strip_control_chars" => pack.strip_control_chars = parse_toml_bool(value),
        "max_chars" => pack.max_chars = parse_toml_usize(value),
        "lowercase" => pack.lowercase = parse_toml_bool(value),
        "collapse_whitespace" => pack.collapse_whitespace = parse_toml_bool(value),
        "allowed_chars" => pack.allowed_chars = parse_toml_string(value),
        _ => {}
    }
}

fn upsert_secret_backend(
    backends: &mut Vec<PackageSecretBackend>,
    id: &str,
    key: &str,
    value: &str,
) {
    let id = id.trim();
    if id.is_empty() {
        return;
    }
    let index = backends.iter().position(|backend| backend.id == id);
    let index = index.unwrap_or_else(|| {
        backends.push(PackageSecretBackend {
            id: id.to_string(),
            provider: "external".to_string(),
            auth_method: "token".to_string(),
            ..PackageSecretBackend::default()
        });
        backends.len() - 1
    });
    let backend = &mut backends[index];
    match key {
        "provider" => {
            if let Some(provider) = parse_toml_string(value) {
                backend.provider = provider;
            }
        }
        "address" => backend.address = parse_toml_string(value),
        "mount" => backend.mount = parse_toml_string(value),
        "path_prefix" | "path" => backend.path_prefix = parse_toml_string(value),
        "auth_method" => {
            if let Some(auth_method) = parse_toml_string(value) {
                backend.auth_method = auth_method;
            }
        }
        "token_env" => {
            backend.token_env = parse_toml_string(value).and_then(|value| {
                let value = value.trim().to_string();
                (!value.is_empty()).then_some(value)
            })
        }
        "credential_env" | "credentials_env" => {
            backend.credential_env = normalize_env_names(parse_toml_string_array(value));
        }
        "optional" => {
            if let Some(optional) = parse_toml_bool(value) {
                backend.optional = optional;
            }
        }
        _ => {}
    }
}

fn upsert_ai_model(models: &mut Vec<PackageAiModel>, alias: &str, key: &str, value: &str) {
    let alias = alias.trim();
    if alias.is_empty() {
        return;
    }
    let index = models.iter().position(|model| model.alias == alias);
    let index = index.unwrap_or_else(|| {
        models.push(PackageAiModel {
            alias: alias.to_string(),
            provider: "external".to_string(),
            model: alias.to_string(),
            ..PackageAiModel::default()
        });
        models.len() - 1
    });
    let model = &mut models[index];
    match key {
        "provider" => {
            if let Some(provider) = parse_toml_string(value) {
                model.provider = provider;
            }
        }
        "model" | "model_id" => {
            if let Some(model_id) = parse_toml_string(value) {
                model.model = model_id;
            }
        }
        "credential_env" | "credentials_env" => {
            model.credential_env = normalize_env_names(parse_toml_string_array(value));
        }
        "timeout_ms" => model.timeout_ms = parse_toml_u64(value),
        "max_cost" | "default_max_cost" => model.max_cost = parse_toml_string(value),
        _ => {}
    }
}

fn upsert_ai_scanner(scanners: &mut Vec<PackageAiScanner>, alias: &str, key: &str, value: &str) {
    let alias = alias.trim();
    if alias.is_empty() {
        return;
    }
    let index = scanners.iter().position(|scanner| scanner.alias == alias);
    let index = index.unwrap_or_else(|| {
        scanners.push(PackageAiScanner {
            alias: alias.to_string(),
            provider: "fixture".to_string(),
            mode: "audit".to_string(),
            audit_redaction: "redacted".to_string(),
            ..PackageAiScanner::default()
        });
        scanners.len() - 1
    });
    let scanner = &mut scanners[index];
    match key {
        "provider" => {
            if let Some(provider) = parse_toml_string(value) {
                scanner.provider = provider;
            }
        }
        "mode" => {
            if let Some(mode) = parse_toml_string(value) {
                scanner.mode = mode;
            }
        }
        "block_threshold" | "threshold" => {
            scanner.block_threshold = parse_toml_string(value);
        }
        "audit_redaction" | "redaction" => {
            if let Some(audit_redaction) = parse_toml_string(value) {
                scanner.audit_redaction = audit_redaction;
            }
        }
        _ => {}
    }
}

fn pack_policy(pack: &PackageSanitizerPack) -> Result<TextSanitizationPolicy, String> {
    let mut policy = TextSanitizationPolicy::default();
    if let Some(trim) = pack.trim {
        policy.trim = trim;
    }
    if let Some(strip_control_chars) = pack.strip_control_chars {
        policy.strip_control_chars = strip_control_chars;
    }
    if let Some(max_chars) = pack.max_chars {
        if max_chars == 0 {
            return Err(format!(
                "sanitizer pack `{}` has invalid max_chars 0",
                pack.name
            ));
        }
        policy.max_chars = Some(max_chars);
    }
    if let Some(lowercase) = pack.lowercase {
        policy.lowercase = lowercase;
    }
    if let Some(collapse_whitespace) = pack.collapse_whitespace {
        policy.collapse_whitespace = collapse_whitespace;
    }
    if let Some(allowed_chars) = &pack.allowed_chars {
        policy.allowed_chars = Some(parse_text_char_class(&pack.name, allowed_chars)?);
    }
    Ok(policy)
}

fn parse_text_char_class(pack_name: &str, value: &str) -> Result<TextCharClass, String> {
    match value {
        "alpha_hyphen" | "latin_identifier" => Ok(TextCharClass::AlphaHyphen),
        "email" => Ok(TextCharClass::Email),
        "identifier" => Ok(TextCharClass::Identifier),
        "person_name" | "name" => Ok(TextCharClass::PersonName),
        other => Err(format!(
            "sanitizer pack `{pack_name}` has unknown allowed_chars `{other}`"
        )),
    }
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
        DependencySource::Git(PackageGitDependency {
            url: git.clone(),
            rev: fields.get("rev").cloned(),
            tag: fields.get("tag").cloned(),
            branch: fields.get("branch").cloned(),
            reference: fields.get("ref").cloned(),
        })
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

fn parse_toml_string_array(value: &str) -> Vec<String> {
    let value = value.trim();
    if value.len() < 2 || !value.starts_with('[') || !value.ends_with(']') {
        return Vec::new();
    }

    let inner = &value[1..value.len() - 1];
    split_toml_array_items(inner)
        .into_iter()
        .filter_map(|item| parse_toml_string(&item))
        .collect()
}

fn split_toml_array_items(inner: &str) -> Vec<String> {
    let mut items = Vec::new();
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
            let item = current.trim();
            if !item.is_empty() {
                items.push(item.to_string());
            }
            current.clear();
            continue;
        }
        current.push(ch);
    }

    let item = current.trim();
    if !item.is_empty() {
        items.push(item.to_string());
    }

    items
}

fn normalize_env_names(names: Vec<String>) -> Vec<String> {
    names
        .into_iter()
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
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

fn parse_toml_u64(value: &str) -> Option<u64> {
    value.trim().parse().ok()
}

fn parse_toml_i64(value: &str) -> Option<i64> {
    value.trim().parse().ok()
}

fn parse_toml_usize(value: &str) -> Option<usize> {
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

fn push_lock_array_field(out: &mut String, key: &str, values: &[String]) {
    out.push_str(key);
    out.push_str(" = [");
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        out.push('"');
        out.push_str(&escape_lock_string(value));
        out.push('"');
    }
    out.push_str("]\n");
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
        assert_eq!(
            manifest.language.version,
            compatibility::CURRENT_LANGUAGE_VERSION
        );
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
shared = { path = "../shared", version = "0.3.0" }
banking = { git = "https://example.com/banking.num.git", version = "1.4.0", rev = "abc123" }
"#,
        );

        assert_eq!(manifest.source_dir(), root.join("source"));
        assert_eq!(manifest.entry_path(), root.join("source/app.num"));
        assert_eq!(manifest.dependencies.len(), 3);
        assert_eq!(manifest.dependencies[0].name, "banking");
        assert_eq!(
            manifest.dependencies[0].source,
            DependencySource::Git(PackageGitDependency {
                url: "https://example.com/banking.num.git".to_string(),
                rev: Some("abc123".to_string()),
                tag: None,
                branch: None,
                reference: None,
            })
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
"payments.find" = { command = "node", args = "connectors/payments-find.js --mode real", cwd = "ops", timeout_ms = "1500" }
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
        assert_eq!(manifest.connectors[1].timeout_ms, Some(1500));
    }

    #[test]
    fn manifest_reads_javascript_modules() {
        let root = Path::new("/workspace/app");
        let manifest = PackageManifest::parse(
            root,
            &root.join("num.toml"),
            r#"
[project]
name = "app"
version = "0.1.0"

[javascript]
"risk.score" = { module = "interop/risk.cjs", export = "score", cwd = "ops", timeout_ms = "1500" }
"profile.enrich" = { module = "interop/profile.cjs" }
"#,
        );

        assert_eq!(manifest.javascript.len(), 2);
        assert_eq!(manifest.javascript[0].method, "profile.enrich");
        assert_eq!(manifest.javascript[0].module, "interop/profile.cjs");
        assert_eq!(manifest.javascript[0].export, "default");
        assert_eq!(manifest.javascript[1].method, "risk.score");
        assert_eq!(manifest.javascript[1].module, "interop/risk.cjs");
        assert_eq!(manifest.javascript[1].export, "score");
        assert_eq!(manifest.javascript[1].cwd, Some("ops".to_string()));
        assert_eq!(manifest.javascript[1].timeout_ms, Some(1500));
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

[security.jwt]
issuer = "https://issuer.example"
audience = "num-api"
algorithms = ["HS256"]
secret_env = "NUM_TEST_JWT_SECRET"
leeway_seconds = 30

[security.session]
cookie_name = "num_session"
secret_env = "NUM_TEST_SESSION_SECRET"
leeway_seconds = 15
"#,
        );

        assert_eq!(manifest.security.policy_mode, "strict");
        assert!(manifest.security.tenant_isolation);
        let jwt = manifest.security.jwt.as_ref().unwrap();
        assert_eq!(jwt.issuer, "https://issuer.example");
        assert_eq!(jwt.audience, "num-api");
        assert_eq!(jwt.algorithms, vec!["HS256"]);
        assert_eq!(jwt.secret_env, "NUM_TEST_JWT_SECRET");
        assert_eq!(jwt.leeway_seconds, 30);
        let session = manifest.security.session.as_ref().unwrap();
        assert_eq!(session.cookie_name, "num_session");
        assert_eq!(session.secret_env, "NUM_TEST_SESSION_SECRET");
        assert_eq!(session.leeway_seconds, 15);
    }

    #[test]
    fn manifest_reads_configured_sanitizer_packs() {
        let root = Path::new("/workspace/app");
        let manifest = PackageManifest::parse(
            root,
            &root.join("num.toml"),
            r#"
[project]
name = "app"
version = "0.1.0"

[sanitizer_packs.strict_latin_identifier]
extends = ["plain_text"]
max_chars = 32
lowercase = true
allowed_chars = "identifier"
"#,
        );

        assert_eq!(manifest.sanitizer_packs.len(), 1);
        let policies = manifest.sanitizer_pack_policies().unwrap();
        assert_eq!(policies.len(), 1);
        assert_eq!(policies[0].0, "strict_latin_identifier");
        assert_eq!(policies[0].1.max_chars, Some(32));
        assert!(policies[0].1.lowercase);
    }

    #[test]
    fn manifest_rejects_invalid_sanitizer_pack_config() {
        let root = Path::new("/workspace/app");
        let manifest = PackageManifest::parse(
            root,
            &root.join("num.toml"),
            r#"
[project]
name = "app"
version = "0.1.0"

[sanitizer_packs.bad]
max_chars = 0
"#,
        );

        let err = manifest.sanitizer_pack_policies().unwrap_err();

        assert!(err.contains("max_chars 0"));
    }

    #[test]
    fn manifest_rejects_recursive_sanitizer_pack_extends() {
        let root = Path::new("/workspace/app");
        let manifest = PackageManifest::parse(
            root,
            &root.join("num.toml"),
            r#"
[project]
name = "app"
version = "0.1.0"

[sanitizer_packs.loop]
extends = ["loop"]
"#,
        );

        let err = manifest.sanitizer_pack_policies().unwrap_err();

        assert!(err.contains("extends itself recursively"));
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
registry = "ghcr.io/acme"
image = "billing-api"
tag_strategy = "version"
credentials_ref = "secret://docker/ghcr"
"#,
        );

        assert_eq!(manifest.runtime.workflow_store, "file:.num-state");
        assert_eq!(manifest.runtime.audit_store, "file:audit/events.jsonl");
        assert_eq!(manifest.deployment.target, "container");
        assert_eq!(manifest.deployment.service, Some("BillingApi".to_string()));
        assert_eq!(manifest.deployment.region, Some("eu-west-1".to_string()));
        assert_eq!(manifest.deployment.artifact, "dist/deploy.json");
        assert_eq!(
            manifest.deployment.registry,
            Some("ghcr.io/acme".to_string())
        );
        assert_eq!(manifest.deployment.image, Some("billing-api".to_string()));
        assert_eq!(
            manifest.deployment.tag_strategy,
            Some("version".to_string())
        );
        assert_eq!(
            manifest.deployment.credentials_ref,
            Some("secret://docker/ghcr".to_string())
        );
    }

    #[test]
    fn manifest_reads_environment_metadata() {
        let root = Path::new("/workspace/app");
        let manifest = PackageManifest::parse(
            root,
            &root.join("num.toml"),
            r#"
[project]
name = "app"
version = "0.1.0"

[environment]
required = ["PAYMENTS_API_KEY", " SMTP_TOKEN ", "PAYMENTS_API_KEY"]
optional = ["NUM_LOG_LEVEL"]
"#,
        );

        assert_eq!(
            manifest.environment.required,
            vec!["PAYMENTS_API_KEY".to_string(), "SMTP_TOKEN".to_string()]
        );
        assert_eq!(
            manifest.environment.optional,
            vec!["NUM_LOG_LEVEL".to_string()]
        );
    }

    #[test]
    fn manifest_reads_external_secret_backend_metadata_without_values() {
        let root = Path::new("/workspace/app");
        let manifest = PackageManifest::parse(
            root,
            &root.join("num.toml"),
            r#"
[project]
name = "app"
version = "0.1.0"

[secrets.vault]
provider = "vault"
address = "https://vault.internal:8200"
mount = "secret"
path_prefix = "apps/billing"
auth_method = "token"
token_env = " VAULT_TOKEN "
credential_env = ["VAULT_ADDR", "VAULT_TOKEN", "VAULT_ADDR"]

[secrets.kms]
provider = "kms"
credential_env = ["KMS_KEYRING"]
optional = true
"#,
        );

        assert_eq!(manifest.secrets.len(), 2);
        assert_eq!(manifest.secrets[0].id, "kms");
        assert_eq!(manifest.secrets[0].provider, "kms");
        assert_eq!(manifest.secrets[0].credential_env, vec!["KMS_KEYRING"]);
        assert!(manifest.secrets[0].optional);
        assert_eq!(manifest.secrets[1].id, "vault");
        assert_eq!(manifest.secrets[1].provider, "vault");
        assert_eq!(
            manifest.secrets[1].address,
            Some("https://vault.internal:8200".to_string())
        );
        assert_eq!(manifest.secrets[1].mount, Some("secret".to_string()));
        assert_eq!(
            manifest.secrets[1].path_prefix,
            Some("apps/billing".to_string())
        );
        assert_eq!(manifest.secrets[1].auth_method, "token");
        assert_eq!(
            manifest.secrets[1].token_env,
            Some("VAULT_TOKEN".to_string())
        );
        assert_eq!(
            manifest.secrets[1].credential_env,
            vec!["VAULT_ADDR".to_string(), "VAULT_TOKEN".to_string()]
        );
        assert!(!format!("{manifest:?}").contains("secret-value"));
    }

    #[test]
    fn manifest_reads_ai_model_metadata_without_values() {
        let root = Path::new("/workspace/app");
        let manifest = PackageManifest::parse(
            root,
            &root.join("num.toml"),
            r#"
[project]
name = "app"
version = "0.1.0"

[ai]
default_model = "fast-classifier"

[ai.models.reasoner]
provider = "anthropic"
model = "claude-3-5-sonnet"
credential_env = ["ANTHROPIC_API_KEY"]
timeout_ms = 12000
max_cost = "0.50 USD"
future_temperature = "0.2"

[ai.models.fast-classifier]
provider = "openai"
model = "gpt-4.1-mini"
credential_env = [" OPENAI_API_KEY ", "OPENAI_ORG", "OPENAI_API_KEY"]
timeout_ms = 5000
max_cost = "0.10 USD"
region = "future-region"

[ai.scanners.fixture-safe]
provider = "fixture"
mode = "audit"
future_rule_pack = "ignored"

[ai.scanners.prompt-guard]
provider = "fixture"
mode = "block"
block_threshold = "blocked"
audit_redaction = "redacted"
"#,
        );

        assert_eq!(
            manifest.ai.default_model,
            Some("fast-classifier".to_string())
        );
        assert_eq!(manifest.ai.models.len(), 2);
        assert_eq!(manifest.ai.models[0].alias, "fast-classifier");
        assert_eq!(manifest.ai.models[0].provider, "openai");
        assert_eq!(manifest.ai.models[0].model, "gpt-4.1-mini");
        assert_eq!(
            manifest.ai.models[0].credential_env,
            vec!["OPENAI_API_KEY".to_string(), "OPENAI_ORG".to_string()]
        );
        assert_eq!(manifest.ai.models[0].timeout_ms, Some(5000));
        assert_eq!(manifest.ai.models[0].max_cost, Some("0.10 USD".to_string()));
        assert_eq!(manifest.ai.models[1].alias, "reasoner");
        assert_eq!(manifest.ai.models[1].provider, "anthropic");
        assert_eq!(manifest.ai.models[1].model, "claude-3-5-sonnet");
        assert_eq!(manifest.ai.models[1].timeout_ms, Some(12000));
        assert_eq!(manifest.ai.models[1].max_cost, Some("0.50 USD".to_string()));
        assert_eq!(manifest.ai.scanners.len(), 2);
        assert_eq!(manifest.ai.scanners[0].alias, "fixture-safe");
        assert_eq!(manifest.ai.scanners[0].provider, "fixture");
        assert_eq!(manifest.ai.scanners[0].mode, "audit");
        assert_eq!(manifest.ai.scanners[0].block_threshold, None);
        assert_eq!(manifest.ai.scanners[0].audit_redaction, "redacted");
        assert_eq!(manifest.ai.scanners[1].alias, "prompt-guard");
        assert_eq!(manifest.ai.scanners[1].provider, "fixture");
        assert_eq!(manifest.ai.scanners[1].mode, "block");
        assert_eq!(
            manifest.ai.scanners[1].block_threshold,
            Some("blocked".to_string())
        );
        assert_eq!(manifest.ai.scanners[1].audit_redaction, "redacted");
        assert!(!format!("{manifest:?}").contains("secret-value"));
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
        assert!(lockfile.contains(&format!(
            "language = \"{}\"",
            compatibility::CURRENT_LANGUAGE_VERSION
        )));
        assert!(lockfile.contains("manifest_schema = 1"));
        assert!(lockfile.find("name = \"alpha\"") < lockfile.find("name = \"zeta\""));
        assert!(lockfile.contains("source = \"path:../alpha\""));
    }

    #[test]
    fn lockfile_pins_git_dependency_selectors() {
        let root = Path::new("/workspace/app");
        let manifest = PackageManifest::parse(
            root,
            &root.join("num.toml"),
            r#"
[project]
name = "app"
version = "0.1.0"

[dependencies]
banking = { git = "https://example.com/banking.num.git", version = "1.4.0", rev = "abc123" }
billing = { git = "https://example.com/billing.num.git", version = "2.0.0", tag = "v2.0.0" }
ledger = { git = "https://example.com/ledger.num.git", version = "0.8.0", branch = "release" }
audit = { git = "https://example.com/audit.num.git", version = "0.3.0", ref = "refs/pull/1/head" }
"#,
        );

        let lockfile = render_lockfile(&manifest);

        assert!(
            lockfile.contains("source = \"git:https://example.com/banking.num.git#rev:abc123\"")
        );
        assert!(
            lockfile.contains("source = \"git:https://example.com/billing.num.git#tag:v2.0.0\"")
        );
        assert!(
            lockfile.contains("source = \"git:https://example.com/ledger.num.git#branch:release\"")
        );
        assert!(lockfile
            .contains("source = \"git:https://example.com/audit.num.git#ref:refs/pull/1/head\""));
        assert!(lockfile.contains(
            "dependencies = [\"audit@0.3.0 git:https://example.com/audit.num.git#ref:refs/pull/1/head\""
        ));
    }

    #[test]
    fn validates_current_lockfile_schema() {
        let root = Path::new("/workspace/app");
        let lock_path = root.join("num.lock");

        validate_lockfile_source(
            &lock_path,
            r#"
# generated
version = 1

[[package]]
name = "app"
version = "0.1.0"
source = "workspace"
"#,
        )
        .unwrap();
    }

    #[test]
    fn rejects_unsupported_lockfile_schema() {
        let root = Path::new("/workspace/app");
        let lock_path = root.join("num.lock");

        let future = validate_lockfile_source(
            &lock_path,
            r#"
version = 2

[[package]]
name = "app"
"#,
        )
        .unwrap_err();
        assert!(future.contains("requires lockfile version 2"));

        let zero = validate_lockfile_source(&lock_path, "version = 0\n").unwrap_err();
        assert!(zero.contains("invalid lockfile version 0"));

        let missing = validate_lockfile_source(&lock_path, "[[package]]\n").unwrap_err();
        assert!(missing.contains("missing lockfile `version`"));
    }

    #[test]
    fn migrates_legacy_lockfile_without_schema_header() {
        let root = temp_package_dir("migrate_lock_missing_schema");
        fs::write(
            root.join("num.toml"),
            r#"
[project]
name = "app"
version = "0.1.0"
"#,
        )
        .unwrap();
        fs::write(
            root.join("num.lock"),
            r#"# generated by an older num CLI

[[package]]
name = "app"
version = "0.1.0"
source = "workspace"
"#,
        )
        .unwrap();

        let plan = migrate_lockfile(&root, false).unwrap();
        assert_eq!(plan.schema, None);
        assert_eq!(plan.target_schema, CURRENT_LOCKFILE_SCHEMA);
        assert!(plan.changed);
        assert!(!plan.applied);
        assert!(plan
            .actions
            .contains(&"add lockfile schema version 1".to_string()));
        assert!(!fs::read_to_string(root.join("num.lock"))
            .unwrap()
            .contains("version = 1\n\n[[package]]"));

        let migration = migrate_lockfile(&root, true).unwrap();
        assert!(migration.changed);
        assert!(migration.applied);
        let migrated = fs::read_to_string(root.join("num.lock")).unwrap();
        assert!(migrated.contains("# generated by an older num CLI"));
        assert!(migrated.contains("version = 1\n\n[[package]]"));
        validate_project_lockfile(&root).unwrap();

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn migrates_lockfile_schema_zero_to_current_schema() {
        let root = temp_package_dir("migrate_lock_schema_zero");
        fs::write(
            root.join("num.toml"),
            r#"
[project]
name = "app"
version = "0.1.0"
"#,
        )
        .unwrap();
        fs::write(root.join("num.lock"), "version = 0\n\n[[package]]\n").unwrap();

        let migration = migrate_lockfile(&root, true).unwrap();

        assert_eq!(migration.schema, Some(0));
        assert!(migration
            .actions
            .contains(&"upgrade lockfile schema from 0 to 1".to_string()));
        assert!(migration.applied);
        assert!(fs::read_to_string(root.join("num.lock"))
            .unwrap()
            .starts_with("version = 1\n\n[[package]]"));
        validate_project_lockfile(&root).unwrap();

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn lockfile_migration_rejects_future_schema() {
        let root = temp_package_dir("migrate_lock_future_schema");
        fs::write(
            root.join("num.toml"),
            r#"
[project]
name = "app"
version = "0.1.0"
"#,
        )
        .unwrap();
        fs::write(root.join("num.lock"), "version = 2\n\n[[package]]\n").unwrap();

        let err = migrate_lockfile(&root, false).unwrap_err();

        assert!(err.contains("cannot migrate lockfile schema 2"));
        fs::remove_dir_all(root).unwrap();
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
    fn validates_project_lockfile_next_to_manifest() {
        let root = temp_package_dir("check_lock");
        fs::write(
            root.join("num.toml"),
            r#"
[project]
name = "app"
version = "0.1.0"
"#,
        )
        .unwrap();
        let path = write_lockfile(&root).unwrap();

        assert_eq!(validate_project_lockfile(&root).unwrap(), path);

        fs::write(root.join("num.lock"), "version = 2\n").unwrap();
        let err = validate_project_lockfile(&root).unwrap_err();
        assert!(err.contains("requires lockfile version 2"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn lockfile_pins_transitive_path_dependencies() {
        let root = temp_package_dir("path_lock_root");
        let shared = root.with_file_name(format!(
            "{}_shared",
            root.file_name().unwrap().to_string_lossy()
        ));
        let core = root.with_file_name(format!(
            "{}_core",
            root.file_name().unwrap().to_string_lossy()
        ));
        fs::create_dir_all(&shared).unwrap();
        fs::create_dir_all(&core).unwrap();

        write_manifest(
            &root,
            &format!(
                r#"
[project]
name = "app"
version = "0.1.0"

[dependencies]
shared = {{ path = "{}", version = "0.3.0" }}
"#,
                path_to_toml_string(shared.display().to_string())
            ),
        );
        write_manifest(
            &shared,
            &format!(
                r#"
[project]
name = "shared"
version = "0.3.0"

[dependencies]
core = {{ path = "{}", version = "1.0.0" }}
"#,
                path_to_toml_string(core.display().to_string())
            ),
        );
        write_manifest(
            &core,
            r#"
[project]
name = "core"
version = "1.0.0"
"#,
        );

        let path = write_lockfile(&root).unwrap();
        let lockfile = fs::read_to_string(path).unwrap();

        assert!(lockfile.contains("name = \"app\""));
        assert!(lockfile.contains("name = \"shared\""));
        assert!(lockfile.contains("name = \"core\""));
        assert!(lockfile.contains("dependencies = [\"shared@0.3.0 path:"));
        assert!(lockfile.contains("dependencies = [\"core@1.0.0 path:"));
        assert!(lockfile.find("name = \"core\"") < lockfile.find("name = \"shared\""));

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(shared).unwrap();
        fs::remove_dir_all(core).unwrap();
    }

    #[test]
    fn lockfile_rejects_resolved_dependency_identity_mismatch() {
        let root = temp_package_dir("path_lock_mismatch_root");
        let shared = root.with_file_name(format!(
            "{}_shared",
            root.file_name().unwrap().to_string_lossy()
        ));
        fs::create_dir_all(&shared).unwrap();

        write_manifest(
            &root,
            &format!(
                r#"
[project]
name = "app"
version = "0.1.0"

[dependencies]
shared = {{ path = "{}", version = "0.3.0" }}
"#,
                path_to_toml_string(shared.display().to_string())
            ),
        );
        write_manifest(
            &shared,
            r#"
[project]
name = "shared"
version = "0.4.0"
"#,
        );

        let err = write_lockfile(&root).unwrap_err();

        assert!(err.contains(
            "dependency `shared` version `0.3.0` resolved to package `shared` version `0.4.0`"
        ));

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(shared).unwrap();
    }

    #[test]
    fn lockfile_resolves_local_git_dependency_to_commit_sha() {
        let root = temp_package_dir("git_lock_root");
        let shared = root.with_file_name(format!(
            "{}_shared_git",
            root.file_name().unwrap().to_string_lossy()
        ));
        fs::create_dir_all(&shared).unwrap();
        write_manifest(
            &shared,
            r#"
[project]
name = "shared"
version = "0.3.0"
"#,
        );
        init_git_repo(&shared);
        let rev = git_head_rev(&shared).unwrap();

        write_manifest(
            &root,
            &format!(
                r#"
[project]
name = "app"
version = "0.1.0"

[dependencies]
shared = {{ git = "{}", version = "0.3.0", rev = "{}" }}
"#,
                path_to_toml_string(shared.display().to_string()),
                rev
            ),
        );

        let path = write_lockfile(&root).unwrap();
        let lockfile = fs::read_to_string(path).unwrap();

        assert!(lockfile.contains("name = \"app\""));
        assert!(lockfile.contains("name = \"shared\""));
        assert!(lockfile.contains(&format!("source = \"git:{}#rev:{rev}\"", shared.display())));
        assert!(lockfile.contains(&format!(
            "language = \"{}\"",
            compatibility::CURRENT_LANGUAGE_VERSION
        )));
        assert!(root.join(".num-git").is_dir());

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(shared).unwrap();
    }

    #[test]
    fn lockfile_reuses_cached_git_rev_without_fetching_origin() {
        let root = temp_package_dir("git_lock_cache_root");
        let shared = root.with_file_name(format!(
            "{}_shared_git",
            root.file_name().unwrap().to_string_lossy()
        ));
        fs::create_dir_all(&shared).unwrap();
        write_manifest(
            &shared,
            r#"
[project]
name = "shared"
version = "0.3.0"
"#,
        );
        init_git_repo(&shared);
        let rev = git_head_rev(&shared).unwrap();

        write_manifest(
            &root,
            &format!(
                r#"
[project]
name = "app"
version = "0.1.0"

[dependencies]
shared = {{ git = "{}", version = "0.3.0", rev = "{}" }}
"#,
                path_to_toml_string(shared.display().to_string()),
                rev
            ),
        );

        write_lockfile(&root).unwrap();
        fs::remove_dir_all(&shared).unwrap();

        let path = write_lockfile(&root).unwrap();
        let lockfile = fs::read_to_string(path).unwrap();

        assert!(lockfile.contains(&format!("source = \"git:{}#rev:{rev}\"", shared.display())));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn git_dependency_failures_include_non_interactive_auth_policy() {
        let err = run_git(["clone", "--quiet", "/definitely/missing/num.git"], None).unwrap_err();

        assert!(err.contains("non-interactive authentication is enforced"));
        assert!(err.contains(".num-git checkouts are reused offline only for explicit rev pins"));
    }

    #[test]
    fn lockfile_pins_transitive_local_registry_dependencies() {
        let root = temp_package_dir("registry_lock_root");
        let registry = root.with_file_name(format!(
            "{}_registry",
            root.file_name().unwrap().to_string_lossy()
        ));
        let shared = registry.join("shared").join("0.3.0");
        let core = registry.join("core").join("1.0.0");
        fs::create_dir_all(&shared).unwrap();
        fs::create_dir_all(&core).unwrap();

        write_manifest(
            &root,
            &format!(
                r#"
[project]
name = "app"
version = "0.1.0"

[registry]
path = "{}"

[dependencies]
shared = "0.3.0"
"#,
                path_to_toml_string(registry.display().to_string())
            ),
        );
        write_manifest(
            &shared,
            r#"
[project]
name = "shared"
version = "0.3.0"

[dependencies]
core = "1.0.0"
"#,
        );
        write_manifest(
            &core,
            r#"
[project]
name = "core"
version = "1.0.0"
"#,
        );

        let path = write_lockfile(&root).unwrap();
        let lockfile = fs::read_to_string(path).unwrap();

        assert!(lockfile.contains("name = \"shared\""));
        assert!(lockfile.contains("name = \"core\""));
        assert!(lockfile.contains("source = \"registry\""));
        assert_eq!(lockfile.matches("content_hash = \"sha256:").count(), 2);
        assert!(lockfile.contains("dependencies = [\"core@1.0.0 registry\"]"));

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(registry).unwrap();
    }

    #[test]
    fn lockfile_check_validates_matching_local_registry_content_pins() {
        let root = temp_package_dir("registry_lock_check_root");
        let registry = root.with_file_name(format!(
            "{}_registry",
            root.file_name().unwrap().to_string_lossy()
        ));
        let shared = registry.join("shared").join("0.3.0");
        fs::create_dir_all(&shared).unwrap();

        write_manifest(
            &root,
            &format!(
                r#"
[project]
name = "app"
version = "0.1.0"

[registry]
path = "{}"

[dependencies]
shared = "0.3.0"
"#,
                path_to_toml_string(registry.display().to_string())
            ),
        );
        write_manifest(
            &shared,
            r#"
[project]
name = "shared"
version = "0.3.0"
"#,
        );

        let path = write_lockfile(&root).unwrap();

        assert_eq!(validate_project_lockfile(&root).unwrap(), path);

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(registry).unwrap();
    }

    #[test]
    fn lockfile_check_rejects_changed_local_registry_content_pin() {
        let root = temp_package_dir("registry_lock_changed_root");
        let registry = root.with_file_name(format!(
            "{}_registry",
            root.file_name().unwrap().to_string_lossy()
        ));
        let shared = registry.join("shared").join("0.3.0");
        fs::create_dir_all(&shared).unwrap();

        write_manifest(
            &root,
            &format!(
                r#"
[project]
name = "app"
version = "0.1.0"

[registry]
path = "{}"

[dependencies]
shared = "0.3.0"
"#,
                path_to_toml_string(registry.display().to_string())
            ),
        );
        write_manifest(
            &shared,
            r#"
[project]
name = "shared"
version = "0.3.0"
"#,
        );

        write_lockfile(&root).unwrap();
        fs::write(shared.join("README.md"), "changed remote package bytes").unwrap();
        let err = validate_project_lockfile(&root).unwrap_err();

        assert!(err.contains("content_hash mismatch"));
        assert!(err.contains("shared 0.3.0"));

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(registry).unwrap();
    }

    #[test]
    fn lockfile_check_rejects_missing_registry_content_hash_pin() {
        let root = temp_package_dir("registry_lock_missing_hash_root");
        let registry = root.with_file_name(format!(
            "{}_registry",
            root.file_name().unwrap().to_string_lossy()
        ));
        let shared = registry.join("shared").join("0.3.0");
        fs::create_dir_all(&shared).unwrap();

        write_manifest(
            &root,
            &format!(
                r#"
[project]
name = "app"
version = "0.1.0"

[registry]
path = "{}"

[dependencies]
shared = "0.3.0"
"#,
                path_to_toml_string(registry.display().to_string())
            ),
        );
        write_manifest(
            &shared,
            r#"
[project]
name = "shared"
version = "0.3.0"
"#,
        );

        let path = write_lockfile(&root).unwrap();
        let without_hash = fs::read_to_string(&path)
            .unwrap()
            .lines()
            .filter(|line| !line.trim_start().starts_with("content_hash = "))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        fs::write(&path, without_hash).unwrap();
        let err = validate_project_lockfile(&root).unwrap_err();

        assert!(err.contains("missing required content_hash pin"));
        assert!(err.contains("shared 0.3.0"));

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(registry).unwrap();
    }

    #[test]
    fn lockfile_check_keeps_path_dependencies_hashless() {
        let root = temp_package_dir("path_lock_check_root");
        let shared = root.with_file_name(format!(
            "{}_shared",
            root.file_name().unwrap().to_string_lossy()
        ));
        fs::create_dir_all(&shared).unwrap();

        write_manifest(
            &root,
            &format!(
                r#"
[project]
name = "app"
version = "0.1.0"

[dependencies]
shared = {{ path = "{}", version = "0.3.0" }}
"#,
                path_to_toml_string(shared.display().to_string())
            ),
        );
        write_manifest(
            &shared,
            r#"
[project]
name = "shared"
version = "0.3.0"
"#,
        );

        let path = write_lockfile(&root).unwrap();
        let lockfile = fs::read_to_string(&path).unwrap();

        assert!(!lockfile.contains("content_hash = "));
        assert_eq!(validate_project_lockfile(&root).unwrap(), path);

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(shared).unwrap();
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
shared = {{ path = "{}", version = "0.3.0" }}
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
version = "0.3.0"
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

    fn write_manifest(root: &Path, source: &str) {
        fs::write(root.join("num.toml"), source).unwrap();
    }

    fn init_git_repo(root: &Path) {
        run_git(["init", "--quiet"], Some(root)).unwrap();
        run_git(["add", "num.toml"], Some(root)).unwrap();
        let output = Command::new("git")
            .args([
                "-c",
                "user.email=num@example.com",
                "-c",
                "user.name=num",
                "commit",
                "--quiet",
                "-m",
                "init",
            ])
            .current_dir(root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git commit failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
