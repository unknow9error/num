# num CLI Reference

The `num` binary is implemented in `language/crates/num-cli`.

During development, run commands through Cargo from the repository root:

```bash
cargo run -p num -- <command>
```

After installing a release package, use:

```bash
num <command>
```

## Commands

### `check`

Parse, lower, and semantically validate a `.num` source file together with
other `.num` files in its directory, or all `.num` files under a directory.

```bash
cargo run -p num -- check examples/refund_workflow/src/main.num
```

For multi-file projects, pass a directory:

```bash
cargo run -p num -- check examples/refund_workflow/src
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
cargo run -p num -- lint examples/refund_workflow
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
cargo run -p num -- fmt examples/refund_workflow/src/main.num
```

The formatter is stdout-only in v0.1.0. Redirect output manually if needed.

### `ir`

Print the lowered IR for a `.num` file.

```bash
cargo run -p num -- ir examples/refund_workflow/src/main.num
```

The IR is a compact effect-oriented representation of top-level declarations.
It includes selected effects such as permissions, data policies, connector
method signatures, rollback metadata, and audit requirements. It is intended for
compiler/runtime integration work, not as a stable external format.

### `run`

Validate and execute a demo workflow through the lightweight interpreter.
The command accepts either an entry `.num` file or a source directory.

```bash
cargo run -p num -- run examples/refund_workflow/src/main.num
cargo run -p num -- run examples/refund_workflow/src
```

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
cargo run -p num -- test examples/refund_workflow
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
`Uncertain<T>`.

### `trace`

Validate and execute a demo workflow, then print runtime trace events as JSON
after the normal demo output.

```bash
cargo run -p num -- trace examples/refund_workflow
```

Trace events include workflow start/completion/failure, service route
start/completion/failure, statement execution, function/action/connector calls,
and audit logging. This is an observability/debugging foundation, not an
interactive debugger.

### `debug`

Validate and execute a workflow, then summarize trace events against scripted
breakpoints.

```bash
cargo run -p num -- debug examples/refund_workflow process_refund \
  --break action:issue_refund \
  --break connector:payments.find
cargo run -p num -- debug examples/refund_workflow --break audit:refund_issued --json
```

Supported breakpoint kinds:

- `workflow:<name>`
- `statement:<trace target>`
- `function:<name>`
- `action:<name>`
- `connector:<namespace.method>`
- `audit:<event>`

The `--json` flag emits the debug session, hits, and full trace as structured
JSON. This is a scripted CLI debugger foundation; step/continue interaction and
IDE debug adapter integration are not implemented yet.

### `cost-report`

Validate and execute a demo workflow, then summarize action costs recorded by
the runtime cost ledger.

```bash
cargo run -p num -- cost-report examples/refund_workflow
cargo run -p num -- cost-report examples/refund_workflow --json
```

The report is printed after the normal demo workflow output. It groups
successful, non-replayed action charges by currency and action name, then lists
individual cost entries. The `--json` flag emits the final report payload as
structured JSON. This is a cost dashboard foundation, not an interactive or
persisted cost dashboard.

### `audit-report`

Summarize append-only audit JSONL events written by the runtime `FileAuditSink`.

```bash
cargo run -p num -- audit-report audit/events.jsonl
cargo run -p num -- audit-report audit/events.jsonl --json
```

The text report groups events by result, action, actor, and tenant, and lists
failed audit events with their failure reason. The `--json` flag emits the same
summary as structured JSON for dashboards or external tooling. This is an audit
dashboard foundation, not an interactive web dashboard.

### `workflow-report`

Summarize workflow state files from a runtime `FileStateStore` root.

```bash
cargo run -p num -- workflow-report .num-state
cargo run -p num -- workflow-report .num-state --json
```

The command reads `.json` workflow state files under `<state-root>/workflows`,
then groups workflows by status, workflow name, actor, and tenant. The text
output includes the most recently updated workflows first. The `--json` flag
emits the same read model as structured JSON for dashboards or external
operations tooling. This is a workflow dashboard foundation, not an interactive
web dashboard.

