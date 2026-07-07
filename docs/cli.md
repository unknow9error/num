# num CLI Reference

The `num` binary is implemented in `language/crates/num-cli`.

During development, build the CLI once and put the local binary directory on
`PATH`:

```bash
cargo build -p num
export PATH="$PWD/target/debug:$PATH"
```

After that, and after installing a release package, use:

```bash
num <command>
```

## Commands

### `check`

Parse, lower, and semantically validate a `.num` source file together with
other `.num` files in its directory, or all `.num` files under a directory.

```bash
num check examples/refund_workflow/src/main.num
```

For multi-file projects, pass a directory:

```bash
num check examples/refund_workflow/src
```

Both file and directory checks resolve `use <module.path>` declarations against
the checked files. Passing a file uses that file as the entry source and scans
its containing directory. Passing a directory prefers `main.num`, then
`src/main.num`, then the first `.num` file as the entry source for commands that
need one.

The command prints diagnostics to stderr. It exits with a non-zero status when
any error diagnostic is emitted. For projects with
`[security].policy_mode = "strict"` in `num.toml`, `num check` also runs lints
and treats warnings as blocking diagnostics. Standalone files without a
manifest keep advisory warning behavior.

### `lint`

Run project quality and security lints over a `.num` file or project.

```bash
num lint examples/refund_workflow
```

`num lint` loads the same multi-file project and path dependency graph as
`num check`, prints parser/semantic diagnostics plus lint findings, and exits
with a non-zero status when any finding is emitted. Current lints focus on
production readiness: explicit module names, high-risk action timeout/cost/
idempotency metadata, service route permissions, provenance on private data,
and explicit `secret` labels for `Secret<T>`.

### `fmt`

Parse a `.num` file and print the formatter output to stdout.

```bash
num fmt examples/refund_workflow/src/main.num
```

For editor and CI workflows, use write or check mode:

```bash
num fmt --write examples/refund_workflow/src
num fmt --check examples/refund_workflow/src
```

Default stdout mode is intended for a single file and remains backward
compatible. `--write` and `--check` accept either one `.num` file or a directory,
traverse directories in stable order, and ignore non-`.num` files. `--check`
prints each unformatted file and exits with a non-zero status when formatting
would change any source. Parse or validation diagnostics also fail write/check
mode before any formatted output is accepted.

### `ir`

Print the lowered IR for a `.num` file.

```bash
num ir examples/refund_workflow/src/main.num
```

The IR is a compact effect-oriented representation of top-level declarations.
It includes selected effects such as permissions, data policies, connector
method signatures, rollback metadata, and audit requirements. It is intended for
compiler/runtime integration work, not as a stable external format.

### `run`

Validate and execute a demo workflow through the lightweight interpreter.
The command accepts either an entry `.num` file or a source directory.

```bash
num run examples/refund_workflow/src/main.num
num run examples/refund_workflow/src
num run examples/refund_workflow --json
```

With `--json`, the command emits a structured run report containing the
selected workflow, final status, trace events, the legacy text `error`, and a
typed `runtime_error` object when the failure comes from the runtime. Connector
failures include `runtime_error.kind = "connector_failed"` and a nested
`connector` object with `method`, stable `code`, human `message`, and
`retryable`. Secret values are rendered as `<redacted>` in the legacy `error`,
structured `runtime_error`, and trace payloads. Runtime execution logs are
suppressed in JSON mode so stdout is a single machine-readable payload.

Current limitations:

- the first workflow declaration in the linked entry module is selected
  automatically;
- permissions are injected by the CLI for demo purposes;
- arguments are hardcoded for the included demo workflows;
- configured `[connectors]` process commands run before the demo connector
  fallback;
- this is not durable workflow execution.

### `test`

Validate and execute top-level `.num` test declarations through the lightweight
interpreter.

```bash
num test examples/refund_workflow
```

Supported test syntax:

```num
test "basic truth" {
    let allowed: Bool = true
    assert allowed == true
}
```

The command loads the same multi-file project graph as `num check`, applies the
same manifest policy mode, then runs each linked `test` block in a fresh runtime
scope. `assert` expressions must type-check as `Bool`; a false assertion fails
the test run. `test policy` blocks additionally support static `expect_deny`
and `expect_allow` policy expectations. `test workflow` blocks support
`expect_workflow_success workflow_name(...)` and
`expect_workflow_failure workflow_name(...)` runtime scenario expectations, plus
deterministic `mock_connector connector.method(...) => Value` responses for
declared non-AI connector methods and `expect_audit "event"` audit-trail
expectations.
`test ai` blocks support deterministic `mock_ai ai.method(...) => Value
confidence <number>` responses for declared AI connector methods returning
`Uncertain<T>`. They also support deterministic scanner fixtures:
`mock_ai_scan ai.method(...) => pass|suspicious|block [reason Text]`. `pass`
and `suspicious` record auditable scanner outcomes before the mocked AI call;
`block` fails closed and redacts sensitive reason text in runtime errors and
trace details.

### `trace`

Validate and execute a demo workflow, then print runtime trace events as JSON
after the normal demo output.

```bash
num trace examples/refund_workflow
```

Trace events include workflow start/completion/failure, service route
start/completion/failure, statement execution, function/action/connector calls,
and audit logging. This is an observability/debugging foundation, not an
interactive debugger.

Workflow and service `rate limit <count> per <duration>` metadata is enforced
with tenant/actor/subject keys. The demo runtime uses an in-memory shared store
for local execution, so `serve` requests handled by the same process share
limits even though each request runs in a fresh interpreter. The runtime also
has a file-backed store for deterministic local/shared-instance tests and
handoff experiments. Production deployments still need an external backend such
as Redis or database storage before multi-process limits are authoritative.

### `debug`

Validate and execute a workflow, then summarize trace events against scripted
breakpoints.

```bash
num debug examples/refund_workflow process_refund \
  --break action:issue_refund \
  --break connector:payments.find
num debug examples/refund_workflow --break audit:refund_issued --json
```

Supported breakpoint kinds:

- `workflow:<name>`
- `statement:<trace target>`
- `function:<name>`
- `action:<name>`
- `connector:<namespace.method>`
- `audit:<event>`

