use serde_json::{json, Map, Value as JsonValue};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentValue {
    pub id: String,
    pub name: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub source: String,
    pub privacy: String,
    pub trust: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfValue {
    pub document: DocumentValue,
    pub page_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocxValue {
    pub document: DocumentValue,
    pub title: String,
    pub creator: String,
    pub paragraph_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpreadsheetSheetValue {
    pub name: String,
    pub row_count: i64,
    pub column_count: i64,
    pub header_row: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpreadsheetValue {
    pub document: DocumentValue,
    pub sheet_count: i64,
    pub sheets: Vec<SpreadsheetSheetValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageValue {
    pub document: DocumentValue,
    pub width: i64,
    pub height: i64,
    pub format: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OcrResultValue {
    pub image: ImageValue,
    pub text: String,
    pub confidence: f64,
    pub provider: String,
    pub model: String,
    pub source: String,
    pub privacy: String,
    pub trust: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedDocumentTextValue {
    pub document: DocumentValue,
    pub text: String,
    pub provider: String,
    pub model: String,
    pub source: String,
    pub privacy: String,
    pub trust: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentExtractionMetadataValue {
    pub document: DocumentValue,
    pub title: String,
    pub author: String,
    pub language: String,
    pub page_count: i64,
    pub provider: String,
    pub source: String,
    pub privacy: String,
    pub trust: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentExtractionErrorValue {
    pub document: DocumentValue,
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub provider: String,
    pub source: String,
    pub privacy: String,
    pub trust: String,
}

impl std::fmt::Display for DocumentValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "<document id=\"{}\" name=\"{}\" mime=\"{}\" size_bytes={}>",
            self.id, self.name, self.mime_type, self.size_bytes
        )
    }
}

impl std::fmt::Display for PdfValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "<pdf id=\"{}\" name=\"{}\" pages={}>",
            self.document.id, self.document.name, self.page_count
        )
    }
}

impl std::fmt::Display for DocxValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "<docx id=\"{}\" name=\"{}\" paragraphs={}>",
            self.document.id, self.document.name, self.paragraph_count
        )
    }
}

impl std::fmt::Display for SpreadsheetSheetValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "<spreadsheet_sheet name=\"{}\" rows={} columns={} header_row={}>",
            self.name, self.row_count, self.column_count, self.header_row
        )
    }
}

impl std::fmt::Display for SpreadsheetValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "<spreadsheet id=\"{}\" name=\"{}\" sheets={}>",
            self.document.id, self.document.name, self.sheet_count
        )
    }
}

impl std::fmt::Display for ImageValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "<image id=\"{}\" name=\"{}\" format=\"{}\" width={} height={}>",
            self.document.id, self.document.name, self.format, self.width, self.height
        )
    }
}

impl std::fmt::Display for OcrResultValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "<ocr_result image=\"{}\" provider=\"{}\" confidence={:.2} trust=\"{}\">",
            self.image.document.id, self.provider, self.confidence, self.trust
        )
    }
}

impl std::fmt::Display for ExtractedDocumentTextValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "<extracted_document_text document=\"{}\" provider=\"{}\" trust=\"{}\">",
            self.document.id, self.provider, self.trust
        )
    }
}

impl std::fmt::Display for DocumentExtractionMetadataValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "<document_extraction_metadata document=\"{}\" provider=\"{}\" pages={}>",
            self.document.id, self.provider, self.page_count
        )
    }
}

impl std::fmt::Display for DocumentExtractionErrorValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "<document_extraction_error document=\"{}\" code=\"{}\" retryable={}>",
            self.document.id, self.code, self.retryable
        )
    }
}

impl DocumentValue {
    pub fn field(&self, field: &str) -> Option<crate::interpreter::Value> {
        match field {
            "id" => Some(crate::interpreter::Value::String(self.id.clone())),
            "name" => Some(crate::interpreter::Value::String(self.name.clone())),
            "mime_type" => Some(crate::interpreter::Value::String(self.mime_type.clone())),
            "size_bytes" => Some(crate::interpreter::Value::Int(self.size_bytes)),
            "source" => Some(crate::interpreter::Value::String(self.source.clone())),
            "privacy" => Some(crate::interpreter::Value::String(self.privacy.clone())),
            "trust" => Some(crate::interpreter::Value::String(self.trust.clone())),
            _ => None,
        }
    }

    pub fn to_json(&self) -> JsonValue {
        json!({
            "id": self.id,
            "name": self.name,
            "mime_type": self.mime_type,
            "size_bytes": self.size_bytes,
            "source": self.source,
            "privacy": self.privacy,
            "trust": self.trust,
        })
    }
}

impl PdfValue {
    pub fn field(&self, field: &str) -> Option<crate::interpreter::Value> {
        match field {
            "document" => Some(crate::interpreter::Value::Document(self.document.clone())),
            "page_count" => Some(crate::interpreter::Value::Int(self.page_count)),
            _ => self.document.field(field),
        }
    }

    pub fn to_json(&self) -> JsonValue {
        json!({
            "document": self.document.to_json(),
            "page_count": self.page_count,
        })
    }
}

impl DocxValue {
    pub fn field(&self, field: &str) -> Option<crate::interpreter::Value> {
        match field {
            "document" => Some(crate::interpreter::Value::Document(self.document.clone())),
            "title" => Some(crate::interpreter::Value::String(self.title.clone())),
            "creator" => Some(crate::interpreter::Value::String(self.creator.clone())),
            "paragraph_count" => Some(crate::interpreter::Value::Int(self.paragraph_count)),
            _ => self.document.field(field),
        }
    }

