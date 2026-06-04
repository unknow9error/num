# num Roadmap

This roadmap tracks the path from the current compiler/runtime/tooling
foundation toward the full `.num` language and platform described in the
technical specification.

## Now

- Keep the compiler, runtime, CLI, examples, VS Code extension, and docs green.
- Preserve compatibility metadata through `num.toml`, `num compat`, `num lock`,
  and `num deploy`.
- Grow the local filesystem package registry into a stable package ecosystem
  foundation through publish/list/install workflows.
- Use GitHub issues and pull requests for each meaningful platform slice.

## Near Term

- Distributed workflow execution:
  - multi-worker coordination on top of the file-backed queue runner;
  - idempotent event processing;
  - worker ownership and retry semantics;
  - tenant-aware state transitions.
- Connector platform:
  - connector SDK contract;
  - auth/secrets binding;
  - generated clients;
  - process connector hardening.
- Deployment story:
  - checked deploy plans;
  - local/CI artifact generation;
  - container/runtime target mapping;
  - environment validation.
- Observability:
  - structured runtime events;
  - workflow/audit/cost dashboards;
  - debugger integration points.

## Later

- Remote package registry publish/download service.
- Git dependency checkout and lockfile transitive pinning.
- Schema versioning and automatic migrations.
- Full standard library.
- Hardened production HTTP runtime.
- Broader OpenAPI and SQL import coverage.
- Backward compatibility test matrix.

## Definition of Done for Major Features

- Compiler support with diagnostics.
- Runtime behavior or explicit runtime boundary.
- CLI command or integration path.
- Docs and examples.
- Tests covering success and failure paths.
- GitHub issue/PR history showing design and verification.