The `--json` flag emits the debug session, hits, full trace, and a
`debug_adapter` object. The adapter object is versioned as
`num.debug.adapter.v1` and maps runtime trace events to a future Debug Adapter
Protocol server shape: one workflow thread, trace-backed stack frames, scopes,
variables, configured breakpoints, and breakpoint hit events.

This is still a scripted trace-replay debugger foundation. The adapter JSON
declares `continue`, `next`, `stepIn`, `stepOut`, `pause`, and `setVariable` as
unsupported so IDE integrations can fail closed instead of pretending
interactive stepping exists.

### `cost-report`

Validate and execute a demo workflow, then summarize action costs recorded by
the runtime cost ledger.

```bash
num cost-report examples/refund_workflow
num cost-report examples/refund_workflow --json
```

The report is printed after the normal demo workflow output. It groups
successful, non-replayed action charges by currency and action name, then lists
individual cost entries. The `--json` flag emits the final report payload as
structured JSON.

`cost-report --json` includes a versioned dashboard read model under
`schema_version = "num.cost_dashboard.v1"`. The stable dashboard payload
contains `totals` by currency, action, connector, model, workflow, route, actor,
request id, correlation id, and tenant; `time_window` fields; and `raw_entries`
for drill-down. Connector, model, workflow, route, request/correlation, actor,
tenant, and timestamp values are populated only when the source runtime entry
carries that context. The current demo interpreter records action and currency
for successful action charges, so the other dimensions remain present as stable
empty aggregates until richer runtime boundaries emit them. This is a cost
dashboard foundation, not an interactive or persisted cost dashboard.

When a project manifest sets `[runtime].audit_store = "file:<events.jsonl>"`,
demo interpreter commands (`run`, `test`, `trace`, `debug`, `cost-report`, and
`route`) append report-compatible demo audit JSONL to that manifest-relative
path. `audit_store = "stdout"` keeps the previous console-only behavior.
HTTP service commands (`serve` and `serve-once`) use the same setting and record
request actor, tenant, request id, correlation id, service, method, and path
metadata from the request security context.

### `audit-report`

Summarize append-only audit JSONL events written by the runtime `FileAuditSink`.

```bash
num audit-report audit/events.jsonl
num audit-report audit/events.jsonl --json
```

The text report groups events by result, action, actor, tenant, connector,
route, and workflow when those dimensions are present, and lists failed audit
events with their redacted failure reason. The `--json` flag emits the
versioned `num.audit_dashboard.v1` read model for dashboards or external
tooling.

Stable JSON fields are `schema_version`, `total_events`, `counts.by_result`,
`counts.by_action`, `counts.by_actor`, `counts.by_tenant`,
`counts.by_connector`, `counts.by_route`, `counts.by_workflow`, `time_window`,
and `failures`. Legacy top-level `by_result`, `by_action`, `by_actor`, and
`by_tenant` maps remain present for existing tooling. Connector, route,
workflow, and timestamp dimensions are best-effort: they are populated when the
audit event carries connector metadata, service method/path or route metadata,
workflow metadata, and `timestamp_ms` or `recorded_at_unix_ms`. Failure details
include event id, action, actor, tenant, request/correlation ids, optional
connector/route/workflow dimensions, timestamp, and a redacted reason; raw audit
payloads are intentionally not included. This is an audit dashboard foundation,
not an interactive web dashboard.

### `workflow-report`

Summarize workflow state files from a runtime `FileStateStore` root or from a
project manifest with `[runtime].workflow_store = "file:<state-root>"`.

```bash
num workflow-report .num-state
num workflow-report .num-state --json
num workflow-report durable-refund --json
```

The command reads `.json` workflow state files under `<state-root>/workflows`,
then groups workflows by status, workflow name, actor, and tenant. The text
output includes the most recently updated workflows first. The `--json` flag
emits the versioned `num.workflow_dashboard.v1` read model for dashboards or
external operations tooling.

Stable JSON fields are `schema_version`, `total_workflows`,
`counts.by_status`, `counts.by_name`, `counts.by_actor`, `counts.by_tenant`,
and each workflow's `id`, `name`, `status`, `actor`, `tenant`,
`started_at_ms`, `updated_at_ms`, `pending_compensation`, `recent_failure`, and
`recent_audit`. Legacy top-level `by_status`, `by_name`, `by_actor`, and
`by_tenant` maps remain present for existing tooling. Request/correlation ids
come from persisted workflow state. `pending_compensation`, `recent_failure`,
and `recent_audit` are dashboard-oriented best-effort fields: they use workflow
status and known metadata keys such as `failure_reason`, `last_failure`,
`error`, `last_audit_event`, `last_audit_result`, and timestamp metadata when
available. This is a workflow dashboard foundation, not an interactive web
dashboard.

### `workflow`

Queue and drain durable workflow lifecycle events through the file-backed
runtime state root. The first argument can be an explicit state root or a
project path whose manifest declares `[runtime].workflow_store =
"file:<state-root>"`. In the project examples below, `durable-refund` is any
package whose `[runtime]` section sets `workflow_store = "file:.num-state"`.

```bash
num workflow enqueue .num-state start wf_1 process_refund \
  --actor agent@example.com \
  --tenant tenant_1 \
  --permission IssueRefund \
  --metadata source=cli
num workflow enqueue .num-state wait wf_1
num workflow enqueue .num-state resume wf_1
num workflow enqueue .num-state complete wf_1
num workflow drain .num-state --max-events 10
num workflow drain .num-state --worker-id worker_a --max-attempts 5
num workflow drain .num-state --json
num workflow lease-heartbeat .num-state evt_1 --worker-id worker_a
num workflow lease-heartbeat .num-state evt_1 --worker-id worker_a --json
num workflow enqueue durable-refund start wf_project process_refund --json
num workflow drain durable-refund --max-events 10 --json
```

Supported event kinds are `start`, `wait`, `resume`, `complete`, `fail`,
`compensate`, and `cancel`. `start` events require `<workflow-id>` and
`<workflow-name>`. `fail` events require `<workflow-id>` and `<reason>`.
Transition events require only `<workflow-id>`.

The state root uses:

- `<state-root>/events` for queued event files;
- `<state-root>/events/leases` for claimed event leases;
- `<state-root>/events/dead` for exhausted failed events;
- `<state-root>/workflows` for persisted workflow state;
- `<state-root>/audit/events.jsonl` for lifecycle audit events.

When a project manifest is used, `workflow_store = "file:.num-state"` resolves
relative to the package root. `audit_store = "file:audit/events.jsonl"` writes
lifecycle audit events to that manifest-relative path; `audit_store = "stdout"`
falls back to `<state-root>/audit/events.jsonl` for durable workflow workers.

`workflow drain` stops on the first failed event by default and reports
processed states plus failures. Pass `--no-stop-on-error` to continue through a
batch after failures. File-backed draining claims events with a worker lease,
acks successful events, requeues failed events until `--max-attempts`, and moves
exhausted events into the dead-letter directory. `--worker-id` controls the
lease owner label, `--lease-ms` controls stale lease recovery, and
`--max-attempts` controls retry exhaustion. Long-running workers can call
`workflow lease-heartbeat <target> <event-id> --worker-id <id>` to refresh a
claimed lease before `--lease-ms` expires; the heartbeat is accepted only from
the worker that owns the active lease. Successfully applied lifecycle events are
recorded in workflow metadata by event id, so replayed duplicate event files are
acknowledged without reapplying terminal transitions or writing duplicate
lifecycle audit events. This is a durable local/CI worker foundation with
multi-worker lease coordination, not a networked cluster scheduler.

### `route`

Validate and execute a demo service route through the lightweight interpreter.
The command accepts either an entry `.num` file or a source directory.

```bash
num route examples/refund_workflow/src/main.num POST /refunds
num route examples/refund_workflow/src POST /refunds
```

The command selects the first service by default. Pass a service name as the
optional final argument when a module declares multiple services:

```bash
num route app.num POST /refunds BillingApi
```

For tenant-aware dry-runs, pass the request context that would normally come
from HTTP headers:

```bash
num route app.num POST /refunds BillingApi \
  --tenant tenant_a \
  --actor agent@example.com \
  --request-id req_42 \
  --correlation-id corr_42
```

For auth-configured dry-runs, pass the request credential header explicitly:

```bash
num route app.num POST /billing/summary BillingApi --bearer "$JWT"
num route app.num POST /billing/summary BillingApi \
  --cookie "num_session=$SIGNED_SESSION"
```

`num route` prints the route response body. Success responses are plain `ok`;
failure responses are JSON and use the same contract as `num serve` and
`num serve-once`:

```json
{
  "error": {
    "kind": "validation",
    "code": "missing_route_input",
    "message": "Missing route input",
    "request_id": "req_demo",
    "correlation_id": "corr_demo"
  }
}
```

Current limitations:

- this is a route dry-run, not an HTTP listener;
- permissions are injected by the CLI for demo purposes;
- route input values are generated for included demo schemas;
- configured `[connectors]` process commands run before the demo connector
  fallback.

Service route failures are machine-readable. Parse and body validation failures
return `400`; route misses return `404`; permission and tenant failures return
`403`; connector failures return `502` with `error.kind = "connector"` and a
stable connector `code`; rate-limit failures return `429`; other workflow or
internal failures return `500`. Error payloads include `request_id` and
`correlation_id` when the request supplied `X-Request-Id` or
`X-Correlation-Id`, otherwise the demo defaults are used. Client-facing
connector failures use a generic message so connector stderr and secrets do not
leak into HTTP responses.

When a project manifest enables `[security].tenant_isolation = true`, `num
route` checks `--tenant` and the service runtime checks `X-Tenant` against the
service tenant before executing the route body. Cross-tenant requests return a
structured `403` tenant error and write an audit event. If the setting is absent
or `false`, demo service commands keep the previous permissive behavior.
Tenant-scoped policy rules also use the request tenant for service routes:
`num route`, `num serve`, and `num serve-once` defer route-local policy checks
until request handling, then return `403` with `error.code = "policy_denied"`
when the route body is not allowed for that tenant. Standalone `num check`
remains conservative when no runtime tenant is known.

### `serve`

Validate an entry `.num` file or source directory, bind the first service by
default, listen for HTTP requests, and execute matching service routes through
the lightweight interpreter.

```bash
num serve examples/refund_workflow/src/main.num 127.0.0.1:4000
```

Then send a request from another shell:

```bash
curl -X POST http://127.0.0.1:4000/refunds \
  -H 'Content-Type: application/json' \
  -d '{"payment_id":"pay_827361","reason":"Item damaged in transit","amount":{"minor_units":15000,"currency":"KZT"}}'
```

Pass a service name as the optional final argument when a module declares
multiple services:

```bash
num serve app.num 127.0.0.1:4000 BillingApi
```

For deterministic smoke tests, stop after a fixed number of accepted requests:

```bash
num serve app.num 127.0.0.1:4000 BillingApi --max-requests 2
```

Current limitations:

- this is a persistent demo listener, not a hardened production HTTP server;
- request bodies are read using `Content-Length`; headers are capped at 16 KiB
  and bodies at 1 MiB;
- non-empty JSON request bodies are decoded into typed route input using the
  `.num` `type` schema;
- `X-Actor`, `X-Tenant`, `X-Request-Id`, and `X-Correlation-Id` headers are
  captured into the runtime `SecurityContext`; `X-Actor` is exposed to `.num`
  code as `current_user.id`;
- when `[security].tenant_isolation = true`, `X-Tenant` must match the service
  tenant before route execution starts; cross-tenant requests return a
  structured `403` tenant error and are written to audit output;
- `X-Role` and comma-separated `X-Roles` headers are resolved against `.num`
  `role` declarations and grant the role's allowed permissions for the request;
- when `[security.jwt]` is configured, `Authorization: Bearer <jwt>` is
  verified before route execution; missing, invalid, expired, wrong-issuer, or
  wrong-audience tokens return a structured `401` auth error, and verified
  `sub`/`tenant`/`roles` claims populate `current_user` and permissions;
- when `[security.session]` is configured, the named signed session cookie is
  verified before route execution; missing, malformed, expired, or tampered
  cookies return a structured `401` auth error, and verified
  `actor`/`tenant`/`roles` claims populate `current_user` and permissions;