    pub fn to_json(&self) -> JsonValue {
        json!({
            "document": self.document.to_json(),
            "title": self.title,
            "creator": self.creator,
            "paragraph_count": self.paragraph_count,
        })
    }
}

impl SpreadsheetSheetValue {
    pub fn field(&self, field: &str) -> Option<crate::interpreter::Value> {
        match field {
            "name" => Some(crate::interpreter::Value::String(self.name.clone())),
            "row_count" => Some(crate::interpreter::Value::Int(self.row_count)),
            "column_count" => Some(crate::interpreter::Value::Int(self.column_count)),
            "header_row" => Some(crate::interpreter::Value::Int(self.header_row)),
            _ => None,
        }
    }

    pub fn to_json(&self) -> JsonValue {
        json!({
            "name": self.name,
            "row_count": self.row_count,
            "column_count": self.column_count,
            "header_row": self.header_row,
        })
    }
}

impl SpreadsheetValue {
    pub fn field(&self, field: &str) -> Option<crate::interpreter::Value> {
        match field {
            "document" => Some(crate::interpreter::Value::Document(self.document.clone())),
            "sheet_count" => Some(crate::interpreter::Value::Int(self.sheet_count)),
            "sheets" => Some(crate::interpreter::Value::List(
                self.sheets
                    .iter()
                    .cloned()
                    .map(crate::interpreter::Value::SpreadsheetSheet)
                    .collect(),
            )),
            "sheet_names" => Some(crate::interpreter::Value::List(
                self.sheets
                    .iter()
                    .map(|sheet| crate::interpreter::Value::String(sheet.name.clone()))
                    .collect(),
            )),
            _ => self.document.field(field),
        }
    }

    pub fn to_json(&self) -> JsonValue {
        json!({
            "document": self.document.to_json(),
            "sheet_count": self.sheet_count,
            "sheets": self.sheets.iter().map(SpreadsheetSheetValue::to_json).collect::<Vec<_>>(),
        })
    }
}

impl ImageValue {
    pub fn field(&self, field: &str) -> Option<crate::interpreter::Value> {
        match field {
            "document" => Some(crate::interpreter::Value::Document(self.document.clone())),
            "width" => Some(crate::interpreter::Value::Int(self.width)),
            "height" => Some(crate::interpreter::Value::Int(self.height)),
            "format" => Some(crate::interpreter::Value::String(self.format.clone())),
            _ => self.document.field(field),
        }
    }

    pub fn to_json(&self) -> JsonValue {
        json!({
            "document": self.document.to_json(),
            "width": self.width,
            "height": self.height,
            "format": self.format,
        })
    }
}

impl OcrResultValue {
    pub fn field(&self, field: &str) -> Option<crate::interpreter::Value> {
        match field {
            "image" => Some(crate::interpreter::Value::Image(self.image.clone())),
            "text" => Some(crate::interpreter::Value::String(self.text.clone())),
            "confidence" => Some(crate::interpreter::Value::Float(self.confidence)),
            "provider" => Some(crate::interpreter::Value::String(self.provider.clone())),
            "model" => Some(crate::interpreter::Value::String(self.model.clone())),
            "source" => Some(crate::interpreter::Value::String(self.source.clone())),
            "privacy" => Some(crate::interpreter::Value::String(self.privacy.clone())),
            "trust" => Some(crate::interpreter::Value::String(self.trust.clone())),
            _ => None,
        }
    }

    pub fn to_json(&self) -> JsonValue {
        json!({
            "image": self.image.to_json(),
            "text": self.text,
            "confidence": self.confidence,
            "provider": self.provider,
            "model": self.model,
            "source": self.source,
            "privacy": self.privacy,
            "trust": self.trust,
        })
    }
}

impl ExtractedDocumentTextValue {
    pub fn field(&self, field: &str) -> Option<crate::interpreter::Value> {
        match field {
            "document" => Some(crate::interpreter::Value::Document(self.document.clone())),
            "text" => Some(crate::interpreter::Value::String(self.text.clone())),
            "provider" => Some(crate::interpreter::Value::String(self.provider.clone())),
            "model" => Some(crate::interpreter::Value::String(self.model.clone())),
            "source" => Some(crate::interpreter::Value::String(self.source.clone())),
            "privacy" => Some(crate::interpreter::Value::String(self.privacy.clone())),
            "trust" => Some(crate::interpreter::Value::String(self.trust.clone())),
            _ => self.document.field(field),
        }
    }

    pub fn to_json(&self) -> JsonValue {
        json!({
            "document": self.document.to_json(),
            "text": self.text,
            "provider": self.provider,
            "model": self.model,
            "source": self.source,
            "privacy": self.privacy,
            "trust": self.trust,
        })
    }
}

impl DocumentExtractionMetadataValue {
    pub fn field(&self, field: &str) -> Option<crate::interpreter::Value> {
        match field {
            "document" => Some(crate::interpreter::Value::Document(self.document.clone())),
            "title" => Some(crate::interpreter::Value::String(self.title.clone())),
            "author" => Some(crate::interpreter::Value::String(self.author.clone())),
            "language" => Some(crate::interpreter::Value::String(self.language.clone())),
            "page_count" => Some(crate::interpreter::Value::Int(self.page_count)),
            "provider" => Some(crate::interpreter::Value::String(self.provider.clone())),
            "source" => Some(crate::interpreter::Value::String(self.source.clone())),
            "privacy" => Some(crate::interpreter::Value::String(self.privacy.clone())),
            "trust" => Some(crate::interpreter::Value::String(self.trust.clone())),
            _ => self.document.field(field),
        }
    }

