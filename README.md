# num

[![CI](https://github.com/unknow9error/num/actions/workflows/ci.yml/badge.svg)](https://github.com/unknow9error/num/actions/workflows/ci.yml)
[![Version](https://img.shields.io/github/v/tag/unknow9error/num?label=version)](https://github.com/unknow9error/num/tags)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](vscode-extension/LICENSE)

`num` is a Rust-built programming language and toolchain for safe AI
automations, auditable backend workflows, permissioned services, policy-aware
data movement, cost-aware execution, and reversible external actions.

The language file extension is `.num`.

```num
module examples.refund_workflow

workflow process_refund(request: RefundRequest) budget 100 KZT rate limit 60 per 1m {
    require Permission.ViewBilling for current_user

    let payment = payments.find(request.payment_id)

    if request.amount > payment.amount {
        reject("Refund amount is greater than payment amount")
        return
    }

    let risk: Uncertain<RiskLevel> = ai.assess_refund_risk(request)

    if risk.confidence < 0.85 {
        require_human_approval(
            action: "issue_refund",
            reason: "Low AI confidence"
        )
        return
    }

    complete_refund(payment, request)
}
```

## Status

This repository implements the v0.3.0 production slice of the larger Num
language specification. It includes a working compiler, CLI, runtime foundation,
language server, VS Code extension, package/deploy tooling, and example
projects.

It is not yet the full industrial platform described in the complete technical
specification. The exact implemented boundary is tracked in
[docs/spec-coverage.md](docs/spec-coverage.md).

## Install

Download the latest package for your platform from
[GitHub Releases](https://github.com/unknow9error/num/releases). Each release
contains:

- the `num` CLI and language server;
- a VS Code extension package;
- macOS/Linux and Windows installer scripts;
- shell completion support for bash, fish, and zsh on macOS/Linux.

macOS and Linux:

```bash
tar -xzf num-<version>-<platform>.tar.gz
cd num-<version>-<platform>
./install.sh
```

Windows PowerShell:

```powershell
Expand-Archive .\num-<version>-windows-x64.zip
cd .\num-<version>-windows-x64
.\install.ps1
```

From source:

```bash
cargo build -p num
export PATH="$PWD/target/debug:$PATH"
num version
```

Shell completions:

```bash
num completions bash > ~/.local/share/bash-completion/completions/num
num completions fish > ~/.config/fish/completions/num.fish
num completions zsh > ~/.zfunc/_num
```

## Quick Start

Check, lint, test, inspect, run, and route the bundled refund workflow:

```bash
num check examples/refund_workflow
num lint examples/refund_workflow
num test examples/refund_workflow
num ir examples/refund_workflow/src/main.num
num run examples/refund_workflow
num route examples/refund_workflow POST /refunds
```

Create a new project:

```bash
num new hello-num
cd hello-num
num check .
num test .
```

Materialize a deploy bundle from a project manifest:

```bash
num deploy examples/refund_workflow --check --json
num deploy examples/refund_workflow --apply
```

## What num Provides Today

- Parser, AST, formatter, IR lowering, diagnostics, and semantic checks for the
  implemented `.num` language surface.
- Multi-file module checks with `use <module.path>` resolution.
- Permissions, roles, policies, privacy labels, provenance labels, audit
  requirements, high-risk action checks, and AI uncertainty checks.
- Typed connectors, services, routes, functions, workflows, actions, enums,
  branded aliases, `Option<T>`, `Result<T,E>`, and selected expression typing.
- Lightweight runtime execution for demo workflows, service route dry-runs,
  tests, traces, scripted debugging, audit reports, workflow reports, and cost
  reports.
- File-backed workflow state, event queue draining, worker leases, retries,
  dead-letter handling, and lease heartbeat refresh.
- Runtime connector egress context propagation for distributed data-leak
  controls across process and external connector boundaries.
- Manifest compatibility, migration, version upgrade, lockfile, local registry,
  package integrity, TypeScript/Python connector SDKs, OpenAPI import, SQL
  import, and deployment artifact commands.
- VS Code syntax, snippets, diagnostics, completion, hover, formatting, and
  document symbols through the bundled language server.

## Commands

The `num` CLI is the main public surface:

```bash
num check <file.num|dir>
num lint <file.num|dir>
num fmt <file.num>
num test <file.num|dir>
num run <file.num|dir> [--json]
num route <file.num|dir> <METHOD> <PATH>
num serve <file.num|dir> [addr] [service]
num deploy [project-dir|file] [--check|--apply|--kubernetes-dry-run]
num compat [project-dir|file] [--json]
num migrate [project-dir|file] [--write] [--json]
num upgrade-version [project-dir|file]
num bench [fixture-root] [--json] [--iterations N]
num release-plan [CHANGELOG.md] [--json]
num lock [project-dir|file] [--check|--migrate]
num registry <publish|list|index|install>
num workflow <enqueue|drain|lease-heartbeat>
num connector <probe>
num connector-sdk [project-dir|file]
num import openapi <json> [module]
num import sql <schema.sql> [module]
num lsp
```

See [docs/cli.md](docs/cli.md) for the complete command reference.

## Documentation

- [Documentation index](docs/index.md)
- [Language reference](docs/language-reference.md)
- [CLI reference](docs/cli.md)
- [Architecture](docs/architecture.md)
- [Diagnostics](docs/diagnostics.md)
- [Examples](docs/examples.md)
- [Project configuration](docs/project-configuration.md)
- [Migration guides](docs/migration-guides.md)
- [VS Code extension](docs/vscode-extension.md)
- [Specification coverage](docs/spec-coverage.md)
- [Release process](RELEASES.md)

## Repository Layout

- `language/` - Rust workspace for `num-compiler`, `num-runtime`, `num-lsp`,
  and the `num` CLI.
- `vscode-extension/` - VS Code extension for `.num` files.
- `examples/` - Standalone `.num` projects used as executable documentation.
- `docs/` - Language, CLI, architecture, diagnostics, and compatibility docs.
- `scripts/release/` - Installer scripts and release package documentation.
- `.github/` - CI, release workflow, issue templates, PR template, and
  ownership metadata.

## Releases

Release artifacts are published through
[GitHub Releases](https://github.com/unknow9error/num/releases) from `v*` tags.
Each release is expected to include Linux, macOS Intel, macOS Apple Silicon, and
Windows packages.

The project uses `CHANGELOG.md` as the source of truth for release notes and
keeps language, manifest, and lockfile compatibility visible through
`num version`. Every user-visible PR should classify its changelog entry under
`Major`, `Minor`, or `Patch`; `num release-plan --json` computes the current
unreleased SemVer bump from those sections.

For maintainer steps and compatibility rules, see [RELEASES.md](RELEASES.md).

## Contributing

Contributions should keep the language surface, runtime behavior, CLI,
diagnostics, docs, examples, and release notes aligned.

Start with:

- [CONTRIBUTING.md](CONTRIBUTING.md)
- [ROADMAP.md](ROADMAP.md)
- [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)
- [SECURITY.md](SECURITY.md)

Run the local gate before opening a pull request:

```bash
cargo test
npm --prefix vscode-extension ci
npm --prefix vscode-extension run compile

cargo build -p num
export PATH="$PWD/target/debug:$PATH"

num check examples/refund_workflow
num lint examples/refund_workflow
num test examples/refund_workflow
num check examples/ai_agent
num check examples/policy_guard
num check examples/contract_driven_refund
```

## License

The repository is MIT licensed. The VS Code extension package includes the
license file at [vscode-extension/LICENSE](vscode-extension/LICENSE).
