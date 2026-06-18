use serde_json::Value;
use std::fs;
use std::path::Path;

pub fn import_openapi(path: &Path, module_name: Option<&str>) -> Result<String, String> {
    let source = fs::read_to_string(path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    let document = parse_openapi_document(path, &source)?;
    Ok(render_openapi_connector(&document, module_name))
}

fn parse_openapi_document(path: &Path, source: &str) -> Result<Value, String> {
    if looks_like_json(path, source) {
        return serde_json::from_str(source)
            .map_err(|err| format!("failed to parse OpenAPI JSON {}: {err}", path.display()));
    }

    match serde_yaml::from_str::<Value>(source) {
        Ok(value) => Ok(value),
        Err(yaml_error) if path.extension().is_none_or(|extension| !is_yaml_extension(extension)) => {
            serde_json::from_str(source).map_err(|json_error| {
                format!(
                    "failed to parse OpenAPI document {} as YAML ({yaml_error}) or JSON ({json_error})",
                    path.display()
                )
            })
        }
        Err(err) => Err(format!(
            "failed to parse OpenAPI YAML {}: {err}",
            path.display()
        )),
    }
}

fn looks_like_json(path: &Path, source: &str) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
        || source.trim_start().starts_with(['{', '['])
}

fn is_yaml_extension(extension: &std::ffi::OsStr) -> bool {
    extension
        .to_str()
        .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "yaml" | "yml"))
}

