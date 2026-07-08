# num Project Configuration

`num.toml` is the project manifest for generated projects and examples.
The CLI loads it when checking or running a file/directory inside a project.
The manifest controls language compatibility, source discovery, dependencies,
runtime metadata, security policy mode, connectors, and deployment planning.

## Minimal Manifest

`num new <name>` creates:

```toml
[language]
version = "0.3.0"
compatibility = "minor"
manifest_schema = 1

[project]
name = "new-num-service"
version = "0.1.0"
source = "src"
entry = "src/main.num"

[dependencies]

[runtime]
workflow_store = "memory"
audit_store = "stdout"

[security]
policy_mode = "strict"
```

## Sections

### `[language]`

Current fields:

```toml
[language]
version = "0.3.0"
compatibility = "minor"
manifest_schema = 1
```

Meaning:

- `version` - language version the package was authored against.
- `compatibility` - compatibility policy for the current CLI. Supported values
  are `exact`, `minor`, and `major`.
- `manifest_schema` - numeric `num.toml` schema version.

Project commands validate this contract before compiling package source. The
check applies to direct and transitive path/local-registry dependencies too, so
an incompatible dependency cannot be silently compiled. `num compat
[project-dir|file] [--json]` prints compatibility reports, including structured
`status` and `reason` fields for incompatible manifests when `--json` is used,
and `num deploy` embeds the same language/schema metadata into the deployment
plan artifact.
`num migrate [project-dir|file] [--write]` can add missing `[language]`
metadata, fill partial language sections, and upgrade schema `0` manifests to
the current schema. `num migrate --source` provides deterministic source
migration over workspace `.num` files, including blocking compiler diagnostics
and per-file actions. The current source rewrite rule inserts missing explicit
`module` declarations based on manifest source-relative file paths.
`num upgrade-version [project-dir|file] [--write]` can
plan/apply a `[language].version` upgrade to the current CLI language version
and optionally bump `[project].version` with `--project <x.y.z>`. With
`--include-dependencies`, it reports upgrade readiness across resolved
path/local-registry dependency manifests; `--write-dependencies` applies
dependency language upgrades only when paired with `--write`. The CLI test
suite includes fixture projects for current manifests, legacy missing-language
manifests, schema `0` migration, future schema rejection, future language
rejection, project-version upgrade compatibility, and graph-aware dependency
upgrade planning. It also pins structured incompatible `num compat --json`
reports and released lockfile migration behavior for legacy missing-schema and
schema `0` lockfiles. Released migration behavior is documented in
[migration-guides.md](migration-guides.md).

### `[project]`

Current fields:

```toml
[project]
name = "refund-workflow"
version = "0.3.0"
source = "src"
entry = "src/main.num"
```

Meaning:

- `name` - project/package name;
- `version` - project version.
- `source` - directory containing `.num` source files, relative to the manifest;
- `entry` - entry `.num` source file, relative to the manifest.

When `source` or `entry` is omitted, the CLI defaults to `src` and
`src/main.num`.

### `[dependencies]`

Example:

```toml
[dependencies]
std = "0.3.0"
shared = { path = "../shared", version = "0.3.0" }
banking = { git = "https://example.com/banking.num.git", version = "1.4.0" }
```

Supported dependency forms:

- `name = "version"` for a registry-style dependency;
- `name = { path = "../package", version = "x.y.z" }` for a local package;
- `name = { git = "https://...", version = "x.y.z" }` for a git package
  reference.
- git package references may also include `rev`, `tag`, `branch`, or `ref`;
  `num lock` preserves that selector in the deterministic source label.
  Git URLs are handed to the installed `git` binary as written, so local paths,
  `file://`, `https://`, and SSH URLs follow host Git configuration. Locking
  disables interactive terminal prompts; CI and deploy environments must
  provide credentials through configured Git credential helpers, tokens, or SSH
  agents. `num.lock` and deploy metadata record selectors and resolved commit
  SHAs, never credentials.

