# num Language Reference

`num` is a statically checked workflow and backend language foundation focused
on operational safety. This reference documents the syntax and semantics
implemented by v0.3.0.

## Files

Source files use the `.num` extension.

Example:

```num
module examples.refund_workflow
```

`num.toml` is used by examples and generated projects to declare source/entry
paths and security behavior such as strict policy enforcement.
`num check <file.num>` checks the file together with other `.num` files in its
directory. `num check <directory>` checks all `.num` files under the directory
as one program. Both modes resolve module imports.

## Modules and Imports

### `module`

Declares the module path for a file.

```num
module examples.refund_workflow
```

The parser stores the path as text. During program checks, module paths must be
unique across the checked files.

### `use`

Records an import path.

```num
use company.billing
```

During program checks, imports are resolved against the module paths declared by
the checked `.num` files. Imported declarations are visible to the importing
module's semantic checker.

Example:

```num
module app.domain

type RefundRequest {
    reason: Text
}
```

```num
module app.main
use app.domain

workflow main(request: RefundRequest) {
    audit(request.reason)
}
```

`num check src/main.num` or `num check src` validates both files together. A
missing import emits `N1002`; a duplicate module path emits `N1001`.

## Permissions and Roles

### `permission`

Declares a permission name.

```num
permission IssueRefund
```

Permission names are module-scoped. Duplicate top-level declarations are
rejected.

### `role`

Groups permissions.

```num
role FinanceManager {
    allow ViewBilling
    allow IssueRefund
}
```

The checker verifies that every `allow` references a declared permission.

## Policies

Policies describe allowed or denied data flows.

```num
policy DataSharing {
    allow public PublicData -> ExternalApi
    deny regulated UserInput -> ExternalApi
    allow private UserInput -> external.crm
    allow private UserInput -> external.crm.send
    allow private UserInput -> ConnectorApi
    allow private UserInput -> mailer.send
    allow private BillingRecord -> ExternalApi for tenant tenant_a
    allow private verified UserInput -> ExternalApi
    allow private HttpBody -> external.audit when route POST "/documents"
}
```

Supported rule shape:

```num
allow <privacy> <source> -> <target>
deny <privacy> <source> -> <target>
allow <privacy> <trust> <source> -> <target>
deny <privacy> <trust> <source> -> <target>
allow <privacy> <source> -> <target> for tenant <tenant-id>
deny <privacy> <source> -> <target> for tenant <tenant-id>
allow <privacy> <source> -> <target> when route <METHOD> "<PATH>"
deny <privacy> <source> -> <target> when route <METHOD> "<PATH>"
```

The semantic checker composes rules from all policy blocks for external flows.
`ExternalApi` matches any `external.*` call. `ConnectorApi` matches any declared
`connector` method call. More specific targets such as `external.crm`,
`external.crm.send`, `mailer`, and `mailer.send` match only that namespace or
method. A matching `deny` rule takes precedence over matching `allow` rules,
including broad targets such as `ExternalApi` or `ConnectorApi`. An `allow` rule
permits matching privacy/source/target, and a source-specific rule only matches
values carrying that source label. Trust constraints such as `trusted` or
`verified` require the value to carry that trust label. Tenant scopes are
represented in the policy model and only match when evaluation has the same
tenant context; the current static checker does not treat tenant-scoped rules
as global allows. Route conditions match only while checking the corresponding
service route body; outside that route context, the rule does not apply.

## Types

### Structured Types

```num
type RefundRequest {
    payment_id: PaymentId
    reason: Text from UserInput private
    amount: Money<KZT>
}
```

Fields have:

- a name;
- a type reference;
- optional provenance/trust/privacy labels.

Duplicate field names are rejected. Unknown type names are rejected unless they
are built in.

Structured types can declare generic parameters.

```num
type Page<T> {
    items: List<T>
    total: Int
}

workflow main(page: Page<RefundRequest>) {
    let items: List<RefundRequest> = page.items
}
```

Generic type references are checked for arity, and field access substitutes
the concrete generic arguments into field result types.

### Map and Set

`Map<K, V>` and `Set<T>` are available through explicit stdlib constructors
and pure helper functions. Literal syntax is intentionally deferred: constructors
avoid adding collection grammar before Queue, Stack, and Stream semantics are
specified.

```num
let permissions: Set<Text> = set_empty()
let permissions2: Set<Text> = set_insert(permissions, "refund.approve")
let has_permission: Bool = set_contains(permissions2, "refund.approve")

let metadata: Map<Text, Bool> = map_empty()
let metadata2: Map<Text, Bool> = map_insert(metadata, "enabled", true)
let enabled: Bool = map_get(metadata2, "enabled")
```

Supported helpers are `map_empty`, `map_contains`, `map_get`, `map_insert`,
`map_remove`, `set_empty`, `set_contains`, `set_insert`, and `set_remove`.
`insert` and `remove` return a new collection value, so immutable bindings stay
compatible with the language's current mutability model. JSON route decoding
accepts `Set<T>` as an array, `Map<Text,V>` as a JSON object, and non-text-key
maps as explicit `{ "$map": [[key, value]] }` pairs.

### Queue, Stack, and Stream

`Queue<T>`, `Stack<T>`, and `Stream<T>` have a first stdlib slice with explicit
constructors and pure helpers. Queue is FIFO, Stack is LIFO, and Stream is an
ordered immutable sequence that can be inspected with `stream_next` and advanced
with `stream_advance`. This first slice is intentionally synchronous and local;
clustered queues, backpressure, async streaming, and provider-backed stream
lifecycles are separate runtime concerns.

```num
let events: Queue<Text> = queue_empty()
let events2: Queue<Text> = queue_enqueue(events, "evt_1")
let next_event: Text = queue_front(events2)

let rollbacks: Stack<Text> = stack_empty()
let rollbacks2: Stack<Text> = stack_push(rollbacks, "undo_inventory")
let next_rollback: Text = stack_peek(rollbacks2)

let chunks: Stream<Text> = stream_empty()
let chunks2: Stream<Text> = stream_append(chunks, "chunk_1")
let next_chunk: Text = stream_next(chunks2)
```

