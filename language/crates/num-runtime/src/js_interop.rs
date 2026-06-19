use crate::connectors::{ConnectorCallContext, ConnectorError, ConnectorExecutor};
use crate::interpreter::Value;
use crate::process_connectors::{value_from_json, value_to_json};
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const NODE_BRIDGE: &str = r#"
const modulePath = process.argv[1];
const exportName = process.argv[2] || "default";

function fail(code, message) {
  const error = new Error(message);
  error.code = code;
  throw error;
}

async function main() {
  let inputText = "";
  process.stdin.setEncoding("utf8");
  for await (const chunk of process.stdin) {
    inputText += chunk;
  }
  const input = inputText.trim() ? JSON.parse(inputText) : {};
  const loaded = require(modulePath);
  const callable = exportName === "default"
    ? (typeof loaded === "function" ? loaded : loaded.default)
    : loaded[exportName];
  if (typeof callable !== "function") {
    fail("js_export_missing", `JavaScript module export '${exportName}' is not callable`);
  }
  const result = await callable({
    method: input.method,
    args: input.args || [],
    context: input.context || null,
  });
  process.stdout.write(JSON.stringify({
    ok: true,
    value: result === undefined ? null : result,
  }));
}

main().catch((error) => {
  const code = typeof error.code === "string" ? error.code : "js_exception";
  const rawMessage = error && typeof error.message === "string" ? error.message : String(error);
  const message = rawMessage.split(/\r?\n/)[0];
  process.stdout.write(JSON.stringify({
    ok: false,
    error: { code, message },
  }));
});
"#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaScriptModuleConfig {
    pub method: String,
    pub module: PathBuf,
    pub export: String,
    pub cwd: Option<PathBuf>,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct JavaScriptModuleExecutor {
    configs: HashMap<String, JavaScriptModuleConfig>,
}

impl JavaScriptModuleExecutor {
    pub fn new(configs: Vec<JavaScriptModuleConfig>) -> Self {
        Self {
            configs: configs
                .into_iter()
                .map(|config| (config.method.clone(), config))
                .collect(),
        }
    }

    fn run(
        config: &JavaScriptModuleConfig,
        method: &str,
        args: &[Value],
        context: Option<&ConnectorCallContext>,
    ) -> Result<Value, ConnectorError> {
        let mut command = Command::new("node");
        command.arg("-e");
        command.arg(NODE_BRIDGE);
        command.arg(&config.module);
        command.arg(&config.export);
        if let Some(cwd) = &config.cwd {
            command.current_dir(cwd);
        }
        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        let mut child = command.spawn().map_err(|err| {
            ConnectorError::new(
                "js_start_failed",
                format!("failed to start JavaScript callable `{method}` with node: {err}"),
                false,
            )
        })?;

        let input = json!({
            "method": method,
            "args": args.iter().map(value_to_json).collect::<Vec<_>>(),
            "context": context.map(ConnectorCallContext::to_json),
        });
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(input.to_string().as_bytes())
                .map_err(|err| {
                    ConnectorError::new(
                        "js_stdin_write_failed",
                        format!("failed to write JavaScript callable `{method}` stdin: {err}"),
                        true,
                    )
                })?;
        }

        let output = wait_with_optional_timeout(child, method, config.timeout_ms)?;
        if !output.status.success() {
            return Err(ConnectorError::new(
                "js_nonzero_exit",
                format!(
                    "JavaScript callable `{method}` exited with {}",
                    output.status
                ),
                true,
            ));
        }

        let stdout = String::from_utf8(output.stdout).map_err(|err| {
            ConnectorError::new(
                "js_invalid_stdout",
                format!("JavaScript callable `{method}` wrote non-UTF8 stdout: {err}"),
                false,
            )
        })?;
        let stdout = stdout.trim();
        if stdout.is_empty() {
            return Err(ConnectorError::new(
                "js_invalid_json",
                format!("JavaScript callable `{method}` returned empty stdout"),
                false,
            ));
        }

        let payload: JsonValue = serde_json::from_str(stdout).map_err(|err| {
            ConnectorError::new(
                "js_invalid_json",
                format!("JavaScript callable `{method}` returned invalid JSON: {err}"),
                false,
            )
        })?;
        decode_bridge_payload(method, &payload)
    }
}

