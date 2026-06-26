# Num Specification Coverage

This document maps the full Num technical specification to the current `num`
v0.4.0 development implementation.

The short version: the repository implements a working compiler frontend,
semantic checker, IR, CLI, editor integration, examples, release package, and a
mocked demo runtime. It does not yet implement the complete industrial Num
language/runtime/platform.

## Covered in v0.4.0

### Language Surface

Implemented top-level declarations:

- `module`
- `use`
- `permission`
- `role`
- `policy`
- `type`
- `enum`
- `fn`
- `workflow`
- `action`
- `connector`
- `service`
- `test`, `test policy`, `test workflow`, and `test ai`

Implemented type declaration forms:

- structured types: `type Name { field: Type }`;
- generic structured types: `type Page<T> { item: T }`;
- plain aliases: `type Name = ExistingType`;
- generic aliases: `type Maybe<T> = Option<T>`;
- nominal branded aliases: `type UserId = Brand<Text, "UserId">`;
- union aliases: `type SearchResult = User | Company`.

Implemented statement forms:

- `let`
- `var`
- assignment to `var` bindings
- `assert`
- `expect_deny`
- `expect_allow`
- `expect_workflow_success`
- `expect_workflow_failure`
- `expect_audit`
- `mock_connector`
- `mock_ai`
- `require`
- `transaction`
- `transaction saga`
- `if` / `else`
- `match`
- `return`
- raw expression statements

Implemented labels:

- provenance: `from <Source>`
- trust: `untrusted`, `trusted`, `verified`
- privacy: `public`, `internal`, `private`, `sensitive`, `secret`,
  `regulated`

Implemented action metadata:

- `requires Permission.<Name>`
- `risk low|medium|high|critical`
- `timeout <raw-value>`
- `retry <attempt-count>`
- `idempotency key <raw-expression>`
- `rollback <raw-call>`
- `cost <raw-value>`

Implemented connector schemas:

- connector method name;
- method parameters with types and labels;
- optional method result type.

### Semantic Checks

The compiler checks:

- duplicate top-level declarations;
- duplicate module paths during program checks;
- missing `use` imports during program checks;
- role references to unknown permissions;
- unknown permission references;
- duplicate fields in a type;
- unknown type references;
- assignment to immutable or unknown bindings;
- assignment type compatibility for supported expression result types;
- enum and union alias `match` arm validity, duplicate arms, and
  exhaustiveness;
- AI calls assigned to non-`Uncertain<T>` bindings;
- uncertain values used without confidence handling;
- `Secret<T>` values treated as intrinsic secret privacy even without an
  explicit `secret` label;
- secret values passed to `log`;
- private/sensitive/secret/regulated values sent to `ExternalApi`,
  `ConnectorApi`, or a specific external/connector target without an allow
  policy;
- action calls without required permissions;
- high-risk actions without audit events;
- high-risk actions without rollback metadata;
- undeclared connector/service namespaces;
- undeclared direct callable names;
- duplicate connector methods;
- undeclared connector methods;
- connector call arity;
- connector argument types when inferrable;
- explicit binding type compatibility for connector results;
- inferred `let` binding types for supported expression result types, including
  connector calls and direct callable calls;
- duplicate service route method/path declarations;
- service route permission references;
- service route input type references;
- service route body semantic checks;
- service route permission grants for action calls;
- direct callable arity and argument type checks;
- explicit binding type compatibility for direct callable results;
- test-body semantic checks using the same statement and expression checker as
  functions/workflows;
- `assert` expressions must type-check as `Bool`;
- static `test policy` expectations through `expect_deny` and `expect_allow`,
  including inversion of expected `N2400` policy-denial diagnostics;
- `test workflow` runtime expectations through direct workflow calls with
  `expect_workflow_success` and `expect_workflow_failure`;
- `test workflow` audit expectations through `expect_audit`;
- deterministic `test workflow` connector fixtures through
  `mock_connector connector.method(...) => Value` for declared non-AI connector
  methods with non-`Unit` results;
- checked-in workflow lifecycle fixtures for wait/resume audit checkpoints,
  saga compensation audits, and idempotent action replay behavior;
