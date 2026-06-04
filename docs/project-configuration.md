# num Project Configuration

`num.toml` is the project manifest for generated projects and examples.
The CLI loads it when checking or running a file/directory inside a project.
The manifest controls language compatibility, source discovery, dependencies,
runtime metadata, security policy mode, connectors, and deployment planning.

## Minimal Manifest

`num new <name>` creates:

```toml
[language]
version = "0.1.0"
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
version = "0.1.0"
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
[project-dir|file] [--json]` prints the compatibility report, and `num deploy`
embeds the same language/schema metadata into the deployment plan artifact.
`num migrate [project-dir|file] [--write]` can add missing `[language]`
metadata, fill partial language sections, and upgrade schema `0` manifests to
the current schema.

### `[project]`

Current fields:

```toml
[project]
name = "refund-workflow"
version = "0.1.0"
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
std = "0.1.0"
shared = { path = "../shared", version = "0.2.0" }
banking = { git = "https://example.com/banking.num.git", version = "1.4.0" }
```

Supported dependency forms:

- `name = "version"` for a registry-style dependency;
- `name = { path = "../package", version = "x.y.z" }` for a local package;
- `name = { git = "https://...", version = "x.y.z" }` for a git package
  reference.

`num lock [project-dir|file]` writes a deterministic `num.lock` beside the
manifest. The lockfile currently records direct dependencies only.

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
    0.2.0/
      num.toml
      src/
```

Registry dependencies participate in program checks and runtime compilation,
including transitive registry dependencies. Git dependencies are currently
lockfile metadata only.

`num registry publish [project-dir|file] --registry <registry-root>` publishes a
validated package into that layout. `num registry list --registry
<registry-root>` prints available packages, and `num registry install <name>
<version> --registry <registry-root> --to <install-root>` copies a package into
a local vendor-style directory. Existing publish/install targets require
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

At runtime, `num run`, `num test`, `num trace`, `num cost-report`,
`num route`, `num serve`, and `num serve-once` use configured process
connectors before falling back to the built-in demo connector executor. The
process receives JSON on stdin:

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

`tenant_isolation` is parsed into package metadata. Runtime workflow state
access can be guarded with `TenantGuard`; full CLI-driven runtime wiring is
still a platform integration step.

### `[runtime]`

Example:

```toml
[runtime]
workflow_store = "memory"
audit_store = "stdout"
```

Current fields used in examples:

- `workflow_store` - intended workflow state backend;
- `audit_store` - intended audit sink backend.

These fields are parsed as metadata but are not loaded by the demo interpreter
in v0.1.0. `num deploy` includes them in the generated deployment plan.

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
metadata, and security metadata.

## Current Boundary

Implemented:

- generated `num.toml` skeleton via `num new`;
- example manifests for supported examples.
- manifest loading in `num check`, `num run`, `num test`, `num trace`,
  `num debug`, `num route`, `num serve`, and `num serve-once`;
- language/schema compatibility validation through `[language]` and
  `num compat`;
- manifest migration planning/application through `num migrate`;
- source directory and entry source selection through `[project]`.
- direct dependency declarations through `[dependencies]`;
- direct path dependency source discovery for module imports;
- local filesystem registry dependency source discovery for module imports;
- local filesystem registry publish/list/install through `num registry`;
- deterministic `num.lock` generation through `num lock`.
- deployment plan generation through `num deploy`.

Not implemented yet:

- remote registry package download/publish APIs;
- lockfile transitive dependency pinning;
- automatic source migrations between language versions;
- git dependency checkout;
- runtime backend selection from manifest values;
- deployment execution against cloud/container platforms.
