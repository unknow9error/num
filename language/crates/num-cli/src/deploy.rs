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
    pub image_publish: ImagePublishDeployment,
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
    pub validation: DeploymentTargetValidation,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DeploymentTargetValidation {
    pub status: String,
    pub required: Vec<DeploymentTargetField>,
    pub recommended: Vec<DeploymentTargetField>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub boundary: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DeploymentTargetField {
    pub name: String,
    pub present: bool,
    pub description: String,
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
pub struct ImagePublishDeployment {
    pub enabled: bool,
    pub registry: Option<String>,
    pub image: String,
    pub tag_strategy: String,
    pub tag: String,
    pub reference: Option<String>,
    pub credentials_ref: Option<String>,
    pub validation: ImagePublishValidation,
}

#[derive(Debug, Clone)]
pub struct ImagePublishValidation {
    pub status: String,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub boundary: String,
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
    pub runtime_artifacts: Vec<String>,
    pub files: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct KubernetesDryRun {
    pub manifest: String,
    pub validation: KubernetesDryRunValidation,
}

#[derive(Debug, Clone)]
pub struct KubernetesDryRunValidation {
    pub status: String,
    pub namespace: String,
    pub image: String,
    pub ports: Vec<u16>,
    pub secret_references: Vec<String>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
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

    let image_publish = ImagePublishDeployment::from_manifest(manifest);

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
        image_publish,
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

pub fn build_kubernetes_dry_run(plan: &DeploymentPlan) -> KubernetesDryRun {
    KubernetesDryRun {
        manifest: render_kubernetes_manifest(plan),
        validation: KubernetesDryRunValidation::for_plan(plan),
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
            "image_publish": self.image_publish.to_json(),
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
        out.push_str(&format!(
            "Target validation: status={}\n",
            self.target_profile.validation.status
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
        if self.image_publish.enabled {
            out.push_str(&format!(
                "Image publish: status={}, reference={}\n",
                self.image_publish.validation.status,
                self.image_publish
                    .reference
                    .as_deref()
                    .unwrap_or("<unresolved>")
            ));
        }
        if !self.target_profile.validation.errors.is_empty() {
            out.push_str("Deployment validation errors:\n");
            for error in &self.target_profile.validation.errors {
                out.push_str(&format!("  - {error}\n"));
            }
        }
        if self.image_publish.enabled && !self.image_publish.validation.errors.is_empty() {
            out.push_str("Image publish validation errors:\n");
            for error in &self.image_publish.validation.errors {
                out.push_str(&format!("  - {error}\n"));
            }
        }
        if !self.target_profile.validation.warnings.is_empty()
            || self.target_profile.validation.boundary.is_some()
        {
            out.push_str("Deployment warnings:\n");
            for warning in &self.target_profile.validation.warnings {
                out.push_str(&format!("  - {warning}\n"));
            }
            if let Some(boundary) = &self.target_profile.validation.boundary {
                out.push_str(&format!("  - {boundary}\n"));
            }
        }
        if self.image_publish.enabled
            && (!self.image_publish.validation.warnings.is_empty()
                || !self.image_publish.validation.boundary.is_empty())
        {
            out.push_str("Image publish warnings:\n");
            for warning in &self.image_publish.validation.warnings {
                out.push_str(&format!("  - {warning}\n"));
            }
            out.push_str(&format!("  - {}\n", self.image_publish.validation.boundary));
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

        let (class, execution, required_artifacts, validation) = match normalize_target(target)
            .as_str()
        {
            "local" => (
                "local",
                "local-ci-bundle",
                vec!["num-deploy.json", "num.toml", "modules/"],
                DeploymentTargetValidation::new(vec![], vec![], None),
            ),
            "container" | "docker" | "oci" => {
                let validation = DeploymentTargetValidation::new(
                    vec![],
                    vec![DeploymentTargetField::new(
                        "[deployment].service",
                        service.is_some(),
                        "service route entrypoint for container serve commands",
                    )],
                    None,
                );
                (
                    "container",
                    "external-container-runner",
                    vec![
                        "num-deploy.json",
                        "num.toml",
                        "modules/",
                        "src/",
                        "RUNBOOK.md",
                        "deploy/Dockerfile",
                        "deploy/compose.yaml",
                    ],
                    validation,
                )
            }
            "kubernetes" | "k8s" => {
                let validation = DeploymentTargetValidation::new(
                    vec![
                        DeploymentTargetField::new(
                            "[deployment].service",
                            service.is_some(),
                            "service route entrypoint for generated Kubernetes workload args",
                        ),
                        DeploymentTargetField::new(
                            "[deployment].region",
                            region.is_some(),
                            "cluster context or region label for Kubernetes handoff",
                        ),
                    ],
                    vec![],
                    None,
                );
                (
                    "orchestrator",
                    "external-kubernetes-applier",
                    vec![
                        "num-deploy.json",
                        "num.toml",
                        "modules/",
                        "src/",
                        "RUNBOOK.md",
                        "deploy/Dockerfile",
                        "deploy/kubernetes.yaml",
                    ],
                    validation,
                )
            }
            "cloud" | "aws" | "gcp" | "azure" => {
                let validation = DeploymentTargetValidation::new(
                    vec![
                        DeploymentTargetField::new(
                            "[deployment].service",
                            service.is_some(),
                            "service route entrypoint for the external cloud deployer",
                        ),
                        DeploymentTargetField::new(
                            "[deployment].region",
                            region.is_some(),
                            "cloud region for the external cloud deployer",
                        ),
                    ],
                    vec![],
                    None,
                );
                (
                    "cloud",
                    "external-cloud-deployer",
                    vec!["num-deploy.json", "num.toml", "modules/", "RUNBOOK.md"],
                    validation,
                )
            }
            "bare-metal" | "baremetal" | "systemd" | "host" => {
                let validation = DeploymentTargetValidation::new(
                    vec![DeploymentTargetField::new(
                        "[deployment].service",
                        service.is_some(),
                        "service route entrypoint for the generated systemd unit",
                    )],
                    vec![DeploymentTargetField::new(
                        "[deployment].region",
                        region.is_some(),
                        "host group, datacenter, or inventory label for operator handoff",
                    )],
                    Some(
                        "bare-metal bundles are runbook artifacts only; SSH provisioning, package installation, and systemctl execution stay external"
                            .to_string(),
                    ),
                );
                (
                    "host",
                    "external-systemd-operator",
                    vec![
                        "num-deploy.json",
                        "num.toml",
                        "modules/",
                        "src/",
                        "RUNBOOK.md",
                        "deploy/num.service",
                        "deploy/num.env",
                    ],
                    validation,
                )
            }
            _ => {
                let validation = DeploymentTargetValidation::new(
                    vec![],
                    vec![],
                    Some(format!(
                        "deployment target `{target}` is preserved as a custom target; execution requires a custom runner"
                    )),
                );
                (
                    "custom",
                    "external-custom-runner",
                    vec!["num-deploy.json", "num.toml", "modules/", "RUNBOOK.md"],
                    validation,
                )
            }
        };
        let mut required_artifacts = required_artifacts
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        if deployment_image_publish_enabled(manifest)
            && matches!(class, "container" | "orchestrator")
        {
            required_artifacts.push("deploy/image-publish.json".to_string());
        }
        if class != "local" {
            required_artifacts.push("deploy/Jenkinsfile".to_string());
        }
        let mut warnings = validation.messages();
        if class == "host" {
            warnings.extend(bare_metal_external_service_warnings(manifest));
        }

        Self {
            class: class.to_string(),
            execution: execution.to_string(),
            required_artifacts,
            validation,
            warnings,
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "class": self.class,
            "execution": self.execution,
            "required_artifacts": self.required_artifacts,
            "validation": self.validation.to_json(),
            "warnings": self.warnings,
        })
    }
}

impl DeploymentTargetValidation {
    fn new(
        required: Vec<DeploymentTargetField>,
        recommended: Vec<DeploymentTargetField>,
        boundary: Option<String>,
    ) -> Self {
        let errors = required
            .iter()
            .filter(|field| !field.present)
            .map(|field| format!("{} is required: {}", field.name, field.description))
            .collect::<Vec<_>>();
        let warnings = recommended
            .iter()
            .filter(|field| !field.present)
            .map(|field| format!("{} is recommended: {}", field.name, field.description))
            .collect::<Vec<_>>();
        let status = if !errors.is_empty() {
            "missing-required"
        } else if !warnings.is_empty() {
            "missing-recommended"
        } else if boundary.is_some() {
            "custom-boundary"
        } else {
            "ready"
        };

        Self {
            status: status.to_string(),
            required,
            recommended,
            errors,
            warnings,
            boundary,
        }
    }

    fn messages(&self) -> Vec<String> {
        self.errors
            .iter()
            .chain(self.warnings.iter())
            .cloned()
            .chain(self.boundary.iter().cloned())
            .collect()
    }

    fn to_json(&self) -> Value {
        json!({
            "status": self.status,
            "required": self.required.iter().map(DeploymentTargetField::to_json).collect::<Vec<_>>(),
            "recommended": self.recommended.iter().map(DeploymentTargetField::to_json).collect::<Vec<_>>(),
            "errors": self.errors,
            "warnings": self.warnings,
            "boundary": self.boundary,
        })
    }
}

impl DeploymentTargetField {
    fn new(name: &str, present: bool, description: &str) -> Self {
        Self {
            name: name.to_string(),
            present,
            description: description.to_string(),
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "name": self.name,
            "present": self.present,
            "description": self.description,
        })
    }
}

fn bare_metal_external_service_warnings(manifest: &PackageManifest) -> Vec<String> {
    let mut warnings = Vec::new();
    if !manifest.connectors.is_empty() {
        let methods = manifest
            .connectors
            .iter()
            .map(|connector| connector.method.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        warnings.push(format!(
            "bare-metal bundle includes process connectors [{methods}]; install their binaries, working directories, network access, and credentials on each host"
        ));
    }
    if !manifest.runtime.workflow_store.starts_with("file:")
        && manifest.runtime.workflow_store != "memory"
    {
        warnings.push(format!(
            "bare-metal workflow_store `{}` may require an external service; document and provision it outside this bundle",
            sanitize_warning_value(&manifest.runtime.workflow_store)
        ));
    }
    if !manifest.runtime.audit_store.starts_with("file:")
        && manifest.runtime.audit_store != "stdout"
    {
        warnings.push(format!(
            "bare-metal audit_store `{}` may require an external service; document and provision it outside this bundle",
            sanitize_warning_value(&manifest.runtime.audit_store)
        ));
    }
    let secret_like_vars = manifest
        .environment
        .required
        .iter()
        .chain(manifest.environment.optional.iter())
        .filter(|name| is_secret_service_env_name(name))
        .cloned()
        .collect::<Vec<_>>();
    if !secret_like_vars.is_empty() {
        warnings.push(format!(
            "bare-metal environment variables [{}] look secret-store or key-provider related; fill deploy/num.env on the host without committing values",
            secret_like_vars.join(", ")
        ));
    }
    warnings
}

fn sanitize_warning_value(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            '\r' | '\n' | '\t' => ' ',
            ch if ch.is_control() => ' ',
            ch => ch,
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_secret_service_env_name(name: &str) -> bool {
    let normalized = name.to_ascii_uppercase();
    ["SECRET", "VAULT", "KMS", "TOKEN", "KEY"]
        .iter()
        .any(|needle| normalized.contains(needle))
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
            "runtime_artifacts": self.runtime_artifacts,
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
        if !self.runtime_artifacts.is_empty() {
            out.push_str(&format!(
                "Runtime artifacts: {}\n",
                self.runtime_artifacts.join(", ")
            ));
        }
        out.push_str(&format!("Files: {}\n", self.files.len()));
        out
    }
}

impl KubernetesDryRun {
    pub fn to_json(&self) -> Value {
        json!({
            "manifest": self.manifest,
            "validation": self.validation.to_json(),
        })
    }

    pub fn render_text(&self) -> String {
        let mut out = String::new();
        out.push_str("Kubernetes dry-run handoff\n");
        out.push_str(&format!("Validation: {}\n", self.validation.status));
        out.push_str(&format!("Namespace: {}\n", self.validation.namespace));
        out.push_str(&format!("Image: {}\n", self.validation.image));
        out.push_str(&format!(
            "Ports: {}\n",
            self.validation
                .ports
                .iter()
                .map(u16::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ));
        if !self.validation.secret_references.is_empty() {
            out.push_str(&format!(
                "Secret references: {}\n",
                self.validation.secret_references.join(", ")
            ));
        }
        if !self.validation.errors.is_empty() {
            out.push_str("Validation errors:\n");
            for error in &self.validation.errors {
                out.push_str(&format!("  - {error}\n"));
            }
        }
        if !self.validation.warnings.is_empty() {
            out.push_str("Validation warnings:\n");
            for warning in &self.validation.warnings {
                out.push_str(&format!("  - {warning}\n"));
            }
        }
        out.push_str("---\n");
        out.push_str(&self.manifest);
        out
    }
}

impl KubernetesDryRunValidation {
    fn for_plan(plan: &DeploymentPlan) -> Self {
        let namespace = kubernetes_namespace(plan);
        let image = runtime_image_reference(plan);
        let ports = vec![4000];
        let secret_references = plan
            .environment
            .required
            .iter()
            .chain(plan.environment.optional.iter())
            .filter(|variable| is_secret_service_env_name(&variable.name))
            .map(|variable| variable.name.clone())
            .collect::<Vec<_>>();
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        if plan.target_profile.class != "orchestrator" {
            errors.push(format!(
                "kubernetes dry-run requires [deployment].target = \"kubernetes\" or \"k8s\", got `{}`",
                sanitize_warning_value(&plan.target)
            ));
        }
        if !is_kubernetes_dns_label(&namespace) {
            errors.push(format!(
                "namespace `{namespace}` is not a valid Kubernetes DNS label"
            ));
        }
        if image.trim().is_empty() || image.chars().any(char::is_whitespace) {
            errors.push(format!("image `{image}` is empty or contains whitespace"));
        }
        if ports.iter().any(|port| *port == 0) {
            errors.push("container ports must be between 1 and 65535".to_string());
        }
        if let Some(raw_namespace) = plan.region.as_deref() {
            if raw_namespace != namespace {
                warnings.push(format!(
                    "[deployment].region `{}` was normalized to Kubernetes namespace `{namespace}`",
                    sanitize_warning_value(raw_namespace)
                ));
            }
        }
        if !secret_references.is_empty() {
            warnings.push(format!(
                "environment variables [{}] look secret-like; create Kubernetes Secret mappings before real apply",
                secret_references.join(", ")
            ));
        }

        let status = if errors.is_empty() {
            "ready"
        } else {
            "invalid"
        };

        Self {
            status: status.to_string(),
            namespace,
            image,
            ports,
            secret_references,
            errors,
            warnings,
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "status": self.status,
            "namespace": self.namespace,
            "image": self.image,
            "ports": self.ports,
            "secret_references": self.secret_references,
            "errors": self.errors,
            "warnings": self.warnings,
        })
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
    let mut runtime_artifacts = Vec::new();

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
    let mut source_entries = Vec::new();
    for (index, source_file) in source_files.iter().enumerate() {
        let module_path = modules_dir.join(module_artifact_name(index, &source_file.name));
        write_text(&module_path, &source_file.source, artifact_root, &mut files)?;
        module_entries.push(json!({
            "source": source_file.name,
            "artifact": relative_to(&module_path, artifact_root)?,
        }));
        if let Some(source_path) = source_artifact_path(manifest, source_file, artifact_root) {
            write_text(&source_path, &source_file.source, artifact_root, &mut files)?;
            source_entries.push(json!({
                "source": source_file.name,
                "artifact": relative_to(&source_path, artifact_root)?,
            }));
        }
    }

    runtime_artifacts.extend(materialize_runtime_artifacts(
        plan,
        artifact_root,
        &mut files,
    )?);

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
                "image_publish": plan.image_publish.to_json(),
                "environment": plan.environment.to_json(),
                "modules": module_entries,
                "sources": source_entries,
                "runtime_artifacts": runtime_artifacts,
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
        runtime_artifacts,
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

impl ImagePublishDeployment {
    fn from_manifest(manifest: &PackageManifest) -> Self {
        let enabled = deployment_image_publish_enabled(manifest);
        let registry = manifest
            .deployment
            .registry
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.trim_end_matches('/').to_string());
        let image = manifest
            .deployment
            .image
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| default_runtime_image_name(&manifest.project.name));
        let tag_strategy = manifest
            .deployment
            .tag_strategy
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("version")
            .to_ascii_lowercase();
        let tag = match tag_strategy.as_str() {
            "version" => manifest.project.version.clone(),
            "latest" => "latest".to_string(),
            _ => manifest.project.version.clone(),
        };
        let credentials_ref = manifest
            .deployment
            .credentials_ref
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let reference = if enabled {
            registry
                .as_ref()
                .filter(|_| !image.is_empty())
                .map(|registry| format!("{registry}/{image}:{tag}"))
        } else {
            None
        };
        let validation = ImagePublishValidation::for_metadata(
            enabled,
            registry.as_deref(),
            &image,
            &tag_strategy,
            credentials_ref.as_deref(),
        );

        Self {
            enabled,
            registry,
            image,
            tag_strategy,
            tag,
            reference,
            credentials_ref,
            validation,
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "enabled": self.enabled,
            "registry": self.registry,
            "image": self.image,
            "tag_strategy": self.tag_strategy,
            "tag": self.tag,
            "reference": self.reference,
            "credentials_ref": self.credentials_ref,
            "validation": self.validation.to_json(),
        })
    }
}

impl ImagePublishValidation {
    fn for_metadata(
        enabled: bool,
        registry: Option<&str>,
        image: &str,
        tag_strategy: &str,
        credentials_ref: Option<&str>,
    ) -> Self {
        let boundary = "registry credentials stay behind the secret-store boundary; num deploy records only credentials_ref and never performs docker login, build, tag, or push"
            .to_string();
        if !enabled {
            return Self {
                status: "not-configured".to_string(),
                errors: Vec::new(),
                warnings: Vec::new(),
                boundary,
            };
        }

        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        if registry.is_none() {
            errors.push("[deployment].registry is required for image publishing".to_string());
        }
        if image.trim().is_empty() || image.chars().any(char::is_whitespace) {
            errors.push(
                "[deployment].image must be a non-empty image name without whitespace".to_string(),
            );
        }
        if !matches!(tag_strategy, "version" | "latest") {
            errors.push(format!(
                "[deployment].tag_strategy `{}` is unsupported; use `version` or `latest`",
                sanitize_warning_value(tag_strategy)
            ));
        }
        if credentials_ref.is_none() {
            errors.push(
                "[deployment].credentials_ref is required so registry credentials remain outside plain config"
                    .to_string(),
            );
        }
        if matches!(tag_strategy, "latest") {
            warnings.push(
                "[deployment].tag_strategy = \"latest\" is mutable; prefer `version` for audited releases"
                    .to_string(),
            );
        }

        Self {
            status: if errors.is_empty() {
                "ready".to_string()
            } else {
                "invalid".to_string()
            },
            errors,
            warnings,
            boundary,
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "status": self.status,
            "errors": self.errors,
            "warnings": self.warnings,
            "boundary": self.boundary,
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

fn source_artifact_path(
    manifest: &PackageManifest,
    source_file: &SourceFile,
    artifact_root: &Path,
) -> Option<PathBuf> {
    let source_path = Path::new(&source_file.name);
    let relative = source_path.strip_prefix(&manifest.root).ok()?;
    Some(artifact_root.join(relative))
}

fn materialize_runtime_artifacts(
    plan: &DeploymentPlan,
    artifact_root: &Path,
    files: &mut Vec<String>,
) -> Result<Vec<String>, String> {
    let mut artifacts = Vec::new();
    match plan.target_profile.class.as_str() {
        "container" => {
            let dockerfile = artifact_root.join("deploy/Dockerfile");
            write_text(&dockerfile, &render_dockerfile(plan), artifact_root, files)?;
            artifacts.push(relative_to(&dockerfile, artifact_root)?);

            if let Some(image_publish) =
                materialize_image_publish_artifact(plan, artifact_root, files)?
            {
                artifacts.push(image_publish);
            }

            let compose = artifact_root.join("deploy/compose.yaml");
            write_text(&compose, &render_compose(plan), artifact_root, files)?;
            artifacts.push(relative_to(&compose, artifact_root)?);
        }
        "orchestrator" => {
            let dockerfile = artifact_root.join("deploy/Dockerfile");
            write_text(&dockerfile, &render_dockerfile(plan), artifact_root, files)?;
            artifacts.push(relative_to(&dockerfile, artifact_root)?);

            if let Some(image_publish) =
                materialize_image_publish_artifact(plan, artifact_root, files)?
            {
                artifacts.push(image_publish);
            }

            let kubernetes = artifact_root.join("deploy/kubernetes.yaml");
            write_text(
                &kubernetes,
                &render_kubernetes_manifest(plan),
                artifact_root,
                files,
            )?;
            artifacts.push(relative_to(&kubernetes, artifact_root)?);
        }
        "host" => {
            let service = artifact_root.join("deploy/num.service");
            write_text(
                &service,
                &render_systemd_service(plan),
                artifact_root,
                files,
            )?;
            artifacts.push(relative_to(&service, artifact_root)?);

            let environment = artifact_root.join("deploy/num.env");
            write_text(
                &environment,
                &render_systemd_environment(plan),
                artifact_root,
                files,
            )?;
            artifacts.push(relative_to(&environment, artifact_root)?);
        }
        _ => {}
    }
    if plan.target_profile.class != "local" {
        let jenkinsfile = artifact_root.join("deploy/Jenkinsfile");
        write_text(
            &jenkinsfile,
            &render_jenkinsfile(plan),
            artifact_root,
            files,
        )?;
        artifacts.push(relative_to(&jenkinsfile, artifact_root)?);
    }
    Ok(artifacts)
}

fn materialize_image_publish_artifact(
    plan: &DeploymentPlan,
    artifact_root: &Path,
    files: &mut Vec<String>,
) -> Result<Option<String>, String> {
    if !plan.image_publish.enabled {
        return Ok(None);
    }
    let path = artifact_root.join("deploy/image-publish.json");
    write_text(
        &path,
        &render_image_publish_artifact(plan)?,
        artifact_root,
        files,
    )?;
    relative_to(&path, artifact_root).map(Some)
}

fn render_image_publish_artifact(plan: &DeploymentPlan) -> Result<String, String> {
    let reference = plan.image_publish.reference.as_deref();
    let commands = reference
        .map(|reference| {
            vec![
                format!("docker build -t {reference} -f deploy/Dockerfile ."),
                format!(
                    "docker login {} using credentials_ref {}",
                    plan.image_publish
                        .registry
                        .as_deref()
                        .unwrap_or("<registry>"),
                    plan.image_publish
                        .credentials_ref
                        .as_deref()
                        .unwrap_or("<credentials_ref>")
                ),
                format!("docker push {reference}"),
            ]
        })
        .unwrap_or_default();
    serde_json::to_string_pretty(&json!({
        "kind": "num.deploy.image_publish.v1",
        "package": {
            "name": plan.package_name,
            "version": plan.package_version,
        },
        "target": plan.target,
        "registry": plan.image_publish.registry,
        "image": plan.image_publish.image,
        "tag_strategy": plan.image_publish.tag_strategy,
        "tag": plan.image_publish.tag,
        "reference": plan.image_publish.reference,
        "credentials_ref": plan.image_publish.credentials_ref,
        "validation": plan.image_publish.validation.to_json(),
        "commands": commands,
        "execution": "handoff-only",
    }))
    .map(|json| format!("{json}\n"))
    .map_err(|err| format!("failed to render image publish artifact JSON: {err}"))
}

fn render_dockerfile(plan: &DeploymentPlan) -> String {
    let command = runtime_command(plan);
    format!(
        "# Generated by num deploy. Build from the deployment artifact root.\n\
ARG NUM_IMAGE=ghcr.io/unknow9error/num:{}\n\
FROM $NUM_IMAGE\n\
WORKDIR /app\n\
COPY . /app\n\
ENV NUM_DEPLOY_PLAN=/app/num-deploy.json\n\
EXPOSE 4000\n\
CMD [{}]\n",
        env!("CARGO_PKG_VERSION"),
        json_string_array(&command)
    )
}

fn render_compose(plan: &DeploymentPlan) -> String {
    let image = runtime_image_reference(plan);
    format!(
        "services:\n  num:\n    build:\n      context: ..\n      dockerfile: deploy/Dockerfile\n    image: {image}\n    ports:\n      - \"4000:4000\"\n    environment:\n      NUM_DEPLOY_PLAN: /app/num-deploy.json\n"
    )
}

fn render_kubernetes_manifest(plan: &DeploymentPlan) -> String {
    let name = kubernetes_name(&plan.package_name);
    let namespace = kubernetes_namespace(plan);
    let image = runtime_image_reference(plan);
    let args = runtime_command(plan)
        .into_iter()
        .map(|arg| format!("            - {}", yaml_string(&arg)))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: {name}\n  namespace: {namespace}\n  labels:\n    app.kubernetes.io/name: {name}\nspec:\n  replicas: 1\n  selector:\n    matchLabels:\n      app.kubernetes.io/name: {name}\n  template:\n    metadata:\n      labels:\n        app.kubernetes.io/name: {name}\n    spec:\n      containers:\n        - name: num\n          image: {image}\n          args:\n{args}\n          ports:\n            - containerPort: 4000\n          env:\n            - name: NUM_DEPLOY_PLAN\n              value: /app/num-deploy.json\n---\napiVersion: v1\nkind: Service\nmetadata:\n  name: {name}\n  namespace: {namespace}\nspec:\n  selector:\n    app.kubernetes.io/name: {name}\n  ports:\n    - name: http\n      port: 4000\n      targetPort: 4000\n"
    )
}

fn render_systemd_service(plan: &DeploymentPlan) -> String {
    let command = runtime_command(plan).join(" ");
    format!(
        "# Generated by num deploy. Review paths, user, and environment before installing.\n\
[Unit]\n\
Description=Num service for {package}\n\
After=network-online.target\n\
Wants=network-online.target\n\
\n\
[Service]\n\
Type=simple\n\
WorkingDirectory=/opt/num/{package}\n\
EnvironmentFile=/etc/num/{package}.env\n\
Environment=NUM_DEPLOY_PLAN=/opt/num/{package}/num-deploy.json\n\
ExecStart=/usr/bin/env {command}\n\
Restart=on-failure\n\
RestartSec=5s\n\
NoNewPrivileges=true\n\
PrivateTmp=true\n\
\n\
[Install]\n\
WantedBy=multi-user.target\n",
        package = systemd_identifier(&plan.package_name),
        command = command
    )
}

fn render_systemd_environment(plan: &DeploymentPlan) -> String {
    let mut out = String::new();
    out.push_str(
        "# Generated by num deploy. Fill values on the target host; do not commit secrets.\n",
    );
    out.push_str(&format!(
        "NUM_DEPLOY_PLAN=/opt/num/{}/num-deploy.json\n",
        systemd_identifier(&plan.package_name)
    ));
    out.push_str(&format!(
        "NUM_WORKFLOW_STORE={}\n",
        shell_env_value(&plan.runtime.workflow_store)
    ));
    out.push_str(&format!(
        "NUM_AUDIT_STORE={}\n",
        shell_env_value(&plan.runtime.audit_store)
    ));
    for variable in &plan.environment.required {
        out.push_str(&format!("{}=\n", variable.name));
    }
    for variable in &plan.environment.optional {
        out.push_str(&format!("# {}=\n", variable.name));
    }
    out
}

fn render_jenkinsfile(plan: &DeploymentPlan) -> String {
    format!(
        concat!(
            "// Generated by num deploy for {package} {version}.\n",
            "// Required Jenkins runtime: num CLI on PATH and repository checkout access.\n",
            "// Required parameters: NUM_PROJECT_DIR and NUM_DEPLOY_DIR.\n",
            "// Secret values stay in Jenkins credentials or an external secret store; if\n",
            "// [deployment].credentials_ref is set, map it to NUM_REGISTRY_CREDENTIALS_ID\n",
            "// without committing the underlying credential value.\n",
            "pipeline {{\n",
            "  agent any\n",
            "\n",
            "  options {{\n",
            "    timestamps()\n",
            "  }}\n",
            "\n",
            "  parameters {{\n",
            "    string(name: 'NUM_PROJECT_DIR', defaultValue: '.', description: 'Num project directory or entry file to validate and package')\n",
            "    string(name: 'NUM_DEPLOY_DIR', defaultValue: 'dist/num-deploy', description: 'Deployment bundle output directory')\n",
            "    string(name: 'NUM_REGISTRY_CREDENTIALS_ID', defaultValue: '', description: 'Optional Jenkins credentials id matching [deployment].credentials_ref')\n",
            "  }}\n",
            "\n",
            "  stages {{\n",
            "    stage('Checkout') {{\n",
            "      steps {{\n",
            "        checkout scm\n",
            "      }}\n",
            "    }}\n",
            "\n",
            "    stage('Policy gate') {{\n",
            "      steps {{\n",
            "        sh 'num check \"$NUM_PROJECT_DIR\"'\n",
            "        sh 'num test \"$NUM_PROJECT_DIR\"'\n",
            "      }}\n",
            "    }}\n",
            "\n",
            "    stage('Cost gate') {{\n",
            "      steps {{\n",
            "        sh 'num cost-report \"$NUM_PROJECT_DIR\" --json > num-cost-report.json'\n",
            "      }}\n",
            "    }}\n",
            "\n",
            "    stage('Security gate') {{\n",
            "      steps {{\n",
            "        sh 'num lint \"$NUM_PROJECT_DIR\"'\n",
            "      }}\n",
            "    }}\n",
            "\n",
            "    stage('Deploy artifact') {{\n",
            "      steps {{\n",
            "        sh 'num deploy \"$NUM_PROJECT_DIR\" --apply --replace --dir \"$NUM_DEPLOY_DIR\" --json > num-deploy.json'\n",
            "      }}\n",
            "    }}\n",
            "  }}\n",
            "\n",
            "  post {{\n",
            "    always {{\n",
            "      archiveArtifacts artifacts: 'num-cost-report.json,num-deploy.json,dist/num-deploy/**', allowEmptyArchive: true\n",
            "    }}\n",
            "  }}\n",
            "}}\n",
        ),
        package = plan.package_name,
        version = plan.package_version
    )
}

fn runtime_command(plan: &DeploymentPlan) -> Vec<String> {
    if let Some(service) = &plan.service {
        vec![
            "num".to_string(),
            "serve".to_string(),
            ".".to_string(),
            "0.0.0.0:4000".to_string(),
            service.clone(),
        ]
    } else {
        vec![
            "num".to_string(),
            "run".to_string(),
            ".".to_string(),
            "--json".to_string(),
        ]
    }
}

fn systemd_identifier(name: &str) -> String {
    let mut out = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    while out.contains("--") {
        out = out.replace("--", "-");
    }
    out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "num-app".to_string()
    } else {
        out
    }
}

fn shell_env_value(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            '\n' | '\r' | '\0' => '_',
            ch => ch,
        })
        .collect()
}

fn runtime_image_name(plan: &DeploymentPlan) -> String {
    default_runtime_image_name(&plan.package_name)
}

fn default_runtime_image_name(package_name: &str) -> String {
    format!("num-{}", kubernetes_name(package_name))
}

fn runtime_image_reference(plan: &DeploymentPlan) -> String {
    plan.image_publish
        .reference
        .clone()
        .unwrap_or_else(|| format!("{}:local", runtime_image_name(plan)))
}

fn deployment_image_publish_enabled(manifest: &PackageManifest) -> bool {
    manifest.deployment.registry.is_some()
        || manifest.deployment.image.is_some()
        || manifest.deployment.tag_strategy.is_some()
        || manifest.deployment.credentials_ref.is_some()
}

fn kubernetes_namespace(plan: &DeploymentPlan) -> String {
    plan.region
        .as_deref()
        .map(kubernetes_name)
        .filter(|namespace| !namespace.is_empty())
        .unwrap_or_else(|| "default".to_string())
}

fn kubernetes_name(name: &str) -> String {
    let mut out = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    while out.contains("--") {
        out = out.replace("--", "-");
    }
    out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "num-app".to_string()
    } else {
        out
    }
}

fn is_kubernetes_dns_label(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 63
        && value
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
        && value
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit())
        && value
            .chars()
            .last()
            .is_some_and(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit())
}

fn json_string_array(items: &[String]) -> String {
    items
        .iter()
        .map(|item| {
            serde_json::to_string(item)
                .unwrap_or_else(|_| format!("\"{}\"", item.replace('"', "\\\"")))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn yaml_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| format!("\"{}\"", value.replace('"', "\\\"")))
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
    out.push_str(&format!(
        "Validation: `{}`\n\n",
        plan.target_profile.validation.status
    ));
    out.push_str("Required artifacts:\n\n");
    for artifact in &plan.target_profile.required_artifacts {
        out.push_str(&format!("- `{artifact}`\n"));
    }
    out.push('\n');
    if !plan.target_profile.validation.required.is_empty() {
        out.push_str("Required target fields:\n\n");
        for field in &plan.target_profile.validation.required {
            let status = if field.present { "present" } else { "missing" };
            out.push_str(&format!("- `{}` - {status}\n", field.name));
        }
        out.push('\n');
    }
    if !plan.target_profile.validation.recommended.is_empty() {
        out.push_str("Recommended target fields:\n\n");
        for field in &plan.target_profile.validation.recommended {
            let status = if field.present { "present" } else { "missing" };
            out.push_str(&format!("- `{}` - {status}\n", field.name));
        }
        out.push('\n');
    }
    if !plan.target_profile.validation.errors.is_empty() {
        out.push_str("Validation errors:\n\n");
        for error in &plan.target_profile.validation.errors {
            out.push_str(&format!("- {error}\n"));
        }
        out.push('\n');
    }
    if !plan.target_profile.validation.warnings.is_empty()
        || plan.target_profile.validation.boundary.is_some()
    {
        out.push_str("Warnings:\n\n");
        for warning in &plan.target_profile.validation.warnings {
            out.push_str(&format!("- {warning}\n"));
        }
        if let Some(boundary) = &plan.target_profile.validation.boundary {
            out.push_str(&format!("- {boundary}\n"));
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
    out.push_str("- source tree snapshot at the manifest `[project].source` path\n");
    out.push_str("- `manifest.json` artifact metadata and module map\n\n");
    match plan.target_profile.class.as_str() {
        "container" => {
            out.push_str("## Container Runtime\n\n");
            out.push_str(
                "- `deploy/Dockerfile` builds a runnable Num service image from this artifact\n",
            );
            out.push_str("- `deploy/compose.yaml` runs the image locally on port `4000`\n\n");
        }
        "orchestrator" => {
            out.push_str("## Kubernetes Runtime\n\n");
            out.push_str("- `deploy/Dockerfile` builds the image expected by the manifest\n");
            out.push_str(
                "- `deploy/kubernetes.yaml` provides a deployment and service scaffold\n\n",
            );
        }
        "host" => {
            out.push_str("## Bare-Metal Runtime\n\n");
            out.push_str("- `deploy/num.service` is a systemd-style service unit draft\n");
            out.push_str(
                "- `deploy/num.env` is an environment file template for host-local values\n",
            );
            out.push_str("- install the artifact under `/opt/num/<package>` or edit the generated paths before running `systemctl`\n\n");
            out.push_str("Runtime stores:\n\n");
            out.push_str(&format!(
                "- workflow store: `{}`\n",
                plan.runtime.workflow_store
            ));
            out.push_str(&format!(
                "- audit store: `{}`\n\n",
                plan.runtime.audit_store
            ));
            if !plan.process_connector_bindings.is_empty() {
                out.push_str("Process connector host requirements:\n\n");
                for connector in &plan.process_connector_bindings {
                    out.push_str(&format!(
                        "- `{}` uses command `{}`; install command dependencies, cwd, network access, and credentials on each host\n",
                        connector.method, connector.command
                    ));
                }
                out.push('\n');
            }
        }
        _ => {}
    }
    if plan.image_publish.enabled {
        out.push_str("## Container Image Publish Handoff\n\n");
        out.push_str("- `deploy/image-publish.json` records the image publish artifact metadata\n");
        out.push_str(&format!(
            "- reference: `{}`\n",
            plan.image_publish
                .reference
                .as_deref()
                .unwrap_or("<unresolved>")
        ));
        out.push_str(&format!(
            "- credentials reference: `{}`\n",
            plan.image_publish
                .credentials_ref
                .as_deref()
                .unwrap_or("<missing>")
        ));
        out.push_str(&format!(
            "- validation: `{}`\n",
            plan.image_publish.validation.status
        ));
        out.push_str("- publishing is a CI/operator handoff; this artifact does not contain registry secrets and `num deploy` does not run docker login/build/push\n\n");
    }
    out.push_str("## Operations Boundary\n\n");
    out.push_str("This bundle is a reproducible local/CI deployment artifact with generated runtime scaffolding for supported targets. Production image publishing, cluster credentials, host provisioning, SSH access, package installation, systemctl execution, and cloud rollout execution stay outside the artifact boundary.\n");
    out
}

#[cfg(test)]
mod tests {
    use super::{
        build_deployment_plan, build_kubernetes_dry_run, materialize_deployment_artifact,
        render_jenkinsfile,
    };
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
registry = "ghcr.io/acme"
image = "billing-api"
tag_strategy = "version"
credentials_ref = "secret://docker/ghcr"

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
        assert!(plan
            .target_profile
            .required_artifacts
            .contains(&"deploy/Dockerfile".to_string()));
        assert!(plan
            .target_profile
            .required_artifacts
            .contains(&"deploy/compose.yaml".to_string()));
        assert!(plan
            .target_profile
            .required_artifacts
            .contains(&"deploy/image-publish.json".to_string()));
        assert!(plan
            .target_profile
            .required_artifacts
            .contains(&"deploy/Jenkinsfile".to_string()));
        assert_eq!(plan.target_profile.validation.status, "ready");
        assert!(plan.target_profile.warnings.is_empty());
        assert!(plan.image_publish.enabled);
        assert_eq!(plan.image_publish.validation.status, "ready");
        assert_eq!(
            plan.image_publish.reference,
            Some("ghcr.io/acme/billing-api:1.2.3".to_string())
        );
        assert_eq!(
            plan.image_publish.credentials_ref,
            Some("secret://docker/ghcr".to_string())
        );
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
            crate::compatibility::CURRENT_LANGUAGE_VERSION
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
        assert_eq!(
            plan.to_json()["image_publish"]["reference"],
            "ghcr.io/acme/billing-api:1.2.3"
        );
        assert!(plan
            .render_text()
            .contains("Deployment plan: billing 1.2.3"));
        assert!(plan
            .render_text()
            .contains("Image publish: status=ready, reference=ghcr.io/acme/billing-api:1.2.3"));
    }

    #[test]
    fn jenkinsfile_template_runs_deploy_gates_in_order() {
        let root = Path::new("/workspace/app");
        let manifest = PackageManifest::parse(
            root,
            &root.join("num.toml"),
            r#"
[project]
name = "billing"
version = "1.2.3"

[deployment]
target = "container"
registry = "ghcr.io/acme"
image = "billing-api"
tag_strategy = "version"
credentials_ref = "secret://docker/ghcr"
"#,
        );
        let compilation = compile("main.num", "module app.main\n\nworkflow main() {}\n");
        let plan = build_deployment_plan(&manifest, &compilation.module, 1);

        let jenkinsfile = render_jenkinsfile(&plan);

        assert_eq!(
            jenkinsfile,
            include_str!("../tests/fixtures/deploy/Jenkinsfile")
        );
        let policy = jenkinsfile.find("stage('Policy gate')").unwrap();
        let cost = jenkinsfile.find("stage('Cost gate')").unwrap();
        let security = jenkinsfile.find("stage('Security gate')").unwrap();
        let deploy = jenkinsfile.find("stage('Deploy artifact')").unwrap();
        assert!(policy < cost);
        assert!(cost < security);
        assert!(security < deploy);
        assert!(jenkinsfile.contains("num check \"$NUM_PROJECT_DIR\""));
        assert!(jenkinsfile.contains("num test \"$NUM_PROJECT_DIR\""));
        assert!(jenkinsfile.contains("num cost-report \"$NUM_PROJECT_DIR\" --json"));
        assert!(jenkinsfile.contains("num lint \"$NUM_PROJECT_DIR\""));
        assert!(jenkinsfile.contains("num deploy \"$NUM_PROJECT_DIR\" --apply --replace"));
        assert!(jenkinsfile.contains("NUM_REGISTRY_CREDENTIALS_ID"));
        assert!(jenkinsfile.contains("archiveArtifacts"));
    }

    #[test]
    fn deployment_plan_reports_target_validation() {
        let root = Path::new("/workspace/app");
        let cloud_manifest = PackageManifest::parse(
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

        let cloud_plan = build_deployment_plan(&cloud_manifest, &compilation.module, 1);

        assert_eq!(cloud_plan.target_profile.class, "cloud");
        assert_eq!(
            cloud_plan.target_profile.execution,
            "external-cloud-deployer"
        );
        assert_eq!(
            cloud_plan.target_profile.validation.status,
            "missing-required"
        );
        assert_eq!(cloud_plan.target_profile.validation.errors.len(), 2);
        assert!(cloud_plan
            .target_profile
            .validation
            .errors
            .iter()
            .any(|error| error.contains("[deployment].service")));
        assert!(cloud_plan
            .target_profile
            .validation
            .errors
            .iter()
            .any(|error| error.contains("[deployment].region")));
        assert!(cloud_plan
            .target_profile
            .warnings
            .iter()
            .any(|warning| warning.contains("[deployment].service")));
        assert_eq!(
            cloud_plan.to_json()["deployment"]["profile"]["validation"]["status"],
            "missing-required"
        );
        assert!(cloud_plan
            .render_text()
            .contains("Deployment validation errors"));

        let container_manifest = PackageManifest::parse(
            root,
            &root.join("num.toml"),
            r#"
[project]
name = "billing"
version = "1.2.3"

[deployment]
target = "container"
"#,
        );
        let container_plan = build_deployment_plan(&container_manifest, &compilation.module, 1);
        assert_eq!(
            container_plan.target_profile.validation.status,
            "missing-recommended"
        );
        assert_eq!(container_plan.target_profile.validation.errors.len(), 0);
        assert!(container_plan
            .target_profile
            .validation
            .warnings
            .iter()
            .any(|warning| warning.contains("[deployment].service")));
        assert!(container_plan.render_text().contains("Deployment warnings"));

        let custom_manifest = PackageManifest::parse(
            root,
            &root.join("num.toml"),
            r#"
[project]
name = "billing"
version = "1.2.3"

[deployment]
target = "edge-custom"
"#,
        );
        let custom_plan = build_deployment_plan(&custom_manifest, &compilation.module, 1);
        assert_eq!(custom_plan.target_profile.class, "custom");
        assert_eq!(
            custom_plan.target_profile.validation.status,
            "custom-boundary"
        );
        assert!(custom_plan
            .target_profile
            .validation
            .boundary
            .as_deref()
            .unwrap()
            .contains("custom runner"));
    }

    #[test]
    fn deployment_plan_requires_registry_credentials_reference() {
        let root = Path::new("/workspace/app");
        let manifest = PackageManifest::parse(
            root,
            &root.join("num.toml"),
            r#"
[project]
name = "billing"
version = "1.2.3"

[deployment]
target = "container"
registry = "ghcr.io/acme"
image = "billing-api"
"#,
        );
        let compilation = compile(
            "main.num",
            "module app.main\n\nworkflow main() {\n    audit(\"main\")\n}\n",
        );

        let plan = build_deployment_plan(&manifest, &compilation.module, 1);

        assert!(plan.image_publish.enabled);
        assert_eq!(plan.image_publish.validation.status, "invalid");
        assert!(plan
            .image_publish
            .validation
            .errors
            .iter()
            .any(|error| error.contains("[deployment].credentials_ref")));
        assert!(plan
            .render_text()
            .contains("registry credentials stay behind the secret-store boundary"));
        assert_eq!(
            plan.to_json()["image_publish"]["reference"],
            "ghcr.io/acme/billing-api:1.2.3"
        );
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
version = "0.3.0"
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
registry = "ghcr.io/acme"
image = "billing-api"
tag_strategy = "version"
credentials_ref = "secret://docker/ghcr"
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
        assert!(artifact_root.join("src/main.num").is_file());
        assert!(artifact_root.join("deploy/Dockerfile").is_file());
        assert!(artifact_root.join("deploy/image-publish.json").is_file());
        assert!(artifact_root.join("deploy/compose.yaml").is_file());
        assert!(artifact_root.join("deploy/Jenkinsfile").is_file());
        assert_eq!(
            report.runtime_artifacts,
            vec![
                "deploy/Dockerfile".to_string(),
                "deploy/image-publish.json".to_string(),
                "deploy/compose.yaml".to_string(),
                "deploy/Jenkinsfile".to_string()
            ]
        );
        assert_eq!(report.files.len(), 10);
        let compose = fs::read_to_string(artifact_root.join("deploy/compose.yaml")).unwrap();
        assert!(compose.contains("services:\n  num:\n    build:"));
        assert!(compose.contains("    image: ghcr.io/acme/billing-api:1.2.3"));
        assert!(compose.contains("    environment:\n      NUM_DEPLOY_PLAN: /app/num-deploy.json"));
        let image_publish =
            fs::read_to_string(artifact_root.join("deploy/image-publish.json")).unwrap();
        assert!(image_publish.contains("\"kind\": \"num.deploy.image_publish.v1\""));
        assert!(image_publish.contains("\"reference\": \"ghcr.io/acme/billing-api:1.2.3\""));
        assert!(image_publish.contains("\"credentials_ref\": \"secret://docker/ghcr\""));
        assert!(!image_publish.contains("password"));
        let jenkinsfile = fs::read_to_string(artifact_root.join("deploy/Jenkinsfile")).unwrap();
        assert!(jenkinsfile.contains("stage('Policy gate')"));
        assert!(jenkinsfile.contains("stage('Cost gate')"));
        assert!(jenkinsfile.contains("stage('Security gate')"));
        assert!(jenkinsfile.contains("stage('Deploy artifact')"));
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
            .contains("\"validation\""));
        assert!(fs::read_to_string(&report.metadata_path)
            .unwrap()
            .contains("\"environment\""));
        assert!(fs::read_to_string(&report.metadata_path)
            .unwrap()
            .contains("\"runtime_artifacts\""));
        assert!(fs::read_to_string(&report.metadata_path)
            .unwrap()
            .contains("\"image_publish\""));
        assert!(fs::read_to_string(&report.runbook_path)
            .unwrap()
            .contains("## Container Runtime"));
        assert!(fs::read_to_string(&report.runbook_path)
            .unwrap()
            .contains("## Container Image Publish Handoff"));
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
        assert_eq!(replaced.files.len(), 10);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn materialized_artifact_includes_valid_lockfile_when_present() {
        let root = temp_dir("bundle_with_lock_project");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("num.toml"),
            r#"[language]
version = "0.3.0"
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
        assert!(artifact_root.join("src/main.num").is_file());
        assert!(report.runtime_artifacts.is_empty());
        assert_eq!(report.files.len(), 7);
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
    fn materializes_kubernetes_runtime_scaffold() {
        let root = temp_dir("bundle_kubernetes_project");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("num.toml"),
            r#"[language]
version = "0.3.0"
compatibility = "minor"
manifest_schema = 1

[project]
name = "billing-api"
version = "1.2.3"
source = "src"
entry = "src/main.num"

[deployment]
target = "kubernetes"
service = "BillingApi"
region = "local"
"#,
        )
        .unwrap();
        let manifest = PackageManifest::discover(&root).unwrap().unwrap();
        let source = SourceFile {
            name: root.join("src/main.num").display().to_string(),
            source: r#"module app.main

permission IssueRefund

service BillingApi {
    route POST "/refunds" requires Permission.IssueRefund {
        audit("refund")
    }
}
"#
            .to_string(),
        };
        let compilation = compile(&source.name, &source.source);
        let plan = build_deployment_plan(&manifest, &compilation.module, 1);
        let artifact_root = root.join("dist/bundle");

        assert!(plan
            .target_profile
            .required_artifacts
            .contains(&"deploy/kubernetes.yaml".to_string()));
        assert!(plan
            .target_profile
            .required_artifacts
            .contains(&"deploy/Jenkinsfile".to_string()));
        assert_eq!(plan.target_profile.validation.status, "ready");
        assert!(plan.target_profile.validation.errors.is_empty());

        let report = materialize_deployment_artifact(
            &plan,
            &manifest,
            std::slice::from_ref(&source),
            &artifact_root,
            false,
        )
        .unwrap();

        assert_eq!(
            report.runtime_artifacts,
            vec![
                "deploy/Dockerfile".to_string(),
                "deploy/kubernetes.yaml".to_string(),
                "deploy/Jenkinsfile".to_string()
            ]
        );
        let kubernetes = fs::read_to_string(artifact_root.join("deploy/kubernetes.yaml")).unwrap();
        assert!(kubernetes.contains("kind: Deployment"));
        assert!(kubernetes.contains("name: billing-api"));
        assert!(kubernetes.contains("namespace: local"));
        assert!(kubernetes.contains("          args:\n            - \"num\""));
        assert!(kubernetes.contains("\"BillingApi\""));
        assert!(fs::read_to_string(&report.runbook_path)
            .unwrap()
            .contains("## Kubernetes Runtime"));

        let dry_run = build_kubernetes_dry_run(&plan);
        assert_eq!(dry_run.validation.status, "ready");
        assert_eq!(dry_run.validation.namespace, "local");
        assert_eq!(dry_run.validation.image, "num-billing-api:local");
        assert_eq!(dry_run.validation.ports, vec![4000]);
        assert!(dry_run.validation.secret_references.is_empty());
        assert!(dry_run.manifest.contains("namespace: local"));
        assert!(dry_run.render_text().contains("Kubernetes dry-run handoff"));
        assert_eq!(dry_run.to_json()["validation"]["status"], "ready");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn kubernetes_dry_run_reports_handoff_validation() {
        let root = Path::new("/workspace/app");
        let manifest = PackageManifest::parse(
            root,
            &root.join("num.toml"),
            r#"
[project]
name = "Billing API"
version = "1.2.3"

[environment]
required = ["API_TOKEN"]

[deployment]
target = "kubernetes"
service = "BillingApi"
region = "Prod Namespace"
"#,
        );
        let compilation = compile(
            "main.num",
            r#"module app.main

permission IssueRefund

service BillingApi {
    route POST "/refunds" requires Permission.IssueRefund {
        audit("refund")
    }
}
"#,
        );
        let plan = build_deployment_plan(&manifest, &compilation.module, 1);

        let dry_run = build_kubernetes_dry_run(&plan);

        assert_eq!(dry_run.validation.status, "ready");
        assert_eq!(dry_run.validation.namespace, "prod-namespace");
        assert_eq!(dry_run.validation.image, "num-billing-api:local");
        assert_eq!(dry_run.validation.secret_references, vec!["API_TOKEN"]);
        assert!(dry_run
            .validation
            .warnings
            .iter()
            .any(|warning| warning.contains("normalized to Kubernetes namespace")));
        assert!(dry_run
            .validation
            .warnings
            .iter()
            .any(|warning| warning.contains("Kubernetes Secret mappings")));
        assert!(dry_run.manifest.contains("namespace: prod-namespace"));
        assert!(dry_run.to_json()["manifest"]
            .as_str()
            .unwrap()
            .contains("kind: Service"));
    }

    #[test]
    fn materializes_bare_metal_runbook_bundle() {
        let root = temp_dir("bundle_bare_metal_project");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("num.toml"),
            r#"[language]
version = "0.3.0"
compatibility = "minor"
manifest_schema = 1

[project]
name = "billing-api"
version = "1.2.3"
source = "src"
entry = "src/main.num"

[runtime]
workflow_store = "file:.num-state"
audit_store = "file:audit/events.jsonl"

[environment]
required = ["VAULT_TOKEN"]
optional = ["PAYMENTS_URL"]

[deployment]
target = "bare-metal"
service = "BillingApi"
region = "rack-a"

[connectors]
"payments.find" = { command = "node", args = "connectors/payments-find.js", cwd = "ops", timeout_ms = "2000" }
"#,
        )
        .unwrap();
        let manifest = PackageManifest::discover(&root).unwrap().unwrap();
        let source = SourceFile {
            name: root.join("src/main.num").display().to_string(),
            source: r#"module app.main

permission IssueRefund

connector payments {
    find(id: Text) -> Text
}

service BillingApi {
    route POST "/refunds" requires Permission.IssueRefund {
        audit("refund")
    }
}
"#
            .to_string(),
        };
        let compilation = compile(&source.name, &source.source);
        let plan = build_deployment_plan(&manifest, &compilation.module, 1);
        let artifact_root = root.join("dist/bundle");

        assert_eq!(plan.target_profile.class, "host");
        assert_eq!(plan.target_profile.execution, "external-systemd-operator");
        assert_eq!(plan.target_profile.validation.status, "custom-boundary");
        assert!(plan
            .target_profile
            .required_artifacts
            .contains(&"deploy/num.service".to_string()));
        assert!(plan
            .target_profile
            .required_artifacts
            .contains(&"deploy/num.env".to_string()));
        assert!(plan
            .target_profile
            .required_artifacts
            .contains(&"deploy/Jenkinsfile".to_string()));
        assert!(plan
            .target_profile
            .warnings
            .iter()
            .any(|warning| warning.contains("process connectors [payments.find]")));
        assert!(plan
            .target_profile
            .warnings
            .iter()
            .any(|warning| warning.contains("VAULT_TOKEN")));

        let report = materialize_deployment_artifact(
            &plan,
            &manifest,
            std::slice::from_ref(&source),
            &artifact_root,
            false,
        )
        .unwrap();

        assert_eq!(
            report.runtime_artifacts,
            vec![
                "deploy/num.service".to_string(),
                "deploy/num.env".to_string(),
                "deploy/Jenkinsfile".to_string()
            ]
        );
        assert_eq!(report.files.len(), 9);
        let service = fs::read_to_string(artifact_root.join("deploy/num.service")).unwrap();
        assert!(service.contains("Description=Num service for billing-api"));
        assert!(service.contains("WorkingDirectory=/opt/num/billing-api"));
        assert!(service.contains("EnvironmentFile=/etc/num/billing-api.env"));
        assert!(service.contains("ExecStart=/usr/bin/env num serve . 0.0.0.0:4000 BillingApi"));

        let env_file = fs::read_to_string(artifact_root.join("deploy/num.env")).unwrap();
        assert!(env_file.contains("NUM_DEPLOY_PLAN=/opt/num/billing-api/num-deploy.json"));
        assert!(env_file.contains("NUM_WORKFLOW_STORE=file:.num-state"));
        assert!(env_file.contains("NUM_AUDIT_STORE=file:audit/events.jsonl"));
        assert!(env_file.contains("VAULT_TOKEN=\n"));
        assert!(env_file.contains("# PAYMENTS_URL=\n"));

        let runbook = fs::read_to_string(&report.runbook_path).unwrap();
        assert!(runbook.contains("## Bare-Metal Runtime"));
        assert!(runbook.contains("workflow store: `file:.num-state`"));
        assert!(runbook.contains("audit store: `file:audit/events.jsonl`"));
        assert!(runbook.contains("Process connector host requirements"));
        assert!(runbook.contains("SSH access"));
        assert!(fs::read_to_string(&report.metadata_path)
            .unwrap()
            .contains("\"external-systemd-operator\""));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn materialization_rejects_unsupported_lockfile_schema() {
        let root = temp_dir("bundle_bad_lock_project");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("num.toml"),
            r#"[language]
version = "0.3.0"
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
