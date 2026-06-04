use crate::ast::*;

pub fn format_module(module: &Module) -> String {
    let mut out = String::new();
    if let Some(name) = &module.name {
        out.push_str("module ");
        out.push_str(name);
        out.push_str("\n\n");
    }

    for import in &module.imports {
        out.push_str("use ");
        out.push_str(&import.path);
        out.push('\n');
    }

    if !module.imports.is_empty() {
        out.push('\n');
    }

    for (index, decl) in module.declarations.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        format_decl(decl, &mut out);
        out.push('\n');
    }

    out
}

fn format_decl(decl: &Declaration, out: &mut String) {
    match decl {
        Declaration::Permission(permission) => {
            out.push_str("permission ");
            out.push_str(&permission.name);
            out.push('\n');
        }
        Declaration::Role(role) => {
            out.push_str("role ");
            out.push_str(&role.name);
            out.push_str(" {\n");
            for permission in &role.allows {
                out.push_str("    allow ");
                out.push_str(permission);
                out.push('\n');
            }
            out.push_str("}\n");
        }
        Declaration::Policy(policy) => {
            out.push_str("policy ");
            out.push_str(&policy.name);
            out.push_str(" {\n");
            for rule in &policy.rules {
                out.push_str("    ");
                out.push_str(match rule.effect {
                    PolicyEffect::Allow => "allow ",
                    PolicyEffect::Deny => "deny ",
                });
                out.push_str(&rule.raw);
                out.push('\n');
            }
            out.push_str("}\n");
        }
        Declaration::Type(ty) => {
            out.push_str("type ");
            out.push_str(&ty.name);
            generic_params(&ty.generic_params, out);
            match &ty.body {
                TypeBody::Struct(fields) => {
                    out.push_str(" {\n");
                    for field in fields {
                        out.push_str("    ");
                        out.push_str(&field.name);
                        out.push_str(": ");
                        out.push_str(&field.ty.raw);
                        format_labels(&field.labels, out);
                        out.push('\n');
                    }
                    out.push_str("}\n");
                }
                TypeBody::Alias(alias) => {
                    out.push_str(" = ");
                    out.push_str(&alias.raw);
                    out.push('\n');
                }
            }
        }
        Declaration::Enum(en) => {
            out.push_str("enum ");
            out.push_str(&en.name);
            out.push_str(" {\n");
            for variant in &en.variants {
                out.push_str("    ");
                out.push_str(&variant.name);
                if let Some(payload) = &variant.payload {
                    out.push('(');
                    out.push_str(&payload.raw);
                    out.push(')');
                }
                out.push('\n');
            }
            out.push_str("}\n");
        }
        Declaration::Function(function) => callable("fn", function, 0, out),
        Declaration::Workflow(workflow) => callable("workflow", workflow, 0, out),
        Declaration::Action(action) => action_decl(action, out),
        Declaration::Connector(connector) => connector_decl(connector, out),
        Declaration::Service(service) => service_decl(service, out),
        Declaration::Test(test) => test_decl(test, out),
        Declaration::Impl(imp) => impl_decl(imp, out),
    }
}

fn generic_params(params: &[String], out: &mut String) {
    if params.is_empty() {
        return;
    }
    out.push('<');
    out.push_str(&params.join(", "));
    out.push('>');
}

fn callable(kind: &str, callable: &CallableDecl, level: usize, out: &mut String) {
    let indent = "    ".repeat(level);
    out.push_str(&indent);
    out.push_str(kind);
    out.push(' ');
    out.push_str(&callable.name);
    params(&callable.params, out);
    if let Some(result) = &callable.result {
        out.push_str(" -> ");
        out.push_str(&result.raw);
    }
    for permission in &callable.requires {
        out.push_str(" requires Permission.");
        out.push_str(permission);
    }
    if let Some(budget) = &callable.budget {
        out.push_str(" budget ");
        out.push_str(budget);
    }
    if let Some(rate_limit) = &callable.rate_limit {
        out.push_str(" rate limit ");
        out.push_str(rate_limit);
    }
    out.push_str(" {\n");
    stmts(&callable.body, level + 1, out);
    out.push_str(&indent);
    out.push_str("}\n");
}