pub fn render_openapi_connector(document: &Value, module_name: Option<&str>) -> String {
    let module_name = module_name.unwrap_or("generated.openapi");
    let connector_name = document
        .pointer("/info/title")
        .and_then(Value::as_str)
        .map(to_identifier)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "openapi".to_string());

    let mut out = String::new();
    out.push_str("module ");
    out.push_str(module_name);
    out.push_str("\n\n");

    for (name, schema) in component_schemas(document) {
        render_type(&mut out, name, schema);
        out.push('\n');
    }

    out.push_str("connector ");
    out.push_str(&connector_name);
    out.push_str(" {\n");
    for operation in operations(document) {
        for metadata in &operation.unsupported_metadata {
            out.push_str("    // ");
            out.push_str(&metadata.comment());
            out.push('\n');
        }
        out.push_str("    ");
        out.push_str(&operation.name);
        out.push('(');
        for (index, param) in operation.params.iter().enumerate() {
            if index > 0 {
                out.push_str(", ");
            }
            out.push_str(&param.name);
            out.push_str(": ");
            out.push_str(&param.ty);
        }
        out.push_str(") -> ");
        out.push_str(&operation.result);
        out.push('\n');
    }
    out.push_str("}\n");

    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Operation {
    name: String,
    params: Vec<OperationParam>,
    result: String,
    unsupported_metadata: Vec<UnsupportedOpenApiMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OperationParam {
    name: String,
    ty: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum UnsupportedOpenApiMetadata {
    Callback {
        operation: String,
        name: String,
    },
    Link {
        operation: String,
        response: String,
        name: String,
    },
}

fn component_schemas(document: &Value) -> Vec<(&str, &Value)> {
    let mut schemas = document
        .pointer("/components/schemas")
        .and_then(Value::as_object)
        .map(|schemas| {
            schemas
                .iter()
                .map(|(name, schema)| (name.as_str(), schema))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    schemas.sort_by(|left, right| left.0.cmp(right.0));
    schemas
}

fn render_type(out: &mut String, name: &str, schema: &Value) {
    out.push_str("type ");
    out.push_str(&to_type_name(name));
    out.push_str(" {\n");

    let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
        out.push_str("    value: Json\n");
        out.push_str("}\n");
        return;
    };

    let mut fields = properties.iter().collect::<Vec<_>>();
    fields.sort_by(|left, right| left.0.cmp(right.0));
    for (field, schema) in fields {
        out.push_str("    ");
        out.push_str(&to_identifier(field));
        out.push_str(": ");
        out.push_str(&schema_type(schema));
        out.push('\n');
    }
    out.push_str("}\n");
}

fn operations(document: &Value) -> Vec<Operation> {
    let mut operations = Vec::new();
    let Some(paths) = document.get("paths").and_then(Value::as_object) else {
        return operations;
    };

    for (path, path_item) in paths {
        let Some(methods) = path_item.as_object() else {
            continue;
        };
        for method in ["get", "post", "put", "patch", "delete"] {
            let Some(operation) = methods.get(method) else {
                continue;
            };
            let name = operation
                .get("operationId")
                .and_then(Value::as_str)
                .map(to_identifier)
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| fallback_operation_name(method, path));
            operations.push(Operation {
                unsupported_metadata: unsupported_operation_metadata(operation, &name),
                name,
                params: operation_params(operation),
                result: operation_result(operation),
            });
        }
    }

    operations.sort_by(|left, right| left.name.cmp(&right.name));
    operations
}

fn unsupported_operation_metadata(
    operation: &Value,
    operation_name: &str,
) -> Vec<UnsupportedOpenApiMetadata> {
    let mut metadata = Vec::new();

    if let Some(callbacks) = operation.get("callbacks").and_then(Value::as_object) {
        let mut callbacks = callbacks.keys().collect::<Vec<_>>();
        callbacks.sort();
        for callback in callbacks {
            metadata.push(UnsupportedOpenApiMetadata::Callback {
                operation: operation_name.to_string(),
                name: sanitize_comment_text(callback),
            });
        }
    }

    if let Some(responses) = operation.get("responses").and_then(Value::as_object) {
        let mut responses = responses.iter().collect::<Vec<_>>();
        responses.sort_by(|left, right| left.0.cmp(right.0));
        for (response_code, response) in responses {
            let Some(links) = response.get("links").and_then(Value::as_object) else {
                continue;
            };
            let mut links = links.keys().collect::<Vec<_>>();
            links.sort();
            for link in links {
                metadata.push(UnsupportedOpenApiMetadata::Link {
                    operation: operation_name.to_string(),
                    response: sanitize_comment_text(response_code),
                    name: sanitize_comment_text(link),
                });
            }
        }
    }

    metadata
}

impl UnsupportedOpenApiMetadata {
    fn comment(&self) -> String {
        match self {
            Self::Callback { operation, name } => format!(
                "OpenAPI callback `{name}` on operation `{operation}` is preserved as unsupported metadata; runtime generation is not implemented yet"
            ),
            Self::Link {
                operation,
                response,
                name,
            } => format!(
                "OpenAPI link `{name}` on operation `{operation}` response `{response}` is preserved as unsupported metadata; runtime generation is not implemented yet"
            ),
        }
    }
}

fn operation_params(operation: &Value) -> Vec<OperationParam> {
    let mut params = Vec::new();

    if let Some(raw_params) = operation.get("parameters").and_then(Value::as_array) {
        for param in raw_params {
            let Some(name) = param.get("name").and_then(Value::as_str) else {
                continue;
            };
            let ty = param
                .get("schema")
                .map(schema_type)
                .unwrap_or_else(|| "Json".to_string());
            params.push(OperationParam {
                name: to_identifier(name),
                ty,
            });
        }
    }

    if let Some(schema) = operation
        .pointer("/requestBody/content/application~1json/schema")
        .or_else(|| {
            operation.pointer("/requestBody/content/application~1x-www-form-urlencoded/schema")
        })
    {
        params.push(OperationParam {
            name: "body".to_string(),
            ty: schema_type(schema),
        });
    }

    params
}

fn operation_result(operation: &Value) -> String {
    let Some(responses) = operation.get("responses").and_then(Value::as_object) else {
        return "Unit".to_string();
    };

    for code in ["200", "201", "202", "204", "default"] {
        let Some(response) = responses.get(code) else {
            continue;
        };
        if code == "204" {
            return "Unit".to_string();
        }
        if let Some(schema) = response.pointer("/content/application~1json/schema") {
            return schema_type(schema);
        }
    }

    "Unit".to_string()
}

fn schema_type(schema: &Value) -> String {
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        return reference
            .rsplit('/')
            .next()
            .map(to_type_name)
            .unwrap_or_else(|| "Json".to_string());
    }

    match schema.get("type").and_then(Value::as_str) {
        Some("string") => "Text".to_string(),
        Some("integer") => "Int".to_string(),
        Some("number") => "Float".to_string(),
        Some("boolean") => "Bool".to_string(),
        Some("array") => {
            let inner = schema
                .get("items")
                .map(schema_type)
                .unwrap_or_else(|| "Json".to_string());
            format!("List<{inner}>")
        }
        Some("object") => "Json".to_string(),
        _ => "Json".to_string(),
    }
}

fn fallback_operation_name(method: &str, path: &str) -> String {
    let mut name = method.to_string();
    for part in path.split('/') {
        let part = part.trim_matches(|ch| ch == '{' || ch == '}');
        if part.is_empty() {
            continue;
        }
        name.push('_');
        name.push_str(&to_identifier(part));
    }
    name
}

fn to_type_name(value: &str) -> String {
    let mut output = String::new();
    for part in identifier_parts(value) {
        output.push_str(&capitalize_identifier_part(&part));
    }
    if output.is_empty() {
        "GeneratedType".to_string()
    } else {
        output
    }
}

fn to_identifier(value: &str) -> String {
    let mut parts = identifier_parts(value).into_iter();
    let mut output = parts
        .next()
        .map(|part| lower_first(&normalize_identifier_part(&part)))
        .unwrap_or_else(|| "value".to_string());
    for part in parts {
        output.push_str(&capitalize_identifier_part(&part));
    }
    if output.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        output.insert(0, '_');
    }
    output
}

