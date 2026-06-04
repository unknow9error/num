use crate::{engine::WorkflowStart, WorkflowId};
use crate::{RuntimeError, SecurityContext};
use serde_json::{json, Value};
use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct WorkflowEvent {
    pub id: String,
    pub queued_at: SystemTime,
    pub kind: WorkflowEventKind,
}

impl WorkflowEvent {
    pub fn start(id: impl Into<String>, start: WorkflowStart) -> Self {
        Self {
            id: id.into(),
            queued_at: SystemTime::now(),
            kind: WorkflowEventKind::Start(start),
        }
    }

    pub fn wait(id: impl Into<String>, workflow_id: impl Into<WorkflowId>) -> Self {
        Self::transition(id, workflow_id, WorkflowEventKindName::Wait)
    }

    pub fn resume(id: impl Into<String>, workflow_id: impl Into<WorkflowId>) -> Self {
        Self::transition(id, workflow_id, WorkflowEventKindName::Resume)
    }

    pub fn complete(id: impl Into<String>, workflow_id: impl Into<WorkflowId>) -> Self {
        Self::transition(id, workflow_id, WorkflowEventKindName::Complete)
    }

    pub fn fail(
        id: impl Into<String>,
        workflow_id: impl Into<WorkflowId>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            queued_at: SystemTime::now(),
            kind: WorkflowEventKind::Fail {
                workflow_id: workflow_id.into(),
                reason: reason.into(),
            },
        }
    }

    pub fn compensate(id: impl Into<String>, workflow_id: impl Into<WorkflowId>) -> Self {
        Self::transition(id, workflow_id, WorkflowEventKindName::Compensate)
    }

    pub fn cancel(id: impl Into<String>, workflow_id: impl Into<WorkflowId>) -> Self {
        Self::transition(id, workflow_id, WorkflowEventKindName::Cancel)
    }

    fn transition(
        id: impl Into<String>,
        workflow_id: impl Into<WorkflowId>,
        kind: WorkflowEventKindName,
    ) -> Self {
        let workflow_id = workflow_id.into();
        let kind = match kind {
            WorkflowEventKindName::Wait => WorkflowEventKind::Wait { workflow_id },
            WorkflowEventKindName::Resume => WorkflowEventKind::Resume { workflow_id },
            WorkflowEventKindName::Complete => WorkflowEventKind::Complete { workflow_id },
            WorkflowEventKindName::Compensate => WorkflowEventKind::Compensate { workflow_id },
            WorkflowEventKindName::Cancel => WorkflowEventKind::Cancel { workflow_id },
        };
        Self {
            id: id.into(),
            queued_at: SystemTime::now(),
            kind,
        }
    }
}

#[derive(Debug, Clone)]
pub enum WorkflowEventKind {
    Start(WorkflowStart),
    Wait {
        workflow_id: WorkflowId,
    },
    Resume {
        workflow_id: WorkflowId,
    },
    Complete {
        workflow_id: WorkflowId,
    },
    Fail {
        workflow_id: WorkflowId,
        reason: String,
    },
    Compensate {
        workflow_id: WorkflowId,
    },
    Cancel {
        workflow_id: WorkflowId,
    },
}

enum WorkflowEventKindName {
    Wait,
    Resume,
    Complete,
    Compensate,
    Cancel,
}

pub trait WorkflowEventQueue {
    fn enqueue(&mut self, event: WorkflowEvent) -> Result<(), RuntimeError>;
    fn dequeue(&mut self) -> Result<Option<WorkflowEvent>, RuntimeError>;
}

#[derive(Debug, Default)]
pub struct MemoryWorkflowEventQueue {
    events: VecDeque<WorkflowEvent>,
}

impl MemoryWorkflowEventQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

impl WorkflowEventQueue for MemoryWorkflowEventQueue {
    fn enqueue(&mut self, event: WorkflowEvent) -> Result<(), RuntimeError> {
        self.events.push_back(event);
        Ok(())
    }

    fn dequeue(&mut self) -> Result<Option<WorkflowEvent>, RuntimeError> {
        Ok(self.events.pop_front())
    }
}

#[derive(Debug, Clone)]
pub struct FileWorkflowEventQueue {
    dir: PathBuf,
}

