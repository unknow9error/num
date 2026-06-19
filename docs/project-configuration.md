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

`num lock [project-dir|file]` writes a deterministic `num.lock` beside the
manifest. `num lock --check` validates the lockfile schema, and
`num lock --migrate` plans safe lockfile schema migrations before applying them
with `--write`. The lockfile records the workspace package plus direct and
transitive path/local-registry dependencies when those packages can be resolved
locally. Resolved package entries include language/schema compatibility metadata
and sorted dependency edges. Local-registry package entries also include a
`content_hash` pin from `.num-package.json` metadata, or from the computed
package hash when metadata has not been written yet. Git dependencies are
checked out into a project-local `.num-git` cache during locking, and their
lock entries pin the resolved commit SHA. Registry dependencies without a
configured local registry root remain metadata-only entries.

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
package identity, language version, manifest schema, content hash, metadata
path, and file counts. `num registry install <name> <version> --registry
<registry-root> --to <install-root>` verifies registry metadata when present,
then copies a package into a local vendor-style directory and writes the same
metadata next to the installed package. Existing publish/install targets require
`--replace` before they are overwritten.

### `[registry]`

Example:

```toml
[registry]
path = "../num-registry"
```

`path` points to a local filesystem registry root. If omitted, commands use the
`NUM_REGISTRY_PATH` environment variable when registry dependencies are present.

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
{ "$enum": "RiskLevel.Low" }
{ "$uncertain": { "$enum": "RiskLevel.Low" }, "confidence": 0.92 }
```

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
```

Supported fields:

- `target` - deployment target label, such as `local`, `container`, or a cloud
  environment name;
- `service` - preferred service entrypoint for service deployments;
- `region` - optional deployment region label;
- `artifact` - default path for deployment plan output.

`num deploy` validates the project and renders these values together with the
compiled workflows, actions, service routes, connectors, dependencies, runtime
metadata, environment validation metadata, target profile metadata, deployment
warnings, and security metadata. Target profiles classify `local`,
`container`/`docker`/`oci`, `kubernetes`/`k8s`,
`cloud`/`aws`/`gcp`/`azure`, `bare-metal`/`systemd`/`host`, and custom targets,
then record the expected external execution boundary, required artifacts, and
target-specific validation result. Container targets recommend
`[deployment].service`; Kubernetes and cloud targets require
`[deployment].service` and `[deployment].region`; bare-metal targets require
`[deployment].service`, recommend `[deployment].region` as a host inventory
label, and generate `deploy/num.service` plus `deploy/num.env` as runbook
artifacts. Custom targets record that execution needs a custom runner.
`num deploy --apply` also materializes a local/CI deployment bundle. By default,
the bundle directory is derived from `artifact` by removing the file extension;
`--dir <artifact-dir>` overrides that path, and `--replace` allows an existing
bundle directory to be overwritten.

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
- image publishing, cluster credential management, SSH/host provisioning,
  `systemctl` execution, and cloud rollout execution.