`num lock [project-dir|file]` writes a deterministic `num.lock` beside the
manifest. `num lock --check` validates the lockfile schema, and
`num lock --migrate` plans safe lockfile schema migrations before applying them
with `--write`. The lockfile records the workspace package plus direct and
transitive path/local-registry dependencies when those packages can be resolved
locally. Resolved package entries include language/schema compatibility metadata
and sorted dependency edges. Local-registry package entries also include a
`content_hash` pin from `.num-package.json` metadata, or from the computed
package hash when metadata has not been written yet. `num lock --check`
re-resolves available local-registry packages and rejects registry lock entries
whose `content_hash` is missing or no longer matches the resolved package. Git
dependencies are checked out into a project-local `.num-git` cache during
locking, and their lock entries pin the resolved commit SHA. Existing
`.num-git` checkouts are reused offline only when an explicit `rev` pin is
already present in the cache; `tag`, `branch`, and `ref` selectors fetch from
`origin` before checkout because those selectors can move. Registry
dependencies without a configured
local registry root remain metadata-only entries.

Path dependencies are included in program checks and runtime compilation. Their
`.num` files are loaded from the dependency package's own `[project].source`
directory, so modules can be imported by their declared module path:

```num
module app.main
use shared.domain
```

Registry-style dependencies are resolved from a local filesystem registry when
`[registry].path` or `NUM_REGISTRY_PATH` is set. The registry layout is:

```text
registry-root/
  shared/
    0.3.0/
      num.toml
      src/
```

Registry and git dependencies participate in program checks and runtime
compilation, including transitive dependency graphs. Git dependencies are
resolved through the same project-local `.num-git` cache used by `num lock`.

`num registry publish [project-dir|file] --registry <registry-root>` publishes a
validated package into that layout and writes `.num-package.json` metadata with
schema, package identity, language/manifest metadata, per-file hashes, and a
package content hash. `num registry list --registry <registry-root>` prints
available packages. `num registry index --registry <registry-root> --json`
validates package metadata and emits an API-ready package index containing
package identity, language version, manifest schema, content hash, integrity
fields, metadata path, remote-compatible endpoint paths, and file counts. `num
registry install <name> <version> --registry <registry-root> --to
<install-root>` verifies registry metadata when present, then copies a package
into a local vendor-style directory and writes the same metadata next to the
installed package. Existing publish/install targets require `--replace` before
they are overwritten.

The remote registry protocol declared by `index --json` is intentionally
read-only:

- `GET /v1/packages/{name}` for package metadata;
- `GET /v1/packages/{name}/versions` for version listings;
- `GET /v1/packages/{name}/versions/{version}` for version metadata and
  integrity fields;
- `GET /v1/packages/{name}/versions/{version}/download` for version downloads;
- `GET /v1/packages/{name}/blobs/{sha256}` for content-addressed downloads.

The current CLI does not publish to, authenticate with, or download from a
remote registry service. `[registry].path` and `NUM_REGISTRY_PATH` still point
to the local filesystem registry used by checks, lockfiles, and installs.

### `[registry]`

Example:

```toml
[registry]
path = "../num-registry"
```

`path` points to a local filesystem registry root. If omitted, commands use the
`NUM_REGISTRY_PATH` environment variable when registry dependencies are present.

### `[secrets.<backend>]`

Projects can declare external secret-store backends without placing secret
values in `num.toml`. A backend id is referenced by `secret://<backend>/<name>`
at runtime boundaries that accept external secret references.

```toml
[secrets.vault]
provider = "vault"
address = "https://vault.internal:8200"
mount = "secret"
path_prefix = "apps/billing"
auth_method = "token"
token_env = "VAULT_TOKEN"
credential_env = ["VAULT_ADDR", "VAULT_TOKEN"]

[secrets.kms]
provider = "kms"
credential_env = ["KMS_KEYRING"]
optional = true
```

Supported fields:

- `provider` - provider family label such as `vault`, `kms`, or
  `cloud-secrets`. `vault` has the first secret-store runtime adapter slice.
  KMS has an encryption-provider boundary and deterministic fake provider for
  tests; cloud-specific KMS clients still remain provider-client work.
- `address` - Vault base address metadata. Address values are not secret
  values, but deploy plans still record them separately from credential
  presence.
- `mount` - Vault KV v2 mount name, such as `secret`.
- `path_prefix` or `path` - optional provider-side path prefix metadata for
  operators and future runtime wiring.
- `auth_method` - provider auth method. The current Vault runtime adapter
  supports `token`.
- `token_env` - environment variable name that should contain the Vault token
  at runtime. Deploy plans record the name and whether it is present, never the
  token value.
- `credential_env` - environment variable names required by the external
  provider adapter. `num deploy` records names and presence only; it never reads
  or serializes credential values.
- `optional` - marks the backend as advisory in deploy checks when credential
  environment names are missing.

