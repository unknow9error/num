use crate::interpreter::Value;
use num_compiler::ast::{Declaration, Module, TypeBody, TypeDecl, TypeRef};
use serde_json::Value as JsonValue;
use std::collections::HashMap;

pub fn route_input_from_body(
    module: &Module,
    service_name: &str,
    method: &str,
    path: &str,
    body: &str,
) -> Result<Option<Value>, String> {
    let Some(type_name) = route_input_type(module, service_name, method, path) else {
        return Ok(None);
    };
    let json: JsonValue = serde_json::from_str(body)
        .map_err(|err| format!("invalid JSON request body for {method} {path}: {err}"))?;
    value_from_json(module, &TypeRef { raw: type_name }, &json).map(Some)
}

pub fn value_from_json(module: &Module, ty: &TypeRef, json: &JsonValue) -> Result<Value, String> {
    let raw = ty.raw.trim();
    match raw {
        "Text" | "String" | "Email" | "Url" | "Uuid" | "PhoneNumber" | "Bytes" | "DateTime" => json
            .as_str()
            .map(|value| Value::String(value.to_string()))
            .ok_or_else(|| format!("expected string for {raw}")),
        "Bool" | "Boolean" => json
            .as_bool()
            .map(Value::Bool)
            .ok_or_else(|| format!("expected boolean for {raw}")),
        "Int" | "Integer" => json
            .as_i64()
            .map(Value::Int)
            .ok_or_else(|| format!("expected integer for {raw}")),
        "Float" | "Number" => json
            .as_f64()
            .map(Value::Float)
            .ok_or_else(|| format!("expected number for {raw}")),
        "Decimal" => json
            .as_str()
            .ok_or_else(|| "expected string for Decimal".to_string())
            .and_then(|value| crate::decimal::Decimal::parse(value).map(Value::Decimal)),
        _ if raw.starts_with("Money<") => money_from_json(raw, json),
        _ if raw.starts_with("Brand<") => brand_from_json(module, raw, json),
        _ if raw.starts_with("Secret<") => secret_from_json(module, raw, json),
        _ if raw.starts_with("Distance<")
            || raw.starts_with("Duration<")
            || raw.starts_with("Speed<") =>
        {
            quantity_from_json(raw, json)
        }
        _ if raw.starts_with("Option<") => {
            if json.is_null() {
                Ok(Value::Null)
            } else {
                let inner = single_generic_arg(raw, "Option")?;
                value_from_json(module, &TypeRef { raw: inner }, json)
            }
        }
        _ => declared_value_from_json(module, raw, json),
    }
}

fn secret_from_json(module: &Module, raw: &str, json: &JsonValue) -> Result<Value, String> {
    let inner = raw
        .strip_prefix("Secret<")
        .and_then(|value| value.strip_suffix('>'))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("invalid secret type '{raw}'"))?
        .to_string();
    value_from_json(module, &TypeRef { raw: inner }, json)
        .map(|value| Value::Secret(Box::new(value)))
}

fn route_input_type(
    module: &Module,
    service_name: &str,
    method: &str,
    path: &str,
) -> Option<String> {
    module.declarations.iter().find_map(|decl| match decl {
        Declaration::Service(service) if service.name == service_name => service
            .routes
            .iter()
            .find(|route| route.method.eq_ignore_ascii_case(method) && route.path == path)
            .and_then(|route| route.input.as_ref())
            .map(|input| input.ty.raw.clone()),
        _ => None,
    })
}

fn declared_value_from_json(
    module: &Module,
    type_name: &str,
    json: &JsonValue,
) -> Result<Value, String> {
    let Some(type_decl) = find_type(module, type_name) else {
        return Err(format!("cannot decode JSON for unknown type '{type_name}'"));
    };

    match &type_decl.body {
        TypeBody::Struct(fields) => {
            let object = json
                .as_object()
                .ok_or_else(|| format!("expected object for {type_name}"))?;
            let mut values = HashMap::new();
            for field in fields {
                let field_json = object.get(&field.name).ok_or_else(|| {
                    format!("missing JSON field '{}' for {type_name}", field.name)
                })?;
                let field_value =
                    value_from_json(module, &field.ty, field_json).map_err(|err| {
                        format!(
                            "failed to decode field '{}.{}': {err}",
                            type_name, field.name
                        )
                    })?;
                values.insert(field.name.clone(), field_value);
            }
            Ok(Value::Struct(type_name.to_string(), values))
        }
        TypeBody::Alias(alias) => value_from_json(module, alias, json),
    }
}

fn find_type<'a>(module: &'a Module, name: &str) -> Option<&'a TypeDecl> {
    module.declarations.iter().find_map(|decl| match decl {
        Declaration::Type(type_decl) if type_decl.name == name => Some(type_decl),
        _ => None,
    })
}

