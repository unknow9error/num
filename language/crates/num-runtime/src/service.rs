use crate::http::{HttpRequest, HttpResponse};
use crate::interpreter::{Runtime, Value};
use crate::json;
use crate::{RuntimeError, SecurityContext};
use crate::{connectors::ConnectorExecutor, connectors::DemoConnectorExecutor};
use num_compiler::ast::{Declaration, Module};
use std::collections::BTreeSet;
use std::sync::Arc;

type ServiceAuditRecorder<'a> =
    Arc<dyn Fn(&SecurityContext, &str, &str, &[String]) -> Result<(), RuntimeError> + 'a>;

pub struct ServiceRuntime<'a> {
    module: &'a Module,
    service_name: String,
    permissions: Vec<String>,
    connectors: Arc<dyn ConnectorExecutor + 'a>,
    audit_recorder: Option<ServiceAuditRecorder<'a>>,
}

impl<'a> ServiceRuntime<'a> {
    pub fn new(
        module: &'a Module,
        service_name: impl Into<String>,
        permissions: Vec<String>,
    ) -> Self {
        Self {
            module,
            service_name: service_name.into(),
            permissions,
            connectors: Arc::new(DemoConnectorExecutor::new()),
            audit_recorder: None,
        }
    }

    pub fn with_connectors(
        module: &'a Module,
        service_name: impl Into<String>,
        permissions: Vec<String>,
        connectors: Arc<dyn ConnectorExecutor + 'a>,
    ) -> Self {
        Self {
            module,
            service_name: service_name.into(),
            permissions,
            connectors,
            audit_recorder: None,
        }
    }

    pub fn with_audit_recorder(
        mut self,
        recorder: impl Fn(&SecurityContext, &str, &str, &[String]) -> Result<(), RuntimeError> + 'a,
    ) -> Self {
        self.audit_recorder = Some(Arc::new(recorder));
        self
    }

    pub fn first_service_name(module: &Module) -> Option<String> {
        module
            .declarations
            .iter()
            .find(|decl| matches!(decl, Declaration::Service(_)))
            .map(|decl| decl.name().to_string())
    }

    pub fn handle_http_request(&self, request: &HttpRequest) -> HttpResponse {
        self.handle_http_request_with_empty_body_input(request, None)
    }

    pub fn security_context(&self, request: &HttpRequest) -> SecurityContext {
        let mut permissions = self.permissions.iter().cloned().collect::<BTreeSet<_>>();
        for role in request_roles(request) {
            permissions.extend(self.permissions_for_role(role));
        }

        SecurityContext {
            actor: request.header("x-actor").unwrap_or("anonymous").to_string(),
            tenant: request.header("x-tenant").unwrap_or("default").to_string(),
            permissions,
            correlation_id: request
                .header("x-correlation-id")
                .or_else(|| request.header("x-request-id"))
                .unwrap_or("corr_demo")
                .to_string(),
            request_id: request
                .header("x-request-id")
                .unwrap_or("req_demo")
                .to_string(),
        }
    }

    fn permissions_for_role(&self, role_name: &str) -> Vec<String> {
        self.module
            .declarations
            .iter()
            .find_map(|decl| match decl {
                Declaration::Role(role) if role.name == role_name => Some(role.allows.clone()),
                _ => None,
            })
            .unwrap_or_default()
    }

    pub fn handle_http_request_with_empty_body_input(
        &self,
        request: &HttpRequest,
        empty_body_input: Option<Value>,
    ) -> HttpResponse {
        let security = self.security_context(request);
        let input = if request.body.trim().is_empty() {
            empty_body_input
        } else {
            match json::route_input_from_body(
                self.module,
                &self.service_name,
                &request.method,
                &request.path,
                &request.body,
            ) {
                Ok(input) => input,
                Err(message) => {
                    return HttpResponse::text(400, "Bad Request", format!("{message}\n"));
                }
            }
        };

        let mut runtime = Runtime::with_connectors_and_security(
            self.module,
            security.clone(),
            Box::new(self.connectors.clone()),
        );
        let result =
            runtime.run_service_route(&self.service_name, &request.method, &request.path, input);
        if let Err(err) =
            self.record_audit_events(&security, &request.method, &request.path, runtime.audit_events())
        {
            return HttpResponse::text(
                500,
                "Internal Server Error",
                format!("failed to persist audit events: {err:?}\n"),
            );
        }

        match result {
            Ok(()) => HttpResponse::text(200, "OK", "ok\n"),
            Err(message) if message.contains("not found") => {
                HttpResponse::text(404, "Not Found", format!("{message}\n"))
            }
            Err(message) if message.contains("Security Violation") => {
                HttpResponse::text(403, "Forbidden", format!("{message}\n"))
            }
            Err(message) if message.contains("Missing route input") => {
                HttpResponse::text(400, "Bad Request", format!("{message}\n"))
            }
            Err(message) => {
                HttpResponse::text(500, "Internal Server Error", format!("{message}\n"))
            }
        }
    }