The initial Vault runtime adapter maps KV v2 JSON responses under
`data.data` and distinguishes missing secrets, permission denial, unavailable
Vault responses, and invalid response shapes. Its bundled HTTP transport is
limited to `http://` fixture/dev endpoints for deterministic tests; production
Vault HTTPS transport and additional auth methods are future provider-client
work.

The KMS encryption boundary uses `Encrypted<T>` envelopes and provider-neutral
key ids such as `alias/billing/refunds` or a cloud provider's non-secret key
resource name. Runtime configuration and deploy artifacts should carry only key
ids, algorithm metadata, and credential environment variable names. Raw key
bytes, PEM material, and credential values are not accepted as key ids and must
stay outside `num.toml`, generated artifacts, and logs. The bundled fake KMS
provider is deterministic test infrastructure, not a production cloud KMS
client.

`num deploy` and `num deploy --check` include a `secrets` section in their JSON
plans. Missing credential environment variables for non-optional backends block
deploy checks; optional missing backends remain visible as
`optional-missing`.

### `[ai]`, `[ai.models.<alias>]`, and `[ai.scanners.<alias>]`

Projects can declare AI model aliases and provider metadata without hardcoding
provider details in `.num` source files. This is planning metadata only: real
provider execution and provider-specific policy routing are future runtime work.

```toml
[ai]
default_model = "fast-classifier"

[ai.models.fast-classifier]
provider = "openai"
model = "gpt-4.1-mini"
credential_env = ["OPENAI_API_KEY"]
timeout_ms = 5000
max_cost = "0.10 USD"

[ai.models.reasoner]
provider = "anthropic"
model = "claude-3-5-sonnet"
credential_env = ["ANTHROPIC_API_KEY"]
timeout_ms = 12000
max_cost = "0.50 USD"

[ai.scanners.prompt-guard]
provider = "fixture"
mode = "block"
block_threshold = "blocked"
audit_redaction = "redacted"
```

Supported fields:

- `[ai].default_model` or `[ai].default` - optional alias selected as the
  package default.
- `provider` - provider family label such as `openai`, `anthropic`, or a
  future internal provider name.
- `model` or `model_id` - provider model identifier for the alias.
- `credential_env` or `credentials_env` - environment variable names required
  by the provider adapter. Names are trimmed, sorted, and deduplicated.
- `timeout_ms` - default provider call timeout metadata in milliseconds.
- `max_cost` or `default_max_cost` - operator-facing cost metadata for the
  alias, stored as text until provider-specific pricing is implemented.
- `[ai.scanners.<alias>].provider` - scanner provider label. The built-in
  deterministic test fixture provider is `fixture`.
- `[ai.scanners.<alias>].mode` - scanner planning mode, such as `audit` or
  `block`.
- `[ai.scanners.<alias>].block_threshold` or `threshold` - optional textual
  decision threshold metadata for blocking scanners.
- `[ai.scanners.<alias>].audit_redaction` or `redaction` - audit redaction
  profile label. `redacted` is the default.

Unknown fields under `[ai]`, `[ai.models.<alias>]`, and
`[ai.scanners.<alias>]` are ignored for forward compatibility, so manifests can
carry provider-specific future metadata before the CLI understands it.

`num deploy` includes an `ai` section in JSON plans with aliases, provider
labels, model ids, scanner catalog entries, timeout/cost metadata, and
credential environment name presence. `num deploy --check` blocks when a
declared model has missing credential environment variables. It records names,
scanner metadata, and environment variable presence only; it never reads or
serializes credential values.

### `[sanitizer_packs.<name>]`

Projects can define named text sanitizer packs in the manifest and use them at
runtime with `sanitize(value, "pack_name")`. Pack specs can compose built-in and
project packs with `+` or `,`, for example
`sanitize(raw, "plain_text+strict_latin_identifier")`.

```toml
[sanitizer_packs.strict_latin_identifier]
extends = ["plain_text"]
max_chars = 32
lowercase = true
allowed_chars = "identifier"
```

Supported fields:

- `extends` - array of built-in or project pack names to compose first.
- `trim` - trims leading/trailing whitespace when `true`.
- `strip_control_chars` - removes control characters when `true`.
- `max_chars` - maximum character count; `0` is rejected.
- `lowercase` - lowercases text when `true`.
- `collapse_whitespace` - collapses whitespace runs when `true`.
- `allowed_chars` - one of `alpha_hyphen`, `latin_identifier`, `email`,
  `identifier`, `person_name`, or `name`.

