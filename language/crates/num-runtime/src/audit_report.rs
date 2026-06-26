use crate::{redaction, RuntimeError};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub const AUDIT_DASHBOARD_SCHEMA_VERSION: &str = "num.audit_dashboard.v1";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuditReport {
    pub total_events: usize,
    pub by_result: BTreeMap<String, usize>,
    pub by_action: BTreeMap<String, usize>,
    pub by_actor: BTreeMap<String, usize>,
    pub by_tenant: BTreeMap<String, usize>,
    pub by_connector: BTreeMap<String, usize>,
    pub by_route: BTreeMap<String, usize>,
    pub by_workflow: BTreeMap<String, usize>,
    pub time_window: AuditTimeWindow,
    pub failures: Vec<AuditFailure>,
}

impl AuditReport {
    pub fn to_json(&self) -> Value {
        json!({
            "schema_version": AUDIT_DASHBOARD_SCHEMA_VERSION,
            "total_events": self.total_events,
            "by_result": self.by_result,
            "by_action": self.by_action,
            "by_actor": self.by_actor,
            "by_tenant": self.by_tenant,
            "counts": {
                "by_result": count_map_to_json(&self.by_result),
                "by_action": count_map_to_json(&self.by_action),
                "by_actor": count_map_to_json(&self.by_actor),
                "by_tenant": count_map_to_json(&self.by_tenant),
                "by_connector": count_map_to_json(&self.by_connector),
                "by_route": count_map_to_json(&self.by_route),
                "by_workflow": count_map_to_json(&self.by_workflow),
            },
            "dimensions": {
                "by_connector": count_map_to_json(&self.by_connector),
                "by_route": count_map_to_json(&self.by_route),
                "by_workflow": count_map_to_json(&self.by_workflow),
            },
            "time_window": self.time_window.to_json(),
            "stable_fields": [
                "schema_version",
                "total_events",
                "counts.by_result",
                "counts.by_action",
                "counts.by_actor",
                "counts.by_tenant",
                "counts.by_connector",
                "counts.by_route",
                "counts.by_workflow",
                "time_window",
                "failures",
            ],
            "conditional_fields": {
                "connector": "present when audit events include connector metadata",
                "route": "present when audit events include service, method, and path or route metadata",
                "workflow": "present when audit events include workflow metadata",
                "time_window": "start_unix_ms and end_unix_ms are present when events carry timestamp_ms or recorded_at_unix_ms",
                "failures.reason": "redacted diagnostic text for failed audit events; raw event payloads are not included"
            },
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
        push_counts(&mut out, "by_connector", &self.by_connector);
        push_counts(&mut out, "by_route", &self.by_route);
        push_counts(&mut out, "by_workflow", &self.by_workflow);
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
    pub connector: Option<String>,
    pub route: Option<String>,
    pub workflow: Option<String>,
    pub timestamp_ms: Option<i128>,
    pub correlation_id: Option<String>,
    pub request_id: Option<String>,
    pub reason: String,
}

impl AuditFailure {
    fn to_json(&self) -> Value {
        json!({
            "event_id": self.event_id,
            "action": self.action,
            "actor": self.actor,
            "tenant": self.tenant,
            "connector": self.connector,
            "route": self.route,
            "workflow": self.workflow,
            "timestamp_ms": self.timestamp_ms,
            "correlation_id": self.correlation_id,
            "request_id": self.request_id,
            "reason": self.reason,
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuditTimeWindow {
    pub start_unix_ms: Option<i128>,
    pub end_unix_ms: Option<i128>,
}

impl AuditTimeWindow {
    fn observe(&mut self, timestamp_ms: Option<i128>) {
        let Some(timestamp_ms) = timestamp_ms else {
            return;
        };
        self.start_unix_ms = Some(
            self.start_unix_ms
                .map_or(timestamp_ms, |current| current.min(timestamp_ms)),
        );
        self.end_unix_ms = Some(
            self.end_unix_ms
                .map_or(timestamp_ms, |current| current.max(timestamp_ms)),
        );
    }

    fn to_json(&self) -> Value {
        json!({
            "start_unix_ms": self.start_unix_ms,
            "end_unix_ms": self.end_unix_ms,
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
    let connector = connector_dimension(event);
    let route = route_dimension(event);
    let workflow = workflow_dimension(event);
    let timestamp_ms = timestamp_ms(event);

    report.total_events += 1;
    increment(&mut report.by_result, &result_kind);
    increment(&mut report.by_action, &action);
    increment(&mut report.by_actor, &actor);
    increment(&mut report.by_tenant, &tenant);
    increment_optional(&mut report.by_connector, connector.as_deref());
    increment_optional(&mut report.by_route, route.as_deref());
    increment_optional(&mut report.by_workflow, workflow.as_deref());
    report.time_window.observe(timestamp_ms);

    if result_kind == "Failed" {
        report.failures.push(AuditFailure {
            event_id: string_field(event, "event_id")?,
            action,
            actor,
            tenant,
            connector,
            route,
            workflow,
            timestamp_ms,
            correlation_id: optional_string_field(event, "correlation_id"),
            request_id: optional_string_field(event, "request_id"),
            reason: redaction::redact_text(
                &string_field(result, "reason").unwrap_or_else(|_| "unknown".to_string()),
            ),
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

fn increment_optional(counts: &mut BTreeMap<String, usize>, key: Option<&str>) {
    if let Some(key) = key {
        increment(counts, key);
    }
}

fn count_map_to_json(counts: &BTreeMap<String, usize>) -> Vec<Value> {
    counts
        .iter()
        .map(|(key, count)| {
            json!({
                "key": key,
                "count": count,
            })
        })
        .collect()
}

fn string_field(value: &Value, key: &str) -> Result<String, RuntimeError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| storage_error(format!("missing string field '{key}'")))
}

fn optional_string_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

fn connector_dimension(event: &Value) -> Option<String> {
    optional_string_field(event, "connector")
        .or_else(|| optional_nested_string_field(event, "connector", "method"))
        .or_else(|| optional_string_field(event, "connector_method"))
}

fn route_dimension(event: &Value) -> Option<String> {
    optional_string_field(event, "route").or_else(|| {
        let method = optional_string_field(event, "method")?;
        let path = optional_string_field(event, "path")?;
        Some(format!("{method} {path}"))
    })
}

fn workflow_dimension(event: &Value) -> Option<String> {
    optional_string_field(event, "workflow")
        .or_else(|| optional_string_field(event, "workflow_name"))
        .or_else(|| optional_nested_string_field(event, "workflow", "name"))
        .or_else(|| optional_nested_string_field(event, "workflow", "id"))
}

fn optional_nested_string_field(value: &Value, object_key: &str, key: &str) -> Option<String> {
    value
        .get(object_key)
        .and_then(|object| object.get(key))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn timestamp_ms(event: &Value) -> Option<i128> {
    integer_field(event, "timestamp_ms").or_else(|| integer_field(event, "recorded_at_unix_ms"))
}

fn integer_field(value: &Value, key: &str) -> Option<i128> {
    value
        .get(key)
        .and_then(Value::as_i64)
        .map(i128::from)
        .or_else(|| value.get(key).and_then(Value::as_u64).map(i128::from))
}

fn storage_error(message: impl Into<String>) -> RuntimeError {
    RuntimeError::Storage(message.into())
}

#[cfg(test)]
mod tests {
    use super::summarize_audit_jsonl;
    use serde_json::json;

    #[test]
    fn audit_report_summarizes_jsonl_events() {
        let source = r#"
{"event_id":"evt_1","timestamp_ms":1700000000000,"actor":"alice","tenant":"t1","action":"refund","workflow":"refund_flow","result":{"kind":"Started"}}
{"event_id":"evt_2","timestamp_ms":1700000000500,"actor":"alice","tenant":"t1","action":"refund","workflow":"refund_flow","result":{"kind":"Succeeded"}}
{"event_id":"evt_3","timestamp_ms":1700000001000,"actor":"bob","tenant":"t2","action":"sync","connector":"crm.sync","method":"POST","path":"/sync","correlation_id":"corr_3","request_id":"req_3","result":{"kind":"Failed","reason":"timeout token=sk_live_123"}}
"#;

        let report = summarize_audit_jsonl(source).unwrap();

        assert_eq!(report.total_events, 3);
        assert_eq!(report.by_result.get("Succeeded"), Some(&1));
        assert_eq!(report.by_action.get("refund"), Some(&2));
        assert_eq!(report.by_actor.get("alice"), Some(&2));
        assert_eq!(report.by_connector.get("crm.sync"), Some(&1));
        assert_eq!(report.by_route.get("POST /sync"), Some(&1));
        assert_eq!(report.by_workflow.get("refund_flow"), Some(&2));
        assert_eq!(report.failures.len(), 1);
        assert_eq!(report.failures[0].reason, "timeout token=<redacted>");
        assert!(report.render_text().contains("total_events: 3"));
        assert_eq!(
            report.to_json()["schema_version"],
            super::AUDIT_DASHBOARD_SCHEMA_VERSION
        );
        assert_eq!(
            report.to_json()["time_window"]["start_unix_ms"],
            json!(1_700_000_000_000i64)
        );
    }

    #[test]
    fn audit_report_rejects_invalid_jsonl() {
        let error = summarize_audit_jsonl("{").unwrap_err();

        assert!(format!("{error:?}").contains("invalid audit JSONL"));
    }

    #[test]
    fn audit_dashboard_read_model_matches_fixture() {
        let source = r#"
{"event_id":"evt_1","timestamp_ms":1700000000000,"actor":"alice","tenant":"tenant_a","action":"refund","workflow":"refund_flow","result":{"kind":"Started"}}
{"event_id":"evt_2","timestamp_ms":1700000000500,"actor":"alice","tenant":"tenant_a","action":"refund","workflow":"refund_flow","result":{"kind":"Succeeded"}}
{"event_id":"evt_3","timestamp_ms":1700000001000,"actor":"service","tenant":"tenant_a","action":"BillingApi POST /refunds","method":"POST","path":"/refunds","correlation_id":"corr_3","request_id":"req_3","result":{"kind":"Failed","reason":"password=hunter2 connector denied"}}
{"event_id":"evt_4","timestamp_ms":1700000001500,"actor":"worker","tenant":"tenant_b","action":"sync","connector":"crm.sync","result":{"kind":"Succeeded"}}
"#;
        let report = summarize_audit_jsonl(source).unwrap();
        let expected: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/audit_dashboard_v1.json"))
                .unwrap();

        assert_eq!(report.to_json(), expected);
    }
}
