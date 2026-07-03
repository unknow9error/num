#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinKind {
    Namespace,
    Type,
    Function,
    Currency,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltinSymbol {
    pub name: &'static str,
    pub kind: BuiltinKind,
    pub signature: &'static str,
    pub documentation: &'static str,
}

const BUILTIN_SYMBOLS: &[BuiltinSymbol] = &[
    BuiltinSymbol {
        name: "Permission",
        kind: BuiltinKind::Namespace,
        signature: "Permission.<Name>",
        documentation: "Built-in namespace used to reference permissions declared with `permission <Name>`.",
    },
    BuiltinSymbol {
        name: "Money",
        kind: BuiltinKind::Type,
        signature: "Money<CurrencyCode>",
        documentation: "Built-in amount type parameterized by an ISO 4217-style currency code, for example `Money<KZT>`.",
    },
    BuiltinSymbol {
        name: "ExchangeRate",
        kind: BuiltinKind::Type,
        signature: "ExchangeRate<FromCurrency, ToCurrency>",
        documentation: "Built-in audited exchange-rate boundary for explicit Money currency conversion.",
    },
    BuiltinSymbol {
        name: "Uncertain",
        kind: BuiltinKind::Type,
        signature: "Uncertain<T>",
        documentation: "Built-in wrapper for probabilistic AI output. Values must be checked before being used as facts.",
    },
    BuiltinSymbol {
        name: "Secret",
        kind: BuiltinKind::Type,
        signature: "Secret<T>",
        documentation: "Built-in wrapper for values that must not be logged or sent to unsafe sinks.",
    },
    BuiltinSymbol {
        name: "Encrypted",
        kind: BuiltinKind::Type,
        signature: "Encrypted<T>",
        documentation: "Built-in envelope type for provider-encrypted payloads with algorithm, key id, and redacted ciphertext metadata.",
    },
    BuiltinSymbol {
        name: "Document",
        kind: BuiltinKind::Type,
        signature: "Document",
        documentation: "Built-in metadata-only document value with id, name, MIME type, byte size, source, privacy, and trust fields.",
    },
    BuiltinSymbol {
        name: "Pdf",
        kind: BuiltinKind::Type,
        signature: "Pdf",
        documentation: "Built-in document subtype for PDF metadata.",
    },
    BuiltinSymbol {
        name: "Docx",
        kind: BuiltinKind::Type,
        signature: "Docx",
        documentation: "Built-in document subtype for DOCX metadata.",
    },
    BuiltinSymbol {
        name: "SpreadsheetSheet",
        kind: BuiltinKind::Type,
        signature: "SpreadsheetSheet",
        documentation: "Built-in metadata value for one spreadsheet sheet.",
    },
    BuiltinSymbol {
        name: "Spreadsheet",
        kind: BuiltinKind::Type,
        signature: "Spreadsheet",
        documentation: "Built-in document subtype for sheet-level spreadsheet metadata.",
    },
    BuiltinSymbol {
        name: "Image",
        kind: BuiltinKind::Type,
        signature: "Image",
        documentation: "Built-in document subtype for safe image metadata.",
    },
    BuiltinSymbol {
        name: "OcrResult",
        kind: BuiltinKind::Type,
        signature: "OcrResult",
        documentation: "Built-in value for OCR connector handoff results that remain untrusted by default.",
    },
    BuiltinSymbol {
        name: "Distance",
        kind: BuiltinKind::Type,
        signature: "Distance<Unit>",
        documentation: "Built-in distance type parameterized by a unit of measurement, for example `Distance<Kilometer>`.",
    },
    BuiltinSymbol {
        name: "Duration",
        kind: BuiltinKind::Type,
        signature: "Duration<Unit>",
        documentation: "Built-in duration type parameterized by a unit of measurement, for example `Duration<Hour>`.",
    },
    BuiltinSymbol {
        name: "Speed",
        kind: BuiltinKind::Type,
        signature: "Speed<Unit>",
        documentation: "Built-in speed type parameterized by a unit of measurement, for example `Speed<KilometersPerHour>`.",
    },
    BuiltinSymbol {
        name: "Kilometer",
        kind: BuiltinKind::Type,
        signature: "Kilometer",
        documentation: "Distance unit representation.",
    },
    BuiltinSymbol {
        name: "Hour",
        kind: BuiltinKind::Type,
        signature: "Hour",
        documentation: "Duration unit representation.",
    },
    BuiltinSymbol {
        name: "KilometersPerHour",
        kind: BuiltinKind::Type,
        signature: "KilometersPerHour",
        documentation: "Speed unit representation.",
    },
    BuiltinSymbol {
        name: "sanitize",
        kind: BuiltinKind::Function,
        signature: "sanitize(value)",
        documentation: "Trust gateway for sanitized input. The checker treats the returned value as trusted when assigned with a trusted or verified label.",
    },
    BuiltinSymbol {
        name: "anonymize",
        kind: BuiltinKind::Function,
        signature: "anonymize(value)",
        documentation: "Privacy gateway for declassified derived data. The checker treats the returned value as public derived data.",
    },
    BuiltinSymbol {
        name: "validate_trust",
        kind: BuiltinKind::Function,
        signature: "validate_trust(value)",
        documentation: "Trust gateway for validation-backed promotion of untrusted data.",
    },
    BuiltinSymbol {
        name: "validate_email",
        kind: BuiltinKind::Function,
        signature: "validate_email(value: Text) -> Email",
        documentation: "Validates a text value as a simple email address and returns Email without changing privacy or provenance labels.",
    },
    BuiltinSymbol {
        name: "validate_url",
        kind: BuiltinKind::Function,
        signature: "validate_url(value: Text) -> Url",
        documentation: "Validates an absolute http(s) URL and returns Url without changing privacy or provenance labels.",
    },
    BuiltinSymbol {
        name: "validate_uuid",
        kind: BuiltinKind::Function,
        signature: "validate_uuid(value: Text) -> Uuid",
        documentation: "Validates an RFC 4122-style UUID string and returns Uuid without changing privacy or provenance labels.",
    },
    BuiltinSymbol {
        name: "validate_phone_number",
        kind: BuiltinKind::Function,
        signature: "validate_phone_number(value: Text) -> PhoneNumber",
        documentation: "Validates a conservative E.164-style phone number and returns PhoneNumber without changing privacy or provenance labels.",
    },
    BuiltinSymbol {
        name: "hash_sha256_hex",
        kind: BuiltinKind::Function,
        signature: "hash_sha256_hex(value: Text|Bytes) -> Text",
        documentation: "Computes a SHA-256 digest for deterministic non-password hashing and returns lowercase hexadecimal text.",
    },
    BuiltinSymbol {
        name: "hash_sha256_base64",
        kind: BuiltinKind::Function,
        signature: "hash_sha256_base64(value: Text|Bytes) -> Text",
        documentation: "Computes a SHA-256 digest for deterministic non-password hashing and returns standard base64 text.",
    },
    BuiltinSymbol {
        name: "bytes_from_text",
        kind: BuiltinKind::Function,
        signature: "bytes_from_text(value: Text) -> Bytes",
        documentation: "Encodes text as UTF-8 bytes.",
    },
    BuiltinSymbol {
        name: "bytes_from_base64",
        kind: BuiltinKind::Function,
        signature: "bytes_from_base64(value: Text) -> Bytes",
        documentation: "Decodes standard base64 text into Bytes.",
    },
    BuiltinSymbol {
        name: "bytes_to_base64",
        kind: BuiltinKind::Function,
        signature: "bytes_to_base64(value: Bytes) -> Text",
        documentation: "Encodes Bytes as standard base64 text for JSON and connector boundaries.",
    },
    BuiltinSymbol {
        name: "bytes_len",
        kind: BuiltinKind::Function,
        signature: "bytes_len(value: Bytes) -> Int",
        documentation: "Returns the byte length of a Bytes value.",
    },
    BuiltinSymbol {
        name: "xml_parse",
        kind: BuiltinKind::Function,
        signature: "xml_parse(value: Text) -> Xml",
        documentation: "Validates text as the first Xml representation and returns Xml.",
    },
    BuiltinSymbol {
        name: "xml_to_text",
        kind: BuiltinKind::Function,
        signature: "xml_to_text(value: Xml) -> Text",
        documentation: "Returns the original text backing an Xml value.",
    },
    BuiltinSymbol {
        name: "document_metadata",
        kind: BuiltinKind::Function,
        signature: "document_metadata(id: Text, name: Text, mime_type: Text, size_bytes: Int, source: Text, privacy: Text, trust: Text) -> Document",
        documentation: "Builds a metadata-only Document value without parsing file contents.",
    },
    BuiltinSymbol {
        name: "pdf_metadata",
        kind: BuiltinKind::Function,
        signature: "pdf_metadata(document: Document, page_count: Int) -> Pdf",
        documentation: "Builds a metadata-only Pdf wrapper from already validated Document metadata.",
    },
    BuiltinSymbol {
        name: "docx_metadata",
        kind: BuiltinKind::Function,
        signature: "docx_metadata(document: Document, title: Text, creator: Text, paragraph_count: Int) -> Docx",
        documentation: "Builds a metadata-only Docx wrapper from already validated Document metadata.",
    },
    BuiltinSymbol {
        name: "spreadsheet_sheet_metadata",
        kind: BuiltinKind::Function,
        signature: "spreadsheet_sheet_metadata(name: Text, row_count: Int, column_count: Int, header_row: Int) -> SpreadsheetSheet",
        documentation: "Builds metadata for a single spreadsheet sheet without reading cell contents.",
    },
    BuiltinSymbol {
        name: "spreadsheet_metadata",
        kind: BuiltinKind::Function,
        signature: "spreadsheet_metadata(document: Document, sheets_json: Text) -> Spreadsheet",
        documentation: "Builds a metadata-only Spreadsheet wrapper from already validated sheet metadata JSON.",
    },
    BuiltinSymbol {
        name: "image_metadata",
        kind: BuiltinKind::Function,
        signature: "image_metadata(document: Document, width: Int, height: Int, format: Text) -> Image",
        documentation: "Builds a metadata-only Image wrapper from already validated image metadata.",
    },
    BuiltinSymbol {
        name: "ocr_result",
        kind: BuiltinKind::Function,
        signature: "ocr_result(image: Image, text: Text, confidence: Float, provider: Text, model: Text) -> OcrResult",
        documentation: "Builds an untrusted OCR result for fake OCR tests and connector handoff adapters.",
    },
    BuiltinSymbol {
        name: "pdf_parse_metadata",
        kind: BuiltinKind::Function,
        signature: "pdf_parse_metadata(document: Document, bytes: Bytes) -> Pdf",
        documentation: "Parses safe PDF metadata from bytes and returns a Pdf wrapper preserving the original Document metadata.",
    },
    BuiltinSymbol {
        name: "docx_parse_metadata",
        kind: BuiltinKind::Function,
        signature: "docx_parse_metadata(document: Document, bytes: Bytes) -> Docx",
        documentation: "Parses safe DOCX metadata from stored ZIP test-fixture bytes and returns a Docx wrapper preserving the original Document metadata.",
    },
    BuiltinSymbol {
        name: "spreadsheet_parse_metadata",
        kind: BuiltinKind::Function,
        signature: "spreadsheet_parse_metadata(document: Document, bytes: Bytes) -> Spreadsheet",
        documentation: "Parses safe sheet-level XLSX metadata from stored ZIP test-fixture bytes without executing formulas.",
    },
    BuiltinSymbol {
        name: "image_parse_metadata",
        kind: BuiltinKind::Function,
        signature: "image_parse_metadata(document: Document, bytes: Bytes) -> Image",
        documentation: "Parses safe PNG/JPEG image dimensions from bytes without decoding pixels or running OCR.",
    },
    BuiltinSymbol {
        name: "datetime_parse_iso",
        kind: BuiltinKind::Function,
        signature: "datetime_parse_iso(value: Text) -> DateTime",
        documentation: "Parses an explicit UTC ISO-8601 timestamp such as `2026-06-26T12:00:00Z` and returns canonical DateTime text.",
    },
    BuiltinSymbol {
        name: "datetime_format_iso",
        kind: BuiltinKind::Function,
        signature: "datetime_format_iso(value: DateTime) -> Text",
        documentation: "Formats a DateTime value as canonical UTC ISO-8601 text.",
    },
    BuiltinSymbol {
        name: "duration_parse_hours",
        kind: BuiltinKind::Function,
        signature: "duration_parse_hours(value: Text) -> Duration<Hour>",
        documentation: "Parses a deterministic hour duration such as `4h` or `1.5 h`.",
    },
    BuiltinSymbol {
        name: "duration_format_hours",
        kind: BuiltinKind::Function,
        signature: "duration_format_hours(value: Duration<Hour>) -> Text",
        documentation: "Formats a Duration<Hour> value as compact hour text such as `4h`.",
    },
    BuiltinSymbol {
        name: "decimal_parse",
        kind: BuiltinKind::Function,
        signature: "decimal_parse(value: Text) -> Decimal",
        documentation: "Parses text into an exact Decimal value without falling back to Float.",
    },
    BuiltinSymbol {
        name: "decimal_format",
        kind: BuiltinKind::Function,
        signature: "decimal_format(value: Decimal) -> Text",
        documentation: "Formats an exact Decimal value as canonical text.",
    },
    BuiltinSymbol {
        name: "exchange_rate",
        kind: BuiltinKind::Function,
        signature: "exchange_rate(from: Text, to: Text, rate: Decimal, source: Text) -> ExchangeRate<From, To>",
        documentation: "Builds an explicit exchange-rate value in an ExchangeRate<From, To> assignment context.",
    },
    BuiltinSymbol {
        name: "convert_money",
        kind: BuiltinKind::Function,
        signature: "convert_money(amount: Money<From>, rate: ExchangeRate<From, To>) -> Money<To>",
        documentation: "Converts Money through an explicit audited exchange-rate boundary.",
    },
    BuiltinSymbol {
        name: "verify_trust",
        kind: BuiltinKind::Function,
        signature: "verify_trust(value)",
        documentation: "Trust gateway for verification-backed promotion of untrusted data.",
    },
    BuiltinSymbol {
        name: "require_human_review",
        kind: BuiltinKind::Function,
        signature: "require_human_review(reason)",
        documentation: "Human-in-the-loop gateway for uncertain or untrusted data.",
    },
    BuiltinSymbol {
        name: "require_human_approval",
        kind: BuiltinKind::Function,
        signature: "require_human_approval(reason) or require_human_approval(action: Text, reason: Text)",
        documentation: "Human approval gateway for high-risk actions and uncertain decisions.",
    },
    BuiltinSymbol {
        name: "reject",
        kind: BuiltinKind::Function,
        signature: "reject(reason)",
        documentation: "Workflow-control builtin that rejects the current operation with a human-readable reason.",
    },
    BuiltinSymbol {
        name: "KZT",
        kind: BuiltinKind::Currency,
        signature: "KZT",
        documentation: "Currency code for Kazakhstani tenge. Commonly used as the type argument in `Money<KZT>`.",
    },
    BuiltinSymbol {
        name: "USD",
        kind: BuiltinKind::Currency,
        signature: "USD",
        documentation: "Currency code for United States dollar. Commonly used as the type argument in `Money<USD>`.",
    },
    BuiltinSymbol {
        name: "EUR",
        kind: BuiltinKind::Currency,
        signature: "EUR",
        documentation: "Currency code for euro. Commonly used as the type argument in `Money<EUR>`.",
    },
    BuiltinSymbol {
        name: "GBP",
        kind: BuiltinKind::Currency,
        signature: "GBP",
        documentation: "Currency code for pound sterling. Commonly used as the type argument in `Money<GBP>`.",
    },
    BuiltinSymbol {
        name: "RUB",
        kind: BuiltinKind::Currency,
        signature: "RUB",
        documentation: "Currency code for Russian ruble. Commonly used as the type argument in `Money<RUB>`.",
    },
    BuiltinSymbol {
        name: "CNY",
        kind: BuiltinKind::Currency,
        signature: "CNY",
        documentation: "Currency code for Chinese yuan. Commonly used as the type argument in `Money<CNY>`.",
    },
];

pub fn symbols() -> &'static [BuiltinSymbol] {
    BUILTIN_SYMBOLS
}

pub fn symbol(name: &str) -> Option<BuiltinSymbol> {
    BUILTIN_SYMBOLS
        .iter()
        .copied()
        .find(|symbol| symbol.name == name)
}

pub fn currency_codes() -> impl Iterator<Item = &'static str> {
    BUILTIN_SYMBOLS
        .iter()
        .filter(|symbol| symbol.kind == BuiltinKind::Currency)
        .map(|symbol| symbol.name)
}
