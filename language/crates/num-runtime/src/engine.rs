use crate::{
    events::{WorkflowEvent, WorkflowEventKind, WorkflowEventQueue},
    tenant::TenantGuard,
    AuditEvent, AuditResult, AuditSink, RuntimeError, SecurityContext, StateStore, WorkflowId,
    WorkflowState, WorkflowStatus,
};
use std::collections::BTreeMap;
use std::time::SystemTime;

#[derive(Debug, Clone)]
pub struct WorkflowStart {
    pub id: WorkflowId,
    pub name: String,
    pub security: SecurityContext,
    pub metadata: BTreeMap<String, String>,
}

pub struct WorkflowEngine<S, A> {
    state_store: S,
    audit_sink: A,
    tenant_guard: TenantGuard,
}

impl<S, A> WorkflowEngine<S, A>
where
    S: StateStore,
    A: AuditSink,
{
    pub fn new(state_store: S, audit_sink: A) -> Self {
        Self {
            state_store,
            audit_sink,
            tenant_guard: TenantGuard::strict(),
        }
    }

    pub fn with_tenant_guard(mut self, tenant_guard: TenantGuard) -> Self {
        self.tenant_guard = tenant_guard;
        self
    }

    pub fn start_workflow(&mut self, start: WorkflowStart) -> Result<WorkflowState, RuntimeError> {
        if self.state_store.load_workflow(&start.id)?.is_some() {
            return Err(RuntimeError::Storage(format!(
                "workflow '{}' already exists",
                start.id
            )));
        }
        let now = SystemTime::now();
        let state = WorkflowState {
            id: start.id,
            name: start.name,
            status: WorkflowStatus::Running,
            security: start.security,
            started_at: now,
            updated_at: now,
            metadata: start.metadata,
        };
        self.state_store.save_workflow(state.clone())?;
        self.audit_sink
            .append(workflow_audit_event(&state, AuditResult::Started))?;
        Ok(state)
    }

    pub fn start_workflow_as(
        &mut self,
        context: &SecurityContext,
        start: WorkflowStart,
    ) -> Result<WorkflowState, RuntimeError> {
        self.tenant_guard
            .ensure_access(context, &start.security.tenant)?;
        self.start_workflow(start)
    }

    pub fn wait_workflow(&mut self, id: &str) -> Result<WorkflowState, RuntimeError> {
        self.transition_workflow(id, WorkflowStatus::Waiting, AuditResult::Waiting)
    }

    pub fn wait_workflow_as(
        &mut self,
        context: &SecurityContext,
        id: &str,
    ) -> Result<WorkflowState, RuntimeError> {
        self.transition_workflow_as(context, id, WorkflowStatus::Waiting, AuditResult::Waiting)
    }

    pub fn resume_workflow(&mut self, id: &str) -> Result<WorkflowState, RuntimeError> {
        self.transition_workflow(id, WorkflowStatus::Running, AuditResult::Resumed)
    }

    pub fn resume_workflow_as(
        &mut self,
        context: &SecurityContext,
        id: &str,
    ) -> Result<WorkflowState, RuntimeError> {
        self.transition_workflow_as(context, id, WorkflowStatus::Running, AuditResult::Resumed)
    }

    pub fn complete_workflow(&mut self, id: &str) -> Result<WorkflowState, RuntimeError> {
        self.transition_workflow(id, WorkflowStatus::Completed, AuditResult::Succeeded)
    }

    pub fn complete_workflow_as(
        &mut self,
        context: &SecurityContext,
        id: &str,
    ) -> Result<WorkflowState, RuntimeError> {
        self.transition_workflow_as(
            context,
            id,
            WorkflowStatus::Completed,
            AuditResult::Succeeded,
        )
    }

    pub fn fail_workflow(
        &mut self,
        id: &str,
        reason: impl Into<String>,
    ) -> Result<WorkflowState, RuntimeError> {
        let reason = reason.into();
        self.transition_workflow(id, WorkflowStatus::Failed, AuditResult::Failed(reason))
    }

    pub fn fail_workflow_as(
        &mut self,
        context: &SecurityContext,
        id: &str,
        reason: impl Into<String>,
    ) -> Result<WorkflowState, RuntimeError> {
        let reason = reason.into();
        self.transition_workflow_as(
            context,
            id,
            WorkflowStatus::Failed,
            AuditResult::Failed(reason),
        )
    }

    pub fn compensate_workflow(&mut self, id: &str) -> Result<WorkflowState, RuntimeError> {
        self.transition_workflow(id, WorkflowStatus::Compensated, AuditResult::RolledBack)
    }

    pub fn compensate_workflow_as(
        &mut self,
        context: &SecurityContext,
        id: &str,
    ) -> Result<WorkflowState, RuntimeError> {
        self.transition_workflow_as(
            context,
            id,
            WorkflowStatus::Compensated,
            AuditResult::RolledBack,
        )
    }

    pub fn cancel_workflow(&mut self, id: &str) -> Result<WorkflowState, RuntimeError> {
        self.transition_workflow(id, WorkflowStatus::Cancelled, AuditResult::Cancelled)
    }

    pub fn cancel_workflow_as(
        &mut self,
        context: &SecurityContext,
        id: &str,
    ) -> Result<WorkflowState, RuntimeError> {
        self.transition_workflow_as(
            context,
            id,
            WorkflowStatus::Cancelled,
            AuditResult::Cancelled,
        )
    }

    pub fn load_workflow(&self, id: &str) -> Result<Option<WorkflowState>, RuntimeError> {
        self.state_store.load_workflow(id)
    }

    pub fn load_workflow_as(
        &self,
        context: &SecurityContext,
        id: &str,
    ) -> Result<Option<WorkflowState>, RuntimeError> {
        let Some(state) = self.state_store.load_workflow(id)? else {
            return Ok(None);
        };
        self.tenant_guard
            .ensure_access(context, &state.security.tenant)?;
        Ok(Some(state))
    }

    pub fn apply_event(&mut self, event: WorkflowEvent) -> Result<WorkflowState, RuntimeError> {
        match event.kind {
            WorkflowEventKind::Start(start) => self.start_workflow(start),
            WorkflowEventKind::Wait { workflow_id } => self.wait_workflow(&workflow_id),
            WorkflowEventKind::Resume { workflow_id } => self.resume_workflow(&workflow_id),
            WorkflowEventKind::Complete { workflow_id } => self.complete_workflow(&workflow_id),
            WorkflowEventKind::Fail {
                workflow_id,
                reason,
            } => self.fail_workflow(&workflow_id, reason),
            WorkflowEventKind::Compensate { workflow_id } => self.compensate_workflow(&workflow_id),
            WorkflowEventKind::Cancel { workflow_id } => self.cancel_workflow(&workflow_id),
        }
    }

    pub fn apply_event_as(
        &mut self,
        context: &SecurityContext,
        event: WorkflowEvent,
    ) -> Result<WorkflowState, RuntimeError> {
        match event.kind {
            WorkflowEventKind::Start(start) => self.start_workflow_as(context, start),
            WorkflowEventKind::Wait { workflow_id } => self.wait_workflow_as(context, &workflow_id),
            WorkflowEventKind::Resume { workflow_id } => {
                self.resume_workflow_as(context, &workflow_id)
            }
            WorkflowEventKind::Complete { workflow_id } => {
                self.complete_workflow_as(context, &workflow_id)
            }
            WorkflowEventKind::Fail {
                workflow_id,
                reason,
            } => self.fail_workflow_as(context, &workflow_id, reason),
            WorkflowEventKind::Compensate { workflow_id } => {
                self.compensate_workflow_as(context, &workflow_id)
            }
            WorkflowEventKind::Cancel { workflow_id } => {
                self.cancel_workflow_as(context, &workflow_id)
            }
        }
    }

    pub fn process_next_event<Q>(
        &mut self,
        queue: &mut Q,
    ) -> Result<Option<WorkflowState>, RuntimeError>
    where
        Q: WorkflowEventQueue,
    {
        let Some(event) = queue.dequeue()? else {
            return Ok(None);
        };
        self.apply_event(event).map(Some)
    }

    pub fn process_next_event_as<Q>(
        &mut self,
        context: &SecurityContext,
        queue: &mut Q,
    ) -> Result<Option<WorkflowState>, RuntimeError>
    where
        Q: WorkflowEventQueue,
    {
        let Some(event) = queue.dequeue()? else {
            return Ok(None);
        };
        self.apply_event_as(context, event).map(Some)
    }

    pub fn into_parts(self) -> (S, A) {
        (self.state_store, self.audit_sink)
    }

    fn transition_workflow(
        &mut self,
        id: &str,
        status: WorkflowStatus,
        result: AuditResult,
    ) -> Result<WorkflowState, RuntimeError> {
        let mut state = self
            .state_store
            .load_workflow(id)?
            .ok_or_else(|| RuntimeError::Storage(format!("workflow '{id}' not found")))?;
        validate_transition(&state.status, &status)?;
        state.status = status;
        state.updated_at = SystemTime::now();
        self.state_store.save_workflow(state.clone())?;
        self.audit_sink
            .append(workflow_audit_event(&state, result))?;
        Ok(state)
    }

    fn transition_workflow_as(
        &mut self,
        context: &SecurityContext,
        id: &str,
        status: WorkflowStatus,
        result: AuditResult,
    ) -> Result<WorkflowState, RuntimeError> {
        let state = self
            .state_store
            .load_workflow(id)?
            .ok_or_else(|| RuntimeError::Storage(format!("workflow '{id}' not found")))?;
        self.tenant_guard
            .ensure_access(context, &state.security.tenant)?;
        self.transition_workflow(id, status, result)
    }
}

