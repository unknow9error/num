use crate::RuntimeError;
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuditReport {
    pub total_events: usize,
    pub by_result: BTreeMap<String, usize>,
    pub by_action: BTreeMap<String, usize>,
    pub by_actor: BTreeMap<String, usize>,
    pub by_tenant: BTreeMap<String, usize>,
    pub failures: Vec<AuditFailure>,
}

impl AuditReport {
    pub fn to_json(&self) -> Value {
        json!({
            "total_events": self.total_events,
            "by_result": self.by_result,
            "by_action": self.by_action,
            "by_actor": self.by_actor,
            "by_tenant": self.by_tenant,
            "failures": self.failures.iter().map(AuditFailure::to_json).collect::<Vec<_>>(),
        })
    }

    pub fn render_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("total_events: {}\n", self.total_events));
        push_counts(&mut out, "by_result", &self.by_result);
        push_counts(&mut out, "by_action", &self.by_action);
        push_counts(&mut out, "by_actor", &self.by_actor);
        push_counts(&mut out, "by_tenant", &self.by_tenant);
        if !self.failures.is_empty() {
            out.push_str("failures:\n");
            for failure in &self.failures {
                out.push_str(&format!(
                    "  - {} {} actor={} tenant={}: {}\n",
                    failure.event_id, failure.action, failure.actor, failure.tenant, failure.reason
                ));
            }
        }
        out
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditFailure {
    pub event_id: String,
    pub action: String,
    pub actor: String,
    pub tenant: String,
    pub reason: String,
}

impl AuditFailure {
    fn to_json(&self) -> Value {
        json!({
            "event_id": self.event_id,
            "action": self.action,
            "actor": self.actor,
            "tenant": self.tenant,
            "reason": self.reason,
        })
    }
}

pub fn summarize_audit_jsonl(source: &str) -> Result<AuditReport, RuntimeError> {
    let mut report = AuditReport::default();
    for (index, raw_line) in source.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(line).map_err(|err| {
            RuntimeError::Storage(format!("invalid audit JSONL at line {}: {err}", index + 1))
        })?;
        apply_audit_event(&mut report, &value)?;
    }
    Ok(report)
}

fn apply_audit_event(report: &mut AuditReport, event: &Value) -> Result<(), RuntimeError> {
    let result = event
        .get("result")
        .ok_or_else(|| storage_error("audit event missing result"))?;
    let result_kind = string_field(result, "kind")?;
    let action = string_field(event, "action")?;
    let actor = string_field(event, "actor")?;
    let tenant = string_field(event, "tenant")?;

    report.total_events += 1;
    increment(&mut report.by_result, &result_kind);
    increment(&mut report.by_action, &action);
    increment(&mut report.by_actor, &actor);
    increment(&mut report.by_tenant, &tenant);

    if result_kind == "Failed" {
        report.failures.push(AuditFailure {
            event_id: string_field(event, "event_id")?,
            action,
            actor,
            tenant,
            reason: string_field(result, "reason").unwrap_or_else(|_| "unknown".to_string()),
        });
    }
    Ok(())
}

fn push_counts(out: &mut String, title: &str, counts: &BTreeMap<String, usize>) {
    if counts.is_empty() {
        return;
    }
    out.push_str(title);
    out.push_str(":\n");
    for (key, value) in counts {
        out.push_str(&format!("  {key}: {value}\n"));
    }
}

fn increment(counts: &mut BTreeMap<String, usize>, key: &str) {
    *counts.entry(key.to_string()).or_insert(0) += 1;
}

fn string_field(value: &Value, key: &str) -> Result<String, RuntimeError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| storage_error(format!("missing string field '{key}'")))
}

fn storage_error(message: impl Into<String>) -> RuntimeError {
    RuntimeError::Storage(message.into())
}

#[cfg(test)]
mod tests {
    use super::summarize_audit_jsonl;

    #[test]
    fn audit_report_summarizes_jsonl_events() {
        let source = r#"
{"event_id":"evt_1","actor":"alice","tenant":"t1","action":"refund","result":{"kind":"Started"}}
{"event_id":"evt_2","actor":"alice","tenant":"t1","action":"refund","result":{"kind":"Succeeded"}}
{"event_id":"evt_3","actor":"bob","tenant":"t2","action":"sync","result":{"kind":"Failed","reason":"timeout"}}
"#;

        let report = summarize_audit_jsonl(source).unwrap();

        assert_eq!(report.total_events, 3);
        assert_eq!(report.by_result.get("Succeeded"), Some(&1));
        assert_eq!(report.by_action.get("refund"), Some(&2));
        assert_eq!(report.by_actor.get("alice"), Some(&2));
        assert_eq!(report.failures.len(), 1);
        assert_eq!(report.failures[0].reason, "timeout");
        assert!(report.render_text().contains("total_events: 3"));
    }

    #[test]
    fn audit_report_rejects_invalid_jsonl() {
        let error = summarize_audit_jsonl("{").unwrap_err();

        assert!(format!("{error:?}").contains("invalid audit JSONL"));
    }
}
