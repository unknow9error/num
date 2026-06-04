use crate::{connector_sdk, project};
use num_compiler::compile_program;
use std::fs;
use std::path::PathBuf;

pub fn run(args: impl Iterator<Item = String>) -> Result<(), String> {
    let (path, options) = parse_args(args)?;
    let input = project::load_program_input(&path)?;
    let entry_source_name = input.entry_source_name.clone();
    let policy_mode = input.policy_mode.clone();
    let program = compile_program(input.files, entry_source_name.as_deref());
    let mut diagnostics = program.diagnostics.clone();
    let fail_on_warnings =
        crate::apply_policy_mode(policy_mode.as_deref(), &program.modules, &mut diagnostics)?;
    crate::print_diagnostics(&diagnostics);
    if crate::has_failing_diagnostics(&diagnostics, fail_on_warnings) {
        return Err("program has compile errors".to_string());
    }

    let sdk = match options.language {
        ConnectorSdkLanguageArg::TypeScript => {
            connector_sdk::render_typescript_sdk(&program.module)
        }
    };

    if let Some(out_path) = &options.out_path {
        if let Some(parent) = out_path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)
                    .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
            }
        }
        fs::write(out_path, &sdk.contents)
            .map_err(|err| format!("failed to write {}: {err}", out_path.display()))?;
    }

    if options.format_json {
        let mut payload = sdk.to_json();
        if let Some(out_path) = &options.out_path {
            payload["out_path"] = serde_json::json!(out_path.display().to_string());
        }
        let json = serde_json::to_string_pretty(&payload)
            .map_err(|err| format!("failed to render connector SDK JSON: {err}"))?;
        println!("{json}");
    } else if options.out_path.is_none() {
        print!("{}", sdk.contents);
    } else {
        println!(
            "wrote connector SDK {} (connectors={}, methods={}, types={})",
            options.out_path.unwrap().display(),
            sdk.connector_count,
            sdk.method_count,
            sdk.type_count
        );
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnectorSdkLanguageArg {
    TypeScript,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConnectorSdkOptions {
    language: ConnectorSdkLanguageArg,
    out_path: Option<PathBuf>,
    format_json: bool,
}

fn parse_args(
    args: impl Iterator<Item = String>,
) -> Result<(PathBuf, ConnectorSdkOptions), String> {
    let mut path = None;
    let mut language = ConnectorSdkLanguageArg::TypeScript;
    let mut out_path = None;
    let mut format_json = false;
    let mut args = args.peekable();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--language" | "--lang" => {
                let raw = args
                    .next()
                    .ok_or_else(|| "usage: --language typescript".to_string())?;
                language = match raw.as_str() {
                    "typescript" | "ts" => ConnectorSdkLanguageArg::TypeScript,
                    other => {
                        return Err(format!(
                            "unsupported connector SDK language `{other}`; supported: typescript"
                        ))
                    }
                };
            }
            "--out" | "-o" => {
                let raw = args
                    .next()
                    .ok_or_else(|| "usage: --out <connectors.d.ts>".to_string())?;
                out_path = Some(PathBuf::from(raw));
            }
            "--json" => format_json = true,
            _ if path.is_none() => path = Some(PathBuf::from(arg)),
            _ => return Err(format!("unexpected connector-sdk argument '{arg}'")),
        }
    }

    Ok((
        path.unwrap_or_else(|| PathBuf::from(".")),
        ConnectorSdkOptions {
            language,
            out_path,
            format_json,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::{parse_args, ConnectorSdkLanguageArg};
    use std::path::PathBuf;

    #[test]
    fn parses_connector_sdk_args() {
        let (path, options) = parse_args(
            [
                "examples/refund_workflow".to_string(),
                "--language".to_string(),
                "typescript".to_string(),
                "--out".to_string(),
                "generated/connectors.d.ts".to_string(),
                "--json".to_string(),
            ]
            .into_iter(),
        )
        .unwrap();

        assert_eq!(path, PathBuf::from("examples/refund_workflow"));
        assert_eq!(options.language, ConnectorSdkLanguageArg::TypeScript);
        assert_eq!(
            options.out_path,
            Some(PathBuf::from("generated/connectors.d.ts"))
        );
        assert!(options.format_json);
    }
}
