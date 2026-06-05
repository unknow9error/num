use num_runtime::{
    engine::{WorkflowEngine, WorkflowStart},
    events::{FileWorkflowEventQueue, WorkflowEvent, WorkflowEventKind, WorkflowEventQueue},
    storage::{FileAuditSink, FileStateStore},
    worker::{WorkflowLeasedDrainOptions, WorkflowWorker},
    SecurityContext,
};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub fn run(args: impl Iterator<Item = String>) -> Result<(), String> {
    let mut args = args;
    match args.next().as_deref() {
        Some("enqueue") => enqueue(args),
        Some("drain") => drain(args),
        Some(other) => Err(format!(
            "unknown workflow command `{other}`\n\nSupported workflow commands:\n  enqueue\n  drain"
        )),
        None => Err("usage: num workflow <enqueue|drain> ...".to_string()),
    }
}

fn enqueue(args: impl Iterator<Item = String>) -> Result<(), String> {
    let mut args = args;
    let state_root = args
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| "usage: num workflow enqueue <state-root> <event> ...".to_string())?;
    let kind = args
        .next()
        .ok_or_else(|| "usage: num workflow enqueue <state-root> <event> ...".to_string())?;

    let (event, format_json) = match kind.as_str() {
        "start" => parse_start_event(args)?,
        "wait" => parse_transition_event(args, WorkflowTransition::Wait)?,
        "resume" => parse_transition_event(args, WorkflowTransition::Resume)?,
        "complete" => parse_transition_event(args, WorkflowTransition::Complete)?,
        "fail" => parse_fail_event(args)?,
        "compensate" => parse_transition_event(args, WorkflowTransition::Compensate)?,
        "cancel" => parse_transition_event(args, WorkflowTransition::Cancel)?,
        other => {
            return Err(format!(
                "unknown workflow event `{other}`\n\nSupported events:\n  start\n  wait\n  resume\n  complete\n  fail\n  compensate\n  cancel"
            ))
        }
    };

    let mut queue = FileWorkflowEventQueue::new(queue_dir(&state_root));
    queue
        .enqueue(event.clone())
        .map_err(|err| format!("failed to enqueue workflow event: {err:?}"))?;

    if format_json {
        let payload = json!({
            "queued": true,
            "state_root": state_root.display().to_string(),
            "queue_dir": queue.dir().display().to_string(),
            "event_id": event.id,
            "event_kind": event_kind_label(&event.kind),
        });
        let json = serde_json::to_string_pretty(&payload)
            .map_err(|err| format!("failed to render workflow event JSON: {err}"))?;
        println!("{json}");
    } else {
        println!(
            "queued workflow event {} kind={} queue={}",
            event.id,
            event_kind_label(&event.kind),
            queue.dir().display()
        );
    }

    Ok(())
}