    pub fn to_json(&self) -> JsonValue {
        json!({
            "document": self.document.to_json(),
            "title": self.title,
            "author": self.author,
            "language": self.language,
            "page_count": self.page_count,
            "provider": self.provider,
            "source": self.source,
            "privacy": self.privacy,
            "trust": self.trust,
        })
    }
}

impl DocumentExtractionErrorValue {
    pub fn field(&self, field: &str) -> Option<crate::interpreter::Value> {
        match field {
            "document" => Some(crate::interpreter::Value::Document(self.document.clone())),
            "code" => Some(crate::interpreter::Value::String(self.code.clone())),
            "message" => Some(crate::interpreter::Value::String(self.message.clone())),
            "retryable" => Some(crate::interpreter::Value::Bool(self.retryable)),
            "provider" => Some(crate::interpreter::Value::String(self.provider.clone())),
            "source" => Some(crate::interpreter::Value::String(self.source.clone())),
            "privacy" => Some(crate::interpreter::Value::String(self.privacy.clone())),
            "trust" => Some(crate::interpreter::Value::String(self.trust.clone())),
            _ => self.document.field(field),
        }
    }

    pub fn to_json(&self) -> JsonValue {
        json!({
            "document": self.document.to_json(),
            "code": self.code,
            "message": self.message,
            "retryable": self.retryable,
            "provider": self.provider,
            "source": self.source,
            "privacy": self.privacy,
            "trust": self.trust,
        })
    }
}

pub fn value_from_json(json: &JsonValue) -> Result<DocumentValue, String> {
    let object = json
        .as_object()
        .and_then(|object| {
            object
                .get("$document")
                .and_then(JsonValue::as_object)
                .or(Some(object))
        })
        .ok_or_else(|| "expected object for Document".to_string())?;

    Ok(DocumentValue {
        id: required_string(object, "id")?,
        name: required_string(object, "name")?,
        mime_type: required_string(object, "mime_type")?,
        size_bytes: required_i64(object, "size_bytes")?,
        source: required_string(object, "source")?,
        privacy: required_string(object, "privacy")?,
        trust: required_string(object, "trust")?,
    })
}

pub fn pdf_from_json(json: &JsonValue) -> Result<PdfValue, String> {
    let object = json
        .as_object()
        .and_then(|object| {
            object
                .get("$pdf")
                .and_then(JsonValue::as_object)
                .or(Some(object))
        })
        .ok_or_else(|| "expected object for Pdf".to_string())?;
    let document = object
        .get("document")
        .ok_or_else(|| "Pdf field `document` is required".to_string())
        .and_then(value_from_json)?;
    let page_count = required_i64(object, "page_count")?;
    Ok(PdfValue {
        document,
        page_count,
    })
}

pub fn docx_from_json(json: &JsonValue) -> Result<DocxValue, String> {
    let object = json
        .as_object()
        .and_then(|object| {
            object
                .get("$docx")
                .and_then(JsonValue::as_object)
                .or(Some(object))
        })
        .ok_or_else(|| "expected object for Docx".to_string())?;
    let document = object
        .get("document")
        .ok_or_else(|| "Docx field `document` is required".to_string())
        .and_then(value_from_json)?;
    Ok(DocxValue {
        document,
        title: optional_string(object, "title")?,
        creator: optional_string(object, "creator")?,
        paragraph_count: required_i64(object, "paragraph_count")?,
    })
}

pub fn spreadsheet_sheet_from_json(json: &JsonValue) -> Result<SpreadsheetSheetValue, String> {
    let object = json
        .as_object()
        .and_then(|object| {
            object
                .get("$spreadsheet_sheet")
                .and_then(JsonValue::as_object)
                .or(Some(object))
        })
        .ok_or_else(|| "expected object for SpreadsheetSheet".to_string())?;
    Ok(SpreadsheetSheetValue {
        name: required_string(object, "name")?,
        row_count: required_i64(object, "row_count")?,
        column_count: required_i64(object, "column_count")?,
        header_row: required_i64(object, "header_row")?,
    })
}

pub fn spreadsheet_from_json(json: &JsonValue) -> Result<SpreadsheetValue, String> {
    let object = json
        .as_object()
        .and_then(|object| {
            object
                .get("$spreadsheet")
                .and_then(JsonValue::as_object)
                .or(Some(object))
        })
        .ok_or_else(|| "expected object for Spreadsheet".to_string())?;
    let document = object
        .get("document")
        .ok_or_else(|| "Spreadsheet field `document` is required".to_string())
        .and_then(value_from_json)?;
    let sheets = object
        .get("sheets")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| "Spreadsheet field `sheets` must be an array".to_string())?
        .iter()
        .map(spreadsheet_sheet_from_json)
        .collect::<Result<Vec<_>, _>>()?;
    let sheet_count = object
        .get("sheet_count")
        .and_then(JsonValue::as_i64)
        .unwrap_or(sheets.len() as i64);
    Ok(SpreadsheetValue {
        document,
        sheet_count,
        sheets,
    })
}

pub fn image_from_json(json: &JsonValue) -> Result<ImageValue, String> {
    let object = json
        .as_object()
        .and_then(|object| {
            object
                .get("$image")
                .and_then(JsonValue::as_object)
                .or(Some(object))
        })
        .ok_or_else(|| "expected object for Image".to_string())?;
    let document = object
        .get("document")
        .ok_or_else(|| "Image field `document` is required".to_string())
        .and_then(value_from_json)?;
    Ok(ImageValue {
        document,
        width: required_i64(object, "width")?,
        height: required_i64(object, "height")?,
        format: required_string(object, "format")?,
    })
}