fn impl_decl(imp: &ImplDecl, out: &mut String) {
    out.push_str("impl ");
    out.push_str(&imp.target);
    out.push_str(" {\n");
    for method in &imp.methods {
        callable("fn", method, 1, out);
    }
    out.push_str("}\n");
}

fn action_decl(action: &ActionDecl, out: &mut String) {
    out.push_str("action ");
    out.push_str(&action.name);
    params(&action.params, out);
    if let Some(result) = &action.result {
        out.push_str(" -> ");
        out.push_str(&result.raw);
    }
    for permission in &action.requires {
        out.push_str("\n    requires Permission.");
        out.push_str(permission);
    }
    if action.risk != Risk::Low {
        out.push_str("\n    risk ");
        out.push_str(match action.risk {
            Risk::Low => "low",
            Risk::Medium => "medium",
            Risk::High => "high",
            Risk::Critical => "critical",
        });
    }
    if let Some(timeout) = &action.timeout {
        out.push_str("\n    timeout ");
        out.push_str(timeout);
    }
    if let Some(cost) = &action.cost {
        out.push_str("\n    cost ");
        out.push_str(cost);
    }
    if let Some(retry) = &action.retry {
        out.push_str("\n    retry ");
        out.push_str(retry);
    }
    if let Some(idempotency_key) = &action.idempotency_key {
        out.push_str("\n    idempotency key ");
        out.push_str(&format_call_expr(idempotency_key));
    }
    if let Some(rollback) = &action.rollback {
        out.push_str("\n    rollback ");
        out.push_str(&format_call_expr(rollback));
    }
    out.push_str("\n{\n");
    stmts(&action.body, 1, out);
    out.push_str("}\n");
}

fn connector_decl(connector: &ConnectorDecl, out: &mut String) {
    out.push_str("connector ");
    out.push_str(&connector.name);
    out.push_str(" {\n");
    for method in &connector.methods {
        out.push_str("    ");
        out.push_str(&method.name);
        params(&method.params, out);
        if let Some(result) = &method.result {
            out.push_str(" -> ");
            out.push_str(&result.raw);
        }
        out.push('\n');
    }
    out.push_str("}\n");
}

fn service_decl(service: &ServiceDecl, out: &mut String) {
    out.push_str("service ");
    out.push_str(&service.name);
    if let Some(budget) = &service.budget {
        out.push_str(" budget ");
        out.push_str(budget);
    }
    if let Some(rate_limit) = &service.rate_limit {
        out.push_str(" rate limit ");
        out.push_str(rate_limit);
    }
    out.push_str(" {\n");
    for route in &service.routes {
        out.push_str("    route ");
        out.push_str(&route.method);
        out.push(' ');
        out.push('"');
        out.push_str(&route.path);
        out.push('"');
        for permission in &route.requires {
            out.push_str(" requires Permission.");
            out.push_str(permission);
        }
        out.push_str(" {\n");
        if let Some(input) = &route.input {
            out.push_str("        input ");
            out.push_str(&input.name);
            out.push_str(": ");
            out.push_str(&input.ty.raw);
            format_labels(&input.labels, out);
            out.push('\n');
        }
        stmts(&route.body, 2, out);
        out.push_str("    }\n");
    }
    out.push_str("}\n");
}

fn test_decl(test: &TestDecl, out: &mut String) {
    out.push_str("test ");
    match test.kind {
        TestKind::Unit => {}
        TestKind::Policy => out.push_str("policy "),
        TestKind::Workflow => out.push_str("workflow "),
        TestKind::Ai => out.push_str("ai "),
    }
    out.push('"');
    out.push_str(&test.name);
    out.push_str("\" {\n");
    stmts(&test.body, 1, out);
    out.push_str("}\n");
}

fn params(params: &[Param], out: &mut String) {
    out.push('(');
    for (index, param) in params.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        out.push_str(&param.name);
        out.push_str(": ");
        out.push_str(&param.ty.raw);
        format_labels(&param.labels, out);
    }
    out.push(')');
}

