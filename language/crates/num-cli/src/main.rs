mod compatibility;
mod demo;
mod deploy;
mod openapi;
mod package;
mod project;
mod registry;
mod sql_schema;

use num_compiler::{
    check_program, compile, compile_program,
    diagnostic::{Diagnostic, Severity},
    formatter, lint, Compilation, ProgramModule,
};
use num_runtime::{
    audit_report,
    connectors::{ChainedConnectorExecutor, ConnectorExecutor, DemoConnectorExecutor},
    cost_report,
    debugger::{BreakpointSpec, DebugReport},
    http,
    process_connectors::{ProcessConnectorConfig, ProcessConnectorExecutor},
    service::ServiceRuntime,
    storage::FileStateStore,
    workflow_report,
};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        print_help();
        return Ok(());
    };

    match command.as_str() {
        "check" => {
            let path = required_path(args.next(), "check")?;
            let input = project::load_program_input(&path)?;
            let file_count = input.files.len();
            let policy_mode = input.policy_mode.clone();
            let program = check_program(input.files);
            let mut diagnostics = program.diagnostics;
            let fail_on_warnings =
                apply_policy_mode(policy_mode.as_deref(), &program.modules, &mut diagnostics)?;
            print_diagnostics(&diagnostics);
            if has_failing_diagnostics(&diagnostics, fail_on_warnings) {
                Err("num check failed".to_string())
            } else {
                println!(
                    "num check passed: {} ({} .num files)",
                    path.display(),
                    file_count
                );
                Ok(())
            }
        }
        "lint" => {
            let path = required_path(args.next(), "lint")?;
            let input = project::load_program_input(&path)?;
            let policy_mode = input.policy_mode.clone();
            let program = check_program(input.files);
            let mut diagnostics = program.diagnostics;
            for module in &program.modules {
                diagnostics.extend(lint::lint(&module.module));
            }
            validate_policy_mode(policy_mode.as_deref())?;
            print_diagnostics(&diagnostics);
            if diagnostics.is_empty() {
                println!("num lint passed: {}", path.display());
                Ok(())
            } else {
                Err("num lint failed".to_string())
            }
        }
        "fmt" => {
            let path = required_path(args.next(), "fmt")?;
            let source = fs::read_to_string(&path)
                .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
            let compilation = compile(path.display().to_string(), &source);
            print!("{}", formatter::format_module(&compilation.module));
            Ok(())
        }
        "ir" => {
            let path = required_path(args.next(), "ir")?;
            let source = fs::read_to_string(&path)
                .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
            let compilation = compile(path.display().to_string(), &source);
            print_diagnostics(&compilation.diagnostics);
            println!("{:#?}", compilation.ir);
            Ok(())
        }
        "run" => {
            let path = required_path(args.next(), "run")?;
            let compilation = compile_checked(&path)?;
            let connectors = connector_executor_for_path(&path)?;

            let mut runtime = num_runtime::interpreter::Runtime::with_connectors(
                &compilation.module,
                demo::default_permissions(),
                Box::new(connectors.clone()),
            );

            let workflow_name = demo::first_workflow_name(&compilation.module)
                .ok_or_else(|| "No workflow declared in the module".to_string())?;
            let workflow_args = demo::workflow_args(&workflow_name);

            runtime.run_workflow(&workflow_name, workflow_args)?;
            Ok(())
        }
        "test" => {
            let path = required_path(args.next(), "test")?;
            let compilation = compile_checked(&path)?;
            let connectors = connector_executor_for_path(&path)?;
            let tests = compilation
                .module
                .declarations
                .iter()
                .filter_map(|decl| match decl {
                    num_compiler::ast::Declaration::Test(test) => Some(test.name.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>();

            if tests.is_empty() {
                println!("num test passed: {} (0 tests)", path.display());
                return Ok(());
            }

            let mut failed = Vec::new();
            for test_name in &tests {
                let mut runtime = num_runtime::interpreter::Runtime::with_connectors(
                    &compilation.module,
                    demo::default_permissions(),
                    Box::new(connectors.clone()),
                );
                match runtime.run_test(test_name) {
                    Ok(()) => println!("test {test_name} ... ok"),
                    Err(err) => {
                        println!("test {test_name} ... FAILED");
                        failed.push(err);
                    }
                }
            }

            if failed.is_empty() {
                println!(
                    "num test passed: {} ({} tests)",
                    path.display(),
                    tests.len()
                );
                Ok(())
            } else {
                let failed_count = failed.len();
                for failure in failed {
                    eprintln!("{failure}");
                }
                Err(format!(
                    "num test failed: {} ({} failed of {} tests)",
                    path.display(),
                    failed_count,
                    tests.len()
                ))
            }
        }
        "trace" => {
            let path = required_path(args.next(), "trace")?;
            let compilation = compile_checked(&path)?;
            let connectors = connector_executor_for_path(&path)?;
            let mut runtime = num_runtime::interpreter::Runtime::with_connectors(
                &compilation.module,
                demo::default_permissions(),
                Box::new(connectors.clone()),
            );
            let workflow_name = demo::first_workflow_name(&compilation.module)
                .ok_or_else(|| "No workflow declared in the module".to_string())?;
            let workflow_args = demo::workflow_args(&workflow_name);

            runtime.run_workflow(&workflow_name, workflow_args)?;
            let events = runtime
                .trace_events()
                .iter()
                .map(|event| event.to_json())
                .collect::<Vec<_>>();
            let json = serde_json::to_string_pretty(&events)
                .map_err(|err| format!("failed to render trace JSON: {err}"))?;
            println!("{json}");
            Ok(())
        }
        "debug" => {
            let path = required_path(args.next(), "debug")?;
            let options = parse_debug_options(args)?;
            let compilation = compile_checked(&path)?;
            let connectors = connector_executor_for_path(&path)?;
            let mut runtime = num_runtime::interpreter::Runtime::with_connectors(
                &compilation.module,
                demo::default_permissions(),
                Box::new(connectors.clone()),
            );
            let workflow_name = options
                .workflow_name
                .or_else(|| demo::first_workflow_name(&compilation.module))
                .ok_or_else(|| "No workflow declared in the module".to_string())?;
            let workflow_args = demo::workflow_args(&workflow_name);

            let result = runtime.run_workflow(&workflow_name, workflow_args);
            let report = DebugReport::from_trace(
                workflow_name,
                result.clone(),
                options.breakpoints,
                runtime.trace_events(),
            );
            if options.format_json {
                let json = serde_json::to_string_pretty(&report.to_json())
                    .map_err(|err| format!("failed to render debug report JSON: {err}"))?;
                println!("{json}");
            } else {
                print!("{}", report.render_text());
            }
            result
        }
        "deploy" => {
            let path = args
                .next()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            let options = parse_deploy_options(args)?;
            let manifest = package::PackageManifest::discover(&path)?
                .ok_or_else(|| format!("no num.toml found for {}", path.display()))?;
            let input = project::load_program_input(&path)?;
            let module_count = input.files.len();
            let policy_mode = input.policy_mode.clone();
            let program = compile_program(input.files, input.entry_source_name.as_deref());
            let mut diagnostics = program.diagnostics;
            let fail_on_warnings =
                apply_policy_mode(policy_mode.as_deref(), &program.modules, &mut diagnostics)?;
            print_diagnostics(&diagnostics);
            if has_failing_diagnostics(&diagnostics, fail_on_warnings) {
                return Err("program has compile errors".to_string());
            }

            let plan = deploy::build_deployment_plan(&manifest, &program.module, module_count);
            let json = serde_json::to_string_pretty(&plan.to_json())
                .map_err(|err| format!("failed to render deployment plan JSON: {err}"))?;
            if let Some(out_path) = &options.out_path {
                if let Some(parent) = out_path.parent() {
                    if !parent.as_os_str().is_empty() {
                        fs::create_dir_all(parent).map_err(|err| {
                            format!("failed to create {}: {err}", parent.display())
                        })?;
                    }
                }
                fs::write(&out_path, format!("{json}\n"))
                    .map_err(|err| format!("failed to write {}: {err}", out_path.display()))?;
                println!("wrote deployment plan {}", out_path.display());
            }
            if options.format_json {
                println!("{json}");
            } else if options.out_path.is_none() {
                print!("{}", plan.render_text());
            }
            Ok(())
        }
        "compat" => {
            let path = args
                .next()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            let format_json = match args.next().as_deref() {
                Some("--json") => true,
                Some(other) => return Err(format!("unexpected compat argument '{other}'")),
                None => false,
            };
            if let Some(other) = args.next() {
                return Err(format!("unexpected compat argument '{other}'"));
            }

            let reports = project::load_package_manifests(&path)?
                .iter()
                .map(compatibility::CompatibilityReport::from_manifest)
                .collect::<Vec<_>>();
            if format_json {
                let json = serde_json::to_string_pretty(
                    &reports
                        .iter()
                        .map(compatibility::CompatibilityReport::to_json)
                        .collect::<Vec<_>>(),
                )
                .map_err(|err| format!("failed to render compatibility JSON: {err}"))?;
                println!("{json}");
            } else {
                for report in &reports {
                    print!("{}", report.render_text());
                }
            }
            Ok(())
        }
        "cost-report" => {
            let path = required_path(args.next(), "cost-report")?;
            let format_json = match args.next().as_deref() {
                Some("--json") => true,
                Some(other) => return Err(format!("unexpected cost-report argument '{other}'")),
                None => false,
            };
            if let Some(other) = args.next() {
                return Err(format!("unexpected cost-report argument '{other}'"));
            }
            let compilation = compile_checked(&path)?;
            let connectors = connector_executor_for_path(&path)?;
            let mut runtime = num_runtime::interpreter::Runtime::with_connectors(
                &compilation.module,
                demo::default_permissions(),
                Box::new(connectors.clone()),
            );
            let workflow_name = demo::first_workflow_name(&compilation.module)
                .ok_or_else(|| "No workflow declared in the module".to_string())?;
            let workflow_args = demo::workflow_args(&workflow_name);

            runtime.run_workflow(&workflow_name, workflow_args)?;
            let report = cost_report::summarize_cost_entries(runtime.cost_entries());
            if format_json {
                let json = serde_json::to_string_pretty(&report.to_json())
                    .map_err(|err| format!("failed to render cost report JSON: {err}"))?;
                println!("{json}");
            } else {
                print!("{}", report.render_text());
            }
            Ok(())
        }
        "audit-report" => {
            let path = args
                .next()
                .map(PathBuf::from)
                .ok_or_else(|| "usage: num audit-report <events.jsonl> [--json]".to_string())?;
            let format_json = match args.next().as_deref() {
                Some("--json") => true,
                Some(other) => return Err(format!("unexpected audit-report argument '{other}'")),
                None => false,
            };
            if let Some(other) = args.next() {
                return Err(format!("unexpected audit-report argument '{other}'"));
            }
            let source = fs::read_to_string(&path)
                .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
            let report = audit_report::summarize_audit_jsonl(&source)
                .map_err(|err| format!("failed to summarize {}: {err:?}", path.display()))?;
            if format_json {
                let json = serde_json::to_string_pretty(&report.to_json())
                    .map_err(|err| format!("failed to render audit report JSON: {err}"))?;
                println!("{json}");
            } else {
                print!("{}", report.render_text());
            }
            Ok(())
        }
        "workflow-report" => {
            let path = args
                .next()
                .map(PathBuf::from)
                .ok_or_else(|| "usage: num workflow-report <state-root> [--json]".to_string())?;
            let format_json = match args.next().as_deref() {
                Some("--json") => true,
                Some(other) => {
                    return Err(format!("unexpected workflow-report argument '{other}'"))
                }
                None => false,
            };
            if let Some(other) = args.next() {
                return Err(format!("unexpected workflow-report argument '{other}'"));
            }
            let store = FileStateStore::new(path);
            let report = workflow_report::summarize_workflow_store(&store)
                .map_err(|err| format!("failed to summarize workflows: {err:?}"))?;
            if format_json {
                let json = serde_json::to_string_pretty(&report.to_json())
                    .map_err(|err| format!("failed to render workflow report JSON: {err}"))?;
                println!("{json}");
            } else {
                print!("{}", report.render_text());
            }
            Ok(())
        }
        "route" => {
            let path = required_path(args.next(), "route")?;
            let method = args.next().ok_or_else(|| {
                "usage: num route <file.num|dir> <METHOD> <PATH> [service]".to_string()
            })?;
            let route_path = args.next().ok_or_else(|| {
                "usage: num route <file.num|dir> <METHOD> <PATH> [service]".to_string()
            })?;
            let compilation = compile_checked(&path)?;

            let service_name = args
                .next()
                .or_else(|| demo::first_service_name(&compilation.module))
                .ok_or_else(|| "No service declared in the module".to_string())?;
            let input = demo::route_input(&compilation.module, &service_name, &method, &route_path);
            let connectors = connector_executor_for_path(&path)?;

            let mut runtime = num_runtime::interpreter::Runtime::with_connectors(
                &compilation.module,
                demo::default_permissions(),
                Box::new(connectors.clone()),
            );
            runtime.run_service_route(&service_name, &method, &route_path, input)?;
            Ok(())
        }
        "serve-once" => {
            let path = required_path(args.next(), "serve-once")?;
            let (addr, service_name) = serve_target(args, "serve-once")?;
            let compilation = compile_checked(&path)?;
            let service_name = service_name
                .or_else(|| ServiceRuntime::first_service_name(&compilation.module))
                .ok_or_else(|| "No service declared in the module".to_string())?;
            let connectors = connector_executor_for_path(&path)?;
            let runtime = ServiceRuntime::with_connectors(
                &compilation.module,
                service_name.clone(),
                demo::default_permissions(),
                connectors,
            );

            println!("Serving one request for service {service_name} on http://{addr}");
            http::serve_once(&addr, |request| {
                let empty_body_input = demo::route_input(
                    &compilation.module,
                    &service_name,
                    &request.method,
                    &request.path,
                );
                runtime.handle_http_request_with_empty_body_input(&request, empty_body_input)
            })
        }
        "serve" => {
            let path = required_path(args.next(), "serve")?;
            let options = parse_serve_options(args)?;
            let compilation = compile_checked(&path)?;
            let service_name = options
                .service_name
                .or_else(|| ServiceRuntime::first_service_name(&compilation.module))
                .ok_or_else(|| "No service declared in the module".to_string())?;
            let connectors = connector_executor_for_path(&path)?;
            let runtime = ServiceRuntime::with_connectors(
                &compilation.module,
                service_name.clone(),
                demo::default_permissions(),
                connectors,
            );

            println!("Serving service {service_name} on http://{}", options.addr);
            http::serve(&options.addr, options.max_requests, |request| {
                runtime.handle_http_request(&request)
            })
        }
        "new" => {
            let name = args
                .next()
                .ok_or_else(|| "usage: num new <project-name>".to_string())?;
            project::create_project(Path::new(&name))
        }
        "lock" => {
            let path = args
                .next()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            let lock_path = package::write_lockfile(&path)?;
            println!("wrote {}", lock_path.display());
            Ok(())
        }
        "import" => match args.next().as_deref() {
            Some("openapi") => {
                let path = args.next().map(PathBuf::from).ok_or_else(|| {
                    "usage: num import openapi <openapi.json> [module.name]".to_string()
                })?;
                let module_name = args.next();
                print!(
                    "{}",
                    openapi::import_openapi(&path, module_name.as_deref())?
                );
                Ok(())
            }
            Some("sql") => {
                let path = args.next().map(PathBuf::from).ok_or_else(|| {
                    "usage: num import sql <schema.sql> [module.name]".to_string()
                })?;
                let module_name = args.next();
                print!(
                    "{}",
                    sql_schema::import_sql_schema(&path, module_name.as_deref())?
                );
                Ok(())
            }
            Some(other) => Err(format!(
                "unknown import kind `{other}`\n\nSupported imports:\n  openapi\n  sql"
            )),
            None => Err("usage: num import <openapi|sql> <schema-file> [module.name]".to_string()),
        },
        "completions" => {
            let shell = args
                .next()
                .ok_or_else(|| "usage: num completions <zsh>".to_string())?;
            print_completions(&shell)
        }
        "lsp" => num_lsp::run_server(),
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        other => Err(format!("unknown command `{other}`\n\n{}", help_text())),
    }
}

fn required_path(value: Option<String>, command: &str) -> Result<PathBuf, String> {
    value
        .map(PathBuf::from)
        .ok_or_else(|| format!("usage: num {command} <file.num>"))
}

fn compile_checked(path: &Path) -> Result<Compilation, String> {
    let input = project::load_program_input(path)?;
    let policy_mode = input.policy_mode.clone();
    let entry_source_name = input.entry_source_name.clone();
    let program = compile_program(input.files, entry_source_name.as_deref());
    let mut diagnostics = program.diagnostics;
    let fail_on_warnings =
        apply_policy_mode(policy_mode.as_deref(), &program.modules, &mut diagnostics)?;
    let compilation = Compilation {
        module: program.module,
        ir: program.ir,
        diagnostics,
    };
    print_diagnostics(&compilation.diagnostics);
    if has_failing_diagnostics(&compilation.diagnostics, fail_on_warnings) {
        Err("program has compile errors".to_string())
    } else {
        Ok(compilation)
    }
}

fn connector_executor_for_path(path: &Path) -> Result<Arc<dyn ConnectorExecutor>, String> {
    let Some(manifest) = package::PackageManifest::discover(path)? else {
        return Ok(Arc::new(DemoConnectorExecutor));
    };
    if manifest.connectors.is_empty() {
        return Ok(Arc::new(DemoConnectorExecutor));
    }

    let configs = manifest
        .connectors
        .iter()
        .map(|connector| ProcessConnectorConfig {
            method: connector.method.clone(),
            command: connector.command.clone(),
            args: connector.args.clone(),
            cwd: Some(
                connector
                    .cwd
                    .as_ref()
                    .map(|cwd| manifest.root.join(cwd))
                    .unwrap_or_else(|| manifest.root.clone()),
            ),
        })
        .collect::<Vec<_>>();
    let process = ProcessConnectorExecutor::new(configs);
    Ok(Arc::new(ChainedConnectorExecutor::new(vec![
        Box::new(process),
        Box::new(DemoConnectorExecutor),
    ])))
}

fn apply_policy_mode(
    policy_mode: Option<&str>,
    modules: &[ProgramModule],
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<bool, String> {
    match validate_policy_mode(policy_mode)? {
        PolicyMode::Strict => {
            for module in modules {
                diagnostics.extend(lint::lint(&module.module));
            }
            Ok(true)
        }
        PolicyMode::Advisory | PolicyMode::Off => Ok(false),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PolicyMode {
    Strict,
    Advisory,
    Off,
}

fn validate_policy_mode(policy_mode: Option<&str>) -> Result<PolicyMode, String> {
    match policy_mode.unwrap_or("advisory") {
        "strict" => Ok(PolicyMode::Strict),
        "advisory" => Ok(PolicyMode::Advisory),
        "off" => Ok(PolicyMode::Off),
        other => Err(format!(
            "invalid [security].policy_mode `{other}`; expected `strict`, `advisory`, or `off`"
        )),
    }
}

fn has_failing_diagnostics(diagnostics: &[Diagnostic], fail_on_warnings: bool) -> bool {
    diagnostics.iter().any(|diagnostic| {
        diagnostic.is_error() || (fail_on_warnings && diagnostic.severity == Severity::Warning)
    })
}

struct ServeOptions {
    addr: String,
    service_name: Option<String>,
    max_requests: Option<usize>,
}

struct DebugOptions {
    workflow_name: Option<String>,
    breakpoints: Vec<BreakpointSpec>,
    format_json: bool,
}

struct DeployOptions {
    out_path: Option<PathBuf>,
    format_json: bool,
}

fn parse_deploy_options(args: impl Iterator<Item = String>) -> Result<DeployOptions, String> {
    let mut out_path = None;
    let mut format_json = false;
    let mut args = args.peekable();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" | "-o" => {
                let raw = args
                    .next()
                    .ok_or_else(|| "usage: --out <deploy-plan.json>".to_string())?;
                out_path = Some(PathBuf::from(raw));
            }
            "--json" => format_json = true,
            _ => return Err(format!("unexpected deploy argument '{arg}'")),
        }
    }

    Ok(DeployOptions {
        out_path,
        format_json,
    })
}

fn parse_debug_options(args: impl Iterator<Item = String>) -> Result<DebugOptions, String> {
    let mut workflow_name = None;
    let mut breakpoints = Vec::new();
    let mut format_json = false;
    let mut args = args.peekable();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--break" | "-b" => {
                let raw = args
                    .next()
                    .ok_or_else(|| "usage: --break <kind:target>".to_string())?;
                breakpoints.push(BreakpointSpec::parse(&raw)?);
            }
            "--json" => format_json = true,
            _ if workflow_name.is_none() => workflow_name = Some(arg),
            _ => return Err(format!("unexpected debug argument '{arg}'")),
        }
    }

    Ok(DebugOptions {
        workflow_name,
        breakpoints,
        format_json,
    })
}

