# num Architecture

This document describes the v0.3.0 repository architecture and the boundary
between implemented components and planned Num platform work.

## Pipeline

```text
Source
  -> Lexer
  -> Parser
  -> AST
  -> Semantic Checker
  -> IR
  -> CLI / LSP / Demo Runtime
```

## Workspace Layout

```text
.
  Cargo.toml
  language/
    Cargo.toml
    crates/
      num-compiler/
      num-runtime/
      num-lsp/
      num-cli/
  vscode-extension/
  examples/
  docs/
  scripts/
  .github/workflows/
```

The root Cargo workspace includes `language/crates/*`.

## Crates

### `num-compiler`

Owns source analysis:

- token definitions;
- lexer;
- expression AST/parser;
- parser;
- AST;
- semantic diagnostics;
- built-in symbol metadata;
- formatter;
- IR lowering.

The public entry points are:

- `compile(source_name, source)`;
- `check(source_name, source)`;
- `check_program(files)`;
- `compile_program(files, entry_source_name)`.

The single-file entry points keep the module-local behavior used by formatter,
IR printing, and the LSP. `check_program` parses multiple files, builds a
module index, resolves `use` imports, and semantically checks each module with
its imported declarations visible. `compile_program` also returns a linked entry
module and lowered IR for runtime commands.

### `num-runtime`

Owns runtime contracts and the demo interpreter:

- workflow identifiers and statuses;
- security context;
- action specification;
- money representation;
- audit event representation;
- audit JSONL report summarization;
- `Uncertain<T>`;
- redacted `SecretValue`;
- runtime errors;
- `AuditSink`, `StateStore`, and `SecretStore` traits;
- provider-neutral external secret backend adapter boundary and first Vault
  token-auth/KV v2 adapter slice;
- provider-neutral encryption envelope boundary with redacted `Encrypted<T>`
  payload metadata, provider-backed encrypt/decrypt helpers, deterministic test
  provider coverage, and decrypted secret privacy/trust labels;
- KMS-style encryption provider adapter boundary over the same envelope
  contract, with provider-neutral key ids, metadata-only credential env names,
  structured key/provider failures, and deterministic fake KMS coverage;
- service-route JWT verification boundary with manifest-configured
  issuer/audience/allowed algorithms, env-backed signing secret, verified
  actor/tenant/role claims, and structured fail-closed auth errors;
- workflow event and queue contracts;
- tenant isolation guard for tenant-scoped workflow state and event access;
- text sanitization contracts, reusable sanitizer packs, and policy composition;
- runtime trace event model for observability/debugging;
- runtime metrics export boundary with OpenTelemetry-compatible names and
  attributes for workflow events, route latency, connector failures, AI calls,
  cost counters, and rate-limit hits, plus safe-by-default tenant/actor labels;
- connector egress context envelopes for propagating actor, tenant, scoped
  capability, request/correlation identifiers, policy decision, and declared
  source/privacy/trust labels across external connector boundaries;
- cost ledger report summarization for dashboard-oriented tooling;
- file-backed `StateStore`;
- file-backed workflow state listing and dashboard-oriented summary reports;
- append-only file-backed `AuditSink`;
- memory and file-backed secret stores, a deterministic external secret stub
  backend for tests, and mocked/fixture-server Vault adapter coverage;
- memory and file-backed workflow event queues;
- file-backed worker leases, retry attempts, and dead-letter event handling;
- batch workflow event worker for draining queued lifecycle events into
  file-backed workflow state and audit logs;
- in-memory database connector executor for SQL-imported connector contracts;
- runtime trace collection through the demo interpreter;
- `WorkflowEngine` lifecycle wrapper for start/wait/resume/complete/fail/
  compensate/cancel state transitions and audit events;
- `require_permission`;
- lightweight interpreter for examples.

The interpreter is intentionally small and mocked. It is useful for validating
the end-to-end language slice. The runtime has durable file-backed state, audit
primitives, lifecycle transitions, file-backed event queues, worker leases,
retry attempts, dead-letter handling, and a batch worker for draining queued
lifecycle events, but it is not yet a clustered distributed workflow platform.

### `num-lsp`

Adapts compiler capabilities to editor workflows:

- diagnostics;
- completions;
- hover;
- formatting;
- document symbols;
- lightweight JSON-RPC handling.

The crate keeps protocol parsing in `json.rs`; `lib.rs` owns the server loop,
document state, and editor feature handlers.

The VS Code extension launches the CLI with `num lsp`.

### `num-cli`

Owns user-facing commands:

- `check`;
- `lint`;
- `fmt`;
- `ir`;
- `run`;
- `trace`;
- `debug`;
- `deploy`;
- `compat`;
- `migrate`;
- `upgrade-version`;
- `version`;
- `registry`;
- `workflow`;
- `connector-sdk`;
- `cost-report`;
- `audit-report`;
- `workflow-report`;
- `route`;
- `serve`;
- `serve-once`;
- `new`;
- `lock`;
- `import openapi`;
- `import sql`;
- `completions`;
- `lsp`;
- `help`.

