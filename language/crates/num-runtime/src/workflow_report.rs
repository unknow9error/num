use crate::{storage::FileStateStore, RuntimeError, WorkflowState, WorkflowStatus};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkflowReport {
    pub total_workflows: usize,
    pub by_status: BTreeMap<String, usize>,
    pub by_name: BTreeMap<String, usize>,
    pub by_actor: BTreeMap<String, usize>,
    pub by_tenant: BTreeMap<String, usize>,
    pub workflows: Vec<WorkflowSummary>,
}

impl WorkflowReport {
    pub fn to_json(&self) -> Value {
        json!({
            "total_workflows": self.total_workflows,
            "by_status": self.by_status,
            "by_name": self.by_name,
            "by_actor": self.by_actor,
            "by_tenant": self.by_tenant,
            "workflows": self.workflows.iter().map(WorkflowSummary::to_json).collect::<Vec<_>>(),
        })
    }

    pub fn render_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("total_workflows: {}\n", self.total_workflows));
        push_counts(&mut out, "by_status", &self.by_status);
        push_counts(&mut out, "by_name", &self.by_name);
        push_counts(&mut out, "by_actor", &self.by_actor);
        push_counts(&mut out, "by_tenant", &self.by_tenant);
        if !self.workflows.is_empty() {
            out.push_str("workflows:\n");
            for workflow in &self.workflows {
                out.push_str(&format!(
                    "  - {} {} status={} actor={} tenant={} updated_at_ms={}\n",
                    workflow.id,
                    workflow.name,
                    workflow.status,
                    workflow.actor,
                    workflow.tenant,
                    workflow.updated_at_ms
                ));
            }
        }
        out
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowSummary {
    pub id: String,
    pub name: String,
    pub status: String,
    pub actor: String,
    pub tenant: String,
    pub started_at_ms: u64,
    pub updated_at_ms: u64,
}

impl WorkflowSummary {
    fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "status": self.status,
            "actor": self.actor,
            "tenant": self.tenant,
            "started_at_ms": self.started_at_ms,
            "updated_at_ms": self.updated_at_ms,
        })
    }
}

pub fn summarize_workflow_store(store: &FileStateStore) -> Result<WorkflowReport, RuntimeError> {
    let workflows = store.list_workflows()?;
    Ok(summarize_workflows(&workflows))
}

pub fn summarize_workflows(workflows: &[WorkflowState]) -> WorkflowReport {
    let mut report = WorkflowReport::default();
    for workflow in workflows {
        let status = workflow_status(&workflow.status);
        report.total_workflows += 1;
        increment(&mut report.by_status, status);
        increment(&mut report.by_name, &workflow.name);
        increment(&mut report.by_actor, &workflow.security.actor);
        increment(&mut report.by_tenant, &workflow.security.tenant);
        report.workflows.push(WorkflowSummary {
            id: workflow.id.clone(),
            name: workflow.name.clone(),
            status: status.to_string(),
            actor: workflow.security.actor.clone(),
            tenant: workflow.security.tenant.clone(),
            started_at_ms: system_time_ms(workflow.started_at),
            updated_at_ms: system_time_ms(workflow.updated_at),
        });
    }
    report
        .workflows
        .sort_by(|left, right| right.updated_at_ms.cmp(&left.updated_at_ms));
    report
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

fn workflow_status(status: &WorkflowStatus) -> &'static str {
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

fn system_time_ms(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::summarize_workflows;
    use crate::{SecurityContext, WorkflowState, WorkflowStatus};
    use std::collections::{BTreeMap, BTreeSet};
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn workflow_report_summarizes_states() {
        let workflows = vec![
            workflow_state("wf_1", WorkflowStatus::Running, "tenant_a", 10),
            workflow_state("wf_2", WorkflowStatus::Completed, "tenant_a", 20),
            workflow_state("wf_3", WorkflowStatus::Failed, "tenant_b", 30),
        ];

        let report = summarize_workflows(&workflows);

        assert_eq!(report.total_workflows, 3);
        assert_eq!(report.by_status.get("Running"), Some(&1));
        assert_eq!(report.by_tenant.get("tenant_a"), Some(&2));
        assert_eq!(report.workflows[0].id, "wf_3");
        assert!(report.render_text().contains("total_workflows: 3"));
        assert_eq!(report.to_json()["total_workflows"], 3);
    }

    fn workflow_state(
        id: &str,
        status: WorkflowStatus,
        tenant: &str,
        updated_at_seconds: u64,
    ) -> WorkflowState {
        WorkflowState {
            id: id.to_string(),
            name: "refund_flow".to_string(),
            status,
            security: SecurityContext {
                actor: "support@example.com".to_string(),
                tenant: tenant.to_string(),
                permissions: BTreeSet::new(),
                correlation_id: "corr_1".to_string(),
                request_id: "req_1".to_string(),
            },
            started_at: UNIX_EPOCH + Duration::from_secs(1),
            updated_at: UNIX_EPOCH + Duration::from_secs(updated_at_seconds),
            metadata: BTreeMap::new(),
        }
    }
}