The runtime rejects unknown pack names, recursive `extends`, unknown
`allowed_chars` values, and impossible limits with manifest diagnostics before
running project code.

### `[connectors]`

External connector methods can be backed by local processes. The key is the
declared connector method name and the value is either a command line string or
an inline table:

```toml
[connectors]
"payments.find" = { command = "node", args = "connectors/payments-find.js" }
"payment_gateway.refund" = "node connectors/refund.js"
```

Commands run with the package root as their working directory by default.
Override it with `cwd` when connector scripts live under a dedicated operations
directory:

```toml
[connectors]
"mailer.send" = { command = "node", args = "send.js", cwd = "ops/mailer" }
```

Inline process connector tables can also set `timeout_ms` as a string value.
When present, the runtime kills the process and reports a connector error if
the command exceeds that wall-clock budget:

```toml
[connectors]
"payments.find" = { command = "node", args = "find.js", cwd = "ops/payments", timeout_ms = "2000" }
```

Use `num connector probe <project-dir|file> <connector.method> --arg <json>` to
validate a process binding directly. Probe calls do not fall back to demo
connectors, so they are the intended smoke test for real connector processes.

At runtime, `num run`, `num test`, `num trace`, `num cost-report`, `num route`,
`num serve`, and `num serve-once` use configured process connectors before
falling back to the built-in demo connector executor. The process receives JSON
on stdin:

```json
{ "method": "payments.find", "args": ["pay_1"] }
```

The process must write one JSON value to stdout. Objects become runtime structs
with field access support. Special object forms are available for richer values:

```json
{ "minor_units": 15000, "currency": "KZT" }
{ "$exchange_rate": true, "from": "USD", "to": "KZT", "rate": "450.25", "source": "NBK fixture" }
{ "$enum": "RiskLevel.Low" }
{ "$uncertain": { "$enum": "RiskLevel.Low" }, "confidence": 0.92 }
```

Embedded runtimes can also host connector implementations in process through
`StaticConnectorRegistry`. Register each supported `connector.method` name with
`register_with_context(...)` when the implementation needs the same egress
context that process and JavaScript connectors receive, or with `register(...)`
for legacy argument-only handlers. `registered_methods()` returns the sorted
method list exposed by that registry. Runtime selection is ordered: an embedded
caller can provide an in-process/generated-client registry, CLI project commands
then select JavaScript module bindings, process bindings, and finally the demo
executor; if no executor handles a declared connector method, Num reports the
structured `missing_implementation` connector error.

### `[javascript]`

Local JavaScript modules can back declared connector methods through a narrow
callable-module bridge. The key is the declared connector method name:

```toml
[javascript]
"profile.enrich" = { module = "interop/profile.cjs", export = "enrich", timeout_ms = "1500" }
```

`module` is resolved relative to the package root. `export` defaults to
`default`, `cwd` defaults to the package root, and `timeout_ms` uses the same
wall-clock timeout behavior as process connectors. Runtime commands call
JavaScript bindings before generic process connectors and demo connectors.

The JavaScript export receives one envelope with JSON-converted arguments and
the same connector egress context used by process connectors:

```js
exports.enrich = async ({ args, context }) => {
  return { "$type": "Profile", id: args[0], source: context.actor };
};
```

Exceptions become structured connector errors such as `js_exception` or
`js_export_missing` without raw stack traces by default. Use this for small
local JS/TS integration points. Prefer `[connectors]` plus `num connector-sdk`
when a production integration needs generated typed interfaces, auth/secrets,
or a separately hosted worker.

### `[security]`

Example:

```toml
[security]
policy_mode = "strict"
tenant_isolation = true

[security.jwt]
issuer = "https://issuer.example"
audience = "num-api"
algorithms = ["HS256"]
secret_env = "NUM_JWT_SECRET"
leeway_seconds = 30

[security.session]
cookie_name = "num_session"
secret_env = "NUM_SESSION_SECRET"
leeway_seconds = 30
```

Supported `policy_mode` values:

- `strict` - project commands run compiler checks plus lints, and warnings are
  blocking. This is the default for generated projects.
- `advisory` - project commands run compiler checks, but lints are only run by
  `num lint`.
- `off` - project commands run compiler checks without project policy lint
  enforcement.

`tenant_isolation` is parsed into package metadata. When enabled, workflow state
access is guarded with `TenantGuard`, and service-route commands (`num route`,
`num serve`, and `num serve-once`) reject requests whose runtime tenant context
does not match the service tenant. The current demo service tenant defaults to
`default`; omit the field or set it to `false` to preserve permissive demo
behavior while prototyping.