Supported helpers are `queue_empty`, `queue_enqueue`, `queue_front`,
`queue_dequeue`, `queue_is_empty`, `stack_empty`, `stack_push`, `stack_peek`,
`stack_pop`, `stack_is_empty`, `stream_empty`, `stream_append`,
`stream_has_next`, `stream_next`, and `stream_advance`. JSON route decoding
accepts all three as arrays, and process connector JSON uses explicit
`$queue`, `$stack`, and `$stream` array wrappers.

### Type Aliases and Branded Types

Type aliases are declared with `=`.

```num
type UserId = Brand<Text, "UserId">
type OrderId = Brand<Text, "OrderId">
type Maybe<T> = Option<T>
type SearchResult = User | Company
```

Plain aliases are structural: `Maybe<Text>` is compatible with `Option<Text>`,
and generic alias parameters are substituted during type compatibility checks.
Union aliases accept any compatible member type when used as the expected type.
`match` can also discriminate a union alias by member type name. When the
matched expression is a simple binding, the checker narrows that binding to the
member type inside the corresponding arm.

```num
type User {
    email: Email
}

type Company {
    name: Text
}

type SearchResult = User | Company

workflow audit_result(result: SearchResult) {
    match result {
        User { email } => {
            audit(email)
        }
        Company { name: company_name } => {
            audit(company_name)
        }
    }
}
```

Destructuring is supported for structured union member arms using
`Type { field }` or `Type { field: binding_name }`. The introduced bindings are
immutable and scoped to that arm. Field labels such as provenance, privacy, and
trust are preserved on destructured bindings.

`Brand<T, "Tag">` creates a nominal wrapper type. Branded aliases are distinct
from their base type and from other branded aliases, so a `UserId` cannot be
passed where `Text` or `OrderId` is expected. The checker validates the alias
target type, validates alias generic arity, and records aliases in IR.

Branded aliases can be constructed by calling the alias name with a payload
compatible with the brand base type:

```num
let user_id: UserId = UserId("user_123")
```

Generic branded aliases are context typed. When the payload type directly
mentions the generic parameter, the checker can also infer the alias from the
constructor argument:

```num
type Boxed<T> = Brand<T, "Boxed">

let value: Boxed<Int> = Boxed(42)
let inferred = Boxed(7)
```

The constructor returns the nominal alias type. Passing `OrderId("order_1")`
where `UserId` is expected remains a compile-time error. If generic arguments
cannot be supplied by an expected type or inferred from the payload, the
constructor emits `N2312`; add a binding annotation, parameter type, or return
type context.

Use `unbrand(value)` to explicitly remove the nominal wrapper and recover the
base value:

```num
let raw: Text = unbrand(user_id)
```

`unbrand` is intentionally one-way. Going from a base value back to a branded
alias still requires the explicit alias constructor.

### Enums

```num
enum RiskLevel {
    Low
    Medium
    High
}
```

Enum variants are parsed and stored. `match` statements over enum values are
checked for unknown variants, duplicate arms, and exhaustiveness.

Enum variants may carry one typed payload:

```num
enum PaymentStatus {
    Paid
    Failed(Text)
    Pending
}

fn failed(reason: Text) -> PaymentStatus {
    return Failed(reason)
}
```

Payload constructors are context typed when an expected enum type is available,
such as a typed binding, typed argument, assignment, or return. If a variant name
is declared by exactly one enum in the module, the checker can also infer the
enum type from the constructor itself. Payloads are checked against the declared
variant payload type.

### Built-in Type Names

The semantic checker recognizes these type names:

- `Text`
- `Int`
- `Float`
- `Decimal`
- `Bool`
- `Date`
- `DateTime`
- `Duration`
- `Uuid`
- `Email`
- `PhoneNumber`
- `Url`
- `Json`
- `Bytes`
- `Xml`
- `Result`
- `Option`
- `List`
- `Map`
- `Set`
- `Brand`
- `Money`
- `Secret`
- `Uncertain`
- `Document`
- `Pdf`
- `Docx`
- `Image`
- `Unit`

Built-in currency symbols:

- `KZT`
- `USD`
- `EUR`
- `GBP`
- `RUB`
- `CNY`

### Scalar Validators

`Email`, `Url`, `Uuid`, and `PhoneNumber` are validated with explicit standard
library helpers:

```num
let email: Email from UserInput private trusted = validate_email(raw_email)
let profile: Url from UserInput private trusted = validate_url(raw_profile)
let request_id: Uuid from UserInput private trusted = validate_uuid(raw_request_id)
let phone: PhoneNumber from UserInput private trusted = validate_phone_number(raw_phone)
```

The validators accept `Text` and return the corresponding scalar type. They are
trust gateways for untrusted input while preserving source and privacy labels:
validated private user input is still private user input for policy checks.

Validation is intentionally conservative in the first stdlib slice:

- email values require one `@`, non-empty local/domain parts, a dotted domain,
  and ASCII email-safe characters;
- URL values must be absolute `http://` or `https://` URLs with a dotted ASCII
  host;
- UUID values use the 8-4-4-4-12 hexadecimal form;
- phone numbers use 8 to 15 digits with an optional leading `+`.

Locale-specific phone parsing, IDNA/punycode domains, provider-specific email
rules, and non-HTTP URL schemes remain explicit connector/provider policy
instead of hidden stdlib behavior.

### Hashing Helpers

The security standard library exposes explicit SHA-256 helpers for deterministic
non-password hashing:

```num
let hex: Text = hash_sha256_hex(raw_event)
let compact: Text = hash_sha256_base64(payload_bytes)
```

Both helpers accept `Text` or `Bytes` and return `Text`. The algorithm and
encoding are part of the function name:

- `hash_sha256_hex(value)` returns lowercase hexadecimal SHA-256 output;
- `hash_sha256_base64(value)` returns standard base64 SHA-256 output.

Use these helpers for audit correlation keys, content fingerprints, cache keys,
idempotency material, and deterministic non-secret comparisons. Do not use
SHA-256 helpers for passwords, API keys, or bearer tokens; password hashing
needs a dedicated salted, slow, memory-hard boundary such as Argon2/bcrypt, and
secret values should stay inside `Secret<T>` or an external secret store.

