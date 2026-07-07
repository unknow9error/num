use crate::observability::{RuntimeTraceEvent, RuntimeTraceKind};
use crate::redaction;
use crate::RuntimeError;
use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BreakpointSpec {
    pub kind: RuntimeTraceKind,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BreakpointHit {
    pub breakpoint: BreakpointSpec,
    pub event: RuntimeTraceEvent,
}

#[derive(Debug, Clone)]
pub struct DebugReport {
    pub workflow: String,
    pub result: Result<(), String>,
    pub runtime_error: Option<RuntimeError>,
    pub breakpoints: Vec<BreakpointSpec>,
    pub hits: Vec<BreakpointHit>,
    pub trace: Vec<RuntimeTraceEvent>,
}

impl BreakpointSpec {
    pub fn parse(raw: &str) -> Result<Self, String> {
        let Some((kind, target)) = raw.split_once(':') else {
            return Err(format!(
                "invalid breakpoint `{raw}`; expected kind:target, e.g. action:issue_refund"
            ));
        };
        let kind = parse_breakpoint_kind(kind.trim())?;
        let target = normalize_target(target);
        if target.is_empty() {
            return Err(format!(
                "invalid breakpoint `{raw}`; target cannot be empty"
            ));
        }
        Ok(Self { kind, target })
    }

    pub fn matches(&self, event: &RuntimeTraceEvent) -> bool {
        self.kind == event.kind && normalize_target(&event.target) == self.target
    }

    pub fn label(&self) -> String {
        format!("{}:{}", self.kind.as_str(), self.target)
    }

    pub fn to_json(&self) -> Value {
        json!({
            "kind": self.kind.as_str(),
            "target": self.target,
        })
    }
}

impl DebugReport {
    pub fn from_trace(
        workflow: impl Into<String>,
        result: Result<(), String>,
        runtime_error: Option<RuntimeError>,
        breakpoints: Vec<BreakpointSpec>,
        trace: &[RuntimeTraceEvent],
    ) -> Self {
        let mut hits = Vec::new();
        for event in trace {
            for breakpoint in &breakpoints {
                if breakpoint.matches(event) {
                    hits.push(BreakpointHit {
                        breakpoint: breakpoint.clone(),
                        event: event.clone(),
                    });
                }
            }
        }

        Self {
            workflow: workflow.into(),
            result,
            runtime_error,
            breakpoints,
            hits,
            trace: trace.to_vec(),
        }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "workflow": self.workflow,
            "status": if self.result.is_ok() { "completed" } else { "failed" },
            "error": self.result.as_ref().err().map(|error| redaction::redact_text(error)),
            "runtime_error": self.runtime_error.as_ref().map(RuntimeError::to_json),
            "breakpoints": self.breakpoints.iter().map(BreakpointSpec::to_json).collect::<Vec<_>>(),
            "hits": self.hits.iter().map(BreakpointHit::to_json).collect::<Vec<_>>(),
            "trace": self.trace.iter().map(RuntimeTraceEvent::to_json).collect::<Vec<_>>(),
            "debug_adapter": self.to_adapter_json(),
        })
    }

    pub fn to_adapter_json(&self) -> Value {
        json!({
            "protocol": "num.debug.adapter.v1",
            "session": {
                "workflow": self.workflow,
                "status": if self.result.is_ok() { "completed" } else { "failed" },
                "error": self.result.as_ref().err().map(|error| redaction::redact_text(error)),
                "runtime_error": self.runtime_error.as_ref().map(RuntimeError::to_json),
            },
            "capabilities": {
                "supports_breakpoints": true,
                "supports_stack_frames": true,
                "supports_scopes": true,
                "supports_variables": true,
                "supports_continue": false,
                "supports_next": false,
                "supports_step_in": false,
                "supports_step_out": false,
                "unsupported_requests": [
                    "continue",
                    "next",
                    "stepIn",
                    "stepOut",
                    "pause",
                    "setVariable"
                ],
                "execution_model": "scripted trace replay; breakpoints are reported as hits after workflow execution",
            },
            "threads": [{
                "id": 1,
                "name": format!("workflow:{}", self.workflow),
            }],
            "breakpoints": self.breakpoints.iter().enumerate().map(|(index, breakpoint)| {
                adapter_breakpoint(index + 1, breakpoint)
            }).collect::<Vec<_>>(),
            "stopped_events": self.hits.iter().filter_map(|hit| {
                let breakpoint_id = self.breakpoints.iter().position(|breakpoint| {
                    breakpoint == &hit.breakpoint
                })? + 1;
                Some(adapter_stopped_event(breakpoint_id, hit))
            }).collect::<Vec<_>>(),
            "stack_frames": self.trace.iter().map(adapter_stack_frame).collect::<Vec<_>>(),
            "scopes": self.trace.iter().map(adapter_scopes).collect::<Vec<_>>(),
        })
    }

    pub fn render_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("Debug session: workflow {}\n", self.workflow));
        match &self.result {
            Ok(()) => out.push_str("Status: completed\n"),
            Err(err) => out.push_str(&format!("Status: failed ({err})\n")),
        }
        out.push_str(&format!("Trace events: {}\n", self.trace.len()));
        out.push_str(&format!("Breakpoints: {}\n", self.breakpoints.len()));
        if self.breakpoints.is_empty() {
            out.push_str("Hits: no breakpoints configured\n");
            return out;
        }
        if self.hits.is_empty() {
            out.push_str("Hits: none\n");
            return out;
        }
        out.push_str("Hits:\n");
        for hit in &self.hits {
            out.push_str(&format!(
                "  - #{} {} target={} breakpoint={}\n",
                hit.event.sequence,
                hit.event.kind.as_str(),
                hit.event.target,
                hit.breakpoint.label()
            ));
        }
        out
    }
}

