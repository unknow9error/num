# num Roadmap

This roadmap tracks the path from the current compiler/runtime/tooling
foundation toward the full `.num` language and platform described in the
technical specification.

## Now

- Keep the compiler, runtime, CLI, examples, VS Code extension, and docs green.
- Preserve compatibility metadata through `num.toml`, `num compat`,
  `num upgrade-version`, `num lock`, and `num deploy`.
- Keep generated lockfiles schema-versioned and rejected by older CLIs when
  they require unsupported lockfile formats.
- Keep lockfile migration dry-runs and writes available for released schema
  changes.
- Preserve resolved git dependency commits in lockfiles and declared git
  selectors in deploy metadata.
- Keep manifest compatibility, migration, and version-upgrade behavior covered
  by fixture-backed CLI matrix tests.
- Keep version upgrade tooling graph-aware for resolved package dependencies.
- Keep source migration rewrites deterministic and reviewable as more language
  versions are introduced.
- Keep migration guides and fixtures aligned for every released migration rule.
- Grow the local filesystem package registry into a stable package ecosystem
  foundation through publish/list/install workflows, package metadata, and
  integrity checks, including lockfile content-hash pins for resolved registry
  packages.
- Use GitHub issues and pull requests for each meaningful platform slice.

## Near Term

- Distributed workflow execution:
  - multi-worker coordination on top of the file-backed queue runner;
  - clustered queue sharding beyond file-backed worker leases;
  - tenant-aware state transitions.
- Connector platform:
  - broader connector SDK language targets;
  - auth/secrets binding;
  - generated runtime clients;
  - process connector hardening beyond timeout enforcement and basic error
    taxonomy.
- Deployment story:
  - checked deploy plans;
  - local/CI artifact generation;
  - container/runtime target profile mapping;
  - checked environment validation metadata.
- Observability:
  - structured runtime events;
  - workflow/audit/cost dashboards;
  - debugger integration points.

## Later

- Remote package registry publish/download service.
- Remote registry download service and production git auth/cache hardening.
- Broader automatic source rewrite rules between language versions.
- Full standard library.
- Hardened production HTTP runtime.
- Broader OpenAPI and SQL import coverage.
- Released-version backward compatibility matrix as new language versions are
  introduced.

## Definition of Done for Major Features

- Compiler support with diagnostics.
- Runtime behavior or explicit runtime boundary.
- CLI command or integration path.
- Docs and examples.
- Tests covering success and failure paths.
- GitHub issue/PR history showing design and verification.
