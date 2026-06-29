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

pub fn connector_json(value: &DocumentValue) -> JsonValue {
    json!({ "$document": value.to_json() })
}

pub fn pdf_connector_json(value: &PdfValue) -> JsonValue {
    json!({ "$pdf": value.to_json() })
}

pub fn docx_connector_json(value: &DocxValue) -> JsonValue {
    json!({ "$docx": value.to_json() })
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
    let entries = stored_zip_entries(bytes)?;
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

fn stored_zip_entries(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut offset = 0usize;
    let mut entries = Vec::new();
    while offset + 30 <= bytes.len() {
        if &bytes[offset..offset + 4] != b"PK\x03\x04" {
            break;
        }
        let method = le_u16(bytes, offset + 8)?;
        let compressed_size = le_u32(bytes, offset + 18)? as usize;
        let uncompressed_size = le_u32(bytes, offset + 22)? as usize;
        let name_len = le_u16(bytes, offset + 26)? as usize;
        let extra_len = le_u16(bytes, offset + 28)? as usize;
        let name_start = offset + 30;
        let data_start = name_start
            .checked_add(name_len)
            .and_then(|value| value.checked_add(extra_len))
            .ok_or_else(|| "malformed DOCX: ZIP entry offset overflow".to_string())?;
        let data_end = data_start
            .checked_add(compressed_size)
            .ok_or_else(|| "malformed DOCX: ZIP entry size overflow".to_string())?;
        if data_end > bytes.len() {
            return Err("malformed DOCX: truncated ZIP entry".to_string());
        }
        let name = std::str::from_utf8(&bytes[name_start..name_start + name_len])
            .map_err(|err| format!("malformed DOCX: invalid ZIP entry name: {err}"))?
            .to_string();
        if method != 0 {
            return Err(format!(
                "unsupported DOCX compression method {method}; first slice supports stored test fixtures"
            ));
        }
        if compressed_size != uncompressed_size {
            return Err("malformed DOCX: stored ZIP entry size mismatch".to_string());
        }
        entries.push((name, bytes[data_start..data_end].to_vec()));
        offset = data_end;
    }
    if entries.is_empty() {
        return Err("malformed DOCX: missing ZIP local file entries".to_string());
    }
    Ok(entries)
}

fn le_u16(bytes: &[u8], offset: usize) -> Result<u16, String> {
    let slice = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| "malformed DOCX: truncated ZIP header".to_string())?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn le_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| "malformed DOCX: truncated ZIP header".to_string())?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn extract_xml_text(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    Some(xml[start..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::{parse_docx_metadata, parse_pdf_metadata, value_from_json, DocumentValue};
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
}