    fn record_audit_events(
        &self,
        security: &SecurityContext,
        method: &str,
        path: &str,
        events: &[String],
    ) -> Result<(), RuntimeError> {
        let Some(recorder) = &self.audit_recorder else {
            return Ok(());
        };
        recorder(security, method, path, events)
    }
}

fn request_roles(request: &HttpRequest) -> impl Iterator<Item = &str> {
    request
        .header("x-roles")
        .into_iter()
        .chain(request.header("x-role"))
        .flat_map(|roles| roles.split(','))
        .map(str::trim)
        .filter(|role| !role.is_empty())
}

#[cfg(test)]
mod tests {
    use super::ServiceRuntime;
    use crate::http::HttpRequest;
    use crate::RuntimeError;
    use num_compiler::compile;
    use std::sync::{Arc, Mutex};

    #[test]
    fn maps_http_request_to_service_route() {
        let source = r#"
module test.api

permission IssueRefund

type RefundRequest {
    id: Text
}

service BillingApi {
    route POST "/refunds" requires Permission.IssueRefund {
        input request: RefundRequest from HttpBody
        audit(request.id)
    }
}
"#;
        let compilation = compile("test.num", source);
        let runtime = ServiceRuntime::new(
            &compilation.module,
            "BillingApi",
            vec!["IssueRefund".to_string()],
        );
        let response = runtime.handle_http_request(&HttpRequest::new(
            "POST",
            "/refunds",
            r#"{"id":"refund_1"}"#,
        ));

        assert_eq!(response.status, 200);
        assert_eq!(response.body, "ok\n");
    }

    #[test]
    fn maps_missing_input_to_bad_request() {
        let source = r#"
module test.api

type RefundRequest {
    id: Text
}

service BillingApi {
    route POST "/refunds" {
        input request: RefundRequest from HttpBody
        audit("refund")
    }
}
"#;
        let compilation = compile("test.num", source);
        let runtime = ServiceRuntime::new(&compilation.module, "BillingApi", vec![]);
        let response = runtime.handle_http_request(&HttpRequest::new("POST", "/refunds", ""));

        assert_eq!(response.status, 400);
        assert!(response.body.contains("Missing route input"));
    }

    #[test]
    fn builds_security_context_from_http_headers() {
        let source = r#"
module test.api

permission IssueRefund

service BillingApi {
    route POST "/refunds" requires Permission.IssueRefund {
        audit("refund")
    }
}
"#;
        let compilation = compile("test.num", source);
        let runtime = ServiceRuntime::new(
            &compilation.module,
            "BillingApi",
            vec!["IssueRefund".to_string()],
        );
        let mut request = HttpRequest::new("POST", "/refunds", "");
        request
            .headers
            .insert("x-actor".to_string(), "agent@example.com".to_string());
        request
            .headers
            .insert("x-tenant".to_string(), "tenant_a".to_string());
        request
            .headers
            .insert("x-request-id".to_string(), "req_42".to_string());

        let context = runtime.security_context(&request);

        assert_eq!(context.actor, "agent@example.com");
        assert_eq!(context.tenant, "tenant_a");
        assert_eq!(context.request_id, "req_42");
        assert!(context.permissions.contains("IssueRefund"));
    }