- deterministic `test ai` mocks through `mock_ai ai.method(...) => Value
  confidence <number>` for declared `Uncertain<T>` AI connector methods;
- `return` value compatibility with declared `fn`, `workflow`, and `action`
  result types;
- supported expression syntax;
- struct field access;
- ordering comparisons over compatible ordered scalar types and same-currency
  `Money<C>` values;
- equality comparisons over compatible operand types;
- boolean `&&` / `||` expressions over `Bool` operands;
- `async <expr>` task creation as `Task<T>`, `await <task>` unwrapping, and
  rejection of `await` on non-task values;
- first structured-concurrency lost-task guard, rejecting bare `async`
  expression statements that create tasks without owners;
- arithmetic expressions over compatible numeric types and selected
  `Money<C>` operations;
- object literal expressions such as audit contexts, with nested labels
  preserved for policy and trust-flow checks.

### Tooling

Implemented:

- `num check <file.num>` for entry-file program checks over the containing
  source directory;
- `num check <directory>` for multi-file program checks with `use` resolution;
- `num fmt` stdout, `--write`, and `--check` modes
- `num ir`
- `num run`
- `num test`
- `num trace`
- `num debug`
- `num deploy`
- `num compat`
- `num migrate`
- `num upgrade-version`
- `num bench`
- `num bench --compare <baseline.json>` opt-in parse/check regression gates
  with percentage and absolute thresholds;
- `num release-plan`
- `num version`
- `num registry publish`
- `num registry list`
- `num registry index`
- `num registry install`
- `num workflow enqueue`
- `num workflow drain`
- `num workflow lease-heartbeat`
- `num connector probe`
- `num connector-sdk`
- `num cost-report`
- `num audit-report`
- `num workflow-report`
- `num route`
- `num serve`
- `num serve-once`
- `num new`
- `num completions bash`
- `num completions fish`
- `num completions zsh`
- `num lsp`
- `num.toml` source and entry manifest loading for CLI project commands;
- `[language]` manifest metadata for language version, compatibility policy,
  and manifest schema version;
- `num compat` language/schema compatibility reports for packages and loaded
  path/local-registry dependencies, including structured incompatible JSON
  reports with non-zero CI gate exit behavior;
- `num migrate` dry-run/write migration reports for legacy or partial
  `[language]` manifest metadata;
- `num migrate --source` source migration reports for workspace `.num` files,
  including blocking compiler diagnostics, per-file source actions, and
  automatic insertion of missing explicit `module` declarations;
- released migration guide coverage for manifest metadata and explicit source
  module declarations;
- `num upgrade-version` dry-run/write reports for safe `[language].version`
  and optional `[project].version` upgrades;
- `num upgrade-version --include-dependencies` graph reports for resolved
  path/local-registry dependency manifests, with explicit
  `--write-dependencies` application;
- `num version` CLI/language/manifest-schema/lockfile-schema version reporting;
- `num release-plan` SemVer bump planning from changelog `Major`/`Minor`/
  `Patch` sections;
- `num lock --check` lockfile schema validation, and deploy-time validation
  plus inclusion of `num.lock` in materialized bundles when present;
- `num lock --migrate` dry-run/write lockfile schema migration for legacy
  missing-schema and schema `0` lockfiles;
- fixture-backed CLI compatibility matrix coverage for current manifests,
  legacy missing-language manifests, schema `0` migration, future schema
  rejection, future language rejection, project-version upgrades, source
  module declaration rewrites, structured incompatible reports, and lockfile
  schema migrations;
- local filesystem registry publish/list/index/install workflow for package
  development and private package sharing, including package metadata,
  SemVer-aware version ordering, `latest` install resolution, API-ready package
  indexes, and content-hash verification;
- deterministic transitive `num.lock` pinning for resolved path/local-registry
  dependency graphs, including content-hash pins for resolved local-registry
  packages;
- deterministic git dependency checkout into `.num-git` during locking, with
  resolved commit SHA metadata in lockfiles and declared git selector labels in
  deploy plans;
- git package source discovery for project commands through the same `.num-git`
  checkout cache;
- deployment environment validation metadata from `[environment]` in deploy
  plans, materialized artifact metadata, and generated runbooks;