`project.rs` owns source discovery, including direct path dependency source
loading, and project scaffolding. `package.rs` owns `num.toml`
package/dependency parsing, deterministic `num.lock` generation, lockfile
schema validation/migration, and transitive path/local-registry lock graph
resolution. During locking, git dependencies are cloned/fetched into the
project-local `.num-git` cache and recorded with resolved commit source labels;
deploy metadata keeps deterministic source labels for declared dependencies.
`deploy.rs`
owns deployment plan construction and local/CI deployment bundle materialization
from checked projects. `compatibility.rs`
owns language-version, manifest-schema, and compatibility-policy validation for
projects and dependency packages. `migration.rs` owns `num.toml` migration
planning/application for legacy and partial manifest language metadata, plus
`.num` source migration planning/application for versioned rewrite rules.
Migration guide fixtures in `language/crates/num-cli/tests/fixtures` pin
released compatibility behavior.
`version_upgrade.rs` owns safe manifest language/project version upgrade
planning/application, including dependency graph reporting for resolved
path/local-registry manifests.
`registry.rs` owns local filesystem registry resolution, publish, list, index,
and install operations, package artifact file selection, and package metadata
validation.
`workflow_cli.rs` owns file-backed workflow event enqueue/drain operations for
durable lifecycle processing.
`connector_sdk.rs` owns connector implementation SDK rendering from checked
`.num` schemas, while `connector_sdk_cli.rs` owns CLI argument parsing and file
output for `num connector-sdk`.
`connector_cli.rs` owns direct process connector probing for
manifest-configured `[connectors]` bindings.
`openapi.rs` owns generation of `.num` connector contracts and minimal
TypeScript transport client stubs from OpenAPI JSON and YAML, including
preservation comments for authentication metadata, unsupported callbacks, and
links.
`sql_schema.rs` owns generation of `.num` table types, foreign-key relation
hint comments, and database connector contracts from SQL schema files.

The compiler's `lint.rs` module owns project quality/security lint rules. Lints
are run by `num lint` and intentionally stay separate from semantic errors used
by `num check`.

See [cli.md](cli.md).

## Parser Design

The parser recognizes the core declaration and statement surface directly:

- modules and imports;
- permissions and roles;
- policies;
- structured types, type aliases, and enums;
- functions, workflows, and actions;
- connectors and services;
- `let`, `var`, assignment, `require`, `transaction`, `if`, `match`,
  `return`, and expression statements.

Connector bodies are parsed into typed method schemas. Service bodies are parsed
into route schemas with method, path, route permissions, optional input binding,
and statement body. Statement-level expressions are stored as text in the
surface AST, then parsed by the semantic checker through the dedicated
expression AST.

This keeps the source AST simple while still giving semantic analysis a proper
tree for supported expression forms: literals, identifiers, calls, member
access, arithmetic expressions, ordering comparisons, equality comparisons, and
boolean operators. Full expression grammar remains a later compiler phase.

## Semantic Checker

The semantic checker builds module-local indexes for:

- declared permissions;
- action permission requirements;
- action risks;
- callable names;
- data policy rules;
- declared types;
- declared type arities and generic parameters;
- external connector/service namespaces;
- service route schemas.

It then checks declarations and statement bodies for the diagnostics documented
in [diagnostics.md](diagnostics.md).

The checker is intentionally conservative around the features it knows, but it
is not a complete type checker yet.
Semantic feature groups are being split into submodules:

- call validation lives in `semantic/calls.rs`;
- enum constructor checking lives in `semantic/enum_constructors.rs`;
- expression typing, field access, and binary operator checks live in
  `semantic/expressions.rs`;
- Option constructor checking lives in `semantic/option_constructors.rs`;
- Option flow narrowing lives in `semantic/option_flow.rs`;
- Result constructor checking lives in `semantic/result_constructors.rs`;
- Result flow narrowing lives in `semantic/result_flow.rs`;
- trust label propagation and untrusted sink checks live in
  `semantic/trust_flow.rs`.

Expression typing currently covers:

- literals;
- binding references;
- struct fields;
- generic struct field substitution;
- structural alias expansion for type compatibility;
- exact compatibility for nominal branded aliases;
- enum payload variant constructors, unique enum variant constructor
  inference, and payload match bindings;
- union alias compatibility against member types;
- union alias `match` validation, exhaustiveness, and simple binding
  narrowing;
- structured union member `match` destructuring with field label propagation;
- trust labels through field access and local bindings;
- `Option<T>.is_some`;
- `Option<T>.is_none`;
- guarded `Option<T>.value`;
- `Some(...)` inference and `Some(...)` / `None` constructors in typed
  `Option<T>` contexts;
- `Result<T,E>.is_ok`;
- `Result<T,E>.is_err`;
- guarded `Result<T,E>.value`;
- guarded `Result<T,E>.error`;
- `Ok(...)` and `Err(...)` constructors in typed `Result<T,E>` contexts;
- `Result<T,E>?` unwrap with compatible enclosing `Result<_,E>` propagation;
- `Uncertain<T>.confidence`;
- `Uncertain<T>.value`;
- direct `fn`, `workflow`, and `action` calls;
- connector method results;
- arithmetic expressions;
- selected `Money<C>` arithmetic rules;
- ordering comparisons;
- equality comparisons;
- boolean operators;
- enum and union alias `match` exhaustiveness;
- typed `return` expressions;
- exhaustive return-path analysis for typed callables across statement
  sequences, `if`/`else`, `transaction`, and exhaustive `match`.

