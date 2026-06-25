use crate::connectors::{ConnectorCallContext, ConnectorError, ConnectorExecutor};
use crate::interpreter::Value;
use crate::redaction;
use serde_json::{json, Map, Value as JsonValue};
use std::collections::HashMap;
use std::io::{ErrorKind, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessConnectorConfig {
    pub method: String,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct ProcessConnectorExecutor {
    configs: HashMap<String, ProcessConnectorConfig>,
}

impl ProcessConnectorExecutor {
    pub fn new(configs: Vec<ProcessConnectorConfig>) -> Self {
        Self {
            configs: configs
                .into_iter()
                .map(|config| (config.method.clone(), config))
                .collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.configs.is_empty()
    }

    fn run(
        config: &ProcessConnectorConfig,
        method: &str,
        args: &[Value],
        context: Option<&ConnectorCallContext>,
    ) -> Result<Value, ConnectorError> {
        let mut command = Command::new(&config.command);
        command.args(&config.args);
        if let Some(cwd) = &config.cwd {
            command.current_dir(cwd);
        }
        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        let mut child = command.spawn().map_err(|err| {
            ConnectorError::new(
                "start_failed",
                format!(
                    "failed to start connector `{method}` command `{}`: {err}",
                    config.command
                ),
                false,
            )
        })?;

        let input = json!({
            "method": method,
            "args": args.iter().map(value_to_json).collect::<Vec<_>>(),
            "egress": context.map(ConnectorCallContext::to_json),
        });
        let stdin_write_error = if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(input.to_string().as_bytes()).err()
        } else {
            None
        };

        let output = wait_with_optional_timeout(child, method, config.timeout_ms)?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(ConnectorError::new(
                "nonzero_exit",
                if stderr.is_empty() {
                    format!("connector `{method}` exited with {}", output.status)
                } else {
                    format!(
                        "connector `{method}` exited with {}: {stderr}",
                        output.status
                    )
                },
                true,
            ));
        }
        if let Some(err) = stdin_write_error {
            if err.kind() != ErrorKind::BrokenPipe {
                return Err(ConnectorError::new(
                    "stdin_write_failed",
                    format!("failed to write connector `{method}` stdin: {err}"),
                    true,
                ));
            }
        }

        let stdout = String::from_utf8(output.stdout).map_err(|err| {
            ConnectorError::new(
                "invalid_stdout",
                format!("connector `{method}` wrote non-UTF8 stdout: {err}"),
                false,
            )
        })?;
        let stdout = stdout.trim();
        if stdout.is_empty() {
            return Ok(Value::Null);
        }
        let json = serde_json::from_str(stdout).map_err(|err| {
            ConnectorError::new(
                "invalid_json",
                format!("connector `{method}` returned invalid JSON: {err}"),
                false,
            )
        })?;
        value_from_json(&json)
            .map_err(|message| ConnectorError::new("invalid_json", message, false))
    }
}

fn wait_with_optional_timeout(
    mut child: std::process::Child,
    method: &str,
    timeout_ms: Option<u64>,
) -> Result<std::process::Output, ConnectorError> {
    let Some(timeout_ms) = timeout_ms else {
        return child.wait_with_output().map_err(|err| {
            ConnectorError::new(
                "wait_failed",
                format!("failed to wait for connector `{method}`: {err}"),
                true,
            )
        });
    };
    let timeout = Duration::from_millis(timeout_ms);
    let started = Instant::now();

    loop {
        if child
            .try_wait()
            .map_err(|err| {
                ConnectorError::new(
                    "poll_failed",
                    format!("failed to poll connector `{method}`: {err}"),
                    true,
                )
            })?
            .is_some()
        {
            return child.wait_with_output().map_err(|err| {
                ConnectorError::new(
                    "wait_failed",
                    format!("failed to collect connector `{method}` output: {err}"),
                    true,
                )
            });
        }

        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ConnectorError::new(
                "timeout",
                format!("connector `{method}` exceeded timeout of {timeout_ms}ms"),
                true,
            ));
        }

        let remaining = timeout.saturating_sub(started.elapsed());
        thread::sleep(remaining.min(Duration::from_millis(10)));
    }
}

impl ConnectorExecutor for ProcessConnectorExecutor {
    fn call(&self, name: &str, args: &[Value]) -> Option<Result<Value, ConnectorError>> {
        self.configs
            .get(name)
            .map(|config| Self::run(config, name, args, None))
    }

    fn call_with_context(
        &self,
        context: &ConnectorCallContext,
        args: &[Value],
    ) -> Option<Result<Value, ConnectorError>> {
        self.configs
            .get(&context.method)
            .map(|config| Self::run(config, &context.method, args, Some(context)))
    }
}