Hashing does not automatically remove privacy or provenance labels. A hash of
private user input may still be linkable private data, so declassification
requires an explicit policy decision or `anonymize(...)` where appropriate.

### Bytes and Xml Helpers

`Bytes` and `Xml` are real stdlib values at runtime. The first slice keeps their
construction explicit at text boundaries:

```num
let payload: Bytes = bytes_from_text(raw_text)
let decoded: Bytes = bytes_from_base64(raw_base64)
let encoded: Text = bytes_to_base64(payload)
let size: Int = bytes_len(payload)

let manifest: Xml = xml_parse(raw_manifest)
let manifest_text: Text = xml_to_text(manifest)
```

`Bytes` display as a bounded summary such as `<bytes len=3 sha256=...>` rather
than dumping raw binary data. Connector JSON uses `{ "$bytes_base64": "..." }`.
Typed HTTP JSON accepts a base64 string or the same wrapper object for `Bytes`.

`Xml` stores the original validated text and displays as `<xml len=...>`.
Connector JSON uses `{ "$xml": "..." }`. The first slice validates only an
XML-shaped document boundary: non-empty text, `<...>` delimiters, at least one
element tag, and no unsupported control characters.

Intentionally unsupported in this slice:

- byte literals and byte slicing;
- XML DOM traversal, XPath, schemas, namespaces, and canonicalization;
- streaming XML or binary parsing;
- automatic conversion between `Text`, `Bytes`, and `Xml` without helper calls.

### Document Metadata

`Document` is a metadata-only stdlib value for uploaded or connector-provided
documents. It does not parse file contents in the first slice. The fixed fields
are:

- `id: Text`
- `name: Text`
- `mime_type: Text`
- `size_bytes: Int`
- `source: Text`
- `privacy: Text`
- `trust: Text`

Example:

```num
workflow review(document: Document from Upload private untrusted) {
    let fixture: Document = document_metadata("doc_2", "invoice.pdf", "application/pdf", 2048, "Upload", "private", "trusted")
    let name: Text = document.name
    let mime: Text = document.mime_type
    let size: Int = document.size_bytes
    audit(name)
    audit(mime)
    audit(size)
}
```

`document_metadata(id, name, mime_type, size_bytes, source, privacy, trust)`
constructs a `Document` value for tests, connector mocks, and metadata handoff
code. The constructor does not read or validate file contents.

Typed HTTP JSON accepts a document metadata object:

```json
{
  "id": "doc_1",
  "name": "contract.pdf",
  "mime_type": "application/pdf",
  "size_bytes": 4096,
  "source": "Upload",
  "privacy": "private",
  "trust": "untrusted"
}
```

Process connector JSON uses `{ "$document": { ... } }` so connectors can
distinguish document metadata from an arbitrary struct. The runtime displays
documents as a bounded metadata summary and does not expose file bytes.

Intentionally unsupported in this slice:

- PDF/DOCX/image parsing;
- OCR and AI extraction;
- page counts, checksums, text extraction, thumbnails, and embedded files;
- MIME sniffing or content validation beyond the explicit metadata fields;
- storing or streaming raw document bytes inside `Document`.

### Pdf and Docx Metadata

`Pdf` and `Docx` are document-specialized metadata wrappers. They preserve the
original `Document` metadata and add format-specific fields:

- `Pdf.document: Document`
- `Pdf.page_count: Int`
- `Docx.document: Document`
- `Docx.title: Text`
- `Docx.creator: Text`
- `Docx.paragraph_count: Int`

Both wrappers also expose the base `Document` fields directly, such as
`pdf.name`, `pdf.privacy`, `docx.mime_type`, and `docx.source`.

```num
let pdf: Pdf = pdf_parse_metadata(document, pdf_bytes)
let pages: Int = pdf.page_count

let docx: Docx = docx_parse_metadata(document, docx_bytes)
let title: Text = docx.title
let paragraphs: Int = docx.paragraph_count
```

`pdf_metadata(document, page_count)` and
`docx_metadata(document, title, creator, paragraph_count)` build wrappers from
already validated metadata. They are intended for connector mocks, test
fixtures, and adapters that receive trusted metadata from an external document
service.

`pdf_parse_metadata(document, bytes)` accepts a PDF byte stream with a `%PDF-`
header and `%%EOF` marker, then counts page objects. `docx_parse_metadata`
accepts a deliberately small first-slice DOCX fixture shape: ZIP local file
entries stored without compression, with `word/document.xml` and optional
`docProps/core.xml`. Compressed DOCX files are rejected with a structured
runtime error until a real ZIP/Deflate parser boundary is introduced.

Intentionally unsupported in this slice:

- PDF text extraction, outlines, annotations, forms, embedded files, images, or
  cryptographic validation;
- DOCX styles, tables, comments, tracked changes, relationships, media, or
  compressed ZIP parsing;
- OCR and AI extraction, which belong to the later document extraction boundary.

### Spreadsheet Metadata

`Spreadsheet` is a document-specialized metadata wrapper for workbook-level
inspection before cell-level import. It preserves the original `Document`
metadata and adds sheet metadata:

- `Spreadsheet.document: Document`
- `Spreadsheet.sheet_count: Int`
- `Spreadsheet.sheet_names: List<Text>`
- `Spreadsheet.sheets: List<SpreadsheetSheet>`
- `SpreadsheetSheet.name: Text`
- `SpreadsheetSheet.row_count: Int`
- `SpreadsheetSheet.column_count: Int`
- `SpreadsheetSheet.header_row: Int`

`header_row` is `0` when the first slice cannot detect a header row.
`Spreadsheet` also exposes the base `Document` fields directly, such as
`workbook.name`, `workbook.privacy`, and `workbook.source`.

```num
let workbook: Spreadsheet = spreadsheet_parse_metadata(document, bytes)
let names: List<Text> = workbook.sheet_names

let sheet: SpreadsheetSheet = spreadsheet_sheet_metadata("Revenue", 120, 8, 1)
let trusted: Spreadsheet = spreadsheet_metadata(document, "[{\"name\":\"Revenue\",\"row_count\":120,\"column_count\":8,\"header_row\":1}]")
```

