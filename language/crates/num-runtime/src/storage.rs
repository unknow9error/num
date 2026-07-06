use crate::{
    AuditEvent, AuditResult, AuditSink, RuntimeError, SecurityContext, StateStore, WorkflowState,
    WorkflowStatus,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct FileStateStore {
    root: PathBuf,
}

impl FileStateStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn list_workflows(&self) -> Result<Vec<WorkflowState>, RuntimeError> {
        let dir = self.workflow_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut paths = fs::read_dir(dir)
            .map_storage()?
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
            .collect::<Vec<_>>();
        paths.sort();

        paths
            .into_iter()
            .map(|path| {
                let bytes = fs::read(path).map_storage()?;
                let value: Value = serde_json::from_slice(&bytes).map_storage()?;
                json_to_workflow(&value)
            })
            .collect()
    }

    fn workflow_dir(&self) -> PathBuf {
        self.root.join("workflows")
    }

    fn workflow_path(&self, id: &str) -> PathBuf {
        self.workflow_dir()
            .join(format!("{}.json", safe_file_id(id)))
    }
}

impl StateStore for FileStateStore {
    fn save_workflow(&mut self, state: WorkflowState) -> Result<(), RuntimeError> {
        fs::create_dir_all(self.workflow_dir()).map_storage()?;
        let path = self.workflow_path(&state.id);
        let temp_path = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(&workflow_to_json(&state)).map_storage()?;
        fs::write(&temp_path, bytes).map_storage()?;
        fs::rename(temp_path, path).map_storage()
    }

    fn load_workflow(&self, id: &str) -> Result<Option<WorkflowState>, RuntimeError> {
        let path = self.workflow_path(id);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(path).map_storage()?;
        let value: Value = serde_json::from_slice(&bytes).map_storage()?;
        json_to_workflow(&value).map(Some)
    }
}

#[derive(Debug, Clone)]
pub struct FileAuditSink {
    path: PathBuf,
}

impl FileAuditSink {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl AuditSink for FileAuditSink {
    fn append(&mut self, event: AuditEvent) -> Result<(), RuntimeError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_storage()?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_storage()?;
        let line = serde_json::to_string(&audit_to_json(&event)).map_storage()?;
        writeln!(file, "{line}").map_storage()
    }
}

fn workflow_to_json(state: &WorkflowState) -> Value {
    json!({
        "id": state.id,
        "name": state.name,
        "status": workflow_status_str(&state.status),
        "security": {
            "actor": state.security.actor,
            "tenant": state.security.tenant,
            "permissions": state.security.permissions.iter().cloned().collect::<Vec<_>>(),
            "correlation_id": state.security.correlation_id,
            "request_id": state.security.request_id,
        },
        "started_at_ms": system_time_ms(state.started_at),
        "updated_at_ms": system_time_ms(state.updated_at),
        "metadata": state.metadata,
    })
}

fn json_to_workflow(value: &Value) -> Result<WorkflowState, RuntimeError> {
    let security = value
        .get("security")
        .ok_or_else(|| storage_error("missing workflow security"))?;
    Ok(WorkflowState {
        id: string_field(value, "id")?,
        name: string_field(value, "name")?,
        status: workflow_status_from_str(&string_field(value, "status")?)?,
        security: SecurityContext {
            actor: string_field(security, "actor")?,
            tenant: string_field(security, "tenant")?,
            roles: Default::default(),
            permissions: string_array_field(security, "permissions")?
                .into_iter()
                .collect(),
            correlation_id: string_field(security, "correlation_id")?,
            request_id: string_field(security, "request_id")?,
            provenance: None,
            trust: None,
        },
        started_at: system_time_from_ms(u64_field(value, "started_at_ms")?),
        updated_at: system_time_from_ms(u64_field(value, "updated_at_ms")?),
        metadata: string_map_field(value, "metadata")?,
    })
}

