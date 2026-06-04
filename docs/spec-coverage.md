# Num Specification Coverage

This document maps the full Num technical specification to the current `num`
v0.1.0 implementation.

The short version: the repository implements a working compiler frontend,
semantic checker, IR, CLI, editor integration, examples, release package, and a
mocked demo runtime. It does not yet implement the complete industrial Num
language/runtime/platform.

## Covered in v0.1.0

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
- arithmetic expressions over compatible numeric types and selected
  `Money<C>` operations;
- object literal expressions such as audit contexts, with nested labels
  preserved for policy and trust-flow checks.

### Tooling

Implemented:

- `num check <file.num>` for entry-file program checks over the containing
  source directory;
- `num check <directory>` for multi-file program checks with `use` resolution;
- `num fmt`
- `num ir`
- `num run`
- `num test`
- `num trace`
- `num debug`
- `num deploy`
- `num compat`
- `num migrate`
- `num upgrade-version`
- `num version`
- `num registry publish`
- `num registry list`
- `num registry install`
- `num workflow enqueue`
- `num workflow drain`
- `num connector-sdk`
- `num cost-report`
- `num audit-report`
- `num workflow-report`
- `num route`
- `num serve`
- `num serve-once`
- `num new`
- `num completions zsh`
- `num lsp`
- `num.toml` source and entry manifest loading for CLI project commands;
- `[language]` manifest metadata for language version, compatibility policy,
  and manifest schema version;
- `num compat` language/schema compatibility reports for packages and loaded
  path/local-registry dependencies;
- `num migrate` dry-run/write migration reports for legacy or partial
  `[language]` manifest metadata;
- `num upgrade-version` dry-run/write reports for safe `[language].version`
  and optional `[project].version` upgrades;
- `num version` CLI/language/schema version reporting;
- fixture-backed CLI compatibility matrix coverage for current manifests,
  legacy missing-language manifests, schema `0` migration, future schema
  rejection, future language rejection, and project-version upgrades;
- local filesystem registry publish/list/install workflow for package
  development and private package sharing;
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
- file-backed workflow state store;
- file-backed append-only audit JSONL sink;
- memory and file-backed secret stores with redacted secret value debug output;
- memory and file-backed workflow event queues;
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
- typed JSON request body decoding for route inputs;
- connector execution interface and static connector registry;
- manifest-configured process connector execution for `num run`, `num test`,
  `num trace`, `num cost-report`, `num route`, `num serve`, and
  `num serve-once`;
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
- OpenAPI JSON import for a focused connector-contract subset:
  `components.schemas`, `paths`, operation parameters, JSON request bodies, and
  JSON success response schemas.
- SQL schema import for a focused database-contract subset: `CREATE TABLE`
  columns, common scalar types, nullable columns, inline primary keys, table
  types, single-column table-level primary keys, and basic database connector
  methods.
- runtime in-memory database connector executor for generated `database`
  connector methods: `list_<table>`, `find_<table>_by_<primary_key>`, and
  `insert_<table>`.
- TypeScript connector implementation SDK generation for visible `.num`
  structs, aliases, enums, and connector method signatures.

Not yet implemented:

- managed connector hosting;
- generated network-native runtime clients;
- connector SDK targets beyond TypeScript declarations;
- connector authentication/secrets;
- generated database clients;
- preemptive async cancellation for timed-out connector calls;
- full OpenAPI coverage such as YAML input, security schemes, `allOf`/`oneOf`,
  callbacks, links, and generated runtime clients.
- full SQL/database import coverage such as foreign-key relation typing,
  indexes, migrations, dialect-specific features, and composite primary-key
  finder methods.

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
- binding runtime request tenant context into static service-route policy checks;
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
- queued workflow event processing through the lifecycle engine;
- state transition validation for terminal and compensation states.

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
- richer workflow fixtures beyond deterministic connector return-value mocks.

### Cost and Limits

Action declarations can carry raw `cost` and `timeout` metadata. Runtime data
types include cost-limit and timeout errors, and the demo interpreter enforces
action timeout budgets, pre-authorizes declared action cost against active
budget scopes, and records successful action costs in a cost ledger.

Not yet implemented:

- distributed rate-limit persistence across runtime processes;
- distributed timeout propagation;
- interactive and persisted cost dashboards;
- per-model or per-connector cost accounting.

## Not Covered Yet

Major full-spec areas not implemented in v0.1.0:

- remote package registry HTTP/service APIs;
- git package fetching;
- lockfile transitive dependency pinning;
- complete standard library;
- hardened production HTTP server runtime;
- production database connectors;
- document processing APIs;
- full OpenAPI import coverage beyond the current connector-contract subset;
- full database schema import coverage beyond the current SQL contract subset;
- async/await;
- structured concurrency;
- actor model;
- clustered distributed queue ownership, retries, and worker leasing;
- external secrets manager integration such as Vault/KMS/cloud secret stores;
- tenant isolation enforcement across every non-workflow runtime surface;
- locale-specific sanitizer catalogs and externally configured sanitizer packs;
- interactive debugger and IDE debug adapter;
- interactive workflow dashboard;
- interactive audit dashboard;
- interactive cost dashboard;
- cloud/container deployment execution model;
- CI/CD integrations beyond local deployment bundle generation and release
  packaging;
- external-language interop;
- remote registry and git package imports;
- performance optimization strategy;
- automatic source migrations between language versions.

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
- language/schema compatibility checks.
- manifest migration tooling.
- manifest version upgrade tooling.
- fixture-backed manifest compatibility matrix coverage.

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
- compatibility matrix coverage for the current v0.1.0 manifest/schema
  surface;
- standard library.

### Planned Platform Work

- dashboards;
- deployment execution;
- package ecosystem;
- direct package dependency declarations and deterministic lockfile generation;
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

Use these commands to verify the documented v0.1.0 surface:

```bash
cargo test
cargo run -p num -- check examples/refund_workflow/src/main.num
cargo run -p num -- check examples/ai_agent/src/main.num
cargo run -p num -- check examples/policy_guard/src/main.num
cargo run -p num -- check examples/contract_driven_refund/src/main.num
cargo run -p num -- ir examples/refund_workflow/src/main.num
cargo run -p num -- run examples/refund_workflow/src/main.num
cargo run -p num -- test examples/refund_workflow
node examples/contract_driven_refund/backend/runtime-demo.js success
node examples/contract_driven_refund/backend/runtime-demo.js approval
```