fn brand_from_json(module: &Module, raw: &str, json: &JsonValue) -> Result<Value, String> {
    let inner = raw
        .strip_prefix("Brand<")
        .and_then(|value| value.strip_suffix('>'))
        .ok_or_else(|| format!("invalid brand type '{raw}'"))?;
    let mut parts = split_top_level(inner, ',');
    let base = parts
        .next()
        .ok_or_else(|| format!("missing brand base type in '{raw}'"))?
        .trim()
        .to_string();
    value_from_json(module, &TypeRef { raw: base }, json)
}

fn money_from_json(raw: &str, json: &JsonValue) -> Result<Value, String> {
    let currency = raw
        .strip_prefix("Money<")
        .and_then(|value| value.strip_suffix('>'))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("UNKNOWN")
        .to_string();

    if let Some(minor_units) = json.as_i64() {
        return Ok(Value::Money(i128::from(minor_units), currency));
    }

    let object = json
        .as_object()
        .ok_or_else(|| format!("expected integer minor units or object for {raw}"))?;
    let minor_units = object
        .get("minor_units")
        .or_else(|| object.get("amount_minor"))
        .and_then(JsonValue::as_i64)
        .ok_or_else(|| format!("expected 'minor_units' integer for {raw}"))?;
    let body_currency = object
        .get("currency")
        .and_then(JsonValue::as_str)
        .unwrap_or(&currency);
    if body_currency != currency {
        return Err(format!(
            "currency mismatch for {raw}: expected {currency}, got {body_currency}"
        ));
    }
    Ok(Value::Money(i128::from(minor_units), currency))
}

fn quantity_from_json(raw: &str, json: &JsonValue) -> Result<Value, String> {
    let unit = raw
        .split_once('<')
        .and_then(|(_, rest)| rest.strip_suffix('>'))
        .map(str::trim)
        .ok_or_else(|| format!("Invalid quantity type: {raw}"))?;

    if let Some(num) = json.as_f64().or_else(|| json.as_i64().map(|i| i as f64)) {
        return Ok(Value::Quantity(num, unit.to_string()));
    }

    let object = json
        .as_object()
        .ok_or_else(|| format!("expected number or object for quantity type {raw}"))?;
    let amount = object
        .get("$quantity")
        .or_else(|| object.get("amount"))
        .and_then(|v| v.as_f64().or_else(|| v.as_i64().map(|i| i as f64)))
        .ok_or_else(|| format!("expected numeric amount in quantity type {raw}"))?;
    let unit_override = object
        .get("unit")
        .and_then(JsonValue::as_str)
        .unwrap_or(unit);
    Ok(Value::Quantity(amount, unit_override.to_string()))
}

fn single_generic_arg(raw: &str, wrapper: &str) -> Result<String, String> {
    raw.strip_prefix(&format!("{wrapper}<"))
        .and_then(|value| value.strip_suffix('>'))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("invalid {wrapper} type '{raw}'"))
}

fn split_top_level(input: &str, delimiter: char) -> impl Iterator<Item = &str> {
    let mut depth = 0_i32;
    let mut start = 0_usize;
    let mut parts = Vec::new();
    for (idx, ch) in input.char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => depth -= 1,
            _ if ch == delimiter && depth == 0 => {
                parts.push(&input[start..idx]);
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(&input[start..]);
    parts.into_iter()
}

#[cfg(test)]
mod tests {
    use super::route_input_from_body;
    use crate::interpreter::Value;
    use num_compiler::compile;

    #[test]
    fn decodes_route_input_from_json_body() {
        let source = r#"
module test.api

permission IssueRefund

type PaymentId = Brand<Text, "PaymentId">

type RefundRequest {
    payment_id: PaymentId
    reason: Text
    amount: Money<KZT>
}

service BillingApi {
    route POST "/refunds" requires Permission.IssueRefund {
        input request: RefundRequest from HttpBody private
        audit("refund")
    }
}
"#;
        let compilation = compile("test.num", source);
        let value = route_input_from_body(
            &compilation.module,
            "BillingApi",
            "POST",
            "/refunds",
            r#"{"payment_id":"pay_42","reason":"duplicate","amount":{"minor_units":15000,"currency":"KZT"}}"#,
        )
        .unwrap()
        .unwrap();

        let Value::Struct(name, fields) = value else {
            panic!("expected struct value");
        };
        assert_eq!(name, "RefundRequest");
        assert_eq!(
            fields.get("payment_id"),
            Some(&Value::String("pay_42".to_string()))
        );
        assert_eq!(
            fields.get("amount"),
            Some(&Value::Money(15000, "KZT".to_string()))
        );
    }

    #[test]
    fn rejects_currency_mismatch() {
        let source = r#"
module test.api

type RefundRequest {
    amount: Money<KZT>
}

service BillingApi {
    route POST "/refunds" {
        input request: RefundRequest from HttpBody
    }
}
"#;
        let compilation = compile("test.num", source);
        let err = route_input_from_body(
            &compilation.module,
            "BillingApi",
            "POST",
            "/refunds",
            r#"{"amount":{"minor_units":15000,"currency":"USD"}}"#,
        )
        .unwrap_err();

        assert!(err.contains("currency mismatch"));
    }
}
