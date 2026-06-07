use crate::interpreter::Value;
use std::collections::HashMap;
use std::sync::Arc;

pub type ConnectorHandler = Box<dyn Fn(&[Value]) -> Result<Value, ConnectorError>>;

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
}

impl<T: ConnectorExecutor + ?Sized> ConnectorExecutor for Arc<T> {
    fn call(&self, name: &str, args: &[Value]) -> Option<Result<Value, ConnectorError>> {
        self.as_ref().call(name, args)
    }
}

#[derive(Default)]
pub struct StaticConnectorRegistry {
    handlers: HashMap<String, ConnectorHandler>,
}

impl StaticConnectorRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<F>(&mut self, name: impl Into<String>, handler: F)
    where
        F: Fn(&[Value]) -> Result<Value, String> + 'static,
    {
        self.handlers.insert(
            name.into(),
            Box::new(move |args| handler(args).map_err(ConnectorError::execution)),
        );
    }
}

impl ConnectorExecutor for StaticConnectorRegistry {
    fn call(&self, name: &str, args: &[Value]) -> Option<Result<Value, ConnectorError>> {
        self.handlers.get(name).map(|handler| handler(args))
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
        ChainedConnectorExecutor, ConnectorExecutor, DemoConnectorExecutor, StaticConnectorRegistry,
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