    #[test]
    fn grants_route_permissions_from_request_role_header() {
        let source = r#"
module test.api

permission IssueRefund

role FinanceManager {
    allow IssueRefund
}

service BillingApi {
    route POST "/refunds" requires Permission.IssueRefund {
        audit(current_user)
    }
}
"#;
        let compilation = compile("test.num", source);
        let runtime = ServiceRuntime::new(&compilation.module, "BillingApi", vec![]);
        let mut request = HttpRequest::new("POST", "/refunds", "");
        request
            .headers
            .insert("x-actor".to_string(), "finance@example.com".to_string());
        request
            .headers
            .insert("x-role".to_string(), "FinanceManager".to_string());

        let response = runtime.handle_http_request(&request);

        assert_eq!(response.status, 200);
        assert_eq!(response.body, "ok\n");
    }

    #[test]
    fn role_header_does_not_grant_unknown_roles() {
        let source = r#"
module test.api

permission IssueRefund

role FinanceManager {
    allow IssueRefund
}

service BillingApi {
    route POST "/refunds" requires Permission.IssueRefund {
        audit("refund")
    }
}
"#;
        let compilation = compile("test.num", source);
        let runtime = ServiceRuntime::new(&compilation.module, "BillingApi", vec![]);
        let mut request = HttpRequest::new("POST", "/refunds", "");
        request
            .headers
            .insert("x-role".to_string(), "SupportAgent".to_string());

        let response = runtime.handle_http_request(&request);

        assert_eq!(response.status, 403);
        assert!(response.body.contains("IssueRefund"));
    }

    #[test]
    fn security_context_unions_static_and_role_permissions() {
        let source = r#"
module test.api

permission ViewBilling
permission IssueRefund
permission ExportData

role FinanceManager {
    allow ViewBilling
    allow IssueRefund
}

role Auditor {
    allow ExportData
}
"#;
        let compilation = compile("test.num", source);
        let runtime = ServiceRuntime::new(
            &compilation.module,
            "BillingApi",
            vec!["ViewBilling".to_string()],
        );
        let mut request = HttpRequest::new("POST", "/refunds", "");
        request
            .headers
            .insert("x-roles".to_string(), "FinanceManager, Auditor".to_string());

        let context = runtime.security_context(&request);

        assert!(context.permissions.contains("ViewBilling"));
        assert!(context.permissions.contains("IssueRefund"));
        assert!(context.permissions.contains("ExportData"));
    }

    #[test]
    fn records_service_audit_events_with_request_security() {
        let source = r#"
module test.api

type RefundRequest {
    id: Text
}

service BillingApi {
    route POST "/refunds" {
        input request: RefundRequest from HttpBody
        audit(request.id)
    }
}
"#;
        let compilation = compile("test.num", source);
        let captured = Arc::new(Mutex::new(Vec::new()));
        let captured_events = Arc::clone(&captured);
        let runtime = ServiceRuntime::new(&compilation.module, "BillingApi", vec![])
            .with_audit_recorder(move |security, method, path, events| {
                captured_events.lock().unwrap().push((
                    security.actor.clone(),
                    security.tenant.clone(),
                    method.to_string(),
                    path.to_string(),
                    events.to_vec(),
                ));
                Ok(())
            });
        let mut request = HttpRequest::new("POST", "/refunds", r#"{"id":"refund_1"}"#);
        request
            .headers
            .insert("x-actor".to_string(), "agent@example.com".to_string());
        request
            .headers
            .insert("x-tenant".to_string(), "tenant_a".to_string());

        let response = runtime.handle_http_request(&request);

        assert_eq!(response.status, 200);
        let records = captured.lock().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].0, "agent@example.com");
        assert_eq!(records[0].1, "tenant_a");
        assert_eq!(records[0].2, "POST");
        assert_eq!(records[0].3, "/refunds");
        assert_eq!(records[0].4, vec!["\"refund_1\"".to_string()]);
    }

    #[test]
    fn audit_recorder_failure_returns_internal_server_error() {
        let source = r#"
module test.api

service BillingApi {
    route POST "/refunds" {
        audit("refund")
    }
}
"#;
        let compilation = compile("test.num", source);
        let runtime = ServiceRuntime::new(&compilation.module, "BillingApi", vec![])
            .with_audit_recorder(|_, _, _, _| {
                Err(RuntimeError::Storage("audit disk is unavailable".to_string()))
            });

        let response = runtime.handle_http_request(&HttpRequest::new("POST", "/refunds", ""));

        assert_eq!(response.status, 500);
        assert!(response.body.contains("failed to persist audit events"));
    }
}
