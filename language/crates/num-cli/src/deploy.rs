use crate::compatibility::CompatibilityReport;
use crate::package::{self, DependencySource, PackageManifest};
use num_compiler::ast::{Declaration, Module, Risk};
use num_compiler::SourceFile;
use serde_json::{json, Value};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct DeploymentPlan {
    pub package_name: String,
    pub package_version: String,
    pub compatibility: CompatibilityDeployment,
    pub target: String,
    pub service: Option<String>,
    pub region: Option<String>,
    pub artifact: String,
    pub target_profile: DeploymentTargetProfile,
    pub source: String,
    pub entry: String,
    pub runtime: RuntimeDeployment,
    pub environment: DeploymentEnvironment,
    pub security: SecurityDeployment,
    pub modules: usize,
    pub workflows: Vec<String>,
    pub actions: Vec<ActionDeployment>,
    pub services: Vec<ServiceDeployment>,
    pub connectors: Vec<String>,
    pub process_connectors: Vec<String>,
    pub process_connector_bindings: Vec<ProcessConnectorDeployment>,
    pub dependencies: Vec<DependencyDeployment>,
}

#[derive(Debug, Clone)]
pub struct CompatibilityDeployment {
    pub language_version: String,
    pub current_language_version: String,
    pub compatibility: String,
    pub manifest_schema: u32,
    pub current_manifest_schema: u32,
}

#[derive(Debug, Clone)]
pub struct RuntimeDeployment {
    pub workflow_store: String,
    pub audit_store: String,
}