- permissions can also be injected by the CLI for demo purposes;
- configured `[connectors]` process commands run before the demo connector
  fallback.

### `serve-once`

Validate a `.num` file, listen for one HTTP request, and execute the matching
service route. This command is kept for quick manual checks and uses generated
demo input when the request body is empty.

```bash
num serve-once examples/refund_workflow/src/main.num 127.0.0.1:4000
```

### `new`

Create a multi-file project skeleton with `num.toml`, a source directory, and a
manifest entry file.

```bash
num new my-service
```

The command writes:

- `my-service/num.toml`
- `my-service/src/access.num`
- `my-service/src/domain.num`
- `my-service/src/connectors.num`
- `my-service/src/main.num`

The generated manifest declares `source = "src"` and `entry = "src/main.num"`,
so `num check my-service` and `num run my-service` work from the project root.

### `lock`

Generate `num.lock` next to the discovered `num.toml`.

```bash
num lock examples/refund_workflow
num lock examples/refund_workflow --check
num lock legacy_project --migrate --json
num lock legacy_project --migrate --write
```

The command records the workspace package plus sorted `[dependencies]` entries
from `num.toml`. Dependency values can be version strings or inline tables:

```toml
[dependencies]
std = "0.3.0"
shared = { path = "../shared", version = "0.3.0" }
banking = { git = "https://example.com/banking.num.git", version = "1.4.0" }
ledger = { git = "https://example.com/ledger.num.git", version = "2.1.0", rev = "abc123" }
```

The current lockfile is deterministic, versioned local metadata. Its top-level
`version = 1` schema is checked by `num lock --check` and by deployment
materialization. Future lockfile schemas are rejected instead of being silently
interpreted by an older CLI. The lockfile records the workspace package
language/schema compatibility metadata plus direct and transitive
path/local-registry dependencies that can be resolved locally. Resolved package
entries include sorted dependency edges. Local-registry package entries also
include the registry package `content_hash` from `.num-package.json` metadata
or the computed package hash when metadata has not been written yet, so
lockfiles pin the resolved package content as well as its name and version. Git
inline tables can include `rev`, `tag`, `branch`, or `ref`; `num lock` checks
out git dependencies into a project-local `.num-git` cache and pins the
resolved commit SHA in the package entry source label. Git dependency URLs are
passed to `git clone` as written and support the schemes configured in the host
Git installation, including local paths, `file://`, `https://`, and SSH URLs.
`num lock` runs git non-interactively with terminal prompts disabled, so CI and
deploys must provide credentials through existing Git credential helpers,
tokens, or SSH agents. Credentials are never written to `num.lock` or deploy
metadata. Existing `.num-git` checkouts are reused without network access only
for explicit `rev` pins already present in the cache; `tag`, `branch`, and
`ref` selectors fetch from `origin` before checkout because they can move.
Registry dependencies without a configured local registry root remain
metadata-only entries. Remote registry downloads are not implemented yet.

`num lock --migrate` plans safe lockfile schema migrations without rewriting by
default. Current migrations add a missing top-level `version = 1` header for
legacy lockfiles and upgrade schema `0` lockfiles to schema `1`. Pass
`--migrate --write` to apply the migration after reviewing the plan, or add
`--json` for machine-readable CI output.

Direct `path`, local filesystem registry, and git dependencies are loaded
during `check`, `run`, `route`, `serve`, and `serve-once`, which lets
`use <module.path>` resolve modules declared in a dependency package.

### `registry`

Manage a local filesystem package registry.

```bash
num registry publish examples/refund_workflow --registry /tmp/num-registry
num registry publish examples/refund_workflow --registry /tmp/num-registry --dry-run --json
num registry list --registry /tmp/num-registry
num registry index --registry /tmp/num-registry --json
num registry install refund-workflow 0.3.0 --registry /tmp/num-registry --to vendor/num
num registry install refund-workflow latest --registry /tmp/num-registry --to vendor/num
```

`publish` validates the package manifest, collects package source files, copies
them into `<registry-root>/<package-name>/<version>/`, and writes
`.num-package.json` package metadata with schema, language/manifest metadata,
per-file hashes, and a package content hash. It skips common build/runtime
output directories such as `.git`, `target`, `node_modules`, `.num-state`, and
`dist`. Existing package versions are protected by default; pass `--replace` to
overwrite a published local version.

`list` reads package/version directories that contain `num.toml` and sorts
versions with SemVer precedence instead of lexicographic string order. `index`
validates each package metadata file and emits a stable machine-readable package
index with name, version, language version, manifest schema, content hash,
metadata path, and file count, also in SemVer order. `install` copies a
published package from the registry into `<install-root>/<name>/<version>/`; the
special version `latest` resolves to the highest SemVer-compatible local
registry version before copying. When registry metadata exists, `install`
verifies the content hash before copying and writes the metadata into the
installed package too. The default install root is `.num/packages`. These
commands are local registry tooling for development and private package
workflows; `index` is the current API-ready metadata surface, while remote
download/publish service endpoints are still platform work.

### `connector`

Probe manifest-configured process connector bindings directly, without running
a workflow or service route.

```bash
num connector probe my-service payments.find --arg '"pay_1"'
num connector probe my-service payments.find --arg '"pay_1"' --json
```

`probe` loads the project manifest, validates the `.num` module graph, finds
the exact `[connectors]` process binding for `<connector.method>`, converts each
`--arg <json>` value into the runtime connector value model, and invokes the
configured process. It does not fall back to demo connectors, so a successful
probe proves that the real manifest binding can start, receive stdin, return
valid JSON, and pass runtime value conversion. `--json` returns either
`status = "ok"` with `result`, or `status = "error"` with the stable connector
`code`, `message`, and `retryable` fields.

### `connector-sdk`

Generate connector implementation SDKs from the checked `.num` module graph.

```bash
num connector-sdk examples/contract_driven_refund
num connector-sdk examples/contract_driven_refund \
  --language typescript \
  --out examples/contract_driven_refund/generated/connectors.d.ts
num connector-sdk examples/connector_echo_pipeline \
  --language python \
  --out examples/connector_echo_pipeline/generated/connectors.py
num connector-sdk examples/contract_driven_refund --json
```

