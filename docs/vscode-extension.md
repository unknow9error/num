# num VS Code Extension

The VS Code extension lives in `vscode-extension/` and contributes language
support for `.num` files.

## Features

Implemented in v0.2.0:

- `.num` language registration;
- TextMate syntax highlighting;
- bracket/comment language configuration;
- snippets for workflows, functions, actions, permissions, roles, policies,
  types, enums, `require`, and transactions;
- LSP diagnostics across sibling `.num` modules and open editor buffers;
- completions across sibling `.num` modules and open editor buffers;
- hover support across sibling `.num` modules and open editor buffers;
- go-to-definition across sibling `.num` modules and open editor buffers;
- document symbols;
- formatting through the `num fmt` command;
- status bar updates;
- output channel for command results.

## Commands

Command palette entries:

- `Num: Check Current File`
- `Num: Format Current File`
- `Num: Restart Language Server`
- `Num: Create New Project`

Editor context menu entries for `.num` files:

- `Num: Check Current File`
- `Num: Format Current File`

## Settings

### `num.cliPath`

Path to the `num` executable.

Default: empty string.

When empty, the extension tries workspace-local debug binaries first:

- `<workspace>/target/debug/num`
- `<workspace>/language/target/debug/num`

If neither exists, it falls back to `num` from `PATH`.

### `num.lsp.trace.server`

Boolean flag for writing language server protocol traces to the `num` output
channel.

Default: `false`.

## Development

Install dependencies:

```bash
npm --prefix vscode-extension ci
```

Compile:

```bash
npm --prefix vscode-extension run compile
```

Package generation is handled by:

```bash
bash scripts/package-current-platform.sh
```

The package script compiles TypeScript and uses `@vscode/vsce` to build a
`.vsix`.

## Current Boundary

Implemented:

- editor integration over the current compiler and CLI;
- program-aware diagnostics, completions, hover, and definitions for `.num`
  files in the same source directory;
- command execution by spawning the configured `num` binary;
- language server launch through `num lsp`.

Not implemented yet:

- extension-managed installation of the CLI outside release installers;
- multi-root project graph awareness;
- advanced refactors;
- IDE debugger integration; scripted CLI debugging is available through
  `num debug`;
- test explorer integration.