fn drain(args: impl Iterator<Item = String>) -> Result<(), String> {
    let mut args = args;
    let state_root = args
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| {
            "usage: num workflow drain <state-root> [--max-events N] [--worker-id ID] [--lease-ms N] [--max-attempts N]".to_string()
        })?;
    let options = parse_drain_options(args)?;

    let state_store = FileStateStore::new(&state_root);
    let audit_sink = FileAuditSink::new(state_root.join("audit/events.jsonl"));
    let engine = WorkflowEngine::new(state_store, audit_sink);
    let queue = FileWorkflowEventQueue::new(queue_dir(&state_root));
    let mut worker = WorkflowWorker::new(engine, queue);
    let report = worker
        .drain_leased(WorkflowLeasedDrainOptions {
            max_events: options.max_events,
            stop_on_error: options.stop_on_error,
            worker_id: options.worker_id,
            lease_timeout: Duration::from_millis(options.lease_timeout_ms),
            max_attempts: options.max_attempts,
        })
        .map_err(|err| format!("failed to drain workflow events: {err:?}"))?;

    if options.format_json {
        let json = serde_json::to_string_pretty(&report.to_json())
            .map_err(|err| format!("failed to render workflow worker JSON: {err}"))?;
        println!("{json}");
    } else {
        print!("{}", report.render_text());
    }

    if report.failed > 0 {
        return Err(format!(
            "workflow drain failed: {} event(s) failed",
            report.failed
        ));
    }

    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum WorkflowTransition {
    Wait,
    Resume,
    Complete,
    Compensate,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EventOptions {
    event_id: Option<String>,
    actor: String,
    tenant: String,
    permissions: BTreeSet<String>,
    correlation_id: Option<String>,
    request_id: Option<String>,
    metadata: BTreeMap<String, String>,
    format_json: bool,
}

impl Default for EventOptions {
    fn default() -> Self {
        Self {
            event_id: None,
            actor: "system".to_string(),
            tenant: "default".to_string(),
            permissions: BTreeSet::new(),
            correlation_id: None,
            request_id: None,
            metadata: BTreeMap::new(),
            format_json: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DrainCliOptions {
    max_events: Option<usize>,
    stop_on_error: bool,
    worker_id: String,
    lease_timeout_ms: u64,
    max_attempts: u32,
    format_json: bool,
}

impl Default for DrainCliOptions {
    fn default() -> Self {
        Self {
            max_events: None,
            stop_on_error: true,
            worker_id: "local-worker".to_string(),
            lease_timeout_ms: 30_000,
            max_attempts: 3,
            format_json: false,
        }
    }
}

fn parse_start_event(args: impl Iterator<Item = String>) -> Result<(WorkflowEvent, bool), String> {
    let mut args = args;
    let workflow_id = args.next().ok_or_else(|| {
        "usage: num workflow enqueue <state-root> start <workflow-id> <workflow-name>".to_string()
    })?;
    let workflow_name = args.next().ok_or_else(|| {
        "usage: num workflow enqueue <state-root> start <workflow-id> <workflow-name>".to_string()
    })?;
    let options = parse_event_options(args)?;
    let event_id = options
        .event_id
        .clone()
        .unwrap_or_else(|| format!("{workflow_id}-start"));
    let security = SecurityContext {
        actor: options.actor,
        tenant: options.tenant,
        permissions: options.permissions,
        correlation_id: options
            .correlation_id
            .unwrap_or_else(|| format!("corr_{event_id}")),
        request_id: options
            .request_id
            .unwrap_or_else(|| format!("req_{event_id}")),
    };
    let event = WorkflowEvent::start(
        event_id,
        WorkflowStart {
            id: workflow_id,
            name: workflow_name,
            security,
            metadata: options.metadata,
        },
    );
    Ok((event, options.format_json))
}

fn parse_transition_event(
    args: impl Iterator<Item = String>,
    transition: WorkflowTransition,
) -> Result<(WorkflowEvent, bool), String> {
    let mut args = args;
    let workflow_id = args.next().ok_or_else(|| {
        format!(
            "usage: num workflow enqueue <state-root> {} <workflow-id>",
            transition.cli_name()
        )
    })?;
    let options = parse_event_options(args)?;
    let event_id = options
        .event_id
        .clone()
        .unwrap_or_else(|| format!("{workflow_id}-{}", transition.cli_name()));
    let event = match transition {
        WorkflowTransition::Wait => WorkflowEvent::wait(event_id, workflow_id),
        WorkflowTransition::Resume => WorkflowEvent::resume(event_id, workflow_id),
        WorkflowTransition::Complete => WorkflowEvent::complete(event_id, workflow_id),
        WorkflowTransition::Compensate => WorkflowEvent::compensate(event_id, workflow_id),
        WorkflowTransition::Cancel => WorkflowEvent::cancel(event_id, workflow_id),
    };
    Ok((event, options.format_json))
}

fn parse_fail_event(args: impl Iterator<Item = String>) -> Result<(WorkflowEvent, bool), String> {
    let mut args = args;
    let workflow_id = args.next().ok_or_else(|| {
        "usage: num workflow enqueue <state-root> fail <workflow-id> <reason>".to_string()
    })?;
    let reason = args.next().ok_or_else(|| {
        "usage: num workflow enqueue <state-root> fail <workflow-id> <reason>".to_string()
    })?;
    let options = parse_event_options(args)?;
    let event_id = options
        .event_id
        .clone()
        .unwrap_or_else(|| format!("{workflow_id}-fail"));
    Ok((
        WorkflowEvent::fail(event_id, workflow_id, reason),
        options.format_json,
    ))
}

fn parse_event_options(args: impl Iterator<Item = String>) -> Result<EventOptions, String> {
    let mut options = EventOptions::default();
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--event-id" => {
                options.event_id = Some(
                    args.next()
                        .ok_or_else(|| "usage: --event-id <event-id>".to_string())?,
                );
            }
            "--actor" => {
                options.actor = args
                    .next()
                    .ok_or_else(|| "usage: --actor <actor>".to_string())?;
            }
            "--tenant" => {
                options.tenant = args
                    .next()
                    .ok_or_else(|| "usage: --tenant <tenant>".to_string())?;
            }
            "--permission" => {
                options.permissions.insert(
                    args.next()
                        .ok_or_else(|| "usage: --permission <permission>".to_string())?,
                );
            }
            "--correlation-id" => {
                options.correlation_id = Some(
                    args.next()
                        .ok_or_else(|| "usage: --correlation-id <id>".to_string())?,
                );
            }
            "--request-id" => {
                options.request_id = Some(
                    args.next()
                        .ok_or_else(|| "usage: --request-id <id>".to_string())?,
                );
            }
            "--metadata" => {
                let raw = args
                    .next()
                    .ok_or_else(|| "usage: --metadata <key=value>".to_string())?;
                let (key, value) = raw
                    .split_once('=')
                    .ok_or_else(|| "usage: --metadata <key=value>".to_string())?;
                if key.is_empty() {
                    return Err("metadata key cannot be empty".to_string());
                }
                options.metadata.insert(key.to_string(), value.to_string());
            }
            "--json" => options.format_json = true,
            other => return Err(format!("unexpected workflow event argument '{other}'")),
        }
    }
    Ok(options)
}

fn parse_drain_options(args: impl Iterator<Item = String>) -> Result<DrainCliOptions, String> {
    let mut options = DrainCliOptions::default();
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--max-events" => {
                let raw = args
                    .next()
                    .ok_or_else(|| "usage: --max-events <count>".to_string())?;
                options.max_events = Some(
                    raw.parse::<usize>()
                        .map_err(|_| format!("invalid --max-events value '{raw}'"))?,
                );
            }
            "--no-stop-on-error" => options.stop_on_error = false,
            "--worker-id" => {
                options.worker_id = args
                    .next()
                    .ok_or_else(|| "usage: --worker-id <id>".to_string())?;
            }
            "--lease-ms" => {
                let raw = args
                    .next()
                    .ok_or_else(|| "usage: --lease-ms <milliseconds>".to_string())?;
                options.lease_timeout_ms = raw
                    .parse::<u64>()
                    .map_err(|_| format!("invalid --lease-ms value '{raw}'"))?;
            }
            "--max-attempts" => {
                let raw = args
                    .next()
                    .ok_or_else(|| "usage: --max-attempts <count>".to_string())?;
                options.max_attempts = raw
                    .parse::<u32>()
                    .map_err(|_| format!("invalid --max-attempts value '{raw}'"))?;
                if options.max_attempts == 0 {
                    return Err("--max-attempts must be at least 1".to_string());
                }
            }
            "--json" => options.format_json = true,
            other => return Err(format!("unexpected workflow drain argument '{other}'")),
        }
    }
    Ok(options)
}

