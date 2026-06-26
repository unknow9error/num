use crate::{Money, RuntimeError};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CostEntry {
    pub action: String,
    pub amount: Money,
    pub dimensions: CostDimensions,
    pub recorded_at_unix_ms: Option<i128>,
}

impl CostEntry {
    pub fn action_charge(action: impl Into<String>, amount: Money) -> Self {
        Self {
            action: action.into(),
            amount,
            dimensions: CostDimensions::default(),
            recorded_at_unix_ms: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CostDimensions {
    pub connector: Option<String>,
    pub model: Option<String>,
    pub workflow: Option<String>,
    pub route: Option<String>,
    pub request_id: Option<String>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
    pub tenant: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CostLedger {
    spent: BTreeMap<String, i128>,
    entries: Vec<CostEntry>,
    budget_scopes: Vec<BudgetScope>,
}

#[derive(Debug, Clone)]
struct BudgetScope {
    limits: BTreeMap<String, i128>,
    baseline: BTreeMap<String, i128>,
}

impl CostLedger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_budget(mut self, budget: Money) -> Self {
        self.set_budget(budget);
        self
    }

    pub fn set_budget(&mut self, budget: Money) {
        if self.budget_scopes.is_empty() {
            self.budget_scopes.push(BudgetScope {
                limits: BTreeMap::new(),
                baseline: BTreeMap::new(),
            });
        }
        self.budget_scopes[0]
            .limits
            .insert(budget.currency.clone(), budget.minor_units);
    }

    pub fn push_budget_scope(&mut self, _name: impl Into<String>, budget: Money) {
        let mut limits = BTreeMap::new();
        limits.insert(budget.currency.clone(), budget.minor_units);
        self.budget_scopes.push(BudgetScope {
            limits,
            baseline: self.spent.clone(),
        });
    }

    pub fn pop_budget_scope(&mut self) {
        self.budget_scopes.pop();
    }

    pub fn active_budget_scopes(&self) -> usize {
        self.budget_scopes.len()
    }

    pub fn authorize(&self, amount: &Money) -> Result<(), RuntimeError> {
        let current = self.spent.get(&amount.currency).copied().unwrap_or(0);
        let Some(next) = current.checked_add(amount.minor_units) else {
            return Err(RuntimeError::Storage(format!(
                "cost ledger overflow for currency {}",
                amount.currency
            )));
        };
        self.check_budget_scopes(&amount.currency, next)
    }

    pub fn charge(&mut self, action: impl Into<String>, amount: Money) -> Result<(), RuntimeError> {
        let current = self.spent.get(&amount.currency).copied().unwrap_or(0);
        let Some(next) = current.checked_add(amount.minor_units) else {
            return Err(RuntimeError::Storage(format!(
                "cost ledger overflow for currency {}",
                amount.currency
            )));
        };

        self.check_budget_scopes(&amount.currency, next)?;

        self.spent.insert(amount.currency.clone(), next);
        self.entries.push(CostEntry::action_charge(action, amount));
        Ok(())
    }

    pub fn spent(&self, currency: &str) -> Money {
        Money {
            minor_units: self.spent.get(currency).copied().unwrap_or(0),
            currency: currency.to_string(),
        }
    }

    pub fn entries(&self) -> &[CostEntry] {
        &self.entries
    }

    fn check_budget_scopes(&self, currency: &str, next_total: i128) -> Result<(), RuntimeError> {
        for scope in &self.budget_scopes {
            let Some(limit) = scope.limits.get(currency) else {
                continue;
            };
            let baseline = scope.baseline.get(currency).copied().unwrap_or(0);
            let scoped_actual = next_total.saturating_sub(baseline);
            if scoped_actual > *limit {
                return Err(RuntimeError::CostLimitExceeded {
                    limit: Money {
                        minor_units: *limit,
                        currency: currency.to_string(),
                    },
                    actual: Money {
                        minor_units: scoped_actual,
                        currency: currency.to_string(),
                    },
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::CostLedger;
    use crate::{Money, RuntimeError};

    #[test]
    fn tracks_action_costs_by_currency() {
        let mut ledger = CostLedger::new();

        ledger.charge("first", money(100, "USD")).unwrap();
        ledger.charge("second", money(250, "USD")).unwrap();
        ledger.charge("third", money(90, "KZT")).unwrap();

        assert_eq!(ledger.spent("USD"), money(350, "USD"));
        assert_eq!(ledger.spent("KZT"), money(90, "KZT"));
        assert_eq!(ledger.entries().len(), 3);
        assert_eq!(ledger.entries()[1].action, "second");
    }

    #[test]
    fn rejects_charges_that_exceed_budget() {
        let mut ledger = CostLedger::new().with_budget(money(300, "USD"));

        ledger.charge("first", money(250, "USD")).unwrap();
        let err = ledger.charge("second", money(51, "USD")).unwrap_err();

        assert!(matches!(err, RuntimeError::CostLimitExceeded { .. }));
        assert_eq!(ledger.spent("USD"), money(250, "USD"));
        assert_eq!(ledger.entries().len(), 1);
    }

    #[test]
    fn rejects_authorization_against_child_budget_scope() {
        let mut ledger = CostLedger::new();
        ledger.push_budget_scope("workflow:main", money(1_000, "USD"));
        ledger.push_budget_scope("function:step", money(100, "USD"));

        let err = ledger.authorize(&money(101, "USD")).unwrap_err();

        assert!(matches!(err, RuntimeError::CostLimitExceeded { .. }));
        assert_eq!(ledger.spent("USD"), money(0, "USD"));
        assert_eq!(ledger.entries().len(), 0);
    }

    #[test]
    fn parent_budget_scope_counts_nested_spend() {
        let mut ledger = CostLedger::new();
        ledger.push_budget_scope("workflow:main", money(300, "USD"));
        ledger.charge("first", money(250, "USD")).unwrap();
        ledger.push_budget_scope("function:step", money(200, "USD"));

        let err = ledger.charge("second", money(51, "USD")).unwrap_err();

        assert!(matches!(err, RuntimeError::CostLimitExceeded { .. }));
        assert_eq!(ledger.spent("USD"), money(250, "USD"));
        assert_eq!(ledger.entries().len(), 1);
    }

    fn money(minor_units: i128, currency: &str) -> Money {
        Money {
            minor_units,
            currency: currency.to_string(),
        }
    }
}
