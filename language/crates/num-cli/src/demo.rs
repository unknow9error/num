use num_compiler::ast::{Declaration, Module};
use num_runtime::interpreter::Value;
use std::collections::HashMap;

pub fn default_permissions() -> Vec<String> {
    [
        "ViewBilling",
        "IssueRefund",
        "NotifyCustomer",
        "ExportData",
        "AssignTicket",
        "SendReply",
        "Execute",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

pub fn first_workflow_name(module: &Module) -> Option<String> {
    module
        .declarations
        .iter()
        .find(|decl| matches!(decl, Declaration::Workflow(_)))
        .map(|decl| decl.name().to_string())
}

pub fn first_service_name(module: &Module) -> Option<String> {
    module
        .declarations
        .iter()
        .find(|decl| matches!(decl, Declaration::Service(_)))
        .map(|decl| decl.name().to_string())
}

pub fn workflow_args(workflow_name: &str) -> HashMap<String, Value> {
    let mut args = HashMap::new();
    match workflow_name {
        "process_refund" => {
            args.insert("request".to_string(), value_for_type("RefundRequest"));
        }
        "handle_ticket" => {
            args.insert("ticket".to_string(), value_for_type("Ticket"));
        }
        "export_public_report" => {
            args.insert("request".to_string(), value_for_type("ExportRequest"));
        }
        "convert_invoice" => {
            args.insert("amount".to_string(), Value::Money(10000, "USD".to_string()));
        }
        _ => {}
    }
    args
}

pub fn route_input(module: &Module, service_name: &str, method: &str, path: &str) -> Option<Value> {
    module.declarations.iter().find_map(|decl| match decl {
        Declaration::Service(service) if service.name == service_name => service
            .routes
            .iter()
            .find(|route| route.method.eq_ignore_ascii_case(method) && route.path == path)
            .and_then(|route| route.input.as_ref())
            .map(|input| value_for_type(&input.ty.raw)),
        _ => None,
    })
}

pub fn value_for_type(type_name: &str) -> Value {
    match type_name {
        "RefundRequest" => struct_value(
            "RefundRequest",
            [
                ("payment_id", Value::String("pay_827361".to_string())),
                (
                    "reason",
                    Value::String("Item damaged in transit".to_string()),
                ),
                ("amount", Value::Money(15000, "KZT".to_string())),
            ],
        ),
        "Ticket" => struct_value(
            "Ticket",
            [
                ("id", Value::String("ticket_901".to_string())),
                (
                    "message",
                    Value::String("I would like to request a refund for my order".to_string()),
                ),
                ("email", Value::String("user@example.com".to_string())),
            ],
        ),
        "ExportRequest" => struct_value(
            "ExportRequest",
            [
                (
                    "customer_email",
                    Value::String("auditor@external.com".to_string()),
                ),
                ("report_id", Value::String("rep_2026".to_string())),
            ],
        ),
        other => Value::Struct(other.to_string(), HashMap::new()),
    }
}

fn struct_value<const N: usize>(name: &str, fields: [(&str, Value); N]) -> Value {
    Value::Struct(
        name.to_string(),
        fields
            .into_iter()
            .map(|(field, value)| (field.to_string(), value))
            .collect(),
    )
}
