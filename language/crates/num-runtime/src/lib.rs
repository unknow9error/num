use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, SystemTime};

pub mod audit_report;
pub mod connectors;
pub mod cost;
pub mod cost_report;
pub mod database;
pub mod debugger;
pub mod engine;
pub mod events;
pub mod execution;
pub mod http;
pub mod interpreter;
pub mod json;
pub mod observability;
pub mod process_connectors;
pub mod rate_limit;
pub mod sanitization;
pub mod secrets;
pub mod service;
pub mod storage;
pub mod tenant;
pub mod worker;
pub mod workflow_report;

pub type WorkflowId = String;
pub type TenantId = String;
pub type ActorId = String;
pub type Permission = String;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowStatus {
    Created,
    Running,
    Waiting,
    Failed,
    Compensated,
    Completed,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct SecurityContext {
    pub actor: ActorId,
    pub tenant: TenantId,
    pub permissions: BTreeSet<Permission>,
    pub correlation_id: String,
    pub request_id: String,
}

impl SecurityContext {
    pub fn has_permission(&self, permission: &str) -> bool {
        self.permissions.contains(permission)
    }
}

#[derive(Debug, Clone)]
pub struct WorkflowState {
    pub id: WorkflowId,
    pub name: String,
    pub status: WorkflowStatus,
    pub security: SecurityContext,
    pub started_at: SystemTime,
    pub updated_at: SystemTime,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct ActionSpec {
    pub name: String,
    pub required_permissions: Vec<Permission>,
    pub risk: RiskLevel,
    pub timeout: Option<Duration>,
    pub rollback: Option<String>,
    pub idempotency_key: Option<String>,
    pub max_cost: Option<Money>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Money {
    pub minor_units: i128,
    pub currency: String,
}

#[derive(Debug, Clone)]
pub struct AuditEvent {
    pub event_id: String,
    pub timestamp: SystemTime,
    pub actor: ActorId,
    pub tenant: TenantId,
    pub action: String,
    pub result: AuditResult,
    pub permissions_used: Vec<Permission>,
    pub data_sources: Vec<String>,
    pub ai_models: Vec<String>,
    pub confidence_values: Vec<f64>,
    pub rollback_status: Option<String>,
    pub correlation_id: String,
    pub request_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditResult {
    Started,
    Waiting,
    Resumed,
    Succeeded,
    Failed(String),
    RolledBack,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct Uncertain<T> {
    pub value: T,
    pub confidence: f64,
    pub source: String,
    pub explanation: Option<String>,
    pub evidence: Vec<String>,
}

impl<T> Uncertain<T> {
    pub fn accept(self, threshold: f64) -> Result<T, UncertaintyError<T>> {
        if self.confidence >= threshold {
            Ok(self.value)
        } else {
            Err(UncertaintyError {
                value: self.value,
                confidence: self.confidence,
                threshold,
            })
        }
    }
}

#[derive(Debug, Clone)]
pub struct UncertaintyError<T> {
    pub value: T,
    pub confidence: f64,
    pub threshold: f64,
}

pub trait AuditSink {
    fn append(&mut self, event: AuditEvent) -> Result<(), RuntimeError>;
}

pub trait StateStore {
    fn save_workflow(&mut self, state: WorkflowState) -> Result<(), RuntimeError>;
    fn load_workflow(&self, id: &str) -> Result<Option<WorkflowState>, RuntimeError>;
}

pub trait SecretStore {
    fn put_secret(&mut self, name: &str, value: SecretValue) -> Result<(), RuntimeError>;
    fn get_secret(&self, name: &str) -> Result<SecretValue, RuntimeError>;
    fn delete_secret(&mut self, name: &str) -> Result<(), RuntimeError>;
}

#[derive(Clone, PartialEq, Eq)]
pub struct SecretValue {
    bytes: Vec<u8>,
}

impl SecretValue {
    pub fn new(value: impl Into<Vec<u8>>) -> Self {
        Self {
            bytes: value.into(),
        }
    }

    pub fn expose(&self) -> &[u8] {
        &self.bytes
    }

    pub fn expose_text(&self) -> Result<&str, RuntimeError> {
        std::str::from_utf8(&self.bytes)
            .map_err(|err| RuntimeError::Storage(format!("secret is not valid UTF-8: {err}")))
    }
}

impl std::fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretValue(<redacted>)")
    }
}

#[derive(Debug, Clone)]
pub enum RuntimeError {
    PermissionDenied {
        permission: Permission,
    },
    CostLimitExceeded {
        limit: Money,
        actual: Money,
    },
    RateLimitExceeded {
        scope: String,
        limit: u32,
    },
    Timeout {
        action: String,
    },
    ActionFailed {
        action: String,
        reason: String,
    },
    ConnectorFailed {
        method: String,
        code: String,
        message: String,
        retryable: bool,
    },
    SanitizationFailed {
        reason: String,
    },
    TenantIsolationViolation {
        expected: TenantId,
        actual: TenantId,
    },
    SecretNotFound {
        name: String,
    },
    Storage(String),
}

pub fn require_permission(ctx: &SecurityContext, permission: &str) -> Result<(), RuntimeError> {
    if ctx.has_permission(permission) {
        Ok(())
    } else {
        Err(RuntimeError::PermissionDenied {
            permission: permission.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::connectors::StaticConnectorRegistry;
    use super::interpreter::Runtime;
    use super::interpreter::Value;
    use super::observability::RuntimeTraceKind;
    use super::SecurityContext;
    use num_compiler::compile;
    use std::collections::{BTreeSet, HashMap};

    fn security_context<const N: usize>(
        actor: &str,
        tenant: &str,
        permissions: [&str; N],
    ) -> SecurityContext {
        SecurityContext {
            actor: actor.to_string(),
            tenant: tenant.to_string(),
            permissions: permissions
                .into_iter()
                .map(str::to_string)
                .collect::<BTreeSet<_>>(),
            correlation_id: "corr_test".to_string(),
            request_id: "req_test".to_string(),
        }
    }

    #[test]
    fn test_interpreter_success() {
        let source = r#"
module test.flow

permission Execute

workflow main() {
    require Permission.Execute for current_user
    let x = 42
}
"#;
        let compilation = compile("test.num", source);
        let mut runtime = Runtime::new(&compilation.module, vec!["Execute".to_string()]);
        let res = runtime.run_workflow("main", HashMap::new());
        assert!(res.is_ok());
    }

    #[test]
    fn test_interpreter_permission_violation() {
        let source = r#"
module test.flow

permission Execute

workflow main() {
    require Permission.Execute for current_user
}
"#;
        let compilation = compile("test.num", source);
        let mut runtime = Runtime::new(&compilation.module, vec![]);
        let res = runtime.run_workflow("main", HashMap::new());
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("Security Violation"));
    }

    #[test]
    fn test_service_route_success() {
        let source = r#"
module test.api

permission IssueRefund

type RefundRequest {
    id: Text
}

action issue_refund(request: RefundRequest)
    requires Permission.IssueRefund
    risk high
    rollback reverse_refund(request)
{
    audit("refund")
}

service BillingApi {
    route POST "/refunds" requires Permission.IssueRefund {
        input request: RefundRequest from HttpBody private
        issue_refund(request)
    }
}
"#;
        let compilation = compile("test.num", source);
        let mut runtime = Runtime::new(&compilation.module, vec!["IssueRefund".to_string()]);
        let mut fields = HashMap::new();
        fields.insert(
            "id".to_string(),
            super::interpreter::Value::String("pay_1".to_string()),
        );
        let res = runtime.run_service_route(
            "BillingApi",
            "POST",
            "/refunds",
            Some(super::interpreter::Value::Struct(
                "RefundRequest".to_string(),
                fields,
            )),
        );
        assert!(res.is_ok());
    }

    #[test]
    fn test_service_route_permission_violation() {
        let source = r#"
module test.api

permission IssueRefund

type RefundRequest {
    id: Text
}

service BillingApi {
    route POST "/refunds" requires Permission.IssueRefund {
        input request: RefundRequest from HttpBody private
        audit("refund")
    }
}
"#;
        let compilation = compile("test.num", source);
        let mut runtime = Runtime::new(&compilation.module, vec![]);
        let mut fields = HashMap::new();
        fields.insert(
            "id".to_string(),
            super::interpreter::Value::String("pay_1".to_string()),
        );
        let res = runtime.run_service_route(
            "BillingApi",
            "POST",
            "/refunds",
            Some(super::interpreter::Value::Struct(
                "RefundRequest".to_string(),
                fields,
            )),
        );
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("Security Violation"));
    }

    #[test]
    fn test_runtime_uses_custom_connector_registry() {
        let source = r#"
module test.connector

connector echo {
    text(value: Text) -> Text
}

workflow main() {
    let message: Text = echo.text("hello")
}
"#;
        let compilation = compile("test.num", source);
        let mut registry = StaticConnectorRegistry::new();
        registry.register("echo.text", |args| {
            Ok(args.first().cloned().unwrap_or(Value::Null))
        });
        let mut runtime = Runtime::with_connectors(&compilation.module, vec![], Box::new(registry));
        let res = runtime.run_workflow("main", HashMap::new());

        assert!(res.is_ok());
    }

    #[test]
    fn test_runtime_executes_branded_alias_constructor() {
        let source = r#"
module test.brand

type PaymentId = Brand<Text, "PaymentId">

connector payments {
    find(payment_id: PaymentId) -> Unit
}

workflow main() {
    payments.find(PaymentId("pay_1"))
}
"#;
        let compilation = compile("test.num", source);
        let received = std::rc::Rc::new(std::cell::RefCell::new(String::new()));
        let received_for_handler = received.clone();
        let mut registry = StaticConnectorRegistry::new();
        registry.register("payments.find", move |args| {
            *received_for_handler.borrow_mut() =
                args.first().cloned().unwrap_or(Value::Null).to_string();
            Ok(Value::Null)
        });
        let mut runtime = Runtime::with_connectors(&compilation.module, vec![], Box::new(registry));
        let res = runtime.run_workflow("main", HashMap::new());

        assert!(res.is_ok());
        assert_eq!(received.borrow().as_str(), "\"pay_1\"");
    }

    #[test]
    fn test_runtime_executes_generic_branded_alias_constructor() {
        let source = r#"
module test.generic_brand

type Boxed<T> = Brand<T, "Boxed">

connector sink {
    write(value: Boxed<Int>) -> Unit
}

workflow main() {
    sink.write(Boxed(42))
}
"#;
        let compilation = compile("test.num", source);
        let received = std::rc::Rc::new(std::cell::RefCell::new(String::new()));
        let received_for_handler = received.clone();
        let mut registry = StaticConnectorRegistry::new();
        registry.register("sink.write", move |args| {
            *received_for_handler.borrow_mut() =
                args.first().cloned().unwrap_or(Value::Null).to_string();
            Ok(Value::Null)
        });
        let mut runtime = Runtime::with_connectors(&compilation.module, vec![], Box::new(registry));
        let res = runtime.run_workflow("main", HashMap::new());

        assert!(res.is_ok());
        assert_eq!(received.borrow().as_str(), "42");
    }

    #[test]
    fn test_runtime_unbrands_branded_alias_values() {
        let source = r#"
module test.unbrand

type PaymentId = Brand<Text, "PaymentId">

connector payments {
    find(raw_id: Text) -> Unit
}

workflow main() {
    payments.find(unbrand(PaymentId("pay_1")))
}
"#;
        let compilation = compile("test.num", source);
        let received = std::rc::Rc::new(std::cell::RefCell::new(String::new()));
        let received_for_handler = received.clone();
        let mut registry = StaticConnectorRegistry::new();
        registry.register("payments.find", move |args| {
            *received_for_handler.borrow_mut() =
                args.first().cloned().unwrap_or(Value::Null).to_string();
            Ok(Value::Null)
        });
        let mut runtime = Runtime::with_connectors(&compilation.module, vec![], Box::new(registry));
        let res = runtime.run_workflow("main", HashMap::new());

        assert!(res.is_ok());
        assert_eq!(received.borrow().as_str(), "\"pay_1\"");
    }

    #[test]
    fn test_runtime_matches_union_struct_member() {
        let source = r#"
module test.union

type User {
    email: Text
}

type Company {
    name: Text
}

type SearchResult = User | Company

connector search {
    first() -> SearchResult
}

workflow main() {
    let result: SearchResult = search.first()
    match result {
        User { email } => {
            audit(email)
        }
        Company { name: company_name } => {
            audit(company_name)
        }
    }
}
"#;
        let compilation = compile("test.num", source);
        let mut registry = StaticConnectorRegistry::new();
        registry.register("search.first", |_| {
            let mut fields = HashMap::new();
            fields.insert(
                "email".to_string(),
                Value::String("user@example.com".to_string()),
            );
            Ok(Value::Struct("User".to_string(), fields))
        });
        let mut runtime = Runtime::with_connectors(&compilation.module, vec![], Box::new(registry));
        let res = runtime.run_workflow("main", HashMap::new());

        assert!(res.is_ok());
    }

    #[test]
    fn test_runtime_matches_nested_union_struct_member() {
        let source = r#"
module test.nested_union

type Profile {
    email: Email
}

type User {
    profile: Profile
}

type Company {
    name: Text
}

type SearchResult = User | Company

connector search {
    first() -> SearchResult
}

workflow main() {
    let result: SearchResult = search.first()
    match result {
        User { profile: Profile { email } } => {
            audit(email)
        }
        Company => {
            audit("company")
        }
    }
}
"#;
        let compilation = compile("test.num", source);
        let mut registry = StaticConnectorRegistry::new();
        registry.register("search.first", |_| {
            let mut profile_fields = HashMap::new();
            profile_fields.insert(
                "email".to_string(),
                Value::String("user@example.com".to_string()),
            );
            let mut user_fields = HashMap::new();
            user_fields.insert(
                "profile".to_string(),
                Value::Struct("Profile".to_string(), profile_fields),
            );
            Ok(Value::Struct("User".to_string(), user_fields))
        });
        let mut runtime = Runtime::with_connectors(&compilation.module, vec![], Box::new(registry));
        let res = runtime.run_workflow("main", HashMap::new());

        assert!(res.is_ok());
        assert_eq!(
            runtime.audit_events(),
            &["\"user@example.com\"".to_string()]
        );
    }

    #[test]
    fn test_runtime_matches_enum_payload() {
        let source = r#"
module test.enum_payload

enum PaymentStatus {
    Paid
    Failed(Text)
}

connector logger {
    send(reason: Text) -> Unit
}

workflow main() {
    let status = Failed("network")
    match status {
        Failed(reason) => {
            logger.send(reason)
        }
        Paid => {
            logger.send("paid")
        }
    }
}
"#;
        let compilation = compile("test.num", source);
        let received = std::rc::Rc::new(std::cell::RefCell::new(String::new()));
        let received_for_handler = received.clone();
        let mut registry = StaticConnectorRegistry::new();
        registry.register("logger.send", move |args| {
            *received_for_handler.borrow_mut() =
                args.first().cloned().unwrap_or(Value::Null).to_string();
            Ok(Value::Null)
        });
        let mut runtime = Runtime::with_connectors(&compilation.module, vec![], Box::new(registry));
        let res = runtime.run_workflow("main", HashMap::new());

        assert!(res.is_ok());
        assert_eq!(received.borrow().as_str(), "\"network\"");
    }

    #[test]
    fn test_runtime_skips_false_match_guard() {
        let source = r#"
module test.match_guard

enum Decision {
    Approve(Int)
    Reject
}

workflow main() {
    let decision = Approve(70)
    match decision {
        Approve(score) if score >= 90 => {
            audit("auto_approved")
        }
        Approve(score) if score >= 60 => {
            audit("manual_review")
        }
        Approve(_) => {
            audit("fallback")
        }
        Reject => {
            audit("rejected")
        }
    }
}
"#;
        let compilation = compile("test.num", source);
        let mut runtime = Runtime::new(&compilation.module, vec![]);
        let res = runtime.run_workflow("main", HashMap::new());

        assert!(res.is_ok());
        assert_eq!(runtime.audit_events(), &["\"manual_review\"".to_string()]);
    }

    #[test]
    fn test_action_retry_metadata_retries_connector_failure() {
        let source = r#"
module test.retry

connector unstable {
    send() -> Unit
}

action send_once()
    retry 3
{
    unstable.send()
}

workflow main() {
    send_once()
}
"#;
        let compilation = compile("test.num", source);
        let calls = std::rc::Rc::new(std::cell::RefCell::new(0));
        let calls_for_handler = calls.clone();
        let mut registry = StaticConnectorRegistry::new();
        registry.register("unstable.send", move |_| {
            let mut calls = calls_for_handler.borrow_mut();
            *calls += 1;
            if *calls == 1 {
                Err("temporary failure".to_string())
            } else {
                Ok(Value::Null)
            }
        });
        let mut runtime = Runtime::with_connectors(&compilation.module, vec![], Box::new(registry));
        let res = runtime.run_workflow("main", HashMap::new());

        assert!(res.is_ok());
        assert_eq!(*calls.borrow(), 2);
    }

    #[test]
    fn test_action_idempotency_metadata_replays_successful_action() {
        let source = r#"
module test.idempotency

connector counters {
    hit(id: Text) -> Unit
}

action record_once(id: Text)
    idempotency key id
{
    counters.hit(id)
}

workflow main() {
    record_once("pay_1")
    record_once("pay_1")
}
"#;
        let compilation = compile("test.num", source);
        let calls = std::rc::Rc::new(std::cell::RefCell::new(0));
        let calls_for_handler = calls.clone();
        let mut registry = StaticConnectorRegistry::new();
        registry.register("counters.hit", move |_| {
            *calls_for_handler.borrow_mut() += 1;
            Ok(Value::Null)
        });
        let mut runtime = Runtime::with_connectors(&compilation.module, vec![], Box::new(registry));
        let res = runtime.run_workflow("main", HashMap::new());

        assert!(res.is_ok());
        assert_eq!(*calls.borrow(), 1);
    }

    #[test]
    fn test_action_cost_metadata_is_charged_once_for_successful_idempotent_action() {
        let source = r#"
module test.costs

connector counters {
    hit(id: Text) -> Unit
}

action record_once(id: Text)
    cost 1.25 USD
    idempotency key id
{
    counters.hit(id)
}

workflow main() {
    record_once("pay_1")
    record_once("pay_1")
}
"#;
        let compilation = compile("test.num", source);
        let calls = std::rc::Rc::new(std::cell::RefCell::new(0));
        let calls_for_handler = calls.clone();
        let mut registry = StaticConnectorRegistry::new();
        registry.register("counters.hit", move |_| {
            *calls_for_handler.borrow_mut() += 1;
            Ok(Value::Null)
        });
        let mut runtime = Runtime::with_connectors(&compilation.module, vec![], Box::new(registry));
        let res = runtime.run_workflow("main", HashMap::new());

        assert!(res.is_ok());
        assert_eq!(*calls.borrow(), 1);
        assert_eq!(runtime.cost_spent("USD").minor_units, 125);
        assert_eq!(runtime.cost_entry_count(), 1);
    }

    #[test]
    fn test_workflow_budget_metadata_rejects_over_budget_action_cost() {
        let source = r#"
module test.budget

connector counters {
    hit() -> Unit
}

action expensive()
    cost 2 USD
{
    counters.hit()
}

workflow main() budget 1 USD {
    expensive()
}
"#;
        let compilation = compile("test.num", source);
        let calls = std::rc::Rc::new(std::cell::RefCell::new(0));
        let calls_for_handler = calls.clone();
        let mut registry = StaticConnectorRegistry::new();
        registry.register("counters.hit", move |_| {
            *calls_for_handler.borrow_mut() += 1;
            Ok(Value::Null)
        });
        let mut runtime = Runtime::with_connectors(&compilation.module, vec![], Box::new(registry));
        let res = runtime.run_workflow("main", HashMap::new());

        assert!(res.is_err());
        assert!(res.unwrap_err().contains("Cost limit exceeded"));
        assert_eq!(*calls.borrow(), 0);
        assert_eq!(runtime.cost_entry_count(), 0);
    }

    #[test]
    fn test_function_budget_metadata_rejects_over_budget_nested_action_cost() {
        let source = r#"
module test.function_budget

connector counters {
    hit() -> Unit
}

action expensive()
    cost 2 USD
{
    counters.hit()
}

fn guarded_step() budget 1 USD {
    expensive()
}

workflow main() budget 10 USD {
    guarded_step()
}
"#;
        let compilation = compile("test.num", source);
        let calls = std::rc::Rc::new(std::cell::RefCell::new(0));
        let calls_for_handler = calls.clone();
        let mut registry = StaticConnectorRegistry::new();
        registry.register("counters.hit", move |_| {
            *calls_for_handler.borrow_mut() += 1;
            Ok(Value::Null)
        });
        let mut runtime = Runtime::with_connectors(&compilation.module, vec![], Box::new(registry));
        let res = runtime.run_workflow("main", HashMap::new());

        assert!(res.is_err());
        assert!(res.unwrap_err().contains("Cost limit exceeded"));
        assert_eq!(*calls.borrow(), 0);
        assert_eq!(runtime.cost_entry_count(), 0);
    }

    #[test]
    fn test_runtime_uses_function_return_value() {
        let source = r#"
module test.function_return

fn message() -> Text {
    return "ok"
}

workflow main() {
    let value: Text = message()
    audit(value)
}
"#;
        let compilation = compile("test.num", source);
        let mut runtime = Runtime::new(&compilation.module, vec![]);

        runtime.run_workflow("main", HashMap::new()).unwrap();

        assert_eq!(runtime.audit_events(), &["\"ok\"".to_string()]);
    }

    #[test]
    fn test_runtime_security_context_sets_current_user() {
        let source = r#"
module test.security_context

workflow main() {
    audit(current_user.id)
}
"#;
        let compilation = compile("test.num", source);
        let runtime_security = security_context("agent@example.com", "tenant_a", []);
        let mut runtime = Runtime::with_security(&compilation.module, runtime_security);

        runtime.run_workflow("main", HashMap::new()).unwrap();

        assert_eq!(
            runtime.audit_events(),
            &["\"agent@example.com\"".to_string()]
        );
    }

    #[test]
    fn test_runtime_executes_privacy_and_trust_gateways_as_value_builtins() {
        let source = r#"
module test.gateways

workflow main() {
    let marker: Text = sanitize(anonymize("private@example.com"))
    audit(marker)
}
"#;
        let compilation = compile("test.num", source);
        let mut runtime = Runtime::new(&compilation.module, vec![]);

        runtime.run_workflow("main", HashMap::new()).unwrap();

        assert_eq!(
            runtime.audit_events(),
            &["\"private@example.com\"".to_string()]
        );
    }

    #[test]
    fn test_runtime_records_trace_events() {
        let source = r#"
module test.traces

workflow main() {
    let value: Text = "ok"
    audit(value)
}
"#;
        let compilation = compile("test.num", source);
        let mut runtime = Runtime::new(&compilation.module, vec![]);

        runtime.run_workflow("main", HashMap::new()).unwrap();

        let kinds = runtime
            .trace_events()
            .iter()
            .map(|event| event.kind.as_str())
            .collect::<Vec<_>>();
        assert!(kinds.contains(&"WorkflowStarted"));
        assert!(kinds.contains(&"StatementStarted"));
        assert!(kinds.contains(&"AuditLogged"));
        assert!(kinds.contains(&"WorkflowCompleted"));
        assert_eq!(runtime.trace_events()[0].sequence, 1);
    }

    #[test]
    fn test_parent_budget_scope_limits_nested_function_action_costs() {
        let source = r#"
module test.parent_budget

connector counters {
    hit(id: Text) -> Unit
}

action charge(id: Text)
    cost 2 USD
{
    counters.hit(id)
}

fn guarded_step(id: Text) budget 10 USD {
    charge(id)
}

workflow main() budget 3 USD {
    guarded_step("first")
    guarded_step("second")
}
"#;
        let compilation = compile("test.num", source);
        let calls = std::rc::Rc::new(std::cell::RefCell::new(0));
        let calls_for_handler = calls.clone();
        let mut registry = StaticConnectorRegistry::new();
        registry.register("counters.hit", move |_| {
            *calls_for_handler.borrow_mut() += 1;
            Ok(Value::Null)
        });
        let mut runtime = Runtime::with_connectors(&compilation.module, vec![], Box::new(registry));
        let res = runtime.run_workflow("main", HashMap::new());

        assert!(res.is_err());
        assert!(res.unwrap_err().contains("Cost limit exceeded"));
        assert_eq!(*calls.borrow(), 1);
        assert_eq!(runtime.cost_spent("USD").minor_units, 200);
        assert_eq!(runtime.cost_entry_count(), 1);
    }

    #[test]
    fn test_workflow_rate_limit_metadata_rejects_second_run() {
        let source = r#"
module test.rate

workflow main() rate limit 1 per 1m {
    audit("run")
}
"#;
        let compilation = compile("test.num", source);
        let mut runtime = Runtime::new(&compilation.module, vec![]);

        assert!(runtime.run_workflow("main", HashMap::new()).is_ok());
        let res = runtime.run_workflow("main", HashMap::new());

        assert!(res.is_err());
        assert!(res.unwrap_err().contains("Rate limit exceeded"));
    }

    #[test]
    fn test_service_budget_metadata_rejects_over_budget_route_action_cost() {
        let source = r#"
module test.service_budget

connector counters {
    hit() -> Unit
}

action expensive()
    cost 2 USD
{
    counters.hit()
}

service BillingApi budget 1 USD {
    route POST "/charge" {
        expensive()
    }
}
"#;
        let compilation = compile("test.num", source);
        let mut registry = StaticConnectorRegistry::new();
        registry.register("counters.hit", |_| Ok(Value::Null));
        let mut runtime = Runtime::with_connectors(&compilation.module, vec![], Box::new(registry));
        let res = runtime.run_service_route("BillingApi", "POST", "/charge", None);

        assert!(res.is_err());
        assert!(res.unwrap_err().contains("Cost limit exceeded"));
        assert_eq!(runtime.cost_entry_count(), 0);
    }

    #[test]
    fn test_service_rate_limit_metadata_rejects_second_route_run() {
        let source = r#"
module test.service_rate

service BillingApi rate limit 1 per 1m {
    route POST "/charge" {
        audit("charge")
    }
}
"#;
        let compilation = compile("test.num", source);
        let mut runtime = Runtime::new(&compilation.module, vec![]);

        assert!(runtime
            .run_service_route("BillingApi", "POST", "/charge", None)
            .is_ok());
        let res = runtime.run_service_route("BillingApi", "POST", "/charge", None);

        assert!(res.is_err());
        assert!(res.unwrap_err().contains("Rate limit exceeded"));
    }

    #[test]
    fn test_action_timeout_metadata_blocks_timed_out_action() {
        let source = r#"
module test.timeout

connector counters {
    hit() -> Unit
}

action record_once()
    timeout 0ms
    retry 2
{
    counters.hit()
}

workflow main() {
    record_once()
}
"#;
        let compilation = compile("test.num", source);
        let calls = std::rc::Rc::new(std::cell::RefCell::new(0));
        let calls_for_handler = calls.clone();
        let mut registry = StaticConnectorRegistry::new();
        registry.register("counters.hit", move |_| {
            *calls_for_handler.borrow_mut() += 1;
            Ok(Value::Null)
        });
        let mut runtime = Runtime::with_connectors(&compilation.module, vec![], Box::new(registry));
        let res = runtime.run_workflow("main", HashMap::new());

        assert!(res.is_err());
        assert!(res.unwrap_err().contains("Timeout while executing action"));
        assert_eq!(*calls.borrow(), 0);
    }

    #[test]
    fn test_declared_connector_without_implementation_fails() {
        let source = r#"
module test.connector

connector echo {
    text(value: Text) -> Text
}

workflow main() {
    let message: Text = echo.text("hello")
}
"#;
        let compilation = compile("test.num", source);
        let registry = StaticConnectorRegistry::new();
        let mut runtime = Runtime::with_connectors(&compilation.module, vec![], Box::new(registry));
        let res = runtime.run_workflow("main", HashMap::new());

        assert!(res.is_err());
        assert!(res
            .unwrap_err()
            .contains("Connector implementation missing"));
    }

    #[test]
    fn test_runtime_runs_num_test_assertions() {
        let source = r#"
module test.tests

test "truth" {
    let allowed: Bool = true
    assert allowed == true
}
"#;
        let compilation = compile("test.num", source);
        let mut runtime = Runtime::new(&compilation.module, vec![]);

        assert!(runtime.run_test("truth").is_ok());
    }

    #[test]
    fn test_runtime_fails_false_num_test_assertion() {
        let source = r#"
module test.tests

test "falsehood" {
    assert false
}
"#;
        let compilation = compile("test.num", source);
        let mut runtime = Runtime::new(&compilation.module, vec![]);
        let res = runtime.run_test("falsehood");

        assert!(res.is_err());
        assert!(res.unwrap_err().contains("assertion failed"));
    }

    #[test]
    fn test_runtime_skips_policy_expectation_body_after_compile_check() {
        let source = r#"
module test.policy_tests

policy DataSharing {
    deny private UserInput -> ExternalApi
}

test policy "private user input cannot leave" {
    let email: Text from UserInput private = "user@example.com"
    expect_deny {
        external.analytics.send(email)
    }
}
"#;
        let compilation = compile("test.num", source);
        assert!(compilation.diagnostics.is_empty());
        let mut runtime = Runtime::new(&compilation.module, vec![]);

        assert!(runtime.run_test("private user input cannot leave").is_ok());
    }

    #[test]
    fn test_runtime_runs_workflow_expectations() {
        let source = r#"
module test.workflow_tests

permission IssueRefund

workflow succeeds() {
    audit("workflow_succeeded")
    assert true
}

workflow fails_without_permission() {
    require Permission.IssueRefund for current_user
}

test workflow "workflow expectations" {
    expect_workflow_success succeeds()
    expect_audit "workflow_succeeded"
    expect_workflow_failure fails_without_permission()
}
"#;
        let compilation = compile("test.num", source);
        assert!(compilation.diagnostics.is_empty());
        let mut runtime = Runtime::new(&compilation.module, vec![]);

        assert!(runtime.run_test("workflow expectations").is_ok());
    }

    #[test]
    fn test_runtime_records_audit_object_context() {
        let source = r#"
module test.audit_context

workflow succeeds() {
    audit("workflow_succeeded", {
        actor: current_user.id,
        amount: 42
    })
}

test workflow "workflow audit context" {
    expect_workflow_success succeeds()
    expect_audit "workflow_succeeded"
}
"#;
        let compilation = compile("test.num", source);
        assert!(compilation.diagnostics.is_empty());
        let mut runtime = Runtime::new(&compilation.module, vec![]);

        assert!(runtime.run_test("workflow audit context").is_ok());
        let audit_trace = runtime
            .trace_events()
            .iter()
            .find(|event| event.kind == RuntimeTraceKind::AuditLogged)
            .expect("audit trace should be recorded");
        assert_eq!(audit_trace.target, "\"workflow_succeeded\"");
        let detail = audit_trace
            .detail
            .as_ref()
            .expect("audit context should be recorded as trace detail");
        assert!(detail.contains("actor: \"admin@company.com\""));
        assert!(detail.contains("amount: 42"));
    }

    #[test]
    fn test_runtime_accepts_named_human_approval_arguments() {
        let source = r#"
module test.approval_context

workflow approval_needed() {
    require_human_approval(
        action: "issue_refund",
        reason: "Low AI confidence"
    )
    audit("approval_requested")
}

test workflow "approval context" {
    expect_workflow_success approval_needed()
    expect_audit "approval_requested"
}
"#;
        let compilation = compile("test.num", source);
        assert!(compilation.diagnostics.is_empty());
        let mut runtime = Runtime::new(&compilation.module, vec![]);

        assert!(runtime.run_test("approval context").is_ok());
    }

    #[test]
    fn test_runtime_reject_builtin_fails_workflow() {
        let source = r#"
module test.reject

workflow blocked() {
    reject("Refund amount is greater than payment amount")
}

test workflow "reject fails workflow" {
    expect_workflow_failure blocked()
}
"#;
        let compilation = compile("test.num", source);
        assert!(compilation.diagnostics.is_empty());
        let mut runtime = Runtime::new(&compilation.module, vec![]);

        assert!(runtime.run_test("reject fails workflow").is_ok());
    }

    #[test]
    fn test_runtime_runs_ai_mocks() {
        let source = r#"
module test.ai_tests

enum Intent {
    RefundRequest
    BillingQuestion
}

connector ai {
    classify(message: Text) -> Uncertain<Intent>
}

test ai "ai mock controls classification" {
    mock_ai ai.classify("refund") => RefundRequest confidence 0.91
    let intent: Uncertain<Intent> = ai.classify("refund")
    assert intent.confidence == 0.91
    assert intent.value == RefundRequest
}
"#;
        let compilation = compile("test.num", source);
        assert!(compilation.diagnostics.is_empty());
        let mut runtime = Runtime::new(&compilation.module, vec![]);

        assert!(runtime.run_test("ai mock controls classification").is_ok());
    }

    #[test]
    fn test_runtime_runs_connector_mocks_in_workflow_tests() {
        let source = r#"
module test.workflow_fixtures

connector reports {
    render(report_id: Text) -> Text
}

workflow export_report() {
    let rendered: Text = reports.render("r_1")
    assert rendered == "mock report"
}

test workflow "connector mock controls workflow" {
    mock_connector reports.render("r_1") => "mock report"
    expect_workflow_success export_report()
}
"#;
        let compilation = compile("test.num", source);
        assert!(compilation.diagnostics.is_empty());
        let mut runtime = Runtime::new(&compilation.module, vec![]);

        assert!(runtime.run_test("connector mock controls workflow").is_ok());
    }

    #[test]
    fn test_runtime_executes_impl_methods() {
        let source = r#"
module test.methods

type Customer {
    id: Text
}

impl Customer {
    fn get_id() -> Text {
        return self.id
    }
}

workflow main(customer: Customer) {
    let id: Text = customer.get_id()
    assert id == "C-123"
}
"#;
        let compilation = compile("test.num", source);
        assert!(
            compilation.diagnostics.is_empty(),
            "Diagnostics: {:?}",
            compilation.diagnostics
        );
        let mut runtime = Runtime::new(&compilation.module, vec![]);

        let mut fields = HashMap::new();
        fields.insert("id".to_string(), Value::String("C-123".to_string()));
        let customer = Value::Struct("Customer".to_string(), fields);

        let mut args = HashMap::new();
        args.insert("customer".to_string(), customer);

        let res = runtime.run_workflow("main", args);
        assert!(res.is_ok(), "Run workflow failed: {:?}", res);
    }

    #[test]
    fn test_runtime_scope_statement() {
        let source = r#"
module test.scope_stmt

workflow main() {
    let outer = 10
    scope {
        let inner = 20
        assert outer == 10
        assert inner == 20
    }
    assert outer == 10
}
"#;
        let compilation = compile("test.num", source);
        assert!(compilation.diagnostics.is_empty(), "Diagnostics: {:?}", compilation.diagnostics);
        let mut runtime = Runtime::new(&compilation.module, vec![]);
        let res = runtime.run_workflow("main", HashMap::new());
        assert!(res.is_ok(), "Run workflow failed: {:?}", res);
    }

    #[test]
    fn test_runtime_quantity_arithmetic() {
        let source = r#"
module test.quantities

workflow main() {
        let distance: Distance<Kilometer> = 10 km
        let time: Duration<Hour> = 2 h
        let speed: Speed<KilometersPerHour> = distance / time
        assert speed == 5.0 km/h
        
        let distance2: Distance<Kilometer> = speed * time
        assert distance2 == 10.0 km
        
        let time2: Duration<Hour> = distance / speed
        assert time2 == 2.0 h
        
        let sum: Distance<Kilometer> = distance + distance2
        assert sum == 20.0 km
        
        let diff: Distance<Kilometer> = sum - distance
        assert diff == 10.0 km
        
        let ratio: Float = sum / distance
        assert ratio == 2.0
}
"#;
        let compilation = compile("test.num", source);
        assert!(compilation.diagnostics.is_empty(), "Diagnostics: {:?}", compilation.diagnostics);
        let mut runtime = Runtime::new(&compilation.module, vec![]);
        let res = runtime.run_workflow("main", HashMap::new());
        assert!(res.is_ok(), "Run workflow failed: {:?}", res);
    }

    #[test]
    fn test_runtime_transaction_saga_rollback() {
        let source = r#"
module test.saga

connector payment {
    charge(amount: Money<KZT>) -> Unit
    refund(amount: Money<KZT>) -> Unit
}

action make_payment(amount: Money<KZT>)
    rollback cancel_payment(amount)
{
    payment.charge(amount)
}

action cancel_payment(amount: Money<KZT>) {
    payment.refund(amount)
}

workflow main() {
    let fee: Money<KZT> = 15000 KZT
    transaction saga {
        make_payment(fee)
        reject("intentional saga failure")
    }
}
"#;
        let compilation = compile("test.num", source);
        assert!(compilation.diagnostics.is_empty(), "Diagnostics: {:?}", compilation.diagnostics);
        
        let charges = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let charges_handler = charges.clone();
        let refunds = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let refunds_handler = refunds.clone();
        
        let mut registry = StaticConnectorRegistry::new();
        registry.register("payment.charge", move |args| {
            charges_handler.borrow_mut().push(args.first().unwrap().clone());
            Ok(Value::Null)
        });
        registry.register("payment.refund", move |args| {
            refunds_handler.borrow_mut().push(args.first().unwrap().clone());
            Ok(Value::Null)
        });
        
        let mut runtime = Runtime::with_connectors(&compilation.module, vec![], Box::new(registry));
        let res = runtime.run_workflow("main", HashMap::new());
        assert!(res.is_err(), "Saga should fail");
        
        assert_eq!(charges.borrow().len(), 1);
        assert_eq!(refunds.borrow().len(), 1);
        assert_eq!(charges.borrow()[0], Value::Money(1500000, "KZT".to_string()));
        assert_eq!(refunds.borrow()[0], Value::Money(1500000, "KZT".to_string()));
    }
}
