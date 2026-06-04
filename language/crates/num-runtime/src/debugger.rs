use crate::observability::{RuntimeTraceEvent, RuntimeTraceKind};
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
            breakpoints,
            hits,
            trace: trace.to_vec(),
        }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "workflow": self.workflow,
            "status": if self.result.is_ok() { "completed" } else { "failed" },
            "error": self.result.as_ref().err(),
            "breakpoints": self.breakpoints.iter().map(BreakpointSpec::to_json).collect::<Vec<_>>(),
            "hits": self.hits.iter().map(BreakpointHit::to_json).collect::<Vec<_>>(),
            "trace": self.trace.iter().map(RuntimeTraceEvent::to_json).collect::<Vec<_>>(),
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

        let report = DebugReport::from_trace("main", Ok(()), vec![breakpoint], &trace);

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
}