#[derive(Debug, Clone)]
pub struct DeploymentEnvironment {
    pub status: String,
    pub required: Vec<EnvironmentVariableDeployment>,
    pub optional: Vec<EnvironmentVariableDeployment>,
    pub missing_required: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct EnvironmentVariableDeployment {
    pub name: String,
    pub present: bool,
}

#[derive(Debug, Clone)]
pub struct DeploymentTargetProfile {
    pub class: String,
    pub execution: String,
    pub required_artifacts: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SecurityDeployment {
    pub policy_mode: String,
    pub tenant_isolation: bool,
}

#[derive(Debug, Clone)]
pub struct ActionDeployment {
    pub name: String,
    pub risk: String,
    pub requires: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ServiceDeployment {
    pub name: String,
    pub routes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DependencyDeployment {
    pub name: String,
    pub version: String,
    pub source: String,
}

#[derive(Debug, Clone)]
pub struct ProcessConnectorDeployment {
    pub method: String,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct DeploymentArtifactReport {
    pub artifact_root: PathBuf,
    pub plan_path: PathBuf,
    pub manifest_path: PathBuf,
    pub lock_path: Option<PathBuf>,
    pub modules_dir: PathBuf,
    pub runbook_path: PathBuf,
    pub metadata_path: PathBuf,
    pub files: Vec<String>,
}

pub fn build_deployment_plan(
    manifest: &PackageManifest,
    module: &Module,
    module_count: usize,
) -> DeploymentPlan {
    let mut workflows = Vec::new();
    let mut actions = Vec::new();
    let mut services = Vec::new();
    let mut connectors = Vec::new();

    for declaration in &module.declarations {
        match declaration {
            Declaration::Workflow(workflow) => workflows.push(workflow.name.clone()),
            Declaration::Action(action) => actions.push(ActionDeployment {
                name: action.name.clone(),
                risk: risk_label(action.risk).to_string(),
                requires: action.requires.clone(),
            }),
            Declaration::Service(service) => services.push(ServiceDeployment {
                name: service.name.clone(),
                routes: service
                    .routes
                    .iter()
                    .map(|route| format!("{} {}", route.method, route.path))
                    .collect(),
            }),
            Declaration::Connector(connector) => connectors.push(connector.name.clone()),
            _ => {}
        }
    }

    workflows.sort();
    actions.sort_by(|left, right| left.name.cmp(&right.name));
    services.sort_by(|left, right| left.name.cmp(&right.name));
    connectors.sort();

    DeploymentPlan {
        package_name: manifest.project.name.clone(),
        package_version: manifest.project.version.clone(),
        compatibility: compatibility_from_report(&CompatibilityReport::from_manifest(manifest)),
        target: manifest.deployment.target.clone(),
        service: manifest.deployment.service.clone(),
        region: manifest.deployment.region.clone(),
        artifact: manifest.deployment.artifact.clone(),
        target_profile: DeploymentTargetProfile::for_manifest(manifest),
        source: manifest.project.source.clone(),
        entry: manifest.project.entry.clone(),
        runtime: RuntimeDeployment {
            workflow_store: manifest.runtime.workflow_store.clone(),
            audit_store: manifest.runtime.audit_store.clone(),
        },
        environment: DeploymentEnvironment::from_manifest(manifest),
        security: SecurityDeployment {
            policy_mode: manifest.security.policy_mode.clone(),
            tenant_isolation: manifest.security.tenant_isolation,
        },
        modules: module_count,
        workflows,
        actions,
        services,
        connectors,
        process_connectors: manifest
            .connectors
            .iter()
            .map(|connector| connector.method.clone())
            .collect(),
        process_connector_bindings: manifest
            .connectors
            .iter()
            .map(|connector| ProcessConnectorDeployment {
                method: connector.method.clone(),
                command: connector.command.clone(),
                args: connector.args.clone(),
                cwd: connector.cwd.clone(),
                timeout_ms: connector.timeout_ms,
            })
            .collect(),
        dependencies: manifest
            .dependencies
            .iter()
            .map(|dependency| DependencyDeployment {
                name: dependency.name.clone(),
                version: dependency.version.clone(),
                source: dependency_source_label(&dependency.source),
            })
            .collect(),
    }
}

pub fn default_artifact_root(manifest: &PackageManifest) -> PathBuf {
    let artifact = PathBuf::from(&manifest.deployment.artifact);
    let path = if artifact.is_absolute() {
        artifact
    } else {
        manifest.root.join(artifact)
    };
    if path.extension().is_some() {
        path.with_extension("")
    } else {
        path
    }
}

impl DeploymentPlan {
    pub fn to_json(&self) -> Value {
        json!({
            "package": {
                "name": self.package_name,
                "version": self.package_version,
            },
            "compatibility": {
                "language": {
                    "version": self.compatibility.language_version,
                    "current": self.compatibility.current_language_version,
                    "compatibility": self.compatibility.compatibility,
                },
                "manifest": {
                    "schema": self.compatibility.manifest_schema,
                    "current_schema": self.compatibility.current_manifest_schema,
                },
            },
            "deployment": {
                "target": self.target,
                "service": self.service,
                "region": self.region,
                "artifact": self.artifact,
                "profile": self.target_profile.to_json(),
            },
            "project": {
                "source": self.source,
                "entry": self.entry,
                "modules": self.modules,
            },
            "runtime": {
                "workflow_store": self.runtime.workflow_store,
                "audit_store": self.runtime.audit_store,
            },
            "environment": self.environment.to_json(),
            "security": {
                "policy_mode": self.security.policy_mode,
                "tenant_isolation": self.security.tenant_isolation,
            },
            "workflows": self.workflows,
            "actions": self.actions.iter().map(ActionDeployment::to_json).collect::<Vec<_>>(),
            "services": self.services.iter().map(ServiceDeployment::to_json).collect::<Vec<_>>(),
            "connectors": self.connectors,
            "process_connectors": self.process_connectors,
            "process_connector_bindings": self.process_connector_bindings.iter().map(ProcessConnectorDeployment::to_json).collect::<Vec<_>>(),
            "dependencies": self.dependencies.iter().map(DependencyDeployment::to_json).collect::<Vec<_>>(),
        })
    }

    pub fn render_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "Deployment plan: {} {}\n",
            self.package_name, self.package_version
        ));
        out.push_str(&format!("Target: {}\n", self.target));
        out.push_str(&format!(
            "Target profile: class={}, execution={}\n",
            self.target_profile.class, self.target_profile.execution
        ));
        if let Some(service) = &self.service {
            out.push_str(&format!("Service: {service}\n"));
        }
        if let Some(region) = &self.region {
            out.push_str(&format!("Region: {region}\n"));
        }
        out.push_str(&format!("Artifact: {}\n", self.artifact));
        out.push_str(&format!(
            "Compatibility: language {} against current {} ({}), manifest schema {}\n",
            self.compatibility.language_version,
            self.compatibility.current_language_version,
            self.compatibility.compatibility,
            self.compatibility.manifest_schema
        ));
        out.push_str(&format!("Entry: {}\n", self.entry));
        out.push_str(&format!("Modules: {}\n", self.modules));
        out.push_str(&format!(
            "Runtime: workflow_store={}, audit_store={}\n",
            self.runtime.workflow_store, self.runtime.audit_store
        ));
        out.push_str(&format!(
            "Environment: status={}, required={}, optional={}\n",
            self.environment.status,
            self.environment.required.len(),
            self.environment.optional.len()
        ));
        out.push_str(&format!(
            "Security: policy_mode={}, tenant_isolation={}\n",
            self.security.policy_mode, self.security.tenant_isolation
        ));
        out.push_str(&format!("Workflows: {}\n", self.workflows.join(", ")));
        out.push_str(&format!(
            "Services: {}\n",
            self.services
                .iter()
                .map(|service| service.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
        out.push_str(&format!("Connectors: {}\n", self.connectors.join(", ")));
        if !self.process_connectors.is_empty() {
            out.push_str(&format!(
                "Process connectors: {}\n",
                self.process_connectors.join(", ")
            ));
        }
        if !self.target_profile.warnings.is_empty() {
            out.push_str("Deployment warnings:\n");
            for warning in &self.target_profile.warnings {
                out.push_str(&format!("  - {warning}\n"));
            }
        }
        if !self.environment.missing_required.is_empty() {
            out.push_str("Environment warnings:\n");
            for name in &self.environment.missing_required {
                out.push_str(&format!("  - missing required env `{name}`\n"));
            }
        }
        out
    }
}

impl DeploymentTargetProfile {
    fn for_manifest(manifest: &PackageManifest) -> Self {
        let target = manifest.deployment.target.trim();
        let service = manifest.deployment.service.as_deref();
        let region = manifest.deployment.region.as_deref();
        let mut warnings = Vec::new();

        let (class, execution, required_artifacts) = match normalize_target(target).as_str() {
            "local" => (
                "local",
                "local-ci-bundle",
                vec!["num-deploy.json", "num.toml", "modules/"],
            ),
            "container" | "docker" | "oci" => {
                if service.is_none() {
                    warnings.push(
                        "container targets should set [deployment].service for route entrypoint selection"
                            .to_string(),
                    );
                }
                (
                    "container",
                    "external-container-runner",
                    vec!["num-deploy.json", "num.toml", "modules/", "RUNBOOK.md"],
                )
            }
            "kubernetes" | "k8s" => {
                if service.is_none() {
                    warnings.push(
                        "kubernetes targets should set [deployment].service before execution"
                            .to_string(),
                    );
                }
                if region.is_none() {
                    warnings.push(
                        "kubernetes targets should set [deployment].region or cluster context before execution"
                            .to_string(),
                    );
                }
                (
                    "orchestrator",
                    "external-kubernetes-applier",
                    vec!["num-deploy.json", "num.toml", "modules/", "RUNBOOK.md"],
                )
            }
            "cloud" | "aws" | "gcp" | "azure" => {
                if service.is_none() {
                    warnings.push(
                        "cloud targets should set [deployment].service before execution"
                            .to_string(),
                    );
                }
                if region.is_none() {
                    warnings.push(
                        "cloud targets should set [deployment].region before execution".to_string(),
                    );
                }
                (
                    "cloud",
                    "external-cloud-deployer",
                    vec!["num-deploy.json", "num.toml", "modules/", "RUNBOOK.md"],
                )
            }
            _ => {
                warnings.push(format!(
                    "deployment target `{target}` is preserved as a custom target; execution requires a custom runner"
                ));
                (
                    "custom",
                    "external-custom-runner",
                    vec!["num-deploy.json", "num.toml", "modules/", "RUNBOOK.md"],
                )
            }
        };

        Self {
            class: class.to_string(),
            execution: execution.to_string(),
            required_artifacts: required_artifacts.into_iter().map(str::to_string).collect(),
            warnings,
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "class": self.class,
            "execution": self.execution,
            "required_artifacts": self.required_artifacts,
            "warnings": self.warnings,
        })
    }
}

impl DeploymentEnvironment {
    fn from_manifest(manifest: &PackageManifest) -> Self {
        let required = manifest
            .environment
            .required
            .iter()
            .map(|name| EnvironmentVariableDeployment::from_name(name))
            .collect::<Vec<_>>();
        let optional = manifest
            .environment
            .optional
            .iter()
            .map(|name| EnvironmentVariableDeployment::from_name(name))
            .collect::<Vec<_>>();
        let missing_required = required
            .iter()
            .filter(|variable| !variable.present)
            .map(|variable| variable.name.clone())
            .collect::<Vec<_>>();
        let status = if missing_required.is_empty() {
            "ready"
        } else {
            "missing-required"
        };

        Self {
            status: status.to_string(),
            required,
            optional,
            missing_required,
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "status": self.status,
            "required": self.required.iter().map(EnvironmentVariableDeployment::to_json).collect::<Vec<_>>(),
            "optional": self.optional.iter().map(EnvironmentVariableDeployment::to_json).collect::<Vec<_>>(),
            "missing_required": self.missing_required,
        })
    }
}

impl EnvironmentVariableDeployment {
    fn from_name(name: &str) -> Self {
        Self {
            name: name.to_string(),
            present: env::var_os(name).is_some(),
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "name": self.name,
            "present": self.present,
        })
    }
}

impl DeploymentArtifactReport {
    pub fn to_json(&self) -> Value {
        json!({
            "artifact_root": self.artifact_root.display().to_string(),
            "plan_path": self.plan_path.display().to_string(),
            "manifest_path": self.manifest_path.display().to_string(),
            "lock_path": self.lock_path.as_ref().map(|path| path.display().to_string()),
            "modules_dir": self.modules_dir.display().to_string(),
            "runbook_path": self.runbook_path.display().to_string(),
            "metadata_path": self.metadata_path.display().to_string(),
            "files": self.files,
        })
    }

