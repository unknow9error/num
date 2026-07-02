# num Migration Guides

This document records released migration behavior for `num` projects. Migration
rules are intentionally deterministic and reviewable: run commands without
`--write` first, inspect the JSON/text plan, then apply.

## 0.1.x Source Modules

Current language version: `0.4.3`.

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

### Legacy Rate Limit Metadata

Workflow and service rate limits now use the two-word metadata spelling
`rate limit`:

```num
workflow nightly() rate limit 2 per 1m {
}

service Billing rate limit 5 per 10s {
}
```

Legacy source that used `rate_limit` in workflow or service declaration headers
can be migrated with the same source migration commands:

```bash
num migrate <project-dir|file> --source --json
num migrate <project-dir|file> --source --write
```

The rewrite only targets declaration metadata before the opening `{`, so string
literals, comments, and parameter names are preserved. Running the migration
again reports the file as up to date.

### Manifest Metadata

Legacy manifests without `[language]` metadata can be migrated with:

```bash
num migrate <project-dir|file> --json
num migrate <project-dir|file> --write
```

The manifest migration inserts the current language version, compatibility
policy, and manifest schema metadata. Schema `0` manifests are upgraded to the
current schema; manifests that require a future schema are rejected.

### Lockfile Schema

Modern `num.lock` files declare their schema at the top:

```toml
version = 1
```

Legacy lockfiles that omit this header, or schema `0` lockfiles, can be planned
and migrated with:

```bash
num lock <project-dir|file> --migrate --json
num lock <project-dir|file> --migrate --write
```

The migration inserts or upgrades the top-level lockfile schema while preserving
the package entries. Lockfiles that require a future schema are rejected instead
of being rewritten by an older CLI.