### `workflow`

Queue and drain durable workflow lifecycle events through the file-backed
runtime state root.

```bash
cargo run -p num -- workflow enqueue .num-state start wf_1 process_refund \
  --actor agent@example.com \
  --tenant tenant_1 \
  --permission IssueRefund \
  --metadata source=cli
cargo run -p num -- workflow enqueue .num-state wait wf_1
cargo run -p num -- workflow enqueue .num-state resume wf_1
cargo run -p num -- workflow enqueue .num-state complete wf_1
cargo run -p num -- workflow drain .num-state --max-events 10
cargo run -p num -- workflow drain .num-state --worker-id worker_a --max-attempts 5
cargo run -p num -- workflow drain .num-state --json
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

`workflow drain` stops on the first failed event by default and reports
processed states plus failures. Pass `--no-stop-on-error` to continue through a
batch after failures. File-backed draining claims events with a worker lease,
acks successful events, requeues failed events until `--max-attempts`, and moves
exhausted events into the dead-letter directory. `--worker-id` controls the
lease owner label, `--lease-ms` controls stale lease recovery, and
`--max-attempts` controls retry exhaustion. This is a durable local/CI worker
foundation, not a networked cluster scheduler.

### `route`

Validate and execute a demo service route through the lightweight interpreter.
The command accepts either an entry `.num` file or a source directory.

```bash
cargo run -p num -- route examples/refund_workflow/src/main.num POST /refunds
cargo run -p num -- route examples/refund_workflow/src POST /refunds
```

The command selects the first service by default. Pass a service name as the
optional final argument when a module declares multiple services:

```bash
cargo run -p num -- route app.num POST /refunds BillingApi
```

Current limitations:

- this is a route dry-run, not an HTTP listener;
- permissions are injected by the CLI for demo purposes;
- route input values are generated for included demo schemas;
- configured `[connectors]` process commands run before the demo connector
  fallback.

### `serve`

Validate an entry `.num` file or source directory, bind the first service by
default, listen for HTTP requests, and execute matching service routes through
the lightweight interpreter.

```bash
cargo run -p num -- serve examples/refund_workflow/src/main.num 127.0.0.1:4000
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
cargo run -p num -- serve app.num 127.0.0.1:4000 BillingApi
```

For deterministic smoke tests, stop after a fixed number of accepted requests:

```bash
cargo run -p num -- serve app.num 127.0.0.1:4000 BillingApi --max-requests 2
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
- `X-Role` and comma-separated `X-Roles` headers are resolved against `.num`
  `role` declarations and grant the role's allowed permissions for the request;
- permissions can also be injected by the CLI for demo purposes;
- configured `[connectors]` process commands run before the demo connector
  fallback.

### `serve-once`

Validate a `.num` file, listen for one HTTP request, and execute the matching
service route. This command is kept for quick manual checks and uses generated
demo input when the request body is empty.

```bash
cargo run -p num -- serve-once examples/refund_workflow/src/main.num 127.0.0.1:4000
```

### `new`

Create a multi-file project skeleton with `num.toml`, a source directory, and a
manifest entry file.

```bash
cargo run -p num -- new my-service
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
cargo run -p num -- lock examples/refund_workflow
```

The command records the workspace package plus sorted `[dependencies]` entries
from `num.toml`. Dependency values can be version strings or inline tables:

```toml
[dependencies]
std = "0.1.0"
shared = { path = "../shared", version = "0.2.0" }
banking = { git = "https://example.com/banking.num.git", version = "1.4.0" }
```

The current lockfile is deterministic local metadata. It records the workspace
package language/schema compatibility metadata plus direct and transitive
path/local-registry dependencies that can be resolved locally. Resolved package
entries include sorted dependency edges. Git dependencies, and registry
dependencies without a configured local registry root, remain metadata-only
entries. Remote package fetching and git checkout are not implemented yet.