pub fn value_to_json(value: &Value) -> JsonValue {
    match value {
        Value::Null => JsonValue::Null,
        Value::Bool(value) => JsonValue::Bool(*value),
        Value::Int(value) => json!(value),
        Value::Float(value) => json!(value),
        Value::String(value) => JsonValue::String(value.clone()),
        Value::Money(minor_units, currency) => json!({
            "minor_units": minor_units,
            "currency": currency,
        }),
        Value::Brand(name, value) => json!({
            "$brand": name,
            "value": value_to_json(value),
        }),
        Value::Secret(_) => JsonValue::String(redaction::REDACTION_MARKER.to_string()),
        Value::Uncertain(value, confidence) => json!({
            "$uncertain": value_to_json(value),
            "confidence": confidence,
        }),
        Value::List(items) => JsonValue::Array(items.iter().map(value_to_json).collect()),
        Value::Struct(name, fields) => {
            let mut object = Map::new();
            if name != "Object" {
                object.insert("$type".to_string(), JsonValue::String(name.clone()));
            }
            for (key, value) in fields {
                object.insert(key.clone(), value_to_json(value));
            }
            JsonValue::Object(object)
        }
        Value::Enum(name, variant, payload) => {
            let mut object = Map::new();
            object.insert(
                "$enum".to_string(),
                JsonValue::String(format!("{name}.{variant}")),
            );
            if let Some(payload) = payload {
                object.insert("payload".to_string(), value_to_json(payload));
            }
            JsonValue::Object(object)
        }
        Value::Quantity(amount, unit) => json!({
            "$quantity": amount,
            "unit": unit,
        }),
    }
}

pub fn value_from_json(json: &JsonValue) -> Result<Value, String> {
    match json {
        JsonValue::Null => Ok(Value::Null),
        JsonValue::Bool(value) => Ok(Value::Bool(*value)),
        JsonValue::Number(value) => {
            if let Some(integer) = value.as_i64() {
                Ok(Value::Int(integer))
            } else if let Some(float) = value.as_f64() {
                Ok(Value::Float(float))
            } else {
                Err(format!("unsupported JSON number `{value}`"))
            }
        }
        JsonValue::String(value) => Ok(Value::String(value.clone())),
        JsonValue::Array(items) => items
            .iter()
            .map(value_from_json)
            .collect::<Result<Vec<_>, _>>()
            .map(Value::List),
        JsonValue::Object(object) => object_from_json(object),
    }
}

fn object_from_json(object: &Map<String, JsonValue>) -> Result<Value, String> {
    if let (Some(amount), Some(unit)) = (
        object
            .get("$quantity")
            .and_then(|v| v.as_f64().or_else(|| v.as_i64().map(|i| i as f64))),
        object.get("unit").and_then(JsonValue::as_str),
    ) {
        return Ok(Value::Quantity(amount, unit.to_string()));
    }

    if let (Some(minor_units), Some(currency)) = (
        object.get("minor_units").and_then(JsonValue::as_i64),
        object.get("currency").and_then(JsonValue::as_str),
    ) {
        if object.len() == 2 {
            return Ok(Value::Money(i128::from(minor_units), currency.to_string()));
        }
    }

    if let Some(enum_name) = object.get("$enum").and_then(JsonValue::as_str) {
        let (name, variant) = enum_name
            .split_once('.')
            .or_else(|| enum_name.split_once("::"))
            .ok_or_else(|| format!("invalid `$enum` value `{enum_name}`"))?;
        let payload = object
            .get("payload")
            .map(value_from_json)
            .transpose()?
            .map(Box::new);
        return Ok(Value::Enum(name.to_string(), variant.to_string(), payload));
    }

    if let Some(value) = object.get("$uncertain") {
        let confidence = object
            .get("confidence")
            .and_then(JsonValue::as_f64)
            .ok_or_else(|| {
                "`$uncertain` connector output requires numeric `confidence`".to_string()
            })?;
        return Ok(Value::Uncertain(
            Box::new(value_from_json(value)?),
            confidence,
        ));
    }

    if let (Some(name), Some(value)) = (
        object.get("$brand").and_then(JsonValue::as_str),
        object.get("value"),
    ) {
        return Ok(Value::Brand(
            name.to_string(),
            Box::new(value_from_json(value)?),
        ));
    }

    if let Some(value) = object.get("$secret") {
        return Ok(Value::Secret(Box::new(value_from_json(value)?)));
    }

    let type_name = object
        .get("$type")
        .and_then(JsonValue::as_str)
        .unwrap_or("Object")
        .to_string();
    let mut fields = HashMap::new();
    for (key, value) in object {
        if key.starts_with('$') {
            continue;
        }
        fields.insert(key.clone(), value_from_json(value)?);
    }
    Ok(Value::Struct(type_name, fields))
}

#[cfg(test)]
mod tests {
    use super::{value_from_json, value_to_json, ProcessConnectorConfig, ProcessConnectorExecutor};
    use crate::connectors::{ConnectorArgLabel, ConnectorCallContext, ConnectorExecutor};
    use crate::interpreter::Value;
    use serde_json::json;