`spreadsheet_parse_metadata(document, bytes)` accepts a deliberately small
first-slice XLSX fixture shape: ZIP local file entries stored without
compression, with `xl/workbook.xml` and `xl/worksheets/sheet*.xml`. It reads
only workbook and worksheet XML metadata, counts dimensions/rows/cells, and does
not execute formulas. Compressed XLSX files are rejected with a structured
runtime error until a real ZIP/Deflate parser boundary is introduced.
`spreadsheet_metadata(document, sheets_json)` builds a trusted fixture or
adapter value from a JSON array of sheet metadata objects; this avoids adding a
new collection literal syntax in the metadata slice.

Intentionally unsupported in this slice:

- typed table inference and schema detection;
- formula evaluation, recalculation, macros, external links, and pivot tables;
- shared-string resolution beyond safe header-row hints;
- cell-level import and large-file streaming, which belong to later document
  extraction work.

### Image Metadata and OCR Handoff

`Image` is a document-specialized metadata wrapper for safe image inspection
before OCR or pixel decoding. It preserves the original `Document` metadata and
adds dimensions:

- `Image.document: Document`
- `Image.width: Int`
- `Image.height: Int`
- `Image.format: Text`

`OcrResult` represents the output of a fake OCR fixture or external OCR
connector handoff:

- `OcrResult.image: Image`
- `OcrResult.text: Text`
- `OcrResult.confidence: Float`
- `OcrResult.provider: Text`
- `OcrResult.model: Text`
- `OcrResult.source: Text`
- `OcrResult.privacy: Text`
- `OcrResult.trust: Text`

OCR text is always created with `trust` set to `"untrusted"` in this first
slice. Workflows must sanitize, validate, or send it through a human review
step before treating the extracted text as trusted data.

```num
let image: Image = image_parse_metadata(document, bytes)
let fixture: Image = image_metadata(document, 640, 480, "png")
let ocr: OcrResult = ocr_result(fixture, "Invoice total", 0.91, "fake-ocr", "fixture-v1")
```

`image_parse_metadata(document, bytes)` reads only PNG IHDR or JPEG SOF
dimensions. It does not decode pixels, inspect EXIF, run OCR, or call external
providers. `ocr_result(image, text, confidence, provider, model)` is the
deterministic handoff constructor for tests and connector adapters; connector
implementations receive normal egress context, including tenant/request
metadata, through generated SDK contracts.

Intentionally unsupported in this slice:

- OCR engines, provider clients, OCR billing, and provider-specific options;
- EXIF/GPS/color-profile parsing and image transformations;
- pixel decoding, thumbnails, object detection, handwriting recognition, and
  layout analysis;
- trusting extracted text automatically.

### DateTime and Duration Helpers

`DateTime` values use explicit UTC ISO-8601 text in the first stdlib slice:

```num
let created_at: DateTime = datetime_parse_iso(raw_created_at)
let timeout: Duration<Hour> = duration_parse_hours("4h")
let deadline: DateTime = created_at + timeout
let retry_at: DateTime = deadline - duration_parse_hours("1h")
let deadline_text: Text = datetime_format_iso(deadline)
let timeout_text: Text = duration_format_hours(timeout)
```

The supported helpers are deliberately small and deterministic:

- `datetime_parse_iso(value: Text) -> DateTime` accepts
  `YYYY-MM-DDTHH:MM:SSZ` only;
- `datetime_format_iso(value: DateTime) -> Text` returns canonical UTC ISO text;
- `duration_parse_hours(value: Text) -> Duration<Hour>` accepts hour values such
  as `4h` or `1.5 h`;
- `duration_format_hours(value: Duration<Hour>) -> Text` returns compact hour
  text;
- `DateTime + Duration<Hour>` and `DateTime - Duration<Hour>` return
  `DateTime`;
- `DateTime` values can be compared with `<`, `<=`, `>`, and `>=`.

Timezone behavior is explicit: this slice accepts only UTC `Z` timestamps and
does not infer local time, offsets, daylight-saving transitions, calendars,
locale formatting, leap seconds, or month/year duration arithmetic. Those remain
connector or future stdlib work.

### Decimal Helpers

`Decimal` values are exact runtime values parsed through explicit helpers:

```num
let subtotal: Decimal = decimal_parse(raw_subtotal)
let tax: Decimal = decimal_parse("2.25")
let total: Decimal = subtotal + tax
let display_total: Text = decimal_format(total)
```

`decimal_parse(value: Text) -> Decimal` accepts ASCII decimal text with an
optional leading `-` and at most one decimal point. `decimal_format(value:
Decimal) -> Text` emits canonical text without trailing insignificant zeroes.

Decimal arithmetic is same-type only: `Decimal + Decimal`, `Decimal - Decimal`,
`Decimal * Decimal`, and `Decimal / Decimal` return `Decimal`; mixed
`Int`/`Float`/`Decimal` arithmetic is rejected by the checker. Runtime connector
JSON represents Decimal as `{"$decimal": "12.34"}` so exact values do not fall
back to JSON floating-point numbers. Division uses a deterministic first-slice
precision of up to 9 fractional digits and trims insignificant trailing zeroes.

### `Option<T>`

`Option<T>` represents nullable values without ordinary `null`.

```num
fn maybe_phone(has_phone: Bool) -> Option<Text> {
    if has_phone {
        return Some("555")
    } else {
        return None
    }
}

workflow main(phone: Option<Text>) {
    if phone.is_some {
        let actual: Text = phone.value
        audit("phone_available")
    }

    if phone.is_none {
        audit("phone_missing")
    } else {
        let actual: Text = phone.value
        audit(actual)
    }
}
```

`Some(value)` constructs a present value and `None` constructs an empty value.
`Some(value)` can infer `Option<T>` from the payload type. `None` needs an
expected `Option<T>` type because it carries no payload. Typed returns,
bindings, assignments, and arguments provide that expected type.

The checker treats `option.is_some` and `option.is_none` as `Bool`.
`option.value` is only available when the current branch guarantees a present
value. Direct checks narrow as expected:
`if option.is_some { option.value }` and
`if option.is_none { ... } else { option.value }`.