`[security.jwt]` enables fail-closed JWT verification for service routes.
The first slice verifies HS256 bearer tokens against configured `issuer`,
`audience`, `algorithms`, and a signing secret loaded from `secret_env`.
Verified `sub`, `tenant`, and `roles` claims populate the route
`SecurityContext`; role names are resolved against `.num` `role` declarations.
`[security.session]` enables fail-closed signed-cookie session verification for
service routes. The cookie value is a signed `payload.signature` token whose
payload contains minimal `id`, `actor`, `tenant`, `roles`, and `exp` claims.
The signature uses HMAC-SHA256 with a secret loaded from `secret_env`, and
verified session actor/tenant/role claims populate the route `SecurityContext`.
Configure only one service authentication provider at a time:
`[security.jwt]` or `[security.session]`.

Token minting, OAuth authorization-code flow, refresh tokens, and persistent
server-side session stores remain future security-provider work.

### `[runtime]`

Example:

```toml
[runtime]
workflow_store = "memory"
audit_store = "stdout"
```

Current fields used in examples:

- `workflow_store` - workflow state backend, either `memory` for demo
  interpreter commands or `file:<state-root>` for durable workflow commands;
- `audit_store` - audit sink backend, either `stdout` or
  `file:<events.jsonl>`.

`num workflow enqueue`, `num workflow drain`, `num workflow lease-heartbeat`,
and `num workflow-report` resolve these fields when their first argument is a
project directory or project file. Relative `file:` paths are resolved from the
package root. Explicit state-root arguments still use the direct
`<state-root>/events`, `<state-root>/workflows`, and
`<state-root>/audit/events.jsonl` layout. Demo interpreter commands `num run`,
`num test`, `num trace`, `num debug`, `num cost-report`, and
`num route` append report-compatible demo audit JSONL when `audit_store` is a
`file:` path. HTTP service commands `num serve` and `num serve-once` append
request-scoped audit JSONL to the same manifest-relative audit path, preserving
actor, tenant, request id, correlation id, service, method, and path metadata.
Tenant-isolation rejections are also recorded as service audit events so failed
cross-tenant access attempts remain visible to audit reports.
`num deploy` includes the same runtime values in the generated deployment plan.

### `[environment]`

Example:

```toml
[environment]
required = ["PAYMENTS_API_KEY", "SMTP_TOKEN"]
optional = ["NUM_LOG_LEVEL"]
```

Supported fields:

- `required` - environment variables that must be present before executing the
  deployment target;
- `optional` - environment variables recorded in the deployment plan when
  present, without making the target incomplete.

`num deploy` checks these variables at plan time without reading or emitting
their values. The generated plan, materialized `manifest.json`, and runbook
include each variable name, whether it is present, and a `missing-required`
status when required variables are absent.

### `[deployment]`

Example:

```toml
[deployment]
target = "container"
service = "BillingApi"
region = "eu-west-1"
artifact = "dist/num-deploy.json"
registry = "ghcr.io/acme"
image = "billing-api"
tag_strategy = "version"
credentials_ref = "secret://docker/ghcr"
```

Supported fields:

- `target` - deployment target label, such as `local`, `container`, or a cloud
  environment name;
- `service` - preferred service entrypoint for service deployments;
- `region` - optional deployment region label;
- `artifact` - default path for deployment plan output;
- `registry` - optional container registry host/path for image publishing
  handoff metadata;
- `image` - optional image repository/name. If omitted while image publishing
  is configured, deploy planning defaults to `num-<project-name>`;
- `tag_strategy` - image tag strategy, currently `version` or `latest`;
- `credentials_ref` - secret-store reference for registry credentials. The
  deployment plan records this reference only and never stores credential
  values.