- explicit Docker registry image publish handoff metadata in deploy plans and
  `deploy/image-publish.json`, including registry/image/tag strategy, publish
  reference, and credentials references without credential values;
- early project-command rejection for packages that require a future
  language/manifest schema version;
- `[security].policy_mode = "strict"` enforcement for project commands, which
  runs lints and treats warnings as blocking diagnostics;
- VS Code syntax highlighting;
- VS Code snippets;
- LSP diagnostics, completions, hover, and go-to-definition over sibling `.num`
  modules and open editor buffers;
- VS Code commands for check, format, restart LSP, and new project;
- VS Code configuration for CLI path and LSP tracing;
- release packaging for CLI and VS Code extension.

Program checks, LSP diagnostics, LSP language-intelligence features, and runtime
demo commands can operate on linked multi-file entry modules. Formatter, IR
printing, formatting edits, and document symbols still operate on the current
source file.

### Runtime

Implemented:

- runtime data types for workflow state, security context, actions, audit
  events, money, uncertainty, state stores, audit sinks, secret stores, and
  workflow event queues;
- runtime text sanitization policy/result contracts, a default text sanitizer,
  reusable sanitizer packs, and policy composition;
- tenant isolation guard and tenant-aware workflow state load, transition, and
  queued-event processing helpers;
- in-memory database connector executor for generated SQL connector contracts;
- runtime trace event model and demo interpreter trace collection;
- audit JSONL report summarization by result/action/actor/tenant plus failure
  details;
- workflow lifecycle engine for persisted start/wait/resume/complete/fail/
  compensate/cancel transitions;
- workflow state listing and dashboard-oriented report summarization by
  status/name/actor/tenant;
- cost dashboard report breakdowns by currency, action, connector, model,
  workflow, route, request id, correlation id, actor, and tenant;
- project manifest `[runtime].workflow_store = "file:<state-root>"` and
  `[runtime].audit_store = "file:<events.jsonl>"` resolution for `num workflow
  enqueue`, `num workflow drain`, `num workflow lease-heartbeat`, and
  `num workflow-report`;
- project manifest `[runtime].audit_store = "file:<events.jsonl>"` resolution
  for demo interpreter commands `num run`, `num test`, `num trace`,
  `num debug`, `num cost-report`, and `num route`, with report-compatible demo
  audit JSONL output;
- project manifest `[runtime].audit_store = "file:<events.jsonl>"` resolution
  for HTTP service commands `num serve` and `num serve-once`, with
  request-scoped actor, tenant, request id, correlation id, service, method, and
  path metadata in report-compatible audit JSONL output;
- file-backed workflow state store;
- file-backed append-only audit JSONL sink;
- idempotent durable workflow event replay by persisted event id metadata,
  preventing duplicate lifecycle audit output and invalid terminal transition
  reapplication for already processed queue events;
- memory and file-backed secret stores with redacted secret value debug output;
- runtime redaction of `Secret<T>` values and secret-like connector failures in
  trace/debug JSON, structured runtime errors, process connector JSON
  conversion, and service error responses;
- memory and file-backed workflow event queues;
- file-backed worker ownership leases, lease heartbeat refresh, retry attempts,
  stale lease recovery, and dead-letter event handling;
- queued workflow event processing for start/wait/resume/complete/fail/
  compensate/cancel transitions;
- `require_permission` helper;
- lightweight interpreter for demo workflows;
- lightweight interpreter entrypoint for executable `.num` test declarations
  with runtime `assert` failures;
- lightweight interpreter entrypoint for demo service routes;
- persistent HTTP boundary for demo service routes;
- one-request HTTP boundary for demo service routes;
- HTTP request framing with `Content-Length` body reads and basic request size
  limits;
- HTTP header capture for service-route `SecurityContext` fields:
  `X-Actor`, `X-Tenant`, `X-Request-Id`, and `X-Correlation-Id`;
- request role headers `X-Role` and `X-Roles` resolved against `.num` `role`
  declarations to populate service-route runtime permissions;
- project manifest `[security].tenant_isolation` wiring for `num route`,
  `num serve`, and `num serve-once`, with cross-tenant service-route requests
  rejected before route execution and recorded in audit output;