fn decode_bridge_payload(method: &str, payload: &JsonValue) -> Result<Value, ConnectorError> {
    let Some(object) = payload.as_object() else {
        return Err(ConnectorError::new(
            "js_invalid_json",
            format!("JavaScript callable `{method}` must return a bridge object"),
            false,
        ));
    };
    if object.get("ok").and_then(JsonValue::as_bool) == Some(true) {
        return value_from_json(object.get("value").unwrap_or(&JsonValue::Null))
            .map_err(|message| ConnectorError::new("js_invalid_json", message, false));
    }

    let error = object.get("error").and_then(JsonValue::as_object);
    let code = error
        .and_then(|error| error.get("code"))
        .and_then(JsonValue::as_str)
        .unwrap_or("js_exception");
    let message = error
        .and_then(|error| error.get("message"))
        .and_then(JsonValue::as_str)
        .unwrap_or("JavaScript callable failed");
    Err(ConnectorError::new(code, message, false))
}

fn wait_with_optional_timeout(
    mut child: std::process::Child,
    method: &str,
    timeout_ms: Option<u64>,
) -> Result<std::process::Output, ConnectorError> {
    let Some(timeout_ms) = timeout_ms else {
        return child.wait_with_output().map_err(|err| {
            ConnectorError::new(
                "js_wait_failed",
                format!("failed to wait for JavaScript callable `{method}`: {err}"),
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
                    "js_poll_failed",
                    format!("failed to poll JavaScript callable `{method}`: {err}"),
                    true,
                )
            })?
            .is_some()
        {
            return child.wait_with_output().map_err(|err| {
                ConnectorError::new(
                    "js_wait_failed",
                    format!("failed to collect JavaScript callable `{method}` output: {err}"),
                    true,
                )
            });
        }

        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ConnectorError::new(
                "js_timeout",
                format!("JavaScript callable `{method}` exceeded timeout of {timeout_ms}ms"),
                true,
            ));
        }

        let remaining = timeout.saturating_sub(started.elapsed());
        thread::sleep(remaining.min(Duration::from_millis(10)));
    }
}

impl ConnectorExecutor for JavaScriptModuleExecutor {
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

#[cfg(test)]
mod tests {
    use super::{JavaScriptModuleConfig, JavaScriptModuleExecutor};
    use crate::connectors::{ConnectorArgLabel, ConnectorCallContext, ConnectorExecutor};
    use crate::interpreter::Value;
    use std::fs;

    #[test]
    fn javascript_module_receives_context_and_returns_json_value() {
        let root = std::env::temp_dir().join(format!(
            "num-js-interop-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let module = root.join("callable.cjs");
        fs::write(
            &module,
            r#"
exports.enrich = async ({ args, context }) => ({
  "$type": "Greeting",
  "message": `${args[0]}:${context.actor}:${context.tenant}:${context.request_id}`
});
"#,
        )
        .unwrap();

        let executor = JavaScriptModuleExecutor::new(vec![JavaScriptModuleConfig {
            method: "js.enrich".to_string(),
            module,
            export: "enrich".to_string(),
            cwd: Some(root.clone()),
            timeout_ms: Some(1000),
        }]);
        let context = ConnectorCallContext {
            connector: "js".to_string(),
            method_name: "enrich".to_string(),
            method: "js.enrich".to_string(),
            capability: "connector:js.enrich".to_string(),
            actor: "actor-1".to_string(),
            tenant: "tenant-1".to_string(),
            correlation_id: "corr-1".to_string(),
            request_id: "req-1".to_string(),
            policy_decision: "not_evaluated".to_string(),
            arg_labels: vec![ConnectorArgLabel {
                index: 0,
                name: "name".to_string(),
                ty: "Text".to_string(),
                source: Some("UserInput".to_string()),
                privacy: Some("private".to_string()),
                trust: None,
            }],
        };

        let result = executor
            .call_with_context(&context, &[Value::String("hello".to_string())])
            .unwrap()
            .unwrap();

        let Value::Struct(name, fields) = result else {
            panic!("expected struct result");
        };
        assert_eq!(name, "Greeting");
        assert_eq!(
            fields.get("message"),
            Some(&Value::String("hello:actor-1:tenant-1:req-1".to_string()))
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn javascript_module_reports_structured_errors_without_stack_traces() {
        let root = std::env::temp_dir().join(format!(
            "num-js-interop-error-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let module = root.join("callable.cjs");
        fs::write(
            &module,
            r#"
exports.fail = () => {
  throw new Error("module rejected the value");
};
"#,
        )
        .unwrap();

        let executor = JavaScriptModuleExecutor::new(vec![JavaScriptModuleConfig {
            method: "js.fail".to_string(),
            module,
            export: "fail".to_string(),
            cwd: Some(root.clone()),
            timeout_ms: Some(1000),
        }]);
        let error = executor.call("js.fail", &[]).unwrap().unwrap_err();

        assert_eq!(error.code, "js_exception");
        assert_eq!(error.message, "module rejected the value");
        assert!(!error.message.contains("at "));

        let _ = fs::remove_dir_all(root);
    }
}
