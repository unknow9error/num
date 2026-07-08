use crate::interpreter::Value;
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use std::sync::Arc;

pub type ConnectorHandler = Box<dyn Fn(&[Value]) -> Result<Value, ConnectorError>>;
pub type ConnectorContextHandler =
    Box<dyn Fn(&ConnectorCallContext, &[Value]) -> Result<Value, ConnectorError>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectorError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

impl ConnectorError {
    pub fn new(code: impl Into<String>, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retryable,
        }
    }

    pub fn execution(message: impl Into<String>) -> Self {
        Self::new("execution_failed", message, false)
    }

    pub fn missing_implementation(method: &str) -> Self {
        Self::new(
            "missing_implementation",
            format!("Connector implementation missing for declared connector method '{method}'"),
            false,
        )
    }
}

impl From<String> for ConnectorError {
    fn from(message: String) -> Self {
        Self::execution(message)
    }
}

impl From<&str> for ConnectorError {
    fn from(message: &str) -> Self {
        Self::execution(message)
    }
}

pub trait ConnectorExecutor {
    fn call(&self, name: &str, args: &[Value]) -> Option<Result<Value, ConnectorError>>;

    fn call_with_context(
        &self,
        context: &ConnectorCallContext,
        args: &[Value],
    ) -> Option<Result<Value, ConnectorError>> {
        self.call(&context.method, args)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectorArgLabel {
    pub index: usize,
    pub name: String,
    pub ty: String,
    pub source: Option<String>,
    pub privacy: Option<String>,
    pub trust: Option<String>,
}

impl ConnectorArgLabel {
    pub fn to_json(&self) -> JsonValue {
        json!({
            "index": self.index,
            "name": self.name,
            "type": self.ty,
            "source": self.source,
            "privacy": self.privacy,
            "trust": self.trust,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectorCallContext {
    pub connector: String,
    pub method_name: String,
    pub method: String,
    pub capability: String,
    pub actor: String,
    pub tenant: String,
    pub correlation_id: String,
    pub request_id: String,
    pub policy_decision: String,
    pub arg_labels: Vec<ConnectorArgLabel>,
}

impl ConnectorCallContext {
    pub fn for_method(method: &str) -> Self {
        let (connector, method_name) = method
            .split_once('.')
            .map(|(connector, method)| (connector.to_string(), method.to_string()))
            .unwrap_or_else(|| ("external".to_string(), method.to_string()));
        Self {
            connector,
            method_name,
            method: method.to_string(),
            capability: format!("connector:{method}"),
            actor: "system".to_string(),
            tenant: "default".to_string(),
            correlation_id: "corr_static_connector".to_string(),
            request_id: "req_static_connector".to_string(),
            policy_decision: "runtime_unlabeled".to_string(),
            arg_labels: Vec::new(),
        }
    }

    pub fn to_json(&self) -> JsonValue {
        json!({
            "connector": self.connector,
            "method_name": self.method_name,
            "method": self.method,
            "capability": self.capability,
            "actor": self.actor,
            "tenant": self.tenant,
            "correlation_id": self.correlation_id,
            "request_id": self.request_id,
            "policy_decision": self.policy_decision,
            "arg_labels": self
                .arg_labels
                .iter()
                .map(ConnectorArgLabel::to_json)
                .collect::<Vec<_>>(),
        })
    }
}

pub struct ChainedConnectorExecutor {
    executors: Vec<Box<dyn ConnectorExecutor>>,
}

impl ChainedConnectorExecutor {
    pub fn new(executors: Vec<Box<dyn ConnectorExecutor>>) -> Self {
        Self { executors }
    }
}

impl ConnectorExecutor for ChainedConnectorExecutor {
    fn call(&self, name: &str, args: &[Value]) -> Option<Result<Value, ConnectorError>> {
        self.executors
            .iter()
            .find_map(|executor| executor.call(name, args))
    }

    fn call_with_context(
        &self,
        context: &ConnectorCallContext,
        args: &[Value],
    ) -> Option<Result<Value, ConnectorError>> {
        self.executors
            .iter()
            .find_map(|executor| executor.call_with_context(context, args))
    }
}

impl<T: ConnectorExecutor + ?Sized> ConnectorExecutor for Arc<T> {
    fn call(&self, name: &str, args: &[Value]) -> Option<Result<Value, ConnectorError>> {
        self.as_ref().call(name, args)
    }

    fn call_with_context(
        &self,
        context: &ConnectorCallContext,
        args: &[Value],
    ) -> Option<Result<Value, ConnectorError>> {
        self.as_ref().call_with_context(context, args)
    }
}

#[derive(Default)]
pub struct StaticConnectorRegistry {
    handlers: HashMap<String, ConnectorContextHandler>,
}

impl StaticConnectorRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<F>(&mut self, name: impl Into<String>, handler: F)
    where
        F: Fn(&[Value]) -> Result<Value, String> + 'static,
    {
        self.register_with_context(name, move |_context, args| {
            handler(args).map_err(ConnectorError::execution)
        });
    }

    pub fn register_with_context<F>(&mut self, name: impl Into<String>, handler: F)
    where
        F: Fn(&ConnectorCallContext, &[Value]) -> Result<Value, ConnectorError> + 'static,
    {
        self.handlers.insert(name.into(), Box::new(handler));
    }

    pub fn registered_methods(&self) -> Vec<String> {
        let mut methods = self.handlers.keys().cloned().collect::<Vec<_>>();
        methods.sort();
        methods
    }
}

impl ConnectorExecutor for StaticConnectorRegistry {
    fn call(&self, name: &str, args: &[Value]) -> Option<Result<Value, ConnectorError>> {
        self.handlers.get(name).map(|handler| {
            let context = ConnectorCallContext::for_method(name);
            handler(&context, args)
        })
    }

    fn call_with_context(
        &self,
        context: &ConnectorCallContext,
        args: &[Value],
    ) -> Option<Result<Value, ConnectorError>> {
        self.handlers
            .get(&context.method)
            .map(|handler| handler(context, args))
    }
}

#[derive(Debug, Clone)]
pub struct DemoConnectorExecutor {
    output_enabled: bool,
}

impl Default for DemoConnectorExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl DemoConnectorExecutor {
    pub fn new() -> Self {
        Self {
            output_enabled: true,
        }
    }

    pub fn silent() -> Self {
        Self {
            output_enabled: false,
        }
    }

    fn log(&self, message: impl AsRef<str>) {
        if self.output_enabled {
            println!("{}", message.as_ref());
        }
    }
}

impl ConnectorExecutor for DemoConnectorExecutor {
    fn call(&self, name: &str, args: &[Value]) -> Option<Result<Value, ConnectorError>> {
        match name {
            "payments.find" => Some(Ok(find_payment(args, self))),
            "ai.assess_refund_risk" => Some(Ok(assess_refund_risk(self))),
            "ai.classify" => Some(Ok(classify_intent(self))),
            "mailer.send" => Some(Ok(send_mail(args, self))),
            "external.analytics.send" => Some(Ok(send_analytics(args, self))),
            "reports.render_public" => Some(Ok(render_public_report(self))),
            "payment_gateway.refund" => Some(Ok(refund_payment(args, self))),
            "support_queue.assign" => Some(Ok(assign_support_ticket(args, self))),
            _ => None,
        }
    }
}

fn find_payment(args: &[Value], executor: &DemoConnectorExecutor) -> Value {
    let id_str = args.first().cloned().unwrap_or(Value::Null).to_string();
    let mut fields = HashMap::new();
    fields.insert("id".to_string(), Value::String(id_str));
    fields.insert("amount".to_string(), Value::Money(15000, "KZT".to_string()));
    fields.insert(
        "customer_email".to_string(),
        Value::String("customer@example.com".to_string()),
    );
    executor.log("    [MOCK DB] payments.find -> Payment customer_email: customer@example.com");
    Value::Struct("Payment".to_string(), fields)
}

fn assess_refund_risk(executor: &DemoConnectorExecutor) -> Value {
    executor.log("    [MOCK AI] ai.assess_refund_risk -> RiskLevel::Low with 92% confidence");
    Value::Uncertain(
        Box::new(Value::Enum(
            "RiskLevel".to_string(),
            "Low".to_string(),
            None,
        )),
        0.92,
    )
}

fn classify_intent(executor: &DemoConnectorExecutor) -> Value {
    executor.log("    [MOCK AI] ai.classify -> Intent::RefundRequest with 88% confidence");
    Value::Uncertain(
        Box::new(Value::Enum(
            "Intent".to_string(),
            "RefundRequest".to_string(),
            None,
        )),
        0.88,
    )
}

fn send_mail(args: &[Value], executor: &DemoConnectorExecutor) -> Value {
    let email = args.first().cloned().unwrap_or(Value::Null);
    executor.log(format!("    [MAILER] Sent notification to: {email}"));
    Value::Null
}

fn send_analytics(args: &[Value], executor: &DemoConnectorExecutor) -> Value {
    let payload = args.first().cloned().unwrap_or(Value::Null);
    executor.log(format!("    [EXTERNAL API] analytics sent: {payload}"));
    Value::Null
}

fn render_public_report(executor: &DemoConnectorExecutor) -> Value {
    executor.log("    [MOCK DB] reports.render_public -> rendered successfully");
    Value::String("Public Report #42".to_string())
}

fn refund_payment(args: &[Value], executor: &DemoConnectorExecutor) -> Value {
    let id = args.first().cloned().unwrap_or(Value::Null);
    let amount = args.get(1).cloned().unwrap_or(Value::Null);
    executor.log(format!(
        "    [PAYMENT GATEWAY] Processing refund of {} on payment {}",
        amount, id
    ));
    Value::Null
}

fn assign_support_ticket(args: &[Value], executor: &DemoConnectorExecutor) -> Value {
    let ticket_id = args.first().cloned().unwrap_or(Value::Null);
    executor.log(format!("    [SUPPORT QUEUE] Assigned ticket: {ticket_id}"));
    Value::Null
}

#[cfg(test)]
mod tests {
    use super::{
        ChainedConnectorExecutor, ConnectorCallContext, ConnectorError, ConnectorExecutor,
        DemoConnectorExecutor, StaticConnectorRegistry,
    };
    use crate::interpreter::Value;

    #[test]
    fn demo_registry_returns_payment() {
        let registry = DemoConnectorExecutor::new();
        let result = registry
            .call("payments.find", &[Value::String("pay_1".to_string())])
            .unwrap()
            .unwrap();

        let Value::Struct(name, fields) = result else {
            panic!("expected payment struct");
        };
        assert_eq!(name, "Payment");
        assert_eq!(
            fields.get("customer_email"),
            Some(&Value::String("customer@example.com".to_string()))
        );
    }

    #[test]
    fn demo_registry_ignores_unknown_calls() {
        let registry = DemoConnectorExecutor::new();
        assert!(registry.call("unknown.call", &[]).is_none());
    }

    #[test]
    fn static_registry_dispatches_registered_handler() {
        let mut registry = StaticConnectorRegistry::new();
        registry.register("echo.text", |args| {
            Ok(args.first().cloned().unwrap_or(Value::Null))
        });

        let result = registry
            .call("echo.text", &[Value::String("hello".to_string())])
            .unwrap()
            .unwrap();

        assert_eq!(result, Value::String("hello".to_string()));
    }

    #[test]
    fn static_registry_lists_registered_methods() {
        let mut registry = StaticConnectorRegistry::new();
        registry.register("z.last", |_| Ok(Value::Null));
        registry.register("a.first", |_| Ok(Value::Null));

        assert_eq!(
            registry.registered_methods(),
            vec!["a.first".to_string(), "z.last".to_string()]
        );
    }

    #[test]
    fn static_registry_passes_context_to_hosted_handler() {
        let mut registry = StaticConnectorRegistry::new();
        registry.register_with_context("echo.text", |context, args| {
            assert_eq!(context.connector, "echo");
            assert_eq!(context.method_name, "text");
            assert_eq!(context.method, "echo.text");
            assert_eq!(context.capability, "connector:echo.text");
            assert_eq!(context.tenant, "tenant_a");
            Ok(args.first().cloned().unwrap_or(Value::Null))
        });
        let context = ConnectorCallContext {
            connector: "echo".to_string(),
            method_name: "text".to_string(),
            method: "echo.text".to_string(),
            capability: "connector:echo.text".to_string(),
            actor: "actor_a".to_string(),
            tenant: "tenant_a".to_string(),
            correlation_id: "corr_a".to_string(),
            request_id: "req_a".to_string(),
            policy_decision: "compile_time_checked".to_string(),
            arg_labels: Vec::new(),
        };

        let result = registry
            .call_with_context(&context, &[Value::String("hello".to_string())])
            .unwrap()
            .unwrap();

        assert_eq!(result, Value::String("hello".to_string()));
    }

    #[test]
    fn static_registry_preserves_structured_connector_errors() {
        let mut registry = StaticConnectorRegistry::new();
        registry.register_with_context("echo.text", |_context, _args| {
            Err(ConnectorError::new(
                "upstream_unavailable",
                "echo host unavailable",
                true,
            ))
        });

        let error = registry
            .call("echo.text", &[Value::String("hello".to_string())])
            .unwrap()
            .unwrap_err();

        assert_eq!(error.code, "upstream_unavailable");
        assert_eq!(error.message, "echo host unavailable");
        assert!(error.retryable);
    }

    #[test]
    fn chained_registry_uses_first_executor_that_handles_call() {
        let first = StaticConnectorRegistry::new();
        let mut second = StaticConnectorRegistry::new();
        second.register("echo.text", |args| {
            Ok(args.first().cloned().unwrap_or(Value::Null))
        });
        let registry = ChainedConnectorExecutor::new(vec![Box::new(first), Box::new(second)]);

        let result = registry
            .call("echo.text", &[Value::String("hello".to_string())])
            .unwrap()
            .unwrap();

        assert_eq!(result, Value::String("hello".to_string()));
    }
}