pub fn ocr_result_from_json(json: &JsonValue) -> Result<OcrResultValue, String> {
    let object = json
        .as_object()
        .and_then(|object| {
            object
                .get("$ocr_result")
                .and_then(JsonValue::as_object)
                .or(Some(object))
        })
        .ok_or_else(|| "expected object for OcrResult".to_string())?;
    let image = object
        .get("image")
        .ok_or_else(|| "OcrResult field `image` is required".to_string())
        .and_then(image_from_json)?;
    Ok(OcrResultValue {
        image,
        text: required_string(object, "text")?,
        confidence: required_f64(object, "confidence")?,
        provider: required_string(object, "provider")?,
        model: required_string(object, "model")?,
        source: optional_string(object, "source")?,
        privacy: optional_string(object, "privacy")?,
        trust: optional_string(object, "trust")?,
    })
}

pub fn extracted_document_text_from_json(
    json: &JsonValue,
) -> Result<ExtractedDocumentTextValue, String> {
    let object = json
        .as_object()
        .and_then(|object| {
            object
                .get("$extracted_document_text")
                .and_then(JsonValue::as_object)
                .or(Some(object))
        })
        .ok_or_else(|| "expected object for ExtractedDocumentText".to_string())?;
    let document = object
        .get("document")
        .ok_or_else(|| "ExtractedDocumentText field `document` is required".to_string())
        .and_then(value_from_json)?;
    Ok(ExtractedDocumentTextValue {
        document,
        text: required_string(object, "text")?,
        provider: required_string(object, "provider")?,
        model: required_string(object, "model")?,
        source: optional_string(object, "source")?,
        privacy: optional_string(object, "privacy")?,
        trust: optional_string(object, "trust")?,
    })
}

pub fn document_extraction_metadata_from_json(
    json: &JsonValue,
) -> Result<DocumentExtractionMetadataValue, String> {
    let object = json
        .as_object()
        .and_then(|object| {
            object
                .get("$document_extraction_metadata")
                .and_then(JsonValue::as_object)
                .or(Some(object))
        })
        .ok_or_else(|| "expected object for DocumentExtractionMetadata".to_string())?;
    let document = object
        .get("document")
        .ok_or_else(|| "DocumentExtractionMetadata field `document` is required".to_string())
        .and_then(value_from_json)?;
    Ok(DocumentExtractionMetadataValue {
        document,
        title: optional_string(object, "title")?,
        author: optional_string(object, "author")?,
        language: optional_string(object, "language")?,
        page_count: required_i64(object, "page_count")?,
        provider: required_string(object, "provider")?,
        source: optional_string(object, "source")?,
        privacy: optional_string(object, "privacy")?,
        trust: optional_string(object, "trust")?,
    })
}

pub fn document_extraction_error_from_json(
    json: &JsonValue,
) -> Result<DocumentExtractionErrorValue, String> {
    let object = json
        .as_object()
        .and_then(|object| {
            object
                .get("$document_extraction_error")
                .and_then(JsonValue::as_object)
                .or(Some(object))
        })
        .ok_or_else(|| "expected object for DocumentExtractionError".to_string())?;
    let document = object
        .get("document")
        .ok_or_else(|| "DocumentExtractionError field `document` is required".to_string())
        .and_then(value_from_json)?;
    Ok(DocumentExtractionErrorValue {
        document,
        code: required_string(object, "code")?,
        message: required_string(object, "message")?,
        retryable: required_bool(object, "retryable")?,
        provider: required_string(object, "provider")?,
        source: optional_string(object, "source")?,
        privacy: optional_string(object, "privacy")?,
        trust: optional_string(object, "trust")?,
    })
}

pub fn connector_json(value: &DocumentValue) -> JsonValue {
    json!({ "$document": value.to_json() })
}

pub fn pdf_connector_json(value: &PdfValue) -> JsonValue {
    json!({ "$pdf": value.to_json() })
}

pub fn docx_connector_json(value: &DocxValue) -> JsonValue {
    json!({ "$docx": value.to_json() })
}

pub fn spreadsheet_sheet_connector_json(value: &SpreadsheetSheetValue) -> JsonValue {
    json!({ "$spreadsheet_sheet": value.to_json() })
}

pub fn spreadsheet_connector_json(value: &SpreadsheetValue) -> JsonValue {
    json!({ "$spreadsheet": value.to_json() })
}

pub fn image_connector_json(value: &ImageValue) -> JsonValue {
    json!({ "$image": value.to_json() })
}

pub fn ocr_result_connector_json(value: &OcrResultValue) -> JsonValue {
    json!({ "$ocr_result": value.to_json() })
}

pub fn extracted_document_text_connector_json(value: &ExtractedDocumentTextValue) -> JsonValue {
    json!({ "$extracted_document_text": value.to_json() })
}

pub fn document_extraction_metadata_connector_json(
    value: &DocumentExtractionMetadataValue,
) -> JsonValue {
    json!({ "$document_extraction_metadata": value.to_json() })
}

pub fn document_extraction_error_connector_json(value: &DocumentExtractionErrorValue) -> JsonValue {
    json!({ "$document_extraction_error": value.to_json() })
}

pub fn parse_pdf_metadata(document: DocumentValue, bytes: &[u8]) -> Result<PdfValue, String> {
    if !bytes.starts_with(b"%PDF-") {
        return Err("malformed PDF: missing %PDF header".to_string());
    }
    if !find_bytes(bytes, b"%%EOF") {
        return Err("malformed PDF: missing EOF marker".to_string());
    }
    let page_count = count_pdf_pages(bytes);
    if page_count == 0 {
        return Err("malformed PDF: no page objects found".to_string());
    }
    Ok(PdfValue {
        document,
        page_count: page_count as i64,
    })
}

