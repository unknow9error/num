# num Documentation

This directory documents the current v0.1.0 implementation of `num`.

This directory documents the language, compiler, runtime, and tools. The current
codebase is a production slice toward the full Num specification, so the
documentation separates implemented behavior from planned language features.

## Start Here

- [Language reference](language-reference.md) - syntax and safety semantics
  accepted by the current parser and semantic checker.
- [CLI reference](cli.md) - supported commands and local workflows.
- [Architecture](architecture.md) - crates, compilation pipeline, runtime
  contracts, and editor integration.
- [Diagnostics](diagnostics.md) - compiler diagnostic codes and what they mean.
- [Examples](examples.md) - the included example projects and what each one
  demonstrates.
- [Project configuration](project-configuration.md) - current `num.toml`
  fields used by generated projects and examples.
- [VS Code extension](vscode-extension.md) - editor commands, settings, and
  language features.
- [Specification coverage](spec-coverage.md) - implementation status against the
  full Num technical specification.

## Supported Local Checks

Run these from the repository root:

```bash
cargo test
cargo run -p num -- check examples/refund_workflow/src/main.num
cargo run -p num -- check examples/refund_workflow/src
cargo run -p num -- check examples/ai_agent/src/main.num
cargo run -p num -- check examples/policy_guard/src/main.num
cargo run -p num -- check examples/contract_driven_refund/src/main.num
```

The demo runtime currently supports selected mocked workflows:

```bash
cargo run -p num -- run examples/refund_workflow/src/main.num
cargo run -p num -- route examples/refund_workflow/src POST /refunds
node examples/contract_driven_refund/backend/runtime-demo.js success
```
