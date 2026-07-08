use crate::ast::*;
use crate::diagnostic::Diagnostic;

pub fn lint(module: &Module) -> Vec<Diagnostic> {
    let mut linter = Linter {
        diagnostics: Vec::new(),
    };
    linter.module(module);
    linter.diagnostics
}

struct Linter {
    diagnostics: Vec<Diagnostic>,
}

impl Linter {
    fn module(&mut self, module: &Module) {
        if module.name.is_none() {
            if let Some(first) = module.declarations.first() {
                self.diagnostics.push(
                    Diagnostic::warning(
                        "N4000",
                        "module has no explicit module path",
                        first.span().clone(),
                    )
                    .with_reason("multi-file and package imports depend on declared module paths")
                    .with_help("add `module package.name` at the top of the file"),
                );
            }
        }

        for decl in &module.declarations {
            match decl {
                Declaration::Function(function) | Declaration::Workflow(function) => {
                    self.callable_params(&function.params);
                }
                Declaration::Action(action) => self.action(action),
                Declaration::Actor(actor) => {
                    for handler in &actor.handlers {
                        self.callable_params(&handler.params);
                    }
                }
                Declaration::Service(service) => self.service(service),
                Declaration::Type(ty) => self.type_decl(ty),
                Declaration::Permission(_)
                | Declaration::Role(_)
                | Declaration::Policy(_)
                | Declaration::Enum(_)
                | Declaration::Connector(_)
                | Declaration::Test(_) => {}
                Declaration::Impl(imp) => {
                    for method in &imp.methods {
                        self.callable_params(&method.params);
                    }
                }
            }
        }
    }

    fn type_decl(&mut self, ty: &TypeDecl) {
        if let TypeBody::Struct(fields) = &ty.body {
            for field in fields {
                self.labels(&field.name, &field.ty, &field.labels, &field.span, "field");
            }
        }
    }

    fn action(&mut self, action: &ActionDecl) {
        self.callable_params(&action.params);

        if action.risk >= Risk::High {
            if action.timeout.is_none() {
                self.diagnostics.push(
                    Diagnostic::warning(
                        "N4001",
                        format!("high-risk action `{}` has no timeout", action.name),
                        action.span.clone(),
                    )
                    .with_reason("external actions should have bounded execution time")
                    .with_help("add `timeout <duration>` to the action signature"),
                );
            }
            if action.cost.is_none() {
                self.diagnostics.push(
                    Diagnostic::warning(
                        "N4002",
                        format!("high-risk action `{}` has no cost metadata", action.name),
                        action.span.clone(),
                    )
                    .with_reason("cost-aware workflows need action cost metadata")
                    .with_help("add `cost <amount> <currency>` to the action signature"),
                );
            }
            if action.idempotency_key.is_none() {
                self.diagnostics.push(
                    Diagnostic::warning(
                        "N4003",
                        format!("high-risk action `{}` has no idempotency key", action.name),
                        action.span.clone(),
                    )
                    .with_reason(
                        "retries and workflow resumes should not repeat real-world effects",
                    )
                    .with_help("add `idempotency key <stable expression>` to the action signature"),
                );
            }
        }
    }

    fn service(&mut self, service: &ServiceDecl) {
        for route in &service.routes {
            if route.requires.is_empty() {
                self.diagnostics.push(
                    Diagnostic::warning(
                        "N4004",
                        format!(
                            "service route `{} {}` has no permission requirement",
                            route.method, route.path
                        ),
                        route.span.clone(),
                    )
                    .with_reason("backend routes should make authorization explicit")
                    .with_help("add `requires Permission.<Name>` to the route"),
                );
            }
            if let Some(input) = &route.input {
                self.labels(
                    &input.name,
                    &input.ty,
                    &input.labels,
                    &input.span,
                    "route input",
                );
            }
        }
    }

    fn callable_params(&mut self, params: &[Param]) {
        for param in params {
            self.labels(
                &param.name,
                &param.ty,
                &param.labels,
                &param.span,
                "parameter",
            );
        }
    }

    fn labels(
        &mut self,
        name: &str,
        ty: &TypeRef,
        labels: &Labels,
        span: &crate::span::Span,
        kind: &str,
    ) {
        if matches!(
            labels.privacy,
            Some(Privacy::Private | Privacy::Sensitive | Privacy::Regulated)
        ) && labels.source.is_none()
        {
            self.diagnostics.push(
                Diagnostic::warning(
                    "N4005",
                    format!("{kind} `{name}` has privacy label without provenance source"),
                    span.clone(),
                )
                .with_reason("privacy checks are stronger when values also declare their origin")
                .with_help("add `from <Source>` next to the privacy label"),
            );
        }

        if ty.is_secret() && labels.privacy != Some(Privacy::Secret) {
            self.diagnostics.push(
                Diagnostic::warning(
                    "N4006",
                    format!("secret {kind} `{name}` is missing `secret` privacy label"),
                    span.clone(),
                )
                .with_reason("Secret<T> values should also carry the explicit secret data label")
                .with_help("add `secret` to the declaration"),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{lexer, parser};

    fn codes(source: &str) -> Vec<&'static str> {
        let lexed = lexer::lex("test.num", source);
        let parsed = parser::parse("test.num", &lexed.tokens);
        lint(&parsed.module)
            .into_iter()
            .map(|diagnostic| diagnostic.code)
            .collect()
    }

    #[test]
    fn warns_for_high_risk_action_missing_operational_metadata() {
        let source = r#"
module tests.lint

action refund()
    risk high
    rollback reverse_refund()
{
    audit("refund")
}
"#;

        let codes = codes(source);

        assert!(codes.contains(&"N4001"));
        assert!(codes.contains(&"N4002"));
        assert!(codes.contains(&"N4003"));
    }

    #[test]
    fn warns_for_route_without_permission() {
        let source = r#"
module tests.lint

service Api {
    route POST "/refunds" {
        audit("refund")
    }
}
"#;

        assert!(codes(source).contains(&"N4004"));
    }

    #[test]
    fn warns_for_sensitive_value_without_source_and_secret_without_label() {
        let source = r#"
module tests.lint

type Credentials {
    token: Secret<Text>
}

workflow main(email: Email private) {
    audit("x")
}
"#;

        let codes = codes(source);

        assert!(codes.contains(&"N4005"));
        assert!(codes.contains(&"N4006"));
    }
}
