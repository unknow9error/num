# Changelog

## Unreleased

### Minor

- Added a manifest-configured JavaScript callable-module boundary for local
  JS/TS interop, including runtime context passing, structured JS error
  mapping, and a runnable `examples/javascript_interop` fixture.
- Added a static async task model for the 0.4.0 language slice: `async <expr>`
  now produces `Task<T>`, `await <task>` unwraps `Task<T>`, and the checker
  rejects `await` on non-task values and bare async tasks without owners.
- Added a connector echo pipeline example that ties together a `.num`
  connector contract, manifest process connector execution through Python,
  direct connector probing, and generated TypeScript implementation types for
  JavaScript consumers.
- Added a Python target for `num connector-sdk`, generating dataclasses,
  type aliases, connector protocols, and egress context stubs for process
  connector implementations.
- Added `num bench` for checked-in lexer/parser/checker benchmark fixtures with
  stable JSON output suitable for CI artifacts.
- Added opt-in `num bench --compare <baseline.json>` regression gates with
  percentage and absolute parse/check timing thresholds for CI.
- Added bare-metal deployment bundles with a systemd-style service unit draft,
  host environment template, runtime store expectations, and operator runbook
  warnings.
- Added Kubernetes deploy dry-run handoffs that print or write generated
  deployment/service resources with namespace, image, port, and secret-reference
  validation before real cluster apply support.
- Added explicit Docker registry image publish handoff metadata for deploy
  plans, including registry/image/tag strategy fields, credentials references,
  and `deploy/image-publish.json` artifacts for container and Kubernetes
  bundles.
- Added Jenkins deploy-gate templates to external deployment bundles, running
  policy, cost, and security gates before materializing the deploy artifact.
- Added GitLab CI deploy-gate templates and an explicit `num deploy --check`
  mode for CI validation before deployment bundle packaging.
- Added a versioned `num.deploy_check.v1` JSON read model and GitHub Actions
  deploy-gate template for policy, cost, security, and packaging gates.
- Added a versioned `num.audit_dashboard.v1` JSON read model for
  `num audit-report --json`, including stable audit counts, optional
  connector/route/workflow dimensions, time-window metadata, and redacted
  failure details.
- Added a versioned `num.workflow_dashboard.v1` JSON read model for
  `num workflow-report --json`, including stable workflow counts, workflow
  lifecycle summaries, pending-compensation flags, and best-effort recent
  failure/audit summaries.
- Bound service-route policy checks to the runtime request tenant for
  tenant-scoped allow/deny rules in `num route`, `num serve`, and
  `num serve-once`.
- Added a `RateLimitStore` boundary with tenant/actor/subject keys, shared
  in-memory runtime handles, and a file-backed local adapter for rate-limit
  enforcement across runtime instances.
- Added stdlib scalar validators for `Email`, `Url`, `Uuid`, and `PhoneNumber`,
  including compile-time diagnostics for invalid literals and runtime errors for
  invalid dynamic text input.
- Added explicit SHA-256 stdlib hashing helpers for `Text`/`Bytes` inputs with
  hex and base64 output functions for deterministic non-password hashing.
- Added deterministic `DateTime` and `Duration<Hour>` stdlib helpers for UTC ISO
  parsing/formatting, hour-duration parsing/formatting, arithmetic, and runtime
  comparisons.
- Extended `num migrate --source` with an idempotent rewrite that normalizes
  legacy workflow/service `rate_limit` metadata spelling to `rate limit`.
- Extended the `num.cost_dashboard.v1` read model with request and correlation
  dimensions plus fixture coverage for action, AI/model, and connector costs.
- Added workflow lifecycle fixtures covering wait/resume audit checkpoints,
  saga compensation audits, and idempotent action replay behavior.

### Patch

- Preserved simple OpenAPI pagination conventions as review-required connector
  metadata comments for limit/offset, page/pageSize, cursor, and next-link
  response hints during `num import openapi` generation.
- Added review-required OpenAPI import permission candidates and policy
  placeholders for generated connector operations with security or
  private-field hints.
