use crate::redaction;
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeTraceEvent {
    pub sequence: u64,
    pub timestamp: SystemTime,
    pub kind: RuntimeTraceKind,
    pub target: String,
    pub detail: Option<String>,
}

impl RuntimeTraceEvent {
    pub fn new(
        sequence: u64,
        kind: RuntimeTraceKind,
        target: impl Into<String>,
        detail: Option<String>,
    ) -> Self {
        Self {
            sequence,
            timestamp: SystemTime::now(),
            kind,
            target: target.into(),
            detail,
        }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "sequence": self.sequence,
            "timestamp_ms": system_time_ms(self.timestamp),
            "kind": self.kind.as_str(),
            "target": self.target,
            "detail": self.detail.as_ref().map(|detail| redaction::redact_text(detail)),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeTraceKind {
    WorkflowStarted,
    WorkflowCompleted,
    WorkflowFailed,
    ServiceRouteStarted,
    ServiceRouteCompleted,
    ServiceRouteFailed,
    StatementStarted,
    StatementCompleted,
    FunctionCalled,
    ActionCalled,
    ConnectorCalled,
    AuditLogged,
}

impl RuntimeTraceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            RuntimeTraceKind::WorkflowStarted => "WorkflowStarted",
            RuntimeTraceKind::WorkflowCompleted => "WorkflowCompleted",
            RuntimeTraceKind::WorkflowFailed => "WorkflowFailed",
            RuntimeTraceKind::ServiceRouteStarted => "ServiceRouteStarted",
            RuntimeTraceKind::ServiceRouteCompleted => "ServiceRouteCompleted",
            RuntimeTraceKind::ServiceRouteFailed => "ServiceRouteFailed",
            RuntimeTraceKind::StatementStarted => "StatementStarted",
            RuntimeTraceKind::StatementCompleted => "StatementCompleted",
            RuntimeTraceKind::FunctionCalled => "FunctionCalled",
            RuntimeTraceKind::ActionCalled => "ActionCalled",
            RuntimeTraceKind::ConnectorCalled => "ConnectorCalled",
            RuntimeTraceKind::AuditLogged => "AuditLogged",
        }
    }
}

fn system_time_ms(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::{RuntimeTraceEvent, RuntimeTraceKind};

    #[test]
    fn runtime_trace_event_renders_json() {
        let event = RuntimeTraceEvent::new(
            7,
            RuntimeTraceKind::WorkflowStarted,
            "main",
            Some("demo".to_string()),
        );
        let json = event.to_json();

        assert_eq!(json["sequence"], 7);
        assert_eq!(json["kind"], "WorkflowStarted");
        assert_eq!(json["target"], "main");
        assert_eq!(json["detail"], "demo");
    }
}