Boolean guards also narrow when the implication is sound. For example,
`if option.is_some && allowed { option.value }` is accepted, and
`if option.is_none || denied { ... } else { option.value }` is accepted.
`if option.is_some || allowed { option.value }` is rejected because the `||`
branch can be true without the option being present.

### `Result<T, E>`

`Result<T, E>` represents fallible computations.

```num
workflow main(found: Result<Text, Text>) {
    if found.is_ok {
        let value: Text = found.value
        audit("found")
    } else {
        let error: Text = found.error
        audit(error)
    }
}
```

`Ok(value)` constructs the success side and `Err(error)` constructs the error
side. Constructors are context typed: use them where the expected type is known,
such as a typed `return`, typed binding, assignment, or typed function argument.
`Ok()` is accepted for `Result<Unit, E>`.

```num
fn load_user(found: Bool) -> Result<Text, Text> {
    if found {
        return Ok("user")
    } else {
        return Err("missing")
    }
}
```

The checker treats `result.is_ok` and `result.is_err` as `Bool`.
`result.value` is only available after an `is_ok` check. `result.error` is only
available after an `is_err` check. Direct checks and sound boolean guards
narrow branches. For example, `if result.is_ok && allowed { result.value }` is
accepted, and `if result.is_err || retry { ... } else { result.value }`
narrow the else branch to the `Ok` case.

The postfix `?` operator unwraps `Result<T, E>` to `T` and propagates `E` to the
enclosing callable. The enclosing `fn`, `workflow`, or `action` must return a
compatible `Result<_, E>`.

```num
connector users {
    find(id: Text) -> Result<Text, Text>
}

fn load_user(id: Text) -> Result<Text, Text> {
    let user: Text = users.find(id)?
    return Ok(user)
}
```

## Labels

Values can carry metadata.

### Provenance

```num
email: Email from UserInput
```

The source is stored as text, for example `UserInput`, `Database`, `PublicData`,
or `AI`.

### Trust

```num
message: Text untrusted
profile: Customer trusted
document: Document verified
```

Supported trust labels:

- `untrusted`
- `trusted`
- `verified`

The checker preserves trust labels through field access and local `let`
bindings. Untrusted values cannot flow into `external.*` calls, high-risk and
critical actions, or `ai.*` prompts/tool calls. This gives Num a static
prompt-injection boundary: user text or retrieved content must be sanitized,
validated, verified, or reviewed before it can influence an AI call. Promoting
untrusted data into a `trusted` or `verified` binding requires an explicit trust
gateway such as `sanitize(...)`, `validate_trust(...)`, `verify_trust(...)`, or
`require_human_review(...)`. These gateway names are exposed as built-ins for
LSP completion and hover. Trust gateways change the trust label, but preserve
provenance and privacy: sanitized private user input is still private user input
for policy checks.

`anonymize(...)` is the privacy/declassification gateway. It marks the returned
value as `public` data from `DerivedData`, allowing code to send derived,
non-identifying values without granting a policy exception for the original
private source. It does not validate trust: if the input is `untrusted`, combine
it with a trust gateway before sending it to AI, external services, or high-risk
actions.

```num
workflow main(ticket: Ticket) {
    let message: Text trusted = sanitize(ticket.message)
    let intent: Uncertain<Text> = ai.classify(message)
    if intent.confidence < 0.85 {
        require_human_review(ticket)
        return
    }
    external.crm.send(intent.value)
}
```

```num
workflow export_marker(email: Email from UserInput private untrusted) {
    let marker: Text trusted = sanitize(anonymize(email))
    external.analytics.send(marker)
}
```

The runtime includes a text sanitization foundation:
`TextSanitizationPolicy`, `SanitizedText`, `TextSanitizer`, and
`DefaultTextSanitizer`. The default sanitizer can trim text, strip control
characters while preserving newline/tab carriage whitespace, and truncate by
character count. It also supports reusable sanitizer packs for plain text,
email, person names, and identifiers. Packs compose into a single stricter
policy: boolean cleanup options are combined, `max_chars` keeps the tighter
limit, and allowed character classes are intersected when possible.
Projects can add named packs in `num.toml` under
`[sanitizer_packs.<name>]`, then call `sanitize(value, "name")` or compose
packs with `+`/`,` such as `sanitize(raw, "plain_text+strict_latin_identifier")`.
The one-argument form `sanitize(value)` remains the compatibility trust gateway.

### Privacy

```num
email: Email private
report: Text public
token: Secret<Text> secret
```

Supported privacy labels:

- `public`
- `internal`
- `private`
- `sensitive`
- `secret`
- `regulated`

## Functions

Functions define ordinary callable blocks.

```num
fn normalize(input: Text) -> Text {
    return input
}
```

Function bodies use the same statement parser as workflows and actions.
Function calls are checked by name, argument count, argument type where known,
and result type when assigned to an explicitly typed binding. `return`
expressions are checked against the declared callable result type.

Function declarations can define a local spending scope with
`budget <amount> <currency>`:

```num
fn refund_side_effects(payment: Payment) requires Permission.IssueRefund budget 20 KZT {
    issue_refund(payment, payment.amount)
}
```

When a function runs inside a workflow or service route, the demo interpreter
checks every declared action `cost` against all active parent and child budget
scopes before executing the side effect.

## Workflows

Workflows represent business processes.

```num
workflow process_refund(request: RefundRequest) budget 100 KZT rate limit 60 per 1m {
    require Permission.ViewBilling for current_user
    audit("refund_started")
}
```

Workflow parameters support the same type references and labels as type fields.
Workflow declarations can also define a per-run spending limit with
`budget <amount> <currency>`. The demo interpreter opens a budget scope before
executing the workflow body. Nested function/workflow calls inherit that scope,
and their own budgets can further restrict spending.
Workflow declarations can define a rate limit with
`rate limit <count> per <duration>`. The demo interpreter enforces this with an
in-memory runtime limiter.

