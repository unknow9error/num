mod bench;
mod compatibility;
mod connector_cli;
mod connector_sdk;
mod connector_sdk_cli;
mod demo;
mod deploy;
mod integrity;
mod migration;
mod openapi;
mod package;
mod project;
mod registry;
mod registry_cli;
mod release_plan;
mod runtime_config;
mod sql_schema;
mod version_upgrade;
mod version_upgrade_cli;
mod workflow_cli;

use num_compiler::{
    check_program, compile, compile_program,
    diagnostic::{Diagnostic, Severity},
    formatter, lexer, lint, parser, Compilation, ProgramModule,
};
use num_runtime::{
    audit_report,
    connectors::{ChainedConnectorExecutor, ConnectorExecutor, DemoConnectorExecutor},
    cost_report,
    debugger::{BreakpointSpec, DebugReport},
    http,
    js_interop::{JavaScriptModuleConfig, JavaScriptModuleExecutor},
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
            let options = parse_fmt_options(args)?;
            run_fmt(options)
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
            let options = parse_run_options(args)?;
            let path = options.path;
            let compilation = compile_checked(&path)?;
            let connectors = connector_executor_for_path(&path, !options.format_json)?;
            let audit_target = runtime_config::resolve_interpreter_audit_target(&path)?;

            let mut runtime = num_runtime::interpreter::Runtime::with_connectors(
                &compilation.module,
                demo::default_permissions(),
                Box::new(connectors.clone()),
            );
            runtime.set_output_enabled(!options.format_json);

            let workflow_name = demo::first_workflow_name(&compilation.module)
                .ok_or_else(|| "No workflow declared in the module".to_string())?;
            let workflow_args = demo::workflow_args(&workflow_name);

            let result = runtime.run_workflow(&workflow_name, workflow_args);
            persist_interpreter_audits(&audit_target, "run", runtime.audit_events())?;
            if options.format_json {
                let payload = workflow_run_json(
                    &workflow_name,
                    &result,
                    runtime.last_error(),
                    runtime.trace_events(),
                );
                let json = serde_json::to_string_pretty(&payload)
                    .map_err(|err| format!("failed to render run JSON: {err}"))?;
                println!("{json}");
            }
            result?;
            Ok(())
        }
        "test" => {
            let path = required_path(args.next(), "test")?;
            let compilation = compile_checked(&path)?;
            let connectors = connector_executor_for_path(&path, true)?;
            let audit_target = runtime_config::resolve_interpreter_audit_target(&path)?;
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
                let result = runtime.run_test(test_name);
                persist_interpreter_audits(&audit_target, "test", runtime.audit_events())?;
                match result {
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
            let connectors = connector_executor_for_path(&path, false)?;
            let audit_target = runtime_config::resolve_interpreter_audit_target(&path)?;
            let mut runtime = num_runtime::interpreter::Runtime::with_connectors(
                &compilation.module,
                demo::default_permissions(),
                Box::new(connectors.clone()),
            );
            runtime.set_output_enabled(false);
            let workflow_name = demo::first_workflow_name(&compilation.module)
                .ok_or_else(|| "No workflow declared in the module".to_string())?;
            let workflow_args = demo::workflow_args(&workflow_name);

            let result = runtime.run_workflow(&workflow_name, workflow_args);
            persist_interpreter_audits(&audit_target, "trace", runtime.audit_events())?;
            result?;
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
            let connectors = connector_executor_for_path(&path, !options.format_json)?;
            let audit_target = runtime_config::resolve_interpreter_audit_target(&path)?;
            let mut runtime = num_runtime::interpreter::Runtime::with_connectors(
                &compilation.module,
                demo::default_permissions(),
                Box::new(connectors.clone()),
            );
            runtime.set_output_enabled(!options.format_json);
            let workflow_name = options
                .workflow_name
                .or_else(|| demo::first_workflow_name(&compilation.module))
                .ok_or_else(|| "No workflow declared in the module".to_string())?;
            let workflow_args = demo::workflow_args(&workflow_name);

            let result = runtime.run_workflow(&workflow_name, workflow_args);
            persist_interpreter_audits(&audit_target, "debug", runtime.audit_events())?;
            let report = DebugReport::from_trace(
                workflow_name,
                result.clone(),
                runtime.last_error().cloned(),
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
            let source_files = input.files.clone();
            let module_count = input.files.len();
            let policy_mode = input.policy_mode.clone();
            let program = compile_program(input.files, input.entry_source_name.as_deref());
            let compile_diagnostics = program.diagnostics.clone();
            let lint_diagnostics = program
                .modules
                .iter()
                .flat_map(|module| lint::lint(&module.module))
                .collect::<Vec<_>>();
            let fail_on_warnings = matches!(
                validate_policy_mode(policy_mode.as_deref())?,
                PolicyMode::Strict
            );
            let mut diagnostics = compile_diagnostics.clone();
            if fail_on_warnings {
                diagnostics.extend(lint_diagnostics.clone());
            }
            let plan = deploy::build_deployment_plan(&manifest, &program.module, module_count);
            if options.check {
                let check_report = deploy::build_deploy_check_report(
                    &plan,
                    &compile_diagnostics,
                    &lint_diagnostics,
                    fail_on_warnings,
                );
                let blocking = check_report["blocking"].as_bool().unwrap_or(true);
                let payload = serde_json::to_string_pretty(&check_report)
                    .map_err(|err| format!("failed to render deployment check JSON: {err}"))?;
                if let Some(out_path) = &options.out_path {
                    if let Some(parent) = out_path.parent() {
                        if !parent.as_os_str().is_empty() {
                            fs::create_dir_all(parent).map_err(|err| {
                                format!("failed to create {}: {err}", parent.display())
                            })?;
                        }
                    }
                    fs::write(out_path, format!("{payload}\n"))
                        .map_err(|err| format!("failed to write {}: {err}", out_path.display()))?;
                    if !options.format_json {
                        println!("wrote deployment check {}", out_path.display());
                    }
                }
                if options.format_json {
                    println!("{payload}");
                } else {
                    print_diagnostics(&diagnostics);
                    if options.out_path.is_none() {
                        print!("{}", plan.render_text());
                    }
                    println!(
                        "Deploy check: {}",
                        if blocking { "blocked" } else { "ready" }
                    );
                }
                return if blocking {
                    Err("deploy check failed".to_string())
                } else {
                    Ok(())
                };
            }
            print_diagnostics(&diagnostics);
            if has_failing_diagnostics(&diagnostics, fail_on_warnings) {
                return Err("program has compile errors".to_string());
            }

            let json = serde_json::to_string_pretty(&plan.to_json())
                .map_err(|err| format!("failed to render deployment plan JSON: {err}"))?;
            if options.kubernetes_dry_run {
                if options.apply {
                    return Err(
                        "--kubernetes-dry-run cannot be combined with --apply; real cluster mutation is not implemented yet"
                            .to_string(),
                    );
                }
                if options.out_path.is_some() {
                    return Err(
                        "--kubernetes-dry-run writes resources with --kubernetes-out, not --out"
                            .to_string(),
                    );
                }
                let dry_run = deploy::build_kubernetes_dry_run(&plan);
                if let Some(out_path) = &options.kubernetes_out_path {
                    if let Some(parent) = out_path.parent() {
                        if !parent.as_os_str().is_empty() {
                            fs::create_dir_all(parent).map_err(|err| {
                                format!("failed to create {}: {err}", parent.display())
                            })?;
                        }
                    }
                    fs::write(out_path, &dry_run.manifest)
                        .map_err(|err| format!("failed to write {}: {err}", out_path.display()))?;
                    if !options.format_json {
                        println!("wrote Kubernetes dry-run resources {}", out_path.display());
                    }
                }
                if options.format_json {
                    let payload = serde_json::json!({
                        "plan": plan.to_json(),
                        "kubernetes": dry_run.to_json(),
                    });
                    let payload = serde_json::to_string_pretty(&payload).map_err(|err| {
                        format!("failed to render Kubernetes dry-run JSON: {err}")
                    })?;
                    println!("{payload}");
                } else if options.kubernetes_out_path.is_none() {
                    print!("{}", dry_run.render_text());
                }
                return Ok(());
            }
            if options.check && options.apply {
                return Err("--check cannot be combined with --apply".to_string());
            }
            let artifact_report = if options.apply {
                let artifact_root = options
                    .artifact_dir
                    .clone()
                    .unwrap_or_else(|| deploy::default_artifact_root(&manifest));
                Some(deploy::materialize_deployment_artifact(
                    &plan,
                    &manifest,
                    &source_files,
                    &artifact_root,
                    options.replace,
                )?)
            } else {
                None
            };
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
                if !options.format_json {
                    println!("wrote deployment plan {}", out_path.display());
                }
            }
            if options.format_json {
                if let Some(report) = &artifact_report {
                    let payload = serde_json::json!({
                        "plan": plan.to_json(),
                        "artifact": report.to_json(),
                    });
                    let payload = serde_json::to_string_pretty(&payload)
                        .map_err(|err| format!("failed to render deployment JSON: {err}"))?;
                    println!("{payload}");
                } else {
                    println!("{json}");
                }
            } else if let Some(report) = &artifact_report {
                print!("{}", report.render_text());
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
                .map(compatibility::report_manifest)
                .collect::<Vec<_>>();
            let has_incompatible = reports.iter().any(|report| !report.is_compatible());
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
            if has_incompatible {
                let reasons = reports
                    .iter()
                    .filter_map(|report| report.reason.as_deref())
                    .collect::<Vec<_>>()
                    .join("; ");
                return Err(format!("compatibility check failed: {reasons}"));
            }
            Ok(())
        }
        "migrate" => {
            let (path, options) = parse_migrate_args(args)?;
            if options.source {
                let report = migration::plan_source_migrations(&path, options.write)?;
                if options.format_json {
                    let json = serde_json::to_string_pretty(&report.to_json())
                        .map_err(|err| format!("failed to render source migration JSON: {err}"))?;
                    println!("{json}");
                } else {
                    print!("{}", report.render_text());
                }
            } else {
                let report = migration::migrate_manifest(&path, options.write)?;
                if options.format_json {
                    let json = serde_json::to_string_pretty(&report.to_json())
                        .map_err(|err| format!("failed to render migration JSON: {err}"))?;
                    println!("{json}");
                } else {
                    print!("{}", report.render_text());
                }
            }
            Ok(())
        }
        "upgrade-version" => version_upgrade_cli::run(args),
        "bench" => bench::run(args),
        "release-plan" => print_release_plan(args),
        "version" => print_version(args),
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
            let connectors = connector_executor_for_path(&path, !format_json)?;
            let audit_target = runtime_config::resolve_interpreter_audit_target(&path)?;
            let mut runtime = num_runtime::interpreter::Runtime::with_connectors(
                &compilation.module,
                demo::default_permissions(),
                Box::new(connectors.clone()),
            );
            runtime.set_output_enabled(!format_json);
            let workflow_name = demo::first_workflow_name(&compilation.module)
                .ok_or_else(|| "No workflow declared in the module".to_string())?;
            let workflow_args = demo::workflow_args(&workflow_name);

            let result = runtime.run_workflow(&workflow_name, workflow_args);
            persist_interpreter_audits(&audit_target, "cost-report", runtime.audit_events())?;
            result?;
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
            let path = args.next().map(PathBuf::from).ok_or_else(|| {
                "usage: num workflow-report <state-root|project-dir|file.num> [--json]".to_string()
            })?;
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
            let runtime_paths = runtime_config::resolve_workflow_runtime_paths(&path)?;
            let store = FileStateStore::new(&runtime_paths.state_root);
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
                "usage: num route <file.num|dir> <METHOD> <PATH> [service] [--tenant <tenant>]"
                    .to_string()
            })?;
            let route_path = args.next().ok_or_else(|| {
                "usage: num route <file.num|dir> <METHOD> <PATH> [service] [--tenant <tenant>]"
                    .to_string()
            })?;
            let route_options = parse_route_options(args)?;
            let compilation = compile_checked(&path)?;

            let service_name = route_options
                .clone()
                .service_name
                .or_else(|| demo::first_service_name(&compilation.module))
                .ok_or_else(|| "No service declared in the module".to_string())?;
            let input = demo::route_input(&compilation.module, &service_name, &method, &route_path);
            let connectors = connector_executor_for_path(&path, false)?;
            let audit_target = runtime_config::resolve_interpreter_audit_target(&path)?;
            let tenant_isolation = runtime_config::resolve_tenant_isolation(&path)?;
            let runtime = ServiceRuntime::with_connectors(
                &compilation.module,
                service_name.clone(),
                demo::default_permissions(),
                connectors,
            )
            .with_audit_recorder(service_audit_recorder(audit_target, service_name.clone()))
            .with_output_enabled(false)
            .with_tenant_isolation(tenant_isolation);
            let mut request = num_runtime::http::HttpRequest::new(method, route_path, "");
            route_options.apply_headers(&mut request);
            let response = runtime.handle_http_request_with_empty_body_input(&request, input);
            print!("{}", response.body);
            if response.status >= 400 {
                Err(format!("route failed with HTTP status {}", response.status))
            } else {
                Ok(())
            }
        }
        "serve-once" => {
            let path = required_path(args.next(), "serve-once")?;
            let (addr, service_name) = serve_target(args, "serve-once")?;
            let compilation = compile_checked(&path)?;
            let service_name = service_name
                .or_else(|| ServiceRuntime::first_service_name(&compilation.module))
                .ok_or_else(|| "No service declared in the module".to_string())?;
            let connectors = connector_executor_for_path(&path, true)?;
            let audit_target = runtime_config::resolve_interpreter_audit_target(&path)?;
            let tenant_isolation = runtime_config::resolve_tenant_isolation(&path)?;
            let runtime = ServiceRuntime::with_connectors(
                &compilation.module,
                service_name.clone(),
                demo::default_permissions(),
                connectors,
            )
            .with_audit_recorder(service_audit_recorder(audit_target, service_name.clone()))
            .with_tenant_isolation(tenant_isolation);

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
            let connectors = connector_executor_for_path(&path, true)?;
            let audit_target = runtime_config::resolve_interpreter_audit_target(&path)?;
            let tenant_isolation = runtime_config::resolve_tenant_isolation(&path)?;
            let runtime = ServiceRuntime::with_connectors(
                &compilation.module,
                service_name.clone(),
                demo::default_permissions(),
                connectors,
            )
            .with_audit_recorder(service_audit_recorder(audit_target, service_name.clone()))
            .with_tenant_isolation(tenant_isolation);

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
            let options = parse_lock_options(args)?;
            if options.migrate {
                let report = package::migrate_lockfile(&options.path, options.write)?;
                if options.format_json {
                    let json = serde_json::to_string_pretty(&report.to_json())
                        .map_err(|err| format!("failed to render lock migration JSON: {err}"))?;
                    println!("{json}");
                } else {
                    print!("{}", report.render_text());
                }
            } else if options.check {
                let lock_path = package::validate_project_lockfile(&options.path)?;
                println!("checked {}", lock_path.display());
            } else {
                let lock_path = package::write_lockfile(&options.path)?;
                println!("wrote {}", lock_path.display());
            }
            Ok(())
        }
        "registry" => registry_cli::run(args),
        "workflow" => workflow_cli::run(args),
        "connector" => connector_cli::run(args),
        "connector-sdk" => connector_sdk_cli::run(args),
        "import" => match args.next().as_deref() {
            Some("openapi") => {
                let path = args.next().map(PathBuf::from).ok_or_else(|| {
                    "usage: num import openapi <openapi.json|yaml> [module.name]".to_string()
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
                .ok_or_else(|| "usage: num completions <bash|fish|zsh>".to_string())?;
            print_completions(&shell)
        }
        "lsp" => num_lsp::run_server(),
        "--version" | "-V" => print_version(args),
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

fn connector_executor_for_path(
    path: &Path,
    demo_output_enabled: bool,
) -> Result<Arc<dyn ConnectorExecutor>, String> {
    let demo = if demo_output_enabled {
        DemoConnectorExecutor::new()
    } else {
        DemoConnectorExecutor::silent()
    };
    let Some(manifest) = package::PackageManifest::discover(path)? else {
        return Ok(Arc::new(demo));
    };
    if manifest.connectors.is_empty() && manifest.javascript.is_empty() {
        return Ok(Arc::new(demo));
    }

    let process_configs = manifest
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
            timeout_ms: connector.timeout_ms,
        })
        .collect::<Vec<_>>();
    let js_configs = manifest
        .javascript
        .iter()
        .map(|module| JavaScriptModuleConfig {
            method: module.method.clone(),
            module: {
                let path = manifest.root.join(&module.module);
                path.canonicalize().unwrap_or(path)
            },
            export: module.export.clone(),
            cwd: Some(
                module
                    .cwd
                    .as_ref()
                    .map(|cwd| manifest.root.join(cwd))
                    .unwrap_or_else(|| manifest.root.clone()),
            ),
            timeout_ms: module.timeout_ms,
        })
        .collect::<Vec<_>>();

    let mut executors: Vec<Box<dyn ConnectorExecutor>> = Vec::new();
    if !js_configs.is_empty() {
        executors.push(Box::new(JavaScriptModuleExecutor::new(js_configs)));
    }
    if !process_configs.is_empty() {
        executors.push(Box::new(ProcessConnectorExecutor::new(process_configs)));
    }
    executors.push(Box::new(demo));
    Ok(Arc::new(ChainedConnectorExecutor::new(executors)))
}

fn persist_interpreter_audits(
    target: &runtime_config::InterpreterAuditTarget,
    command: &str,
    events: &[String],
) -> Result<(), String> {
    runtime_config::write_interpreter_audit_events(target, command, events)
}

fn service_audit_recorder(
    target: runtime_config::InterpreterAuditTarget,
    service_name: String,
) -> impl Fn(&num_runtime::SecurityContext, &str, &str, &[String]) -> Result<(), num_runtime::RuntimeError>
{
    move |security, method, path, events| {
        runtime_config::write_service_audit_events(
            &target,
            &service_name,
            method,
            path,
            security,
            events,
        )
        .map_err(num_runtime::RuntimeError::Storage)
    }
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct RouteOptions {
    service_name: Option<String>,
    actor: Option<String>,
    tenant: Option<String>,
    request_id: Option<String>,
    correlation_id: Option<String>,
}

impl RouteOptions {
    fn apply_headers(self, request: &mut num_runtime::http::HttpRequest) {
        if let Some(actor) = self.actor {
            request.headers.insert("x-actor".to_string(), actor);
        }
        if let Some(tenant) = self.tenant {
            request.headers.insert("x-tenant".to_string(), tenant);
        }
        if let Some(request_id) = self.request_id {
            request
                .headers
                .insert("x-request-id".to_string(), request_id);
        }
        if let Some(correlation_id) = self.correlation_id {
            request
                .headers
                .insert("x-correlation-id".to_string(), correlation_id);
        }
    }
}

struct DebugOptions {
    workflow_name: Option<String>,
    breakpoints: Vec<BreakpointSpec>,
    format_json: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FmtMode {
    Stdout,
    Write,
    Check,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FmtOptions {
    path: PathBuf,
    mode: FmtMode,
}

struct RunOptions {
    path: PathBuf,
    format_json: bool,
}

struct DeployOptions {
    out_path: Option<PathBuf>,
    artifact_dir: Option<PathBuf>,
    kubernetes_out_path: Option<PathBuf>,
    kubernetes_dry_run: bool,
    check: bool,
    apply: bool,
    replace: bool,
    format_json: bool,
}

struct LockOptions {
    path: PathBuf,
    check: bool,
    migrate: bool,
    write: bool,
    format_json: bool,
}

struct MigrateOptions {
    write: bool,
    format_json: bool,
    source: bool,
}

fn parse_lock_options(args: impl Iterator<Item = String>) -> Result<LockOptions, String> {
    let mut path = None;
    let mut check = false;
    let mut migrate = false;
    let mut write = false;
    let mut format_json = false;

    for arg in args {
        match arg.as_str() {
            "--check" => check = true,
            "--migrate" => migrate = true,
            "--write" => write = true,
            "--json" => format_json = true,
            other if other.starts_with("--") => {
                return Err(format!("unexpected lock argument '{other}'"));
            }
            _ if path.is_none() => path = Some(PathBuf::from(arg)),
            _ => return Err(format!("unexpected lock argument '{arg}'")),
        }
    }
    if check && (migrate || write || format_json) {
        return Err(
            "`num lock --check` cannot be combined with --migrate, --write, or --json".to_string(),
        );
    }
    if (write || format_json) && !migrate {
        return Err("`num lock --write` and `--json` require --migrate".to_string());
    }

    Ok(LockOptions {
        path: path.unwrap_or_else(|| PathBuf::from(".")),
        check,
        migrate,
        write,
        format_json,
    })
}

fn parse_migrate_args(
    args: impl Iterator<Item = String>,
) -> Result<(PathBuf, MigrateOptions), String> {
    let mut path = None;
    let mut write = false;
    let mut format_json = false;
    let mut source = false;

    for arg in args {
        match arg.as_str() {
            "--write" => write = true,
            "--json" => format_json = true,
            "--source" => source = true,
            _ if path.is_none() => path = Some(PathBuf::from(arg)),
            _ => return Err(format!("unexpected migrate argument '{arg}'")),
        }
    }

    Ok((
        path.unwrap_or_else(|| PathBuf::from(".")),
        MigrateOptions {
            write,
            format_json,
            source,
        },
    ))
}

fn parse_deploy_options(args: impl Iterator<Item = String>) -> Result<DeployOptions, String> {
    let mut out_path = None;
    let mut artifact_dir = None;
    let mut kubernetes_out_path = None;
    let mut kubernetes_dry_run = false;
    let mut check = false;
    let mut apply = false;
    let mut replace = false;
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
            "--dir" => {
                let raw = args
                    .next()
                    .ok_or_else(|| "usage: --dir <artifact-dir>".to_string())?;
                artifact_dir = Some(PathBuf::from(raw));
            }
            "--kubernetes-dry-run" => kubernetes_dry_run = true,
            "--kubernetes-out" => {
                let raw = args
                    .next()
                    .ok_or_else(|| "usage: --kubernetes-out <resources.yaml>".to_string())?;
                kubernetes_out_path = Some(PathBuf::from(raw));
            }
            "--check" => check = true,
            "--apply" => apply = true,
            "--replace" => replace = true,
            "--json" => format_json = true,
            _ => return Err(format!("unexpected deploy argument '{arg}'")),
        }
    }

    if kubernetes_out_path.is_some() && !kubernetes_dry_run {
        return Err("--kubernetes-out requires --kubernetes-dry-run".to_string());
    }
    if check && apply {
        return Err("--check cannot be combined with --apply".to_string());
    }
    if check && kubernetes_dry_run {
        return Err("--check cannot be combined with --kubernetes-dry-run".to_string());
    }

    Ok(DeployOptions {
        out_path,
        artifact_dir,
        kubernetes_out_path,
        kubernetes_dry_run,
        check,
        apply,
        replace,
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

fn parse_run_options(args: impl Iterator<Item = String>) -> Result<RunOptions, String> {
    let mut path = None;
    let mut format_json = false;

    for arg in args {
        match arg.as_str() {
            "--json" => format_json = true,
            other if other.starts_with("--") => {
                return Err(format!("unexpected run argument '{other}'"));
            }
            _ if path.is_none() => path = Some(PathBuf::from(arg)),
            _ => return Err(format!("unexpected run argument '{arg}'")),
        }
    }

    Ok(RunOptions {
        path: path.ok_or_else(|| "usage: num run <file.num|dir> [--json]".to_string())?,
        format_json,
    })
}

fn parse_fmt_options(args: impl Iterator<Item = String>) -> Result<FmtOptions, String> {
    let mut path = None;
    let mut mode = FmtMode::Stdout;

    for arg in args {
        match arg.as_str() {
            "--write" | "-w" => {
                if mode == FmtMode::Check {
                    return Err("`num fmt --write` cannot be combined with --check".to_string());
                }
                mode = FmtMode::Write;
            }
            "--check" => {
                if mode == FmtMode::Write {
                    return Err("`num fmt --check` cannot be combined with --write".to_string());
                }
                mode = FmtMode::Check;
            }
            other if other.starts_with("--") => {
                return Err(format!("unexpected fmt argument '{other}'"));
            }
            _ if path.is_none() => path = Some(PathBuf::from(arg)),
            _ => return Err(format!("unexpected fmt argument '{arg}'")),
        }
    }

    Ok(FmtOptions {
        path: path.ok_or_else(|| "usage: num fmt [--write|--check] <file.num|dir>".to_string())?,
        mode,
    })
}

fn run_fmt(options: FmtOptions) -> Result<(), String> {
    match options.mode {
        FmtMode::Stdout => {
            let source = fs::read_to_string(&options.path)
                .map_err(|err| format!("failed to read {}: {err}", options.path.display()))?;
            let formatted = format_num_source(&options.path, &source)?;
            print!("{formatted}");
            Ok(())
        }
        FmtMode::Write => {
            let report = format_num_paths(&options.path, true)?;
            for path in &report.changed {
                println!("formatted {}", path.display());
            }
            println!(
                "num fmt wrote: {} changed, {} checked",
                report.changed.len(),
                report.checked
            );
            Ok(())
        }
        FmtMode::Check => {
            let report = format_num_paths(&options.path, false)?;
            if report.changed.is_empty() {
                println!("num fmt --check passed: {} files", report.checked);
                return Ok(());
            }
            for path in &report.changed {
                println!("unformatted {}", path.display());
            }
            Err(format!(
                "num fmt --check failed: {} unformatted of {} files",
                report.changed.len(),
                report.checked
            ))
        }
    }
}

struct FmtReport {
    checked: usize,
    changed: Vec<PathBuf>,
}

fn format_num_paths(path: &Path, write: bool) -> Result<FmtReport, String> {
    let paths = collect_fmt_paths(path)?;
    let mut changed = Vec::new();

    for path in &paths {
        let source = fs::read_to_string(path)
            .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
        let formatted = format_num_source(path, &source)?;
        if formatted != source {
            changed.push(path.clone());
            if write {
                fs::write(path, formatted)
                    .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
            }
        }
    }

    Ok(FmtReport {
        checked: paths.len(),
        changed,
    })
}

fn collect_fmt_paths(path: &Path) -> Result<Vec<PathBuf>, String> {
    if path.is_file() {
        if is_num_file(path) {
            return Ok(vec![path.to_path_buf()]);
        }
        return Err(format!("{} is not a .num file", path.display()));
    }
    if !path.is_dir() {
        return Err(format!(
            "{} is not a .num file or directory",
            path.display()
        ));
    }

    let mut paths = Vec::new();
    collect_fmt_paths_recursive(path, &mut paths)?;
    paths.sort();
    if paths.is_empty() {
        return Err(format!("no .num files found under {}", path.display()));
    }
    Ok(paths)
}

fn collect_fmt_paths_recursive(path: &Path, paths: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in
        fs::read_dir(path).map_err(|err| format!("failed to read {}: {err}", path.display()))?
    {
        let entry =
            entry.map_err(|err| format!("failed to read {} entry: {err}", path.display()))?;
        let entry_path = entry.path();
        if entry_path.is_dir() {
            collect_fmt_paths_recursive(&entry_path, paths)?;
        } else if is_num_file(&entry_path) {
            paths.push(entry_path);
        }
    }
    Ok(())
}

fn is_num_file(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension == "num")
}

fn format_num_source(path: &Path, source: &str) -> Result<String, String> {
    let source_name = path.display().to_string();
    let lexed = lexer::lex(&source_name, source);
    let parsed = parser::parse(&source_name, &lexed.tokens);
    let mut diagnostics = lexed.diagnostics;
    diagnostics.extend(parsed.diagnostics);
    print_diagnostics(&diagnostics);
    if diagnostics.iter().any(|diagnostic| diagnostic.is_error()) {
        return Err(format!("num fmt failed: {}", path.display()));
    }
    Ok(formatter::format_module(&parsed.module))
}

fn parse_route_options(args: impl Iterator<Item = String>) -> Result<RouteOptions, String> {
    let mut options = RouteOptions::default();
    let mut args = args.peekable();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--actor" => options.actor = Some(next_route_value(&mut args, "--actor")?),
            "--tenant" => options.tenant = Some(next_route_value(&mut args, "--tenant")?),
            "--request-id" => {
                options.request_id = Some(next_route_value(&mut args, "--request-id")?)
            }
            "--correlation-id" => {
                options.correlation_id = Some(next_route_value(&mut args, "--correlation-id")?)
            }
            other if other.starts_with("--") => {
                return Err(format!("unexpected route argument '{other}'"));
            }
            _ if options.service_name.is_none() => options.service_name = Some(arg),
            _ => return Err(format!("unexpected route argument '{arg}'")),
        }
    }

    Ok(options)
}

fn next_route_value(
    args: &mut std::iter::Peekable<impl Iterator<Item = String>>,
    flag: &str,
) -> Result<String, String> {
    args.next()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("usage: {flag} <value>"))
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

fn workflow_run_json(
    workflow: &str,
    result: &Result<(), String>,
    runtime_error: Option<&num_runtime::RuntimeError>,
    trace: &[num_runtime::observability::RuntimeTraceEvent],
) -> serde_json::Value {
    serde_json::json!({
        "workflow": workflow,
        "status": if result.is_ok() { "completed" } else { "failed" },
        "error": result.as_ref().err().map(|error| num_runtime::redaction::redact_text(error)),
        "runtime_error": runtime_error.map(num_runtime::RuntimeError::to_json),
        "trace": trace.iter().map(|event| event.to_json()).collect::<Vec<_>>(),
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

fn print_release_plan(args: impl Iterator<Item = String>) -> Result<(), String> {
    let (path, format_json) = parse_release_plan_options(args)?;
    let plan = release_plan::plan_from_changelog(&path, env!("CARGO_PKG_VERSION"))?;
    if format_json {
        let json = serde_json::to_string_pretty(&plan.to_json())
            .map_err(|err| format!("failed to render release plan JSON: {err}"))?;
        println!("{json}");
    } else {
        print!("{}", plan.render_text());
    }
    Ok(())
}

fn parse_release_plan_options(
    args: impl Iterator<Item = String>,
) -> Result<(PathBuf, bool), String> {
    let mut path = None;
    let mut format_json = false;
    for arg in args {
        match arg.as_str() {
            "--json" => format_json = true,
            other if other.starts_with("--") => {
                return Err(format!("unexpected release-plan argument '{other}'"));
            }
            _ if path.is_none() => path = Some(PathBuf::from(arg)),
            _ => return Err(format!("unexpected release-plan argument '{arg}'")),
        }
    }
    Ok((
        path.unwrap_or_else(|| PathBuf::from("CHANGELOG.md")),
        format_json,
    ))
}

fn print_version(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    let format_json = match args.next().as_deref() {
        Some("--json") => true,
        Some(other) => return Err(format!("unexpected version argument '{other}'")),
        None => false,
    };
    if let Some(other) = args.next() {
        return Err(format!("unexpected version argument '{other}'"));
    }
    if format_json {
        let payload = version_json();
        let json = serde_json::to_string_pretty(&payload)
            .map_err(|err| format!("failed to render version JSON: {err}"))?;
        println!("{json}");
    } else {
        println!("num {}", env!("CARGO_PKG_VERSION"));
        println!("language {}", compatibility::CURRENT_LANGUAGE_VERSION);
        println!("manifest_schema {}", compatibility::CURRENT_MANIFEST_SCHEMA);
        println!("lockfile_schema {}", package::CURRENT_LOCKFILE_SCHEMA);
    }
    Ok(())
}

fn version_json() -> serde_json::Value {
    serde_json::json!({
        "cli": env!("CARGO_PKG_VERSION"),
        "language": compatibility::CURRENT_LANGUAGE_VERSION,
        "manifest_schema": compatibility::CURRENT_MANIFEST_SCHEMA,
        "lockfile_schema": package::CURRENT_LOCKFILE_SCHEMA,
    })
}

fn help_text() -> String {
    format!(
        "num {}\n\nCommands:\n  num check <file.num|dir>                     Parse and validate num source\n  num lint <file.num|dir>                      Run project quality/security lints\n  num fmt [--write|--check] <file.num|dir>     Format source or verify formatting\n  num ir <file.num>                            Print lowered IR\n  num run <file.num|dir> [--json]              Validate and workflow runtime dry-run\n  num test <file.num|dir>                      Run .num test declarations\n  num trace <file.num|dir>                     Run workflow and print runtime trace JSON\n  num debug <file.num|dir> [workflow]          Run workflow with scripted breakpoints\n  num deploy [project-dir|file] [--check|--apply|--kubernetes-dry-run] Build/materialize deployment artifacts\n  num compat [project-dir|file] [--json]       Check language/schema compatibility\n  num migrate [project-dir|file] [--write] [--json] Plan or apply manifest migrations\n  num migrate [project-dir|file] --source [--json] Plan source migrations\n  num upgrade-version [project-dir|file]       Plan/apply manifest version upgrades\n  num bench [fixture-root] [--json|--compare] Benchmark lex/parse/check fixtures\n  num release-plan [CHANGELOG.md] [--json]     Compute SemVer release bump\n  num version [--json]                         Print CLI/language/schema versions\n  num registry <publish|list|index|install>    Manage local package registries\n  num workflow <enqueue|drain|lease-heartbeat> Queue/drain durable workflow events\n  num connector <probe>                        Probe process connector bindings\n  num connector-sdk [project-dir|file]         Generate connector implementation SDKs\n  num cost-report <file.num|dir> [--json]      Run workflow and summarize action costs\n  num audit-report <events.jsonl> [--json]     Summarize audit JSONL events\n  num workflow-report <state-root|project> [--json] Summarize workflow state files\n  num route <file.num|dir> <METHOD> <PATH> [service] [--tenant <tenant>] Dry-run a service route\n  num serve <file.num|dir> [addr] [service]    Serve HTTP requests for a service\n  num serve-once <file.num|dir> [addr] [service] Serve one HTTP request for a service\n  num new <name>                               Create a new num project\n  num lock [project-dir|file] [--check|--migrate] Generate, validate, or migrate num.lock\n  num import openapi <json|yaml> [module]      Generate .num connector contracts\n  num import sql <schema.sql> [module]         Generate .num database contracts\n  num completions <bash|fish|zsh>              Print shell completion script\n  num lsp                                      Start the LSP server\n",
        env!("CARGO_PKG_VERSION")
    )
}

fn print_completions(shell: &str) -> Result<(), String> {
    print!("{}", completion_script(shell)?);
    Ok(())
}

fn completion_script(shell: &str) -> Result<&'static str, String> {
    match shell {
        "bash" => Ok(BASH_COMPLETION),
        "fish" => Ok(FISH_COMPLETION),
        "zsh" => Ok(ZSH_COMPLETION),
        other => Err(format!(
            "unsupported shell `{other}`\n\nSupported shells:\n  bash\n  fish\n  zsh"
        )),
    }
}

const BASH_COMPLETION: &str = r#"# bash completion for num

_num()
{
    local cur words cword
    COMPREPLY=()
    words=("${COMP_WORDS[@]}")
    cword=$COMP_CWORD
    cur="${COMP_WORDS[COMP_CWORD]}"

    local commands="check lint fmt ir run test trace debug deploy compat migrate upgrade-version bench release-plan version registry workflow connector connector-sdk cost-report audit-report workflow-report route serve serve-once new lock import completions lsp help"

    if [[ $cword -eq 1 ]]; then
        COMPREPLY=( $(compgen -W "$commands" -- "$cur") )
        return
    fi

    case "${words[1]}" in
        registry)
            COMPREPLY=( $(compgen -W "publish list index install" -- "$cur") )
            ;;
        workflow)
            COMPREPLY=( $(compgen -W "enqueue drain lease-heartbeat" -- "$cur") )
            ;;
        connector)
            COMPREPLY=( $(compgen -W "probe" -- "$cur") )
            ;;
        import)
            if [[ $cword -eq 2 ]]; then
                COMPREPLY=( $(compgen -W "openapi sql" -- "$cur") )
            else
                COMPREPLY=( $(compgen -f -- "$cur") )
            fi
            ;;
        completions)
            COMPREPLY=( $(compgen -W "bash fish zsh" -- "$cur") )
            ;;
        audit-report)
            COMPREPLY=( $(compgen -f -X '!*.jsonl' -- "$cur") )
            ;;
        workflow-report|new)
            COMPREPLY=( $(compgen -d -- "$cur") )
            ;;
        check|lint|fmt|ir|run|test|trace|debug|deploy|compat|migrate|upgrade-version|bench|release-plan|connector-sdk|cost-report|route|serve|serve-once|lock)
            COMPREPLY=( $(compgen -f -X '!*.num' -- "$cur") $(compgen -d -- "$cur") )
            ;;
        *)
            COMPREPLY=( $(compgen -f -- "$cur") )
            ;;
    esac
}
complete -F _num num
"#;

