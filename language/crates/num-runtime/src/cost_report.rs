use crate::{
    cost::{CostDimensions, CostEntry},
    Money,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub const COST_DASHBOARD_SCHEMA_VERSION: &str = "num.cost_dashboard.v1";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CostReport {
    pub total_entries: usize,
    pub by_currency: BTreeMap<String, i128>,
    pub by_action: BTreeMap<String, BTreeMap<String, i128>>,
    pub by_connector: BTreeMap<String, BTreeMap<String, i128>>,
    pub by_model: BTreeMap<String, BTreeMap<String, i128>>,
    pub by_workflow: BTreeMap<String, BTreeMap<String, i128>>,
    pub by_route: BTreeMap<String, BTreeMap<String, i128>>,
    pub by_actor: BTreeMap<String, BTreeMap<String, i128>>,
    pub by_tenant: BTreeMap<String, BTreeMap<String, i128>>,
    pub time_window: CostTimeWindow,
    pub entries: Vec<CostSummary>,
}

impl CostReport {
    pub fn to_json(&self) -> Value {
        json!({
            "schema_version": COST_DASHBOARD_SCHEMA_VERSION,
            "total_entries": self.total_entries,
            "by_currency": self.by_currency,
            "by_action": self.by_action,
            "totals": {
                "by_currency": money_map_to_json(&self.by_currency),
                "by_action": nested_money_map_to_json(&self.by_action),
                "by_connector": nested_money_map_to_json(&self.by_connector),
                "by_model": nested_money_map_to_json(&self.by_model),
                "by_workflow": nested_money_map_to_json(&self.by_workflow),
                "by_route": nested_money_map_to_json(&self.by_route),
                "by_actor": nested_money_map_to_json(&self.by_actor),
                "by_tenant": nested_money_map_to_json(&self.by_tenant),
            },
            "time_window": self.time_window.to_json(),
            "stable_dimensions": [
                "currency",
                "action",
                "connector",
                "model",
                "workflow",
                "route",
                "actor",
                "tenant",
            ],
            "conditional_fields": {
                "connector": "present when a cost entry is emitted from a connector-aware runtime boundary",
                "model": "present when a cost entry is emitted from an AI/model-aware runtime boundary",
                "workflow": "present when a cost entry is emitted with workflow context",
                "route": "present when a cost entry is emitted with service route context",
                "actor": "present when a cost entry is emitted with actor context",
                "tenant": "present when a cost entry is emitted with tenant context",
                "time_window": "start_unix_ms and end_unix_ms are present when raw entries carry timestamps",
            },
            "entries": self.entries.iter().map(CostSummary::to_json).collect::<Vec<_>>(),
            "raw_entries": self.entries.iter().enumerate().map(|(index, entry)| entry.to_raw_json(index)).collect::<Vec<_>>(),
        })
    }

    pub fn render_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("total_entries: {}\n", self.total_entries));
        if !self.by_currency.is_empty() {
            out.push_str("by_currency:\n");
            for (currency, minor_units) in &self.by_currency {
                out.push_str(&format!(
                    "  {currency}: {}\n",
                    render_money(*minor_units, currency)
                ));
            }
        }
        if !self.by_action.is_empty() {
            out.push_str("by_action:\n");
            for (action, totals) in &self.by_action {
                let rendered = totals
                    .iter()
                    .map(|(currency, minor_units)| render_money(*minor_units, currency))
                    .collect::<Vec<_>>()
                    .join(", ");
                out.push_str(&format!("  {action}: {rendered}\n"));
            }
        }
        if !self.entries.is_empty() {
            out.push_str("entries:\n");
            for entry in &self.entries {
                out.push_str(&format!(
                    "  - {} {}\n",
                    entry.action,
                    render_money(entry.minor_units, &entry.currency)
                ));
            }
        }
        out
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CostSummary {
    pub action: String,
    pub minor_units: i128,
    pub currency: String,
    pub dimensions: CostDimensions,
    pub recorded_at_unix_ms: Option<i128>,
}

impl CostSummary {
    fn to_json(&self) -> Value {
        json!({
            "action": self.action,
            "amount": {
                "minor_units": self.minor_units,
                "currency": self.currency,
            },
            "dimensions": dimensions_to_json(&self.dimensions),
            "recorded_at_unix_ms": self.recorded_at_unix_ms,
        })
    }

    fn to_raw_json(&self, index: usize) -> Value {
        json!({
            "index": index,
            "action": self.action,
            "amount": {
                "minor_units": self.minor_units,
                "currency": self.currency,
            },
            "dimensions": dimensions_to_json(&self.dimensions),
            "recorded_at_unix_ms": self.recorded_at_unix_ms,
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CostTimeWindow {
    pub start_unix_ms: Option<i128>,
    pub end_unix_ms: Option<i128>,
}

impl CostTimeWindow {
    fn observe(&mut self, recorded_at_unix_ms: Option<i128>) {
        let Some(value) = recorded_at_unix_ms else {
            return;
        };
        self.start_unix_ms = Some(
            self.start_unix_ms
                .map_or(value, |current| current.min(value)),
        );
        self.end_unix_ms = Some(self.end_unix_ms.map_or(value, |current| current.max(value)));
    }

    fn to_json(&self) -> Value {
        json!({
            "start_unix_ms": self.start_unix_ms,
            "end_unix_ms": self.end_unix_ms,
        })
    }
}

pub fn summarize_cost_entries(entries: &[CostEntry]) -> CostReport {
    let mut report = CostReport::default();
    for entry in entries {
        report.total_entries += 1;
        add_money(&mut report.by_currency, &entry.amount);
        add_money(
            report
                .by_action
                .entry(entry.action.clone())
                .or_insert_with(BTreeMap::new),
            &entry.amount,
        );
        add_optional_dimension_money(
            &mut report.by_connector,
            entry.dimensions.connector.as_deref(),
            &entry.amount,
        );
        add_optional_dimension_money(
            &mut report.by_model,
            entry.dimensions.model.as_deref(),
            &entry.amount,
        );
        add_optional_dimension_money(
            &mut report.by_workflow,
            entry.dimensions.workflow.as_deref(),
            &entry.amount,
        );
        add_optional_dimension_money(
            &mut report.by_route,
            entry.dimensions.route.as_deref(),
            &entry.amount,
        );
        add_optional_dimension_money(
            &mut report.by_actor,
            entry.dimensions.actor.as_deref(),
            &entry.amount,
        );
        add_optional_dimension_money(
            &mut report.by_tenant,
            entry.dimensions.tenant.as_deref(),
            &entry.amount,
        );
        report.time_window.observe(entry.recorded_at_unix_ms);
        report.entries.push(CostSummary {
            action: entry.action.clone(),
            minor_units: entry.amount.minor_units,
            currency: entry.amount.currency.clone(),
            dimensions: entry.dimensions.clone(),
            recorded_at_unix_ms: entry.recorded_at_unix_ms,
        });
    }
    report
}

fn add_money(totals: &mut BTreeMap<String, i128>, amount: &Money) {
    *totals.entry(amount.currency.clone()).or_insert(0) += amount.minor_units;
}

fn add_optional_dimension_money(
    totals: &mut BTreeMap<String, BTreeMap<String, i128>>,
    dimension: Option<&str>,
    amount: &Money,
) {
    let Some(dimension) = dimension else {
        return;
    };
    add_money(
        totals
            .entry(dimension.to_string())
            .or_insert_with(BTreeMap::new),
        amount,
    );
}

fn money_map_to_json(totals: &BTreeMap<String, i128>) -> Vec<Value> {
    totals
        .iter()
        .map(|(currency, minor_units)| {
            json!({
                "currency": currency,
                "minor_units": minor_units,
            })
        })
        .collect()
}

fn nested_money_map_to_json(totals: &BTreeMap<String, BTreeMap<String, i128>>) -> Vec<Value> {
    totals
        .iter()
        .map(|(key, currencies)| {
            json!({
                "key": key,
                "totals": money_map_to_json(currencies),
            })
        })
        .collect()
}

fn dimensions_to_json(dimensions: &CostDimensions) -> Value {
    json!({
        "connector": dimensions.connector,
        "model": dimensions.model,
        "workflow": dimensions.workflow,
        "route": dimensions.route,
        "actor": dimensions.actor,
        "tenant": dimensions.tenant,
    })
}

fn render_money(minor_units: i128, currency: &str) -> String {
    let sign = if minor_units < 0 { "-" } else { "" };
    let absolute = minor_units.checked_abs().unwrap_or(i128::MAX);
    format!("{sign}{}.{:02} {currency}", absolute / 100, absolute % 100)
}

#[cfg(test)]
mod tests {
    use super::summarize_cost_entries;
    use crate::{
        cost::{CostDimensions, CostEntry},
        Money,
    };
    use serde_json::json;

    #[test]
    fn cost_report_summarizes_entries() {
        let entries = vec![
            cost("charge_card", 125, "USD"),
            cost("charge_card", 75, "USD"),
            cost("send_sms", 30, "KZT"),
        ];

        let report = summarize_cost_entries(&entries);

        assert_eq!(report.total_entries, 3);
        assert_eq!(report.by_currency.get("USD"), Some(&200));
        assert_eq!(report.by_action["charge_card"].get("USD"), Some(&200));
        assert_eq!(report.entries[2].currency, "KZT");
        assert!(report.render_text().contains("2.00 USD"));
        assert_eq!(report.to_json()["total_entries"], 3);
        assert_eq!(
            report.to_json()["schema_version"],
            super::COST_DASHBOARD_SCHEMA_VERSION
        );
        assert_eq!(report.to_json()["raw_entries"][0]["index"], 0);
        assert_eq!(
            report.to_json()["totals"]["by_currency"][0],
            json!({
                "currency": "KZT",
                "minor_units": 30,
            })
        );
    }

    #[test]
    fn cost_dashboard_read_model_groups_available_dimensions() {
        let entries = dimensioned_entries();

        let report = summarize_cost_entries(&entries);
        let json = report.to_json();

        assert_eq!(report.by_connector["ai.complete"]["USD"], 200);
        assert_eq!(report.by_model["gpt-4.1-mini"]["USD"], 200);
        assert_eq!(report.by_workflow["refund"]["USD"], 200);
        assert_eq!(report.by_tenant["tenant_a"]["USD"], 200);
        assert_eq!(report.by_actor["user_1"]["USD"], 125);
        assert_eq!(
            json["time_window"]["start_unix_ms"],
            json!(1_700_000_000_000i64)
        );
        assert_eq!(
            json["time_window"]["end_unix_ms"],
            json!(1_700_000_000_500i64)
        );
        assert_eq!(
            json["raw_entries"][0]["dimensions"]["connector"],
            "ai.complete"
        );
        assert_eq!(json["totals"]["by_route"], json!([]));
    }

    #[test]
    fn cost_dashboard_read_model_matches_fixture() {
        let report = summarize_cost_entries(&dimensioned_entries());
        let expected: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/cost_dashboard_v1.json")).unwrap();

        assert_eq!(report.to_json(), expected);
    }

    fn cost(action: &str, minor_units: i128, currency: &str) -> CostEntry {
        CostEntry::action_charge(
            action,
            Money {
                minor_units,
                currency: currency.to_string(),
            },
        )
    }

    fn dimensioned_entries() -> Vec<CostEntry> {
        vec![
            CostEntry {
                action: "call_model".to_string(),
                amount: Money {
                    minor_units: 125,
                    currency: "USD".to_string(),
                },
                dimensions: CostDimensions {
                    connector: Some("ai.complete".to_string()),
                    model: Some("gpt-4.1-mini".to_string()),
                    workflow: Some("refund".to_string()),
                    route: None,
                    actor: Some("user_1".to_string()),
                    tenant: Some("tenant_a".to_string()),
                },
                recorded_at_unix_ms: Some(1_700_000_000_000),
            },
            CostEntry {
                action: "call_model".to_string(),
                amount: Money {
                    minor_units: 75,
                    currency: "USD".to_string(),
                },
                dimensions: CostDimensions {
                    connector: Some("ai.complete".to_string()),
                    model: Some("gpt-4.1-mini".to_string()),
                    workflow: Some("refund".to_string()),
                    route: None,
                    actor: Some("user_2".to_string()),
                    tenant: Some("tenant_a".to_string()),
                },
                recorded_at_unix_ms: Some(1_700_000_000_500),
            },
        ]
    }
}
