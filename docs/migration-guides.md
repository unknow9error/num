# num Migration Guides

This document records released migration behavior for `num` projects. Migration
rules are intentionally deterministic and reviewable: run commands without
`--write` first, inspect the JSON/text plan, then apply.

## 0.1.x Source Modules

Current language version: `0.1.0`.

### Explicit Module Declarations

Modern `.num` files should declare an explicit module path:

```num
module app.main

workflow main() {
}
```

Legacy files that omit `module` can be migrated with:

```bash
num migrate <project-dir|file> --source --json
num migrate <project-dir|file> --source --write
```

The rewrite inserts a `module` declaration before the first non-comment,
non-blank source line and preserves leading `//` comments. The module path is
derived from the file path relative to `[project].source`:

- `src/main.num` -> `module main`
- `src/workflows/refund-flow.num` -> `module workflows.refund_flow`

Write mode is rejected when the source graph has blocking compiler diagnostics.
Fix those diagnostics first, rerun the dry-run report, then apply.

### Manifest Metadata

Legacy manifests without `[language]` metadata can be migrated with:

```bash
num migrate <project-dir|file> --json
num migrate <project-dir|file> --write
```

The manifest migration inserts the current language version, compatibility
policy, and manifest schema metadata. Schema `0` manifests are upgraded to the
current schema; manifests that require a future schema are rejected.
