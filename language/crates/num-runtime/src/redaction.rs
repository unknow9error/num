use crate::connectors::{ConnectorCallContext, ConnectorError};
use crate::hashing;
use crate::interpreter::Value;

pub const REDACTION_MARKER: &str = "<redacted>";

pub fn redact_text(text: &str) -> String {
    redact_sensitive_assignments(text)
}

pub fn redact_text_with_values(text: &str, values: &[String]) -> String {
    let mut redacted = redact_text(text);
    for value in values {
        if value.is_empty() || value == REDACTION_MARKER {
            continue;
        }
        redacted = redacted.replace(value, REDACTION_MARKER);
    }
    redacted
}

pub fn redact_connector_error(
    error: &ConnectorError,
    context: &ConnectorCallContext,
    args: &[Value],
) -> ConnectorError {
    let values = secret_argument_values(context, args);
    ConnectorError {
        code: error.code.clone(),
        message: redact_text_with_values(&error.message, &values),
        retryable: error.retryable,
    }
}

pub fn redacted_value(value: &Value) -> Value {
    match value {
        Value::Secret(_) => Value::String(REDACTION_MARKER.to_string()),
        Value::Brand(name, inner) => Value::Brand(name.clone(), Box::new(redacted_value(inner))),
        Value::Uncertain(inner, confidence) => {
            Value::Uncertain(Box::new(redacted_value(inner)), *confidence)
        }
        Value::List(items) => Value::List(items.iter().map(redacted_value).collect()),
        Value::Map(entries) => Value::Map(
            entries
                .iter()
                .map(|(key, value)| (redacted_value(key), redacted_value(value)))
                .collect(),
        ),
        Value::Set(items) => Value::Set(items.iter().map(redacted_value).collect()),
        Value::Queue(items) => Value::Queue(items.iter().map(redacted_value).collect()),
        Value::Stack(items) => Value::Stack(items.iter().map(redacted_value).collect()),
        Value::Stream(items) => Value::Stream(items.iter().map(redacted_value).collect()),
        Value::Struct(name, fields) => Value::Struct(
            name.clone(),
            fields
                .iter()
                .map(|(key, value)| (key.clone(), redacted_value(value)))
                .collect(),
        ),
        Value::Enum(name, variant, payload) => Value::Enum(
            name.clone(),
            variant.clone(),
            payload
                .as_ref()
                .map(|payload| Box::new(redacted_value(payload))),
        ),
        other => other.clone(),
    }
}

fn secret_argument_values(context: &ConnectorCallContext, args: &[Value]) -> Vec<String> {
    let mut values = Vec::new();
    for label in &context.arg_labels {
        if !is_secret_label(label.privacy.as_deref(), &label.ty) {
            continue;
        }
        if let Some(value) = args.get(label.index) {
            collect_scalar_values(value, &mut values);
        }
    }

    for value in args {
        collect_explicit_secret_values(value, &mut values);
    }

    values.sort();
    values.dedup();
    values
}

fn is_secret_label(privacy: Option<&str>, ty: &str) -> bool {
    privacy == Some("secret") || ty.trim() == "Secret" || ty.trim().starts_with("Secret<")
}

fn collect_explicit_secret_values(value: &Value, values: &mut Vec<String>) {
    match value {
        Value::Secret(inner) => collect_scalar_values(inner, values),
        Value::Brand(_, inner) | Value::Uncertain(inner, _) => {
            collect_explicit_secret_values(inner, values)
        }
        Value::List(items) => {
            for item in items {
                collect_explicit_secret_values(item, values);
            }
        }
        Value::Map(entries) => {
            for (key, value) in entries {
                collect_explicit_secret_values(key, values);
                collect_explicit_secret_values(value, values);
            }
        }
        Value::Set(items) => {
            for item in items {
                collect_explicit_secret_values(item, values);
            }
        }
        Value::Queue(items) | Value::Stack(items) | Value::Stream(items) => {
            for item in items {
                collect_explicit_secret_values(item, values);
            }
        }
        Value::Struct(_, fields) => {
            for value in fields.values() {
                collect_explicit_secret_values(value, values);
            }
        }
        Value::Enum(_, _, Some(payload)) => collect_explicit_secret_values(payload, values),
        _ => {}
    }
}