const FISH_COMPLETION: &str = r#"# fish completion for num

complete -c num -f -n "__fish_use_subcommand" -a "check" -d "Parse and validate num source"
complete -c num -f -n "__fish_use_subcommand" -a "lint" -d "Run project quality/security lints"
complete -c num -f -n "__fish_use_subcommand" -a "fmt" -d "Format source or verify formatting"
complete -c num -f -n "__fish_use_subcommand" -a "ir" -d "Print lowered IR"
complete -c num -f -n "__fish_use_subcommand" -a "run" -d "Validate and workflow runtime dry-run"
complete -c num -f -n "__fish_use_subcommand" -a "test" -d "Run .num test declarations"
complete -c num -f -n "__fish_use_subcommand" -a "trace" -d "Run workflow and print runtime trace JSON"
complete -c num -f -n "__fish_use_subcommand" -a "debug" -d "Run workflow with scripted breakpoints"
complete -c num -f -n "__fish_use_subcommand" -a "deploy" -d "Build deployment artifacts"
complete -c num -f -n "__fish_use_subcommand" -a "compat" -d "Check language/schema compatibility"
complete -c num -f -n "__fish_use_subcommand" -a "migrate" -d "Plan or apply manifest/source migrations"
complete -c num -f -n "__fish_use_subcommand" -a "upgrade-version" -d "Plan or apply manifest version upgrades"
complete -c num -f -n "__fish_use_subcommand" -a "bench" -d "Benchmark fixtures"
complete -c num -f -n "__fish_use_subcommand" -a "release-plan" -d "Compute SemVer release bump"
complete -c num -f -n "__fish_use_subcommand" -a "version" -d "Print CLI/language/schema versions"
complete -c num -f -n "__fish_use_subcommand" -a "registry" -d "Manage local package registries"
complete -c num -f -n "__fish_use_subcommand" -a "workflow" -d "Queue and drain durable workflow events"
complete -c num -f -n "__fish_use_subcommand" -a "connector" -d "Probe process connector bindings"
complete -c num -f -n "__fish_use_subcommand" -a "connector-sdk" -d "Generate connector implementation SDKs"
complete -c num -f -n "__fish_use_subcommand" -a "cost-report" -d "Summarize action costs"
complete -c num -f -n "__fish_use_subcommand" -a "audit-report" -d "Summarize audit JSONL events"
complete -c num -f -n "__fish_use_subcommand" -a "workflow-report" -d "Summarize workflow state files"
complete -c num -f -n "__fish_use_subcommand" -a "route" -d "Dry-run a service route"
complete -c num -f -n "__fish_use_subcommand" -a "serve" -d "Serve HTTP requests"
complete -c num -f -n "__fish_use_subcommand" -a "serve-once" -d "Serve one HTTP request"
complete -c num -f -n "__fish_use_subcommand" -a "new" -d "Create a new num project"
complete -c num -f -n "__fish_use_subcommand" -a "lock" -d "Generate, validate, or migrate num.lock"
complete -c num -f -n "__fish_use_subcommand" -a "import" -d "Generate num source from external schemas"
complete -c num -f -n "__fish_use_subcommand" -a "completions" -d "Print shell completion script"
complete -c num -f -n "__fish_use_subcommand" -a "lsp" -d "Start the language server"
complete -c num -f -n "__fish_use_subcommand" -a "help" -d "Show help"

