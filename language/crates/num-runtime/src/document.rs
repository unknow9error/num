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

impl std::fmt::Display for DocumentValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "<document id=\"{}\" name=\"{}\" mime=\"{}\" size_bytes={}>",
            self.id, self.name, self.mime_type, self.size_bytes
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

pub fn connector_json(value: &DocumentValue) -> JsonValue {
    json!({ "$document": value.to_json() })
}

fn required_string(object: &Map<String, JsonValue>, key: &str) -> Result<String, String> {
    object
        .get(key)
        .and_then(JsonValue::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("Document field `{key}` must be a string"))
}

fn required_i64(object: &Map<String, JsonValue>, key: &str) -> Result<i64, String> {
    object
        .get(key)
        .and_then(JsonValue::as_i64)
        .ok_or_else(|| format!("Document field `{key}` must be an integer"))
}

#[cfg(test)]
mod tests {
    use super::{value_from_json, DocumentValue};
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
}