fn format_labels(labels: &Labels, out: &mut String) {
    if let Some(source) = &labels.source {
        out.push_str(" from ");
        out.push_str(source);
    }
    if let Some(trust) = labels.trust {
        out.push(' ');
        out.push_str(match trust {
            Trust::Untrusted => "untrusted",
            Trust::Trusted => "trusted",
            Trust::Verified => "verified",
        });
    }
    if let Some(privacy) = labels.privacy {
        out.push(' ');
        out.push_str(match privacy {
            Privacy::Public => "public",
            Privacy::Internal => "internal",
            Privacy::Private => "private",
            Privacy::Sensitive => "sensitive",
            Privacy::Secret => "secret",
            Privacy::Regulated => "regulated",
        });
    }
}

fn format_call_expr(expr: &str) -> String {
    collapse_expr_whitespace(expr)
        .replace(" . ", ".")
        .replace(". ", ".")
        .replace(" .", ".")
        .replace(" ( ", "(")
        .replace(" (", "(")
        .replace("( ", "(")
        .replace(" )", ")")
        .replace(" , ", ", ")
        .replace(" ,", ",")
        .replace(" : ", ": ")
        .replace(" :", ":")
        .replace("{  ", "{ ")
        .replace(" {", " {")
}

fn collapse_expr_whitespace(expr: &str) -> String {
    let mut out = String::new();
    let mut in_string = false;
    let mut escaped = false;
    let mut pending_space = false;

    for ch in expr.trim().chars() {
        if in_string {
            out.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        if ch == '"' {
            if pending_space && !out.is_empty() {
                out.push(' ');
            }
            pending_space = false;
            in_string = true;
            out.push(ch);
            continue;
        }

        if ch.is_whitespace() {
            pending_space = true;
            continue;
        }

        if pending_space && !out.is_empty() {
            out.push(' ');
        }
        pending_space = false;
        out.push(ch);
    }

    out
}

fn stmts(stmts: &[Stmt], level: usize, out: &mut String) {
    for stmt in stmts {
        let indent = "    ".repeat(level);
        match stmt {
            Stmt::Let(stmt) => {
                out.push_str(&indent);
                out.push_str(if stmt.mutable { "var " } else { "let " });
                out.push_str(&stmt.name);
                if let Some(ty) = &stmt.ty {
                    out.push_str(": ");
                    out.push_str(&ty.raw);
                    format_labels(&stmt.labels, out);
                }
                if let Some(expr) = &stmt.expr {
                    out.push_str(" = ");
                    out.push_str(&format_call_expr(&expr.text));
                }
                out.push('\n');
            }
            Stmt::Assign(stmt) => {
                out.push_str(&indent);
                out.push_str(&stmt.name);
                out.push_str(" = ");
                out.push_str(&format_call_expr(&stmt.expr.text));
                out.push('\n');
            }
            Stmt::Assert(stmt) => {
                out.push_str(&indent);
                out.push_str("assert ");
                out.push_str(&format_call_expr(&stmt.expr.text));
                out.push('\n');
            }
            Stmt::ExpectPolicy(stmt) => {
                out.push_str(&indent);
                out.push_str(match stmt.outcome {
                    PolicyExpectation::Allow => "expect_allow {\n",
                    PolicyExpectation::Deny => "expect_deny {\n",
                });
                self::stmts(&stmt.body, level + 1, out);
                out.push_str(&indent);
                out.push_str("}\n");
            }
            Stmt::ExpectWorkflow(stmt) => {
                out.push_str(&indent);
                out.push_str(match stmt.outcome {
                    WorkflowExpectation::Success => "expect_workflow_success ",
                    WorkflowExpectation::Failure => "expect_workflow_failure ",
                });
                out.push_str(&format_call_expr(&stmt.call.text));
                out.push('\n');
            }
            Stmt::ExpectAudit(stmt) => {
                out.push_str(&indent);
                out.push_str("expect_audit ");
                out.push_str(stmt.event.text.trim());
                out.push('\n');
            }
            Stmt::MockAi(stmt) => {
                out.push_str(&indent);
                out.push_str("mock_ai ");
                out.push_str(&format_call_expr(&stmt.call.text));
                out.push_str(" => ");
                out.push_str(&format_call_expr(&stmt.value.text));
                out.push_str(" confidence ");
                out.push_str(stmt.confidence.text.trim());
                out.push('\n');
            }
            Stmt::MockConnector(stmt) => {
                out.push_str(&indent);
                out.push_str("mock_connector ");
                out.push_str(&format_call_expr(&stmt.call.text));
                out.push_str(" => ");
                out.push_str(&format_call_expr(&stmt.value.text));
                out.push('\n');
            }
            Stmt::Require(stmt) => {
                out.push_str(&indent);
                out.push_str("require Permission.");
                out.push_str(&stmt.permission);
                if let Some(actor) = &stmt.actor {
                    out.push_str(" for ");
                    out.push_str(actor);
                }
                out.push('\n');
            }
            Stmt::Transaction(stmt) => {
                out.push_str(&indent);
                out.push_str(if stmt.saga {
                    "transaction saga {\n"
                } else {
                    "transaction {\n"
                });
                self::stmts(&stmt.body, level + 1, out);
                out.push_str(&indent);
                out.push_str("}\n");
            }
            Stmt::If(stmt) => {
                out.push_str(&indent);
                out.push_str("if ");
                out.push_str(&format_call_expr(&stmt.condition.text));
                out.push_str(" {\n");
                self::stmts(&stmt.then_body, level + 1, out);
                out.push_str(&indent);
                out.push('}');
                if !stmt.else_body.is_empty() {
                    out.push_str(" else {\n");
                    self::stmts(&stmt.else_body, level + 1, out);
                    out.push_str(&indent);
                    out.push('}');
                }
                out.push('\n');
            }
            Stmt::Match(stmt) => {
                out.push_str(&indent);
                out.push_str("match ");
                out.push_str(&format_call_expr(&stmt.expr.text));
                out.push_str(" {\n");
                for arm in &stmt.arms {
                    out.push_str(&"    ".repeat(level + 1));
                    match &arm.pattern {
                        MatchPattern::Variant {
                            name,
                            payload,
                            bindings,
                        } => {
                            out.push_str(name);
                            if let Some(payload) = payload {
                                out.push('(');
                                out.push_str(payload);
                                out.push(')');
                            }
                            if !bindings.is_empty() {
                                out.push_str(" { ");
                                for (index, binding) in bindings.iter().enumerate() {
                                    if index > 0 {
                                        out.push_str(", ");
                                    }
                                    format_match_binding(binding, out);
                                }
                                out.push_str(" }");
                            }
                        }
                        MatchPattern::Wildcard => out.push('_'),
                    }
                    if let Some(guard) = &arm.guard {
                        out.push_str(" if ");
                        out.push_str(&format_call_expr(&guard.text));
                    }
                    out.push_str(" => {\n");
                    self::stmts(&arm.body, level + 2, out);
                    out.push_str(&"    ".repeat(level + 1));
                    out.push_str("}\n");
                }
                out.push_str(&indent);
                out.push_str("}\n");
            }
            Stmt::Return(expr) => {
                out.push_str(&indent);
                out.push_str("return");
                let formatted = format_call_expr(&expr.text);
                if !formatted.is_empty() {
                    out.push(' ');
                    out.push_str(&formatted);
                }
                out.push('\n');
            }
            Stmt::Expr(expr) => {
                out.push_str(&indent);
                out.push_str(&format_call_expr(&expr.text));
                out.push('\n');
            }
            Stmt::Scope(stmt) => {
                out.push_str(&indent);
                out.push_str("scope {\n");
                self::stmts(&stmt.body, level + 1, out);
                out.push_str(&indent);
                out.push_str("}\n");
            }
        }
    }
}

fn format_match_binding(binding: &MatchBinding, out: &mut String) {
    out.push_str(&binding.field);
    if let Some(nested_type) = &binding.nested_type {
        out.push_str(": ");
        out.push_str(nested_type);
        out.push_str(" { ");
        for (index, nested) in binding.nested.iter().enumerate() {
            if index > 0 {
                out.push_str(", ");
            }
            format_match_binding(nested, out);
        }
        out.push_str(" }");
    } else if binding.name != binding.field {
        out.push_str(": ");
        out.push_str(&binding.name);
    }
}