fn audit_to_json(event: &AuditEvent) -> Value {
    json!({
        "event_id": event.event_id,
        "timestamp_ms": system_time_ms(event.timestamp),
        "actor": event.actor,
        "tenant": event.tenant,
        "action": event.action,
        "result": audit_result_to_json(&event.result),
        "permissions_used": event.permissions_used,
        "data_sources": event.data_sources,
        "ai_models": event.ai_models,
        "confidence_values": event.confidence_values,
        "rollback_status": event.rollback_status,
        "correlation_id": event.correlation_id,
        "request_id": event.request_id,
    })
}

fn audit_result_to_json(result: &AuditResult) -> Value {
    match result {
        AuditResult::Started => json!({"kind": "Started"}),
        AuditResult::Waiting => json!({"kind": "Waiting"}),
        AuditResult::Resumed => json!({"kind": "Resumed"}),
        AuditResult::Succeeded => json!({"kind": "Succeeded"}),
        AuditResult::Failed(reason) => json!({"kind": "Failed", "reason": reason}),
        AuditResult::RolledBack => json!({"kind": "RolledBack"}),
        AuditResult::Cancelled => json!({"kind": "Cancelled"}),
    }
}

fn workflow_status_str(status: &WorkflowStatus) -> &'static str {
    match status {
        WorkflowStatus::Created => "Created",
        WorkflowStatus::Running => "Running",
        WorkflowStatus::Waiting => "Waiting",
        WorkflowStatus::Failed => "Failed",
        WorkflowStatus::Compensated => "Compensated",
        WorkflowStatus::Completed => "Completed",
        WorkflowStatus::Cancelled => "Cancelled",
    }
}

fn workflow_status_from_str(value: &str) -> Result<WorkflowStatus, RuntimeError> {
    match value {
        "Created" => Ok(WorkflowStatus::Created),
        "Running" => Ok(WorkflowStatus::Running),
        "Waiting" => Ok(WorkflowStatus::Waiting),
        "Failed" => Ok(WorkflowStatus::Failed),
        "Compensated" => Ok(WorkflowStatus::Compensated),
        "Completed" => Ok(WorkflowStatus::Completed),
        "Cancelled" => Ok(WorkflowStatus::Cancelled),
        other => Err(storage_error(format!("unknown workflow status '{other}'"))),
    }
}

fn system_time_ms(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn system_time_from_ms(ms: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_millis(ms)
}

fn string_field(value: &Value, key: &str) -> Result<String, RuntimeError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| storage_error(format!("missing string field '{key}'")))
}

fn u64_field(value: &Value, key: &str) -> Result<u64, RuntimeError> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| storage_error(format!("missing u64 field '{key}'")))
}

fn string_array_field(value: &Value, key: &str) -> Result<Vec<String>, RuntimeError> {
    let array = value
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| storage_error(format!("missing string array field '{key}'")))?;
    array
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::to_string)
                .ok_or_else(|| storage_error(format!("non-string item in '{key}'")))
        })
        .collect()
}

fn string_map_field(value: &Value, key: &str) -> Result<BTreeMap<String, String>, RuntimeError> {
    let object = value
        .get(key)
        .and_then(Value::as_object)
        .ok_or_else(|| storage_error(format!("missing string map field '{key}'")))?;
    object
        .iter()
        .map(|(field, item)| {
            item.as_str()
                .map(|value| (field.clone(), value.to_string()))
                .ok_or_else(|| storage_error(format!("non-string value in '{key}.{field}'")))
        })
        .collect()
}