- Added a versioned `num.cost_dashboard.v1` JSON read model for
  `num cost-report --json`, including stable cost dashboard totals, raw entries,
  time-window fields, and documented conditional dimensions.
- Added deterministic SQL composite primary-key finder methods and in-memory
  database lookup support for `num import sql` generated contracts.
- Preserved SQL inline and table-level foreign-key relationships as generated
  relation hint comments during `num import sql` generation.
- Preserved OpenAPI security schemes and operation requirements as generated
  metadata comments during `num import openapi` generation.
- Preserved OpenAPI callbacks and links as unsupported metadata comments during
  `num import openapi` generation.
- Added bash and fish shell completions alongside the existing zsh script.
- Added target-specific deploy plan validation for required/recommended
  `[deployment]` fields, including JSON metadata in materialized bundles.
- Added YAML and YML input support to `num import openapi` while preserving
  existing JSON import behavior.
- Added `num fmt --write` and `num fmt --check` modes with stable directory
  traversal for `.num` files while preserving stdout formatting for single-file
  usage.
- Wired `[security].tenant_isolation` into `num route`, `num serve`, and
  `num serve-once` so service-route requests build tenant-aware security context,
  reject cross-tenant access, and record tenant failures in audit output.
- Normalized service-route error responses for `num route`, `num serve`, and
  `num serve-once` with stable JSON `kind`/`code` fields, request identifiers,
  and redacted connector/internal messages.
- Redacted `Secret<T>` values and secret-like connector failures from runtime
  trace/debug JSON, structured connector errors, process connector JSON
  conversion, and service error responses.

## 0.3.0 - 2026-06-17

### Minor

- Runtime connector calls now carry a distributed egress context with
  connector/method identity, scoped capability, actor, tenant, request and
  correlation identifiers, policy decision marker, and declared argument
  source/privacy/trust labels.
- Manifest-configured process connectors receive the egress context in their
  stdin JSON payload so external connector processes can enforce, audit, and
  propagate Num data-leak controls beyond one runtime instance.
- Generated TypeScript connector SDKs include `NumConnectorEgressContext` and
  optional context parameters for connector implementations.

## 0.2.0 - 2026-06-16

### Minor

- Workflow lease heartbeat refresh for file-backed durable workers through
  `num workflow lease-heartbeat`.
- Validated local registry metadata indexes through `num registry index`.
- Process connector probes through `num connector probe`.
- Runnable deploy artifact scaffolds for container and Kubernetes targets.
- SemVer release planning from changelog sections through `num release-plan`.
- SemVer-aware local registry version ordering and `latest` install resolution
  through `num registry`.

### Patch

- GitHub-facing project polish: professional README, CI workflow, release
  process guide, improved PR template, and changelog-backed GitHub release
  notes.
- Release workflow validation now runs once before packaging, and macOS Intel
  packaging uses the supported `macos-15-intel` runner.
- README badges now use CI, tag-based version, and license signals so the
  project header does not show stale release workflow or empty-release errors.
- Release artifact upload now publishes only generated archive files instead
  of matching package staging directories.
- Release v0.2.0 version metadata and package artifacts.

## 0.1.1 - 2026-06-07

### Added

- Structured runtime connector errors with stable `code`, `message`, and
  `retryable` fields.
- Machine-readable `runtime_error.connector` payloads in `num run --json` and
  `num debug --json`.
- Silent JSON stdout for runtime reporting commands: `run --json`, `trace`,
  `debug --json`, and `cost-report --json`.
- `lockfile_schema` in `num version` text and JSON output.

### Changed

- Process connector timeout, invalid JSON, non-zero exit, and process lifecycle
  failures are classified as structured runtime errors.
- `num version` is now the public version surface for CLI, language, manifest
  schema, and lockfile schema contracts.

### Compatibility

- `0.1.0` manifests using `compatibility = "minor"` remain compatible with the
  `0.1.1` CLI.
- Manifest schema stays at `1`.
- Lockfile schema stays at `1`.