impl BreakpointHit {
    pub fn to_json(&self) -> Value {
        json!({
            "breakpoint": self.breakpoint.to_json(),
            "event": self.event.to_json(),
        })
    }
}

fn adapter_breakpoint(id: usize, breakpoint: &BreakpointSpec) -> Value {
    json!({
        "id": id,
        "verified": true,
        "kind": breakpoint.kind.as_str(),
        "target": breakpoint.target,
        "label": breakpoint.label(),
        "mode": "post_execution_trace_match",
    })
}

fn adapter_stopped_event(breakpoint_id: usize, hit: &BreakpointHit) -> Value {
    json!({
        "reason": "breakpoint",
        "thread_id": 1,
        "breakpoint_id": breakpoint_id,
        "event_sequence": hit.event.sequence,
        "event_kind": hit.event.kind.as_str(),
        "target": hit.event.target,
        "frame_id": adapter_frame_id(&hit.event),
        "frame": adapter_stack_frame(&hit.event),
        "scopes": adapter_scopes(&hit.event),
    })
}

fn adapter_stack_frame(event: &RuntimeTraceEvent) -> Value {
    json!({
        "id": adapter_frame_id(event),
        "thread_id": 1,
        "name": adapter_frame_name(event),
        "source": Value::Null,
        "line": Value::Null,
        "column": Value::Null,
        "presentation_hint": adapter_frame_presentation(event.kind),
    })
}

fn adapter_scopes(event: &RuntimeTraceEvent) -> Value {
    json!({
        "frame_id": adapter_frame_id(event),
        "scopes": [{
            "name": adapter_scope_name(event.kind),
            "expensive": false,
            "variables": adapter_variables(event),
        }]
    })
}

fn adapter_variables(event: &RuntimeTraceEvent) -> Value {
    json!([
        {
            "name": "sequence",
            "value": event.sequence.to_string(),
            "type": "u64",
        },
        {
            "name": "event_kind",
            "value": event.kind.as_str(),
            "type": "RuntimeTraceKind",
        },
        {
            "name": "target",
            "value": redaction::redact_text(&event.target),
            "type": "Text",
        },
        {
            "name": "detail",
            "value": event.detail.as_ref().map(|detail| redaction::redact_text(detail)).unwrap_or_default(),
            "type": "Text?",
        },
    ])
}

fn adapter_frame_id(event: &RuntimeTraceEvent) -> String {
    format!("trace:{}", event.sequence)
}

fn adapter_frame_name(event: &RuntimeTraceEvent) -> String {
    format!(
        "{} {}",
        match event.kind {
            RuntimeTraceKind::WorkflowStarted
            | RuntimeTraceKind::WorkflowCompleted
            | RuntimeTraceKind::WorkflowFailed => "workflow",
            RuntimeTraceKind::ServiceRouteStarted
            | RuntimeTraceKind::ServiceRouteCompleted
            | RuntimeTraceKind::ServiceRouteFailed => "service_route",
            RuntimeTraceKind::StatementStarted | RuntimeTraceKind::StatementCompleted => {
                "statement"
            }
            RuntimeTraceKind::FunctionCalled => "function",
            RuntimeTraceKind::ActionCalled => "action",
            RuntimeTraceKind::ConnectorCalled => "connector",
            RuntimeTraceKind::AuditLogged => "audit",
        },
        event.target
    )
}