    #[test]
    fn converts_money_and_struct_values_to_json() {
        let mut fields = std::collections::HashMap::new();
        fields.insert("amount".to_string(), Value::Money(15000, "KZT".to_string()));
        let value = Value::Struct("Payment".to_string(), fields);

        let json = value_to_json(&value);

        assert_eq!(
            json,
            json!({
                "$type": "Payment",
                "amount": { "minor_units": 15000, "currency": "KZT" }
            })
        );
    }

    #[test]
    fn converts_json_contract_values_to_runtime_values() {
        let value = value_from_json(&json!({
            "$uncertain": { "$enum": "RiskLevel.Low" },
            "confidence": 0.92
        }))
        .unwrap();

        assert_eq!(
            value,
            Value::Uncertain(
                Box::new(Value::Enum(
                    "RiskLevel".to_string(),
                    "Low".to_string(),
                    None
                )),
                0.92
            )
        );
    }

    #[test]
    fn converts_secret_values_to_redacted_json() {
        let value = Value::Secret(Box::new(Value::String("sk_live_process".to_string())));

        let json = value_to_json(&value);

        assert_eq!(json, json!("<redacted>"));
    }

    #[cfg(unix)]
    #[test]
    fn process_connector_executes_configured_command() {
        let executor = ProcessConnectorExecutor::new(vec![ProcessConnectorConfig {
            method: "echo.text".to_string(),
            command: "/bin/sh".to_string(),
            args: vec![
                "-c".to_string(),
                "cat >/dev/null; printf '%s' '\"ok\"'".to_string(),
            ],
            cwd: None,
            timeout_ms: None,
        }]);

        let result = executor.call("echo.text", &[]).unwrap().unwrap();

        assert_eq!(result, Value::String("ok".to_string()));
    }

    #[cfg(unix)]
    #[test]
    fn process_connector_allows_successful_command_that_closes_stdin() {
        let executor = ProcessConnectorExecutor::new(vec![ProcessConnectorConfig {
            method: "echo.text".to_string(),
            command: "/bin/sh".to_string(),
            args: vec![
                "-c".to_string(),
                "exec 0<&-; printf '%s' '\"ok\"'".to_string(),
            ],
            cwd: None,
            timeout_ms: None,
        }]);

        let result = executor.call("echo.text", &[]).unwrap().unwrap();

        assert_eq!(result, Value::String("ok".to_string()));
    }

    #[cfg(unix)]
    #[test]
    fn process_connector_receives_egress_context() {
        let executor = ProcessConnectorExecutor::new(vec![ProcessConnectorConfig {
            method: "mailer.send".to_string(),
            command: "/usr/bin/env".to_string(),
            args: vec![
                "python3".to_string(),
                "-c".to_string(),
                "import json,sys; payload=json.load(sys.stdin); print(json.dumps(payload['egress']))"
                    .to_string(),
            ],
            cwd: None,
            timeout_ms: None,
        }]);
        let context = ConnectorCallContext {
            connector: "mailer".to_string(),
            method_name: "send".to_string(),
            method: "mailer.send".to_string(),
            capability: "connector:mailer.send".to_string(),
            actor: "user@example.com".to_string(),
            tenant: "tenant_a".to_string(),
            correlation_id: "corr_1".to_string(),
            request_id: "req_1".to_string(),
            policy_decision: "compile_time_checked".to_string(),
            arg_labels: vec![ConnectorArgLabel {
                index: 0,
                name: "email".to_string(),
                ty: "Email".to_string(),
                source: Some("UserInput".to_string()),
                privacy: Some("private".to_string()),
                trust: Some("verified".to_string()),
            }],
        };

        let result = executor
            .call_with_context(
                &context,
                &[Value::String("customer@example.com".to_string())],
            )
            .unwrap()
            .unwrap();

        let Value::Struct(_, fields) = result else {
            panic!("expected egress context object");
        };
        assert_eq!(
            fields.get("capability"),
            Some(&Value::String("connector:mailer.send".to_string()))
        );
        assert_eq!(
            fields.get("tenant"),
            Some(&Value::String("tenant_a".to_string()))
        );
        let Some(Value::List(labels)) = fields.get("arg_labels") else {
            panic!("expected arg labels list");
        };
        let Some(Value::Struct(_, label_fields)) = labels.first() else {
            panic!("expected first arg label");
        };
        assert_eq!(
            label_fields.get("privacy"),
            Some(&Value::String("private".to_string()))
        );
    }

    #[cfg(unix)]
    #[test]
    fn process_connector_rejects_commands_that_exceed_timeout() {
        let executor = ProcessConnectorExecutor::new(vec![ProcessConnectorConfig {
            method: "slow.call".to_string(),
            command: "/bin/sh".to_string(),
            args: vec!["-c".to_string(), "sleep 1; printf '\"late\"'".to_string()],
            cwd: None,
            timeout_ms: Some(10),
        }]);

        let err = executor.call("slow.call", &[]).unwrap().unwrap_err();

        assert_eq!(err.code, "timeout");
        assert!(err.retryable);
        assert!(err.message.contains("exceeded timeout of 10ms"));
    }
}
