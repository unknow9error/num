# num Documentation

This directory documents the current v0.1.1 implementation of `num`: language,
compiler, runtime, CLI, editor tooling, examples, compatibility, and release
process. The current codebase is a production slice toward the full Num
specification, so the documentation separates implemented behavior from planned
language features.

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
- [Release process](../RELEASES.md) - package artifacts, release checklist, and
  compatibility rules.

## Supported Local Checks

Run these from the repository root:

```bash
cargo test
num check examples/refund_workflow/src/main.num
num check examples/refund_workflow/src
num check examples/ai_agent/src/main.num
num check examples/policy_guard/src/main.num
num check examples/contract_driven_refund/src/main.num
```

The demo runtime currently supports selected mocked workflows:

```bash
num run examples/refund_workflow/src/main.num
num route examples/refund_workflow/src POST /refunds
node examples/contract_driven_refund/backend/runtime-demo.js success
```
