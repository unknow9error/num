# Changelog

## Unreleased

### Minor

- Workflow lease heartbeat refresh for file-backed durable workers through
  `num workflow lease-heartbeat`.
- Validated local registry metadata indexes through `num registry index`.
- Process connector probes through `num connector probe`.
- Runnable deploy artifact scaffolds for container and Kubernetes targets.
- SemVer release planning from changelog sections through `num release-plan`.

### Patch

- GitHub-facing project polish: professional README, CI workflow, release
  process guide, improved PR template, and changelog-backed GitHub release
  notes.
- Release workflow validation now runs once before packaging, and macOS Intel
  packaging uses the supported `macos-15-intel` runner.

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