    pub fn render_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "Deployment artifact: {}\n",
            self.artifact_root.display()
        ));
        out.push_str(&format!("Plan: {}\n", self.plan_path.display()));
        out.push_str(&format!("Manifest: {}\n", self.manifest_path.display()));
        if let Some(lock_path) = &self.lock_path {
            out.push_str(&format!("Lockfile: {}\n", lock_path.display()));
        }
        out.push_str(&format!("Modules: {}\n", self.modules_dir.display()));
        out.push_str(&format!("Runbook: {}\n", self.runbook_path.display()));
        out.push_str(&format!("Files: {}\n", self.files.len()));
        out
    }
}

pub fn materialize_deployment_artifact(
    plan: &DeploymentPlan,
    manifest: &PackageManifest,
    source_files: &[SourceFile],
    artifact_root: &Path,
    replace: bool,
) -> Result<DeploymentArtifactReport, String> {
    let source_lock_path = manifest.lock_path();
    let include_lockfile = source_lock_path.is_file();
    if include_lockfile {
        package::validate_lockfile(&source_lock_path)?;
    }

    if artifact_root.exists() {
        if !replace {
            return Err(format!(
                "deployment artifact {} already exists; pass --replace to overwrite it",
                artifact_root.display()
            ));
        }
        fs::remove_dir_all(artifact_root)
            .map_err(|err| format!("failed to replace {}: {err}", artifact_root.display()))?;
    }

    let modules_dir = artifact_root.join("modules");
    fs::create_dir_all(&modules_dir)
        .map_err(|err| format!("failed to create {}: {err}", modules_dir.display()))?;

    let mut files = Vec::new();
    let plan_path = artifact_root.join("num-deploy.json");
    let manifest_path = artifact_root.join("num.toml");
    let lock_path = artifact_root.join("num.lock");
    let metadata_path = artifact_root.join("manifest.json");
    let runbook_path = artifact_root.join("RUNBOOK.md");

    write_text(
        &plan_path,
        &format!(
            "{}\n",
            serde_json::to_string_pretty(&plan.to_json())
                .map_err(|err| format!("failed to render deployment plan JSON: {err}"))?
        ),
        artifact_root,
        &mut files,
    )?;
    copy_file(&manifest.path, &manifest_path, artifact_root, &mut files)?;
    let copied_lock_path = if include_lockfile {
        copy_file(&source_lock_path, &lock_path, artifact_root, &mut files)?;
        Some(lock_path.clone())
    } else {
        None
    };

    let mut module_entries = Vec::new();
    for (index, source_file) in source_files.iter().enumerate() {
        let module_path = modules_dir.join(module_artifact_name(index, &source_file.name));
        write_text(&module_path, &source_file.source, artifact_root, &mut files)?;
        module_entries.push(json!({
            "source": source_file.name,
            "artifact": relative_to(&module_path, artifact_root)?,
        }));
    }

    write_text(
        &metadata_path,
        &format!(
            "{}\n",
            serde_json::to_string_pretty(&json!({
                "package": {
                    "name": plan.package_name,
                    "version": plan.package_version,
                },
                "target": plan.target,
                "service": plan.service,
                "target_profile": plan.target_profile.to_json(),
                "environment": plan.environment.to_json(),
                "modules": module_entries,
                "plan": relative_to(&plan_path, artifact_root)?,
                "manifest": relative_to(&manifest_path, artifact_root)?,
                "lockfile": copied_lock_path
                    .as_ref()
                    .map(|path| relative_to(path, artifact_root))
                    .transpose()?,
            }))
            .map_err(|err| format!("failed to render deployment metadata JSON: {err}"))?
        ),
        artifact_root,
        &mut files,
    )?;
    write_text(
        &runbook_path,
        &render_runbook(plan),
        artifact_root,
        &mut files,
    )?;

    files.sort();
    Ok(DeploymentArtifactReport {
        artifact_root: artifact_root.to_path_buf(),
        plan_path,
        manifest_path,
        lock_path: copied_lock_path,
        modules_dir,
        runbook_path,
        metadata_path,
        files,
    })
}