Direct `path` dependencies and local filesystem registry dependencies are
loaded during `check`, `run`, `route`, `serve`, and `serve-once`, which lets
`use <module.path>` resolve modules declared in a dependency package.

### `registry`

Manage a local filesystem package registry.

```bash
cargo run -p num -- registry publish examples/refund_workflow --registry /tmp/num-registry
cargo run -p num -- registry publish examples/refund_workflow --registry /tmp/num-registry --dry-run --json
cargo run -p num -- registry list --registry /tmp/num-registry
cargo run -p num -- registry install refund-workflow 0.1.0 --registry /tmp/num-registry --to vendor/num
```

`publish` validates the package manifest, collects package source files, and
copies them into `<registry-root>/<package-name>/<version>/`. It skips common
build/runtime output directories such as `.git`, `target`, `node_modules`,
`.num-state`, and `dist`. Existing package versions are protected by default;
pass `--replace` to overwrite a published local version.

`list` reads package/version directories that contain `num.toml`. `install`
copies a published package from the registry into `<install-root>/<name>/<version>/`.
The default install root is `.num/packages`. These commands are local registry
tooling for development and private package workflows; they do not implement a
remote package service yet.

### `connector-sdk`

Generate connector implementation SDKs from the checked `.num` module graph.

```bash
cargo run -p num -- connector-sdk examples/contract_driven_refund
cargo run -p num -- connector-sdk examples/contract_driven_refund \
  --language typescript \
  --out examples/contract_driven_refund/generated/connectors.d.ts
cargo run -p num -- connector-sdk examples/contract_driven_refund --json
```

The TypeScript generator emits:

- runtime wrapper types used by connector signatures, such as `Money`,
  `Option`, `Result`, `Uncertain`, `Secret`, and `JsonValue` when needed;
- checked `.num` struct, alias, and enum declarations visible to the entry
  module;
- a `NumConnectors` interface grouped by connector namespace, with each method
  returning a `Promise`.

This gives backend authors a generated implementation contract for process or
host-language connector code. It is not managed connector hosting, auth/secrets
binding, or a generated network client runtime yet.

### `deploy`

Validate a project and build a deployment plan artifact from `num.toml` and the
compiled `.num` module graph.

```bash
cargo run -p num -- deploy examples/refund_workflow
cargo run -p num -- deploy examples/refund_workflow --json
cargo run -p num -- deploy examples/refund_workflow --out dist/num-deploy.json
cargo run -p num -- deploy examples/refund_workflow --apply --dir dist/refund-deploy
cargo run -p num -- deploy examples/refund_workflow --apply --replace --json
```

The plan includes package name/version, deployment target metadata, runtime
store metadata, security mode, compiled module count, workflows, actions,
service routes, connectors, process connector bindings, and direct
dependencies. It also embeds the manifest language/schema compatibility
contract.

With `--apply`, the command materializes a reproducible local/CI deployment
bundle. The bundle includes:

- `num-deploy.json` - checked deployment plan;
- `num.toml` - source package manifest;
- `modules/` - source module snapshot used for compilation;
- `manifest.json` - artifact metadata and module map;
- `RUNBOOK.md` - operations boundary and handoff notes.

The default bundle directory is derived from `[deployment].artifact` by removing
the file extension. Use `--dir <artifact-dir>` to choose a different output
directory. Existing bundle directories are protected by default; pass
`--replace` to overwrite them. This is deployment artifact materialization, not
cloud/container execution.

### `compat`

Validate manifest language/schema compatibility for a project and any loaded
path or local-registry dependencies.

```bash
cargo run -p num -- compat examples/refund_workflow
cargo run -p num -- compat examples/refund_workflow --json
```

The command checks `[language].version`, `[language].compatibility`, and
`[language].manifest_schema` against the current CLI. Project commands use the
same validation before compiling source, so a package authored for a future
language/schema version fails early instead of being interpreted as an older
project. The CLI compatibility matrix tests cover current manifests, exact
compatibility, legacy missing-language manifests, schema `0` migration, future
schema rejection, future language rejection, and project-version upgrade
compatibility.

