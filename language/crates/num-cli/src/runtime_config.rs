use crate::package::PackageManifest;
use num_runtime::SecurityContext;
use serde_json::{json, Value};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowRuntimePaths {
    pub state_root: PathBuf,
    pub queue_dir: PathBuf,
    pub audit_path: PathBuf,
    pub source: RuntimePathSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimePathSource {
    ExplicitStateRoot,
    Manifest { manifest_path: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterpreterAuditTarget {
    Stdout,
    File(PathBuf),
}

pub fn resolve_workflow_runtime_paths(input: &Path) -> Result<WorkflowRuntimePaths, String> {
    if let Some(manifest) = manifest_for_runtime_target(input)? {
        let state_root = resolve_workflow_store(&manifest)?;
        let audit_path = resolve_audit_store(&manifest, &state_root)?;
        return Ok(WorkflowRuntimePaths {
            queue_dir: state_root.join("events"),
            state_root,
            audit_path,
            source: RuntimePathSource::Manifest {
                manifest_path: manifest.path,
            },
        });
    }

    let state_root = input.to_path_buf();
    Ok(WorkflowRuntimePaths {
        queue_dir: state_root.join("events"),
        audit_path: state_root.join("audit/events.jsonl"),
        state_root,
        source: RuntimePathSource::ExplicitStateRoot,
    })
}

pub fn resolve_interpreter_audit_target(input: &Path) -> Result<InterpreterAuditTarget, String> {
    let Some(manifest) = manifest_for_runtime_target(input)? else {
        return Ok(InterpreterAuditTarget::Stdout);
    };
    resolve_interpreter_audit_store(&manifest)
}

pub fn resolve_tenant_isolation(input: &Path) -> Result<bool, String> {
    Ok(manifest_for_runtime_target(input)?
        .map(|manifest| manifest.security.tenant_isolation)
        .unwrap_or(false))
}

pub fn write_interpreter_audit_events(
    target: &InterpreterAuditTarget,
    command: &str,
    events: &[String],
) -> Result<(), String> {
    append_audit_jsonl(target, events, |event_batch, index, event| {
        json!({
            "event_id": format!("demo-{command}-{event_batch}-{}", index + 1),
            "actor": "demo",
            "tenant": "default",
            "command": command,
            "action": command,
            "result": {
                "kind": "Succeeded",
            },
            "demo_event": event,
        })
    })
}

pub fn write_service_audit_events(
    target: &InterpreterAuditTarget,
    service: &str,
    method: &str,
    path: &str,
    security: &SecurityContext,
    events: &[String],
) -> Result<(), String> {
    append_audit_jsonl(target, events, |event_batch, index, event| {
        json!({
            "event_id": format!("service-{event_batch}-{}", index + 1),
            "actor": security.actor.clone(),
            "tenant": security.tenant.clone(),
            "correlation_id": security.correlation_id.clone(),
            "request_id": security.request_id.clone(),
            "service": service,
            "method": method,
            "path": path,
            "action": format!("{service} {method} {path}"),
            "result": {
                "kind": "Succeeded",
            },
            "demo_event": event,
        })
    })
}

fn append_audit_jsonl(
    target: &InterpreterAuditTarget,
    events: &[String],
    mut render: impl FnMut(u128, usize, &str) -> Value,
) -> Result<(), String> {
    let InterpreterAuditTarget::File(path) = target else {
        return Ok(());
    };
    if events.is_empty() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|err| format!("failed to open {}: {err}", path.display()))?;
    let event_batch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    for (index, event) in events.iter().enumerate() {
        let line = serde_json::to_string(&render(event_batch, index, event))
            .map_err(|err| format!("failed to render audit event: {err}"))?;
        writeln!(file, "{line}")
            .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    }
    Ok(())
}

fn manifest_for_runtime_target(input: &Path) -> Result<Option<PackageManifest>, String> {
    if input.is_file() || input.join("num.toml").is_file() {
        return PackageManifest::discover(input);
    }
    Ok(None)
}

fn resolve_interpreter_audit_store(
    manifest: &PackageManifest,
) -> Result<InterpreterAuditTarget, String> {
    let spec = manifest.runtime.audit_store.trim();
    if spec == "stdout" {
        return Ok(InterpreterAuditTarget::Stdout);
    }
    let path = spec.strip_prefix("file:").ok_or_else(|| {
        format!(
            "{} uses unsupported [runtime].audit_store `{spec}`; expected `stdout` or `file:<events.jsonl>`",
            manifest.path.display()
        )
    })?;
    resolve_manifest_relative_path(manifest, path, "[runtime].audit_store")
        .map(InterpreterAuditTarget::File)
}

fn resolve_workflow_store(manifest: &PackageManifest) -> Result<PathBuf, String> {
    let spec = manifest.runtime.workflow_store.trim();
    if spec == "memory" {
        return Err(format!(
            "{} uses [runtime].workflow_store = \"memory\"; durable workflow commands require `file:<state-root>`",
            manifest.path.display()
        ));
    }
    let path = spec.strip_prefix("file:").ok_or_else(|| {
        format!(
            "{} uses unsupported [runtime].workflow_store `{spec}`; expected `memory` or `file:<state-root>`",
            manifest.path.display()
        )
    })?;
    resolve_manifest_relative_path(manifest, path, "[runtime].workflow_store")
}

fn resolve_audit_store(manifest: &PackageManifest, state_root: &Path) -> Result<PathBuf, String> {
    let spec = manifest.runtime.audit_store.trim();
    if spec == "stdout" {
        return Ok(state_root.join("audit/events.jsonl"));
    }
    let path = spec.strip_prefix("file:").ok_or_else(|| {
        format!(
            "{} uses unsupported [runtime].audit_store `{spec}`; expected `stdout` or `file:<events.jsonl>`",
            manifest.path.display()
        )
    })?;
    resolve_manifest_relative_path(manifest, path, "[runtime].audit_store")
}

fn resolve_manifest_relative_path(
    manifest: &PackageManifest,
    path: &str,
    field: &str,
) -> Result<PathBuf, String> {
    let path = path.trim();
    if path.is_empty() {
        return Err(format!("{} has empty {field}", manifest.path.display()));
    }
    let path = PathBuf::from(path);
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(manifest.root.join(path))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        resolve_interpreter_audit_target, resolve_tenant_isolation, resolve_workflow_runtime_paths,
        write_interpreter_audit_events, write_service_audit_events, InterpreterAuditTarget,
        RuntimePathSource,
    };
    use num_runtime::SecurityContext;
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("num_runtime_config_{name}_{stamp}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn resolves_manifest_file_runtime_paths() {
        let root = temp_dir("manifest_file_paths");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("num.toml"),
            r#"
[project]
name = "app"
version = "0.1.0"

[runtime]
workflow_store = "file:.num-state"
audit_store = "file:audit/events.jsonl"
"#,
        )
        .unwrap();

        let paths = resolve_workflow_runtime_paths(&root).unwrap();

        assert_eq!(paths.state_root, root.join(".num-state"));
        assert_eq!(paths.queue_dir, root.join(".num-state/events"));
        assert_eq!(paths.audit_path, root.join("audit/events.jsonl"));
        assert!(matches!(paths.source, RuntimePathSource::Manifest { .. }));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn preserves_explicit_state_root_paths() {
        let root = PathBuf::from("/tmp/num-explicit-state");

        let paths = resolve_workflow_runtime_paths(&root).unwrap();

        assert_eq!(paths.state_root, root);
        assert_eq!(
            paths.queue_dir,
            PathBuf::from("/tmp/num-explicit-state/events")
        );
        assert_eq!(
            paths.audit_path,
            PathBuf::from("/tmp/num-explicit-state/audit/events.jsonl")
        );
        assert_eq!(paths.source, RuntimePathSource::ExplicitStateRoot);
    }

    #[test]
    fn rejects_project_memory_store_for_durable_workflows() {
        let root = temp_dir("memory_store");
        fs::write(
            root.join("num.toml"),
            r#"
[project]
name = "app"
version = "0.1.0"

[runtime]
workflow_store = "memory"
"#,
        )
        .unwrap();

        let err = resolve_workflow_runtime_paths(&root).unwrap_err();

        assert!(err.contains("durable workflow commands require `file:<state-root>`"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolves_and_writes_interpreter_file_audit_store() {
        let root = temp_dir("interpreter_audit_store");
        fs::write(
            root.join("num.toml"),
            r#"
[project]
name = "app"
version = "0.1.0"

[runtime]
audit_store = "file:audit/demo.jsonl"
"#,
        )
        .unwrap();

        let target = resolve_interpreter_audit_target(&root).unwrap();
        write_interpreter_audit_events(&target, "run", &["\"ok\"".to_string()]).unwrap();

        let InterpreterAuditTarget::File(path) = target else {
            panic!("expected file audit target");
        };
        let source = fs::read_to_string(path).unwrap();
        assert!(source.contains("\"result\":{\"kind\":\"Succeeded\"}"));
        assert!(source.contains("\"action\":\"run\""));
        assert!(source.contains("\"command\":\"run\""));
        assert!(source.contains("\\\"ok\\\""));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolves_tenant_isolation_from_manifest_security() {
        let root = temp_dir("tenant_isolation");
        fs::write(
            root.join("num.toml"),
            r#"
[project]
name = "app"
version = "0.1.0"

[security]
tenant_isolation = true
"#,
        )
        .unwrap();

        assert!(resolve_tenant_isolation(&root).unwrap());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn tenant_isolation_defaults_to_disabled_without_manifest() {
        let root = temp_dir("tenant_isolation_absent");

        assert!(!resolve_tenant_isolation(&root.join("src/main.num")).unwrap());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn writes_service_file_audit_store_with_request_security() {
        let root = temp_dir("service_audit_store");
        let target = InterpreterAuditTarget::File(root.join("audit/service.jsonl"));
        let security = SecurityContext {
            actor: "agent@example.com".to_string(),
            tenant: "tenant_a".to_string(),
            roles: Default::default(),
            permissions: BTreeSet::new(),
            correlation_id: "corr_1".to_string(),
            request_id: "req_1".to_string(),
            provenance: None,
            trust: None,
        };

        write_service_audit_events(
            &target,
            "BillingApi",
            "POST",
            "/refunds",
            &security,
            &["\"refund_1\"".to_string()],
        )
        .unwrap();

        let source = fs::read_to_string(root.join("audit/service.jsonl")).unwrap();
        assert!(source.contains("\"action\":\"BillingApi POST /refunds\""));
        assert!(source.contains("\"actor\":\"agent@example.com\""));
        assert!(source.contains("\"tenant\":\"tenant_a\""));
        assert!(source.contains("\"service\":\"BillingApi\""));
        assert!(source.contains("\"request_id\":\"req_1\""));
        assert!(source.contains("\\\"refund_1\\\""));
        fs::remove_dir_all(root).unwrap();
    }
}