fn compatibility_from_report(report: &CompatibilityReport) -> CompatibilityDeployment {
    CompatibilityDeployment {
        language_version: report.language_version.clone(),
        current_language_version: report.current_language_version.clone(),
        compatibility: report.compatibility.clone(),
        manifest_schema: report.manifest_schema,
        current_manifest_schema: report.current_manifest_schema,
    }
}

impl ActionDeployment {
    fn to_json(&self) -> Value {
        json!({
            "name": self.name,
            "risk": self.risk,
            "requires": self.requires,
        })
    }
}

impl ServiceDeployment {
    fn to_json(&self) -> Value {
        json!({
            "name": self.name,
            "routes": self.routes,
        })
    }
}

impl ProcessConnectorDeployment {
    fn to_json(&self) -> Value {
        json!({
            "method": self.method,
            "command": self.command,
            "args": self.args,
            "cwd": self.cwd,
            "timeout_ms": self.timeout_ms,
        })
    }
}

impl DependencyDeployment {
    fn to_json(&self) -> Value {
        json!({
            "name": self.name,
            "version": self.version,
            "source": self.source,
        })
    }
}

fn dependency_source_label(source: &DependencySource) -> String {
    source.lock_source()
}

fn normalize_target(target: &str) -> String {
    target.trim().to_ascii_lowercase()
}

