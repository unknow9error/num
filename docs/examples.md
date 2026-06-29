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
num check examples/scalar_validation/src/main.num
num check examples/security_hashing/src/main.num
num check examples/configured_sanitizer_pack/src/main.num
num check examples/map_set_collections/src/main.num
num check examples/queue_stack_stream/src/main.num
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

## `scalar_validation`

Path: `examples/scalar_validation/src/main.num`

Demonstrates:

- `validate_email`, `validate_url`, `validate_uuid`, and
  `validate_phone_number`;
- validation of private untrusted user input before external API calls;
- preservation of `UserInput` provenance and `private` privacy labels after
  validation;
- a narrow policy that only allows private trusted user input to leave through
  `ExternalApi`.

Useful command:

```bash
num check examples/scalar_validation/src/main.num
```

## `configured_sanitizer_pack`

Path: `examples/configured_sanitizer_pack/src/main.num`

Demonstrates:

- project-defined sanitizer packs in `num.toml`;
- pack composition through `sanitize(raw, "plain_text+strict_latin_identifier")`;
- lowercasing and identifier-only character validation for private untrusted
  user input.

Useful command:

```bash
num check examples/configured_sanitizer_pack/src/main.num
```

## `map_set_collections`

Path: `examples/map_set_collections/src/main.num`

Demonstrates:

- `Set<Text>` permission accumulation with `set_empty`, `set_insert`, and
  `set_contains`;
- `Map<Text, Bool>` metadata updates with `map_empty`, `map_insert`,
  `map_contains`, and `map_get`;
- pure collection operations that return updated values instead of mutating
  bindings.

Useful command:

```bash
num check examples/map_set_collections/src/main.num
```

## `queue_stack_stream`

Path: `examples/queue_stack_stream/src/main.num`

Demonstrates:

- FIFO event handling with `Queue<Text>`;
- LIFO rollback ordering with `Stack<Text>`;
- synchronous stream inspection with `Stream<Text>`, `stream_next`, and
  `stream_advance`;
- pure ordered-collection operations that return updated values.

Useful command:

```bash
num check examples/queue_stack_stream/src/main.num
```

## `security_hashing`

Path: `examples/security_hashing/src/main.num`

Demonstrates:

- `hash_sha256_hex` for deterministic lowercase hexadecimal digests;
- `hash_sha256_base64` for compact base64 digests;
- explicit `Text` and `Bytes` input boundaries;
- keeping hashed values as derived data rather than treating hashes as password
  storage or automatic declassification.

Useful command:

```bash
num check examples/security_hashing/src/main.num
```

## `xml_bytes_payload`

Path: `examples/xml_bytes_payload/src/main.num`

Demonstrates:

- explicit `Text -> Bytes` and base64 `Text -> Bytes` boundaries;
- `Bytes` length and SHA-256 fingerprinting without dumping raw binary data;
- explicit `Text -> Xml` validation and `Xml -> Text` formatting;
- using `Bytes` and `Xml` as fields in a structured import payload.

Useful command:

```bash
num check examples/xml_bytes_payload/src/main.num
```

## `document_metadata_route`

Path: `examples/document_metadata_route/src/main.num`

Demonstrates:

- `Document` as metadata-only route input;
- reading `id`, `name`, `mime_type`, `size_bytes`, `source`, `privacy`, and
  `trust` fields;
- routing private document metadata through an explicit policy-checked external
  audit call;
- keeping file parsing, OCR, and PDF/DOCX extraction outside the first
  stdlib slice.

Useful command:

```bash
num check examples/document_metadata_route/src/main.num
```

## `pdf_docx_metadata`

Path: `examples/pdf_docx_metadata/src/main.num`

Demonstrates:

- `Pdf` and `Docx` wrappers over `Document` metadata;
- `pdf_parse_metadata(document, bytes)` for safe PDF page-count metadata;
- `docx_parse_metadata(document, bytes)` for stored-ZIP DOCX test fixtures;
- `pdf_metadata(...)` and `docx_metadata(...)` for trusted metadata fixtures;
- preserving source/privacy/trust metadata from the original `Document`.

Useful command:

```bash
num check examples/pdf_docx_metadata/src/main.num
```

## `datetime_deadlines`

Path: `examples/datetime_deadlines/src/main.num`

Demonstrates:

- parsing UTC ISO `DateTime` text at an input boundary;
- parsing `Duration<Hour>` values from explicit hour strings;
- computing route deadlines with `DateTime +/- Duration<Hour>`;
- comparing timestamps and auditing canonical ISO output.

Useful command:

```bash
num check examples/datetime_deadlines/src/main.num
```

## `decimal_arithmetic`

Path: `examples/decimal_arithmetic/src/main.num`

Demonstrates:

- parsing user/config text into exact `Decimal` values;
- same-type decimal arithmetic for invoice totals;
- formatting `Decimal` back to canonical text for audit output.

Useful command:

```bash
num check examples/decimal_arithmetic/src/main.num
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
- generated TypeScript declarations for JavaScript/TypeScript consumers;
- generated Python stubs for process connector implementations.

Useful commands:

```bash
num check examples/connector_echo_pipeline
num connector probe examples/connector_echo_pipeline echo.reply --arg '"hello"' --json
num connector-sdk examples/connector_echo_pipeline \
  --out examples/connector_echo_pipeline/generated/connectors.d.ts
num connector-sdk examples/connector_echo_pipeline \
  --language python \
  --out examples/connector_echo_pipeline/generated/connectors.py
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