The runtime includes `WorkflowEvent`, `WorkflowEventQueue`,
`MemoryWorkflowEventQueue`, and `FileWorkflowEventQueue` primitives. A
`WorkflowEngine` can process queued start/wait/resume/complete/fail/compensate/
cancel events and persist the resulting workflow state and audit events.
Distributed event dispatch, scheduling, and worker orchestration are not
implemented yet.

## Actions

Actions represent external side effects.

```num
action issue_refund(payment: Payment, amount: Money<KZT>)
    requires Permission.IssueRefund
    risk high
    timeout 10s
    cost 15 KZT
    retry 3
    idempotency key payment.id
    rollback reverse_refund(payment, amount)
{
    payment_gateway.refund(payment.id, amount)
    audit("refund_issued", {
        payment_id: payment.id,
        amount: amount,
        actor: current_user.id
    })
}
```

Supported metadata:

- `requires Permission.<Name>`
- `risk low`
- `risk medium`
- `risk high`
- `risk critical`
- `timeout <raw-value>`
- `retry <attempt-count>`
- `idempotency key <raw-expression>`
- `rollback <raw-call>`
- `cost <raw-value>`

Semantic rules:

- referenced permissions must exist;
- timeout metadata is parsed by the demo interpreter and enforced by the action
  execution wrapper as a per-attempt execution budget;
- cost metadata is parsed by the demo interpreter, authorized against every
  active budget scope before the side effect runs, and charged to a runtime
  cost ledger after successful, non-replayed action executions;
- retry metadata is preserved in AST, formatter, IR, and used by the demo
  interpreter for retryable action failures;
- idempotency keys are preserved in AST, formatter, IR, and used by the demo
  interpreter to replay successful action executions without repeating the side
  effect;
- high-risk and critical actions must call `audit`;
- high-risk and critical actions without rollback emit a warning;
- callers must have a matching `require` statement or callable-level
  `requires` metadata before calling a permission-gated action.
- action call argument count and argument types are checked where known.

## Tests

Top-level test declarations are executable `.num` checks.

```num
test "basic truth" {
    let allowed: Bool = true
    assert allowed == true
}
```

`assert` is a statement that requires a `Bool` expression. Non-boolean
assertions fail semantic checking with `N3100`; false assertions fail at runtime
when executed through `num test`.

The parser also accepts typed test categories:

```num
test policy "private data stays internal" {
    let email: Text from UserInput private = "user@example.com"

    expect_deny {
        external.analytics.send(email)
    }
}

test workflow "refund rollback" {
    mock_connector reports.render("r_1") => "mock report"
    expect_workflow_success refund_happy_path()
    expect_audit "refund_completed"
    expect_workflow_failure refund_without_permission()
}

test ai "low confidence requires review" {
    mock_ai ai.classify("refund") => RefundRequest confidence 0.62
    let intent: Uncertain<Intent> = ai.classify("refund")

    assert intent.confidence < 0.85
}
```

Policy tests support static policy expectations:

- `expect_deny { ... }` passes only when the nested block produces a policy
  denial such as `N2400`;
- `expect_allow { ... }` passes only when the nested block has no policy-denial
  diagnostics.
- `expect_workflow_success workflow_name(...)` passes only when the direct
  workflow call completes successfully at runtime.
- `expect_workflow_failure workflow_name(...)` passes only when the direct
  workflow call fails at runtime, for example because a permission, budget, or
  connector expectation was violated.
- `expect_audit "event_name"` passes only when the runtime audit trail contains
  the expected event value.
- `mock_connector connector.method(...) => Value` installs a deterministic
  response for a declared non-AI connector method inside `test workflow`.
- `mock_ai ai.method(...) => Value confidence 0.91` installs a deterministic
  `Uncertain<Value>` response for an AI connector method inside `test ai`.

Expected policy denials do not leak into the outer `num check` diagnostics, and
the runtime does not execute the nested body. Workflow expectations must appear
inside `test workflow` blocks and must call a declared workflow directly. Audit
expectations must appear inside `test workflow` blocks and observe events
written by runtime `audit(...)` calls.
Connector mocks must appear inside `test workflow` blocks and target declared
non-`Unit` connector methods. AI mocks must appear inside `test ai` blocks,
target declared `ai.*` connector methods, and the connector result must be
`Uncertain<T>`.

## Connectors and Services

Connectors and services declare external namespaces.

```num
connector payments {
    find(payment_id: PaymentId) -> Payment
}

service BillingApi {
    route POST "/refunds" requires Permission.IssueRefund {
        input request: RefundRequest from HttpBody private
    }
}
```

Connector bodies are parsed as typed method schemas. The checker validates:

- duplicate method names inside a connector;
- method parameter and result type references;
- calls to undeclared connector methods;
- connector call argument count;
- connector call argument type when the argument type can be inferred;
- explicit `let` binding type compatibility with connector results.

Calls such as `payments.find(...)` must match a declared connector method.
When a connector argument carries `private`, `sensitive`, `secret`, or
`regulated` data, the same policy engine used for `external.*` calls checks the
flow against `ConnectorApi`, the connector namespace, and the concrete
`connector.method` target.

Service bodies are parsed into route schemas. The checker validates:

- duplicate method/path route declarations;
- route `requires Permission.<Name>` references;
- route input type references and labels;
- route body statements using the same semantic checks as workflow bodies;
- action permission requirements satisfied by route-level `requires` clauses.

Services can be exercised through `num route`, the persistent `num serve` HTTP
demo listener, and the one-request `num serve-once` listener. HTTP listeners
decode non-empty JSON request bodies into the declared route input type,
including structural types, `Brand<Text,...>` aliases, and `Money<C>`
minor-unit objects. Request bodies are read using `Content-Length` with basic
header/body size limits. The service runtime captures `X-Actor`, `X-Tenant`,
`X-Request-Id`, and `X-Correlation-Id` headers into a `SecurityContext`; the
actor context is exposed as `current_user`, with `current_user.id`,
`current_user.tenant`, `current_user.request_id`, and
`current_user.correlation_id` available during execution. `X-Role` and
comma-separated `X-Roles` headers are resolved against `.num` `role`
declarations and grant the role's allowed permissions for that request. A
project manifest with `[security].tenant_isolation = true` makes `num route`,
`num serve`, and `num serve-once` reject cross-tenant service requests before
the route body executes and emit a structured tenant error plus audit event. A
hardened production HTTP server runtime is not implemented yet.

