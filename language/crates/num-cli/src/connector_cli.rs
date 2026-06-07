use crate::{package, project};
use num_compiler::check_program;
use num_runtime::{
    connectors::ConnectorExecutor,
    process_connectors::{
        value_from_json, value_to_json, ProcessConnectorConfig, ProcessConnectorExecutor,
    },
};
use serde_json::{json, Value as JsonValue};
use std::path::PathBuf;

pub fn run(args: impl Iterator<Item = String>) -> Result<(), String> {
    let mut args = args;
    match args.next().as_deref() {
        Some("probe") => probe(args),
        Some(other) => Err(format!(
            "unknown connector command `{other}`\n\nSupported connector commands:\n  probe"
        )),
        None => Err("usage: num connector <probe> ...".to_string()),
    }
}

#[derive(Debug, Clone, PartialEq)]
struct ProbeOptions {
    path: PathBuf,
    method: String,
    args: Vec<JsonValue>,
    format_json: bool,
}

fn probe(args: impl Iterator<Item = String>) -> Result<(), String> {
    let options = parse_probe_args(args)?;
    validate_project_for_probe(&options.path)?;
    let manifest = package::PackageManifest::discover(&options.path)?
        .ok_or_else(|| format!("no num.toml found for {}", options.path.display()))?;
    let Some(binding) = manifest
        .connectors
        .iter()
        .find(|connector| connector.method == options.method)
    else {
        return Err(format!(
            "connector method `{}` has no process binding in {}",
            options.method,
            manifest.path.display()
        ));
    };

    let runtime_args = options
        .args
        .iter()
        .map(value_from_json)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("invalid connector argument JSON: {err}"))?;
    let executor = ProcessConnectorExecutor::new(vec![ProcessConnectorConfig {
        method: binding.method.clone(),
        command: binding.command.clone(),
        args: binding.args.clone(),
        cwd: Some(
            binding
                .cwd
                .as_ref()
                .map(|cwd| manifest.root.join(cwd))
                .unwrap_or_else(|| manifest.root.clone()),
        ),
        timeout_ms: binding.timeout_ms,
    }]);

    let result = executor
        .call(&options.method, &runtime_args)
        .ok_or_else(|| format!("connector method `{}` is not configured", options.method))?;

    match result {
        Ok(value) => {
            let report = json!({
                "status": "ok",
                "method": options.method,
                "result": value_to_json(&value),
            });
            if options.format_json {
                print_json(&report)?;
            } else {
                println!(
                    "connector probe ok: {}",
                    report["method"].as_str().unwrap_or("")
                );
                println!("result: {}", report["result"]);
            }
            Ok(())
        }
        Err(error) => {
            let report = json!({
                "status": "error",
                "method": options.method,
                "connector": {
                    "code": error.code,
                    "message": error.message,
                    "retryable": error.retryable,
                }
            });
            if options.format_json {
                print_json(&report)?;
            }
            Err(format!(
                "connector probe failed: {}: {}",
                report["connector"]["code"].as_str().unwrap_or("error"),
                report["connector"]["message"].as_str().unwrap_or("")
            ))
        }
    }
}

fn validate_project_for_probe(path: &PathBuf) -> Result<(), String> {
    let input = project::load_program_input(path)?;
    let program = check_program(input.files);
    if program
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == num_compiler::diagnostic::Severity::Error)
    {
        return Err("program has compile errors".to_string());
    }
    Ok(())
}

fn parse_probe_args(args: impl Iterator<Item = String>) -> Result<ProbeOptions, String> {
    let mut path = None;
    let mut method = None;
    let mut connector_args = Vec::new();
    let mut format_json = false;
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--arg" => {
                let raw = args
                    .next()
                    .ok_or_else(|| "usage: --arg <json-value>".to_string())?;
                let value = serde_json::from_str(&raw)
                    .map_err(|err| format!("invalid --arg JSON `{raw}`: {err}"))?;
                connector_args.push(value);
            }
            "--json" => format_json = true,
            _ if path.is_none() => path = Some(PathBuf::from(arg)),
            _ if method.is_none() => method = Some(arg),
            _ => return Err(format!("unexpected connector probe argument '{arg}'")),
        }
    }

    Ok(ProbeOptions {
        path: path.ok_or_else(|| {
            "usage: num connector probe <project-dir|file.num> <connector.method> [--arg <json>]... [--json]".to_string()
        })?,
        method: method.ok_or_else(|| {
            "usage: num connector probe <project-dir|file.num> <connector.method> [--arg <json>]... [--json]".to_string()
        })?,
        args: connector_args,
        format_json,
    })
}

fn print_json(value: &JsonValue) -> Result<(), String> {
    let rendered = serde_json::to_string_pretty(value)
        .map_err(|err| format!("failed to render connector probe JSON: {err}"))?;
    println!("{rendered}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{parse_probe_args, run};
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn probe_args_parse_json_values() {
        let options = parse_probe_args(
            [
                "examples/refund_workflow".to_string(),
                "payments.find".to_string(),
                "--arg".to_string(),
                "\"pay_1\"".to_string(),
                "--arg".to_string(),
                "{\"limit\": 1}".to_string(),
                "--json".to_string(),
            ]
            .into_iter(),
        )
        .unwrap();

        assert_eq!(options.path, PathBuf::from("examples/refund_workflow"));
        assert_eq!(options.method, "payments.find");
        assert_eq!(options.args, vec![json!("pay_1"), json!({ "limit": 1 })]);
        assert!(options.format_json);
    }

    #[test]
    fn probe_invokes_manifest_process_binding() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("num_connector_probe_{stamp}"));
        let src = root.join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(
            root.join("num.toml"),
            r#"[language]
version = "0.1.1"
compatibility = "minor"
manifest_schema = 1

[project]
name = "connector-probe"
version = "0.1.0"
source = "src"
entry = "src/main.num"

[connectors]
"echo.text" = { command = "/bin/echo", args = "true" }
"#,
        )
        .unwrap();
        fs::write(
            src.join("main.num"),
            r#"module smoke.main

connector echo {
    text() -> Bool
}

workflow main() {
    let ok: Bool = echo.text()
}
"#,
        )
        .unwrap();

        run([
            "probe".to_string(),
            root.display().to_string(),
            "echo.text".to_string(),
            "--json".to_string(),
        ]
        .into_iter())
        .unwrap();
    }
}