The TypeScript generator emits:

- runtime wrapper types used by connector signatures, such as `Money`,
  `Option`, `Result`, `Uncertain`, `Secret`, and `JsonValue` when needed;
- checked `.num` struct, alias, and enum declarations visible to the entry
  module;
- a `NumConnectors` interface grouped by connector namespace, with each method
  returning a `Promise`.

The Python generator emits:

- `dataclass(frozen=True)` wrappers for egress context, structs, payload enum
  variants, and runtime wrappers such as `Money`, `Uncertain`, and `Secret`
  when needed;
- `TypeAlias` declarations for aliases, literal enums, JSON values, and simple
  built-in mappings;
- `Protocol` connector classes grouped under `NumConnectors`, with each method
  accepting an optional `NumConnectorEgressContext`;
- `num_connector_egress_context_from_json(...)` for converting the process
  connector stdin `egress` object into the generated dataclass shape.

Python mappings are intentionally conservative. `Text`, `Email`, `Uuid`,
`Date`, `DateTime`, `Decimal`, `Url`, and `PhoneNumber` map to `str`; `Int`,
`Float`, `Bool`, `Unit`, `List<T>`, `Map<K, V>`, `Option<T>`, `Result<T, E>`,
`Uncertain<T>`, `Secret<T>`, and `Json` map to standard Python typing forms or
generated wrappers. Unsupported or invalid Python identifiers fall back to
`Any` or a sanitized `field_<name>` attribute so the generated stub remains
importable while preserving the checked `.num` contract as the source of truth.

This gives backend authors a generated implementation contract for process or
host-language connector code. Manifest-configured process connectors can set a
`timeout_ms` string in `[connectors]` inline tables; runtime commands kill and
report connector processes that exceed that budget. Runtime connector failures
are classified internally with `code`, `message`, and `retryable` fields so
process, database, and missing-implementation failures share the same boundary.
`num run --json` and `num debug --json` expose connector failures in the
structured `runtime_error.connector` payload, and JSON runtime commands suppress
demo execution logs on stdout. If a `Secret<T>` or `secret`-labeled value reaches
a runtime output boundary, the CLI reports the stable `<redacted>` marker instead
of the raw value.

In v0.3.0, every runtime connector call also carries an egress context. Process
connectors receive this context in stdin under `egress`:

```json
{
  "method": "mailer.send",
  "args": ["customer@example.com"],
  "egress": {
    "connector": "mailer",
    "method_name": "send",
    "method": "mailer.send",
    "capability": "connector:mailer.send",
    "actor": "admin@company.com",
    "tenant": "default",
    "correlation_id": "corr_demo",
    "request_id": "req_demo",
    "policy_decision": "compile_time_checked",
    "arg_labels": [
      {
        "index": 0,
        "name": "email",
        "type": "Email",
        "source": "UserInput",
        "privacy": "private",
        "trust": "verified"
      }
    ]
  }
}
```

Generated TypeScript and Python SDKs expose the same shape as
`NumConnectorEgressContext` and add an optional `context` parameter to connector
methods. External workers should treat `capability`, `tenant`, `actor`,
`correlation_id`, and `arg_labels` as the audit/enforcement envelope for data
that leaves a single Num runtime instance. This is not managed connector
hosting, auth/secrets binding, or a generated network client runtime yet.

Projects can also bind a connector method directly to a local JavaScript module
under `[javascript]` in `num.toml`:

```toml
[javascript]
"profile.enrich" = { module = "interop/profile.cjs", export = "enrich", timeout_ms = "1500" }
```

Runtime commands invoke that module through Node as a narrow callable-module
bridge. The exported function receives one envelope:

```js
exports.enrich = async ({ args, context }) => {
  return { "$type": "EnrichedProfile", id: args[0], email: args[1] };
};
```

`args` uses the same JSON conversion rules as process connectors, and `context`
uses the same connector egress context shape shown above. JavaScript exceptions
are converted to structured connector errors such as `js_exception` or
`js_export_missing` without exposing raw stack traces by default. This bridge is
appropriate for small local JS/TS integration points. Prefer a declared
connector plus `num connector-sdk` for production integrations that need a
stable typed implementation contract, generated interfaces, auth/secrets, or
network-native hosting.

### `deploy`

Validate a project and build a deployment plan artifact from `num.toml` and the
compiled `.num` module graph.

```bash
num deploy examples/refund_workflow
num deploy examples/refund_workflow --json
num deploy examples/refund_workflow --check --json
num deploy examples/refund_workflow --out dist/num-deploy.json
num deploy examples/refund_workflow --apply --dir dist/refund-deploy
num deploy examples/refund_workflow --apply --replace --json
num deploy examples/refund_workflow --kubernetes-dry-run
num deploy examples/refund_workflow --kubernetes-dry-run --kubernetes-out dist/kubernetes.yaml
```

The plan includes package name/version, deployment target metadata, a checked
target profile with execution class, required artifacts, target-specific
validation status, validation errors/warnings, environment validation status
from `[environment]`, AI provider/model alias metadata from `[ai]`, runtime
store metadata, security mode, compiled module count, workflows, actions,
service routes, connectors, process connector bindings, and direct
dependencies. It also embeds the manifest language/schema compatibility
contract. Process connector bindings include method, command, args, cwd, and
timeout metadata for future deployment runners.

`--check` makes deploy validation explicit for CI jobs. It compiles the project,
applies the manifest policy mode, runs lint/security checks, validates the
target profile/environment, checks image-publish metadata, reports high-risk
actions without cost metadata, validates declared AI provider credential
environment names without reading secret values, and exits without
materializing an artifact directory. With `--json`, the command emits a
versioned `num.deploy_check.v1` read model with gate status for `compile`,
`policy`, `cost`, `security`, `target`, `environment`, `secrets`, `ai`, and
`image_publish`. Blocking gates return non-zero; advisory warnings remain
visible in JSON while keeping a zero exit status.
`--check` can be combined with `--json` and `--out`, but not with `--apply` or
`--kubernetes-dry-run`.

