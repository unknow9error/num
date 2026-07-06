use crate::engine::WorkflowEngine;
use crate::events::{
    FileWorkflowEventQueue, WorkflowEvent, WorkflowEventKind, WorkflowEventQueue,
    WorkflowLeaseDisposition, WorkflowLeaseOptions,
};
use crate::{AuditSink, RuntimeError, StateStore, WorkflowState, WorkflowStatus};
use serde_json::{json, Value};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkflowDrainOptions {
    pub max_events: Option<usize>,
    pub stop_on_error: bool,
}

impl Default for WorkflowDrainOptions {
    fn default() -> Self {
        Self {
            max_events: None,
            stop_on_error: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowLeasedDrainOptions {
    pub max_events: Option<usize>,
    pub stop_on_error: bool,
    pub worker_id: String,
    pub lease_timeout: Duration,
    pub max_attempts: u32,
}

impl Default for WorkflowLeasedDrainOptions {
    fn default() -> Self {
        Self {
            max_events: None,
            stop_on_error: true,
            worker_id: "local-worker".to_string(),
            lease_timeout: Duration::from_secs(30),
            max_attempts: 3,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowWorkerReport {
    pub processed: usize,
    pub failed: usize,
    pub retried: usize,
    pub dead_lettered: usize,
    pub idle: bool,
    pub states: Vec<WorkflowWorkerState>,
    pub failures: Vec<WorkflowWorkerFailure>,
}

impl WorkflowWorkerReport {
    fn empty() -> Self {
        Self {
            processed: 0,
            failed: 0,
            retried: 0,
            dead_lettered: 0,
            idle: true,
            states: Vec::new(),
            failures: Vec::new(),
        }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "processed": self.processed,
            "failed": self.failed,
            "retried": self.retried,
            "dead_lettered": self.dead_lettered,
            "idle": self.idle,
            "states": self.states.iter().map(WorkflowWorkerState::to_json).collect::<Vec<_>>(),
            "failures": self.failures.iter().map(WorkflowWorkerFailure::to_json).collect::<Vec<_>>(),
        })
    }

    pub fn render_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("processed: {}\n", self.processed));
        out.push_str(&format!("failed: {}\n", self.failed));
        out.push_str(&format!("retried: {}\n", self.retried));
        out.push_str(&format!("dead_lettered: {}\n", self.dead_lettered));
        out.push_str(&format!("idle: {}\n", self.idle));
        if !self.states.is_empty() {
            out.push_str("states:\n");
            for state in &self.states {
                out.push_str(&format!(
                    "  - event={} workflow={} name={} status={}\n",
                    state.event_id, state.workflow_id, state.workflow_name, state.status
                ));
            }
        }
        if !self.failures.is_empty() {
            out.push_str("failures:\n");
            for failure in &self.failures {
                out.push_str(&format!(
                    "  - event={} kind={} attempt={} disposition={} error={}\n",
                    failure.event_id,
                    failure.event_kind,
                    failure.attempt,
                    failure.disposition.as_deref().unwrap_or("failed"),
                    failure.error
                ));
            }
        }
        out
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowWorkerState {
    pub event_id: String,
    pub event_kind: String,
    pub workflow_id: String,
    pub workflow_name: String,
    pub status: String,
}

impl WorkflowWorkerState {
    fn from_workflow(event: &WorkflowEvent, state: WorkflowState) -> Self {
        Self {
            event_id: event.id.clone(),
            event_kind: event_kind_label(&event.kind).to_string(),
            workflow_id: state.id,
            workflow_name: state.name,
            status: workflow_status_label(&state.status).to_string(),
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "event_id": self.event_id,
            "event_kind": self.event_kind,
            "workflow_id": self.workflow_id,
            "workflow_name": self.workflow_name,
            "status": self.status,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowWorkerFailure {
    pub event_id: String,
    pub event_kind: String,
    pub attempt: u32,
    pub disposition: Option<String>,
    pub error: String,
}

impl WorkflowWorkerFailure {
    fn from_error(event: &WorkflowEvent, error: RuntimeError) -> Self {
        Self {
            event_id: event.id.clone(),
            event_kind: event_kind_label(&event.kind).to_string(),
            attempt: 1,
            disposition: None,
            error: format!("{error:?}"),
        }
    }

    fn from_leased_error(
        event: &WorkflowEvent,
        attempt: u32,
        disposition: WorkflowLeaseDisposition,
        error: RuntimeError,
    ) -> Self {
        Self {
            event_id: event.id.clone(),
            event_kind: event_kind_label(&event.kind).to_string(),
            attempt,
            disposition: Some(disposition_label(disposition).to_string()),
            error: format!("{error:?}"),
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "event_id": self.event_id,
            "event_kind": self.event_kind,
            "attempt": self.attempt,
            "disposition": self.disposition,
            "error": self.error,
        })
    }
}

impl<S, A> WorkflowWorker<S, A, FileWorkflowEventQueue>
where
    S: StateStore,
    A: AuditSink,
{
    pub fn drain_leased(
        &mut self,
        options: WorkflowLeasedDrainOptions,
    ) -> Result<WorkflowWorkerReport, RuntimeError> {
        let mut report = WorkflowWorkerReport::empty();
        let lease_options = WorkflowLeaseOptions {
            worker_id: options.worker_id,
            lease_timeout: options.lease_timeout,
            max_attempts: options.max_attempts,
        };

        loop {
            if options
                .max_events
                .is_some_and(|max_events| report.processed + report.failed >= max_events)
            {
                break;
            }
            let Some(lease) = self.queue.claim(&lease_options)? else {
                break;
            };
            report.idle = false;
            match self.engine.apply_event(lease.event.clone()) {
                Ok(state) => {
                    self.queue.ack(&lease)?;
                    report.processed += 1;
                    report
                        .states
                        .push(WorkflowWorkerState::from_workflow(&lease.event, state));
                }
                Err(error) => {
                    let error_text = format!("{error:?}");
                    let disposition =
                        self.queue
                            .fail(&lease, &error_text, lease_options.max_attempts)?;
                    match disposition {
                        WorkflowLeaseDisposition::Requeued => report.retried += 1,
                        WorkflowLeaseDisposition::DeadLettered => report.dead_lettered += 1,
                    }
                    report.failed += 1;
                    report
                        .failures
                        .push(WorkflowWorkerFailure::from_leased_error(
                            &lease.event,
                            lease.attempt,
                            disposition,
                            error,
                        ));
                    if options.stop_on_error {
                        break;
                    }
                }
            }
        }

        Ok(report)
    }
}

pub struct WorkflowWorker<S, A, Q> {
    engine: WorkflowEngine<S, A>,
    queue: Q,
}

impl<S, A, Q> WorkflowWorker<S, A, Q>
where
    S: StateStore,
    A: AuditSink,
    Q: WorkflowEventQueue,
{
    pub fn new(engine: WorkflowEngine<S, A>, queue: Q) -> Self {
        Self { engine, queue }
    }

    pub fn drain(
        &mut self,
        options: WorkflowDrainOptions,
    ) -> Result<WorkflowWorkerReport, RuntimeError> {
        let mut report = WorkflowWorkerReport::empty();

        loop {
            if options
                .max_events
                .is_some_and(|max_events| report.processed + report.failed >= max_events)
            {
                break;
            }
            let Some(event) = self.queue.dequeue()? else {
                break;
            };
            report.idle = false;
            match self.engine.apply_event(event.clone()) {
                Ok(state) => {
                    report.processed += 1;
                    report
                        .states
                        .push(WorkflowWorkerState::from_workflow(&event, state));
                }
                Err(error) => {
                    report.failed += 1;
                    report
                        .failures
                        .push(WorkflowWorkerFailure::from_error(&event, error));
                    if options.stop_on_error {
                        break;
                    }
                }
            }
        }

        Ok(report)
    }

    pub fn into_parts(self) -> (WorkflowEngine<S, A>, Q) {
        (self.engine, self.queue)
    }
}

fn event_kind_label(kind: &WorkflowEventKind) -> &'static str {
    match kind {
        WorkflowEventKind::Start(_) => "Start",
        WorkflowEventKind::Wait { .. } => "Wait",
        WorkflowEventKind::Resume { .. } => "Resume",
        WorkflowEventKind::Complete { .. } => "Complete",
        WorkflowEventKind::Fail { .. } => "Fail",
        WorkflowEventKind::Compensate { .. } => "Compensate",
        WorkflowEventKind::Cancel { .. } => "Cancel",
    }
}

fn workflow_status_label(status: &WorkflowStatus) -> &'static str {
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

fn disposition_label(disposition: WorkflowLeaseDisposition) -> &'static str {
    match disposition {
        WorkflowLeaseDisposition::Requeued => "requeued",
        WorkflowLeaseDisposition::DeadLettered => "dead_lettered",
    }
}

#[cfg(test)]
mod tests {
    use super::{WorkflowDrainOptions, WorkflowLeasedDrainOptions, WorkflowWorker};
    use crate::engine::{WorkflowEngine, WorkflowStart};
    use crate::events::{
        FileWorkflowEventQueue, MemoryWorkflowEventQueue, WorkflowEvent, WorkflowEventQueue,
    };
    use crate::storage::{FileAuditSink, FileStateStore};
    use crate::{SecurityContext, WorkflowStatus};
    use std::collections::{BTreeMap, BTreeSet};
    use std::fs;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[test]
    fn worker_drains_queued_events() {
        let root = unique_test_dir("drain");
        let state_store = FileStateStore::new(&root);
        let audit_sink = FileAuditSink::new(root.join("audit/events.jsonl"));
        let engine = WorkflowEngine::new(state_store, audit_sink);
        let mut queue = MemoryWorkflowEventQueue::new();

        queue
            .enqueue(WorkflowEvent::start("evt_1", workflow_start("wf_1")))
            .unwrap();
        queue.enqueue(WorkflowEvent::wait("evt_2", "wf_1")).unwrap();
        queue
            .enqueue(WorkflowEvent::resume("evt_3", "wf_1"))
            .unwrap();
        queue
            .enqueue(WorkflowEvent::complete("evt_4", "wf_1"))
            .unwrap();

        let mut worker = WorkflowWorker::new(engine, queue);
        let report = worker.drain(WorkflowDrainOptions::default()).unwrap();

        assert_eq!(report.processed, 4);
        assert_eq!(report.failed, 0);
        assert!(!report.idle);
        assert_eq!(report.states[3].status, "Completed");
        assert_eq!(report.to_json()["processed"], 4);
        assert!(report.render_text().contains("workflow=wf_1"));

        let (engine, _queue) = worker.into_parts();
        assert_eq!(
            engine.load_workflow("wf_1").unwrap().unwrap().status,
            WorkflowStatus::Completed
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn worker_reports_failed_events_without_losing_batch_context() {
        let root = unique_test_dir("fail");
        let state_store = FileStateStore::new(&root);
        let audit_sink = FileAuditSink::new(root.join("audit/events.jsonl"));
        let engine = WorkflowEngine::new(state_store, audit_sink);
        let mut queue = MemoryWorkflowEventQueue::new();

        queue
            .enqueue(WorkflowEvent::start("evt_1", workflow_start("wf_1")))
            .unwrap();
        queue
            .enqueue(WorkflowEvent::complete("evt_2", "wf_1"))
            .unwrap();
        queue
            .enqueue(WorkflowEvent::fail("evt_3", "wf_1", "too late"))
            .unwrap();

        let mut worker = WorkflowWorker::new(engine, queue);
        let report = worker
            .drain(WorkflowDrainOptions {
                max_events: None,
                stop_on_error: false,
            })
            .unwrap();

        assert_eq!(report.processed, 2);
        assert_eq!(report.failed, 1);
        assert_eq!(report.failures[0].event_id, "evt_3");
        assert!(report.failures[0].error.contains("invalid workflow status"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn worker_respects_max_events() {
        let root = unique_test_dir("max");
        let state_store = FileStateStore::new(&root);
        let audit_sink = FileAuditSink::new(root.join("audit/events.jsonl"));
        let engine = WorkflowEngine::new(state_store, audit_sink);
        let mut queue = MemoryWorkflowEventQueue::new();

        queue
            .enqueue(WorkflowEvent::start("evt_1", workflow_start("wf_1")))
            .unwrap();
        queue.enqueue(WorkflowEvent::wait("evt_2", "wf_1")).unwrap();

        let mut worker = WorkflowWorker::new(engine, queue);
        let report = worker
            .drain(WorkflowDrainOptions {
                max_events: Some(1),
                stop_on_error: true,
            })
            .unwrap();

        assert_eq!(report.processed, 1);
        assert_eq!(report.failed, 0);
        let (_engine, queue) = worker.into_parts();
        assert_eq!(queue.len(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn leased_file_worker_retries_then_dead_letters_failed_events() {
        let root = unique_test_dir("leased");
        let state_store = FileStateStore::new(&root);
        let audit_sink = FileAuditSink::new(root.join("audit/events.jsonl"));
        let engine = WorkflowEngine::new(state_store, audit_sink);
        let mut queue = FileWorkflowEventQueue::new(root.join("events"));

        queue
            .enqueue(WorkflowEvent::start("evt_1", workflow_start("wf_1")))
            .unwrap();
        queue
            .enqueue(WorkflowEvent::complete("evt_2", "wf_1"))
            .unwrap();
        queue
            .enqueue(WorkflowEvent::fail("evt_3", "wf_1", "too late"))
            .unwrap();

        let mut worker = WorkflowWorker::new(engine, queue);
        let first = worker
            .drain_leased(WorkflowLeasedDrainOptions {
                max_events: Some(3),
                stop_on_error: false,
                worker_id: "worker_a".to_string(),
                lease_timeout: Duration::from_secs(30),
                max_attempts: 2,
            })
            .unwrap();

        assert_eq!(first.processed, 2);
        assert_eq!(first.failed, 1);
        assert_eq!(first.retried, 1);
        assert_eq!(first.dead_lettered, 0);
        assert_eq!(first.failures[0].attempt, 1);
        assert_eq!(first.failures[0].disposition.as_deref(), Some("requeued"));

        let second = worker
            .drain_leased(WorkflowLeasedDrainOptions {
                max_events: Some(1),
                stop_on_error: false,
                worker_id: "worker_b".to_string(),
                lease_timeout: Duration::from_secs(30),
                max_attempts: 2,
            })
            .unwrap();

        assert_eq!(second.processed, 0);
        assert_eq!(second.failed, 1);
        assert_eq!(second.retried, 0);
        assert_eq!(second.dead_lettered, 1);
        assert_eq!(second.failures[0].attempt, 2);
        assert_eq!(
            second.failures[0].disposition.as_deref(),
            Some("dead_lettered")
        );
        let (_engine, queue) = worker.into_parts();
        assert_eq!(json_file_count(&queue.dead_letter_dir()), 1);
        let _ = fs::remove_dir_all(root);
    }

    fn workflow_start(id: &str) -> WorkflowStart {
        let mut permissions = BTreeSet::new();
        permissions.insert("IssueRefund".to_string());
        let mut metadata = BTreeMap::new();
        metadata.insert("source".to_string(), "worker-test".to_string());
        WorkflowStart {
            id: id.to_string(),
            name: "process_refund".to_string(),
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
            metadata,
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
            "num-worker-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
}