- service-route tenant-scoped policy checks using runtime request tenant
  context for `num route`, `num serve`, and `num serve-once`, while standalone
  file checks remain conservative without request context;
- typed JSON request body decoding for route inputs;
- normalized JSON service-route error responses for `num route`, `num serve`,
  and `num serve-once`, including stable `kind`/`code` fields and
  request/correlation identifiers;
- connector execution interface and static connector registry;
- manifest-configured process connector execution for `num run`, `num test`,
  `num trace`, `num cost-report`, `num route`, `num serve`, and
  `num serve-once`;
- manifest-configured JavaScript callable-module execution through
  `[javascript]` bindings for local Node modules, reusing connector JSON value
  conversion and connector egress context for actor, tenant, request id,
  correlation id, policy marker, and argument labels;
- direct process connector probing through `num connector probe`, without demo
  connector fallback;
- manifest-configured process connector timeout budgets with runtime
  process termination and deploy-plan metadata;
- deploy artifact source-tree snapshots plus generated Docker Compose,
  Kubernetes, and bare-metal systemd-style runtime scaffolds for
  container/orchestrator/host targets;
- container image publish handoff artifacts for configured registry/image
  targets, with generated Compose/Kubernetes scaffolds pointing at the planned
  image reference;
- Kubernetes dry-run handoff output with generated deployment/service YAML,
  namespace/image/port validation, and secret-like environment reference
  warnings before cluster mutation support;
- connector error taxonomy at the runtime executor boundary with stable
  `code`, `message`, and `retryable` fields;
- machine-readable connector failure reports in `num run --json` and
  `num debug --json` through `runtime_error.connector`, with secret values
  rendered as `<redacted>`;
- silent JSON stdout for runtime reporting commands (`run --json`, `trace`,
  `debug --json`, and `cost-report --json`);
- TypeScript connector implementation SDK generation from checked connector
  schemas;
- runtime errors for declared connector methods without an implementation;
- demo connector executor for bundled examples;
- action execution wrapper with retry policy, cost-limit check, and
  timeout/idempotency replay;
- in-memory and file-backed idempotency stores for action execution records;
- runtime cost ledger with per-currency action charges and optional budget
  limits;
- cost ledger report summarization by currency and action;
- demo interpreter application of action `timeout`, `retry`, and `idempotency key`
  metadata;
- demo interpreter authorization of action `cost` metadata against active
  budget scopes before side effects run, followed by charging after successful,
  non-replayed action executions;
- workflow, function, and service `budget <amount> <currency>` metadata parsed,
  formatted, lowered to IR, and enforced by the demo interpreter cost ledger;
- hierarchical runtime budget scopes where nested function/workflow calls
  inherit parent workflow or service route budgets and may add stricter child
  budgets;
- workflow and service `rate limit <count> per <duration>` metadata parsed,
  formatted, lowered to IR, and enforced by the demo interpreter rate limiter;
- mocked implementations for selected demo connector calls;
- in-memory audit output for the demo interpreter;
- best-effort saga rollback registration in the demo interpreter.

## Partially Covered

### Static Type System

The parser stores type references, validates known type names, recognizes
built-in wrappers such as `Money<T>`, `Option<T>`, `Result<T,E>`,
`Uncertain<T>`, `Secret<T>`, and `Brand<T,"Tag">`, tracks labels on bindings,
and the semantic checker parses supported expressions into an expression AST.

Implemented type features:

- structured types with duplicate-field checks;
- generic type parameters on structured types and aliases;
- generic type reference arity checks;
- generic field type substitution for structured type field access;
- structural compatibility for plain aliases;
- generic alias substitution during compatibility checks;
- nominal branded aliases with exact type compatibility only;
- non-generic branded alias constructors such as `UserId("user_1")`;
- context-typed generic branded alias constructors such as
  `let value: Boxed<Int> = Boxed(42)`;
- enum payload variants such as `Failed(Text)`;
- context-typed enum variant constructors such as `Failed("reason")`;
- enum payload match bindings such as `Failed(reason)`;
- union aliases that accept compatible member types;
- union alias `match` validation, exhaustiveness, and simple binding narrowing
  inside member arms;