`num deploy` validates the project and renders these values together with the
compiled workflows, actions, service routes, connectors, dependencies, runtime
metadata, environment validation metadata, target profile metadata, deployment
warnings, and security metadata. Target profiles classify `local`,
`container`/`docker`/`oci`, `kubernetes`/`k8s`,
`cloud`/`aws`/`gcp`/`azure`, `serverless`/`function`/`functions`,
`edge`/`edge-runtime`/`edge-worker`/`worker`/`workers`,
`bare-metal`/`systemd`/`host`, and custom targets, then record the expected
external execution boundary, required artifacts, and target-specific validation
result. Container targets recommend `[deployment].service`; Kubernetes and
cloud targets require `[deployment].service` and `[deployment].region`;
serverless targets require `[deployment].service`, recommend
`[deployment].region` as a provider handoff label, and generate
`deploy/serverless/handler.mjs`, `deploy/serverless/manifest.json`, and
`deploy/serverless/env.example` as provider-neutral handoff artifacts;
edge targets require `[deployment].service`, recommend `[deployment].region` as
an edge placement/provider label, reject file-backed workflow/audit stores and
local process connectors, and generate `deploy/edge/worker.mjs`,
`deploy/edge/manifest.json`, and `deploy/edge/env.example` as provider-neutral
Fetch handler handoff artifacts;
bare-metal targets require `[deployment].service`, recommend
`[deployment].region` as a host inventory label, and generate
`deploy/num.service` plus `deploy/num.env` as runbook artifacts. When any image
publish field is configured for container or Kubernetes targets, `num deploy`
records an explicit image publish handoff under `image_publish` and
`deploy/image-publish.json`, including the exact image reference and
`credentials_ref`. Registry credentials remain behind the secret-store
boundary; build, login, tag, and push execution stays external.
Kubernetes targets can also be inspected with
`num deploy --kubernetes-dry-run`, which prints or writes the generated
deployment/service resources plus validation for namespace, image, ports, and
secret-like environment references before any cluster mutation exists. Custom
targets record that execution needs a custom runner. `num deploy --apply` also
materializes a local/CI deployment bundle. By default, the bundle directory is
derived from `artifact` by removing the file extension; `--dir <artifact-dir>`
overrides that path, and `--replace` allows an existing bundle directory to be
overwritten.

## Current Boundary

Implemented:

- generated `num.toml` skeleton via `num new`;
- example manifests for supported examples.
- manifest loading in `num check`, `num run`, `num test`, `num trace`,
  `num debug`, `num route`, `num serve`, and `num serve-once`;
- language/schema compatibility validation through `[language]` and
  `num compat`;
- manifest migration planning/application through `num migrate`;
- source migration planning/application through `num migrate --source`;
- manifest version upgrade planning/application through `num upgrade-version`;
- graph-aware dependency version upgrade planning/application through
  `num upgrade-version --include-dependencies`;
- fixture-backed manifest compatibility matrix tests for current, legacy, and
  future manifest/version cases;
- source directory and entry source selection through `[project]`.
- direct dependency declarations through `[dependencies]`;
- direct path dependency source discovery for module imports;
- local filesystem registry dependency source discovery for module imports;
- local filesystem registry publish/list/index/install through `num registry`,
  with package metadata and content-hash checks;
- deterministic `num.lock` generation through `num lock`;
- transitive `num.lock` pinning for resolved path/local-registry dependency
  graphs, including content-hash pins for resolved local-registry packages;
- deployment plan generation and local/CI deployment bundle materialization
  through `num deploy --apply`;
- container, Kubernetes, and bare-metal deploy scaffolds generated inside
  deployment bundles for compatible `[deployment].target` values;
- Kubernetes dry-run handoff output with namespace/image/port validation and
  secret-like environment reference warnings before real apply support;
- external secret backend manifest metadata, first Vault token-auth/KV v2
  adapter metadata, and deploy-check validation for provider credential
  environment variable names, without reading or emitting secret values;
- explicit container image publish handoff metadata and
  `deploy/image-publish.json` artifacts for configured registry/image targets,
  with credential values kept out of plain config;
- Jenkins deploy-gate templates for external deployment bundles, with policy,
  cost, and security gates before artifact materialization;
- GitLab CI deploy-gate templates for external deployment bundles, with
  explicit cache/artifact paths and `num deploy --check` before packaging;
- a versioned `num.deploy_check.v1` JSON read model for CI deploy validation,
  including policy, cost, security, target, environment, and image-publish gate
  status;
- deployment target profile classification plus target-specific validation
  status/errors/warnings;
- deployment environment validation metadata through `[environment]`.
- process connector timeout metadata in manifests, runtime execution, and
  deploy plans.
- idempotent durable workflow event replay through persisted event id metadata
  in workflow state.

Not implemented yet:

- remote registry package download/publish APIs;
- production git auth/cache policy;
- broader automatic source rewrite rules between language versions;
- image publishing execution, cluster credential management, Jenkins
  controller/agent provisioning, GitHub/GitLab runner provisioning, Kubernetes
  apply/API-server mutation, SSH/host provisioning, `systemctl` execution, and
  cloud rollout execution.