pub fn parse_docx_metadata(document: DocumentValue, bytes: &[u8]) -> Result<DocxValue, String> {
    let entries = stored_zip_entries(bytes, "DOCX", "DOCX")?;
    let document_xml = entries
        .iter()
        .find(|(name, _)| name == "word/document.xml")
        .map(|(_, data)| String::from_utf8_lossy(data).to_string())
        .ok_or_else(|| "malformed DOCX: missing word/document.xml".to_string())?;
    let core_xml = entries
        .iter()
        .find(|(name, _)| name == "docProps/core.xml")
        .map(|(_, data)| String::from_utf8_lossy(data).to_string())
        .unwrap_or_default();
    Ok(DocxValue {
        document,
        title: extract_xml_text(&core_xml, "dc:title").unwrap_or_default(),
        creator: extract_xml_text(&core_xml, "dc:creator").unwrap_or_default(),
        paragraph_count: count_occurrences(document_xml.as_bytes(), b"<w:p") as i64,
    })
}

pub fn parse_spreadsheet_metadata(
    document: DocumentValue,
    bytes: &[u8],
) -> Result<SpreadsheetValue, String> {
    let entries = stored_zip_entries(bytes, "spreadsheet", "XLSX")?;
    let workbook_xml = entries
        .iter()
        .find(|(name, _)| name == "xl/workbook.xml")
        .map(|(_, data)| String::from_utf8_lossy(data).to_string())
        .ok_or_else(|| "malformed spreadsheet: missing xl/workbook.xml".to_string())?;
    let sheet_names = extract_sheet_names(&workbook_xml);
    if sheet_names.is_empty() {
        return Err("malformed spreadsheet: workbook has no sheets".to_string());
    }

    let mut worksheet_entries = entries
        .iter()
        .filter(|(name, _)| name.starts_with("xl/worksheets/sheet") && name.ends_with(".xml"))
        .collect::<Vec<_>>();
    worksheet_entries.sort_by(|(left, _), (right, _)| left.cmp(right));
    if worksheet_entries.is_empty() {
        return Err("malformed spreadsheet: missing worksheet XML entries".to_string());
    }

    let mut sheets = Vec::new();
    for (index, (_, data)) in worksheet_entries.iter().enumerate() {
        let xml = String::from_utf8_lossy(data);
        let fallback_name = format!("Sheet{}", index + 1);
        let name = sheet_names.get(index).cloned().unwrap_or(fallback_name);
        let (row_count, column_count) = worksheet_dimensions(&xml);
        sheets.push(SpreadsheetSheetValue {
            name,
            row_count,
            column_count,
            header_row: detect_header_row(&xml),
        });
    }

    Ok(SpreadsheetValue {
        document,
        sheet_count: sheets.len() as i64,
        sheets,
    })
}

pub fn parse_image_metadata(document: DocumentValue, bytes: &[u8]) -> Result<ImageValue, String> {
    let (format, width, height) = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        parse_png_dimensions(bytes)?
    } else if bytes.starts_with(b"\xff\xd8") {
        parse_jpeg_dimensions(bytes)?
    } else {
        return Err("malformed image: unsupported or missing image header".to_string());
    };
    Ok(ImageValue {
        document,
        width,
        height,
        format,
    })
}

pub fn ocr_result(
    image: ImageValue,
    text: String,
    confidence: f64,
    provider: String,
    model: String,
) -> Result<OcrResultValue, String> {
    if !(0.0..=1.0).contains(&confidence) {
        return Err("OCR confidence must be between 0.0 and 1.0".to_string());
    }
    Ok(OcrResultValue {
        source: format!("OCR:{provider}"),
        privacy: image.document.privacy.clone(),
        trust: "untrusted".to_string(),
        image,
        text,
        confidence,
        provider,
        model,
    })
}

pub fn extracted_document_text(
    document: DocumentValue,
    text: String,
    provider: String,
    model: String,
) -> ExtractedDocumentTextValue {
    ExtractedDocumentTextValue {
        source: format!("DocumentExtraction:{provider}"),
        privacy: document.privacy.clone(),
        trust: "untrusted".to_string(),
        document,
        text,
        provider,
        model,
    }
}

pub fn document_extraction_metadata(
    document: DocumentValue,
    title: String,
    author: String,
    language: String,
    page_count: i64,
    provider: String,
) -> DocumentExtractionMetadataValue {
    DocumentExtractionMetadataValue {
        source: format!("DocumentExtraction:{provider}"),
        privacy: document.privacy.clone(),
        trust: "untrusted".to_string(),
        document,
        title,
        author,
        language,
        page_count,
        provider,
    }
}

pub fn document_extraction_error(
    document: DocumentValue,
    code: String,
    message: String,
    retryable: bool,
    provider: String,
) -> DocumentExtractionErrorValue {
    DocumentExtractionErrorValue {
        source: format!("DocumentExtraction:{provider}"),
        privacy: document.privacy.clone(),
        trust: "trusted".to_string(),
        document,
        code,
        message,
        retryable,
        provider,
    }
}

fn required_string(object: &Map<String, JsonValue>, key: &str) -> Result<String, String> {
    object
        .get(key)
        .and_then(JsonValue::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("Document field `{key}` must be a string"))
}

fn optional_string(object: &Map<String, JsonValue>, key: &str) -> Result<String, String> {
    object
        .get(key)
        .and_then(JsonValue::as_str)
        .map(str::to_string)
        .or_else(|| Some(String::new()))
        .ok_or_else(|| format!("Document field `{key}` must be a string"))
}

fn required_i64(object: &Map<String, JsonValue>, key: &str) -> Result<i64, String> {
    object
        .get(key)
        .and_then(JsonValue::as_i64)
        .ok_or_else(|| format!("Document field `{key}` must be an integer"))
}