Target validation records required and recommended `[deployment]` fields for
the selected target. `container` targets recommend `service`; `kubernetes`/`k8s`
and `cloud`/`aws`/`gcp`/`azure` targets require both `service` and `region`.
`serverless`/`function` targets require `service`, recommend `region` as a
provider handoff label, and record that AWS Lambda, Cloudflare Workers, Vercel,
Netlify, provider event adapters, binary packaging, and upload execution remain
external. `edge`/`edge-runtime`/`edge-worker`/`worker` targets require
`service`, recommend `region` as an edge placement/provider label, block
file-backed runtime stores and local process connectors, and record that
Cloudflare Workers, Vercel Edge, Netlify Edge, Deno Deploy, provider bindings,
durable state, bundling, and rollout execution remain external.
`bare-metal`/`systemd` targets require `service`, recommend `region` as a host
inventory label, and record that host provisioning remains an external operator
step. Custom targets stay valid as custom handoff plans, but their profile
records the explicit external runner boundary.

With `--apply`, the command materializes a reproducible local/CI deployment
bundle. The bundle includes:

- `num-deploy.json` - checked deployment plan;
- `num.toml` - source package manifest;
- `num.lock` - validated package lockfile, when present;
- `modules/` - source module snapshot used for compilation;
- the manifest `[project].source` tree snapshot, so the artifact can be run by
  the `num` CLI without depending on the original workspace checkout;
- `manifest.json` - artifact metadata, target profile validation, environment
  status, and module map;
- `RUNBOOK.md` - operations boundary, environment status, and handoff notes.

External deployment bundles also include `deploy/github-actions.yml`,
`deploy/Jenkinsfile`, and `deploy/.gitlab-ci.yml`. The generated GitHub Actions
template uses `Policy gate` (`num check`, `num test`), `Cost gate`
(`num cost-report --json`), `Security gate` (`num lint`), `Deploy check`
(`num deploy --check --json`), and `Package deploy artifact`
(`num deploy --apply --replace --dir "$NUM_DEPLOY_DIR" --json`) before uploading
the cost report, deploy-check JSON, deploy plan JSON, and materialized bundle as
workflow artifacts. It is a copyable example for `.github/workflows/` and
expects the `num` CLI to be on `PATH`.

The generated Jenkins pipeline checks out the repository, runs deploy gates in
the fixed order `Policy gate` (`num check`, `num test`), `Cost gate`
(`num cost-report --json`), `Security gate` (`num lint`), then materializes the
deploy artifact with `num deploy --apply --replace --dir "$NUM_DEPLOY_DIR"
--json`. Jenkins must run with the `num` CLI on `PATH` and provide repository
checkout access. The template exposes `NUM_PROJECT_DIR` and `NUM_DEPLOY_DIR`
parameters; when the manifest uses `[deployment].credentials_ref`, map that
reference to a Jenkins credential id through `NUM_REGISTRY_CREDENTIALS_ID` or an
external secret store. Credential values are not written to the Jenkinsfile or
deployment bundle.

The generated GitLab template uses stages `policy`, `cost`, `security`, and
`package` in that order. It runs `num check`, `num test`, `num cost-report
--json`, `num lint`, `num deploy --check --json`, and finally
`num deploy --apply --replace --dir "$NUM_DEPLOY_DIR" --json`. Cache paths are
explicit (`.num-cache/` and `dist/num-deploy/`), and artifact paths include the
cost report, deploy-check plan, deploy packaging JSON, and materialized bundle.
GitHub runner provisioning, GitLab runner provisioning, masked CI/CD variables,
registry login, and external secret-store resolution remain outside the
generated templates.

For `[deployment].target = "container"` or compatible targets such as `docker`
and `oci`, the bundle also includes `deploy/Dockerfile` and
`deploy/compose.yaml`. The Dockerfile builds from the artifact root and starts
`num serve . 0.0.0.0:4000 <service>` when `[deployment].service` is set, or
`num run . --json` for workflow-only artifacts. For `kubernetes`/`k8s` targets,
the bundle includes `deploy/Dockerfile` and `deploy/kubernetes.yaml` with a
deployment/service scaffold. `--kubernetes-dry-run` prints the same planned
Kubernetes deployment/service resources without materializing a full bundle, and
`--kubernetes-out <resources.yaml>` writes only those resources. With `--json`,
the dry-run output includes the plan, generated manifest text, namespace/image/
port validation, and secret-like environment references that need Kubernetes
Secret mappings before a real apply exists. For `bare-metal`, `baremetal`,
`systemd`, or `host` targets, the bundle includes `deploy/num.service` and
`deploy/num.env`: the service unit is a systemd-style draft, and the environment
file template lists `NUM_DEPLOY_PLAN`, runtime store expectations, and
required/optional manifest environment variables without secret values.
For `serverless`, `function`, or `functions` targets, the bundle includes
`deploy/serverless/handler.mjs`, `deploy/serverless/manifest.json`, and
`deploy/serverless/env.example`. The handler is a provider-neutral Node scaffold
that adapts a basic HTTP-like event into `num route <project> <METHOD> <PATH>
<service>`; the manifest records service/runtime metadata, connector command
placeholders, unsupported providers, and environment requirements; the env
template lists variable names only. It is a handoff artifact, not an AWS Lambda,
Cloudflare Workers, Vercel, Netlify, or provider upload implementation.
For `edge`, `edge-runtime`, `edge-worker`, `worker`, or `workers` targets, the
bundle includes `deploy/edge/worker.mjs`, `deploy/edge/manifest.json`, and
`deploy/edge/env.example`. The worker is a provider-neutral Fetch scaffold that
does not use filesystem access, `child_process`, or local Num CLI execution; it
returns an explicit adapter-required response until a provider-specific
edge-compatible runtime is bound. The manifest records service/runtime
metadata, edge limitations, unsupported providers, and environment binding
names. File-backed workflow/audit stores and local process connectors are
blocking target-validation errors because edge providers cannot run those local
runtime dependencies.