- structured union member destructuring in `match` patterns, including
  field-binding aliases and label propagation;
- nested structured union member destructuring in `match` patterns;
- `match` arm guard clauses, including boolean type-checking and runtime
  top-to-bottom guarded arm selection;
- alias target type validation;
- alias target lowering to IR.

Implemented expression typing:

- literals;
- binding references;
- struct field access;
- `Option<T>.is_some`;
- `Option<T>.is_none`;
- guarded `Option<T>.value`;
- sound `Option<T>` narrowing through supported `if` boolean guards using
  `&&` and `||`;
- `Some(...)` inference from payload type;
- `Some(...)` and `None` constructors in typed `Option<T>` contexts;
- `Result<T,E>.is_ok`;
- `Result<T,E>.is_err`;
- guarded `Result<T,E>.value`;
- guarded `Result<T,E>.error`;
- sound `Result<T,E>` narrowing through supported `if` boolean guards using
  `&&` and `||`;
- `Ok(...)` and `Err(...)` constructors in typed `Result<T,E>` contexts;
- `Result<T,E>?` unwrap with compatible enclosing `Result<_,E>` propagation;
- branded alias constructor calls with base payload type checks, including
  generic payload substitution from expected type context;
- explicit branded alias unwrap through `unbrand(value)`;
- unique enum variant constructor inference;
- enum variant constructor calls with payload type checks in inferred or typed
  enum contexts;
- `Uncertain<T>.confidence`;
- `Uncertain<T>.value`;
- object literal expressions typed as `Json`, with nested expression checking
  and label aggregation;
- named call payload syntax such as
  `require_human_approval(action: "issue_refund", reason: "Low AI confidence")`,
  desugared to one `Json` object argument;
- connector method calls;
- direct function/workflow/action calls;
- `reject(reason)` workflow-control builtin in the demo interpreter;
- ordering comparisons;
- same-currency `Money<C>` ordering comparisons;
- equality comparisons;
- boolean `&&` / `||` expressions;
- arithmetic `+`, `-`, `*`, and `/` expressions;
- selected `Money<C>` arithmetic rules;
- typed `return` expressions;
- exhaustive return-path analysis for typed callables across statement
  sequences, `if`/`else`, `transaction`, and exhaustive `match`.

Not yet implemented:

- overload or method resolution;
- generic constraints;
- general destructuring patterns beyond structured union member fields;
- broad type inference for complex expressions and generic partial types;
- general nullable/result flow analysis outside supported `if` boolean guards.

### Connector Schemas

The parser accepts typed connector method signatures and the checker validates
calls against those schemas.

Implemented:

- duplicate method detection;
- method parameter/result type validation;
- unknown method diagnostics;
- arity diagnostics;
- argument type diagnostics for known binding/field/literal types;
- connector result compatibility with explicit `let` binding types;
- OpenAPI JSON/YAML import for a focused connector-contract subset:
  `components.schemas`, `paths`, operation parameters, JSON request bodies,
  JSON success response schemas, security scheme/requirement preservation
  comments, review-required permission candidates, review-required policy
  placeholder comments for security/private-field hints, review-required
  pagination convention metadata comments for simple limit/offset, page/pageSize,
  cursor, and next-link response hints, and unsupported callback/link
  preservation comments.
- SQL schema import for a focused database-contract subset: `CREATE TABLE`
  columns, common scalar types, nullable columns, inline primary keys, table
  types, single-column and composite table-level primary keys, basic
  foreign-key relation hint comments, and basic database connector methods.
- runtime in-memory database connector executor for generated `database`
  connector methods: `list_<table>`, `find_<table>_by_<primary_key>`,
  composite `find_<table>_by_<key1>_and_<key2>`, and `insert_<table>`.
- TypeScript connector implementation SDK generation for visible `.num`
  structs, aliases, enums, and connector method signatures.
- Python connector implementation SDK generation for visible `.num` structs,
  aliases, enums, connector method signatures, and connector egress context
  stubs, with unsupported shapes falling back to `Any`.

Not yet implemented:

- managed connector hosting;
- generated network-native runtime clients;
- connector SDK targets beyond TypeScript/Python declarations;
- connector authentication/secrets;
- full JavaScript runtime embedding, generated JS host SDKs, npm package
  management, and network-native JS worker hosting;