fn collect_scalar_values(value: &Value, values: &mut Vec<String>) {
    match value {
        Value::Null => {}
        Value::Bool(value) => values.push(value.to_string()),
        Value::Int(value) => values.push(value.to_string()),
        Value::Float(value) => values.push(value.to_string()),
        Value::Decimal(value) => values.push(value.to_string()),
        Value::String(value) => values.push(value.clone()),
        Value::Bytes(value) => values.push(hashing::base64_encode(value)),
        Value::Xml(value) => values.push(value.clone()),
        Value::Document(value) => {
            values.push(value.id.clone());
            values.push(value.name.clone());
            values.push(value.mime_type.clone());
            values.push(value.size_bytes.to_string());
            values.push(value.source.clone());
            values.push(value.privacy.clone());
            values.push(value.trust.clone());
        }
        Value::Pdf(value) => {
            collect_scalar_values(&Value::Document(value.document.clone()), values)
        }
        Value::Docx(value) => {
            collect_scalar_values(&Value::Document(value.document.clone()), values);
            values.push(value.title.clone());
            values.push(value.creator.clone());
            values.push(value.paragraph_count.to_string());
        }
        Value::SpreadsheetSheet(value) => {
            values.push(value.name.clone());
            values.push(value.row_count.to_string());
            values.push(value.column_count.to_string());
            values.push(value.header_row.to_string());
        }
        Value::Spreadsheet(value) => {
            collect_scalar_values(&Value::Document(value.document.clone()), values);
            values.push(value.sheet_count.to_string());
            for sheet in &value.sheets {
                collect_scalar_values(&Value::SpreadsheetSheet(sheet.clone()), values);
            }
        }
        Value::Image(value) => {
            collect_scalar_values(&Value::Document(value.document.clone()), values);
            values.push(value.width.to_string());
            values.push(value.height.to_string());
            values.push(value.format.clone());
        }
        Value::OcrResult(value) => {
            collect_scalar_values(&Value::Image(value.image.clone()), values);
            values.push(value.text.clone());
            values.push(value.confidence.to_string());
            values.push(value.provider.clone());
            values.push(value.model.clone());
            values.push(value.source.clone());
            values.push(value.privacy.clone());
            values.push(value.trust.clone());
        }
        Value::Money(minor_units, currency) => {
            values.push(minor_units.to_string());
            values.push(format!("{minor_units}:{currency}"));
        }
        Value::Brand(_, inner) | Value::Uncertain(inner, _) | Value::Secret(inner) => {
            collect_scalar_values(inner, values)
        }
        Value::List(items) => {
            for item in items {
                collect_scalar_values(item, values);
            }
        }
        Value::Map(entries) => {
            for (key, value) in entries {
                collect_scalar_values(key, values);
                collect_scalar_values(value, values);
            }
        }
        Value::Set(items) => {
            for item in items {
                collect_scalar_values(item, values);
            }
        }
        Value::Queue(items) | Value::Stack(items) | Value::Stream(items) => {
            for item in items {
                collect_scalar_values(item, values);
            }
        }
        Value::Struct(_, fields) => {
            for value in fields.values() {
                collect_scalar_values(value, values);
            }
        }
        Value::Enum(_, _, Some(payload)) => collect_scalar_values(payload, values),
        Value::Enum(_, _, None) => {}
        Value::Quantity(amount, unit) => values.push(format!("{amount}:{unit}")),
    }
}

fn redact_sensitive_assignments(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut index = 0;
    while index < text.len() {
        let rest = &text[index..];
        let Some((key_offset, key)) = find_sensitive_key(rest) else {
            output.push_str(rest);
            break;
        };
        output.push_str(&rest[..key_offset]);
        let key_start = index + key_offset;
        let key_end = key_start + key.len();
        output.push_str(&text[key_start..key_end]);

        let mut cursor = key_end;
        let after_key = &text[cursor..];
        let separator = after_key.chars().next();
        let separator_len = separator
            .filter(|ch| {
                matches!(ch, '=' | ':') || (*ch == ' ' && matches!(key, "authorization" | "bearer"))
            })
            .map(char::len_utf8)
            .unwrap_or(0);
        if separator_len == 0 {
            index = key_end;
            continue;
        }
        output.push_str(&text[cursor..cursor + separator_len]);
        cursor += separator_len;

        while cursor < text.len() {
            let Some(ch) = text[cursor..].chars().next() else {
                break;
            };
            if ch.is_whitespace() || matches!(ch, ',' | ';' | '}' | ']') {
                break;
            }
            cursor += ch.len_utf8();
        }
        output.push_str(REDACTION_MARKER);
        index = cursor;
    }
    output
}

fn find_sensitive_key(text: &str) -> Option<(usize, &'static str)> {
    const KEYS: [&str; 7] = [
        "secret",
        "token",
        "api_key",
        "apikey",
        "password",
        "authorization",
        "bearer",
    ];

    let lower = text.to_ascii_lowercase();
    KEYS.iter()
        .filter_map(|key| lower.find(key).map(|index| (index, *key)))
        .min_by_key(|(index, _)| *index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connectors::{ConnectorArgLabel, ConnectorCallContext};

    #[test]
    fn redacts_known_secret_argument_values_from_connector_errors() {
        let context = ConnectorCallContext {
            connector: "secrets".to_string(),
            method_name: "send".to_string(),
            method: "secrets.send".to_string(),
            capability: "connector:secrets.send".to_string(),
            actor: "actor".to_string(),
            tenant: "tenant".to_string(),
            correlation_id: "corr".to_string(),
            request_id: "req".to_string(),
            policy_decision: "compile_time_checked".to_string(),
            arg_labels: vec![ConnectorArgLabel {
                index: 0,
                name: "token".to_string(),
                ty: "Secret<Text>".to_string(),
                source: Some("Vault".to_string()),
                privacy: Some("secret".to_string()),
                trust: None,
            }],
        };
        let error = ConnectorError::execution("upstream echoed sk_live_123");

        let redacted = redact_connector_error(
            &error,
            &context,
            &[Value::Secret(Box::new(Value::String(
                "sk_live_123".to_string(),
            )))],
        );

        assert_eq!(redacted.message, "upstream echoed <redacted>");
    }

    #[test]
    fn redacts_common_sensitive_assignments_without_known_values() {
        assert_eq!(
            redact_text("connector failed: token=sk_live_123 password:pw"),
            "connector failed: token=<redacted> password:<redacted>"
        );
        assert_eq!(
            redact_text("secret store reports secret not found"),
            "secret store reports secret not found"
        );
    }
}
