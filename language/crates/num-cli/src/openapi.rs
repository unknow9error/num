use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
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

    let operations = operations(document);
    render_permission_scaffold(&mut out, &connector_name, &operations);

    for (name, schema) in component_schemas(document) {
        render_type(&mut out, document, name, schema);
        out.push('\n');
    }

    out.push_str("connector ");
    out.push_str(&connector_name);
    out.push_str(" {\n");
    for metadata in security_scheme_metadata(document) {
        out.push_str("    // ");
        out.push_str(&metadata.comment());
        out.push('\n');
    }
    for operation in &operations {
        out.push_str("    // ");
        out.push_str(&operation.permission.comment());
        out.push('\n');
        for placeholder in &operation.policy_placeholders {
            out.push_str("    // ");
            out.push_str(placeholder);
            out.push('\n');
        }
        for pagination in &operation.pagination_metadata {
            out.push_str("    // ");
            out.push_str(&pagination.comment());
            out.push('\n');
        }
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
    permission: OpenApiPermissionScaffold,
    policy_placeholders: Vec<String>,
    pagination_metadata: Vec<OpenApiPaginationMetadata>,
    unsupported_metadata: Vec<UnsupportedOpenApiMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OperationParam {
    name: String,
    ty: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OpenApiPermissionScaffold {
    name: String,
    source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OpenApiPaginationMetadata {
    operation: String,
    style: String,
    hints: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum UnsupportedOpenApiMetadata {
    SecurityRequirement {
        operation: String,
        requirements: String,
    },
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum OpenApiSecuritySchemeMetadata {
    ApiKey {
        name: String,
        location: String,
        parameter: String,
    },
    Http {
        name: String,
        scheme: String,
    },
    OAuth2 {
        name: String,
        flows: String,
    },
    Unsupported {
        name: String,
        detail: String,
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

fn render_type(out: &mut String, document: &Value, name: &str, schema: &Value) {
    out.push_str("type ");
    out.push_str(&to_type_name(name));
    out.push_str(" {\n");

    let rendered = render_openapi_type_fields(document, schema);
    for comment in rendered.comments {
        out.push_str("    // ");
        out.push_str(&comment);
        out.push('\n');
    }

    if rendered.fields.is_empty() {
        out.push_str("    value: Json\n");
        out.push_str("}\n");
        return;
    }

    for (field, ty) in rendered.fields {
        out.push_str("    ");
        out.push_str(&field);
        out.push_str(": ");
        out.push_str(&ty);
        out.push('\n');
    }
    out.push_str("}\n");
}

#[derive(Debug, Default)]
struct OpenApiTypeRender {
    fields: BTreeMap<String, String>,
    comments: Vec<String>,
    conflict_fields: BTreeSet<String>,
}

fn render_openapi_type_fields(document: &Value, schema: &Value) -> OpenApiTypeRender {
    let mut rendered = OpenApiTypeRender::default();
    collect_openapi_object_fields(document, schema, &mut rendered);
    rendered
}

fn collect_openapi_object_fields(
    document: &Value,
    schema: &Value,
    rendered: &mut OpenApiTypeRender,
) -> bool {
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        let Some(resolved) = resolve_local_ref(document, reference) else {
            rendered.comments.push(format!(
                "OpenAPI allOf reference `{}` could not be resolved; review schema manually.",
                sanitize_comment_text(reference)
            ));
            return false;
        };
        return collect_openapi_object_fields(document, resolved, rendered);
    }

    let mut merged_any = false;
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        for (field, field_schema) in properties {
            merge_openapi_object_field(rendered, field, field_schema);
        }
        merged_any = true;
    }

    if let Some(items) = schema.get("allOf").and_then(Value::as_array) {
        for item in items {
            if !collect_openapi_object_fields(document, item, rendered) {
                rendered.comments.push(
                    "OpenAPI allOf member could not be merged; review schema manually.".to_string(),
                );
            }
        }
        merged_any = true;
    }

    merged_any
}

fn merge_openapi_object_field(rendered: &mut OpenApiTypeRender, field: &str, field_schema: &Value) {
    let field_name = to_identifier(field);
    let ty = schema_type(field_schema);
    match rendered.fields.get(&field_name) {
        Some(existing) if existing == &ty => {}
        Some(_) => {
            rendered
                .fields
                .insert(field_name.clone(), "Json".to_string());
            if rendered.conflict_fields.insert(field_name.clone()) {
                rendered.comments.push(format!(
                    "OpenAPI allOf conflict on field `{}`; generated as Json for review.",
                    sanitize_comment_text(&field_name)
                ));
            }
        }
        None => {
            rendered.fields.insert(field_name, ty);
        }
    }
}

fn operations(document: &Value) -> Vec<Operation> {
    let mut operations = Vec::new();
    let global_security = document.get("security");
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
            let tags = operation_tags(operation);
            let permission = operation_permission_scaffold(operation, method, path, &name, &tags);
            operations.push(Operation {
                policy_placeholders: operation_policy_placeholders(
                    operation,
                    method,
                    path,
                    &permission,
                    document,
                    global_security,
                ),
                pagination_metadata: operation_pagination_metadata(operation, &name, document),
                unsupported_metadata: unsupported_operation_metadata(
                    operation,
                    &name,
                    global_security,
                ),
                name,
                permission,
                params: operation_params(operation),
                result: operation_result(operation),
            });
        }
    }

    operations.sort_by(|left, right| left.name.cmp(&right.name));
    operations
}

fn render_permission_scaffold(out: &mut String, connector_name: &str, operations: &[Operation]) {
    if operations.is_empty() {
        return;
    }

    out.push_str("// OpenAPI review-required permission candidates. Generated names are scaffolding; review scopes before wiring them into roles or service routes.\n");
    let mut permissions = operations
        .iter()
        .map(|operation| operation.permission.name.as_str())
        .collect::<Vec<_>>();
    permissions.sort();
    permissions.dedup();
    for permission in permissions {
        out.push_str("permission ");
        out.push_str(permission);
        out.push('\n');
    }
    out.push_str("// OpenAPI review-required policy scaffold for connector `");
    out.push_str(connector_name);
    out.push_str("`:\n");
    out.push_str("// policy OpenApi");
    out.push_str(&to_type_name(connector_name));
    out.push_str("DataSharing {\n");
    out.push_str("//     // Add narrow allow rules only after reviewing request/response fields and authentication requirements.\n");
    out.push_str("//     // Example: allow private UserInput -> ");
    out.push_str(connector_name);
    out.push_str(".<method>\n");
    out.push_str("// }\n\n");
}

fn unsupported_operation_metadata(
    operation: &Value,
    operation_name: &str,
    global_security: Option<&Value>,
) -> Vec<UnsupportedOpenApiMetadata> {
    let mut metadata = Vec::new();

    if let Some(requirements) = operation
        .get("security")
        .or(global_security)
        .and_then(security_requirements_comment)
    {
        metadata.push(UnsupportedOpenApiMetadata::SecurityRequirement {
            operation: operation_name.to_string(),
            requirements,
        });
    }

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
            Self::SecurityRequirement {
                operation,
                requirements,
            } => format!(
                "OpenAPI security requirement for operation `{operation}`: {requirements}; authentication binding is not generated yet"
            ),
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

impl OpenApiPaginationMetadata {
    fn comment(&self) -> String {
        format!(
            "OpenAPI review-required pagination metadata for operation `{}`: {} pagination hints [{}]; generated connector is not an executable paginated client yet",
            self.operation,
            self.style,
            self.hints.join(", ")
        )
    }
}

fn operation_pagination_metadata(
    operation: &Value,
    operation_name: &str,
    document: &Value,
) -> Vec<OpenApiPaginationMetadata> {
    let mut parameter_names = BTreeSet::new();
    if let Some(parameters) = operation.get("parameters").and_then(Value::as_array) {
        for parameter in parameters {
            if let Some(name) = parameter.get("name").and_then(Value::as_str) {
                parameter_names.insert(name.to_ascii_lowercase());
            }
        }
    }

    let mut metadata = Vec::new();
    let mut offset_hints = Vec::new();
    for name in ["limit", "offset"] {
        if parameter_names.contains(name) {
            offset_hints.push(format!("parameter:{name}"));
        }
    }
    if !offset_hints.is_empty() && parameter_names.contains("limit") {
        metadata.push(OpenApiPaginationMetadata {
            operation: operation_name.to_string(),
            style: "limit/offset".to_string(),
            hints: offset_hints,
        });
    }

    let mut page_hints = Vec::new();
    for name in ["page", "page_size", "pagesize", "per_page", "perpage"] {
        if parameter_names.contains(name) {
            page_hints.push(format!("parameter:{name}"));
        }
    }
    if page_hints.iter().any(|hint| hint == "parameter:page") && page_hints.len() > 1 {
        metadata.push(OpenApiPaginationMetadata {
            operation: operation_name.to_string(),
            style: "page/pageSize".to_string(),
            hints: page_hints,
        });
    }

    let mut cursor_hints = Vec::new();
    for name in [
        "cursor",
        "after",
        "before",
        "starting_after",
        "ending_before",
    ] {
        if parameter_names.contains(name) {
            cursor_hints.push(format!("parameter:{name}"));
        }
    }
    let mut response_hints = BTreeSet::new();
    collect_pagination_response_hints(operation, document, &mut response_hints);
    for hint in &response_hints {
        if is_cursor_response_hint(hint) {
            cursor_hints.push(hint.clone());
        }
    }
    if !cursor_hints.is_empty() {
        metadata.push(OpenApiPaginationMetadata {
            operation: operation_name.to_string(),
            style: "cursor/next-link".to_string(),
            hints: cursor_hints,
        });
    } else if !response_hints.is_empty() {
        metadata.push(OpenApiPaginationMetadata {
            operation: operation_name.to_string(),
            style: "response next-link".to_string(),
            hints: response_hints.into_iter().collect(),
        });
    }

    metadata
}

fn collect_pagination_response_hints(
    operation: &Value,
    document: &Value,
    hints: &mut BTreeSet<String>,
) {
    let Some(responses) = operation.get("responses").and_then(Value::as_object) else {
        return;
    };
    for (code, response) in responses {
        collect_pagination_schema_hints(
            response.pointer("/content/application~1json/schema"),
            &format!("response {code}"),
            document,
            hints,
        );
    }
}

fn collect_pagination_schema_hints(
    schema: Option<&Value>,
    context: &str,
    document: &Value,
    hints: &mut BTreeSet<String>,
) {
    let Some(schema) = schema else {
        return;
    };
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        if let Some(resolved) = resolve_local_ref(document, reference) {
            collect_pagination_schema_hints(Some(resolved), context, document, hints);
        }
    }
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        for (field, schema) in properties {
            if is_pagination_response_field(field) {
                hints.insert(format!(
                    "{}:{}",
                    sanitize_comment_text(context),
                    sanitize_comment_text(field)
                ));
            }
            collect_pagination_schema_hints(Some(schema), field, document, hints);
        }
    }
    if let Some(items) = schema.get("items") {
        collect_pagination_schema_hints(Some(items), context, document, hints);
    }
    for key in ["allOf", "oneOf", "anyOf"] {
        if let Some(items) = schema.get(key).and_then(Value::as_array) {
            for item in items {
                collect_pagination_schema_hints(Some(item), context, document, hints);
            }
        }
    }
}

fn is_pagination_response_field(name: &str) -> bool {
    let normalized = normalize_pagination_name(name);
    matches!(
        normalized.as_str(),
        "next"
            | "nextlink"
            | "nexturl"
            | "nextpage"
            | "nextpageurl"
            | "nextpagetoken"
            | "nexttoken"
            | "nextcursor"
            | "cursor"
            | "endcursor"
            | "hasnextpage"
    )
}

fn is_cursor_response_hint(hint: &str) -> bool {
    let normalized = normalize_pagination_name(hint);
    normalized.contains("cursor") || normalized.contains("token") || normalized.contains("next")
}

fn normalize_pagination_name(name: &str) -> String {
    name.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase()
}

impl OpenApiSecuritySchemeMetadata {
    fn comment(&self) -> String {
        match self {
            Self::ApiKey {
                name,
                location,
                parameter,
            } => format!(
                "OpenAPI security scheme `{name}` uses apiKey in `{location}` parameter `{parameter}`; authentication binding is not generated yet"
            ),
            Self::Http { name, scheme } => format!(
                "OpenAPI security scheme `{name}` uses HTTP `{scheme}` authentication; authentication binding is not generated yet"
            ),
            Self::OAuth2 { name, flows } => format!(
                "OpenAPI security scheme `{name}` uses OAuth2 flows `{flows}`; OAuth runtime generation is not implemented yet"
            ),
            Self::Unsupported { name, detail } => format!(
                "OpenAPI security scheme `{name}` is preserved as unsupported metadata ({detail}); authentication binding is not generated yet"
            ),
        }
    }
}

fn security_scheme_metadata(document: &Value) -> Vec<OpenApiSecuritySchemeMetadata> {
    let mut schemes = document
        .pointer("/components/securitySchemes")
        .and_then(Value::as_object)
        .map(|schemes| {
            schemes
                .iter()
                .map(|(name, scheme)| security_scheme_metadata_entry(name, scheme))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    schemes.sort_by(|left, right| left.name().cmp(right.name()));
    schemes
}

fn security_scheme_metadata_entry(name: &str, scheme: &Value) -> OpenApiSecuritySchemeMetadata {
    let name = sanitize_comment_text(name);
    let Some(scheme) = scheme.as_object() else {
        return OpenApiSecuritySchemeMetadata::Unsupported {
            name,
            detail: "scheme value is not an object".to_string(),
        };
    };

    match scheme.get("type").and_then(Value::as_str) {
        Some("apiKey") => OpenApiSecuritySchemeMetadata::ApiKey {
            name,
            location: scheme
                .get("in")
                .and_then(Value::as_str)
                .map(sanitize_comment_text)
                .unwrap_or_else(|| "unknown".to_string()),
            parameter: scheme
                .get("name")
                .and_then(Value::as_str)
                .map(sanitize_comment_text)
                .unwrap_or_else(|| "unknown".to_string()),
        },
        Some("http") => OpenApiSecuritySchemeMetadata::Http {
            name,
            scheme: scheme
                .get("scheme")
                .and_then(Value::as_str)
                .map(sanitize_comment_text)
                .unwrap_or_else(|| "unknown".to_string()),
        },
        Some("oauth2") => OpenApiSecuritySchemeMetadata::OAuth2 {
            name,
            flows: oauth2_flows_comment(scheme.get("flows")),
        },
        Some(kind) => OpenApiSecuritySchemeMetadata::Unsupported {
            name,
            detail: format!(
                "type `{}` is not represented by the current importer",
                sanitize_comment_text(kind)
            ),
        },
        None => OpenApiSecuritySchemeMetadata::Unsupported {
            name,
            detail: "missing type".to_string(),
        },
    }
}

impl OpenApiSecuritySchemeMetadata {
    fn name(&self) -> &str {
        match self {
            Self::ApiKey { name, .. }
            | Self::Http { name, .. }
            | Self::OAuth2 { name, .. }
            | Self::Unsupported { name, .. } => name,
        }
    }
}

fn oauth2_flows_comment(flows: Option<&Value>) -> String {
    let Some(flows) = flows.and_then(Value::as_object) else {
        return "unknown".to_string();
    };
    let mut names = flows
        .keys()
        .map(|flow| sanitize_comment_text(flow))
        .collect::<Vec<_>>();
    names.sort();
    if names.is_empty() {
        "unknown".to_string()
    } else {
        names.join(", ")
    }
}

fn security_requirements_comment(security: &Value) -> Option<String> {
    let requirements = security.as_array()?;
    if requirements.is_empty() {
        return Some("none".to_string());
    }

    let mut alternatives = Vec::new();
    for requirement in requirements {
        let Some(requirement) = requirement.as_object() else {
            alternatives.push("unsupported requirement shape".to_string());
            continue;
        };
        if requirement.is_empty() {
            alternatives.push("anonymous".to_string());
            continue;
        }

        let mut schemes = requirement.iter().collect::<Vec<_>>();
        schemes.sort_by(|left, right| left.0.cmp(right.0));
        let schemes = schemes
            .into_iter()
            .map(|(scheme, scopes)| security_requirement_scheme_comment(scheme, scopes))
            .collect::<Vec<_>>();
        alternatives.push(schemes.join(" and "));
    }

    (!alternatives.is_empty()).then(|| alternatives.join(" or "))
}

fn operation_tags(operation: &Value) -> Vec<String> {
    let mut tags = operation
        .get("tags")
        .and_then(Value::as_array)
        .map(|tags| {
            tags.iter()
                .filter_map(Value::as_str)
                .map(to_identifier)
                .filter(|tag| !tag.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    tags.sort();
    tags.dedup();
    tags
}

fn operation_permission_scaffold(
    operation: &Value,
    method: &str,
    path: &str,
    operation_name: &str,
    tags: &[String],
) -> OpenApiPermissionScaffold {
    if operation
        .get("operationId")
        .and_then(Value::as_str)
        .is_some()
    {
        return OpenApiPermissionScaffold {
            name: to_permission_name(operation_name),
            source: "operationId".to_string(),
        };
    }

    if let Some(tag) = tags.first() {
        return OpenApiPermissionScaffold {
            name: to_permission_name(&format!("{tag}_{operation_name}")),
            source: "tag and method/path".to_string(),
        };
    }

    OpenApiPermissionScaffold {
        name: to_permission_name(&fallback_operation_name(method, path)),
        source: "method/path".to_string(),
    }
}

fn operation_policy_placeholders(
    operation: &Value,
    method: &str,
    path: &str,
    permission: &OpenApiPermissionScaffold,
    document: &Value,
    global_security: Option<&Value>,
) -> Vec<String> {
    let mut placeholders = Vec::new();
    let operation_name = operation
        .get("operationId")
        .and_then(Value::as_str)
        .map(to_identifier)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| fallback_operation_name(method, path));

    if let Some(requirements) = operation
        .get("security")
        .or(global_security)
        .and_then(security_requirements_comment)
    {
        placeholders.push(format!(
            "OpenAPI review-required policy placeholder for `{operation_name}`: security requirements {requirements} suggest checking permission `{}` before connector use",
            permission.name
        ));
    }

    let field_hints = sensitive_field_hints(operation, document);
    if !field_hints.is_empty() {
        placeholders.push(format!(
            "OpenAPI review-required policy placeholder for `{operation_name}`: fields [{}] look private/auth-related; review a narrow policy such as `allow private UserInput -> <connector>.{operation_name}` before sending user data",
            field_hints.join(", ")
        ));
    }

    if placeholders.is_empty() {
        placeholders.push(format!(
            "OpenAPI review-required policy placeholder for `{operation_name}`: no obvious private/auth fields were detected; still review data classification before allowing connector calls"
        ));
    }

    placeholders
}

impl OpenApiPermissionScaffold {
    fn comment(&self) -> String {
        format!(
            "OpenAPI review-required permission candidate `{}` inferred from {}; add it to roles/routes only after reviewing the imported operation",
            self.name, self.source
        )
    }
}

fn sensitive_field_hints(operation: &Value, document: &Value) -> Vec<String> {
    let mut fields = BTreeSet::new();
    collect_sensitive_schema_fields(
        operation.pointer("/requestBody/content/application~1json/schema"),
        "requestBody",
        document,
        &mut fields,
    );
    collect_sensitive_schema_fields(
        operation.pointer("/requestBody/content/application~1x-www-form-urlencoded/schema"),
        "requestBody",
        document,
        &mut fields,
    );
    if let Some(parameters) = operation.get("parameters").and_then(Value::as_array) {
        for parameter in parameters {
            if let Some(name) = parameter.get("name").and_then(Value::as_str) {
                collect_sensitive_name(name, "parameter", &mut fields);
            }
            collect_sensitive_schema_fields(
                parameter.get("schema"),
                "parameter",
                document,
                &mut fields,
            );
        }
    }
    if let Some(responses) = operation.get("responses").and_then(Value::as_object) {
        for (code, response) in responses {
            collect_sensitive_schema_fields(
                response.pointer("/content/application~1json/schema"),
                &format!("response {code}"),
                document,
                &mut fields,
            );
        }
    }
    fields.into_iter().collect()
}

fn collect_sensitive_schema_fields(
    schema: Option<&Value>,
    context: &str,
    document: &Value,
    fields: &mut BTreeSet<String>,
) {
    let Some(schema) = schema else {
        return;
    };
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        collect_sensitive_name(
            reference.rsplit('/').next().unwrap_or(reference),
            context,
            fields,
        );
        if let Some(resolved) = resolve_local_ref(document, reference) {
            collect_sensitive_schema_fields(Some(resolved), context, document, fields);
        }
    }
    if let Some(title) = schema.get("title").and_then(Value::as_str) {
        collect_sensitive_name(title, context, fields);
    }
    if let Some(format) = schema.get("format").and_then(Value::as_str) {
        collect_sensitive_name(format, context, fields);
    }
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        for (field, schema) in properties {
            collect_sensitive_name(field, context, fields);
            collect_sensitive_schema_fields(Some(schema), field, document, fields);
        }
    }
    if let Some(items) = schema.get("items") {
        collect_sensitive_schema_fields(Some(items), context, document, fields);
    }
    for key in ["allOf", "oneOf", "anyOf"] {
        if let Some(items) = schema.get(key).and_then(Value::as_array) {
            for item in items {
                collect_sensitive_schema_fields(Some(item), context, document, fields);
            }
        }
    }
}

fn resolve_local_ref<'a>(document: &'a Value, reference: &str) -> Option<&'a Value> {
    let pointer = reference.strip_prefix('#')?;
    document.pointer(pointer)
}

fn collect_sensitive_name(name: &str, context: &str, fields: &mut BTreeSet<String>) {
    if is_sensitive_name(name) {
        fields.insert(format!(
            "{}:{}",
            sanitize_comment_text(context),
            sanitize_comment_text(name)
        ));
    }
}

fn is_sensitive_name(name: &str) -> bool {
    let normalized = name.to_ascii_lowercase();
    [
        "auth",
        "token",
        "secret",
        "password",
        "credential",
        "session",
        "cookie",
        "email",
        "phone",
        "user",
        "customer",
        "tenant",
        "owner",
        "identity",
        "private",
        "ssn",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

fn security_requirement_scheme_comment(scheme: &str, scopes: &Value) -> String {
    let scheme = sanitize_comment_text(scheme);
    let Some(scopes) = scopes.as_array() else {
        return format!("`{scheme}` with unsupported scopes");
    };
    let mut scopes = scopes
        .iter()
        .filter_map(Value::as_str)
        .map(sanitize_comment_text)
        .collect::<Vec<_>>();
    scopes.sort();
    if scopes.is_empty() {
        format!("`{scheme}`")
    } else {
        format!("`{scheme}` scopes [{}]", scopes.join(", "))
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
    let ty = schema_type_inner(schema);
    if schema_is_nullable(schema) && !ty.starts_with("Option<") {
        format!("Option<{ty}>")
    } else {
        ty
    }
}

fn schema_type_inner(schema: &Value) -> String {
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        return reference
            .rsplit('/')
            .next()
            .map(to_type_name)
            .unwrap_or_else(|| "Json".to_string());
    }

    if let Some(types) = schema.get("type").and_then(Value::as_array) {
        if types.iter().any(|ty| ty.as_str() == Some("array")) {
            let inner = schema
                .get("items")
                .map(schema_type)
                .unwrap_or_else(|| "Json".to_string());
            return format!("List<{inner}>");
        }
        return types
            .iter()
            .filter_map(Value::as_str)
            .find(|ty| *ty != "null")
            .map(schema_scalar_type_name)
            .unwrap_or_else(|| "Json".to_string());
    }

    match schema.get("type").and_then(Value::as_str) {
        Some("array") => {
            let inner = schema
                .get("items")
                .map(schema_type)
                .unwrap_or_else(|| "Json".to_string());
            format!("List<{inner}>")
        }
        Some(name) => schema_scalar_type_name(name),
        None => "Json".to_string(),
    }
}

fn schema_scalar_type_name(name: &str) -> String {
    match name {
        "string" => "Text".to_string(),
        "integer" => "Int".to_string(),
        "number" => "Float".to_string(),
        "boolean" => "Bool".to_string(),
        "object" => "Json".to_string(),
        _ => "Json".to_string(),
    }
}

fn schema_is_nullable(schema: &Value) -> bool {
    schema.get("nullable").and_then(Value::as_bool) == Some(true)
        || schema
            .get("type")
            .and_then(Value::as_array)
            .is_some_and(|types| types.iter().any(|ty| ty.as_str() == Some("null")))
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

fn to_permission_name(value: &str) -> String {
    let mut name = to_type_name(&value.replace('_', " "));
    if name.is_empty() {
        name = "OpenApiOperation".to_string();
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
    fn merges_simple_allof_object_schemas() {
        let document: Value = serde_json::from_str(
            r##"
{
  "openapi": "3.0.0",
  "info": { "title": "Customer API", "version": "1.0.0" },
  "components": {
    "schemas": {
      "BaseCustomer": {
        "type": "object",
        "required": ["id"],
        "properties": {
          "id": { "type": "string" }
        }
      },
      "Customer": {
        "allOf": [
          { "$ref": "#/components/schemas/BaseCustomer" },
          {
            "type": "object",
            "required": ["email"],
            "properties": {
              "email": { "type": "string" },
              "nickname": { "type": "string", "nullable": true }
            }
          }
        ]
      }
    }
  },
  "paths": {}
}
"##,
        )
        .unwrap();

        let rendered = render_openapi_connector(&document, Some("generated.customer"));

        assert!(rendered.contains(
            "type Customer {\n    email: Text\n    id: Text\n    nickname: Option<Text>\n}"
        ));
        assert!(num_compiler::check("generated.num", &rendered).is_empty());
    }

    #[test]
    fn preserves_allof_conflicts_as_review_comments() {
        let document: Value = serde_json::from_str(
            r##"
{
  "openapi": "3.0.0",
  "info": { "title": "Customer API", "version": "1.0.0" },
  "components": {
    "schemas": {
      "Customer": {
        "allOf": [
          {
            "type": "object",
            "properties": {
              "id": { "type": "string" }
            }
          },
          {
            "type": "object",
            "properties": {
              "id": { "type": "integer" },
              "email": { "type": "string" }
            }
          }
        ]
      }
    }
  },
  "paths": {}
}
"##,
        )
        .unwrap();

        let rendered = render_openapi_connector(&document, Some("generated.customer"));

        assert!(rendered
            .contains("// OpenAPI allOf conflict on field `id`; generated as Json for review."));
        assert!(rendered.contains("type Customer {\n    // OpenAPI allOf conflict on field `id`; generated as Json for review.\n    email: Text\n    id: Json\n}"));
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

    #[test]
    fn preserves_global_openapi_security_metadata_comments() {
        let document: Value = serde_json::from_str(
            r##"
{
  "openapi": "3.0.0",
  "info": { "title": "Billing API", "version": "1.0.0" },
  "components": {
    "securitySchemes": {
      "apiKeyAuth": {
        "type": "apiKey",
        "in": "header",
        "name": "X-API-Key"
      },
      "bearerAuth": {
        "type": "http",
        "scheme": "bearer"
      }
    }
  },
  "security": [
    { "apiKeyAuth": [] },
    { "bearerAuth": [] }
  ],
  "paths": {
    "/refunds": {
      "get": {
        "operationId": "list_refunds",
        "responses": {
          "200": {
            "content": {
              "application/json": {
                "schema": {
                  "type": "array",
                  "items": { "type": "string" }
                }
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
            "// OpenAPI security scheme `apiKeyAuth` uses apiKey in `header` parameter `X-API-Key`; authentication binding is not generated yet"
        ));
        assert!(rendered.contains(
            "// OpenAPI security scheme `bearerAuth` uses HTTP `bearer` authentication; authentication binding is not generated yet"
        ));
        assert!(rendered.contains(
            "// OpenAPI security requirement for operation `list_refunds`: `apiKeyAuth` or `bearerAuth`; authentication binding is not generated yet"
        ));
        assert!(rendered.contains("list_refunds() -> List<Text>"));
        assert!(num_compiler::check("generated.num", &rendered).is_empty());
    }

    #[test]
    fn preserves_operation_openapi_security_metadata_comments() {
        let document: Value = serde_json::from_str(
            r##"
{
  "openapi": "3.0.0",
  "info": { "title": "Billing API", "version": "1.0.0" },
  "components": {
    "securitySchemes": {
      "oauth": {
        "type": "oauth2",
        "flows": {
          "clientCredentials": {
            "tokenUrl": "https://auth.example.test/token",
            "scopes": {
              "refunds:read": "Read refunds",
              "refunds:write": "Write refunds"
            }
          }
        }
      },
      "mutualTls": {
        "type": "mutualTLS"
      }
    }
  },
  "paths": {
    "/refunds": {
      "post": {
        "operationId": "create_refund",
        "security": [
          { "oauth": ["refunds:write", "refunds:read"] }
        ],
        "responses": {
          "201": {
            "description": "created"
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
            "// OpenAPI security scheme `oauth` uses OAuth2 flows `clientCredentials`; OAuth runtime generation is not implemented yet"
        ));
        assert!(rendered.contains(
            "// OpenAPI security scheme `mutualTls` is preserved as unsupported metadata (type `mutualTLS` is not represented by the current importer); authentication binding is not generated yet"
        ));
        assert!(rendered.contains(
            "// OpenAPI security requirement for operation `create_refund`: `oauth` scopes [refunds:read, refunds:write]; authentication binding is not generated yet"
        ));
        assert!(rendered.contains("create_refund() -> Unit"));
        assert!(num_compiler::check("generated.num", &rendered).is_empty());
    }

    #[test]
    fn renders_review_required_permission_and_policy_scaffolding() {
        let document: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/openapi/customer_portal.json"
        ))
        .unwrap();
        let expected = include_str!("../tests/fixtures/openapi/customer_portal.expected.num");

        let rendered = render_openapi_connector(&document, Some("generated.customer"));

        assert_eq!(rendered, expected);
        assert!(rendered.contains("permission UpdateCustomer"));
        assert!(rendered.contains(
            "// OpenAPI review-required permission candidate `UpdateCustomer` inferred from operationId; add it to roles/routes only after reviewing the imported operation"
        ));
        assert!(rendered.contains(
            "// OpenAPI review-required policy scaffold for connector `customerPortal`:"
        ));
        assert!(rendered.contains(
            "// OpenAPI review-required policy placeholder for `update_customer`: security requirements `bearerAuth` suggest checking permission `UpdateCustomer` before connector use"
        ));
        assert!(rendered.contains("fields ["));
        assert!(rendered.contains("parameter:customer_id"));
        assert!(rendered.contains("requestBody:UpdateCustomerRequest"));
        assert!(rendered.contains("requestBody:customer_email"));
        assert!(rendered.contains(
            "update_customer(customer_id: Text, body: UpdateCustomerRequest) -> CustomerProfile"
        ));
        assert!(num_compiler::check("generated.num", &rendered).is_empty());
    }

    #[test]
    fn renders_review_required_offset_pagination_metadata() {
        let document: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/openapi/offset_pagination.json"
        ))
        .unwrap();
        let expected = include_str!("../tests/fixtures/openapi/offset_pagination.expected.num");

        let rendered = render_openapi_connector(&document, Some("generated.catalog"));

        assert_eq!(rendered, expected);
        assert!(
            rendered.contains("limit/offset pagination hints [parameter:limit, parameter:offset]")
        );
        assert!(rendered.contains("generated connector is not an executable paginated client yet"));
        assert!(num_compiler::check("generated.num", &rendered).is_empty());
    }

    #[test]
    fn renders_review_required_cursor_pagination_metadata() {
        let document: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/openapi/cursor_pagination.json"
        ))
        .unwrap();
        let expected = include_str!("../tests/fixtures/openapi/cursor_pagination.expected.num");

        let rendered = render_openapi_connector(&document, Some("generated.catalog"));

        assert_eq!(rendered, expected);
        assert!(rendered.contains(
            "cursor/next-link pagination hints [parameter:cursor, response 200:next_cursor]"
        ));
        assert!(rendered.contains("generated connector is not an executable paginated client yet"));
        assert!(num_compiler::check("generated.num", &rendered).is_empty());
    }
}