fn required_f64(object: &Map<String, JsonValue>, key: &str) -> Result<f64, String> {
    object
        .get(key)
        .and_then(JsonValue::as_f64)
        .ok_or_else(|| format!("Document field `{key}` must be a number"))
}

fn required_bool(object: &Map<String, JsonValue>, key: &str) -> Result<bool, String> {
    object
        .get(key)
        .and_then(JsonValue::as_bool)
        .ok_or_else(|| format!("Document field `{key}` must be a boolean"))
}

fn parse_png_dimensions(bytes: &[u8]) -> Result<(String, i64, i64), String> {
    if bytes.len() < 24 {
        return Err("malformed PNG: truncated IHDR".to_string());
    }
    if &bytes[12..16] != b"IHDR" {
        return Err("malformed PNG: missing IHDR chunk".to_string());
    }
    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]) as i64;
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]) as i64;
    if width <= 0 || height <= 0 {
        return Err("malformed PNG: invalid dimensions".to_string());
    }
    Ok(("png".to_string(), width, height))
}

fn parse_jpeg_dimensions(bytes: &[u8]) -> Result<(String, i64, i64), String> {
    let mut offset = 2usize;
    while offset + 4 <= bytes.len() {
        while offset < bytes.len() && bytes[offset] == 0xff {
            offset += 1;
        }
        if offset >= bytes.len() {
            break;
        }
        let marker = bytes[offset];
        offset += 1;
        if marker == 0xd9 || marker == 0xda {
            break;
        }
        if offset + 2 > bytes.len() {
            return Err("malformed JPEG: truncated segment length".to_string());
        }
        let segment_len = u16::from_be_bytes([bytes[offset], bytes[offset + 1]]) as usize;
        if segment_len < 2 || offset + segment_len > bytes.len() {
            return Err("malformed JPEG: invalid segment length".to_string());
        }
        if matches!(
            marker,
            0xc0 | 0xc1
                | 0xc2
                | 0xc3
                | 0xc5
                | 0xc6
                | 0xc7
                | 0xc9
                | 0xca
                | 0xcb
                | 0xcd
                | 0xce
                | 0xcf
        ) {
            if segment_len < 7 {
                return Err("malformed JPEG: truncated SOF segment".to_string());
            }
            let height = u16::from_be_bytes([bytes[offset + 3], bytes[offset + 4]]) as i64;
            let width = u16::from_be_bytes([bytes[offset + 5], bytes[offset + 6]]) as i64;
            if width <= 0 || height <= 0 {
                return Err("malformed JPEG: invalid dimensions".to_string());
            }
            return Ok(("jpeg".to_string(), width, height));
        }
        offset += segment_len;
    }
    Err("malformed JPEG: missing SOF dimensions".to_string())
}

fn count_pdf_pages(bytes: &[u8]) -> usize {
    bytes
        .windows(b"/Type /Page".len())
        .enumerate()
        .filter(|(index, window)| {
            *window == b"/Type /Page"
                && !bytes
                    .get(index + b"/Type /Page".len())
                    .is_some_and(|byte| *byte == b's')
        })
        .count()
}

fn count_occurrences(haystack: &[u8], needle: &[u8]) -> usize {
    if needle.is_empty() {
        return 0;
    }
    haystack
        .windows(needle.len())
        .filter(|window| *window == needle)
        .count()
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn stored_zip_entries(
    bytes: &[u8],
    document_kind: &str,
    compression_label: &str,
) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut offset = 0usize;
    let mut entries = Vec::new();
    while offset + 30 <= bytes.len() {
        if &bytes[offset..offset + 4] != b"PK\x03\x04" {
            break;
        }
        let method = le_u16(bytes, offset + 8, document_kind)?;
        let compressed_size = le_u32(bytes, offset + 18, document_kind)? as usize;
        let uncompressed_size = le_u32(bytes, offset + 22, document_kind)? as usize;
        let name_len = le_u16(bytes, offset + 26, document_kind)? as usize;
        let extra_len = le_u16(bytes, offset + 28, document_kind)? as usize;
        let name_start = offset + 30;
        let data_start = name_start
            .checked_add(name_len)
            .and_then(|value| value.checked_add(extra_len))
            .ok_or_else(|| format!("malformed {document_kind}: ZIP entry offset overflow"))?;
        let data_end = data_start
            .checked_add(compressed_size)
            .ok_or_else(|| format!("malformed {document_kind}: ZIP entry size overflow"))?;
        if data_end > bytes.len() {
            return Err(format!("malformed {document_kind}: truncated ZIP entry"));
        }
        let name = std::str::from_utf8(&bytes[name_start..name_start + name_len])
            .map_err(|err| format!("malformed {document_kind}: invalid ZIP entry name: {err}"))?
            .to_string();
        if method != 0 {
            return Err(format!(
                "unsupported {compression_label} compression method {method}; first slice supports stored test fixtures"
            ));
        }
        if compressed_size != uncompressed_size {
            return Err(format!(
                "malformed {document_kind}: stored ZIP entry size mismatch"
            ));
        }
        entries.push((name, bytes[data_start..data_end].to_vec()));
        offset = data_end;
    }
    if entries.is_empty() {
        return Err(format!(
            "malformed {document_kind}: missing ZIP local file entries"
        ));
    }
    Ok(entries)
}

fn le_u16(bytes: &[u8], offset: usize, document_kind: &str) -> Result<u16, String> {
    let slice = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| format!("malformed {document_kind}: truncated ZIP header"))?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn le_u32(bytes: &[u8], offset: usize, document_kind: &str) -> Result<u32, String> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| format!("malformed {document_kind}: truncated ZIP header"))?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn extract_xml_text(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    Some(xml[start..end].to_string())
}