### `migrate`

Plan or apply safe `num.toml` manifest migrations.

```bash
cargo run -p num -- migrate examples/refund_workflow
cargo run -p num -- migrate examples/refund_workflow --json
cargo run -p num -- migrate legacy_project --write
cargo run -p num -- migrate examples/refund_workflow --source --json
cargo run -p num -- migrate legacy_project --source --write
```

The command is a dry-run by default. It discovers `num.toml` from a project
directory or file path, reports the changes needed for the current CLI, and only
writes when `--write` is passed. The current migration path inserts missing
`[language]` metadata, fills partial `[language]` sections, and upgrades
`manifest_schema = 0` to the current manifest schema. Manifests that require a
future schema are rejected instead of rewritten.

`--source` switches from manifest migration to source migration. It discovers
workspace `.num` source files, runs the compiler checks, reports blocking
diagnostics, and lists per-file source migration actions. The first v0.1.0
source rewrite inserts deterministic explicit `module` declarations into legacy
files that omit them, deriving the module path from the manifest source-relative
file path. `--source --write` applies source rewrites only when the current
source graph has no blocking compiler diagnostics. See
[migration-guides.md](migration-guides.md) for released migration behavior.

### `upgrade-version`

Plan or apply safe `num.toml` version upgrades.

```bash
cargo run -p num -- upgrade-version examples/refund_workflow
cargo run -p num -- upgrade-version examples/refund_workflow --json
cargo run -p num -- upgrade-version examples/refund_workflow --project 0.2.0 --write
cargo run -p num -- upgrade-version legacy_project --language 0.1.0 --write
cargo run -p num -- upgrade-version examples/refund_workflow --include-dependencies --json
cargo run -p num -- upgrade-version examples/refund_workflow --include-dependencies --write --write-dependencies
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

### `version`

Print the CLI, language, and manifest schema versions.

```bash
cargo run -p num -- version
cargo run -p num -- version --json
cargo run -p num -- --version
```

### `import openapi`

Generate `.num` type and connector declarations from an OpenAPI JSON document.

```bash
cargo run -p num -- import openapi openapi.json generated.billing > src/billing_api.num
```

The importer currently handles a focused OpenAPI 3 JSON subset:

- `components.schemas` object schemas become `type` declarations;
- `paths` operations become connector methods;
- `operationId` becomes the method name when present;
- JSON request bodies become a `body` parameter;
- JSON success responses become method result types;
- scalar schemas map to `Text`, `Int`, `Float`, `Bool`, `Json`, and `List<T>`.

YAML input, authentication/security schemes, `allOf`/`oneOf` composition,
pagination conventions, and generated runtime clients are not implemented yet.

### `import sql`

Generate `.num` table types and database connector declarations from a SQL
schema file.

```bash
cargo run -p num -- import sql schema.sql generated.db > src/database.num
```

The importer currently handles a focused SQL subset:

- `CREATE TABLE` statements;
- column declarations with common SQL scalar types;
- nullable columns as `Option<T>`;
- inline `PRIMARY KEY` columns;
- single-column table-level `PRIMARY KEY (...)` constraints, including named
  constraints;
- generated `database` connector methods: `list_<table>`,
  `find_<table>_by_<primary_key>`, and `insert_<table>`.

The runtime crate includes an in-memory executor for these generated
`database.*` method names. It is intended for contract tests and demos, not for
production database access.

Indexes, foreign keys as typed relations, composite primary-key finder methods,
migrations, SQL dialect-specific features, and generated runtime clients are not
implemented yet.

### `completions`

Print shell completion scripts.

```bash
cargo run -p num -- completions zsh
```

Only zsh completion is supported in v0.1.0.

### `lsp`

Start the language server process used by the VS Code extension.

```bash
cargo run -p num -- lsp
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
