# Release Process

This project publishes `num` through GitHub Releases. A release is not only a
binary upload: it is a compatibility checkpoint for the CLI, language version,
manifest schema, lockfile schema, docs, examples, and VS Code extension.

## Release Artifacts

Every tagged release should publish packages for:

- `linux-x64`
- `macos-x64`
- `macos-arm64`
- `windows-x64`

Each package contains:

- `bin/num` or `bin/num.exe`
- the `num` language server through the same binary
- a packaged VS Code extension under `vscode-extension/`
- `install.sh` for macOS/Linux
- `install.ps1` for Windows
- package-local installation notes

## Versioning

Use `vMAJOR.MINOR.PATCH` Git tags for public releases.

The CLI version is read from `language/crates/num-cli/Cargo.toml`. A release
must keep these public contracts aligned:

- CLI package version
- `num version` output
- `CHANGELOG.md`
- `docs/spec-coverage.md`
- compatibility notes for manifest and lockfile schemas

The current v0.1.x line is a pre-1.0 language/toolchain line. Patch releases may
add tooling and diagnostics, but they should not silently break existing
v0.1.x projects that declare compatible manifest metadata.

## Maintainer Checklist

Before tagging:

```bash
cargo test
npm --prefix vscode-extension ci
npm --prefix vscode-extension run compile

cargo build -p num
export PATH="$PWD/target/debug:$PATH"

num version
num check examples/refund_workflow
num lint examples/refund_workflow
num test examples/refund_workflow
num check examples/ai_agent
num check examples/policy_guard
num check examples/contract_driven_refund
```

Then update:

- `CHANGELOG.md` with the release date and user-visible changes
- `README.md` if install, commands, or status changed
- `docs/spec-coverage.md` for implemented/planned boundary changes
- `docs/migration-guides.md` when compatibility or migrations changed

Create and push the tag:

```bash
git tag v0.1.2
git push origin v0.1.2
```

The release workflow builds packages, uploads artifacts, and publishes a GitHub
Release. Release notes are generated from `CHANGELOG.md` for the tagged version
when a matching heading exists.

## Manual Package Build

Build the current platform package locally:

```bash
bash scripts/package-current-platform.sh
```

Packages are written to `dist/releases/`. Local packages are useful for smoke
testing installers before pushing a public tag.

## Compatibility Rules

When a release changes a public format, document it in `CHANGELOG.md` and keep
the CLI explicit:

- manifest compatibility through `num compat`
- manifest/source rewrites through `num migrate`
- version upgrade planning through `num upgrade-version`
- lockfile validation and migration through `num lock`
- machine-readable version contracts through `num version --json`

If a future CLI cannot read a project, it should fail with a clear diagnostic
instead of silently accepting incompatible language, manifest, or lockfile
metadata.