Service-route failures use a stable JSON response body:

```json
{
  "error": {
    "kind": "permission",
    "code": "permission_denied",
    "message": "Security Violation: Missing required permission 'IssueRefund'",
    "request_id": "req_42",
    "correlation_id": "corr_42"
  }
}
```

The `kind` field classifies `parse`, `validation`, `permission`, `tenant`,
`connector`, `workflow`, or `internal` failures. The `code` field is stable for
tests and clients. Connector failures return a generic client-facing message
with connector method and retryability metadata, while detailed diagnostics stay
in runtime trace/debug surfaces.

Service declarations can also define a budget applied to every demo route
execution and a rate limit checked before the route body runs. Route execution
opens a parent budget scope, so nested function/workflow calls and actions
share the service route budget:

```num
service BillingApi budget 100 KZT rate limit 60 per 1m {
    route POST "/refunds" {
        audit("refund")
    }
}
```

## Statements

### `let` and `var`

```num
let payment: Payment = payments.find(request.payment_id)
let inferred_payment = payments.find(request.payment_id)
var retries: Int = 0
retries = retries + 1
```

The checker tracks:

- binding name;
- optional type;
- labels;
- whether the binding is mutable;
- whether the binding is uncertain;
- whether the binding is secret.

`let` bindings and parameters are immutable. `var` bindings can be reassigned.
When no type annotation is present, the checker infers supported expression
result types such as literals, field access, connector calls, direct callable
calls, enum constructors, branded constructors, `Option`, `Result`, and object
literals. Assignments are checked against the existing binding type when the
type is known. Assigning to an unknown name or immutable binding is a
compile-time error.

### `reject`

```num
reject("Refund amount is greater than payment amount")
```

`reject(reason)` is a workflow-control builtin. The checker accepts it as a
runtime function, and the demo interpreter fails the current workflow, action,
or function with the provided reason.

### `require`

```num
require Permission.IssueRefund for current_user
```

The checker verifies that the permission exists and records it as granted in the
current checked path.

### `if` / `else`

```num
if risk.confidence < 0.85 {
    require_human_approval(
        action: "issue_refund",
        reason: "Low AI confidence"
    )
    return
} else {
    audit("risk_accepted")
}
```

The parser stores the condition text in the statement AST, and the semantic
checker parses it into an expression AST for supported expression forms.

### `transaction`

```num
transaction {
    audit("local_transaction")
}
```

Plain transactions are parsed and their body is semantically checked. No
database transaction runtime is implemented yet.

### `transaction saga`

```num
transaction saga {
    issue_refund(payment, request.amount)
    notify_customer(payment.customer_email)
}
```

Saga blocks are parsed and checked. If a high-risk action inside the saga lacks
rollback metadata, the checker emits a warning.

The demo interpreter registers rollback expressions when actions execute inside
a saga. It also applies action retry and idempotency metadata. Persistent
compensation is not implemented.

### `match`

```num
match risk.value {
    Low => {
        audit("low_risk")
    }
    Medium if risk.confidence < 0.85 => {
        require_human_approval("medium_uncertain")
    }
    High => {
        require_human_approval("high_risk")
    }
    _ => {
        audit("other_risk")
    }
}
```

`match` expressions must resolve to an enum type or a union alias. Enum arms
must reference variants from that enum. Union arms must reference member type
names from that union alias. A match without `_` must cover every enum variant
or union member. For simple binding expressions such as `match result`, union
arms narrow the binding to the matched member type inside the arm.
Enum payload arms can bind the payload with `Variant(payload_name)`.
Structured union member arms can destructure fields with
`Type { field, other: alias }`.
Nested structured fields can be destructured by naming the nested type:

```num
match result {
    User { profile: Profile { email } } => {
        audit(email)
    }
}
```

Arms may include guard clauses:

```num
match decision {
    Approve(score) if score >= 90 => {
        audit("auto_approved")
    }
    Approve(_) => {
        audit("manual_review")
    }
}
```

The guard is checked after the pattern matches and after payload or field
bindings are introduced, so it can reference names such as `score` or
destructured fields. Guard expressions must type-check as `Bool`. A guarded arm
does not make a match exhaustive because the guard can evaluate to `false`;
include an unguarded arm or `_` fallback for every remaining case. Broader
general destructuring patterns beyond structured union member fields are not
implemented yet.

### `return`

```num
return
return result
```

Return expressions are checked against the declared callable result type when
the callable has `-> Type`. Returning a value from a Unit callable is rejected.
Typed callables must also return on every control-flow path. The checker treats
an `if` as returning only when both branches return, and a `match` as returning
only when it is exhaustive and every arm returns.

### Expression Statements

Any unrecognized statement line is stored as expression text and parsed by the
semantic checker.

```num
audit("workflow_completed")
mailer.send(email)
```

The semantic checker parses supported expression forms and applies safety and
type checks.

## Expressions

v0.3.0 has an expression AST for the supported operational subset.

Supported expression forms:

- identifiers: `request`;
- string literals: `"refund_issued"`;
- boolean literals: `true`, `false`;
- integer literals: `42`;
- float literals: `0.85`;
- object literals: `{ payment_id: payment.id, amount: amount }`;
- security context member access: `current_user.id`;
- named call payloads, desugared to one object argument:
  `require_human_approval(action: "issue_refund", reason: "Low AI confidence")`;
- member access: `payment.customer_email`;
- calls: `payments.find(request.payment_id)`;
- workflow rejection: `reject("Refund amount is greater than payment amount")`;
- branded alias constructors: `PaymentId("pay_1")`;
- explicit brand unwrap: `unbrand(payment_id)`;
- enum variant constructors in typed enum contexts: `Failed("network")`;
- nested member calls: `external.analytics.send(report)`;
- parenthesized expressions;
- arithmetic expressions: `count + 1`, `price * count`;
- ordering comparisons: `risk.confidence < 0.85`, `count >= 2`,
  `requested > paid`;
- equality comparisons: `status == "approved"`, `status != "denied"`;
- boolean operators: `approved && risk.confidence >= 0.85`.