fn adapter_scope_name(kind: RuntimeTraceKind) -> &'static str {
    match kind {
        RuntimeTraceKind::WorkflowStarted
        | RuntimeTraceKind::WorkflowCompleted
        | RuntimeTraceKind::WorkflowFailed => "Workflow",
        RuntimeTraceKind::ServiceRouteStarted
        | RuntimeTraceKind::ServiceRouteCompleted
        | RuntimeTraceKind::ServiceRouteFailed => "Service Route",
        RuntimeTraceKind::StatementStarted | RuntimeTraceKind::StatementCompleted => "Statement",
        RuntimeTraceKind::FunctionCalled => "Function",
        RuntimeTraceKind::ActionCalled => "Action",
        RuntimeTraceKind::ConnectorCalled => "Connector",
        RuntimeTraceKind::AuditLogged => "Audit",
    }
}

fn adapter_frame_presentation(kind: RuntimeTraceKind) -> &'static str {
    match kind {
        RuntimeTraceKind::WorkflowFailed | RuntimeTraceKind::ServiceRouteFailed => "error",
        RuntimeTraceKind::WorkflowCompleted | RuntimeTraceKind::ServiceRouteCompleted => "return",
        _ => "normal",
    }
}

fn parse_breakpoint_kind(raw: &str) -> Result<RuntimeTraceKind, String> {
    match raw {
        "workflow" | "workflow-start" | "WorkflowStarted" => Ok(RuntimeTraceKind::WorkflowStarted),
        "workflow-complete" | "WorkflowCompleted" => Ok(RuntimeTraceKind::WorkflowCompleted),
        "workflow-fail" | "WorkflowFailed" => Ok(RuntimeTraceKind::WorkflowFailed),
        "statement" | "stmt" | "StatementStarted" => Ok(RuntimeTraceKind::StatementStarted),
        "statement-complete" | "stmt-complete" | "StatementCompleted" => {
            Ok(RuntimeTraceKind::StatementCompleted)
        }
        "function" | "fn" | "FunctionCalled" => Ok(RuntimeTraceKind::FunctionCalled),
        "action" | "ActionCalled" => Ok(RuntimeTraceKind::ActionCalled),
        "connector" | "ConnectorCalled" => Ok(RuntimeTraceKind::ConnectorCalled),
        "audit" | "AuditLogged" => Ok(RuntimeTraceKind::AuditLogged),
        other => Err(format!(
            "unknown breakpoint kind `{other}`; supported: workflow, statement, function, action, connector, audit"
        )),
    }
}

fn normalize_target(raw: &str) -> String {
    raw.trim().trim_matches('"').trim_matches('\'').to_string()
}

#[cfg(test)]
mod tests {
    use super::{BreakpointSpec, DebugReport};
    use crate::observability::{RuntimeTraceEvent, RuntimeTraceKind};
    use crate::RuntimeError;

    #[test]
    fn parses_breakpoint_aliases() {
        let breakpoint = BreakpointSpec::parse("action:issue_refund").unwrap();

        assert_eq!(breakpoint.kind, RuntimeTraceKind::ActionCalled);
        assert_eq!(breakpoint.target, "issue_refund");
    }

    #[test]
    fn debug_report_collects_matching_hits() {
        let breakpoint = BreakpointSpec::parse("connector:payments.find").unwrap();
        let trace = vec![
            RuntimeTraceEvent::new(1, RuntimeTraceKind::WorkflowStarted, "main", None),
            RuntimeTraceEvent::new(2, RuntimeTraceKind::ConnectorCalled, "payments.find", None),
        ];

        let report = DebugReport::from_trace("main", Ok(()), None, vec![breakpoint], &trace);

        assert_eq!(report.hits.len(), 1);
        assert_eq!(report.hits[0].event.sequence, 2);
        assert_eq!(
            report.to_json()["hits"][0]["event"]["target"],
            "payments.find"
        );
        assert!(report
            .render_text()
            .contains("breakpoint=ConnectorCalled:payments.find"));
    }