fn risk_label(risk: Risk) -> &'static str {
    match risk {
        Risk::Low => "low",
        Risk::Medium => "medium",
        Risk::High => "high",
        Risk::Critical => "critical",
    }
}

fn write_text(
    path: &Path,
    contents: &str,
    artifact_root: &Path,
    files: &mut Vec<String>,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
    }
    fs::write(path, contents)
        .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    files.push(relative_to(path, artifact_root)?);
    Ok(())
}

fn copy_file(
    from: &Path,
    to: &Path,
    artifact_root: &Path,
    files: &mut Vec<String>,
) -> Result<(), String> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
    }
    fs::copy(from, to).map_err(|err| {
        format!(
            "failed to copy {} to {}: {err}",
            from.display(),
            to.display()
        )
    })?;
    files.push(relative_to(to, artifact_root)?);
    Ok(())
}

fn relative_to(path: &Path, root: &Path) -> Result<String, String> {
    path.strip_prefix(root)
        .map_err(|err| format!("failed to relativize {}: {err}", path.display()))
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
}

fn module_artifact_name(index: usize, source_name: &str) -> String {
    let sanitized = source_name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("{index:04}-{sanitized}")
}

fn render_runbook(plan: &DeploymentPlan) -> String {
    let mut out = String::new();
    out.push_str("# num Deployment Runbook\n\n");
    out.push_str(&format!(
        "Package: {} {}\n\n",
        plan.package_name, plan.package_version
    ));
    out.push_str(&format!("Target: `{}`\n\n", plan.target));
    if let Some(service) = &plan.service {
        out.push_str(&format!("Service: `{service}`\n\n"));
    }
    if let Some(region) = &plan.region {
        out.push_str(&format!("Region: `{region}`\n\n"));
    }
    out.push_str("## Target Profile\n\n");
    out.push_str(&format!("Class: `{}`\n\n", plan.target_profile.class));
    out.push_str(&format!(
        "Execution: `{}`\n\n",
        plan.target_profile.execution
    ));
    out.push_str("Required artifacts:\n\n");
    for artifact in &plan.target_profile.required_artifacts {
        out.push_str(&format!("- `{artifact}`\n"));
    }
    out.push('\n');
    if !plan.target_profile.warnings.is_empty() {
        out.push_str("Warnings:\n\n");
        for warning in &plan.target_profile.warnings {
            out.push_str(&format!("- {warning}\n"));
        }
        out.push('\n');
    }
    out.push_str("## Environment\n\n");
    out.push_str(&format!("Status: `{}`\n\n", plan.environment.status));
    if plan.environment.required.is_empty() {
        out.push_str("Required variables: none\n\n");
    } else {
        out.push_str("Required variables:\n\n");
        for variable in &plan.environment.required {
            let status = if variable.present {
                "present"
            } else {
                "missing"
            };
            out.push_str(&format!("- `{}` - {status}\n", variable.name));
        }
        out.push('\n');
    }
    if !plan.environment.optional.is_empty() {
        out.push_str("Optional variables:\n\n");
        for variable in &plan.environment.optional {
            let status = if variable.present {
                "present"
            } else {
                "missing"
            };
            out.push_str(&format!("- `{}` - {status}\n", variable.name));
        }
        out.push('\n');
    }
    out.push_str("## Included Artifacts\n\n");
    out.push_str("- `num-deploy.json` checked deployment plan\n");
    out.push_str("- `num.toml` source package manifest\n");
    out.push_str("- `num.lock` validated package lockfile, when present\n");
    out.push_str("- `modules/` compiled source module snapshot\n");
    out.push_str("- `manifest.json` artifact metadata and module map\n\n");
    out.push_str("## Operations Boundary\n\n");
    out.push_str(
        "This bundle is a reproducible local/CI deployment artifact. Cloud, container, or Kubernetes execution is intentionally handled by a later deployment executor.\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::{build_deployment_plan, materialize_deployment_artifact};
    use crate::package::{write_lockfile, PackageManifest};
    use num_compiler::{compile, SourceFile};
    use std::env;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("num_deploy_{name}_{stamp}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn deployment_plan_collects_runtime_surface() {
        let root = Path::new("/workspace/app");
        let manifest = PackageManifest::parse(
            root,
            &root.join("num.toml"),
            r#"
[project]
name = "billing"
version = "1.2.3"

[runtime]
workflow_store = "file:.num-state"
audit_store = "file:audit/events.jsonl"

[deployment]
target = "container"
service = "BillingApi"
region = "eu-west-1"

[connectors]
"payments.find" = { command = "node", args = "connectors/payments-find.js", cwd = "ops", timeout_ms = "2000" }

[dependencies]
banking = { git = "https://example.com/banking.num.git", version = "1.4.0", rev = "abc123" }
"#,
        );
        let source = r#"
module app.main

permission IssueRefund

connector payments {
    find(id: Text) -> Text
}

action issue_refund(id: Text)
    requires Permission.IssueRefund
    risk high
{
    audit(id)
}

workflow main() {
    audit("main")
}

service BillingApi {
    route POST "/refunds" requires Permission.IssueRefund {
        audit("refund")
    }
}
"#;
        let compilation = compile("main.num", source);

        let plan = build_deployment_plan(&manifest, &compilation.module, 1);

        assert_eq!(plan.package_name, "billing");
        assert_eq!(plan.target, "container");
        assert_eq!(plan.target_profile.class, "container");
        assert_eq!(plan.target_profile.execution, "external-container-runner");
        assert!(plan.target_profile.warnings.is_empty());
        assert_eq!(plan.runtime.workflow_store, "file:.num-state");
        assert_eq!(
            plan.dependencies[0].source,
            "git:https://example.com/banking.num.git#rev:abc123"
        );
        assert_eq!(plan.workflows, vec!["main".to_string()]);
        assert_eq!(plan.actions[0].risk, "high");
        assert_eq!(plan.services[0].routes, vec!["POST /refunds".to_string()]);
        assert_eq!(plan.process_connectors, vec!["payments.find".to_string()]);
        assert_eq!(plan.process_connector_bindings[0].timeout_ms, Some(2000));
        assert_eq!(
            plan.to_json()["compatibility"]["language"]["version"],
            "0.1.0"
        );
        assert_eq!(plan.to_json()["compatibility"]["manifest"]["schema"], 1);
        assert_eq!(plan.to_json()["deployment"]["service"], "BillingApi");
        assert_eq!(
            plan.to_json()["deployment"]["profile"]["class"],
            "container"
        );
        assert_eq!(
            plan.to_json()["process_connector_bindings"][0]["timeout_ms"],
            2000
        );
        assert!(plan
            .render_text()
            .contains("Deployment plan: billing 1.2.3"));
    }

    #[test]
    fn deployment_plan_warns_for_incomplete_cloud_target_profile() {
        let root = Path::new("/workspace/app");
        let manifest = PackageManifest::parse(
            root,
            &root.join("num.toml"),
            r#"
[project]
name = "billing"
version = "1.2.3"

[deployment]
target = "cloud"
"#,
        );
        let compilation = compile("main.num", "module app.main\n\nworkflow main() {}\n");

        let plan = build_deployment_plan(&manifest, &compilation.module, 1);

        assert_eq!(plan.target_profile.class, "cloud");
        assert_eq!(plan.target_profile.execution, "external-cloud-deployer");
        assert!(plan
            .target_profile
            .warnings
            .iter()
            .any(|warning| warning.contains("[deployment].service")));
        assert!(plan
            .target_profile
            .warnings
            .iter()
            .any(|warning| warning.contains("[deployment].region")));
        assert!(plan.render_text().contains("Deployment warnings"));
    }

    #[test]
    fn deployment_plan_reports_environment_validation() {
        env::set_var("NUM_TEST_DEPLOY_PRESENT", "set");
        env::remove_var("NUM_TEST_DEPLOY_MISSING");
        env::remove_var("NUM_TEST_DEPLOY_OPTIONAL");
        let root = Path::new("/workspace/app");
        let manifest = PackageManifest::parse(
            root,
            &root.join("num.toml"),
            r#"
[project]
name = "billing"
version = "1.2.3"

[environment]
required = ["NUM_TEST_DEPLOY_PRESENT", "NUM_TEST_DEPLOY_MISSING"]
optional = ["NUM_TEST_DEPLOY_OPTIONAL"]
"#,
        );
        let compilation = compile("main.num", "module app.main\n\nworkflow main() {}\n");

        let plan = build_deployment_plan(&manifest, &compilation.module, 1);

        assert_eq!(plan.environment.status, "missing-required");
        assert_eq!(
            plan.environment.missing_required,
            vec!["NUM_TEST_DEPLOY_MISSING".to_string()]
        );
        assert_eq!(plan.environment.required.len(), 2);
        assert_eq!(plan.environment.optional.len(), 1);
        assert_eq!(plan.to_json()["environment"]["status"], "missing-required");
        assert_eq!(
            plan.to_json()["environment"]["missing_required"][0],
            "NUM_TEST_DEPLOY_MISSING"
        );
        assert!(plan
            .render_text()
            .contains("missing required env `NUM_TEST_DEPLOY_MISSING`"));

        env::remove_var("NUM_TEST_DEPLOY_PRESENT");
    }

    #[test]
    fn materializes_deployment_artifact_bundle() {
        let root = temp_dir("bundle_project");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("num.toml"),
            r#"[language]
version = "0.1.0"
compatibility = "minor"
manifest_schema = 1

[project]
name = "billing"
version = "1.2.3"
source = "src"
entry = "src/main.num"

[deployment]
target = "container"
artifact = "dist/num-deploy.json"
"#,
        )
        .unwrap();
        let manifest = PackageManifest::discover(&root).unwrap().unwrap();
        let source = SourceFile {
            name: root.join("src/main.num").display().to_string(),
            source: "module app.main\n\nworkflow main() {\n    audit(\"main\")\n}\n".to_string(),
        };
        let compilation = compile(&source.name, &source.source);
        let plan = build_deployment_plan(&manifest, &compilation.module, 1);
        let artifact_root = root.join("dist/bundle");

        let report = materialize_deployment_artifact(
            &plan,
            &manifest,
            std::slice::from_ref(&source),
            &artifact_root,
            false,
        )
        .unwrap();

        assert!(report.plan_path.is_file());
        assert!(report.manifest_path.is_file());
        assert!(report.runbook_path.is_file());
        assert!(report.metadata_path.is_file());
        assert_eq!(report.files.len(), 5);
        assert!(artifact_root
            .join("modules")
            .read_dir()
            .unwrap()
            .next()
            .is_some());
        assert!(fs::read_to_string(&report.runbook_path)
            .unwrap()
            .contains("Package: billing 1.2.3"));
        assert!(fs::read_to_string(&report.runbook_path)
            .unwrap()
            .contains("Target Profile"));
        assert!(fs::read_to_string(&report.runbook_path)
            .unwrap()
            .contains("## Environment"));
        assert!(fs::read_to_string(&report.metadata_path)
            .unwrap()
            .contains("\"target_profile\""));
        assert!(fs::read_to_string(&report.metadata_path)
            .unwrap()
            .contains("\"environment\""));
        assert!(materialize_deployment_artifact(
            &plan,
            &manifest,
            std::slice::from_ref(&source),
            &artifact_root,
            false,
        )
        .unwrap_err()
        .contains("already exists"));

        let replaced =
            materialize_deployment_artifact(&plan, &manifest, &[source], &artifact_root, true)
                .unwrap();
        assert_eq!(replaced.files.len(), 5);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn materialized_artifact_includes_valid_lockfile_when_present() {
        let root = temp_dir("bundle_with_lock_project");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("num.toml"),
            r#"[language]
version = "0.1.0"
compatibility = "minor"
manifest_schema = 1

[project]
name = "billing"
version = "1.2.3"
source = "src"
entry = "src/main.num"
"#,
        )
        .unwrap();
        write_lockfile(&root).unwrap();
        let manifest = PackageManifest::discover(&root).unwrap().unwrap();
        let source = SourceFile {
            name: root.join("src/main.num").display().to_string(),
            source: "module app.main\n\nworkflow main() {\n}\n".to_string(),
        };
        let compilation = compile(&source.name, &source.source);
        let plan = build_deployment_plan(&manifest, &compilation.module, 1);
        let artifact_root = root.join("dist/bundle");

        let report = materialize_deployment_artifact(
            &plan,
            &manifest,
            std::slice::from_ref(&source),
            &artifact_root,
            false,
        )
        .unwrap();

        let lock_path = report.lock_path.as_ref().unwrap();
        assert!(lock_path.is_file());
        assert_eq!(lock_path, &artifact_root.join("num.lock"));
        assert_eq!(
            report.to_json()["lock_path"],
            artifact_root.join("num.lock").display().to_string()
        );
        assert_eq!(report.files.len(), 6);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(
                &fs::read_to_string(&report.metadata_path).unwrap()
            )
            .unwrap()["lockfile"],
            "num.lock"
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn materialization_rejects_unsupported_lockfile_schema() {
        let root = temp_dir("bundle_bad_lock_project");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("num.toml"),
            r#"[language]
version = "0.1.0"
compatibility = "minor"
manifest_schema = 1

[project]
name = "billing"
version = "1.2.3"
source = "src"
entry = "src/main.num"
"#,
        )
        .unwrap();
        fs::write(root.join("num.lock"), "version = 2\n").unwrap();
        let manifest = PackageManifest::discover(&root).unwrap().unwrap();
        let source = SourceFile {
            name: root.join("src/main.num").display().to_string(),
            source: "module app.main\n\nworkflow main() {\n}\n".to_string(),
        };
        let compilation = compile(&source.name, &source.source);
        let plan = build_deployment_plan(&manifest, &compilation.module, 1);
        let artifact_root = root.join("dist/bundle");

        let err = materialize_deployment_artifact(
            &plan,
            &manifest,
            std::slice::from_ref(&source),
            &artifact_root,
            false,
        )
        .unwrap_err();

        assert!(err.contains("requires lockfile version 2"));
        assert!(!artifact_root.exists());
        fs::remove_dir_all(root).unwrap();
    }
}