impl FileWorkflowEventQueue {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn event_path(&self, event: &WorkflowEvent) -> Result<PathBuf, RuntimeError> {
        let mut prefix = system_time_ns(SystemTime::now());
        while event_prefix_exists(&self.dir, prefix)? {
            prefix += 1;
        }
        Ok(self
            .dir
            .join(format!("{prefix:030}-{}.json", safe_file_id(&event.id))))
    }
}

impl WorkflowEventQueue for FileWorkflowEventQueue {
    fn enqueue(&mut self, event: WorkflowEvent) -> Result<(), RuntimeError> {
        fs::create_dir_all(&self.dir).map_storage()?;
        let path = self.event_path(&event)?;
        let temp_path = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(&event_to_json(&event)).map_storage()?;
        fs::write(&temp_path, bytes).map_storage()?;
        fs::rename(temp_path, path).map_storage()
    }

    fn dequeue(&mut self) -> Result<Option<WorkflowEvent>, RuntimeError> {
        if !self.dir.exists() {
            return Ok(None);
        }
        let Some(path) = next_event_path(&self.dir)? else {
            return Ok(None);
        };
        let bytes = fs::read(&path).map_storage()?;
        let value: Value = serde_json::from_slice(&bytes).map_storage()?;
        let event = json_to_event(&value)?;
        fs::remove_file(path).map_storage()?;
        Ok(Some(event))
    }
}

fn next_event_path(dir: &Path) -> Result<Option<PathBuf>, RuntimeError> {
    let mut entries = fs::read_dir(dir)
        .map_storage()?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .collect::<Vec<_>>();
    entries.sort();
    Ok(entries.into_iter().next())
}