fn extract_sheet_names(workbook_xml: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = workbook_xml;
    while let Some(index) = rest.find("<sheet") {
        rest = &rest[index + "<sheet".len()..];
        if let Some(end) = rest.find('>') {
            let tag = &rest[..end];
            if let Some(name) = xml_attr(tag, "name") {
                names.push(name);
            }
            rest = &rest[end..];
        } else {
            break;
        }
    }
    names
}

fn worksheet_dimensions(xml: &str) -> (i64, i64) {
    if let Some(ref_value) = xml
        .find("<dimension")
        .and_then(|index| xml[index..].find('>').map(|end| &xml[index..index + end]))
        .and_then(|tag| xml_attr(tag, "ref"))
    {
        if let Some((rows, columns)) = dimensions_from_ref(&ref_value) {
            return (rows, columns);
        }
    }
    (
        count_occurrences(xml.as_bytes(), b"<row") as i64,
        max_column_from_cells(xml),
    )
}

fn dimensions_from_ref(value: &str) -> Option<(i64, i64)> {
    let (_, end) = value.rsplit_once(':').unwrap_or(("", value));
    let (column, row) = split_cell_ref(end)?;
    Some((row, column))
}

fn max_column_from_cells(xml: &str) -> i64 {
    let mut max_column = 0;
    let mut rest = xml;
    while let Some(index) = rest.find("<c ") {
        rest = &rest[index + 3..];
        if let Some(end) = rest.find('>') {
            let tag = &rest[..end];
            if let Some(cell_ref) = xml_attr(tag, "r") {
                if let Some((column, _)) = split_cell_ref(&cell_ref) {
                    max_column = max_column.max(column);
                }
            }
            rest = &rest[end..];
        } else {
            break;
        }
    }
    max_column
}

fn detect_header_row(xml: &str) -> i64 {
    let Some(row_index) = xml.find("<row") else {
        return 0;
    };
    let Some(row_end) = xml[row_index..].find("</row>") else {
        return 0;
    };
    let row = &xml[row_index..row_index + row_end];
    let Some(tag_end) = row.find('>') else {
        return 0;
    };
    let row_tag = &row[..tag_end];
    let row_number = xml_attr(row_tag, "r")
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(1);
    if row.contains(" t=\"s\"")
        || row.contains(" t=\"str\"")
        || row.contains(" t=\"inlineStr\"")
        || row.contains("<is>")
    {
        row_number
    } else {
        0
    }
}

fn split_cell_ref(value: &str) -> Option<(i64, i64)> {
    let mut column = 0i64;
    let mut row = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphabetic() {
            column = column
                .checked_mul(26)?
                .checked_add((ch.to_ascii_uppercase() as u8 - b'A' + 1) as i64)?;
        } else if ch.is_ascii_digit() {
            row.push(ch);
        }
    }
    if column == 0 || row.is_empty() {
        return None;
    }
    Some((column, row.parse().ok()?))
}