complete -c num -f -n "__fish_seen_subcommand_from registry" -a "publish list index install"
complete -c num -f -n "__fish_seen_subcommand_from workflow" -a "enqueue drain lease-heartbeat"
complete -c num -f -n "__fish_seen_subcommand_from connector" -a "probe"
complete -c num -f -n "__fish_seen_subcommand_from import" -a "openapi sql"
complete -c num -f -n "__fish_seen_subcommand_from completions" -a "bash fish zsh"

complete -c num -n "__fish_seen_subcommand_from check lint fmt ir run test trace debug deploy compat migrate upgrade-version bench release-plan connector-sdk cost-report route serve serve-once lock" -a "(__fish_complete_suffix .num)"
complete -c num -n "__fish_seen_subcommand_from audit-report" -a "(__fish_complete_suffix .jsonl)"
complete -c num -n "__fish_seen_subcommand_from workflow-report new" -a "(__fish_complete_directories)"
"#;

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
    'fmt:format source or verify formatting'
    'ir:print lowered IR'
    'run:validate and runtime dry-run'
    'test:run .num test declarations'
    'trace:run workflow and print runtime trace JSON'
    'debug:run workflow with scripted breakpoints'
    'deploy:build a deployment plan artifact'
    'compat:check language/schema compatibility'
    'migrate:plan manifest or source migrations'
    'upgrade-version:plan or apply manifest version upgrades'
    'bench:benchmark lex parse and check fixtures'
    'release-plan:compute SemVer release bump'
    'version:print CLI/language/schema versions'
    'registry:manage local package registries'
    'workflow:queue and drain durable workflow events'
    'connector:probe process connector bindings'
    'connector-sdk:generate connector implementation SDKs'
    'cost-report:run workflow and summarize action costs'
    'audit-report:summarize audit JSONL events'
    'workflow-report:summarize workflow state files'
    'route:dry-run a service route'
    'serve:serve HTTP requests for a service'
    'serve-once:serve one HTTP request for a service'
    'new:create a new num project'
    'lock:generate, validate, or migrate num.lock'
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
    check|lint|fmt|ir|run|test|trace|debug|deploy|compat|migrate|upgrade-version|bench|release-plan|connector-sdk|cost-report|route|serve|serve-once|lock)
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
    registry)
      _values 'registry command' publish list index install
      ;;
    workflow)
      _values 'workflow command' enqueue drain lease-heartbeat
      ;;
    connector)
      _values 'connector command' probe
      ;;
    completions)
      _values 'shell' bash fish zsh
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

    fn temp_cli_dir(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("num_cli_{name}_{stamp}"));
        fs::create_dir_all(&path).unwrap();
        path
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
    fn run_options_parse_path_and_json_flag() {
        let options = parse_run_options(
            ["examples/refund_workflow".to_string(), "--json".to_string()].into_iter(),
        )
        .unwrap();

        assert_eq!(options.path, PathBuf::from("examples/refund_workflow"));
        assert!(options.format_json);
    }

    #[test]
    fn fmt_options_parse_write_and_check_modes() {
        let write = parse_fmt_options(
            [
                "--write".to_string(),
                "examples/refund_workflow".to_string(),
            ]
            .into_iter(),
        )
        .unwrap();
        let check = parse_fmt_options(
            [
                "examples/refund_workflow".to_string(),
                "--check".to_string(),
            ]
            .into_iter(),
        )
        .unwrap();

        assert_eq!(write.mode, FmtMode::Write);
        assert_eq!(write.path, PathBuf::from("examples/refund_workflow"));
        assert_eq!(check.mode, FmtMode::Check);
        assert_eq!(check.path, PathBuf::from("examples/refund_workflow"));
        assert!(
            parse_fmt_options(["--write".to_string(), "--check".to_string()].into_iter()).is_err()
        );
    }

    #[test]
    fn fmt_check_passes_for_unchanged_file() {
        let root = temp_cli_dir("fmt_check_clean");
        let path = root.join("main.num");
        let formatted = format_num_source(
            &path,
            "module tests.fmt\nworkflow main(){\naudit(\"ok\")\n}\n",
        )
        .unwrap();
        fs::write(&path, formatted).unwrap();

        run_fmt(FmtOptions {
            path: path.clone(),
            mode: FmtMode::Check,
        })
        .unwrap();

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn fmt_check_fails_for_changed_file_without_writing() {
        let root = temp_cli_dir("fmt_check_dirty");
        let path = root.join("main.num");
        let source = "module tests.fmt\nworkflow main(){\naudit(\"ok\")\n}\n";
        fs::write(&path, source).unwrap();

        let err = run_fmt(FmtOptions {
            path: path.clone(),
            mode: FmtMode::Check,
        })
        .unwrap_err();

        assert!(err.contains("unformatted"));
        assert_eq!(fs::read_to_string(&path).unwrap(), source);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn fmt_write_formats_only_num_files_in_directory() {
        let root = temp_cli_dir("fmt_write_dir");
        let source_path = root.join("main.num");
        let text_path = root.join("notes.txt");
        fs::write(
            &source_path,
            "module tests.fmt\nworkflow main(){\naudit(\"ok\")\n}\n",
        )
        .unwrap();
        fs::write(&text_path, "do not touch\n").unwrap();

        run_fmt(FmtOptions {
            path: root.clone(),
            mode: FmtMode::Write,
        })
        .unwrap();

        let formatted = fs::read_to_string(&source_path).unwrap();
        assert!(formatted.contains("workflow main() {\n    audit(\"ok\")\n}"));
        assert_eq!(fs::read_to_string(&text_path).unwrap(), "do not touch\n");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn fmt_write_fails_on_parse_error() {
        let root = temp_cli_dir("fmt_parse_error");
        let path = root.join("bad.num");
        fs::write(&path, "module tests.bad\n\nworkflow main( {\n").unwrap();

        let err = run_fmt(FmtOptions {
            path: root.clone(),
            mode: FmtMode::Write,
        })
        .unwrap_err();

        assert!(err.contains("num fmt failed"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn route_options_parse_service_and_request_context() {
        let options = parse_route_options(
            [
                "BillingApi".to_string(),
                "--tenant".to_string(),
                "tenant_a".to_string(),
                "--actor".to_string(),
                "agent@example.com".to_string(),
                "--request-id".to_string(),
                "req_42".to_string(),
                "--correlation-id".to_string(),
                "corr_42".to_string(),
            ]
            .into_iter(),
        )
        .unwrap();

        assert_eq!(options.service_name, Some("BillingApi".to_string()));
        assert_eq!(options.tenant, Some("tenant_a".to_string()));
        assert_eq!(options.actor, Some("agent@example.com".to_string()));
        assert_eq!(options.request_id, Some("req_42".to_string()));
        assert_eq!(options.correlation_id, Some("corr_42".to_string()));
    }

    #[test]
    fn route_options_allow_flags_without_service() {
        let options =
            parse_route_options(["--tenant".to_string(), "tenant_a".to_string()].into_iter())
                .unwrap();

        assert_eq!(options.service_name, None);
        assert_eq!(options.tenant, Some("tenant_a".to_string()));
    }

    #[test]
    fn route_options_reject_unknown_or_duplicate_positionals() {
        assert!(parse_route_options(["--unknown".to_string()].into_iter()).is_err());
        assert!(parse_route_options(
            ["BillingApi".to_string(), "OtherApi".to_string()].into_iter()
        )
        .is_err());
    }

    #[test]
    fn run_json_includes_structured_connector_error() {
        let error = num_runtime::RuntimeError::ConnectorFailed {
            method: "payments.find".to_string(),
            code: "timeout".to_string(),
            message: "deadline exceeded".to_string(),
            retryable: true,
        };
        let result = Err(error.message());
        let payload = workflow_run_json("main", &result, Some(&error), &[]);

        assert_eq!(payload["status"], "failed");
        assert_eq!(payload["runtime_error"]["kind"], "connector_failed");
        assert_eq!(
            payload["runtime_error"]["connector"]["method"],
            "payments.find"
        );
        assert_eq!(payload["runtime_error"]["connector"]["retryable"], true);
    }

    #[test]
    fn version_json_includes_all_schema_versions() {
        let payload = version_json();

        assert_eq!(payload["cli"], env!("CARGO_PKG_VERSION"));
        assert_eq!(payload["language"], compatibility::CURRENT_LANGUAGE_VERSION);
        assert_eq!(
            payload["manifest_schema"],
            compatibility::CURRENT_MANIFEST_SCHEMA
        );
        assert_eq!(payload["lockfile_schema"], package::CURRENT_LOCKFILE_SCHEMA);
    }

    #[test]
    fn completions_support_bash_fish_and_zsh() {
        let bash = completion_script("bash").unwrap();
        let fish = completion_script("fish").unwrap();
        let zsh = completion_script("zsh").unwrap();

        assert!(bash.contains("complete -F _num num"));
        assert!(bash.contains("bash fish zsh"));
        assert!(fish.contains("complete -c num"));
        assert!(fish.contains("bash fish zsh"));
        assert!(zsh.contains("#compdef num"));
        assert!(zsh.contains("_values 'shell' bash fish zsh"));
    }

    #[test]
    fn completions_reject_unknown_shells_clearly() {
        let err = completion_script("powershell").unwrap_err();

        assert!(err.contains("unsupported shell `powershell`"));
        assert!(err.contains("bash"));
        assert!(err.contains("fish"));
        assert!(err.contains("zsh"));
    }

    #[test]
    fn release_plan_options_parse_path_and_json_flag() {
        let (path, format_json) = parse_release_plan_options(
            ["CHANGELOG.md".to_string(), "--json".to_string()].into_iter(),
        )
        .unwrap();

        assert_eq!(path, PathBuf::from("CHANGELOG.md"));
        assert!(format_json);
    }

    #[test]
    fn deploy_options_parse_out_and_json_flag() {
        let options = parse_deploy_options(
            [
                "--out".to_string(),
                "dist/deploy.json".to_string(),
                "--apply".to_string(),
                "--dir".to_string(),
                "dist/bundle".to_string(),
                "--replace".to_string(),
                "--json".to_string(),
            ]
            .into_iter(),
        )
        .unwrap();

        assert_eq!(options.out_path, Some(PathBuf::from("dist/deploy.json")));
        assert_eq!(options.artifact_dir, Some(PathBuf::from("dist/bundle")));
        assert_eq!(options.kubernetes_out_path, None);
        assert!(!options.kubernetes_dry_run);
        assert!(!options.check);
        assert!(options.apply);
        assert!(options.replace);
        assert!(options.format_json);
    }

    #[test]
    fn deploy_options_parse_check_flag() {
        let options =
            parse_deploy_options(["--check".to_string(), "--json".to_string()].into_iter())
                .unwrap();

        assert!(options.check);
        assert!(options.format_json);
        assert!(!options.apply);
    }

    #[test]
    fn deploy_options_reject_check_with_apply() {
        let result =
            parse_deploy_options(["--check".to_string(), "--apply".to_string()].into_iter());
        let err = match result {
            Ok(_) => panic!("expected --check with --apply to fail"),
            Err(err) => err,
        };

        assert!(err.contains("--check cannot be combined with --apply"));
    }

    #[test]
    fn deploy_options_reject_check_with_kubernetes_dry_run() {
        let result = parse_deploy_options(
            ["--check".to_string(), "--kubernetes-dry-run".to_string()].into_iter(),
        );
        let err = match result {
            Ok(_) => panic!("expected --check with --kubernetes-dry-run to fail"),
            Err(err) => err,
        };

        assert!(err.contains("--check cannot be combined with --kubernetes-dry-run"));
    }

    #[test]
    fn deploy_options_parse_kubernetes_dry_run_flags() {
        let options = parse_deploy_options(
            [
                "--kubernetes-dry-run".to_string(),
                "--kubernetes-out".to_string(),
                "dist/kubernetes.yaml".to_string(),
                "--json".to_string(),
            ]
            .into_iter(),
        )
        .unwrap();

        assert_eq!(
            options.kubernetes_out_path,
            Some(PathBuf::from("dist/kubernetes.yaml"))
        );
        assert!(options.kubernetes_dry_run);
        assert!(options.format_json);
        assert!(!options.check);
        assert!(!options.apply);
    }

    #[test]
    fn deploy_options_reject_kubernetes_out_without_dry_run() {
        let result = parse_deploy_options(
            [
                "--kubernetes-out".to_string(),
                "dist/kubernetes.yaml".to_string(),
            ]
            .into_iter(),
        );
        let err = match result {
            Ok(_) => panic!("expected --kubernetes-out without --kubernetes-dry-run to fail"),
            Err(err) => err,
        };

        assert!(err.contains("--kubernetes-out requires --kubernetes-dry-run"));
    }

    #[test]
    fn lock_options_parse_path_and_check_flag() {
        let options = parse_lock_options(
            [
                "examples/refund_workflow".to_string(),
                "--check".to_string(),
            ]
            .into_iter(),
        )
        .unwrap();

        assert_eq!(options.path, PathBuf::from("examples/refund_workflow"));
        assert!(options.check);
        assert!(!options.migrate);
    }

    #[test]
    fn lock_options_default_to_current_dir_when_only_flags_are_passed() {
        let options = parse_lock_options(["--check".to_string()].into_iter()).unwrap();

        assert_eq!(options.path, PathBuf::from("."));
        assert!(options.check);
    }

    #[test]
    fn lock_options_parse_migration_flags() {
        let options = parse_lock_options(
            [
                "examples/refund_workflow".to_string(),
                "--migrate".to_string(),
                "--write".to_string(),
                "--json".to_string(),
            ]
            .into_iter(),
        )
        .unwrap();

        assert_eq!(options.path, PathBuf::from("examples/refund_workflow"));
        assert!(options.migrate);
        assert!(options.write);
        assert!(options.format_json);
    }

    #[test]
    fn lock_options_reject_conflicting_flags() {
        assert!(
            parse_lock_options(["--check".to_string(), "--migrate".to_string()].into_iter())
                .is_err()
        );
        assert!(parse_lock_options(["--write".to_string()].into_iter()).is_err());
        assert!(parse_lock_options(["--json".to_string()].into_iter()).is_err());
    }

    #[test]
    fn migrate_args_parse_path_write_and_json_flags() {
        let (path, options) = parse_migrate_args(
            [
                "--write".to_string(),
                "examples/refund_workflow".to_string(),
                "--json".to_string(),
            ]
            .into_iter(),
        )
        .unwrap();

        assert_eq!(path, PathBuf::from("examples/refund_workflow"));
        assert!(options.write);
        assert!(options.format_json);
        assert!(!options.source);
    }

    #[test]
    fn migrate_args_default_to_current_dir_when_only_flags_are_passed() {
        let (path, options) = parse_migrate_args(["--json".to_string()].into_iter()).unwrap();

        assert_eq!(path, PathBuf::from("."));
        assert!(options.format_json);
    }

    #[test]
    fn migrate_args_parse_source_flag() {
        let (path, options) = parse_migrate_args(
            [
                "examples/refund_workflow".to_string(),
                "--source".to_string(),
                "--json".to_string(),
            ]
            .into_iter(),
        )
        .unwrap();

        assert_eq!(path, PathBuf::from("examples/refund_workflow"));
        assert!(options.source);
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

        let executor = connector_executor_for_path(&root, true).unwrap();
        let result = executor.call("echo.bool", &[]).unwrap().unwrap();

        assert_eq!(result, Value::Bool(true));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn connector_executor_uses_manifest_javascript_module() {
        let root = temp_cli_dir("javascript_module");
        fs::create_dir_all(root.join("interop")).unwrap();
        fs::write(
            root.join("num.toml"),
            r#"
[project]
name = "app"
version = "0.1.0"

[javascript]
"profile.enrich" = { module = "interop/profile.cjs", export = "enrich", timeout_ms = "5000" }
"#,
        )
        .unwrap();
        fs::write(
            root.join("interop/profile.cjs"),
            r#"
exports.enrich = ({ args }) => ({
  "$type": "Profile",
  "name": args[0],
  "source": "javascript"
});
"#,
        )
        .unwrap();

        let executor = connector_executor_for_path(&root, true).unwrap();
        let result = executor
            .call("profile.enrich", &[Value::String("Aidar".to_string())])
            .unwrap()
            .unwrap();

        let Value::Struct(name, fields) = result else {
            panic!("expected struct result");
        };
        assert_eq!(name, "Profile");
        assert_eq!(
            fields.get("name"),
            Some(&Value::String("Aidar".to_string()))
        );
        assert_eq!(
            fields.get("source"),
            Some(&Value::String("javascript".to_string()))
        );
        fs::remove_dir_all(root).unwrap();
    }
}