impl WorkflowTransition {
    fn cli_name(self) -> &'static str {
        match self {
            WorkflowTransition::Wait => "wait",
            WorkflowTransition::Resume => "resume",
            WorkflowTransition::Complete => "complete",
            WorkflowTransition::Compensate => "compensate",
            WorkflowTransition::Cancel => "cancel",
        }
    }
}

fn queue_dir(state_root: &Path) -> PathBuf {
    state_root.join("events")
}

fn event_kind_label(kind: &WorkflowEventKind) -> &'static str {
    match kind {
        WorkflowEventKind::Start(_) => "Start",
        WorkflowEventKind::Wait { .. } => "Wait",
        WorkflowEventKind::Resume { .. } => "Resume",
        WorkflowEventKind::Complete { .. } => "Complete",
        WorkflowEventKind::Fail { .. } => "Fail",
        WorkflowEventKind::Compensate { .. } => "Compensate",
        WorkflowEventKind::Cancel { .. } => "Cancel",
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_drain_options, parse_event_options, parse_start_event, run};
    use num_runtime::storage::FileStateStore;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn event_options_parse_actor_permissions_metadata_and_json() {
        let options = parse_event_options(
            [
                "--actor".to_string(),
                "agent@example.com".to_string(),
                "--tenant".to_string(),
                "tenant_1".to_string(),
                "--permission".to_string(),
                "IssueRefund".to_string(),
                "--metadata".to_string(),
                "source=cli".to_string(),
                "--json".to_string(),
            ]
            .into_iter(),
        )
        .unwrap();

        assert_eq!(options.actor, "agent@example.com");
        assert_eq!(options.tenant, "tenant_1");
        assert!(options.permissions.contains("IssueRefund"));
        assert_eq!(
            options.metadata.get("source").map(String::as_str),
            Some("cli")
        );
        assert!(options.format_json);
    }

    #[test]
    fn start_event_parse_builds_security_context() {
        let (event, format_json) = parse_start_event(
            [
                "wf_1".to_string(),
                "process_refund".to_string(),
                "--actor".to_string(),
                "agent@example.com".to_string(),
                "--tenant".to_string(),
                "tenant_1".to_string(),
                "--permission".to_string(),
                "IssueRefund".to_string(),
                "--json".to_string(),
            ]
            .into_iter(),
        )
        .unwrap();

        assert_eq!(event.id, "wf_1-start");
        assert!(format_json);
    }

    #[test]
    fn drain_options_parse_batch_flags() {
        let options = parse_drain_options(
            [
                "--max-events".to_string(),
                "2".to_string(),
                "--no-stop-on-error".to_string(),
                "--worker-id".to_string(),
                "worker_a".to_string(),
                "--lease-ms".to_string(),
                "1500".to_string(),
                "--max-attempts".to_string(),
                "5".to_string(),
                "--json".to_string(),
            ]
            .into_iter(),
        )
        .unwrap();

        assert_eq!(options.max_events, Some(2));
        assert!(!options.stop_on_error);
        assert_eq!(options.worker_id, "worker_a");
        assert_eq!(options.lease_timeout_ms, 1500);
        assert_eq!(options.max_attempts, 5);
        assert!(options.format_json);
    }

    #[test]
    fn workflow_cli_enqueues_and_drains_file_events() {
        let root = unique_test_dir("cli-drain");
        run([
            "enqueue",
            root.to_str().unwrap(),
            "start",
            "wf_1",
            "process_refund",
            "--actor",
            "agent@example.com",
            "--tenant",
            "tenant_1",
            "--permission",
            "IssueRefund",
        ]
        .into_iter()
        .map(str::to_string))
        .unwrap();
        run(["enqueue", root.to_str().unwrap(), "complete", "wf_1"]
            .into_iter()
            .map(str::to_string))
        .unwrap();
        run(["drain", root.to_str().unwrap(), "--max-events", "10"]
            .into_iter()
            .map(str::to_string))
        .unwrap();

        let store = FileStateStore::new(&root);
        assert_eq!(store.list_workflows().unwrap()[0].id, "wf_1");
        fs::remove_dir_all(root).unwrap();
    }

    fn unique_test_dir(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "num-workflow-cli-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
}