fn event_prefix_exists(dir: &Path, prefix: u128) -> Result<bool, RuntimeError> {
    if !dir.exists() {
        return Ok(false);
    }
    let prefix = format!("{prefix:030}-");
    for entry in fs::read_dir(dir).map_storage()? {
        let path = entry.map_storage()?.path();
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(&prefix))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn event_to_json(event: &WorkflowEvent) -> Value {
    json!({
        "id": event.id,
        "queued_at_ms": system_time_ms(event.queued_at),
        "kind": event_kind_to_json(&event.kind),
    })
}

fn event_kind_to_json(kind: &WorkflowEventKind) -> Value {
    match kind {
        WorkflowEventKind::Start(start) => json!({
            "type": "Start",
            "workflow": {
                "id": start.id,
                "name": start.name,
                "security": security_to_json(&start.security),
                "metadata": start.metadata,
            }
        }),
        WorkflowEventKind::Wait { workflow_id } => {
            json!({"type": "Wait", "workflow_id": workflow_id})
        }
        WorkflowEventKind::Resume { workflow_id } => {
            json!({"type": "Resume", "workflow_id": workflow_id})
        }
        WorkflowEventKind::Complete { workflow_id } => {
            json!({"type": "Complete", "workflow_id": workflow_id})
        }
        WorkflowEventKind::Fail {
            workflow_id,
            reason,
        } => json!({"type": "Fail", "workflow_id": workflow_id, "reason": reason}),
        WorkflowEventKind::Compensate { workflow_id } => {
            json!({"type": "Compensate", "workflow_id": workflow_id})
        }
        WorkflowEventKind::Cancel { workflow_id } => {
            json!({"type": "Cancel", "workflow_id": workflow_id})
        }
    }
}

fn security_to_json(security: &SecurityContext) -> Value {
    json!({
        "actor": security.actor,
        "tenant": security.tenant,
        "permissions": security.permissions.iter().cloned().collect::<Vec<_>>(),
        "correlation_id": security.correlation_id,
        "request_id": security.request_id,
    })
}

fn json_to_event(value: &Value) -> Result<WorkflowEvent, RuntimeError> {
    Ok(WorkflowEvent {
        id: string_field(value, "id")?,
        queued_at: system_time_from_ms(u64_field(value, "queued_at_ms")?),
        kind: json_to_event_kind(
            value
                .get("kind")
                .ok_or_else(|| storage_error("missing workflow event kind"))?,
        )?,
    })
}

fn json_to_event_kind(value: &Value) -> Result<WorkflowEventKind, RuntimeError> {
    match string_field(value, "type")?.as_str() {
        "Start" => {
            let workflow = value
                .get("workflow")
                .ok_or_else(|| storage_error("missing start workflow payload"))?;
            let security = workflow
                .get("security")
                .ok_or_else(|| storage_error("missing workflow security payload"))?;
            Ok(WorkflowEventKind::Start(WorkflowStart {
                id: string_field(workflow, "id")?,
                name: string_field(workflow, "name")?,
                security: SecurityContext {
                    actor: string_field(security, "actor")?,
                    tenant: string_field(security, "tenant")?,
                    permissions: string_array_field(security, "permissions")?
                        .into_iter()
                        .collect(),
                    correlation_id: string_field(security, "correlation_id")?,
                    request_id: string_field(security, "request_id")?,
                },
                metadata: string_map_field(workflow, "metadata")?,
            }))
        }
        "Wait" => Ok(WorkflowEventKind::Wait {
            workflow_id: string_field(value, "workflow_id")?,
        }),
        "Resume" => Ok(WorkflowEventKind::Resume {
            workflow_id: string_field(value, "workflow_id")?,
        }),
        "Complete" => Ok(WorkflowEventKind::Complete {
            workflow_id: string_field(value, "workflow_id")?,
        }),
        "Fail" => Ok(WorkflowEventKind::Fail {
            workflow_id: string_field(value, "workflow_id")?,
            reason: string_field(value, "reason")?,
        }),
        "Compensate" => Ok(WorkflowEventKind::Compensate {
            workflow_id: string_field(value, "workflow_id")?,
        }),
        "Cancel" => Ok(WorkflowEventKind::Cancel {
            workflow_id: string_field(value, "workflow_id")?,
        }),
        other => Err(storage_error(format!(
            "unknown workflow event type '{other}'"
        ))),
    }
}

fn system_time_ms(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn system_time_ns(time: SystemTime) -> u128 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
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
    use super::{FileWorkflowEventQueue, MemoryWorkflowEventQueue};
    use super::{WorkflowEvent, WorkflowEventKind, WorkflowEventQueue};
    use crate::engine::WorkflowStart;
    use crate::SecurityContext;
    use std::collections::{BTreeMap, BTreeSet};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn memory_workflow_event_queue_preserves_order() {
        let mut queue = MemoryWorkflowEventQueue::new();

        queue.enqueue(WorkflowEvent::wait("evt_1", "wf_1")).unwrap();
        queue
            .enqueue(WorkflowEvent::complete("evt_2", "wf_1"))
            .unwrap();

        assert_eq!(queue.len(), 2);
        assert_eq!(queue.dequeue().unwrap().unwrap().id, "evt_1");
        assert_eq!(queue.dequeue().unwrap().unwrap().id, "evt_2");
        assert!(queue.dequeue().unwrap().is_none());
        assert!(queue.is_empty());
    }

    #[test]
    fn file_workflow_event_queue_round_trips_start_events() {
        let root = unique_test_dir("events");
        let mut queue = FileWorkflowEventQueue::new(root.join("queue"));

        queue
            .enqueue(WorkflowEvent::start("evt_start", workflow_start("wf_file")))
            .unwrap();

        let event = queue.dequeue().unwrap().unwrap();
        assert_eq!(event.id, "evt_start");
        match event.kind {
            WorkflowEventKind::Start(start) => {
                assert_eq!(start.id, "wf_file");
                assert_eq!(start.name, "process_refund");
                assert!(start.security.permissions.contains("IssueRefund"));
                assert_eq!(
                    start.metadata.get("source").map(String::as_str),
                    Some("test")
                );
            }
            other => panic!("expected start event, got {other:?}"),
        }
        assert!(queue.dequeue().unwrap().is_none());
        let _ = fs::remove_dir_all(root);
    }

    fn workflow_start(id: &str) -> WorkflowStart {
        let mut permissions = BTreeSet::new();
        permissions.insert("IssueRefund".to_string());
        let mut metadata = BTreeMap::new();
        metadata.insert("source".to_string(), "test".to_string());
        WorkflowStart {
            id: id.to_string(),
            name: "process_refund".to_string(),
            security: SecurityContext {
                actor: "agent@example.com".to_string(),
                tenant: "tenant_1".to_string(),
                permissions,
                correlation_id: "corr_1".to_string(),
                request_id: "req_1".to_string(),
            },
            metadata,
        }
    }

    fn unique_test_dir(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "num-workflow-events-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
}
