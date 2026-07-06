use crate::http::{HttpRequest, HttpResponse};
use crate::interpreter::{Runtime, Value};
use crate::json;
use crate::jwt::{bearer_token, JwtVerifier};
use crate::rate_limit::RateLimiter;
use crate::redaction;
use crate::sanitization::TextSanitizationPolicy;
use crate::session::SessionVerifier;
use crate::tenant::TenantGuard;
use crate::{connectors::ConnectorExecutor, connectors::DemoConnectorExecutor};
use crate::{RuntimeError, SecurityContext, TenantId};
use num_compiler::ast::{Declaration, Module};
use serde_json::{json, Value as JsonValue};
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

type ServiceAuditRecorder<'a> =
    Arc<dyn Fn(&SecurityContext, &str, &str, &[String]) -> Result<(), RuntimeError> + 'a>;

pub struct ServiceRuntime<'a> {
    module: &'a Module,
    service_name: String,
    permissions: Vec<String>,
    connectors: Arc<dyn ConnectorExecutor + 'a>,
    audit_recorder: Option<ServiceAuditRecorder<'a>>,
    output_enabled: bool,
    tenant_guard: TenantGuard,
    service_tenant: TenantId,
    rate_limiter: RateLimiter,
    sanitizer_packs: HashMap<String, TextSanitizationPolicy>,
    jwt_verifier: Option<JwtVerifier>,
    session_verifier: Option<SessionVerifier>,
    jwt_clock: Arc<dyn Fn() -> i64 + 'a>,
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
            output_enabled: true,
            tenant_guard: TenantGuard::disabled(),
            service_tenant: "default".to_string(),
            rate_limiter: RateLimiter::new(),
            sanitizer_packs: HashMap::new(),
            jwt_verifier: None,
            session_verifier: None,
            jwt_clock: Arc::new(current_epoch_seconds),
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
            output_enabled: true,
            tenant_guard: TenantGuard::disabled(),
            service_tenant: "default".to_string(),
            rate_limiter: RateLimiter::new(),
            sanitizer_packs: HashMap::new(),
            jwt_verifier: None,
            session_verifier: None,
            jwt_clock: Arc::new(current_epoch_seconds),
        }
    }

    pub fn with_audit_recorder(
        mut self,
        recorder: impl Fn(&SecurityContext, &str, &str, &[String]) -> Result<(), RuntimeError> + 'a,
    ) -> Self {
        self.audit_recorder = Some(Arc::new(recorder));
        self
    }

    pub fn with_output_enabled(mut self, enabled: bool) -> Self {
        self.output_enabled = enabled;
        self
    }

    pub fn with_tenant_isolation(mut self, enabled: bool) -> Self {
        self.tenant_guard = if enabled {
            TenantGuard::strict()
        } else {
            TenantGuard::disabled()
        };
        self
    }

    pub fn with_service_tenant(mut self, tenant: impl Into<TenantId>) -> Self {
        self.service_tenant = tenant.into();
        self
    }

    pub fn with_rate_limiter(mut self, rate_limiter: RateLimiter) -> Self {
        self.rate_limiter = rate_limiter;
        self
    }

    pub fn with_sanitizer_packs(
        mut self,
        packs: impl IntoIterator<Item = (String, TextSanitizationPolicy)>,
    ) -> Self {
        self.sanitizer_packs.extend(packs);
        self
    }

    pub fn with_jwt_verifier(mut self, verifier: JwtVerifier) -> Self {
        self.jwt_verifier = Some(verifier);
        self
    }

    pub fn with_session_verifier(mut self, verifier: SessionVerifier) -> Self {
        self.session_verifier = Some(verifier);
        self
    }

    pub fn with_jwt_clock(mut self, clock: impl Fn() -> i64 + 'a) -> Self {
        self.jwt_clock = Arc::new(clock);
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
        self.security_context_result(request)
            .unwrap_or_else(|_| self.header_security_context(request))
    }

    fn security_context_result(
        &self,
        request: &HttpRequest,
    ) -> Result<SecurityContext, RuntimeError> {
        if let Some(verifier) = &self.jwt_verifier {
            let token = bearer_token(request.header("authorization")).map_err(jwt_runtime_error)?;
            let claims = verifier
                .verify(token, (self.jwt_clock)())
                .map_err(jwt_runtime_error)?;
            let roles = claims.roles.iter().cloned().collect::<BTreeSet<_>>();
            let mut permissions = self.permissions.iter().cloned().collect::<BTreeSet<_>>();
            for role in &roles {
                permissions.extend(self.permissions_for_role(role));
            }

            return Ok(SecurityContext {
                actor: claims.subject,
                tenant: claims
                    .tenant
                    .or_else(|| request.header("x-tenant").map(ToString::to_string))
                    .unwrap_or_else(|| "default".to_string()),
                roles,
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
                provenance: Some(claims.provenance),
                trust: Some(claims.trust),
            });
        }

        if let Some(verifier) = &self.session_verifier {
            let session = verifier
                .verify_cookie_header(request.header("cookie"), (self.jwt_clock)())
                .map_err(session_runtime_error)?;
            let roles = session.roles.iter().cloned().collect::<BTreeSet<_>>();
            let mut permissions = self.permissions.iter().cloned().collect::<BTreeSet<_>>();
            for role in &roles {
                permissions.extend(self.permissions_for_role(role));
            }

            return Ok(SecurityContext {
                actor: session.actor,
                tenant: session.tenant,
                roles,
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
                provenance: Some(session.provenance),
                trust: Some(session.trust),
            });
        }

        Ok(self.header_security_context(request))
    }

    fn header_security_context(&self, request: &HttpRequest) -> SecurityContext {
        let mut permissions = self.permissions.iter().cloned().collect::<BTreeSet<_>>();
        let mut roles = BTreeSet::new();
        for role in request_roles(request) {
            roles.insert(role.to_string());
            permissions.extend(self.permissions_for_role(role));
        }

        SecurityContext {
            actor: request.header("x-actor").unwrap_or("anonymous").to_string(),
            tenant: request.header("x-tenant").unwrap_or("default").to_string(),
            roles,
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
            provenance: Some("headers".to_string()),
            trust: Some("untrusted".to_string()),
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
        let security = match self.security_context_result(request) {
            Ok(security) => security,
            Err(error) => {
                let fallback_security = self.header_security_context(request);
                return route_error_response(
                    classify_route_error(&error.message(), Some(&error)),
                    &fallback_security,
                );
            }
        };
        if let Err(error) = self
            .tenant_guard
            .ensure_access(&security, &self.service_tenant)
        {
            let message = error.message();
            let quoted_message = serde_json::to_string(&message)
                .unwrap_or_else(|_| "\"tenant rejected\"".to_string());
            let audit_events = [format!(
                "{{\"kind\":\"tenant_isolation_violation\",\"message\":{quoted_message}}}"
            )];
            let response = route_error_response(
                RouteErrorClass::Tenant {
                    code: "tenant_isolation_violation",
                    message,
                },
                &security,
            );
            if let Err(audit_error) =
                self.record_audit_events(&security, &request.method, &request.path, &audit_events)
            {
                return route_error_response(
                    RouteErrorClass::Internal {
                        code: "audit_persist_failed",
                        message: format!("failed to persist audit events: {audit_error:?}"),
                    },
                    &security,
                );
            }
            return response;
        }
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
                    return route_error_response(
                        RouteErrorClass::Validation {
                            code: "invalid_request_body",
                            message,
                        },
                        &security,
                    );
                }
            }
        };

        let mut runtime = Runtime::with_connectors_and_security(
            self.module,
            security.clone(),
            Box::new(self.connectors.clone()),
        )
        .with_rate_limiter(self.rate_limiter.clone())
        .with_sanitizer_packs(self.sanitizer_packs.clone());
        runtime.set_output_enabled(self.output_enabled);
        let result =
            runtime.run_service_route(&self.service_name, &request.method, &request.path, input);
        if let Err(err) = self.record_audit_events(
            &security,
            &request.method,
            &request.path,
            runtime.audit_events(),
        ) {
            return route_error_response(
                RouteErrorClass::Internal {
                    code: "audit_persist_failed",
                    message: format!("failed to persist audit events: {err:?}"),
                },
                &security,
            );
        }

        match result {
            Ok(()) => HttpResponse::text(200, "OK", "ok\n"),
            Err(message) => {
                let runtime_error = runtime.last_error();
                route_error_response(classify_route_error(&message, runtime_error), &security)
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

enum RouteErrorClass {
    Validation {
        code: &'static str,
        message: String,
    },
    Permission {
        code: &'static str,
        message: String,
    },
    Tenant {
        code: &'static str,
        message: String,
    },
    Auth {
        code: String,
        message: String,
    },
    Connector {
        code: String,
        method: String,
        retryable: bool,
    },
    Workflow {
        code: &'static str,
        message: String,
    },
    Internal {
        code: &'static str,
        message: String,
    },
}

fn classify_route_error(message: &str, runtime_error: Option<&RuntimeError>) -> RouteErrorClass {
    if let Some(error) = runtime_error {
        match error {
            RuntimeError::PermissionDenied { .. } => {
                return RouteErrorClass::Permission {
                    code: "permission_denied",
                    message: error.message(),
                };
            }
            RuntimeError::TenantIsolationViolation { .. } => {
                return RouteErrorClass::Tenant {
                    code: "tenant_isolation_violation",
                    message: error.message(),
                };
            }
            RuntimeError::AuthenticationFailed { code, .. } => {
                return RouteErrorClass::Auth {
                    code: code.clone(),
                    message: error.message(),
                };
            }
            RuntimeError::ConnectorFailed {
                method,
                code,
                retryable,
                ..
            } => {
                return RouteErrorClass::Connector {
                    code: code.clone(),
                    method: method.clone(),
                    retryable: *retryable,
                };
            }
            RuntimeError::Timeout { .. } => {
                return RouteErrorClass::Workflow {
                    code: "timeout",
                    message: error.message(),
                };
            }
            RuntimeError::CostLimitExceeded { .. } => {
                return RouteErrorClass::Workflow {
                    code: "cost_limit_exceeded",
                    message: error.message(),
                };
            }
            RuntimeError::RateLimitExceeded { .. } => {
                return RouteErrorClass::Workflow {
                    code: "rate_limit_exceeded",
                    message: error.message(),
                };
            }
            RuntimeError::SecretNotFound { .. } => {
                return RouteErrorClass::Internal {
                    code: "secret_not_found",
                    message: error.message(),
                };
            }
            RuntimeError::SecretDenied { .. } => {
                return RouteErrorClass::Permission {
                    code: "secret_denied",
                    message: error.message(),
                };
            }
            RuntimeError::SecretUnavailable { .. } => {
                return RouteErrorClass::Internal {
                    code: "secret_unavailable",
                    message: error.message(),
                };
            }
            RuntimeError::SecretInvalidResponse { .. } => {
                return RouteErrorClass::Internal {
                    code: "secret_invalid_response",
                    message: error.message(),
                };
            }
            RuntimeError::EncryptionDenied { .. } => {
                return RouteErrorClass::Permission {
                    code: "encryption_denied",
                    message: error.message(),
                };
            }
            RuntimeError::EncryptionUnavailable { .. } => {
                return RouteErrorClass::Internal {
                    code: "encryption_unavailable",
                    message: error.message(),
                };
            }
            RuntimeError::EncryptionInvalidEnvelope { .. } => {
                return RouteErrorClass::Internal {
                    code: "encryption_invalid_envelope",
                    message: error.message(),
                };
            }
            RuntimeError::SanitizationFailed { .. } => {
                return RouteErrorClass::Validation {
                    code: "sanitization_failed",
                    message: error.message(),
                };
            }
            RuntimeError::ActionFailed { .. } => {
                return RouteErrorClass::Workflow {
                    code: "action_failed",
                    message: error.message(),
                };
            }
            RuntimeError::Storage(_) => {
                return RouteErrorClass::Internal {
                    code: "storage_error",
                    message: error.message(),
                };
            }
        }
    }

    if message.contains("not found") {
        RouteErrorClass::Validation {
            code: "route_not_found",
            message: message.to_string(),
        }
    } else if message.contains("Security Violation") {
        RouteErrorClass::Permission {
            code: "permission_denied",
            message: message.to_string(),
        }
    } else if message.contains("Policy Violation") {
        RouteErrorClass::Permission {
            code: "policy_denied",
            message: message.to_string(),
        }
    } else if message.contains("Missing route input") {
        RouteErrorClass::Validation {
            code: "missing_route_input",
            message: message.to_string(),
        }
    } else if message.starts_with("rejected:") {
        RouteErrorClass::Workflow {
            code: "workflow_rejected",
            message: message.to_string(),
        }
    } else {
        RouteErrorClass::Internal {
            code: "internal_error",
            message: message.to_string(),
        }
    }
}

fn route_error_response(error: RouteErrorClass, security: &SecurityContext) -> HttpResponse {
    let (status, reason, payload) = route_error_payload(error, security);
    HttpResponse::json(status, reason, payload)
}

fn route_error_payload(
    error: RouteErrorClass,
    security: &SecurityContext,
) -> (u16, &'static str, JsonValue) {
    let request_id = security.request_id.clone();
    let correlation_id = security.correlation_id.clone();
    match error {
        RouteErrorClass::Validation { code, message } => (
            if code == "route_not_found" { 404 } else { 400 },
            if code == "route_not_found" {
                "Not Found"
            } else {
                "Bad Request"
            },
            json!({
                "error": base_error("validation", code, message, request_id, correlation_id),
            }),
        ),
        RouteErrorClass::Permission { code, message } => (
            403,
            "Forbidden",
            json!({
                "error": base_error("permission", code, message, request_id, correlation_id),
            }),
        ),
        RouteErrorClass::Tenant { code, message } => (
            403,
            "Forbidden",
            json!({
                "error": base_error("tenant", code, message, request_id, correlation_id),
            }),
        ),
        RouteErrorClass::Auth { code, message } => (
            401,
            "Unauthorized",
            json!({
                "error": base_error("auth", code, message, request_id, correlation_id),
            }),
        ),
        RouteErrorClass::Connector {
            code,
            method,
            retryable,
        } => (
            502,
            "Bad Gateway",
            json!({
                "error": {
                    "kind": "connector",
                    "code": code,
                    "message": "connector call failed",
                    "request_id": request_id,
                    "correlation_id": correlation_id,
                    "connector": {
                        "method": method,
                        "retryable": retryable,
                    }
                }
            }),
        ),
        RouteErrorClass::Workflow { code, message } => (
            if code == "rate_limit_exceeded" {
                429
            } else {
                500
            },
            if code == "rate_limit_exceeded" {
                "Too Many Requests"
            } else {
                "Internal Server Error"
            },
            json!({
                "error": base_error("workflow", code, message, request_id, correlation_id),
            }),
        ),
        RouteErrorClass::Internal { code, message } => (
            500,
            "Internal Server Error",
            json!({
                "error": base_error("internal", code, message, request_id, correlation_id),
            }),
        ),
    }
}

fn base_error(
    kind: &'static str,
    code: impl Into<String>,
    message: impl AsRef<str>,
    request_id: String,
    correlation_id: String,
) -> JsonValue {
    json!({
        "kind": kind,
        "code": code.into(),
        "message": redaction::redact_text(message.as_ref()),
        "request_id": request_id,
        "correlation_id": correlation_id,
    })
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

fn current_epoch_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn jwt_runtime_error(error: crate::jwt::JwtVerificationError) -> RuntimeError {
    RuntimeError::AuthenticationFailed {
        code: error.kind().to_string(),
        reason: error.message(),
    }
}

fn session_runtime_error(error: crate::session::SessionVerificationError) -> RuntimeError {
    RuntimeError::AuthenticationFailed {
        code: error.kind().to_string(),
        reason: error.message(),
    }
}

#[cfg(test)]
mod tests {
    use super::ServiceRuntime;
    use crate::connectors::StaticConnectorRegistry;
    use crate::http::HttpRequest;
    use crate::interpreter::Value;
    use crate::jwt::{sign_hs256_for_tests, JwtVerificationConfig, JwtVerifier};
    use crate::rate_limit::{FileRateLimitStore, RateLimiter};
    use crate::session::{sign_session_for_tests, SessionVerificationConfig, SessionVerifier};
    use crate::{RuntimeError, SecretValue};
    use num_compiler::compile;
    use serde_json::json;
    use serde_json::Value as JsonValue;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    fn error_body(response: &crate::http::HttpResponse) -> JsonValue {
        serde_json::from_str(&response.body).unwrap()
    }

    fn tenant_scoped_policy_source() -> &'static str {
        r#"
module test.api

policy DataSharing {
    allow private UserInput -> ExternalApi for tenant tenant_a
}

service BillingApi {
    route POST "/refunds" {
        let email: Text from UserInput private = "user@example.com"
        external.analytics.send(email)
    }
}
"#
    }

    fn rate_limited_service_source() -> &'static str {
        r#"
module test.api

service BillingApi rate limit 1 per 1m {
    route POST "/charge" {
        audit("charge")
    }
}
"#
    }

    fn jwt_service_source() -> &'static str {
        r#"
module test.jwt

permission IssueRefund

role FinanceManager {
    allow IssueRefund
}

service BillingApi {
    route POST "/refunds" requires Permission.IssueRefund {
        audit(current_user.id)
        audit(current_user.trust)
        audit(current_user.provenance)
    }
}
"#
    }

    fn jwt_verifier() -> JwtVerifier {
        JwtVerifier::new(
            JwtVerificationConfig::new("https://issuer.example", "num-api"),
            SecretValue::new("test-signing-secret"),
        )
    }

    fn jwt_token(exp: i64) -> String {
        sign_hs256_for_tests(
            json!({"alg": "HS256", "typ": "JWT"}),
            json!({
                "iss": "https://issuer.example",
                "sub": "finance@example.com",
                "aud": "num-api",
                "exp": exp,
                "tenant": "tenant_a",
                "roles": ["FinanceManager"]
            }),
            &SecretValue::new("test-signing-secret"),
        )
    }

    fn session_verifier() -> SessionVerifier {
        SessionVerifier::new(
            SessionVerificationConfig::new("num_session"),
            SecretValue::new("test-session-secret"),
        )
    }

    fn session_token(exp: i64) -> String {
        sign_session_for_tests(
            "test-session-secret",
            json!({
                "id": "sess_123",
                "actor": "finance@example.com",
                "tenant": "tenant_a",
                "roles": ["FinanceManager"],
                "exp": exp,
                "iat": 1_700_000_000,
            }),
        )
    }

    fn unique_test_dir(name: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("num_service_rate_limit_{name}_{stamp}"))
    }

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
    fn service_route_redacts_secret_connector_failure_response() {
        let source = r#"
module test.api

type TokenRequest {
    token: Secret<Text> from HttpBody secret
}

connector secrets {
    send(token: Secret<Text> from HttpBody secret) -> Unit
}

policy SecretRouting {
    allow secret HttpBody -> secrets.send
}

service SecretApi {
    route POST "/secrets" {
        input request: TokenRequest from HttpBody private

        secrets.send(request.token)
    }
}
"#;
        let compilation = compile("test.num", source);
        let mut registry = StaticConnectorRegistry::new();
        registry.register("secrets.send", |args| {
            let Some(Value::Secret(inner)) = args.first() else {
                return Err("missing secret".to_string());
            };
            Err(format!("connector echoed {}", inner))
        });
        let runtime = ServiceRuntime::with_connectors(
            &compilation.module,
            "SecretApi".to_string(),
            vec![],
            std::sync::Arc::new(registry),
        );

        let response = runtime.handle_http_request(&HttpRequest::new(
            "POST",
            "/secrets",
            r#"{"token":"sk_live_http"}"#,
        ));

        assert_eq!(response.status, 502);
        assert_eq!(response.content_type, "application/json; charset=utf-8");
        let body = error_body(&response);
        assert_eq!(body["error"]["kind"], "connector");
        assert_eq!(body["error"]["code"], "execution_failed");
        assert_eq!(body["error"]["message"], "connector call failed");
        assert_eq!(body["error"]["connector"]["method"], "secrets.send");
        assert!(!response.body.contains("sk_live_http"));
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
        let body = error_body(&response);
        assert_eq!(body["error"]["kind"], "validation");
        assert_eq!(body["error"]["code"], "missing_route_input");
        assert_eq!(body["error"]["request_id"], "req_demo");
        assert!(body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Missing route input"));
    }

    #[test]
    fn invalid_request_body_returns_structured_validation_error() {
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
        let mut request = HttpRequest::new("POST", "/refunds", r#"{"id": 42}"#);
        request
            .headers
            .insert("x-request-id".to_string(), "req_invalid".to_string());
        request
            .headers
            .insert("x-correlation-id".to_string(), "corr_invalid".to_string());

        let response = runtime.handle_http_request(&request);

        assert_eq!(response.status, 400);
        let body = error_body(&response);
        assert_eq!(body["error"]["kind"], "validation");
        assert_eq!(body["error"]["code"], "invalid_request_body");
        assert_eq!(body["error"]["request_id"], "req_invalid");
        assert_eq!(body["error"]["correlation_id"], "corr_invalid");
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
    fn tenant_isolation_allows_same_tenant_service_request() {
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
            .with_tenant_isolation(true)
            .with_service_tenant("tenant_a");
        let mut request = HttpRequest::new("POST", "/refunds", "");
        request
            .headers
            .insert("x-tenant".to_string(), "tenant_a".to_string());

        let response = runtime.handle_http_request(&request);

        assert_eq!(response.status, 200);
        assert_eq!(response.body, "ok\n");
    }

    #[test]
    fn tenant_isolation_rejects_cross_tenant_service_request_with_audit_event() {
        let source = r#"
module test.api

service BillingApi {
    route POST "/refunds" {
        audit("refund")
    }
}
"#;
        let compilation = compile("test.num", source);
        let captured = Arc::new(Mutex::new(Vec::new()));
        let captured_events = Arc::clone(&captured);
        let runtime = ServiceRuntime::new(&compilation.module, "BillingApi", vec![])
            .with_tenant_isolation(true)
            .with_service_tenant("tenant_a")
            .with_audit_recorder(move |security, method, path, events| {
                captured_events.lock().unwrap().push((
                    security.tenant.clone(),
                    method.to_string(),
                    path.to_string(),
                    events.to_vec(),
                ));
                Ok(())
            });
        let mut request = HttpRequest::new("POST", "/refunds", "");
        request
            .headers
            .insert("x-tenant".to_string(), "tenant_b".to_string());
        request
            .headers
            .insert("x-request-id".to_string(), "req_tenant".to_string());
        request
            .headers
            .insert("x-correlation-id".to_string(), "corr_tenant".to_string());

        let response = runtime.handle_http_request(&request);

        assert_eq!(response.status, 403);
        let body = error_body(&response);
        assert_eq!(body["error"]["kind"], "tenant");
        assert_eq!(body["error"]["code"], "tenant_isolation_violation");
        assert_eq!(body["error"]["request_id"], "req_tenant");
        assert_eq!(body["error"]["correlation_id"], "corr_tenant");
        let records = captured.lock().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].0, "tenant_b");
        assert_eq!(records[0].1, "POST");
        assert_eq!(records[0].2, "/refunds");
        assert_eq!(records[0].3.len(), 1);
        assert!(records[0].3[0].contains("tenant_isolation_violation"));
    }

    #[test]
    fn service_route_policy_uses_request_tenant_context() {
        let compilation = compile("test.num", tenant_scoped_policy_source());
        assert!(
            compilation
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "N2400"),
            "standalone checks must stay conservative without runtime tenant context"
        );
        assert!(num_compiler::semantic::check_service_route_for_tenant(
            &compilation.module,
            "BillingApi",
            "POST",
            "/refunds",
            "tenant_a",
        )
        .is_empty());
        assert!(num_compiler::semantic::check_service_route_for_tenant(
            &compilation.module,
            "BillingApi",
            "POST",
            "/refunds",
            "tenant_b",
        )
        .iter()
        .any(|diagnostic| diagnostic.code == "N2400"));

        let runtime = ServiceRuntime::new(&compilation.module, "BillingApi", vec![]);
        let mut request = HttpRequest::new("POST", "/refunds", "");
        request
            .headers
            .insert("x-tenant".to_string(), "tenant_a".to_string());

        let response = runtime.handle_http_request(&request);

        assert_eq!(response.status, 200);
        assert_eq!(response.body, "ok\n");
    }

    #[test]
    fn service_route_policy_denies_wrong_request_tenant() {
        let compilation = compile("test.num", tenant_scoped_policy_source());
        let runtime = ServiceRuntime::new(&compilation.module, "BillingApi", vec![]);
        let mut request = HttpRequest::new("POST", "/refunds", "");
        request
            .headers
            .insert("x-tenant".to_string(), "tenant_b".to_string());
        request
            .headers
            .insert("x-request-id".to_string(), "req_policy".to_string());
        request
            .headers
            .insert("x-correlation-id".to_string(), "corr_policy".to_string());

        let response = runtime.handle_http_request(&request);

        assert_eq!(response.status, 403);
        let body = error_body(&response);
        assert_eq!(body["error"]["kind"], "permission");
        assert_eq!(body["error"]["code"], "policy_denied");
        assert_eq!(body["error"]["request_id"], "req_policy");
        assert_eq!(body["error"]["correlation_id"], "corr_policy");
    }

    #[test]
    fn disabled_tenant_isolation_allows_cross_tenant_service_request() {
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
            .with_tenant_isolation(false)
            .with_service_tenant("tenant_a");
        let mut request = HttpRequest::new("POST", "/refunds", "");
        request
            .headers
            .insert("x-tenant".to_string(), "tenant_b".to_string());

        let response = runtime.handle_http_request(&request);

        assert_eq!(response.status, 200);
        assert_eq!(response.body, "ok\n");
    }

    #[test]
    fn service_runtime_shares_rate_limits_across_requests() {
        let compilation = compile("test.num", rate_limited_service_source());
        let runtime = ServiceRuntime::new(&compilation.module, "BillingApi", vec![]);
        let request = HttpRequest::new("POST", "/charge", "");

        let first = runtime.handle_http_request(&request);
        let second = runtime.handle_http_request(&request);

        assert_eq!(first.status, 200);
        assert_eq!(second.status, 429);
        let body = error_body(&second);
        assert_eq!(body["error"]["kind"], "workflow");
        assert_eq!(body["error"]["code"], "rate_limit_exceeded");
    }

    #[test]
    fn file_rate_limit_store_shares_limits_across_service_runtimes() {
        let compilation = compile("test.num", rate_limited_service_source());
        let root = unique_test_dir("file-backed");
        let path = root.join("rate-limits.json");
        let first_runtime = ServiceRuntime::new(&compilation.module, "BillingApi", vec![])
            .with_rate_limiter(RateLimiter::with_store(FileRateLimitStore::new(&path)));
        let second_runtime = ServiceRuntime::new(&compilation.module, "BillingApi", vec![])
            .with_rate_limiter(RateLimiter::with_store(FileRateLimitStore::new(&path)));
        let mut request = HttpRequest::new("POST", "/charge", "");
        request
            .headers
            .insert("x-tenant".to_string(), "tenant_a".to_string());
        request
            .headers
            .insert("x-actor".to_string(), "actor_a".to_string());

        let first = first_runtime.handle_http_request(&request);
        let second = second_runtime.handle_http_request(&request);

        assert_eq!(first.status, 200);
        assert_eq!(second.status, 429);
        let body = error_body(&second);
        assert_eq!(body["error"]["code"], "rate_limit_exceeded");
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
    fn jwt_auth_populates_service_security_context_and_roles() {
        let compilation = compile("test.num", jwt_service_source());
        let captured = Arc::new(Mutex::new(Vec::new()));
        let captured_events = Arc::clone(&captured);
        let runtime = ServiceRuntime::new(&compilation.module, "BillingApi", vec![])
            .with_jwt_verifier(jwt_verifier())
            .with_jwt_clock(|| 1_700_000_100)
            .with_audit_recorder(move |security, method, path, events| {
                captured_events.lock().unwrap().push((
                    security.actor.clone(),
                    security.tenant.clone(),
                    security.roles.iter().cloned().collect::<Vec<_>>(),
                    security.provenance.clone(),
                    security.trust.clone(),
                    method.to_string(),
                    path.to_string(),
                    events.to_vec(),
                ));
                Ok(())
            });
        let mut request = HttpRequest::new("POST", "/refunds", "");
        request.headers.insert(
            "authorization".to_string(),
            format!("Bearer {}", jwt_token(4_102_444_800)),
        );

        let response = runtime.handle_http_request(&request);

        assert_eq!(response.status, 200);
        let records = captured.lock().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].0, "finance@example.com");
        assert_eq!(records[0].1, "tenant_a");
        assert_eq!(records[0].2, vec!["FinanceManager"]);
        assert_eq!(records[0].3.as_deref(), Some("jwt:https://issuer.example"));
        assert_eq!(records[0].4.as_deref(), Some("verified"));
        assert_eq!(records[0].5, "POST");
        assert_eq!(records[0].6, "/refunds");
        assert_eq!(
            records[0].7,
            vec![
                "\"finance@example.com\"".to_string(),
                "\"verified\"".to_string(),
                "\"jwt:https://issuer.example\"".to_string()
            ]
        );
    }

    #[test]
    fn jwt_auth_fails_closed_for_missing_and_expired_tokens() {
        let compilation = compile("test.num", jwt_service_source());
        let runtime = ServiceRuntime::new(&compilation.module, "BillingApi", vec![])
            .with_jwt_verifier(jwt_verifier())
            .with_jwt_clock(|| 1_700_000_100);

        let missing = runtime.handle_http_request(&HttpRequest::new("POST", "/refunds", ""));
        assert_eq!(missing.status, 401);
        let missing_body = error_body(&missing);
        assert_eq!(missing_body["error"]["kind"], "auth");
        assert_eq!(missing_body["error"]["code"], "jwt_missing");

        let mut expired_request = HttpRequest::new("POST", "/refunds", "");
        expired_request.headers.insert(
            "authorization".to_string(),
            format!("Bearer {}", jwt_token(1_700_000_000)),
        );
        let expired = runtime.handle_http_request(&expired_request);
        assert_eq!(expired.status, 401);
        let expired_body = error_body(&expired);
        assert_eq!(expired_body["error"]["kind"], "auth");
        assert_eq!(expired_body["error"]["code"], "jwt_expired");
        assert!(!expired.body.contains("test-signing-secret"));
    }

    #[test]
    fn signed_session_cookie_populates_service_security_context_and_roles() {
        let compilation = compile("test.num", jwt_service_source());
        let captured = Arc::new(Mutex::new(Vec::new()));
        let captured_events = Arc::clone(&captured);
        let runtime = ServiceRuntime::new(&compilation.module, "BillingApi", vec![])
            .with_session_verifier(session_verifier())
            .with_jwt_clock(|| 1_700_000_100)
            .with_audit_recorder(move |security, method, path, events| {
                captured_events.lock().unwrap().push((
                    security.actor.clone(),
                    security.tenant.clone(),
                    security.roles.iter().cloned().collect::<Vec<_>>(),
                    security.provenance.clone(),
                    security.trust.clone(),
                    method.to_string(),
                    path.to_string(),
                    events.to_vec(),
                ));
                Ok(())
            });
        let mut request = HttpRequest::new("POST", "/refunds", "");
        request.headers.insert(
            "cookie".to_string(),
            format!("theme=light; num_session={}", session_token(4_102_444_800)),
        );

        let response = runtime.handle_http_request(&request);

        assert_eq!(response.status, 200);
        let records = captured.lock().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].0, "finance@example.com");
        assert_eq!(records[0].1, "tenant_a");
        assert_eq!(records[0].2, vec!["FinanceManager"]);
        assert_eq!(records[0].3.as_deref(), Some("session:sess_123"));
        assert_eq!(records[0].4.as_deref(), Some("verified"));
        assert_eq!(records[0].5, "POST");
        assert_eq!(records[0].6, "/refunds");
        assert_eq!(
            records[0].7,
            vec![
                "\"finance@example.com\"".to_string(),
                "\"verified\"".to_string(),
                "\"session:sess_123\"".to_string()
            ]
        );
    }

    #[test]
    fn signed_session_cookie_fails_closed_for_missing_expired_and_tampered_requests() {
        let compilation = compile("test.num", jwt_service_source());
        let runtime = ServiceRuntime::new(&compilation.module, "BillingApi", vec![])
            .with_session_verifier(session_verifier())
            .with_jwt_clock(|| 1_700_000_100);

        let missing = runtime.handle_http_request(&HttpRequest::new("POST", "/refunds", ""));
        assert_eq!(missing.status, 401);
        let missing_body = error_body(&missing);
        assert_eq!(missing_body["error"]["kind"], "auth");
        assert_eq!(missing_body["error"]["code"], "session_missing");

        let mut expired_request = HttpRequest::new("POST", "/refunds", "");
        expired_request.headers.insert(
            "cookie".to_string(),
            format!("num_session={}", session_token(1_700_000_000)),
        );
        let expired = runtime.handle_http_request(&expired_request);
        assert_eq!(expired.status, 401);
        let expired_body = error_body(&expired);
        assert_eq!(expired_body["error"]["kind"], "auth");
        assert_eq!(expired_body["error"]["code"], "session_expired");

        let mut tampered_cookie = session_token(4_102_444_800);
        tampered_cookie.push('x');
        let mut tampered_request = HttpRequest::new("POST", "/refunds", "");
        tampered_request.headers.insert(
            "cookie".to_string(),
            format!("num_session={tampered_cookie}"),
        );
        let tampered = runtime.handle_http_request(&tampered_request);
        assert_eq!(tampered.status, 401);
        let tampered_body = error_body(&tampered);
        assert_eq!(tampered_body["error"]["kind"], "auth");
        assert_eq!(tampered_body["error"]["code"], "session_invalid_signature");
        assert!(!tampered.body.contains("test-session-secret"));
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
        let body = error_body(&response);
        assert_eq!(body["error"]["kind"], "permission");
        assert_eq!(body["error"]["code"], "permission_denied");
        assert!(body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("IssueRefund"));
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
                Err(RuntimeError::Storage(
                    "audit disk is unavailable".to_string(),
                ))
            });

        let response = runtime.handle_http_request(&HttpRequest::new("POST", "/refunds", ""));

        assert_eq!(response.status, 500);
        let body = error_body(&response);
        assert_eq!(body["error"]["kind"], "internal");
        assert_eq!(body["error"]["code"], "audit_persist_failed");
        assert!(body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("failed to persist audit events"));
    }
}
