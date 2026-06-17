# Changelog

## Unreleased

### Minor

- Added a static async task model for the 0.4.0 language slice: `async <expr>`
  now produces `Task<T>`, `await <task>` unwraps `Task<T>`, and the checker
  rejects `await` on non-task values and bare async tasks without owners.
- Added a connector echo pipeline example that ties together a `.num`
  connector contract, manifest process connector execution through Python,
  direct connector probing, and generated TypeScript implementation types for
  JavaScript consumers.
- Added `num bench` for checked-in lexer/parser/checker benchmark fixtures with
  stable JSON output suitable for CI artifacts.
- Added workflow lifecycle fixtures covering wait/resume audit checkpoints,
  saga compensation audits, and idempotent action replay behavior.

### Patch

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
