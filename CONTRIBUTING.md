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

cargo build -p num
export PATH="$PWD/target/debug:$PATH"

for dir in examples/*; do
  num check "$dir"
  num lint "$dir"
  num test "$dir"
  num compat "$dir"
done
```

Use focused tests while iterating, but keep the full gate green before pushing.

## Change Shape

- Keep compiler, runtime, CLI, docs, and examples aligned.
- Prefer small modules over growing `main.rs` or `lib.rs`.
- Add tests for new syntax, semantic checks, runtime behavior, and CLI behavior.
- Document the implemented boundary honestly in `docs/spec-coverage.md`.
- Update `docs/diagnostics.md` when adding or changing diagnostic codes.
- Update `CHANGELOG.md` for user-visible behavior, tooling, docs, packaging, or
  compatibility changes.
- Avoid committing build artifacts, generated release archives, local editor
  paths, or dependency folders.

## Pull Requests

Each PR should explain:

- what changed;
- which specification or roadmap area it advances;
- what verification was run;
- known remaining limitations.

## Releases

Release changes should follow [RELEASES.md](RELEASES.md). Keep the CLI version,
`num version`, changelog, docs, examples, package contents, and GitHub release
notes aligned.