fn serve_target(
    args: impl Iterator<Item = String>,
    command: &str,
) -> Result<(String, Option<String>), String> {
    let mut options = parse_serve_options(args)?;
    if options.max_requests.is_some() {
        return Err(format!("usage: num {command} <file.num> [addr] [service]"));
    }
    Ok((options.addr, options.service_name.take()))
}

fn parse_serve_options(args: impl Iterator<Item = String>) -> Result<ServeOptions, String> {
    let mut addr = None;
    let mut service_name = None;
    let mut max_requests = None;
    let mut args = args.peekable();

    while let Some(arg) = args.next() {
        if arg == "--max-requests" {
            let raw = args
                .next()
                .ok_or_else(|| "usage: --max-requests <count>".to_string())?;
            max_requests = Some(
                raw.parse::<usize>()
                    .map_err(|_| format!("invalid --max-requests value '{raw}'"))?,
            );
        } else if addr.is_none() {
            addr = Some(arg);
        } else if service_name.is_none() {
            service_name = Some(arg);
        } else {
            return Err(format!("unexpected serve argument '{arg}'"));
        }
    }

    Ok(ServeOptions {
        addr: addr.unwrap_or_else(|| "127.0.0.1:4000".to_string()),
        service_name,
        max_requests,
    })
}

