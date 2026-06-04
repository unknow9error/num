use crate::{cost::CostEntry, Money};
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CostReport {
    pub total_entries: usize,
    pub by_currency: BTreeMap<String, i128>,
    pub by_action: BTreeMap<String, BTreeMap<String, i128>>,
    pub entries: Vec<CostSummary>,
}

impl CostReport {
    pub fn to_json(&self) -> Value {
        json!({
            "total_entries": self.total_entries,
            "by_currency": self.by_currency,
            "by_action": self.by_action,
            "entries": self.entries.iter().map(CostSummary::to_json).collect::<Vec<_>>(),
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
}

impl CostSummary {
    fn to_json(&self) -> Value {
        json!({
            "action": self.action,
            "amount": {
                "minor_units": self.minor_units,
                "currency": self.currency,
            }
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
        report.entries.push(CostSummary {
            action: entry.action.clone(),
            minor_units: entry.amount.minor_units,
            currency: entry.amount.currency.clone(),
        });
    }
    report
}

fn add_money(totals: &mut BTreeMap<String, i128>, amount: &Money) {
    *totals.entry(amount.currency.clone()).or_insert(0) += amount.minor_units;
}

fn render_money(minor_units: i128, currency: &str) -> String {
    let sign = if minor_units < 0 { "-" } else { "" };
    let absolute = minor_units.checked_abs().unwrap_or(i128::MAX);
    format!("{sign}{}.{:02} {currency}", absolute / 100, absolute % 100)
}

#[cfg(test)]
mod tests {
    use super::summarize_cost_entries;
    use crate::{cost::CostEntry, Money};

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
    }

    fn cost(action: &str, minor_units: i128, currency: &str) -> CostEntry {
        CostEntry {
            action: action.to_string(),
            amount: Money {
                minor_units,
                currency: currency.to_string(),
            },
        }
    }
}
