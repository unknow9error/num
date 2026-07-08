#[path = "../../generated/num_connectors.rs"]
pub mod num_connectors;

use num_connectors::{EchoConnector, NumConnectorContext, NumConnectorError, NumConnectorResult};

pub struct EchoConnectorFixture;

impl EchoConnector for EchoConnectorFixture {
    fn reply(&self, message: String, context: &NumConnectorContext) -> NumConnectorResult<String> {
        if context.tenant.is_empty() {
            return Err(NumConnectorError::new(
                "missing_tenant",
                "Rust connector context must include tenant",
                false,
            ));
        }
        Ok(format!("rust echo [{}]: {message}", context.request_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::num_connectors::num_invoke_echo_reply;

    fn context() -> NumConnectorContext {
        NumConnectorContext {
            connector: "echo".to_string(),
            method_name: "reply".to_string(),
            method: "echo.reply".to_string(),
            capability: "connector:echo.reply".to_string(),
            actor: "fixture".to_string(),
            tenant: "default".to_string(),
            correlation_id: "corr_fixture".to_string(),
            request_id: "req_fixture".to_string(),
            policy_decision: "compile_time_checked".to_string(),
            arg_labels: Vec::new(),
        }
    }

    #[test]
    fn fixture_implements_generated_contract() {
        let connector = EchoConnectorFixture;
        let response = num_invoke_echo_reply(&connector, "hello".to_string(), &context()).unwrap();

        assert_eq!(response, "rust echo [req_fixture]: hello");
    }

    #[test]
    fn fixture_returns_structured_errors() {
        let connector = EchoConnectorFixture;
        let mut context = context();
        context.tenant.clear();

        let error = num_invoke_echo_reply(&connector, "hello".to_string(), &context).unwrap_err();

        assert_eq!(error.code, "missing_tenant");
        assert_eq!(error.message, "Rust connector context must include tenant");
        assert!(!error.retryable);
    }

    #[test]
    fn generated_invoke_wrapper_maps_panics_to_structured_errors() {
        struct PanickingConnector;

        impl EchoConnector for PanickingConnector {
            fn reply(
                &self,
                _message: String,
                _context: &NumConnectorContext,
            ) -> NumConnectorResult<String> {
                panic!("boom")
            }
        }

        let error = num_invoke_echo_reply(&PanickingConnector, "hello".to_string(), &context())
            .unwrap_err();

        assert_eq!(error.code, "rust_panic");
        assert_eq!(error.message, "rust connector panicked");
        assert!(!error.retryable);
    }
}
