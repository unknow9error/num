# num Examples

The `examples/` directory contains independent `.num` projects. Each project has
a `num.toml` and `.num` sources under `src/`. Small examples may be a single
`src/main.num`; larger examples can split declarations across modules imported
by `src/main.num`.

Run checks from the repository root:

```bash
num check examples/refund_workflow/src/main.num
num check examples/refund_workflow/src
num check examples/ai_agent/src/main.num
num check examples/policy_guard/src/main.num
num check examples/contract_driven_refund/src/main.num
num check examples/async_tasks/src/main.num
num test examples/workflow_lifecycle
num check examples/connector_echo_pipeline
```

## `refund_workflow`

Path: `examples/refund_workflow/src/`

Demonstrates:

- multi-file modules and imports;
- permissions and roles;
- data sharing policy;
- connector declarations;
- structured types and enums;
- `Money<KZT>`;
- private data labels;
- high-risk refund action;
- action-level permission requirements;
- timeout metadata;
- rollback metadata;
- AI uncertainty with `Uncertain<RiskLevel>`;
- human approval branch;
- `transaction saga`;
- audit events.

Useful commands:

```bash
num check examples/refund_workflow/src/main.num
num check examples/refund_workflow/src
num ir examples/refund_workflow/src/main.num
num run examples/refund_workflow/src/main.num
num route examples/refund_workflow/src POST /refunds
```

The `run` command executes this example through mocked connectors.

## `ai_agent`

Path: `examples/ai_agent/src/main.num`

Demonstrates:

- AI classification as `Uncertain<Intent>`;
- confidence threshold branch;
- human handoff action;
- permission-gated reply action;
- private and untrusted user input labels;
- audit events for support workflows.

Useful command:

```bash
num check examples/ai_agent/src/main.num
```

## `policy_guard`

Path: `examples/policy_guard/src/main.num`

Demonstrates:

- explicit allow/deny data policy;
- private user data;
- internal database data;
- deriving a public report;
- allowed public flow to `external.analytics`;
- audit event for export.

Useful command:

```bash
num check examples/policy_guard/src/main.num
```

## `contract_driven_refund`

Path: `examples/contract_driven_refund/src/main.num`

Demonstrates:

- `.num` as the source of truth for a risky refund workflow;
- a generated contract consumed by backend runtime code;
- generated TypeScript connector boundaries;
- backend connectors that implement effects without owning workflow order;
- AI confidence gating;
- permission enforcement;
- action audit;
- saga rollback.

Useful commands:

```bash
num check examples/contract_driven_refund/src/main.num
node examples/contract_driven_refund/backend/runtime-demo.js success
node examples/contract_driven_refund/backend/runtime-demo.js approval
node examples/contract_driven_refund/backend/runtime-demo.js denied
node examples/contract_driven_refund/backend/runtime-demo.js rollback
```

## `async_tasks`

Path: `examples/async_tasks/src/main.num`

Demonstrates:

- `Task<T>` as the static type for async work;
- `async <expr>` producing a task;
- `await <task>` unwrapping the task result;
- rejection of assigning a task where the awaited value is expected.

Useful command:

```bash
num check examples/async_tasks/src/main.num
```

## `workflow_lifecycle`

Path: `examples/workflow_lifecycle/src/main.num`

Demonstrates:

- `test workflow` fixtures for lifecycle audit checkpoints;
- saga compensation through rollback metadata;
- idempotency-key replay of repeated action calls;
- file-backed workflow event commands for wait/resume/complete transitions.

Useful commands:

```bash
num test examples/workflow_lifecycle
num workflow enqueue examples/workflow_lifecycle start wf_lifecycle wait_resume_checkpoint --event-id evt-start
num workflow enqueue examples/workflow_lifecycle wait wf_lifecycle --event-id evt-wait
num workflow enqueue examples/workflow_lifecycle resume wf_lifecycle --event-id evt-resume
num workflow drain examples/workflow_lifecycle --max-events 10 --json
num workflow-report examples/workflow_lifecycle --json
```

## `connector_echo_pipeline`

Path: `examples/connector_echo_pipeline/`

Demonstrates:

- `.num` connector declarations as the source contract;
- manifest-configured process connector execution;
- a Python connector implementation that reads runtime JSON from stdin;
- `num connector probe` as the connector smoke test;
- generated TypeScript declarations for JavaScript/TypeScript consumers.

Useful commands:

```bash
num check examples/connector_echo_pipeline
num connector probe examples/connector_echo_pipeline echo.reply --arg '"hello"' --json
num connector-sdk examples/connector_echo_pipeline \
  --out examples/connector_echo_pipeline/generated/connectors.d.ts
num run examples/connector_echo_pipeline
```

## `javascript_interop`

Path: `examples/javascript_interop/`

Demonstrates:

- a `.num` connector method as the typed boundary;
- a `[javascript]` manifest binding to a local CommonJS module;
- runtime JSON argument conversion and connector egress context delivery to JS;
- structured JS error mapping through the existing connector error boundary;
- an explicit policy that permits the private `UserInput` flow into the JS
  callable.

Useful commands:

```bash
num check examples/javascript_interop
num run examples/javascript_interop --json
```

## Adding an Example

Use this structure:

```text
examples/<name>/
  num.toml
  src/access.num
  src/domain.num
  src/connectors.num
  src/main.num
  src/<optional-module>.num
```

Keep examples focused on a single language capability. Add a matching command to
this file when the example is intended to be part of the supported surface.