fn safe_file_id(id: &str) -> String {
    id.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn storage_error(message: impl Into<String>) -> RuntimeError {
    RuntimeError::Storage(message.into())
}

trait MapStorage<T> {
    fn map_storage(self) -> Result<T, RuntimeError>;
}

impl<T, E: std::fmt::Display> MapStorage<T> for Result<T, E> {
    fn map_storage(self) -> Result<T, RuntimeError> {
        self.map_err(|err| RuntimeError::Storage(err.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::{FileAuditSink, FileStateStore};
    use crate::{
        AuditEvent, AuditResult, AuditSink, SecurityContext, StateStore, WorkflowState,
        WorkflowStatus,
    };
    use std::collections::{BTreeMap, BTreeSet};
    use std::fs;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[test]
    fn file_state_store_round_trips_workflow() {
        let root = unique_test_dir("state");
        let mut store = FileStateStore::new(&root);
        let state = workflow_state("wf_1");

        store.save_workflow(state.clone()).unwrap();
        let loaded = store.load_workflow("wf_1").unwrap().unwrap();

        assert_eq!(loaded.id, state.id);
        assert_eq!(loaded.name, state.name);
        assert_eq!(loaded.status, state.status);
        assert_eq!(loaded.security.actor, "agent@example.com");
        assert!(loaded.security.permissions.contains("IssueRefund"));
        assert_eq!(
            loaded.metadata.get("source").map(String::as_str),
            Some("test")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn file_state_store_returns_none_for_missing_workflow() {
        let root = unique_test_dir("missing");
        let store = FileStateStore::new(&root);

        assert!(store.load_workflow("missing").unwrap().is_none());
    }

    #[test]
    fn file_state_store_lists_workflows() {
        let root = unique_test_dir("list");
        let mut store = FileStateStore::new(&root);

        store.save_workflow(workflow_state("wf_b")).unwrap();
        store.save_workflow(workflow_state("wf_a")).unwrap();

        let workflows = store.list_workflows().unwrap();

        assert_eq!(workflows.len(), 2);
        assert_eq!(workflows[0].id, "wf_a");
        assert_eq!(workflows[1].id, "wf_b");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn file_audit_sink_appends_json_lines() {
        let root = unique_test_dir("audit");
        let path = root.join("audit/events.jsonl");
        let mut sink = FileAuditSink::new(&path);

        sink.append(audit_event("evt_1", AuditResult::Started))
            .unwrap();
        sink.append(audit_event("evt_2", AuditResult::Succeeded))
            .unwrap();

        let contents = fs::read_to_string(sink.path()).unwrap();
        let lines: Vec<_> = contents.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("\"event_id\":\"evt_1\""));
        assert!(lines[1].contains("\"kind\":\"Succeeded\""));
        let _ = fs::remove_dir_all(root);
    }

    fn workflow_state(id: &str) -> WorkflowState {
        let mut permissions = BTreeSet::new();
        permissions.insert("IssueRefund".to_string());
        let mut metadata = BTreeMap::new();
        metadata.insert("source".to_string(), "test".to_string());
        WorkflowState {
            id: id.to_string(),
            name: "process_refund".to_string(),
            status: WorkflowStatus::Running,
            security: SecurityContext {
                actor: "agent@example.com".to_string(),
                tenant: "tenant_1".to_string(),
                roles: Default::default(),
                permissions,
                correlation_id: "corr_1".to_string(),
                request_id: "req_1".to_string(),
                provenance: None,
                trust: None,
            },
            started_at: fixed_time(1),
            updated_at: fixed_time(2),
            metadata,
        }
    }

    fn audit_event(id: &str, result: AuditResult) -> AuditEvent {
        AuditEvent {
            event_id: id.to_string(),
            timestamp: fixed_time(3),
            actor: "agent@example.com".to_string(),
            tenant: "tenant_1".to_string(),
            action: "issue_refund".to_string(),
            result,
            permissions_used: vec!["IssueRefund".to_string()],
            data_sources: vec!["payments".to_string()],
            ai_models: vec![],
            confidence_values: vec![],
            rollback_status: None,
            correlation_id: "corr_1".to_string(),
            request_id: "req_1".to_string(),
        }
    }

    fn fixed_time(seconds: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(seconds)
    }

    fn unique_test_dir(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "num-runtime-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
}