fn xml_attr(tag: &str, name: &str) -> Option<String> {
    let pattern = format!("{name}=\"");
    let start = tag.find(&pattern)? + pattern.len();
    let end = tag[start..].find('"')? + start;
    Some(tag[start..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        ocr_result, parse_docx_metadata, parse_image_metadata, parse_pdf_metadata,
        parse_spreadsheet_metadata, value_from_json, DocumentValue,
    };
    use serde_json::json;

    #[test]
    fn decodes_document_metadata() {
        let value = value_from_json(&json!({
            "id": "doc_1",
            "name": "contract.pdf",
            "mime_type": "application/pdf",
            "size_bytes": 4096,
            "source": "Upload",
            "privacy": "private",
            "trust": "untrusted"
        }))
        .unwrap();

        assert_eq!(
            value,
            DocumentValue {
                id: "doc_1".to_string(),
                name: "contract.pdf".to_string(),
                mime_type: "application/pdf".to_string(),
                size_bytes: 4096,
                source: "Upload".to_string(),
                privacy: "private".to_string(),
                trust: "untrusted".to_string(),
            }
        );
    }

    #[test]
    fn rejects_missing_document_fields() {
        assert!(value_from_json(&json!({ "id": "doc_1" })).is_err());
    }

    #[test]
    fn parses_pdf_page_count_metadata() {
        let pdf = b"%PDF-1.7
1 0 obj << /Type /Pages /Count 2 >> endobj
2 0 obj << /Type /Page /Parent 1 0 R >> endobj
3 0 obj << /Type /Page /Parent 1 0 R >> endobj
%%EOF";

        let value = parse_pdf_metadata(test_document("application/pdf"), pdf).unwrap();

        assert_eq!(value.page_count, 2);
        assert_eq!(value.document.mime_type, "application/pdf");
    }

    #[test]
    fn rejects_malformed_pdf_metadata() {
        let err = parse_pdf_metadata(test_document("application/pdf"), b"not a pdf").unwrap_err();
        assert!(err.contains("missing %PDF header"));
    }

    #[test]
    fn parses_stored_docx_metadata() {
        let bytes = stored_zip_fixture(&[
            (
                "docProps/core.xml",
                "<cp:coreProperties><dc:title>Contract</dc:title><dc:creator>Ada</dc:creator></cp:coreProperties>",
            ),
            (
                "word/document.xml",
                "<w:document><w:body><w:p/><w:p/></w:body></w:document>",
            ),
        ]);

        let value = parse_docx_metadata(
            test_document(
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            ),
            &bytes,
        )
        .unwrap();

        assert_eq!(value.title, "Contract");
        assert_eq!(value.creator, "Ada");
        assert_eq!(value.paragraph_count, 2);
    }

    #[test]
    fn rejects_compressed_docx_metadata_in_first_slice() {
        let bytes = zip_fixture_with_method("word/document.xml", "<w:document/>", 8);

        let err = parse_docx_metadata(
            test_document(
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            ),
            &bytes,
        )
        .unwrap_err();

        assert!(err.contains("unsupported DOCX compression method 8"));
    }

    #[test]
    fn parses_stored_spreadsheet_metadata() {
        let bytes = stored_zip_fixture(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Revenue" sheetId="1"/><sheet name="Costs" sheetId="2"/></sheets></workbook>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><dimension ref="A1:C3"/><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Name</t></is></c><c r="B1" t="inlineStr"><is><t>Total</t></is></c></row><row r="2"><c r="A2"/><c r="C2"/></row></sheetData></worksheet>"#,
            ),
            (
                "xl/worksheets/sheet2.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"/><c r="B1"/></row><row r="2"><c r="A2"/></row></sheetData></worksheet>"#,
            ),
        ]);

        let value = parse_spreadsheet_metadata(
            test_document("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
            &bytes,
        )
        .unwrap();

        assert_eq!(value.sheet_count, 2);
        assert_eq!(value.sheets[0].name, "Revenue");
        assert_eq!(value.sheets[0].row_count, 3);
        assert_eq!(value.sheets[0].column_count, 3);
        assert_eq!(value.sheets[0].header_row, 1);
        assert_eq!(value.sheets[1].name, "Costs");
        assert_eq!(value.sheets[1].row_count, 2);
        assert_eq!(value.sheets[1].column_count, 2);
        assert_eq!(value.sheets[1].header_row, 0);
    }

    #[test]
    fn rejects_compressed_spreadsheet_metadata_in_first_slice() {
        let bytes = zip_fixture_with_method("xl/workbook.xml", "<workbook/>", 8);

        let err = parse_spreadsheet_metadata(
            test_document("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
            &bytes,
        )
        .unwrap_err();

        assert!(err.contains("unsupported XLSX compression method 8"));
    }

    #[test]
    fn parses_png_image_metadata() {
        let value = parse_image_metadata(test_document("image/png"), &png_bytes(640, 480)).unwrap();

        assert_eq!(value.format, "png");
        assert_eq!(value.width, 640);
        assert_eq!(value.height, 480);
        assert_eq!(value.document.mime_type, "image/png");
    }

    #[test]
    fn parses_jpeg_image_metadata() {
        let value =
            parse_image_metadata(test_document("image/jpeg"), &jpeg_bytes(1024, 768)).unwrap();

        assert_eq!(value.format, "jpeg");
        assert_eq!(value.width, 1024);
        assert_eq!(value.height, 768);
    }

    #[test]
    fn rejects_malformed_image_metadata() {
        let err = parse_image_metadata(test_document("image/png"), b"not an image").unwrap_err();
        assert!(err.contains("unsupported or missing image header"));
    }

    #[test]
    fn builds_untrusted_ocr_result() {
        let image = parse_image_metadata(test_document("image/png"), &png_bytes(100, 50)).unwrap();
        let value = ocr_result(
            image,
            "Invoice total".to_string(),
            0.91,
            "fake-ocr".to_string(),
            "fixture-v1".to_string(),
        )
        .unwrap();

        assert_eq!(value.text, "Invoice total");
        assert_eq!(value.confidence, 0.91);
        assert_eq!(value.source, "OCR:fake-ocr");
        assert_eq!(value.privacy, "private");
        assert_eq!(value.trust, "untrusted");
    }

    fn test_document(mime_type: &str) -> DocumentValue {
        DocumentValue {
            id: "doc_1".to_string(),
            name: "fixture".to_string(),
            mime_type: mime_type.to_string(),
            size_bytes: 128,
            source: "Upload".to_string(),
            privacy: "private".to_string(),
            trust: "untrusted".to_string(),
        }
    }

    fn stored_zip_fixture(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut out = Vec::new();
        for (name, contents) in entries {
            out.extend(zip_local_entry(name, contents.as_bytes(), 0));
        }
        out
    }

    fn zip_fixture_with_method(name: &str, contents: &str, method: u16) -> Vec<u8> {
        zip_local_entry(name, contents.as_bytes(), method)
    }

    fn zip_local_entry(name: &str, contents: &[u8], method: u16) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend(b"PK\x03\x04");
        out.extend(20u16.to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out.extend(method.to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out.extend(0u32.to_le_bytes());
        out.extend((contents.len() as u32).to_le_bytes());
        out.extend((contents.len() as u32).to_le_bytes());
        out.extend((name.len() as u16).to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out.extend(name.as_bytes());
        out.extend(contents);
        out
    }

    fn png_bytes(width: u32, height: u32) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend(b"\x89PNG\r\n\x1a\n");
        out.extend(13u32.to_be_bytes());
        out.extend(b"IHDR");
        out.extend(width.to_be_bytes());
        out.extend(height.to_be_bytes());
        out.extend([8, 2, 0, 0, 0]);
        out.extend(0u32.to_be_bytes());
        out
    }

    fn jpeg_bytes(width: u16, height: u16) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend([0xff, 0xd8]);
        out.extend([0xff, 0xe0]);
        out.extend(4u16.to_be_bytes());
        out.extend([0, 0]);
        out.extend([0xff, 0xc0]);
        out.extend(17u16.to_be_bytes());
        out.push(8);
        out.extend(height.to_be_bytes());
        out.extend(width.to_be_bytes());
        out.extend([3, 1, 0x11, 0, 2, 0x11, 0, 3, 0x11, 0]);
        out.extend([0xff, 0xd9]);
        out
    }
}