When `[deployment]` includes image publish fields such as `registry`, `image`,
`tag_strategy`, or `credentials_ref`, container and Kubernetes bundles add
`deploy/image-publish.json` and the deployment plan exposes `image_publish`.
The handoff records the exact publish reference, for example
`ghcr.io/acme/billing-api:1.2.3`, the tag strategy, validation status, and the
registry `credentials_ref`. The generated Compose/Kubernetes scaffolds use that
same image reference. Credential values are never read from or written to
`num.toml`; CI or operators must resolve `credentials_ref` through their secret
store and run `docker login`, build, tag, and push outside `num deploy`.

The default bundle directory is derived from `[deployment].artifact` by removing
the file extension. Use `--dir <artifact-dir>` to choose a different output
directory. Existing bundle directories are protected by default; pass
`--replace` to overwrite them. This is deployment artifact materialization plus
runtime scaffolding; image publishing execution, cluster credentials, SSH
access, host package installation, Jenkins controller/agent provisioning,
GitHub runner provisioning, GitLab runner provisioning, `systemctl` execution,
Kubernetes `kubectl apply` or API-server mutation, and cloud rollout execution
remain external deployment steps. Serverless provider adapters, cold-start
tuning, runtime binary packaging, and provider upload execution also remain
external deployment steps.

### `compat`

Validate manifest language/schema compatibility for a project and any loaded
path or local-registry dependencies.

```bash
num compat examples/refund_workflow
num compat examples/refund_workflow --json
```

The command checks `[language].version`, `[language].compatibility`, and
`[language].manifest_schema` against the current CLI. Project commands use the
same validation before compiling source, so a package authored for a future
language/schema version fails early instead of being interpreted as an older
project. With `--json`, the command prints one structured report per package
even when a package is incompatible; incompatible reports include
`"status": "incompatible"` and a `reason`, and the command still exits non-zero
for CI gates. The CLI compatibility matrix tests cover current manifests, exact
compatibility, legacy missing-language manifests, schema `0` migration, future
schema rejection, future language rejection, structured incompatible reports,
and project-version upgrade compatibility.

### `migrate`

Plan or apply safe `num.toml` manifest migrations.

```bash
num migrate examples/refund_workflow
num migrate examples/refund_workflow --json
num migrate legacy_project --write
num migrate examples/refund_workflow --source --json
num migrate legacy_project --source --write
```

The command is a dry-run by default. It discovers `num.toml` from a project
directory or file path, reports the changes needed for the current CLI, and only
writes when `--write` is passed. The current migration path inserts missing
`[language]` metadata, fills partial `[language]` sections, and upgrades
`manifest_schema = 0` to the current manifest schema. Manifests that require a
future schema are rejected instead of rewritten.

`--source` switches from manifest migration to source migration. It discovers
workspace `.num` source files, runs the compiler checks, reports blocking
diagnostics, and lists per-file source migration actions. Source rewrites insert
deterministic explicit `module` declarations into legacy files that omit them,
deriving the module path from the manifest source-relative file path, and
normalize legacy workflow/service `rate_limit` metadata spelling to
`rate limit`. `--source --write` applies source rewrites only when the current
source graph has no blocking compiler diagnostics. See
[migration-guides.md](migration-guides.md) for released migration behavior.

### `upgrade-version`

Plan or apply safe `num.toml` version upgrades.

```bash
num upgrade-version examples/refund_workflow
num upgrade-version examples/refund_workflow --json
num upgrade-version examples/refund_workflow --project 0.3.0 --write
num upgrade-version legacy_project --language 0.3.0 --write
num upgrade-version examples/refund_workflow --include-dependencies --json
num upgrade-version examples/refund_workflow --include-dependencies --write --write-dependencies
```

The command updates `[language].version` to the current CLI language version by
default, fills missing `[language]` metadata when needed, and can also bump
`[project].version` when `--project <x.y.z>` is passed. It refuses downgrades
for both language and project versions. Like `migrate`, this is a dry-run unless
`--write` is passed.

`--include-dependencies` expands the report across the resolved path/local
registry dependency graph, so dependency manifests that need language metadata
or version upgrades are visible before they break project compatibility.
`--write` still applies only the workspace manifest. Pass
`--write-dependencies` together with `--write` to apply the language upgrade to
resolved dependency manifests too. Project version bumps remain scoped to the
workspace package, even in dependency graph mode.

### `bench`

Measure lexer, parser, and checker cost for checked-in benchmark fixture
projects.

```bash
num bench
num bench --json
num bench --iterations 10 --json
num bench --compare bench-baseline.json --json
num bench language/crates/num-cli/tests/fixtures/benchmarks/medium --json
```

Without an explicit path, the command uses the benchmark fixtures bundled with
the CLI crate. A path may point at one fixture project, one `.num` file, or a
directory containing fixture projects.

The JSON output is intended for CI artifacts and baseline files. It includes a stable
`schema_version`, the iteration count, fixture names, input sizes, diagnostic
counts, and median `lex_nanos`, `parse_nanos`, and `check_nanos` timings. These
numbers are observability data only unless `--compare` is enabled.

`--compare <baseline.json>` reads a prior `num bench --json` payload and adds a
`comparison` block to JSON output, or a comparison table to text output. The
command exits non-zero only when a parse or check timing exceeds both the
percentage threshold and the absolute nanosecond threshold. This keeps tiny
timing noise from breaking CI while still catching meaningful regressions.

Supported threshold flags:

```bash
num bench \
  --compare bench-baseline.json \
  --max-parse-regression-pct 25 \
  --max-check-regression-pct 25 \
  --max-parse-regression-nanos 5000000 \
  --max-check-regression-nanos 5000000 \
  --json
```

The default thresholds are 25% and 5,000,000ns for parse and check phases. Lex
timings remain informational because the regression gate is scoped to parser and
checker cost.

A CI job can keep the baseline as an uploaded artifact and opt into failure only
for meaningful regressions:

```yaml
- name: Benchmark
  run: num bench --iterations 10 --json > bench-current.json

- name: Compare benchmark baseline
  run: |
    if [ -f bench-baseline.json ]; then
      num bench --iterations 10 --compare bench-baseline.json --json > bench-comparison.json
    fi

- uses: actions/upload-artifact@v4
  with:
    name: num-benchmarks
    path: |
      bench-current.json
      bench-comparison.json
```

### `version`

Print the CLI, language, manifest schema, and lockfile schema versions.