    #[test]
    fn debug_adapter_model_maps_scripted_breakpoints_to_frames_and_scopes() {
        let breakpoints = vec![
            BreakpointSpec::parse("workflow:main").unwrap(),
            BreakpointSpec::parse("action:issue_refund").unwrap(),
            BreakpointSpec::parse("function:calculate_fee").unwrap(),
            BreakpointSpec::parse("connector:payments.find").unwrap(),
            BreakpointSpec::parse("audit:refund_issued").unwrap(),
        ];
        let trace = vec![
            RuntimeTraceEvent::new(1, RuntimeTraceKind::WorkflowStarted, "main", None),
            RuntimeTraceEvent::new(2, RuntimeTraceKind::ActionCalled, "issue_refund", None),
            RuntimeTraceEvent::new(3, RuntimeTraceKind::FunctionCalled, "calculate_fee", None),
            RuntimeTraceEvent::new(4, RuntimeTraceKind::ConnectorCalled, "payments.find", None),
            RuntimeTraceEvent::new(5, RuntimeTraceKind::AuditLogged, "refund_issued", None),
        ];

        let report = DebugReport::from_trace("main", Ok(()), None, breakpoints, &trace);
        let adapter = report.to_adapter_json();

        assert_eq!(adapter["protocol"], "num.debug.adapter.v1");
        assert_eq!(adapter["threads"][0]["name"], "workflow:main");
        assert_eq!(adapter["breakpoints"].as_array().unwrap().len(), 5);
        assert_eq!(adapter["stopped_events"].as_array().unwrap().len(), 5);
        assert_eq!(
            adapter["stopped_events"][0]["frame"]["name"],
            "workflow main"
        );
        assert_eq!(
            adapter["stopped_events"][1]["frame"]["name"],
            "action issue_refund"
        );
        assert_eq!(
            adapter["stopped_events"][2]["frame"]["name"],
            "function calculate_fee"
        );
        assert_eq!(
            adapter["stopped_events"][3]["frame"]["name"],
            "connector payments.find"
        );
        assert_eq!(
            adapter["stopped_events"][4]["scopes"]["scopes"][0]["name"],
            "Audit"
        );
        assert_eq!(
            adapter["stopped_events"][4]["scopes"]["scopes"][0]["variables"][2]["value"],
            "refund_issued"
        );
    }

    #[test]
    fn debug_adapter_model_makes_unsupported_step_continue_explicit() {
        let report = DebugReport::from_trace("main", Ok(()), None, vec![], &[]);
        let adapter = report.to_adapter_json();

        assert_eq!(adapter["capabilities"]["supports_continue"], false);
        assert_eq!(adapter["capabilities"]["supports_next"], false);
        assert_eq!(adapter["capabilities"]["supports_step_in"], false);
        assert_eq!(adapter["capabilities"]["supports_step_out"], false);
        assert!(adapter["capabilities"]["unsupported_requests"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value == "continue"));
        assert_eq!(
            adapter["capabilities"]["execution_model"],
            "scripted trace replay; breakpoints are reported as hits after workflow execution"
        );
    }

    #[test]
    fn debug_json_includes_adapter_boundary_without_dropping_cli_trace() {
        let breakpoint = BreakpointSpec::parse("workflow:main").unwrap();
        let trace = vec![RuntimeTraceEvent::new(
            1,
            RuntimeTraceKind::WorkflowStarted,
            "main",
            None,
        )];

        let report = DebugReport::from_trace("main", Ok(()), None, vec![breakpoint], &trace);
        let payload = report.to_json();

        assert_eq!(payload["trace"][0]["kind"], "WorkflowStarted");
        assert_eq!(payload["debug_adapter"]["protocol"], "num.debug.adapter.v1");
        assert_eq!(
            payload["debug_adapter"]["stopped_events"][0]["frame_id"],
            "trace:1"
        );
    }

    #[test]
    fn debug_report_includes_structured_connector_error() {
        let error = RuntimeError::ConnectorFailed {
            method: "payments.find".to_string(),
            code: "timeout".to_string(),
            message: "deadline exceeded".to_string(),
            retryable: true,
        };

        let report =
            DebugReport::from_trace("main", Err(error.message()), Some(error), vec![], &[]);

        assert_eq!(
            report.to_json()["runtime_error"]["kind"],
            "connector_failed"
        );
        assert_eq!(
            report.to_json()["runtime_error"]["connector"]["code"],
            "timeout"
        );
        assert_eq!(
            report.to_json()["runtime_error"]["connector"]["retryable"],
            true
        );
    }

    #[test]
    fn debug_report_redacts_secret_error_payloads() {
        let error = RuntimeError::ConnectorFailed {
            method: "secrets.send".to_string(),
            code: "execution_failed".to_string(),
            message: "token=sk_live_debug".to_string(),
            retryable: false,
        };

        let report =
            DebugReport::from_trace("main", Err(error.message()), Some(error), vec![], &[]);
        let payload = report.to_json();

        assert!(!payload.to_string().contains("sk_live_debug"));
        assert_eq!(
            payload["runtime_error"]["connector"]["message"],
            "token=<redacted>"
        );
        assert_eq!(
            payload["error"],
            "Connector 'secrets.send' failed [execution_failed, retryable=false]: token=<redacted>"
        );
    }
}
