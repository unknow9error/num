use crate::interpreter::Value;
use crate::{ActionSpec, Money, RuntimeError};
use serde_json::{json, Value as JsonValue};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_attempts: u32,
}

impl RetryPolicy {
    pub fn none() -> Self {
        Self { max_attempts: 1 }
    }

    pub fn attempts(max_attempts: u32) -> Self {
        Self {
            max_attempts: max_attempts.max(1),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionOutcome {
    Succeeded(Value),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActionExecutionRecord {
    pub action: String,
    pub idempotency_key: Option<String>,
    pub attempts: u32,
    pub outcome: ExecutionOutcome,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActionExecution {
    pub record: ActionExecutionRecord,
    pub replayed: bool,
}

impl ActionExecution {
    pub fn value(&self) -> Option<&Value> {
        match &self.record.outcome {
            ExecutionOutcome::Succeeded(value) => Some(value),
            ExecutionOutcome::Failed(_) => None,
        }
    }
}

pub trait IdempotencyStore {
    fn load(&self, key: &str) -> Result<Option<ActionExecutionRecord>, RuntimeError>;
    fn save(&mut self, record: ActionExecutionRecord) -> Result<(), RuntimeError>;
}

#[derive(Debug, Default, Clone)]
pub struct MemoryIdempotencyStore {
    records: BTreeMap<String, ActionExecutionRecord>,
}

impl MemoryIdempotencyStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl IdempotencyStore for MemoryIdempotencyStore {
    fn load(&self, key: &str) -> Result<Option<ActionExecutionRecord>, RuntimeError> {
        Ok(self.records.get(key).cloned())
    }

    fn save(&mut self, record: ActionExecutionRecord) -> Result<(), RuntimeError> {
        if let Some(key) = &record.idempotency_key {
            self.records.insert(key.clone(), record);
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct FileIdempotencyStore {
    root: PathBuf,
}

impl FileIdempotencyStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn path(&self, key: &str) -> PathBuf {
        self.root
            .join("idempotency")
            .join(format!("{}.json", safe_file_id(key)))
    }
}

impl IdempotencyStore for FileIdempotencyStore {
    fn load(&self, key: &str) -> Result<Option<ActionExecutionRecord>, RuntimeError> {
        let path = self.path(key);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(path).map_storage()?;
        let value: JsonValue = serde_json::from_slice(&bytes).map_storage()?;
        json_to_record(&value).map(Some)
    }

    fn save(&mut self, record: ActionExecutionRecord) -> Result<(), RuntimeError> {
        let Some(key) = &record.idempotency_key else {
            return Ok(());
        };
        let path = self.path(key);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_storage()?;
        }
        let temp_path = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(&record_to_json(&record)).map_storage()?;
        fs::write(&temp_path, bytes).map_storage()?;
        fs::rename(temp_path, path).map_storage()
    }
}

pub struct ActionExecutor<S> {
    idempotency: S,
}

impl<S: IdempotencyStore> ActionExecutor<S> {
    pub fn new(idempotency: S) -> Self {
        Self { idempotency }
    }

    pub fn into_store(self) -> S {
        self.idempotency
    }

    pub fn execute<F>(
        &mut self,
        action: &ActionSpec,
        retry: RetryPolicy,
        actual_cost: Option<Money>,
        mut operation: F,
    ) -> Result<ActionExecution, RuntimeError>
    where
        F: FnMut(u32) -> Result<Value, RuntimeError>,
    {
        enforce_cost(action, actual_cost)?;

        if let Some(key) = &action.idempotency_key {
            if let Some(record) = self.idempotency.load(key)? {
                if matches!(record.outcome, ExecutionOutcome::Succeeded(_)) {
                    return Ok(ActionExecution {
                        record,
                        replayed: true,
                    });
                }
            }
        }

        let mut attempts = 0;
        loop {
            attempts += 1;
            let attempt_result = if action.timeout == Some(std::time::Duration::ZERO) {
                Err(RuntimeError::Timeout {
                    action: action.name.clone(),
                })
            } else {
                let started = Instant::now();
                let result = operation(attempts);
                if let Some(timeout) = action.timeout {
                    if started.elapsed() > timeout {
                        Err(RuntimeError::Timeout {
                            action: action.name.clone(),
                        })
                    } else {
                        result
                    }
                } else {
                    result
                }
            };

            match attempt_result {
                Ok(value) => {
                    let record = ActionExecutionRecord {
                        action: action.name.clone(),
                        idempotency_key: action.idempotency_key.clone(),
                        attempts,
                        outcome: ExecutionOutcome::Succeeded(value),
                    };
                    self.idempotency.save(record.clone())?;
                    return Ok(ActionExecution {
                        record,
                        replayed: false,
                    });
                }
                Err(err) if attempts < retry.max_attempts && is_retryable(&err) => {}
                Err(err) => {
                    let record = ActionExecutionRecord {
                        action: action.name.clone(),
                        idempotency_key: action.idempotency_key.clone(),
                        attempts,
                        outcome: ExecutionOutcome::Failed(error_message(&err)),
                    };
                    self.idempotency.save(record)?;
                    return Err(err);
                }
            }
        }
    }
}

fn enforce_cost(action: &ActionSpec, actual_cost: Option<Money>) -> Result<(), RuntimeError> {
    let (Some(limit), Some(actual)) = (&action.max_cost, actual_cost) else {
        return Ok(());
    };
    if limit.currency == actual.currency && actual.minor_units > limit.minor_units {
        Err(RuntimeError::CostLimitExceeded {
            limit: limit.clone(),
            actual,
        })
    } else {
        Ok(())
    }
}

fn is_retryable(error: &RuntimeError) -> bool {
    matches!(
        error,
        RuntimeError::ActionFailed { .. } | RuntimeError::Timeout { .. } | RuntimeError::Storage(_)
    )
}

fn error_message(error: &RuntimeError) -> String {
    match error {
        RuntimeError::PermissionDenied { permission } => {
            format!("permission denied: {permission}")
        }
        RuntimeError::CostLimitExceeded { limit, actual } => format!(
            "cost limit exceeded: actual {} {}, limit {} {}",
            actual.minor_units, actual.currency, limit.minor_units, limit.currency
        ),
        RuntimeError::RateLimitExceeded { scope, limit } => {
            format!("rate limit exceeded for {scope}: limit {limit}")
        }
        RuntimeError::Timeout { action } => format!("timeout: {action}"),
        RuntimeError::ActionFailed { action, reason } => {
            format!("action failed: {action}: {reason}")
        }
        RuntimeError::SanitizationFailed { reason } => format!("sanitization failed: {reason}"),
        RuntimeError::TenantIsolationViolation { expected, actual } => {
            format!("tenant isolation violation: expected {expected}, actual {actual}")
        }
        RuntimeError::SecretNotFound { name } => format!("secret not found: {name}"),
        RuntimeError::Storage(message) => format!("storage: {message}"),
    }
}

fn record_to_json(record: &ActionExecutionRecord) -> JsonValue {
    json!({
        "action": record.action,
        "idempotency_key": record.idempotency_key,
        "attempts": record.attempts,
        "outcome": outcome_to_json(&record.outcome),
    })
}

fn json_to_record(value: &JsonValue) -> Result<ActionExecutionRecord, RuntimeError> {
    Ok(ActionExecutionRecord {
        action: string_field(value, "action")?,
        idempotency_key: value
            .get("idempotency_key")
            .and_then(JsonValue::as_str)
            .map(str::to_string),
        attempts: u32_field(value, "attempts")?,
        outcome: json_to_outcome(
            value
                .get("outcome")
                .ok_or_else(|| storage_error("missing outcome"))?,
        )?,
    })
}

fn outcome_to_json(outcome: &ExecutionOutcome) -> JsonValue {
    match outcome {
        ExecutionOutcome::Succeeded(value) => {
            json!({"kind": "Succeeded", "value": runtime_value_to_json(value)})
        }
        ExecutionOutcome::Failed(reason) => json!({"kind": "Failed", "reason": reason}),
    }
}

fn json_to_outcome(value: &JsonValue) -> Result<ExecutionOutcome, RuntimeError> {
    match string_field(value, "kind")?.as_str() {
        "Succeeded" => Ok(ExecutionOutcome::Succeeded(json_to_runtime_value(
            value
                .get("value")
                .ok_or_else(|| storage_error("missing successful execution value"))?,
        )?)),
        "Failed" => Ok(ExecutionOutcome::Failed(string_field(value, "reason")?)),
        other => Err(storage_error(format!(
            "unknown execution outcome '{other}'"
        ))),
    }
}

fn runtime_value_to_json(value: &Value) -> JsonValue {
    match value {
        Value::Null => json!({"kind": "Null"}),
        Value::Bool(value) => json!({"kind": "Bool", "value": value}),
        Value::Int(value) => json!({"kind": "Int", "value": value}),
        Value::Float(value) => json!({"kind": "Float", "value": value}),
        Value::String(value) => json!({"kind": "String", "value": value}),
        Value::Money(minor_units, currency) => {
            json!({"kind": "Money", "minor_units": minor_units, "currency": currency})
        }
        Value::Brand(name, inner) => {
            json!({"kind": "Brand", "name": name, "value": runtime_value_to_json(inner)})
        }
        Value::Uncertain(inner, confidence) => {
            json!({"kind": "Uncertain", "value": runtime_value_to_json(inner), "confidence": confidence})
        }
        Value::List(items) => {
            let items = items.iter().map(runtime_value_to_json).collect::<Vec<_>>();
            json!({"kind": "List", "items": items})
        }
        Value::Struct(name, fields) => {
            let fields = fields
                .iter()
                .map(|(key, value)| (key.clone(), runtime_value_to_json(value)))
                .collect::<serde_json::Map<_, _>>();
            json!({"kind": "Struct", "name": name, "fields": fields})
        }
        Value::Enum(name, variant, payload) => {
            let mut value = json!({"kind": "Enum", "name": name, "variant": variant});
            if let Some(payload) = payload {
                value["payload"] = runtime_value_to_json(payload);
            }
            value
        }
        Value::Quantity(amount, unit) => {
            json!({"kind": "Quantity", "amount": amount, "unit": unit})
        }
    }
}

fn json_to_runtime_value(value: &JsonValue) -> Result<Value, RuntimeError> {
    match string_field(value, "kind")?.as_str() {
        "Bool" => Ok(Value::Bool(bool_field(value, "value")?)),
        "Int" => Ok(Value::Int(i64_field(value, "value")?)),
        "Float" => Ok(Value::Float(f64_field(value, "value")?)),
        "String" => Ok(Value::String(string_field(value, "value")?)),
        "Money" => Ok(Value::Money(
            i128_field(value, "minor_units")?,
            string_field(value, "currency")?,
        )),
        "Brand" => Ok(Value::Brand(
            string_field(value, "name")?,
            Box::new(json_to_runtime_value(
                value
                    .get("value")
                    .ok_or_else(|| storage_error("missing brand value"))?,
            )?),
        )),
        "Uncertain" => Ok(Value::Uncertain(
            Box::new(json_to_runtime_value(
                value
                    .get("value")
                    .ok_or_else(|| storage_error("missing uncertain value"))?,
            )?),
            f64_field(value, "confidence")?,
        )),
        "List" => {
            let items = value
                .get("items")
                .and_then(JsonValue::as_array)
                .ok_or_else(|| storage_error("missing list items"))?
                .iter()
                .map(json_to_runtime_value)
                .collect::<Result<Vec<_>, RuntimeError>>()?;
            Ok(Value::List(items))
        }
        "Struct" => {
            let name = string_field(value, "name")?;
            let fields = value
                .get("fields")
                .and_then(JsonValue::as_object)
                .ok_or_else(|| storage_error("missing struct fields"))?
                .iter()
                .map(|(key, value)| Ok((key.clone(), json_to_runtime_value(value)?)))
                .collect::<Result<HashMap<_, _>, RuntimeError>>()?;
            Ok(Value::Struct(name, fields))
        }
        "Enum" => Ok(Value::Enum(
            string_field(value, "name")?,
            string_field(value, "variant")?,
            value
                .get("payload")
                .map(json_to_runtime_value)
                .transpose()?
                .map(Box::new),
        )),
        "Quantity" => Ok(Value::Quantity(
            f64_field(value, "amount")?,
            string_field(value, "unit")?,
        )),
        other => Err(storage_error(format!(
            "unknown runtime value kind '{other}'"
        ))),
    }
}

fn string_field(value: &JsonValue, key: &str) -> Result<String, RuntimeError> {
    value
        .get(key)
        .and_then(JsonValue::as_str)
        .map(str::to_string)
        .ok_or_else(|| storage_error(format!("missing string field '{key}'")))
}

fn bool_field(value: &JsonValue, key: &str) -> Result<bool, RuntimeError> {
    value
        .get(key)
        .and_then(JsonValue::as_bool)
        .ok_or_else(|| storage_error(format!("missing bool field '{key}'")))
}

fn u32_field(value: &JsonValue, key: &str) -> Result<u32, RuntimeError> {
    value
        .get(key)
        .and_then(JsonValue::as_u64)
        .and_then(|value| value.try_into().ok())
        .ok_or_else(|| storage_error(format!("missing u32 field '{key}'")))
}

fn i64_field(value: &JsonValue, key: &str) -> Result<i64, RuntimeError> {
    value
        .get(key)
        .and_then(JsonValue::as_i64)
        .ok_or_else(|| storage_error(format!("missing i64 field '{key}'")))
}

fn i128_field(value: &JsonValue, key: &str) -> Result<i128, RuntimeError> {
    value
        .get(key)
        .and_then(JsonValue::as_i64)
        .map(i128::from)
        .ok_or_else(|| storage_error(format!("missing i128 field '{key}'")))
}

fn f64_field(value: &JsonValue, key: &str) -> Result<f64, RuntimeError> {
    value
        .get(key)
        .and_then(JsonValue::as_f64)
        .ok_or_else(|| storage_error(format!("missing f64 field '{key}'")))
}

fn safe_file_id(id: &str) -> String {
    id.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn storage_error(message: impl Into<String>) -> RuntimeError {
    RuntimeError::Storage(message.into())
}

trait MapStorage<T> {
    fn map_storage(self) -> Result<T, RuntimeError>;
}

impl<T, E: std::fmt::Display> MapStorage<T> for Result<T, E> {
    fn map_storage(self) -> Result<T, RuntimeError> {
        self.map_err(|err| RuntimeError::Storage(err.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ActionExecutor, ExecutionOutcome, FileIdempotencyStore, IdempotencyStore,
        MemoryIdempotencyStore, RetryPolicy,
    };
    use crate::interpreter::Value;
    use crate::{ActionSpec, Money, RiskLevel, RuntimeError};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[test]
    fn retries_retryable_action_failures_until_success() {
        let action = action_spec("issue_refund", None);
        let mut executor = ActionExecutor::new(MemoryIdempotencyStore::new());
        let mut calls = 0;

        let result = executor
            .execute(&action, RetryPolicy::attempts(3), None, |attempt| {
                calls += 1;
                if attempt < 3 {
                    return Err(RuntimeError::ActionFailed {
                        action: "issue_refund".to_string(),
                        reason: "temporary gateway error".to_string(),
                    });
                }
                Ok(Value::String("ok".to_string()))
            })
            .unwrap();

        assert_eq!(calls, 3);
        assert!(!result.replayed);
        assert_eq!(result.record.attempts, 3);
        assert_eq!(result.value(), Some(&Value::String("ok".to_string())));
    }

    #[test]
    fn stores_failed_record_after_retry_budget_is_exhausted() {
        let action = action_spec("issue_refund", Some("refund:pay_1"));
        let mut executor = ActionExecutor::new(MemoryIdempotencyStore::new());

        let error = executor
            .execute(&action, RetryPolicy::attempts(2), None, |_| {
                Err(RuntimeError::ActionFailed {
                    action: "issue_refund".to_string(),
                    reason: "gateway down".to_string(),
                })
            })
            .unwrap_err();
        assert!(matches!(error, RuntimeError::ActionFailed { .. }));

        let store = executor.into_store();
        let record = store.load("refund:pay_1").unwrap().unwrap();
        assert_eq!(record.attempts, 2);
        assert!(matches!(record.outcome, ExecutionOutcome::Failed(_)));
    }

    #[test]
    fn replays_successful_idempotency_key_without_invoking_action() {
        let action = action_spec("issue_refund", Some("refund:pay_1"));
        let mut executor = ActionExecutor::new(MemoryIdempotencyStore::new());
        let mut calls = 0;

        executor
            .execute(&action, RetryPolicy::none(), None, |_| {
                calls += 1;
                Ok(Value::String("ref_1".to_string()))
            })
            .unwrap();
        let replayed = executor
            .execute(&action, RetryPolicy::none(), None, |_| {
                calls += 1;
                Ok(Value::String("ref_2".to_string()))
            })
            .unwrap();

        assert_eq!(calls, 1);
        assert!(replayed.replayed);
        assert_eq!(replayed.value(), Some(&Value::String("ref_1".to_string())));
    }

    #[test]
    fn rejects_action_when_cost_exceeds_limit() {
        let mut action = action_spec("render_report", None);
        action.max_cost = Some(Money {
            minor_units: 100,
            currency: "USD".to_string(),
        });
        let mut executor = ActionExecutor::new(MemoryIdempotencyStore::new());
        let error = executor
            .execute(
                &action,
                RetryPolicy::none(),
                Some(Money {
                    minor_units: 101,
                    currency: "USD".to_string(),
                }),
                |_| Ok(Value::Null),
            )
            .unwrap_err();

        assert!(matches!(error, RuntimeError::CostLimitExceeded { .. }));
    }

    #[test]
    fn enforces_timeout_budget_for_action_execution() {
        let mut action = action_spec("slow_action", Some("slow:1"));
        action.timeout = Some(Duration::from_millis(1));
        let mut executor = ActionExecutor::new(MemoryIdempotencyStore::new());

        let error = executor
            .execute(&action, RetryPolicy::none(), None, |_| {
                std::thread::sleep(Duration::from_millis(5));
                Ok(Value::String("late".to_string()))
            })
            .unwrap_err();

        assert!(matches!(error, RuntimeError::Timeout { .. }));
        let store = executor.into_store();
        let record = store.load("slow:1").unwrap().unwrap();
        assert_eq!(record.attempts, 1);
        assert!(matches!(record.outcome, ExecutionOutcome::Failed(_)));
    }

    #[test]
    fn file_idempotency_store_round_trips_successful_records() {
        let root = unique_test_dir("idempotency");
        let action = action_spec("issue_refund", Some("refund/pay_1"));
        let mut executor = ActionExecutor::new(FileIdempotencyStore::new(&root));
        executor
            .execute(&action, RetryPolicy::none(), None, |_| {
                Ok(Value::Money(2500, "USD".to_string()))
            })
            .unwrap();

        let store = FileIdempotencyStore::new(&root);
        let record = store.load("refund/pay_1").unwrap().unwrap();

        assert_eq!(record.action, "issue_refund");
        assert_eq!(record.idempotency_key.as_deref(), Some("refund/pay_1"));
        assert_eq!(
            record.outcome,
            ExecutionOutcome::Succeeded(Value::Money(2500, "USD".to_string()))
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn file_idempotency_store_round_trips_enum_payload_records() {
        let root = unique_test_dir("enum-payload");
        let action = action_spec("classify", Some("classify/1"));
        let mut executor = ActionExecutor::new(FileIdempotencyStore::new(&root));
        executor
            .execute(&action, RetryPolicy::none(), None, |_| {
                Ok(Value::Enum(
                    "PaymentStatus".to_string(),
                    "Failed".to_string(),
                    Some(Box::new(Value::String("network".to_string()))),
                ))
            })
            .unwrap();

        let store = FileIdempotencyStore::new(&root);
        let record = store.load("classify/1").unwrap().unwrap();

        assert_eq!(
            record.outcome,
            ExecutionOutcome::Succeeded(Value::Enum(
                "PaymentStatus".to_string(),
                "Failed".to_string(),
                Some(Box::new(Value::String("network".to_string()))),
            ))
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn file_idempotency_store_round_trips_list_records() {
        let root = unique_test_dir("list");
        let action = action_spec("list_users", Some("users/list"));
        let mut executor = ActionExecutor::new(FileIdempotencyStore::new(&root));
        executor
            .execute(&action, RetryPolicy::none(), None, |_| {
                Ok(Value::List(vec![Value::String("user_1".to_string())]))
            })
            .unwrap();

        let store = FileIdempotencyStore::new(&root);
        let record = store.load("users/list").unwrap().unwrap();

        assert_eq!(
            record.outcome,
            ExecutionOutcome::Succeeded(Value::List(vec![Value::String("user_1".to_string())]))
        );
        let _ = std::fs::remove_dir_all(root);
    }

    fn action_spec(name: &str, idempotency_key: Option<&str>) -> ActionSpec {
        ActionSpec {
            name: name.to_string(),
            required_permissions: vec![],
            risk: RiskLevel::Low,
            timeout: Some(Duration::from_secs(1)),
            rollback: None,
            idempotency_key: idempotency_key.map(str::to_string),
            max_cost: None,
        }
    }

    fn unique_test_dir(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "num-runtime-execution-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
}