```bash
num version
num version --json
num --version
```

The JSON form is intended for CI and release tooling; it includes `cli`,
`language`, `manifest_schema`, and `lockfile_schema`.

### `release-plan`

Compute the next SemVer release bump from `CHANGELOG.md`.

```bash
num release-plan
num release-plan --json
num release-plan path/to/CHANGELOG.md --json
```

The command reads the `## Unreleased` section and reports the highest current
bump from SemVer headings: `Major`, `Minor`, or `Patch`. A clean post-release
`Unreleased` section with no entries reports `bump: none` and keeps the next
version equal to the current CLI package version. Non-empty unreleased entries
must still live under a SemVer heading. Use it in every PR that changes
user-visible behavior so the changelog and SemVer impact stay aligned before
merge.

### `import openapi`

Generate `.num` type and connector declarations from an OpenAPI JSON or YAML
document.

```bash
num import openapi openapi.json generated.billing > src/billing_api.num
num import openapi openapi.yaml generated.billing > src/billing_api.num
```

The importer currently handles a focused OpenAPI 3 JSON/YAML subset:

- `components.schemas` object schemas become `type` declarations;
- `paths` operations become connector methods;
- `operationId` becomes the method name when present;
- JSON request bodies become a `body` parameter;
- JSON success responses become method result types;
- `components.securitySchemes` and effective operation `security` requirements
  are preserved as generated comments for `apiKey`, HTTP, OAuth2, and
  unsupported security shapes;
- operations emit review-required candidate `permission` declarations inferred
  from `operationId`, tags, method/path fallbacks, and security metadata;
- generated comments include review-required policy placeholders for auth
  requirements and obvious private/user-input field names such as email, token,
  tenant, customer, session, or password. These placeholders do not grant access
  or claim correctness; review and edit them before wiring roles, routes, or
  policy allow rules;
- simple pagination conventions are preserved as review-required connector
  comments when the importer sees `limit`/`offset`, `page` plus `pageSize`-like
  parameters, cursor-like parameters, or JSON response fields such as `next`,
  `next_cursor`, `nextPageToken`, `next_url`, or `hasNextPage`;
- operation callbacks and response links are preserved as generated comments
  that name the unsupported metadata and source operation;
- simple `allOf` object schemas merge their representable properties
  deterministically, including local component `$ref` members. Conflicting
  merged fields fall back to `Json` with a review comment;
- component-level `oneOf` schemas become deterministic union aliases when every
  variant is a local `$ref` to a representable object schema. Mixed primitive,
  inline, unresolved, or non-object variants fall back to `Json` with a review
  comment;
- scalar schemas map to `Text`, `Int`, `Float`, `Bool`, `Json`, `List<T>`, and
  nullable fields map to `Option<T>`.

Executable authentication bindings, `oneOf` beyond named object union aliases,
broad composition beyond simple object `allOf` merges, executable paginated
clients, executable callbacks/links, generated runtime clients, and
automatically correct production policies are not implemented yet. Pagination
comments are review metadata only: edit generated connector wrappers, policies,
and runtime code before relying on them in production. Unsupported security
schemes are emitted as comments rather than silently dropped.

### `import sql`

Generate `.num` table types and database connector declarations from a SQL
schema file.

```bash
num import sql schema.sql generated.db > src/database.num
num import sql --plan old.sql new.sql
num import sql --plan old.sql new.sql --json
```

The importer currently handles a focused SQL subset:

- `CREATE TABLE` statements;
- column declarations with common SQL scalar types;
- nullable columns as `Option<T>`;
- inline `PRIMARY KEY` columns;
- single-column and composite table-level `PRIMARY KEY (...)` constraints,
  including named constraints;
- inline `REFERENCES` columns and table-level `FOREIGN KEY (...) REFERENCES`
  constraints as generated relation hint comments;
- `CREATE INDEX` and `CREATE UNIQUE INDEX` statements with simple column lists
  as generated table metadata comments;
- generated `database` connector methods: `list_<table>`,
  `find_<table>_by_<primary_key>`, composite
  `find_<table>_by_<key1>_and_<key2>`, and `insert_<table>`.

The runtime crate includes an in-memory executor for these generated
`database.*` method names. It is intended for contract tests and demos, not for
production database access.

`num import sql --plan <old.sql> <new.sql>` compares two schema snapshots using
the same focused SQL subset and prints a deterministic review report. The text
form includes additive, breaking, and review counts; `--json` emits
`schema_version = "num.sql_migration_plan.v1"` with a stable `changes` array.
The first slice detects added/removed tables, added/removed/changed columns, and
primary-key changes where the importer can represent the table.

Composite finder method names preserve SQL primary-key column order and join
identifier-normalized column names with `_and_`. Index metadata comments are not
query-planner hints. Expression indexes, partial indexes, operator classes,
included columns, executable relation loading, migrations, SQL dialect-specific
features such as `ALTER TABLE ... ADD CONSTRAINT`, and generated runtime clients
are not implemented yet. Migration-plan reports are planning output only; they
do not generate or execute database migration SQL. Unsupported foreign-key and
index forms are documented as outside the current import subset rather than
represented as runnable relations.

### `completions`

Print shell completion scripts.

```bash
num completions bash
num completions fish
num completions zsh
```

Supported shells are bash, fish, and zsh. The generated scripts complete the
top-level command set, nested command groups such as `registry`, `workflow`,
`connector`, `import`, and `completions`, and common file arguments such as
`.num` sources and audit `.jsonl` reports.

### `lsp`

Start the language server process used by the VS Code extension.

```bash
num lsp
```

The LSP server reads JSON-RPC messages from stdin/stdout and is normally
launched by the editor extension. Diagnostics resolve sibling `.num` modules
and open editor buffers for the current source directory.

## Release Packaging

Build a release archive for the current platform:

```bash
bash scripts/package-current-platform.sh
```

The package includes:

- `bin/num` or `bin/num.exe`;
- the VS Code extension `.vsix`;
- `install.sh`;
- `install.ps1`;
- release README.

GitHub Actions builds packages for Linux x64, macOS x64, macOS arm64, and
Windows x64 when a `v*` tag is pushed or the workflow is run manually.
