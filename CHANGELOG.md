# Changelog

## Unreleased

### Patch

- Added a runtime encryption envelope boundary with `Encrypted<T>` type
  recognition, provider-backed encrypt/decrypt helpers, a deterministic test
  provider, redacted envelope logging, and decrypted secret label metadata.

## 0.4.8 - 2026-07-03

### Patch

- Added a runtime metrics export boundary with OpenTelemetry-compatible metric
  names and attributes, no-op/test exporters, and safe-by-default tenant/actor
  label policy controls.

## 0.4.7 - 2026-07-03

### Patch

- Added a provider-neutral edge deploy target boundary with Fetch handler
  scaffolding, edge runtime limitations, blocking validation for local
  filesystem/process-connector dependencies, and explicit provider handoff
  documentation.

## 0.4.6 - 2026-07-03

### Patch

- Added provider-neutral serverless deploy bundle scaffolding with a handler
  entrypoint, runtime manifest, environment template, connector placeholders,
  and explicit unsupported-provider boundaries.

## 0.4.5 - 2026-07-03

### Patch

- Added manifest-level AI model alias/provider metadata and deploy-check
  validation for provider credential environment names without reading secret
  values.

## 0.4.4 - 2026-07-03

### Patch

- Made `num release-plan` return a deterministic no-op plan for clean
  post-release `Unreleased` sections while still rejecting unclassified entries.

## 0.4.3 - 2026-07-02

### Patch

- Added a provider-neutral external secret backend adapter boundary, manifest
  metadata for secret backend references, deploy-plan validation of provider
  credential environment names, and a deterministic stub backend for tests.
- Added a first Vault external secret backend adapter with token-auth metadata,
  KV v2 response mapping, missing/denied/unavailable/invalid-response errors,
  mocked tests, and an `http://` fixture/dev transport boundary.

## 0.4.2 - 2026-07-02

### Patch

- Added `num import sql --plan` to compare two supported SQL schema snapshots
  and print deterministic text or JSON migration-plan reports.
- Added `num import openapi` support for component-level `oneOf` schemas whose
  variants are local `$ref`s to representable object schemas, generating
  deterministic Num union aliases and review comments for unsupported shapes.

## 0.4.1 - 2026-07-01

### Patch

- Extended `Option<T>` and `Result<T,E>` flow narrowing across simple
  early-return and `reject(...)` guards, while keeping fallthrough branches
  conservative.
- Added a first route-scoped policy condition, `when route <METHOD> "<PATH>"`,
  for service-route data-flow rules.
- Added a safe LSP module rename for `module ...` declarations and matching
  `use ...` imports across sibling `.num` files.
- Added simple OpenAPI `allOf` object-schema merging for generated component
  types, including conflict comments for unsupported field merges.
- Preserved simple SQL `CREATE INDEX` and `CREATE UNIQUE INDEX` metadata as
  generated table comments during schema import.

## 0.4.0 - 2026-07-01

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
- Added first-class `Bytes` and `Xml` runtime values with explicit stdlib
  constructors/formatters, typed JSON and process-connector boundaries,
  bounded diagnostics, and an import-payload example.
- Added a metadata-only `Document` stdlib runtime value with typed field access,
  HTTP/process JSON conversion, connector SDK shapes, persistence support, and
  a policy-checked document route example.
- Added first-slice `Pdf` and `Docx` metadata wrappers with explicit byte
  parsers, structured malformed-file errors, runtime/connector/persistence
  conversion, and a PDF/DOCX metadata example without text extraction.
- Added first-slice `Spreadsheet` and `SpreadsheetSheet` metadata wrappers with
  safe sheet-level XLSX metadata parsing, connector/persistence conversion, and
  a spreadsheet metadata example without formula execution or cell import.
- Added first-slice `Image` and `OcrResult` metadata wrappers with safe PNG/JPEG
  dimension parsing, deterministic OCR handoff values, connector/persistence
  conversion, and an image/OCR metadata example without OCR provider execution.
- Added deterministic `DateTime` and `Duration<Hour>` stdlib helpers for UTC ISO
  parsing/formatting, hour-duration parsing/formatting, arithmetic, and runtime
  comparisons.
- Added exact `Decimal` parsing, formatting, runtime arithmetic, comparison, and
  same-type checker coverage without falling back to `Float`.
- Added explicit `ExchangeRate<From, To>` values and `convert_money` helpers for
  audited `Money<C>` currency conversion while preserving mixed-currency
  arithmetic rejection.
- Added an explicit git dependency auth/cache policy for `num lock`, including
  non-interactive git execution, offline reuse for cached `rev` pins, and
  documentation that credentials stay out of lockfiles and deploy metadata.
- Added project-defined sanitizer packs in `num.toml`, including manifest
  validation, runtime `sanitize(value, "pack")` resolution, pack composition,
  and a configured sanitizer example project.
- Added minimal `Map<K,V>` and `Set<T>` stdlib helpers, runtime values, JSON
  conversion, and a permission/metadata collection example before the
  Queue/Stack/Stream slice.
- Added the first `Queue<T>`, `Stack<T>`, and `Stream<T>` stdlib slice with
  typed pure helpers, runtime values, JSON conversion, and an ordered-work
  example without promising clustered or async streaming semantics.
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
