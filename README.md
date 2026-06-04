# num

`num` is a Rust-built programming language foundation for safe AI automations,
backend services, workflows, policies, auditability, permissions, data
provenance, privacy labels, cost-aware execution, and reversible external
actions.

The language file extension is `.num`.

## Status

This repository implements the v0.1.0 production slice of the larger Num
language specification. It is a working compiler/runtime/editor foundation, not
the full industrial language described in the full technical specification.

For the precise implementation boundary, see
[docs/spec-coverage.md](docs/spec-coverage.md).

## Repository Layout

- `language/` - Rust implementation of the language toolchain:
  `num-compiler`, `num-runtime`, `num-lsp`, and the `num` CLI.
- `vscode-extension/` - VS Code extension for `.num` files.
- `examples/` - Standalone example projects written in `num`.
- `docs/` - Language and architecture documentation.

## Quick Start

```bash
cargo run -p num -- check examples/refund_workflow/src/main.num
cargo run -p num -- check examples/refund_workflow/src
cargo run -p num -- fmt examples/refund_workflow/src/main.num
cargo run -p num -- ir examples/refund_workflow/src/main.num
cargo run -p num -- run examples/refund_workflow/src/main.num
cargo run -p num -- route examples/refund_workflow/src POST /refunds
```

## Documentation

- [Documentation index](docs/index.md)
- [Language reference](docs/language-reference.md)
- [CLI reference](docs/cli.md)
- [Architecture](docs/architecture.md)
- [Diagnostics](docs/diagnostics.md)
- [Examples](docs/examples.md)
- [Project configuration](docs/project-configuration.md)
- [VS Code extension](docs/vscode-extension.md)
- [Specification coverage](docs/spec-coverage.md)

## Packaging

Build an installer archive for the current platform:

```bash
bash scripts/package-current-platform.sh
```

The archive is written to `dist/releases/` and includes the `num` CLI, the VS Code
extension VSIX, and platform installer scripts. Cross-platform release artifacts
are produced by `.github/workflows/release.yml` for Linux, macOS, and Windows.

## Current Production Slice

This repository contains a working production-grade foundation for:

- lexer with spans and diagnostics;
- parser and AST for modules, imports, permissions, roles, policies, types,
  enums, functions, workflows, actions, connectors, and services;
- structured and generic type declarations, structural aliases, union aliases,
  and nominal `Brand<T,"Tag">` aliases;
- semantic checker for duplicate declarations, unknown permissions/types,
  privacy leaks, secrets logging, AI uncertainty, required permissions, and
  high-risk action audit;
- multi-file program checks that resolve `use <module.path>` against checked
  `.num` files;
- linked multi-file entry modules for demo `run`, `route`, `serve`, and
  `serve-once` commands;
- `num.toml` project manifests with `source` and `entry` fields used by CLI
  project commands;
- typed connector method schemas with call arity/type/result checks;
- typed service route schemas with input, route permission, and body checks;
- expression AST/parser for literals, calls, member access, arithmetic,
  ordering comparisons, equality comparisons, and boolean operators;
- expression type checks for connector results, struct fields, arithmetic,
  `Money<C>` rules, ordering, equality, and boolean operands;
- guarded `Option<T>.value` access through `if option.is_some` and
  `if option.is_none { ... } else { ... }` flow checks;
- `Some(...)` inference and `Some(...)` / `None` constructors in typed
  `Option<T>` contexts;
- guarded `Result<T,E>.value` and `.error` access through `is_ok` / `is_err`
  flow checks;
- `Ok(...)` and `Err(...)` constructors in typed `Result<T,E>` contexts;
- `Result<T,E>?` unwrap and compatible error propagation checks;
- non-generic branded alias constructors such as `PaymentId("pay_1")`;
- `var` assignment statements with mutability and type checks;
- enum payload variants, unique enum variant constructor inference,
  context-typed enum variant constructors, and payload match bindings;
- enum and union alias `match` statements with arm validation,
  exhaustiveness checks, simple binding narrowing for union arms, and
  structured union member destructuring;
- typed direct `fn`/`workflow`/`action` call arity, argument, and result checks;
- typed `return` checks against declared callable result types;
- exhaustive return-path analysis for typed callables;
- IR lowering;
- CLI commands: `check`, `fmt`, `ir`, `run`, `route`, `serve`, `serve-once`,
  `deploy`, `compat`, `new`, `completions`, and `lsp`;
- runtime contracts for workflow/action/audit/cost state;
- file-backed workflow state store and audit JSONL sink;
- workflow lifecycle engine with persisted start/wait/resume/complete/fail/
  compensate/cancel transitions;
- a lightweight interpreter for demo workflows, service route dry-runs, and a
  persistent HTTP service route listener with typed JSON body decoding;
- connector execution interface, static registry, manifest-configured process
  connector execution, and a demo connector executor for bundled examples;
- action execution wrapper and demo interpreter support for timeout/retry
  metadata, idempotency replay, and cost-limit checks;
- runtime cost ledger with demo interpreter pre-authorization and charging of
  successful action `cost` metadata;
- workflow, function, and service `budget` metadata enforced by hierarchical
  demo interpreter budget scopes;
- workflow and service `rate limit` metadata enforced by the demo interpreter
  rate limiter;
- scripted CLI debugger over runtime trace events;
- deployment plan artifact generation from checked projects;
- manifest language/schema version compatibility checks;
- LSP diagnostics, completions, hover, formatting, and document symbols;
- VS Code syntax, snippets, commands, and language configuration;
- separate example projects.

Major features from the full Num specification that are not implemented yet
include a complete expression type checker, event-driven workflow runner, real
network-native connector SDKs, remote package registry APIs, dashboard,
interactive debugger, distributed workflow engine, full standard library,
broader OpenAPI/database imports, async runtime, automatic migrations, and
deployment execution.