The semantic checker uses this AST for:

- direct call resolution;
- direct `fn`, `workflow`, and `action` call argument checks;
- connector method resolution;
- connector argument count checks;
- connector argument type checks when argument types are known;
- connector result type checks against explicit `let` binding types;
- direct callable result type checks against explicit `let` binding types;
- `return` value checks against declared callable result types;
- struct field existence checks;
- `Option<T>.is_some`, `Option<T>.is_none`, guarded `Option<T>.value`,
  `Some(...)`, and `None`;
- `Result<T,E>.is_ok`, `.is_err`, guarded `.value`, and guarded `.error`;
- `Ok(...)` and `Err(...)` constructors in typed `Result<T,E>` contexts;
- branded alias constructors such as `PaymentId("pay_1")`;
- explicit branded alias unwrap through `unbrand(value)`;
- enum variant constructors such as `Failed("network")` in typed enum contexts;
- `Result<T,E>?` unwrap and compatible error propagation;
- `async <expr>` task creation and `await <task>` unwrapping for `Task<T>`;
- `Uncertain<T>.confidence` and `Uncertain<T>.value`;
- object literal fields, with provenance, privacy, and trust labels preserved
  through nested field expressions;
- ordinary `log` secret checks;
- private data flow checks;
- arithmetic operand checks;
- ordering operand checks;
- equality operand checks;
- boolean operand checks.

Arithmetic uses strict operand rules:

- `Int`, `Float`, and `Decimal` arithmetic requires matching numeric types;
- `Money<C> + Money<C>` and `Money<C> - Money<C>` return `Money<C>`;
- `Money<C> * Int|Float|Decimal` and `Money<C> / Int|Float|Decimal` return
  `Money<C>`;
- different `Money<C>` currencies cannot be combined without explicit
  conversion.

Ordering comparisons require compatible ordered scalar values:

- `Int`
- `Float`
- `Decimal`
- `Date`
- `DateTime`
- `Duration`
- `Money<C>` when both sides use the same currency

## Async Tasks

The compiler models asynchronous work with `Task<T>`.

```num
fn fetch_profile(id: Text) -> Text {
    return "Aidar"
}

workflow main() -> Text {
    let task: Task<Text> = async fetch_profile("u1")
    let profile: Text = await task
    return profile
}
```

`async <expr>` has type `Task<T>` when `<expr>` has type `T`. `await <task>`
unwraps `Task<T>` back to `T`. Awaiting a non-task value is rejected, and a
task cannot be assigned to the awaited value type without an explicit `await`.

Bare async expressions are rejected so tasks do not get created and forgotten:

```num
workflow main() {
    async fetch_profile("u1")
}
```

Bind the task to an owner or await an existing task instead.

## AI and `Uncertain<T>`

The checker treats expressions containing `ai.` as AI calls.

AI results must be assigned to `Uncertain<T>` when a type is explicit:

```num
let risk: Uncertain<RiskLevel> = ai.assess_refund_risk(request)
```

This is rejected:

```num
let risk: RiskLevel = ai.assess_refund_risk(request)
```

An uncertain binding must be handled before use. The current checker accepts
these handling patterns:

- reading `.confidence`;
- reading `.value`;
- calling `require_human_review`;
- calling `require_human_approval`.

Example:

```num
if risk.confidence < 0.85 {
    require_human_approval(
        action: "issue_refund",
        reason: "Low AI confidence"
    )
    return
}

issue_refund(payment, request.amount)
```

## Privacy and External API Flow

The checker rejects private, sensitive, secret, or regulated values flowing to
`external.*` calls or declared connector method calls without an allow policy.
Policies can target all synthetic external APIs with `ExternalApi`, all declared
connectors with `ConnectorApi`, a namespace such as `external.analytics` or
`mailer`, or a concrete method such as `external.analytics.send` or
`mailer.send`. Code can also declassify a derived, non-identifying value with
`anonymize(...)`; the checker treats the result as `public` `DerivedData`
instead of the original private source.

Example rejected without a matching policy:

```num
workflow main(email: Email from UserInput private) {
    external.crm.send(email)
}
```

Example allowed:

```num
policy DataSharing {
    allow public PublicData -> ExternalApi
    allow private UserInput -> mailer.send
}

connector mailer {
    send(email: Email) -> Unit
}

workflow main(report: Text from PublicData public, email: Email from UserInput private) {
    external.analytics.send(report)
    mailer.send(email)
}
```

## Secrets and Logging

`Secret<T>` types and `secret` privacy labels are tracked as secret values.
The type itself carries intrinsic `secret` privacy for semantic flow checks, so
a `Secret<Text>` cannot flow to `external.*` or connector calls without an
allow policy even when the declaration forgot the explicit `secret` label.
Strict project linting still warns when `Secret<T>` is missing the explicit
label because source code should make secret handling visible.

The checker rejects secret values passed to `log`.

```num
workflow main(token: Secret<Text>) {
    log(token)
}
```

Audit logging is treated separately from ordinary `log` in v0.3.0. High-risk
actions are required to call `audit`.

The runtime exposes a `SecretStore` contract plus memory and file-backed stores
for local execution. It also defines a provider-neutral external secret backend
adapter boundary using `secret://<backend>/<name>` references, with distinct
missing, denied, and unavailable error kinds and a deterministic stub backend
for tests. The first Vault adapter slice supports token-auth configuration,
KV v2 response extraction, invalid-response classification, mocked tests, and a
fixture/dev `http://` transport boundary; production HTTPS transport,
additional Vault auth methods, KMS, and cloud secret providers remain future
provider-client work. Secret values use redacted debug output, and runtime
reporting boundaries use the stable `<redacted>` marker for `Secret<T>` values
in trace/debug JSON, structured connector errors, process connector JSON
conversion, and service error responses.

## Current Expression Limitations

The current implementation has an expression AST for the supported subset above,
but it is not a complete general-purpose expression language yet.

This means v0.3.0 does not yet implement:

- assignment flow analysis beyond supported expression result checks;
- overload resolution;
- nested and general destructuring pattern matching;
- general nullable flow analysis outside supported `if` boolean guards.