## IR

The IR is effect-oriented. It records top-level items and selected effects:

- permissions from roles, functions, workflows, and actions;
- policy rules as data-policy effects;
- type alias targets;
- external action effects;
- audit requirements for high-risk actions;
- connector method signatures;
- rollback metadata;
- cost metadata;
- workflow marker effects.

The IR is a compiler/runtime integration boundary, not a stable user-facing
format.

## Runtime Boundary

The runtime crate contains two layers:

- reusable contracts such as workflow state, security context, audit events,
  money, and uncertainty;
- a demo interpreter that can execute the included example workflows and
  service route dry-runs through an injectable connector executor;
- a static connector registry for wiring connector method names to handlers;
- a process connector executor for manifest-configured external connector
  commands;
- an in-memory database connector executor for generated `database.list_*`,
  `database.find_*_by_*`, and `database.insert_*` methods;
- a demo connector executor with mocked implementations for bundled examples;
- a minimal HTTP helper used by `num serve` / `num serve-once` to bind HTTP
  requests to service routes, including `Content-Length` body reads and basic
  request size limits;
- a `ServiceRuntime` boundary that maps HTTP requests to route execution and
  HTTP responses, including extraction of actor, tenant, request id, and
  correlation id headers into a runtime security context, plus request role
  resolution from `X-Role` / `X-Roles` headers through `.num` role
  declarations;
- a typed JSON decoder that maps HTTP request bodies into runtime `Value`
  instances using `.num` route input schemas.
- file-backed workflow state and audit event persistence primitives.
- a workflow lifecycle engine that validates and persists Running, Waiting,
  Completed, Failed, Compensated, and Cancelled states and writes audit events.
- memory and file-backed workflow event queues.
- file-backed worker ownership leases, retries, and dead-letter handling.
- queued workflow event processing through `WorkflowEngine`.
- tenant-aware workflow load and transition helpers through `WorkflowEngine`.

The demo interpreter supports:

- workflow parameter binding;
- service route input binding;
- permission checks;
- `let`;
- `if`;
- `transaction saga`;
- declared action/function calls;
- connector calls through the runtime connector executor;
- missing implementations for declared connector methods fail at runtime;
- action execution wrapper with retry policy, idempotency replay, and
  timeout/cost-limit checks;
- in-memory and file-backed idempotency stores;
- runtime cost ledger and demo interpreter charging of successful action
  `cost` metadata;
- demo interpreter pre-authorization of declared action `cost` before side
  effects execute;
- demo interpreter enforcement of workflow/function/service `budget` metadata
  through hierarchical budget scopes;
- demo interpreter enforcement of workflow/service `rate limit` metadata;
- demo interpreter application of action `timeout`, `retry`, and `idempotency key`
  metadata;
- runtime trace events for workflow/service/statement/function/action/connector
  execution and audit logging;
- audit collection;
- best-effort rollback registration;
- service route dry-run through `num route`;
- persistent HTTP service route execution through `num serve`;
- one-request HTTP service route execution through `num serve-once`.

Not implemented yet:

- distributed event-driven state machine execution;
- preemptive async cancellation for timed-out connector calls;
- network-native connector SDKs and managed connector hosting;
- distributed rate-limit persistence across runtime processes;
- distributed or cross-process cost accounting and budget persistence;
- real compensation execution across process boundaries.

## VS Code Extension

The extension contributes:

- `.num` language registration;
- TextMate syntax grammar;
- snippets;
- commands:
  - `Num: Check Current File`;
  - `Num: Format Current File`;
  - `Num: Restart Language Server`;
  - `Num: Create New Project`;
- settings:
  - `num.cliPath`;
  - `num.lsp.trace.server`.

The extension can auto-detect a local `target/debug/num` before falling back to
`num` from `PATH`.

## Release Packaging

`scripts/package-current-platform.sh` builds:

- release CLI binary;
- VS Code extension `.vsix`;
- install scripts;
- release README;
- platform archive.

The GitHub Actions release workflow runs tests, builds packages for supported
platforms, uploads artifacts, and publishes a GitHub Release for `v*` tags.

## Planned Evolution

The next architectural steps are:

- expand the expression AST to nested pattern expressions plus richer literals;
- broaden assignment flow checking across branches and richer pattern matches;
- extend multi-file modules beyond checking into runtime/package execution;
- harden `serve` into production HTTP server execution for service routes;
- replace hardcoded runtime mocks with connector interfaces;
- add durable workflow state storage;
- deepen the current executable `.num` unit-test, static policy-test, direct
  workflow expectation, deterministic connector fixture, and deterministic AI
  mock foundation into richer workflow state simulation and AI provider test
  harnesses;
- expand the standard library behind stable compiler/runtime boundaries.