fn print_diagnostics(diagnostics: &[num_compiler::diagnostic::Diagnostic]) {
    for diagnostic in diagnostics {
        eprint!("{diagnostic}");
    }
}

fn print_help() {
    println!("{}", help_text());
}

fn help_text() -> &'static str {
    "num 0.1.0\n\nCommands:\n  num check <file.num|dir>                     Parse and validate num source\n  num lint <file.num|dir>                      Run project quality/security lints\n  num fmt <file.num>                           Print formatted source\n  num ir <file.num>                            Print lowered IR\n  num run <file.num|dir>                       Validate and workflow runtime dry-run\n  num test <file.num|dir>                      Run .num test declarations\n  num trace <file.num|dir>                     Run workflow and print runtime trace JSON\n  num debug <file.num|dir> [workflow]          Run workflow with scripted breakpoints\n  num deploy [project-dir|file]                Build a deployment plan artifact\n  num compat [project-dir|file] [--json]       Check language/schema compatibility\n  num cost-report <file.num|dir> [--json]      Run workflow and summarize action costs\n  num audit-report <events.jsonl> [--json]     Summarize audit JSONL events\n  num workflow-report <state-root> [--json]    Summarize workflow state files\n  num route <file.num|dir> <METHOD> <PATH>     Dry-run a service route\n  num serve <file.num|dir> [addr] [service]    Serve HTTP requests for a service\n  num serve-once <file.num|dir> [addr] [service] Serve one HTTP request for a service\n  num new <name>                               Create a new num project\n  num lock [project-dir|file]                  Generate num.lock from num.toml\n  num import openapi <json> [module]           Generate .num connector contracts\n  num import sql <schema.sql> [module]         Generate .num database contracts\n  num completions <zsh>                        Print shell completion script\n  num lsp                                      Start the LSP server\n"
}

