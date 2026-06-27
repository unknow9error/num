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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowLeaseOptions {
    pub worker_id: String,
    pub lease_timeout: Duration,
    pub max_attempts: u32,
}

impl Default for WorkflowLeaseOptions {
    fn default() -> Self {
        Self {
            worker_id: "local-worker".to_string(),
            lease_timeout: Duration::from_secs(30),
            max_attempts: 3,
        }
    }
}

#[derive(Debug, Clone)]
pub struct WorkflowEventLease {
    pub event: WorkflowEvent,
    pub worker_id: String,
    pub attempt: u32,
    path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowLeaseHeartbeat {
    pub event_id: String,
    pub worker_id: String,
    pub attempt: u32,
    pub previous_leased_at_ms: u64,
    pub leased_at_ms: u64,
}

impl WorkflowLeaseHeartbeat {
    pub fn to_json(&self) -> Value {
        json!({
            "event_id": self.event_id,
            "worker_id": self.worker_id,
            "attempt": self.attempt,
            "previous_leased_at_ms": self.previous_leased_at_ms,
            "leased_at_ms": self.leased_at_ms,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowLeaseDisposition {
    Requeued,
    DeadLettered,
}

#[derive(Debug, Clone)]
struct WorkflowQueueItem {
    event: WorkflowEvent,
    attempt: u32,
}

#[derive(Debug, Clone)]
struct WorkflowLeaseMetadata {
    worker_id: String,
    leased_at: SystemTime,
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

    pub fn leases_dir(&self) -> PathBuf {
        self.dir.join("leases")
    }

    pub fn dead_letter_dir(&self) -> PathBuf {
        self.dir.join("dead")
    }

    pub fn claim(
        &mut self,
        options: &WorkflowLeaseOptions,
    ) -> Result<Option<WorkflowEventLease>, RuntimeError> {
        fs::create_dir_all(&self.dir).map_storage()?;
        fs::create_dir_all(self.leases_dir()).map_storage()?;
        fs::create_dir_all(self.dead_letter_dir()).map_storage()?;
        self.recover_expired_leases(options)?;

        let Some(path) = next_event_path(&self.dir)? else {
            return Ok(None);
        };
        let Some(file_name) = path.file_name().map(|name| name.to_owned()) else {
            return Ok(None);
        };
        let lease_path = self.leases_dir().join(file_name);
        match fs::rename(&path, &lease_path) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(RuntimeError::Storage(err.to_string())),
        }

        let mut item = read_queue_item(&lease_path)?;
        item.attempt = item.attempt.saturating_add(1);
        write_leased_item(&lease_path, &item, &options.worker_id, SystemTime::now())?;
        Ok(Some(WorkflowEventLease {
            event: item.event,
            worker_id: options.worker_id.clone(),
            attempt: item.attempt,
            path: lease_path,
        }))
    }

    pub fn ack(&mut self, lease: &WorkflowEventLease) -> Result<(), RuntimeError> {
        if lease.path.exists() {
            fs::remove_file(&lease.path).map_storage()?;
        }
        Ok(())
    }

    pub fn fail(
        &mut self,
        lease: &WorkflowEventLease,
        error: &str,
        max_attempts: u32,
    ) -> Result<WorkflowLeaseDisposition, RuntimeError> {
        if lease.attempt >= max_attempts {
            let path = self.archive_path(&self.dead_letter_dir(), &lease.event)?;
            write_dead_letter_item(&path, &lease.event, lease.attempt, &lease.worker_id, error)?;
            if lease.path.exists() {
                fs::remove_file(&lease.path).map_storage()?;
            }
            return Ok(WorkflowLeaseDisposition::DeadLettered);
        }

        let item = WorkflowQueueItem {
            event: lease.event.clone(),
            attempt: lease.attempt,
        };
        let path = self.event_path(&lease.event)?;
        write_queue_item(&path, &item)?;
        if lease.path.exists() {
            fs::remove_file(&lease.path).map_storage()?;
        }
        Ok(WorkflowLeaseDisposition::Requeued)
    }

    pub fn heartbeat_lease(
        &mut self,
        event_id: &str,
        worker_id: &str,
    ) -> Result<WorkflowLeaseHeartbeat, RuntimeError> {
        let leases_dir = self.leases_dir();
        if !leases_dir.exists() {
            return Err(storage_error(format!(
                "no active lease found for workflow event '{event_id}'"
            )));
        }

        for entry in fs::read_dir(&leases_dir).map_storage()? {
            let path = entry.map_storage()?.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let (item, lease) = read_leased_queue_item(&path)?;
            if item.event.id != event_id {
                continue;
            }
            if lease.worker_id != worker_id {
                return Err(storage_error(format!(
                    "workflow event '{event_id}' is leased by '{}' not '{worker_id}'",
                    lease.worker_id
                )));
            }

            let now = SystemTime::now();
            write_leased_item(&path, &item, worker_id, now)?;
            return Ok(WorkflowLeaseHeartbeat {
                event_id: item.event.id,
                worker_id: worker_id.to_string(),
                attempt: item.attempt,
                previous_leased_at_ms: system_time_ms(lease.leased_at),
                leased_at_ms: system_time_ms(now),
            });
        }

        Err(storage_error(format!(
            "no active lease found for workflow event '{event_id}'"
        )))
    }

    fn recover_expired_leases(
        &mut self,
        options: &WorkflowLeaseOptions,
    ) -> Result<(), RuntimeError> {
        let leases_dir = self.leases_dir();
        if !leases_dir.exists() {
            return Ok(());
        }
        let now = SystemTime::now();
        for entry in fs::read_dir(&leases_dir).map_storage()? {
            let path = entry.map_storage()?.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let (item, lease) = read_leased_queue_item(&path)?;
            let expired =
                now.duration_since(lease.leased_at).unwrap_or_default() >= options.lease_timeout;
            if !expired {
                continue;
            }
            if item.attempt >= options.max_attempts {
                let dead_path = self.archive_path(&self.dead_letter_dir(), &item.event)?;
                write_dead_letter_item(
                    &dead_path,
                    &item.event,
                    item.attempt,
                    &lease.worker_id,
                    "lease expired",
                )?;
            } else {
                let retry_path = self.event_path(&item.event)?;
                write_queue_item(&retry_path, &item)?;
            }
            fs::remove_file(path).map_storage()?;
        }
        Ok(())
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

    fn archive_path(&self, dir: &Path, event: &WorkflowEvent) -> Result<PathBuf, RuntimeError> {
        fs::create_dir_all(dir).map_storage()?;
        let mut prefix = system_time_ns(SystemTime::now());
        while event_prefix_exists(dir, prefix)? {
            prefix += 1;
        }
        Ok(dir.join(format!("{prefix:030}-{}.json", safe_file_id(&event.id))))
    }
}

impl WorkflowEventQueue for FileWorkflowEventQueue {
    fn enqueue(&mut self, event: WorkflowEvent) -> Result<(), RuntimeError> {
        fs::create_dir_all(&self.dir).map_storage()?;
        let path = self.event_path(&event)?;
        let temp_path = path.with_extension("json.tmp");
        let item = WorkflowQueueItem { event, attempt: 0 };
        let bytes = serde_json::to_vec_pretty(&queue_item_to_json(&item)).map_storage()?;
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
        let item = read_queue_item(&path)?;
        fs::remove_file(path).map_storage()?;
        Ok(Some(item.event))
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

fn queue_item_to_json(item: &WorkflowQueueItem) -> Value {
    json!({
        "event": event_to_json(&item.event),
        "delivery": {
            "attempt": item.attempt,
        },
    })
}

fn leased_queue_item_to_json(
    item: &WorkflowQueueItem,
    worker_id: &str,
    leased_at: SystemTime,
) -> Value {
    json!({
        "event": event_to_json(&item.event),
        "delivery": {
            "attempt": item.attempt,
        },
        "lease": {
            "worker_id": worker_id,
            "leased_at_ms": system_time_ms(leased_at),
        },
    })
}

fn dead_letter_item_to_json(
    event: &WorkflowEvent,
    attempt: u32,
    worker_id: &str,
    error: &str,
) -> Value {
    json!({
        "event": event_to_json(event),
        "delivery": {
            "attempt": attempt,
        },
        "dead_letter": {
            "worker_id": worker_id,
            "failed_at_ms": system_time_ms(SystemTime::now()),
            "error": error,
        },
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

fn json_to_queue_item(value: &Value) -> Result<WorkflowQueueItem, RuntimeError> {
    if let Some(event) = value.get("event") {
        return Ok(WorkflowQueueItem {
            event: json_to_event(event)?,
            attempt: delivery_attempt(value)?,
        });
    }

    Ok(WorkflowQueueItem {
        event: json_to_event(value)?,
        attempt: 0,
    })
}

fn json_to_lease_metadata(value: &Value) -> Result<WorkflowLeaseMetadata, RuntimeError> {
    let lease = value
        .get("lease")
        .ok_or_else(|| storage_error("missing workflow event lease metadata"))?;
    Ok(WorkflowLeaseMetadata {
        worker_id: string_field(lease, "worker_id")?,
        leased_at: system_time_from_ms(u64_field(lease, "leased_at_ms")?),
    })
}

fn delivery_attempt(value: &Value) -> Result<u32, RuntimeError> {
    let Some(delivery) = value.get("delivery") else {
        return Ok(0);
    };
    let raw = delivery.get("attempt").and_then(Value::as_u64).unwrap_or(0);
    raw.try_into()
        .map_err(|_| storage_error("workflow event delivery attempt is too large"))
}

fn read_queue_item(path: &Path) -> Result<WorkflowQueueItem, RuntimeError> {
    let bytes = fs::read(path).map_storage()?;
    let value: Value = serde_json::from_slice(&bytes).map_storage()?;
    json_to_queue_item(&value)
}

fn read_leased_queue_item(
    path: &Path,
) -> Result<(WorkflowQueueItem, WorkflowLeaseMetadata), RuntimeError> {
    let bytes = fs::read(path).map_storage()?;
    let value: Value = serde_json::from_slice(&bytes).map_storage()?;
    Ok((json_to_queue_item(&value)?, json_to_lease_metadata(&value)?))
}

fn write_queue_item(path: &Path, item: &WorkflowQueueItem) -> Result<(), RuntimeError> {
    let temp_path = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(&queue_item_to_json(item)).map_storage()?;
    fs::write(&temp_path, bytes).map_storage()?;
    fs::rename(temp_path, path).map_storage()
}

fn write_leased_item(
    path: &Path,
    item: &WorkflowQueueItem,
    worker_id: &str,
    leased_at: SystemTime,
) -> Result<(), RuntimeError> {
    let bytes = serde_json::to_vec_pretty(&leased_queue_item_to_json(item, worker_id, leased_at))
        .map_storage()?;
    fs::write(path, bytes).map_storage()
}

fn write_dead_letter_item(
    path: &Path,
    event: &WorkflowEvent,
    attempt: u32,
    worker_id: &str,
    error: &str,
) -> Result<(), RuntimeError> {
    let bytes =
        serde_json::to_vec_pretty(&dead_letter_item_to_json(event, attempt, worker_id, error))
            .map_storage()?;
    fs::write(path, bytes).map_storage()
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
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

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

    #[test]
    fn file_workflow_event_queue_claims_and_acks_leases() {
        let root = unique_test_dir("lease-ack");
        let mut queue = FileWorkflowEventQueue::new(root.join("queue"));

        queue
            .enqueue(WorkflowEvent::wait("evt_wait", "wf_file"))
            .unwrap();

        let lease = queue.claim(&lease_options("worker_a", 3)).unwrap().unwrap();
        assert_eq!(lease.event.id, "evt_wait");
        assert_eq!(lease.worker_id, "worker_a");
        assert_eq!(lease.attempt, 1);
        assert!(queue.dequeue().unwrap().is_none());

        queue.ack(&lease).unwrap();
        assert_eq!(json_file_count(&queue.leases_dir()), 0);
        assert!(queue
            .claim(&lease_options("worker_a", 3))
            .unwrap()
            .is_none());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn file_workflow_event_queue_heartbeats_owned_leases() {
        let root = unique_test_dir("lease-heartbeat");
        let mut queue = FileWorkflowEventQueue::new(root.join("queue"));

        queue
            .enqueue(WorkflowEvent::wait("evt_wait", "wf_file"))
            .unwrap();

        let lease = queue.claim(&lease_options("worker_a", 3)).unwrap().unwrap();
        std::thread::sleep(Duration::from_millis(2));

        let heartbeat = queue.heartbeat_lease("evt_wait", "worker_a").unwrap();

        assert_eq!(heartbeat.event_id, "evt_wait");
        assert_eq!(heartbeat.worker_id, "worker_a");
        assert_eq!(heartbeat.attempt, 1);
        assert!(heartbeat.leased_at_ms >= heartbeat.previous_leased_at_ms);
        assert_eq!(heartbeat.to_json()["worker_id"], "worker_a");
        queue.ack(&lease).unwrap();

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn file_workflow_event_queue_rejects_foreign_lease_heartbeat() {
        let root = unique_test_dir("lease-heartbeat-owner");
        let mut queue = FileWorkflowEventQueue::new(root.join("queue"));

        queue
            .enqueue(WorkflowEvent::wait("evt_wait", "wf_file"))
            .unwrap();
        queue.claim(&lease_options("worker_a", 3)).unwrap().unwrap();

        let err = queue.heartbeat_lease("evt_wait", "worker_b").unwrap_err();

        assert!(format!("{err:?}").contains("leased by 'worker_a' not 'worker_b'"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn file_workflow_event_queue_retries_and_dead_letters_failed_leases() {
        let root = unique_test_dir("lease-fail");
        let mut queue = FileWorkflowEventQueue::new(root.join("queue"));

        queue
            .enqueue(WorkflowEvent::complete("evt_complete", "wf_file"))
            .unwrap();

        let first = queue.claim(&lease_options("worker_a", 2)).unwrap().unwrap();
        assert_eq!(first.attempt, 1);
        assert_eq!(
            queue.fail(&first, "workflow missing", 2).unwrap(),
            super::WorkflowLeaseDisposition::Requeued
        );

        let second = queue.claim(&lease_options("worker_b", 2)).unwrap().unwrap();
        assert_eq!(second.attempt, 2);
        assert_eq!(
            queue.fail(&second, "workflow missing", 2).unwrap(),
            super::WorkflowLeaseDisposition::DeadLettered
        );
        assert_eq!(json_file_count(&queue.dead_letter_dir()), 1);
        assert!(queue
            .claim(&lease_options("worker_c", 2))
            .unwrap()
            .is_none());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn file_workflow_event_queue_recovers_expired_leases() {
        let root = unique_test_dir("lease-expired");
        let mut queue = FileWorkflowEventQueue::new(root.join("queue"));

        queue
            .enqueue(WorkflowEvent::wait("evt_wait", "wf_file"))
            .unwrap();
        let first = queue.claim(&lease_options("worker_a", 3)).unwrap().unwrap();
        assert_eq!(first.attempt, 1);

        let recovered = queue
            .claim(&super::WorkflowLeaseOptions {
                worker_id: "worker_b".to_string(),
                lease_timeout: Duration::from_millis(0),
                max_attempts: 3,
            })
            .unwrap()
            .unwrap();

        assert_eq!(recovered.event.id, "evt_wait");
        assert_eq!(recovered.worker_id, "worker_b");
        assert_eq!(recovered.attempt, 2);
        queue.ack(&recovered).unwrap();
        assert_eq!(json_file_count(&queue.leases_dir()), 0);

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

    fn lease_options(worker_id: &str, max_attempts: u32) -> super::WorkflowLeaseOptions {
        super::WorkflowLeaseOptions {
            worker_id: worker_id.to_string(),
            lease_timeout: Duration::from_secs(30),
            max_attempts,
        }
    }

    fn json_file_count(dir: &std::path::Path) -> usize {
        fs::read_dir(dir)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .filter(|entry| {
                        entry.path().extension().and_then(|ext| ext.to_str()) == Some("json")
                    })
                    .count()
            })
            .unwrap_or(0)
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
