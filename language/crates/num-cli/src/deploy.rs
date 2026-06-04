use crate::compatibility::CompatibilityReport;
use crate::package::{DependencySource, PackageManifest};
use num_compiler::ast::{Declaration, Module, Risk};
use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub struct DeploymentPlan {
    pub package_name: String,
    pub package_version: String,
    pub compatibility: CompatibilityDeployment,
    pub target: String,
    pub service: Option<String>,
    pub region: Option<String>,
    pub artifact: String,
    pub source: String,
    pub entry: String,
    pub runtime: RuntimeDeployment,
    pub security: SecurityDeployment,
    pub modules: usize,
    pub workflows: Vec<String>,
    pub actions: Vec<ActionDeployment>,
    pub services: Vec<ServiceDeployment>,
    pub connectors: Vec<String>,
    pub process_connectors: Vec<String>,
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
        source: manifest.project.source.clone(),
        entry: manifest.project.entry.clone(),
        runtime: RuntimeDeployment {
            workflow_store: manifest.runtime.workflow_store.clone(),
            audit_store: manifest.runtime.audit_store.clone(),
        },
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
            "security": {
                "policy_mode": self.security.policy_mode,
                "tenant_isolation": self.security.tenant_isolation,
            },
            "workflows": self.workflows,
            "actions": self.actions.iter().map(ActionDeployment::to_json).collect::<Vec<_>>(),
            "services": self.services.iter().map(ServiceDeployment::to_json).collect::<Vec<_>>(),
            "connectors": self.connectors,
            "process_connectors": self.process_connectors,
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
        out
    }
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
    match source {
        DependencySource::Registry => "registry".to_string(),
        DependencySource::Path(path) => format!("path:{path}"),
        DependencySource::Git(url) => format!("git:{url}"),
    }
}

fn risk_label(risk: Risk) -> &'static str {
    match risk {
        Risk::Low => "low",
        Risk::Medium => "medium",
        Risk::High => "high",
        Risk::Critical => "critical",
    }
}

#[cfg(test)]
mod tests {
    use super::build_deployment_plan;
    use crate::package::PackageManifest;
    use num_compiler::compile;
    use std::path::Path;

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
        assert_eq!(plan.runtime.workflow_store, "file:.num-state");
        assert_eq!(plan.workflows, vec!["main".to_string()]);
        assert_eq!(plan.actions[0].risk, "high");
        assert_eq!(plan.services[0].routes, vec!["POST /refunds".to_string()]);
        assert_eq!(
            plan.to_json()["compatibility"]["language"]["version"],
            "0.1.0"
        );
        assert_eq!(plan.to_json()["compatibility"]["manifest"]["schema"], 1);
        assert_eq!(plan.to_json()["deployment"]["service"], "BillingApi");
        assert!(plan
            .render_text()
            .contains("Deployment plan: billing 1.2.3"));
    }
}
