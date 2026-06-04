# Contributing to num

`num` is a compiler/runtime/tooling project. Contributions should keep the
language surface, runtime behavior, diagnostics, documentation, and examples in
sync.

## Development Setup

Requirements:

- Rust stable toolchain
- Node.js 20+
- npm

Install VS Code extension dependencies:

```bash
npm --prefix vscode-extension ci
```

## Verification

Run the full local gate before opening a pull request:

```bash
cargo test
npm --prefix vscode-extension run compile
for dir in examples/*; do
  cargo run -q --manifest-path language/Cargo.toml -p num -- check "$dir"
  cargo run -q --manifest-path language/Cargo.toml -p num -- lint "$dir"
  cargo run -q --manifest-path language/Cargo.toml -p num -- test "$dir"
  cargo run -q --manifest-path language/Cargo.toml -p num -- compat "$dir"
done
```

Use focused tests while iterating, but keep the full gate green before pushing.

## Change Shape

- Keep compiler, runtime, CLI, docs, and examples aligned.
- Prefer small modules over growing `main.rs` or `lib.rs`.
- Add tests for new syntax, semantic checks, runtime behavior, and CLI behavior.
- Document the implemented boundary honestly in `docs/spec-coverage.md`.
- Update `docs/diagnostics.md` when adding or changing diagnostic codes.
- Avoid committing build artifacts, generated release archives, local editor
  paths, or dependency folders.

## Pull Requests

Each PR should explain:

- what changed;
- which specification or roadmap area it advances;
- what verification was run;
- known remaining limitations.