fn identifier_parts(value: &str) -> Vec<String> {
    value
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

fn normalize_identifier_part(part: &str) -> String {
    if part.chars().all(|ch| ch.is_ascii_uppercase()) {
        part.to_ascii_lowercase()
    } else {
        part.to_string()
    }
}

fn capitalize_identifier_part(part: &str) -> String {
    let part = normalize_identifier_part(part);
    let mut chars = part.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut output = String::new();
    output.extend(first.to_uppercase());
    output.push_str(chars.as_str());
    output
}

fn lower_first(part: &str) -> String {
    let mut chars = part.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut output = String::new();
    output.extend(first.to_lowercase());
    output.push_str(chars.as_str());
    output
}

fn sanitize_comment_text(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            '\r' | '\n' | '\t' => ' ',
            '`' => '\'',
            ch if ch.is_control() => ' ',
            ch => ch,
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("num_openapi_{name}_{stamp}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    const BASIC_OPENAPI_JSON: &str = r##"
{
  "openapi": "3.0.0",
  "info": { "title": "Billing API", "version": "1.0.0" },
  "components": {
    "schemas": {
      "RefundRequest": {
        "type": "object",
        "properties": {
          "payment_id": { "type": "string" },
          "amount": { "type": "number" }
        }
      },
      "RefundResponse": {
        "type": "object",
        "properties": {
          "id": { "type": "string" },
          "approved": { "type": "boolean" }
        }
      }
    }
  },
  "paths": {
    "/refunds": {
      "post": {
        "operationId": "create_refund",
        "requestBody": {
          "content": {
            "application/json": {
              "schema": { "$ref": "#/components/schemas/RefundRequest" }
            }
          }
        },
        "responses": {
          "201": {
            "content": {
              "application/json": {
                "schema": { "$ref": "#/components/schemas/RefundResponse" }
              }
            }
          }
        }
      }
    }
  }
}
"##;

    const BASIC_OPENAPI_YAML: &str = r##"
openapi: 3.0.0
info:
  title: Billing API
  version: 1.0.0
components:
  schemas:
    RefundRequest:
      type: object
      properties:
        payment_id:
          type: string
        amount:
          type: number
    RefundResponse:
      type: object
      properties:
        id:
          type: string
        approved:
          type: boolean
paths:
  /refunds:
    post:
      operationId: create_refund
      requestBody:
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/RefundRequest'
      responses:
        '201':
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/RefundResponse'
"##;

    #[test]
    fn renders_components_and_connector_methods() {
        let document: Value = serde_json::from_str(BASIC_OPENAPI_JSON).unwrap();

        let rendered = render_openapi_connector(&document, Some("generated.billing"));

        assert!(rendered.contains("module generated.billing"));
        assert!(rendered.contains("type RefundRequest"));
        assert!(rendered.contains("payment_id: Text"));
        assert!(rendered.contains("connector billingApi"));
        assert!(rendered.contains("create_refund(body: RefundRequest) -> RefundResponse"));
        assert!(num_compiler::check("generated.num", &rendered).is_empty());
    }

    #[test]
    fn parses_yaml_and_json_to_equivalent_output() {
        let json = parse_openapi_document(Path::new("billing.json"), BASIC_OPENAPI_JSON).unwrap();
        let yaml = parse_openapi_document(Path::new("billing.yaml"), BASIC_OPENAPI_YAML).unwrap();

        assert_eq!(
            render_openapi_connector(&json, Some("generated.billing")),
            render_openapi_connector(&yaml, Some("generated.billing"))
        );
    }

    #[test]
    fn imports_yaml_and_yml_openapi_files() {
        let root = temp_dir("yaml_import");
        let yaml_path = root.join("billing.yaml");
        let yml_path = root.join("billing.yml");
        fs::write(&yaml_path, BASIC_OPENAPI_YAML).unwrap();
        fs::write(&yml_path, BASIC_OPENAPI_YAML).unwrap();

        let yaml_rendered = import_openapi(&yaml_path, Some("generated.billing")).unwrap();
        let yml_rendered = import_openapi(&yml_path, Some("generated.billing")).unwrap();

        assert_eq!(yaml_rendered, yml_rendered);
        assert!(yaml_rendered.contains("module generated.billing"));
        assert!(yaml_rendered.contains("connector billingApi"));
        assert!(yaml_rendered.contains("create_refund(body: RefundRequest) -> RefundResponse"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_yaml_reports_clear_error() {
        let root = temp_dir("invalid_yaml");
        let path = root.join("broken.yaml");
        fs::write(&path, "openapi: [").unwrap();

        let err = import_openapi(&path, Some("generated.broken")).unwrap_err();

        assert!(err.contains("failed to parse OpenAPI YAML"));
        assert!(err.contains("broken.yaml"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn preserves_callbacks_and_links_as_unsupported_metadata_comments() {
        let document: Value = serde_json::from_str(
            r##"
{
  "openapi": "3.0.0",
  "info": { "title": "Billing API", "version": "1.0.0" },
  "paths": {
    "/refunds": {
      "post": {
        "operationId": "create_refund",
        "callbacks": {
          "refundStatusChanged": {
            "{$request.body#/callbackUrl}": {
              "post": {
                "responses": { "200": { "description": "ok" } }
              }
            }
          }
        },
        "responses": {
          "201": {
            "description": "created",
            "links": {
              "getRefund": {
                "operationId": "get_refund",
                "parameters": { "refundId": "$response.body#/id" }
              }
            }
          }
        }
      }
    }
  }
}
"##,
        )
        .unwrap();

        let rendered = render_openapi_connector(&document, Some("generated.billing"));

        assert!(rendered.contains(
            "// OpenAPI callback `refundStatusChanged` on operation `create_refund` is preserved as unsupported metadata; runtime generation is not implemented yet"
        ));
        assert!(rendered.contains(
            "// OpenAPI link `getRefund` on operation `create_refund` response `201` is preserved as unsupported metadata; runtime generation is not implemented yet"
        ));
        assert!(rendered.contains("create_refund() -> Unit"));
        assert!(num_compiler::check("generated.num", &rendered).is_empty());
    }
}