fn print_completions(shell: &str) -> Result<(), String> {
    match shell {
        "zsh" => {
            print!("{ZSH_COMPLETION}");
            Ok(())
        }
        other => Err(format!(
            "unsupported shell `{other}`\n\nSupported shells:\n  zsh"
        )),
    }
}

const ZSH_COMPLETION: &str = r#"#compdef num

_num_num_files() {
  local -a common_num_files
  common_num_files=()
  [[ -f main.num ]] && common_num_files+=("main.num")
  [[ -f src/main.num ]] && common_num_files+=("src/main.num")
  common_num_files+=(${(N)examples/*/src/main.num})

  if (( ${#common_num_files} )); then
    _alternative \
      'common-num-files:common num files:compadd -f -a common_num_files' \
      'num-files:num files:_path_files -g "*.num(-.)"' \
      'directories:directories:_path_files -/'
    return
  fi

  _alternative \
    'num-files:num files:_path_files -g "*.num(-.)"' \
    'directories:directories:_path_files -/'
}

_num() {
  local -a commands
    commands=(
    'check:parse and validate a num source file'
    'lint:run project quality/security lints'
    'fmt:print formatted source'
    'ir:print lowered IR'
    'run:validate and runtime dry-run'
    'test:run .num test declarations'
    'trace:run workflow and print runtime trace JSON'
    'debug:run workflow with scripted breakpoints'
    'deploy:build a deployment plan artifact'
    'compat:check language/schema compatibility'
    'cost-report:run workflow and summarize action costs'
    'audit-report:summarize audit JSONL events'
    'workflow-report:summarize workflow state files'
    'route:dry-run a service route'
    'serve:serve HTTP requests for a service'
    'serve-once:serve one HTTP request for a service'
    'new:create a new num project'
    'lock:generate num.lock from num.toml'
    'import:generate num source from external schemas'
    'completions:print shell completion script'
    'lsp:start the language server'
    'help:show help'
  )

  if (( CURRENT == 2 )); then
    _describe -t commands 'num command' commands
    return
  fi

  case "$words[2]" in
    check|lint|fmt|ir|run|test|trace|debug|deploy|compat|cost-report|route|serve|serve-once|lock)
      _num_num_files
      ;;
    audit-report)
      _path_files -g "*.jsonl(-.)"
      ;;
    workflow-report)
      _files -/
      ;;
    import)
      _values 'import kind' openapi sql
      ;;
    completions)
      _values 'shell' zsh
      ;;
    new)
      _files -/
      ;;
    *)
      _default
      ;;
  esac
}

_num "$@"
"#;

#[allow(dead_code)]
fn severity_code(severity: Severity) -> u8 {
    match severity {
        Severity::Error => 1,
        Severity::Warning => 2,
        Severity::Info => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_compiler::{lexer, parser, span::Span};
    use num_runtime::interpreter::Value;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn program_module(source: &str) -> ProgramModule {
        let lexed = lexer::lex("test.num", source);
        let parsed = parser::parse("test.num", &lexed.tokens);
        ProgramModule {
            source_name: "test.num".to_string(),
            module: parsed.module,
        }
    }

    #[test]
    fn strict_policy_mode_adds_blocking_lints() {
        let module = program_module(
            r#"
module tests.strict

service Api {
    route POST "/refunds" {
        audit("refund")
    }
}
"#,
        );
        let mut diagnostics = Vec::new();

        let fail_on_warnings =
            apply_policy_mode(Some("strict"), &[module], &mut diagnostics).unwrap();

        assert!(fail_on_warnings);
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "N4004"));
        assert!(has_failing_diagnostics(&diagnostics, fail_on_warnings));
    }

    #[test]
    fn advisory_policy_mode_does_not_add_lints() {
        let module = program_module(
            r#"
module tests.advisory

service Api {
    route POST "/refunds" {
        audit("refund")
    }
}
"#,
        );
        let mut diagnostics = Vec::new();

        let fail_on_warnings =
            apply_policy_mode(Some("advisory"), &[module], &mut diagnostics).unwrap();

        assert!(!fail_on_warnings);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn strict_policy_mode_treats_warnings_as_failing() {
        let diagnostics = vec![Diagnostic::warning(
            "N4000",
            "warning",
            Span::synthetic("test.num"),
        )];

        assert!(has_failing_diagnostics(&diagnostics, true));
        assert!(!has_failing_diagnostics(&diagnostics, false));
    }

    #[test]
    fn invalid_policy_mode_is_rejected() {
        assert!(validate_policy_mode(Some("relaxed")).is_err());
    }

    #[test]
    fn debug_options_parse_breakpoints_and_json_flag() {
        let options = parse_debug_options(
            [
                "main".to_string(),
                "--break".to_string(),
                "action:issue_refund".to_string(),
                "--json".to_string(),
            ]
            .into_iter(),
        )
        .unwrap();

        assert_eq!(options.workflow_name, Some("main".to_string()));
        assert!(options.format_json);
        assert_eq!(options.breakpoints.len(), 1);
        assert_eq!(options.breakpoints[0].target, "issue_refund");
    }

    #[test]
    fn deploy_options_parse_out_and_json_flag() {
        let options = parse_deploy_options(
            [
                "--out".to_string(),
                "dist/deploy.json".to_string(),
                "--json".to_string(),
            ]
            .into_iter(),
        )
        .unwrap();

        assert_eq!(options.out_path, Some(PathBuf::from("dist/deploy.json")));
        assert!(options.format_json);
    }

    #[cfg(unix)]
    #[test]
    fn connector_executor_uses_manifest_process_connector() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("num_connector_cli_{stamp}"));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("num.toml"),
            r#"
[project]
name = "app"
version = "0.1.0"

[connectors]
"echo.bool" = { command = "/bin/sh", args = "-c 'cat >/dev/null; echo true'" }
"#,
        )
        .unwrap();

        let executor = connector_executor_for_path(&root).unwrap();
        let result = executor.call("echo.bool", &[]).unwrap().unwrap();

        assert_eq!(result, Value::Bool(true));
        fs::remove_dir_all(root).unwrap();
    }
}