- generated database clients;
- managed/network connector cancellation beyond local process timeout
  termination;
- full OpenAPI coverage such as executable authentication bindings,
  automatically correct production policies, `allOf`/`oneOf`, executable
  paginated clients, executable callbacks/links, and generated runtime clients.
- full SQL/database import coverage such as executable foreign-key relation
  loading, indexes, migrations, dialect-specific features, and generated runtime
  clients.

### Policies

The parser accepts policy blocks and the checker uses allow/deny rules for
external data flows. Rules from multiple policy blocks compose into one flow
policy set; deny rules take precedence over allow rules, including broad target
classes, and source-specific rules require a matching source label. Targets can
be the broad `ExternalApi` class, the broad `ConnectorApi` class, an external or
connector namespace such as `external.crm` or `mailer`, or a concrete method
such as `external.crm.send` or `mailer.send`. Policy checks include labels from
nested structured fields, and trust gateways preserve privacy/provenance labels
while changing trust. The `anonymize(...)` privacy gateway explicitly
declassifies its result to public `DerivedData` without treating untrusted input
as trusted. Policy rules can also be scoped with
`for tenant <tenant-id>`; those rules only match when policy evaluation receives
the same tenant context. Rules can include trust-level constraints such as
`trusted` or `verified`.

Not yet implemented:

- a complete policy language;
- richer policy conditions beyond stored privacy/provenance/trust labels.

### Workflow and Saga Semantics

The parser accepts workflows and saga blocks, and the checker verifies selected
rollback/audit constraints.

Implemented foundation:

- file-backed workflow state JSON persistence;
- append-only audit JSONL persistence.
- persisted workflow start, wait, resume, completion, failure, compensation,
  and cancellation transitions with audit events;
- memory and file-backed workflow event queues;
- file-backed worker leases, lease heartbeat refresh, retries, stale lease
  recovery, and dead-letter handling for queued workflow events;
- queued workflow event processing through the lifecycle engine;
- state transition validation for terminal and compensation states.
- runtime connector egress context propagation for external/process connector
  boundaries, including tenant, actor, request/correlation identifiers, scoped
  connector capability, policy decision marker, and declared argument labels.

Not yet implemented:

- distributed event-driven state transition runner;
- real distributed transactions;
- persistent compensation execution.

### AI Safety

The checker treats `ai.*` calls as uncertain and requires confidence handling.
It also rejects untrusted values passed into `ai.*` prompt/tool-call arguments
unless the value first flows through an explicit trust gateway such as
`sanitize(...)`, `validate_trust(...)`, `verify_trust(...)`, or
`require_human_review(...)`. This provides a static prompt-injection boundary
for user input and retrieved content. The examples demonstrate
human-in-the-loop branches.

Not yet implemented:

- real AI provider integration;
- model registry;
- runtime prompt-injection scanners and configurable scanner catalogs;
- tool-call sandboxing;
- AI policy configuration;
- richer AI test fixtures beyond deterministic `mock_ai` responses.
- workflow fixtures for distributed state simulation beyond the checked-in
  lifecycle examples.

### Cost and Limits

Action declarations can carry raw `cost` and `timeout` metadata. Runtime data
types include cost-limit and timeout errors, and the demo interpreter enforces
action timeout budgets, pre-authorizes declared action cost against active
budget scopes, and records successful action costs in a cost ledger.
`num cost-report --json` exposes a versioned `num.cost_dashboard.v1` read model
with stable totals by currency, action, connector, model, workflow, route,
actor, and tenant plus raw drill-down entries. Dimensions that are not yet
emitted by the current runtime path stay present as empty aggregates rather than
changing the JSON shape.

Not yet implemented:

- distributed rate-limit persistence across runtime processes;
- distributed timeout propagation;
- interactive and persisted cost dashboards;
- runtime-produced per-model or per-connector cost entries beyond the stable
  dashboard read-model fields.

## Not Covered Yet

Major full-spec areas not implemented in v0.3.0:

- remote package registry HTTP/service APIs;
- production remote git auth/cache policy;
- remote registry package lockfile pinning;
- complete standard library;
- hardened production HTTP server runtime;
- production database connectors;
- document processing APIs;
- full OpenAPI import coverage beyond the current connector-contract subset;
- full database schema import coverage beyond the current SQL contract subset;
- full structured concurrency beyond the current lost-task static guard;
- actor model;
- clustered/networked queue coordination beyond the local file-backed worker
  lease and heartbeat model;
- external secrets manager integration such as Vault/KMS/cloud secret stores;
- tenant isolation enforcement across every non-workflow runtime surface;
- locale-specific sanitizer catalogs and externally configured sanitizer packs;
- interactive debugger and IDE debug adapter;
- interactive workflow dashboard beyond the stable `num.workflow_dashboard.v1`
  `workflow-report --json` read model;
- interactive audit dashboard beyond the stable `num.audit_dashboard.v1`
  `audit-report --json` read model;
- interactive cost dashboard;
- cloud/container/bare-metal deployment execution model beyond generated
  local/CI artifacts, image publish handoffs, and Kubernetes dry-run handoffs;
- CI/CD integrations beyond generated GitHub Actions/Jenkins/GitLab deploy-gate
  templates, local deployment bundle generation, and release packaging;
- external-language interop;
- remote registry package imports;
- performance optimization strategy;
- broader automatic source rewrite rules between language versions.

## Coverage by Full Specification Area

### Strongly Represented

- core declaration syntax;
- permissions and roles;
- provenance/trust/privacy labels;
- trust label propagation and untrusted sink checks;
- trust gateway built-ins for sanitization, validation, verification, and human
  review/approval;
- simple data-flow policy checks;
- action metadata;
- audit requirement for high-risk actions;
- AI uncertainty wrapper pattern;
- saga syntax and rollback metadata;
- first-class service route schemas;
- CLI and VS Code foundation;
- release packaging.
- deployment plan artifact generation.
- local/CI deployment bundle materialization.
- deployment target profile classification and deploy-time warnings.
- language/schema compatibility checks.
- manifest migration tooling.
- source migration planning and first source rewrite rule.
- migration guide fixtures for released version behavior.
- manifest version upgrade tooling.
- graph-aware dependency version upgrade reports.
- lockfile schema validation and deploy artifact lockfile inclusion.
- lockfile schema migration tooling.
- fixture-backed manifest/source/lockfile compatibility matrix coverage.
- git dependency checkout, package source discovery, and commit pinning in
  lockfile outputs.

### Foundation Only

- hardened production HTTP server runtime for backend services;
- financial safety;
- documents;
- durable workflows with file-backed lifecycle event enqueue/drain tooling;
- audit schema;
- cost-aware execution;
- runtime observability;
- scripted CLI debugger with workflow/action/function/connector/audit
  breakpoints over runtime trace events;
- deployment planning and local artifact materialization;
- language versioning and compatibility policy;
- manifest migration tooling;
- manifest version upgrade tooling;
- compatibility matrix coverage for the current v0.3.0 manifest/schema
  surface;
- standard library.

### Planned Platform Work

- dashboards;
- deployment execution;
- package ecosystem;
- direct package dependency declarations and deterministic lockfile generation;
- transitive lockfile pinning for resolved local path/local-registry dependency
  graphs;
- direct path package imports;
- local filesystem registry package imports, including transitive registry
  dependencies;
- linter foundation;
- scripted CLI debugger;
- richer workflow fixture/state simulation and AI provider simulation beyond the
  current executable unit-test, static policy-test, direct workflow expectation,
  deterministic connector fixture, and deterministic AI mock foundation;
- full runtime infrastructure.

## Verification Commands

Use these commands to verify the documented v0.3.0 surface:

```bash
cargo test
num check examples/refund_workflow/src/main.num
num check examples/ai_agent/src/main.num
num check examples/policy_guard/src/main.num
num check examples/contract_driven_refund/src/main.num
num ir examples/refund_workflow/src/main.num
num run examples/refund_workflow/src/main.num
num test examples/refund_workflow
num bench --json
node examples/contract_driven_refund/backend/runtime-demo.js success
node examples/contract_driven_refund/backend/runtime-demo.js approval
```