fn workflow_audit_event(state: &WorkflowState, result: AuditResult) -> AuditEvent {
    let result_name = match &result {
        AuditResult::Started => "started",
        AuditResult::Waiting => "waiting",
        AuditResult::Resumed => "resumed",
        AuditResult::Succeeded => "succeeded",
        AuditResult::Failed(_) => "failed",
        AuditResult::RolledBack => "rolled_back",
        AuditResult::Cancelled => "cancelled",
    };
    AuditEvent {
        event_id: format!("{}:{result_name}", state.id),
        timestamp: SystemTime::now(),
        actor: state.security.actor.clone(),
        tenant: state.security.tenant.clone(),
        action: state.name.clone(),
        result,
        permissions_used: state.security.permissions.iter().cloned().collect(),
        data_sources: Vec::new(),
        ai_models: Vec::new(),
        confidence_values: Vec::new(),
        rollback_status: None,
        correlation_id: state.security.correlation_id.clone(),
        request_id: state.security.request_id.clone(),
    }
}

fn validate_transition(
    current: &WorkflowStatus,
    next: &WorkflowStatus,
) -> Result<(), RuntimeError> {
    let allowed = matches!(
        (current, next),
        (WorkflowStatus::Created, WorkflowStatus::Running)
            | (WorkflowStatus::Running, WorkflowStatus::Waiting)
            | (WorkflowStatus::Waiting, WorkflowStatus::Running)
            | (WorkflowStatus::Running, WorkflowStatus::Completed)
            | (WorkflowStatus::Running, WorkflowStatus::Failed)
            | (WorkflowStatus::Waiting, WorkflowStatus::Failed)
            | (WorkflowStatus::Running, WorkflowStatus::Cancelled)
            | (WorkflowStatus::Waiting, WorkflowStatus::Cancelled)
            | (WorkflowStatus::Failed, WorkflowStatus::Compensated)
    );
    if allowed {
        Ok(())
    } else {
        Err(RuntimeError::Storage(format!(
            "invalid workflow status transition {current:?} -> {next:?}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::{WorkflowEngine, WorkflowStart};
    use crate::events::{FileWorkflowEventQueue, WorkflowEvent, WorkflowEventQueue};
    use crate::storage::{FileAuditSink, FileStateStore};
    use crate::tenant::TenantGuard;
    use crate::{RuntimeError, SecurityContext, WorkflowStatus};
    use std::collections::{BTreeMap, BTreeSet};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn engine_persists_start_and_completion() {
        let root = unique_test_dir("complete");
        let state_store = FileStateStore::new(&root);
        let audit_path = root.join("audit/events.jsonl");
        let audit_sink = FileAuditSink::new(&audit_path);
        let mut engine = WorkflowEngine::new(state_store, audit_sink);

        let started = engine
            .start_workflow(workflow_start("wf_complete"))
            .unwrap();
        assert_eq!(started.status, WorkflowStatus::Running);
        let completed = engine.complete_workflow("wf_complete").unwrap();
        assert_eq!(completed.status, WorkflowStatus::Completed);

        let loaded = engine.load_workflow("wf_complete").unwrap().unwrap();
        assert_eq!(loaded.status, WorkflowStatus::Completed);
        let events = fs::read_to_string(&audit_path).unwrap();
        assert!(events.contains("\"kind\":\"Started\""));
        assert!(events.contains("\"kind\":\"Succeeded\""));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn engine_persists_failure_reason() {
        let root = unique_test_dir("fail");
        let state_store = FileStateStore::new(&root);
        let audit_path = root.join("audit/events.jsonl");
        let audit_sink = FileAuditSink::new(&audit_path);
        let mut engine = WorkflowEngine::new(state_store, audit_sink);

        engine.start_workflow(workflow_start("wf_fail")).unwrap();
        let failed = engine
            .fail_workflow("wf_fail", "connector timeout")
            .unwrap();

        assert_eq!(failed.status, WorkflowStatus::Failed);
        let events = fs::read_to_string(&audit_path).unwrap();
        assert!(events.contains("\"kind\":\"Failed\""));
        assert!(events.contains("\"reason\":\"connector timeout\""));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn engine_waits_and_resumes_workflow() {
        let root = unique_test_dir("wait-resume");
        let state_store = FileStateStore::new(&root);
        let audit_path = root.join("audit/events.jsonl");
        let audit_sink = FileAuditSink::new(&audit_path);
        let mut engine = WorkflowEngine::new(state_store, audit_sink);

        engine.start_workflow(workflow_start("wf_wait")).unwrap();
        let waiting = engine.wait_workflow("wf_wait").unwrap();
        assert_eq!(waiting.status, WorkflowStatus::Waiting);
        let running = engine.resume_workflow("wf_wait").unwrap();
        assert_eq!(running.status, WorkflowStatus::Running);

        let events = fs::read_to_string(&audit_path).unwrap();
        assert!(events.contains("\"kind\":\"Waiting\""));
        assert!(events.contains("\"kind\":\"Resumed\""));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn engine_compensates_failed_workflow() {
        let root = unique_test_dir("compensate");
        let state_store = FileStateStore::new(&root);
        let audit_path = root.join("audit/events.jsonl");
        let audit_sink = FileAuditSink::new(&audit_path);
        let mut engine = WorkflowEngine::new(state_store, audit_sink);

        engine
            .start_workflow(workflow_start("wf_compensate"))
            .unwrap();
        engine
            .fail_workflow("wf_compensate", "payment failed")
            .unwrap();
        let compensated = engine.compensate_workflow("wf_compensate").unwrap();

        assert_eq!(compensated.status, WorkflowStatus::Compensated);
        let events = fs::read_to_string(&audit_path).unwrap();
        assert!(events.contains("\"kind\":\"RolledBack\""));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn engine_cancels_waiting_workflow() {
        let root = unique_test_dir("cancel");
        let state_store = FileStateStore::new(&root);
        let audit_path = root.join("audit/events.jsonl");
        let audit_sink = FileAuditSink::new(&audit_path);
        let mut engine = WorkflowEngine::new(state_store, audit_sink);

        engine.start_workflow(workflow_start("wf_cancel")).unwrap();
        engine.wait_workflow("wf_cancel").unwrap();
        let cancelled = engine.cancel_workflow("wf_cancel").unwrap();

        assert_eq!(cancelled.status, WorkflowStatus::Cancelled);
        let events = fs::read_to_string(&audit_path).unwrap();
        assert!(events.contains("\"kind\":\"Cancelled\""));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn engine_rejects_duplicate_start() {
        let root = unique_test_dir("duplicate");
        let state_store = FileStateStore::new(&root);
        let audit_sink = FileAuditSink::new(root.join("audit/events.jsonl"));
        let mut engine = WorkflowEngine::new(state_store, audit_sink);

        engine
            .start_workflow(workflow_start("wf_duplicate"))
            .unwrap();
        let err = engine
            .start_workflow(workflow_start("wf_duplicate"))
            .unwrap_err();

        assert!(format!("{err:?}").contains("already exists"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn engine_rejects_invalid_terminal_transition() {
        let root = unique_test_dir("invalid-transition");
        let state_store = FileStateStore::new(&root);
        let audit_sink = FileAuditSink::new(root.join("audit/events.jsonl"));
        let mut engine = WorkflowEngine::new(state_store, audit_sink);

        engine
            .start_workflow(workflow_start("wf_terminal"))
            .unwrap();
        engine.complete_workflow("wf_terminal").unwrap();
        let err = engine.fail_workflow("wf_terminal", "too late").unwrap_err();

        assert!(format!("{err:?}").contains("invalid workflow status transition"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn engine_rejects_transition_for_missing_workflow() {
        let root = unique_test_dir("missing");
        let state_store = FileStateStore::new(&root);
        let audit_sink = FileAuditSink::new(root.join("audit/events.jsonl"));
        let mut engine = WorkflowEngine::new(state_store, audit_sink);

        let err = engine.complete_workflow("missing").unwrap_err();
        assert!(format!("{err:?}").contains("workflow 'missing' not found"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn engine_processes_queued_workflow_events() {
        let root = unique_test_dir("queued-events");
        let state_store = FileStateStore::new(&root);
        let audit_path = root.join("audit/events.jsonl");
        let audit_sink = FileAuditSink::new(&audit_path);
        let mut engine = WorkflowEngine::new(state_store, audit_sink);
        let mut queue = FileWorkflowEventQueue::new(root.join("events"));

        queue
            .enqueue(WorkflowEvent::start("evt_1", workflow_start("wf_queued")))
            .unwrap();
        queue
            .enqueue(WorkflowEvent::wait("evt_2", "wf_queued"))
            .unwrap();
        queue
            .enqueue(WorkflowEvent::resume("evt_3", "wf_queued"))
            .unwrap();
        queue
            .enqueue(WorkflowEvent::complete("evt_4", "wf_queued"))
            .unwrap();

        assert_eq!(
            engine
                .process_next_event(&mut queue)
                .unwrap()
                .unwrap()
                .status,
            WorkflowStatus::Running
        );
        assert_eq!(
            engine
                .process_next_event(&mut queue)
                .unwrap()
                .unwrap()
                .status,
            WorkflowStatus::Waiting
        );
        assert_eq!(
            engine
                .process_next_event(&mut queue)
                .unwrap()
                .unwrap()
                .status,
            WorkflowStatus::Running
        );
        assert_eq!(
            engine
                .process_next_event(&mut queue)
                .unwrap()
                .unwrap()
                .status,
            WorkflowStatus::Completed
        );
        assert!(engine.process_next_event(&mut queue).unwrap().is_none());

        let loaded = engine.load_workflow("wf_queued").unwrap().unwrap();
        assert_eq!(loaded.status, WorkflowStatus::Completed);
        let events = fs::read_to_string(&audit_path).unwrap();
        assert!(events.contains("\"kind\":\"Started\""));
        assert!(events.contains("\"kind\":\"Waiting\""));
        assert!(events.contains("\"kind\":\"Resumed\""));
        assert!(events.contains("\"kind\":\"Succeeded\""));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn engine_tenant_aware_load_and_transition_allow_same_tenant() {
        let root = unique_test_dir("tenant-allow");
        let state_store = FileStateStore::new(&root);
        let audit_sink = FileAuditSink::new(root.join("audit/events.jsonl"));
        let mut engine = WorkflowEngine::new(state_store, audit_sink);
        let context = security_context("tenant_1");

        engine.start_workflow(workflow_start("wf_tenant")).unwrap();
        let loaded = engine
            .load_workflow_as(&context, "wf_tenant")
            .unwrap()
            .unwrap();
        assert_eq!(loaded.security.tenant, "tenant_1");

        let completed = engine.complete_workflow_as(&context, "wf_tenant").unwrap();
        assert_eq!(completed.status, WorkflowStatus::Completed);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn engine_tenant_aware_transition_rejects_cross_tenant_access() {
        let root = unique_test_dir("tenant-deny");
        let state_store = FileStateStore::new(&root);
        let audit_sink = FileAuditSink::new(root.join("audit/events.jsonl"));
        let mut engine = WorkflowEngine::new(state_store, audit_sink);
        let context = security_context("tenant_2");

        engine.start_workflow(workflow_start("wf_tenant")).unwrap();
        let error = engine
            .complete_workflow_as(&context, "wf_tenant")
            .unwrap_err();

        assert!(matches!(
            error,
            RuntimeError::TenantIsolationViolation { .. }
        ));
        assert_eq!(
            engine.load_workflow("wf_tenant").unwrap().unwrap().status,
            WorkflowStatus::Running
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn engine_tenant_aware_start_rejects_cross_tenant_access() {
        let root = unique_test_dir("tenant-start-deny");
        let state_store = FileStateStore::new(&root);
        let audit_sink = FileAuditSink::new(root.join("audit/events.jsonl"));
        let mut engine = WorkflowEngine::new(state_store, audit_sink);
        let context = security_context("tenant_2");

        let error = engine
            .start_workflow_as(&context, workflow_start("wf_tenant"))
            .unwrap_err();

        assert!(matches!(
            error,
            RuntimeError::TenantIsolationViolation { .. }
        ));
        assert!(engine.load_workflow("wf_tenant").unwrap().is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn engine_tenant_aware_event_processing_rejects_cross_tenant_transition() {
        let root = unique_test_dir("tenant-event-deny");
        let state_store = FileStateStore::new(&root);
        let audit_sink = FileAuditSink::new(root.join("audit/events.jsonl"));
        let mut engine = WorkflowEngine::new(state_store, audit_sink);
        let mut queue = FileWorkflowEventQueue::new(root.join("events"));

        engine.start_workflow(workflow_start("wf_tenant")).unwrap();
        queue
            .enqueue(WorkflowEvent::complete("evt_cross_tenant", "wf_tenant"))
            .unwrap();

        let error = engine
            .process_next_event_as(&security_context("tenant_2"), &mut queue)
            .unwrap_err();

        assert!(matches!(
            error,
            RuntimeError::TenantIsolationViolation { .. }
        ));
        assert_eq!(
            engine.load_workflow("wf_tenant").unwrap().unwrap().status,
            WorkflowStatus::Running
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn engine_tenant_aware_event_processing_allows_same_tenant_transition() {
        let root = unique_test_dir("tenant-event-allow");
        let state_store = FileStateStore::new(&root);
        let audit_sink = FileAuditSink::new(root.join("audit/events.jsonl"));
        let mut engine = WorkflowEngine::new(state_store, audit_sink);
        let mut queue = FileWorkflowEventQueue::new(root.join("events"));

        engine.start_workflow(workflow_start("wf_tenant")).unwrap();
        queue
            .enqueue(WorkflowEvent::complete("evt_same_tenant", "wf_tenant"))
            .unwrap();

        let completed = engine
            .process_next_event_as(&security_context("tenant_1"), &mut queue)
            .unwrap()
            .unwrap();

        assert_eq!(completed.status, WorkflowStatus::Completed);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn engine_can_disable_tenant_guard_for_demo_mode() {
        let root = unique_test_dir("tenant-disabled");
        let state_store = FileStateStore::new(&root);
        let audit_sink = FileAuditSink::new(root.join("audit/events.jsonl"));
        let mut engine =
            WorkflowEngine::new(state_store, audit_sink).with_tenant_guard(TenantGuard::disabled());
        let context = security_context("tenant_2");

        engine.start_workflow(workflow_start("wf_tenant")).unwrap();
        let completed = engine.complete_workflow_as(&context, "wf_tenant").unwrap();

        assert_eq!(completed.status, WorkflowStatus::Completed);
        let _ = fs::remove_dir_all(root);
    }

    fn workflow_start(id: &str) -> WorkflowStart {
        let mut permissions = BTreeSet::new();
        permissions.insert("IssueRefund".to_string());
        let mut metadata = BTreeMap::new();
        metadata.insert("source".to_string(), "engine-test".to_string());
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

    fn security_context(tenant: &str) -> SecurityContext {
        let mut permissions = BTreeSet::new();
        permissions.insert("IssueRefund".to_string());
        SecurityContext {
            actor: "agent@example.com".to_string(),
            tenant: tenant.to_string(),
            permissions,
            correlation_id: "corr_1".to_string(),
            request_id: "req_1".to_string(),
        }
    }

    fn unique_test_dir(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "num-engine-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
}
