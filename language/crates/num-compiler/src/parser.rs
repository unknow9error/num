use crate::ast::*;
use crate::diagnostic::Diagnostic;
use crate::token::{Keyword, Symbol, Token, TokenKind};

#[derive(Debug, Clone)]
pub struct Parsed {
    pub module: Module,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn parse(source_name: &str, tokens: &[Token]) -> Parsed {
    Parser::new(source_name, tokens).parse()
}

struct Parser<'a> {
    tokens: &'a [Token],
    index: usize,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Parser<'a> {
    fn new(_source_name: &'a str, tokens: &'a [Token]) -> Self {
        Self {
            tokens,
            index: 0,
            diagnostics: Vec::new(),
        }
    }

    fn parse(mut self) -> Parsed {
        let mut module = Module::default();
        self.skip_newlines();

        while !self.at(TokenKindRef::Eof) {
            self.skip_newlines();
            if self.at(TokenKindRef::Eof) {
                break;
            }

            if self.match_keyword(Keyword::Module) {
                module.name = self.parse_path_until_line();
                continue;
            }

            if self.match_keyword(Keyword::Use) {
                let span = self.previous().span.clone();
                if let Some(path) = self.parse_path_until_line() {
                    module.imports.push(Import { path, span });
                }
                continue;
            }

            match self.declaration() {
                Some(decl) => module.declarations.push(decl),
                None => self.synchronize_top_level(),
            }
        }

        Parsed {
            module,
            diagnostics: self.diagnostics,
        }
    }

    fn declaration(&mut self) -> Option<Declaration> {
        if self.match_keyword(Keyword::Permission) {
            return self.named_line_decl().map(Declaration::Permission);
        }
        if self.match_keyword(Keyword::Role) {
            return self.role_decl().map(Declaration::Role);
        }
        if self.match_keyword(Keyword::Policy) {
            return self.policy_decl().map(Declaration::Policy);
        }
        if self.match_keyword(Keyword::Type) {
            return self.type_decl().map(Declaration::Type);
        }
        if self.match_keyword(Keyword::Enum) {
            return self.enum_decl().map(Declaration::Enum);
        }
        if self.match_keyword(Keyword::Fn) {
            return self.callable_decl(false).map(Declaration::Function);
        }
        if self.match_keyword(Keyword::Workflow) {
            return self.callable_decl(false).map(Declaration::Workflow);
        }
        if self.match_keyword(Keyword::Action) {
            return self.action_decl().map(Declaration::Action);
        }
        if self.match_keyword(Keyword::Connector) {
            return self.connector_decl().map(Declaration::Connector);
        }
        if self.match_keyword(Keyword::Service) {
            return self.service_decl().map(Declaration::Service);
        }
        if self.match_keyword(Keyword::Test) {
            return self.test_decl().map(Declaration::Test);
        }
        if self.match_keyword(Keyword::Impl) {
            return self.impl_decl().map(Declaration::Impl);
        }

        let token = self.peek().clone();
        self.diagnostics.push(
            Diagnostic::error(
                "N0100",
                format!("expected top-level declaration, found `{}`", token.lexeme),
                token.span,
            )
            .with_help("use module, use, permission, role, policy, type, enum, fn, workflow, action, connector, service, or test"),
        );
        None
    }

    fn named_line_decl(&mut self) -> Option<PermissionDecl> {
        let start = self.previous().span.clone();
        let name = self.expect_ident("expected permission name")?;
        self.consume_line();
        Some(PermissionDecl { name, span: start })
    }

    fn role_decl(&mut self) -> Option<RoleDecl> {
        let span = self.previous().span.clone();
        let name = self.expect_ident("expected role name")?;
        let mut allows = Vec::new();
        let mut recovered_missing_close = false;
        self.expect_symbol(Symbol::LBrace, "expected `{` after role name")?;
        while !self.at_symbol(Symbol::RBrace) && !self.at(TokenKindRef::Eof) {
            self.skip_newlines();
            if self.at_symbol(Symbol::RBrace) {
                break;
            }
            if self.break_on_top_level_keyword_in_block("role") {
                recovered_missing_close = true;
                break;
            }
            if self.match_keyword(Keyword::Allow) {
                if let Some(allowed) = self.expect_ident("expected permission after allow") {
                    allows.push(allowed);
                }
            } else {
                self.advance();
            }
        }
        if !recovered_missing_close {
            self.expect_symbol(Symbol::RBrace, "expected `}` after role body")?;
        }
        Some(RoleDecl { name, allows, span })
    }

    fn policy_decl(&mut self) -> Option<PolicyDecl> {
        let span = self.previous().span.clone();
        let name = self.expect_ident("expected policy name")?;
        let mut rules = Vec::new();
        let mut recovered_missing_close = false;
        self.expect_symbol(Symbol::LBrace, "expected `{` after policy name")?;
        while !self.at_symbol(Symbol::RBrace) && !self.at(TokenKindRef::Eof) {
            self.skip_newlines();
            if self.at_symbol(Symbol::RBrace) {
                break;
            }
            if self.break_on_top_level_keyword_in_block("policy") {
                recovered_missing_close = true;
                break;
            }
            let effect = if self.match_keyword(Keyword::Allow) {
                PolicyEffect::Allow
            } else if self.match_keyword(Keyword::Deny) {
                PolicyEffect::Deny
            } else {
                self.advance();
                continue;
            };

            let rule_span = self.previous().span.clone();
            let raw = self.collect_until_line_or(Symbol::RBrace);
            let privacy = privacy_from_text(&raw);
            let trust = trust_from_text(&raw);
            let source = source_from_rule(&raw);
            let target = target_from_rule(&raw);
            let tenant = tenant_from_rule(&raw);
            let route = route_from_rule(&raw);
            rules.push(PolicyRule {
                effect,
                privacy,
                trust,
                source,
                target,
                tenant,
                route,
                raw,
                span: rule_span,
            });
        }
        if !recovered_missing_close {
            self.expect_symbol(Symbol::RBrace, "expected `}` after policy body")?;
        }
        Some(PolicyDecl { name, rules, span })
    }

    fn type_decl(&mut self) -> Option<TypeDecl> {
        let span = self.previous().span.clone();
        let name = self.expect_ident("expected type name")?;
        let generic_params = self.generic_params();

        if self.match_symbol(Symbol::Eq) {
            let alias = self.collect_type_ref();
            self.consume_line();
            return Some(TypeDecl {
                name,
                generic_params,
                body: TypeBody::Alias(alias),
                span,
            });
        }

        self.skip_until_symbol(Symbol::LBrace);
        self.expect_symbol(Symbol::LBrace, "expected `{` after type name")?;
        let mut fields = Vec::new();
        let mut recovered_missing_close = false;

        while !self.at_symbol(Symbol::RBrace) && !self.at(TokenKindRef::Eof) {
            self.skip_newlines();
            if self.at_symbol(Symbol::RBrace) {
                break;
            }
            if self.break_on_top_level_keyword_in_block("type") {
                recovered_missing_close = true;
                break;
            }
            let field_span = self.peek().span.clone();
            let Some(field_name) = self.expect_ident("expected field name") else {
                self.advance();
                continue;
            };
            self.expect_symbol(Symbol::Colon, "expected `:` after field name")?;
            let ty = self.collect_type_ref();
            let labels = self.collect_labels_until_line_or(Symbol::RBrace);
            fields.push(Field {
                name: field_name,
                ty,
                labels,
                span: field_span,
            });
            self.consume_line();
        }
        if !recovered_missing_close {
            self.expect_symbol(Symbol::RBrace, "expected `}` after type body")?;
        }
        Some(TypeDecl {
            name,
            generic_params,
            body: TypeBody::Struct(fields),
            span,
        })
    }

    fn impl_decl(&mut self) -> Option<ImplDecl> {
        let span = self.previous().span.clone();
        let target = self.expect_ident("expected target type name after impl")?;
        self.expect_symbol(Symbol::LBrace, "expected `{` after target type name")?;
        let mut methods = Vec::new();
        let mut recovered_missing_close = false;
        while !self.at_symbol(Symbol::RBrace) && !self.at(TokenKindRef::Eof) {
            self.skip_newlines();
            if self.at_symbol(Symbol::RBrace) {
                break;
            }
            if self.break_on_top_level_keyword_in_block("impl") {
                recovered_missing_close = true;
                break;
            }
            if self.match_keyword(Keyword::Fn) {
                if let Some(method) = self.callable_decl(false) {
                    methods.push(method);
                }
            } else {
                let token = self.peek().clone();
                self.diagnostics.push(
                    Diagnostic::error(
                        "N0101",
                        "expected method declaration inside impl block",
                        token.span,
                    )
                    .with_help("declare methods as `fn name(...) -> Type { ... }`"),
                );
                self.advance();
            }
        }
        if !recovered_missing_close {
            self.expect_symbol(Symbol::RBrace, "expected `}` after impl body")?;
        }
        Some(ImplDecl {
            target,
            methods,
            span,
        })
    }

    fn generic_params(&mut self) -> Vec<String> {
        if !self.match_symbol(Symbol::Lt) {
            return Vec::new();
        }

        let mut params = Vec::new();
        while !self.at_symbol(Symbol::Gt) && !self.at(TokenKindRef::Eof) {
            if let Some(param) = self.expect_ident("expected generic parameter name") {
                params.push(param);
            } else {
                self.advance();
            }

            if !self.match_symbol(Symbol::Comma) {
                break;
            }
        }
        self.expect_symbol(Symbol::Gt, "expected `>` after generic parameter list");
        params
    }

    fn enum_decl(&mut self) -> Option<EnumDecl> {
        let span = self.previous().span.clone();
        let name = self.expect_ident("expected enum name")?;
        self.expect_symbol(Symbol::LBrace, "expected `{` after enum name")?;
        let mut variants = Vec::new();
        let mut recovered_missing_close = false;
        while !self.at_symbol(Symbol::RBrace) && !self.at(TokenKindRef::Eof) {
            self.skip_newlines();
            if self.at_symbol(Symbol::RBrace) {
                break;
            }
            if self.break_on_top_level_keyword_in_block("enum") {
                recovered_missing_close = true;
                break;
            }
            let variant_span = self.peek().span.clone();
            if let Some(variant_name) = self.maybe_ident() {
                let payload = if self.match_symbol(Symbol::LParen) {
                    let ty = self.collect_type_ref();
                    self.expect_symbol(Symbol::RParen, "expected `)` after enum variant payload");
                    Some(ty)
                } else {
                    None
                };
                variants.push(EnumVariant {
                    name: variant_name,
                    payload,
                    span: variant_span,
                });
            } else {
                self.advance();
            }
        }
        if !recovered_missing_close {
            self.expect_symbol(Symbol::RBrace, "expected `}` after enum body")?;
        }
        Some(EnumDecl {
            name,
            variants,
            span,
        })
    }

    fn callable_decl(&mut self, action_signature: bool) -> Option<CallableDecl> {
        let span = self.previous().span.clone();
        let name = self.expect_ident("expected callable name")?;
        let params = self.params()?;
        let result = self.result_type();
        let mut requires = Vec::new();
        let mut budget = None;
        let mut rate_limit = None;
        while !self.at_symbol(Symbol::LBrace) && !self.at(TokenKindRef::Eof) {
            self.skip_newlines();
            if self.match_keyword(Keyword::Requires) {
                if let Some(permission) = self.permission_path() {
                    requires.push(permission);
                }
                continue;
            }
            if self.match_keyword(Keyword::Budget) {
                budget = Some(self.collect_until_metadata_or(Symbol::LBrace));
                continue;
            }
            if self.match_keyword(Keyword::Rate) {
                self.match_ident_text("limit");
                rate_limit = Some(compact_duration_units(
                    &self.collect_until_metadata_or(Symbol::LBrace),
                ));
                continue;
            }
            if action_signature && self.at_keyword(Keyword::Risk) {
                break;
            }
            self.advance();
        }
        self.expect_symbol(Symbol::LBrace, "expected callable body")?;
        let body = self.block_body();
        Some(CallableDecl {
            name,
            params,
            result,
            requires,
            budget,
            rate_limit,
            body,
            span,
        })
    }

    fn action_decl(&mut self) -> Option<ActionDecl> {
        let span = self.previous().span.clone();
        let name = self.expect_ident("expected action name")?;
        let params = self.params()?;
        let result = self.result_type();
        let mut requires = Vec::new();
        let mut risk = Risk::Low;
        let mut rollback = None;
        let mut timeout = None;
        let mut cost = None;
        let mut retry = None;
        let mut idempotency_key = None;

        while !self.at_symbol(Symbol::LBrace) && !self.at(TokenKindRef::Eof) {
            if self.match_keyword(Keyword::Requires) {
                if let Some(permission) = self.permission_path() {
                    requires.push(permission);
                }
            } else if self.match_keyword(Keyword::Risk) {
                risk = self.risk();
            } else if self.match_keyword(Keyword::Rollback) {
                rollback = Some(self.collect_until_line_or(Symbol::LBrace));
            } else if self.match_keyword(Keyword::Timeout) {
                timeout = Some(self.collect_until_line_or(Symbol::LBrace));
            } else if self.match_keyword(Keyword::Cost) {
                cost = Some(self.collect_until_line_or(Symbol::LBrace));
            } else if self.match_keyword(Keyword::Retry) {
                retry = Some(self.collect_until_line_or(Symbol::LBrace));
            } else if self.match_keyword(Keyword::Idempotency) {
                self.match_ident_text("key");
                idempotency_key = Some(self.collect_until_line_or(Symbol::LBrace));
            } else {
                self.advance();
            }
        }

        self.expect_symbol(Symbol::LBrace, "expected action body")?;
        let body = self.block_body();
        Some(ActionDecl {
            name,
            params,
            result,
            requires,
            risk,
            rollback,
            timeout,
            cost,
            retry,
            idempotency_key,
            body,
            span,
        })
    }

    fn connector_decl(&mut self) -> Option<ConnectorDecl> {
        let span = self.previous().span.clone();
        let name = self.expect_ident("expected connector name")?;
        let mut methods = Vec::new();
        let mut recovered_missing_close = false;
        self.expect_symbol(Symbol::LBrace, "expected connector body")?;

        while !self.at_symbol(Symbol::RBrace) && !self.at(TokenKindRef::Eof) {
            self.skip_newlines();
            if self.at_symbol(Symbol::RBrace) {
                break;
            }
            if self.break_on_top_level_keyword_in_block("connector") {
                recovered_missing_close = true;
                break;
            }

            let method_span = self.peek().span.clone();
            let Some(method_name) = self.expect_ident("expected connector method name") else {
                self.consume_line();
                continue;
            };
            let Some(params) = self.params() else {
                self.consume_line();
                continue;
            };
            let result = self.result_type();
            self.consume_line();
            methods.push(ConnectorMethod {
                name: method_name,
                params,
                result,
                span: method_span,
            });
        }

        if !recovered_missing_close {
            self.expect_symbol(Symbol::RBrace, "expected `}` after connector body")?;
        }

        Some(ConnectorDecl {
            name,
            methods,
            span,
        })
    }

    fn service_decl(&mut self) -> Option<ServiceDecl> {
        let span = self.previous().span.clone();
        let name = self.expect_ident("expected service name")?;
        let mut budget = None;
        let mut rate_limit = None;
        while !self.at_symbol(Symbol::LBrace) && !self.at(TokenKindRef::Eof) {
            self.skip_newlines();
            if self.match_keyword(Keyword::Budget) {
                budget = Some(self.collect_until_metadata_or(Symbol::LBrace));
            } else if self.match_keyword(Keyword::Rate) {
                self.match_ident_text("limit");
                rate_limit = Some(compact_duration_units(
                    &self.collect_until_metadata_or(Symbol::LBrace),
                ));
            } else {
                self.advance();
            }
        }
        self.expect_symbol(Symbol::LBrace, "expected service body")?;
        let mut routes = Vec::new();
        let mut recovered_missing_close = false;

        while !self.at_symbol(Symbol::RBrace) && !self.at(TokenKindRef::Eof) {
            self.skip_newlines();
            if self.at_symbol(Symbol::RBrace) {
                break;
            }
            if self.break_on_top_level_keyword_in_block("service") {
                recovered_missing_close = true;
                break;
            }
            if self.match_ident_text("route") {
                if let Some(route) = self.service_route() {
                    routes.push(route);
                } else {
                    self.consume_line();
                }
            } else {
                let token = self.peek().clone();
                self.diagnostics.push(
                    Diagnostic::error("N0101", "expected service route declaration", token.span)
                        .with_help("declare routes as `route METHOD \"path\" { ... }`"),
                );
                self.consume_line();
            }
        }

        if !recovered_missing_close {
            self.expect_symbol(Symbol::RBrace, "expected `}` after service body")?;
        }

        Some(ServiceDecl {
            name,
            budget,
            rate_limit,
            routes,
            span,
        })
    }

    fn service_route(&mut self) -> Option<ServiceRoute> {
        let span = self.previous().span.clone();
        let method = self.expect_ident("expected HTTP method after route")?;
        let path = self.expect_string("expected route path string")?;
        let mut requires = Vec::new();

        while !self.at_symbol(Symbol::LBrace) && !self.at(TokenKindRef::Eof) {
            if self.match_keyword(Keyword::Requires) {
                if let Some(permission) = self.permission_path() {
                    requires.push(permission);
                }
            } else {
                self.advance();
            }
        }

        self.expect_symbol(Symbol::LBrace, "expected route body")?;
        let (input, body) = self.service_route_body();
        Some(ServiceRoute {
            method,
            path,
            requires,
            input,
            body,
            span,
        })
    }

    fn service_route_body(&mut self) -> (Option<ServiceInput>, Vec<Stmt>) {
        let mut input = None;
        let mut body = Vec::new();
        let mut recovered_missing_close = false;
        while !self.at_symbol(Symbol::RBrace) && !self.at(TokenKindRef::Eof) {
            self.skip_newlines();
            if self.at_symbol(Symbol::RBrace) {
                break;
            }
            if self.break_on_top_level_keyword_in_block("route") {
                recovered_missing_close = true;
                break;
            }
            if self.match_keyword(Keyword::Input) {
                if input.is_some() {
                    let token = self.previous().clone();
                    self.diagnostics.push(
                        Diagnostic::error("N0101", "duplicate route input", token.span)
                            .with_help("declare at most one input binding per route"),
                    );
                    self.consume_line();
                    continue;
                }
                input = self.service_input();
            } else if let Some(stmt) = self.statement() {
                body.push(stmt);
            } else {
                self.consume_line();
            }
        }

        if !recovered_missing_close {
            self.expect_symbol(Symbol::RBrace, "expected `}` after route body");
        }
        (input, body)
    }

    fn service_input(&mut self) -> Option<ServiceInput> {
        let span = self.previous().span.clone();
        let name = self.expect_ident("expected route input name")?;
        self.expect_symbol(Symbol::Colon, "expected `:` after route input name")?;
        let ty = self.collect_type_ref();
        let labels = self.collect_labels_until_line_or(Symbol::RBrace);
        self.consume_line();
        Some(ServiceInput {
            name,
            ty,
            labels,
            span,
        })
    }

    fn test_decl(&mut self) -> Option<TestDecl> {
        let span = self.previous().span.clone();
        let kind = if self.match_keyword(Keyword::Policy) {
            TestKind::Policy
        } else if self.match_keyword(Keyword::Workflow) {
            TestKind::Workflow
        } else if self.match_ident_text("ai") {
            TestKind::Ai
        } else {
            TestKind::Unit
        };
        let name = self.expect_string("expected test name string")?;
        self.expect_symbol(Symbol::LBrace, "expected `{` after test name")?;
        let body = self.block_body();
        Some(TestDecl {
            name,
            kind,
            body,
            span,
        })
    }

    fn params(&mut self) -> Option<Vec<Param>> {
        self.expect_symbol(Symbol::LParen, "expected `(` before parameters")?;
        let mut params = Vec::new();
        while !self.at_symbol(Symbol::RParen) && !self.at(TokenKindRef::Eof) {
            self.skip_newlines();
            let span = self.peek().span.clone();
            let Some(name) = self.expect_ident("expected parameter name") else {
                self.advance();
                continue;
            };
            self.expect_symbol(Symbol::Colon, "expected `:` after parameter name")?;
            let ty = self.collect_type_ref();
            let mut labels = Labels::default();
            while !self.at_symbol(Symbol::Comma)
                && !self.at_symbol(Symbol::RParen)
                && !self.at(TokenKindRef::Eof)
            {
                self.consume_label(&mut labels);
            }
            params.push(Param {
                name,
                ty,
                labels,
                span,
            });
            self.match_symbol(Symbol::Comma);
        }
        self.expect_symbol(Symbol::RParen, "expected `)` after parameters")?;
        Some(params)
    }

    fn result_type(&mut self) -> Option<TypeRef> {
        if self.match_symbol(Symbol::Arrow) {
            Some(self.collect_type_ref())
        } else {
            None
        }
    }

    fn block_body(&mut self) -> Vec<Stmt> {
        let mut body = Vec::new();
        let mut recovered_missing_close = false;
        while !self.at_symbol(Symbol::RBrace) && !self.at(TokenKindRef::Eof) {
            self.skip_newlines();
            if self.at_symbol(Symbol::RBrace) {
                break;
            }
            if self.break_on_top_level_keyword_in_block("block") {
                recovered_missing_close = true;
                break;
            }
            if let Some(stmt) = self.statement() {
                body.push(stmt);
            } else {
                self.consume_line();
            }
        }
        if !recovered_missing_close {
            self.expect_symbol(Symbol::RBrace, "expected `}` after block");
        }
        body
    }

    fn statement(&mut self) -> Option<Stmt> {
        if self.match_keyword(Keyword::Let) {
            return self.let_stmt(false).map(Stmt::Let);
        }
        if self.match_keyword(Keyword::Var) {
            return self.let_stmt(true).map(Stmt::Let);
        }
        if self.match_keyword(Keyword::Assert) {
            return Some(self.assert_stmt());
        }
        if self.match_ident_text("expect_deny") {
            return self
                .expect_policy_stmt(PolicyExpectation::Deny)
                .map(Stmt::ExpectPolicy);
        }
        if self.match_ident_text("expect_allow") {
            return self
                .expect_policy_stmt(PolicyExpectation::Allow)
                .map(Stmt::ExpectPolicy);
        }
        if self.match_ident_text("expect_workflow_success") {
            return Some(Stmt::ExpectWorkflow(
                self.expect_workflow_stmt(WorkflowExpectation::Success),
            ));
        }
        if self.match_ident_text("expect_workflow_failure") {
            return Some(Stmt::ExpectWorkflow(
                self.expect_workflow_stmt(WorkflowExpectation::Failure),
            ));
        }
        if self.match_ident_text("expect_audit") {
            return Some(Stmt::ExpectAudit(self.expect_audit_stmt()));
        }
        if self.match_ident_text("mock_ai") {
            return self.mock_ai_stmt().map(Stmt::MockAi);
        }
        if self.match_ident_text("mock_connector") {
            return self.mock_connector_stmt().map(Stmt::MockConnector);
        }
        if self.match_keyword(Keyword::Require) {
            return self.require_stmt().map(Stmt::Require);
        }
        if self.match_keyword(Keyword::Transaction) {
            return self.transaction_stmt().map(Stmt::Transaction);
        }
        if self.match_keyword(Keyword::Scope) {
            return self.scope_stmt().map(Stmt::Scope);
        }
        if self.match_keyword(Keyword::If) {
            return self.if_stmt().map(Stmt::If);
        }
        if self.match_keyword(Keyword::Match) {
            return self.match_stmt().map(Stmt::Match);
        }
        if self.match_keyword(Keyword::Return) {
            let span = self.previous().span.clone();
            let expr = RawExpr {
                text: self.collect_until_line_or(Symbol::RBrace),
                span,
            };
            return Some(Stmt::Return(expr));
        }
        if self.starts_assignment() {
            return self.assignment_stmt().map(Stmt::Assign);
        }

        let span = self.peek().span.clone();
        let text = self.collect_until_line_or(Symbol::RBrace);
        if text.trim().is_empty() {
            None
        } else {
            Some(Stmt::Expr(RawExpr { text, span }))
        }
    }

    fn assert_stmt(&mut self) -> Stmt {
        let span = self.previous().span.clone();
        let expr = RawExpr {
            text: self.collect_until_line_or(Symbol::RBrace),
            span: span.clone(),
        };
        Stmt::Assert(AssertStmt { expr, span })
    }

    fn expect_policy_stmt(&mut self, outcome: PolicyExpectation) -> Option<ExpectPolicyStmt> {
        let span = self.previous().span.clone();
        self.expect_symbol(Symbol::LBrace, "expected `{` after policy expectation")?;
        let body = self.block_body();
        Some(ExpectPolicyStmt {
            outcome,
            body,
            span,
        })
    }

    fn expect_workflow_stmt(&mut self, outcome: WorkflowExpectation) -> ExpectWorkflowStmt {
        let span = self.previous().span.clone();
        let call = RawExpr {
            text: self.collect_until_line_or(Symbol::RBrace),
            span: span.clone(),
        };
        ExpectWorkflowStmt {
            outcome,
            call,
            span,
        }
    }

    fn expect_audit_stmt(&mut self) -> ExpectAuditStmt {
        let span = self.previous().span.clone();
        let event = RawExpr {
            text: self.collect_until_line_or(Symbol::RBrace),
            span: span.clone(),
        };
        ExpectAuditStmt { event, span }
    }

    fn mock_ai_stmt(&mut self) -> Option<MockAiStmt> {
        let span = self.previous().span.clone();
        let call = RawExpr {
            text: self.collect_until_symbol(Symbol::FatArrow),
            span: span.clone(),
        };
        self.expect_symbol(Symbol::FatArrow, "expected `=>` after AI mock call")?;
        let value = RawExpr {
            text: self.collect_until_ident_text("confidence", Symbol::RBrace),
            span: span.clone(),
        };
        if !self.match_ident_text("confidence") {
            self.diagnostics.push(Diagnostic::error(
                "N0101",
                "expected `confidence` in AI mock",
                span.clone(),
            ));
        }
        let confidence = RawExpr {
            text: self.collect_until_line_or(Symbol::RBrace),
            span: span.clone(),
        };
        Some(MockAiStmt {
            call,
            value,
            confidence,
            span,
        })
    }

    fn mock_connector_stmt(&mut self) -> Option<MockConnectorStmt> {
        let span = self.previous().span.clone();
        let call = RawExpr {
            text: self.collect_until_symbol(Symbol::FatArrow),
            span: span.clone(),
        };
        self.expect_symbol(Symbol::FatArrow, "expected `=>` after connector mock call")?;
        let value = RawExpr {
            text: self.collect_until_line_or(Symbol::RBrace),
            span: span.clone(),
        };
        Some(MockConnectorStmt { call, value, span })
    }

    fn let_stmt(&mut self, mutable: bool) -> Option<LetStmt> {
        let span = self.previous().span.clone();
        let name = self.expect_ident("expected binding name")?;
        let mut ty = None;
        let mut labels = Labels::default();
        let mut expr = None;

        if self.match_symbol(Symbol::Colon) {
            ty = Some(self.collect_type_ref());
            while !self.at_symbol(Symbol::Eq)
                && !self.at(TokenKindRef::Newline)
                && !self.at_symbol(Symbol::RBrace)
                && !self.at(TokenKindRef::Eof)
            {
                self.consume_label(&mut labels);
            }
        }

        if self.match_symbol(Symbol::Eq) {
            let expr_span = self.previous().span.clone();
            expr = Some(RawExpr {
                text: self.collect_until_line_or(Symbol::RBrace),
                span: expr_span,
            });
        }

        Some(LetStmt {
            mutable,
            name,
            ty,
            labels,
            expr,
            span,
        })
    }

    fn match_stmt(&mut self) -> Option<MatchStmt> {
        let span = self.previous().span.clone();
        let expr = RawExpr {
            text: self.collect_until_symbol(Symbol::LBrace),
            span: span.clone(),
        };
        self.expect_symbol(Symbol::LBrace, "expected `{` after match expression")?;

        let mut arms = Vec::new();
        let mut recovered_missing_close = false;
        while !self.at_symbol(Symbol::RBrace) && !self.at(TokenKindRef::Eof) {
            self.skip_newlines();
            if self.at_symbol(Symbol::RBrace) {
                break;
            }
            if self.break_on_top_level_keyword_in_block("match") {
                recovered_missing_close = true;
                break;
            }
            if let Some(arm) = self.match_arm() {
                arms.push(arm);
            } else {
                self.consume_line();
            }
        }
        if !recovered_missing_close {
            self.expect_symbol(Symbol::RBrace, "expected `}` after match arms")?;
        }

        Some(MatchStmt { expr, arms, span })
    }

    fn match_arm(&mut self) -> Option<MatchArm> {
        let span = self.peek().span.clone();
        let pattern_name = self.expect_ident("expected match pattern")?;
        let pattern = if pattern_name == "_" {
            MatchPattern::Wildcard
        } else {
            let payload = self.match_pattern_payload()?;
            MatchPattern::Variant {
                name: pattern_name,
                payload,
                bindings: self.match_pattern_bindings()?,
            }
        };
        let guard = if self.match_keyword(Keyword::If) {
            Some(RawExpr {
                text: self.collect_until_symbol(Symbol::FatArrow),
                span: self.previous().span.clone(),
            })
        } else {
            None
        };
        self.expect_symbol(Symbol::FatArrow, "expected `=>` after match pattern")?;
        self.expect_symbol(Symbol::LBrace, "expected `{` after match arm")?;
        let body = self.block_body();
        Some(MatchArm {
            pattern,
            guard,
            body,
            span,
        })
    }

    fn match_pattern_payload(&mut self) -> Option<Option<String>> {
        if !self.match_symbol(Symbol::LParen) {
            return Some(None);
        }

        let binding = self.expect_ident("expected payload binding in match pattern")?;
        self.expect_symbol(Symbol::RParen, "expected `)` after match pattern payload")?;
        Some(Some(binding))
    }

    fn match_pattern_bindings(&mut self) -> Option<Vec<MatchBinding>> {
        if !self.match_symbol(Symbol::LBrace) {
            return Some(Vec::new());
        }
        self.match_pattern_binding_list()
    }

    fn match_pattern_binding_list(&mut self) -> Option<Vec<MatchBinding>> {
        let mut bindings = Vec::new();
        while !self.at_symbol(Symbol::RBrace) && !self.at(TokenKindRef::Eof) {
            self.skip_newlines();
            if self.at_symbol(Symbol::RBrace) {
                break;
            }

            let field = self.expect_ident("expected field name in match pattern")?;
            let mut name = field.clone();
            let mut nested_type = None;
            let mut nested = Vec::new();
            if self.match_symbol(Symbol::Colon) {
                let target =
                    self.expect_ident("expected binding name after `:` in match pattern")?;
                if self.match_symbol(Symbol::LBrace) {
                    nested_type = Some(target);
                    nested = self.match_pattern_binding_list()?;
                } else {
                    name = target;
                }
            } else if self.match_symbol(Symbol::LBrace) {
                nested_type = Some(field.clone());
                nested = self.match_pattern_binding_list()?;
            }
            bindings.push(MatchBinding {
                field,
                name,
                nested_type,
                nested,
            });

            self.skip_newlines();
            if self.match_symbol(Symbol::Comma) {
                continue;
            }
            if !self.at_symbol(Symbol::RBrace) {
                self.expect_symbol(Symbol::Comma, "expected `,` between match pattern bindings")?;
            }
        }

        self.expect_symbol(Symbol::RBrace, "expected `}` after match pattern bindings")?;
        Some(bindings)
    }
    fn assignment_stmt(&mut self) -> Option<AssignStmt> {
        let span = self.peek().span.clone();
        let name = self.expect_ident("expected assignment target")?;
        self.expect_symbol(Symbol::Eq, "expected `=` in assignment")?;
        let expr = RawExpr {
            text: self.collect_until_line_or(Symbol::RBrace),
            span: span.clone(),
        };
        Some(AssignStmt { name, expr, span })
    }

    fn require_stmt(&mut self) -> Option<RequireStmt> {
        let span = self.previous().span.clone();
        let permission = self.permission_path()?;
        let mut actor = None;
        if self.match_keyword(Keyword::For) {
            actor = Some(self.collect_until_line_or(Symbol::RBrace));
        } else {
            self.consume_line();
        }
        Some(RequireStmt {
            permission,
            actor,
            span,
        })
    }

    fn transaction_stmt(&mut self) -> Option<TransactionStmt> {
        let span = self.previous().span.clone();
        let saga = self.match_keyword(Keyword::Saga);
        self.expect_symbol(Symbol::LBrace, "expected transaction body")?;
        let body = self.block_body();
        Some(TransactionStmt { saga, body, span })
    }

    fn scope_stmt(&mut self) -> Option<ScopeStmt> {
        let span = self.previous().span.clone();
        self.expect_symbol(Symbol::LBrace, "expected scope body")?;
        let body = self.block_body();
        Some(ScopeStmt { body, span })
    }

    fn if_stmt(&mut self) -> Option<IfStmt> {
        let span = self.previous().span.clone();
        let condition = RawExpr {
            text: self.collect_until_symbol(Symbol::LBrace),
            span: span.clone(),
        };
        self.expect_symbol(Symbol::LBrace, "expected `{` after if condition")?;
        let then_body = self.block_body();
        let else_body = if self.match_keyword(Keyword::Else) {
            self.expect_symbol(Symbol::LBrace, "expected `{` after else")?;
            self.block_body()
        } else {
            Vec::new()
        };
        Some(IfStmt {
            condition,
            then_body,
            else_body,
            span,
        })
    }

    fn collect_type_ref(&mut self) -> TypeRef {
        let mut raw = String::new();
        let mut depth = 0usize;
        while !self.at(TokenKindRef::Eof) {
            if depth == 0
                && (self.at(TokenKindRef::Newline)
                    || self.at_symbol(Symbol::Comma)
                    || self.at_symbol(Symbol::RParen)
                    || self.at_symbol(Symbol::Eq)
                    || self.at_symbol(Symbol::LBrace)
                    || self.at_keyword(Keyword::From)
                    || self.is_label_start()
                    || self.at_keyword(Keyword::Requires))
            {
                break;
            }

            if self.match_symbol(Symbol::Lt) {
                depth += 1;
                raw.push('<');
                continue;
            }
            if self.match_symbol(Symbol::Gt) {
                depth = depth.saturating_sub(1);
                raw.push('>');
                continue;
            }
            raw.push_str(&self.advance().lexeme);
        }
        TypeRef {
            raw: raw.trim().to_string(),
        }
    }

    fn collect_labels_until_line_or(&mut self, stop: Symbol) -> Labels {
        let mut labels = Labels::default();
        while !self.at(TokenKindRef::Newline)
            && !self.at_symbol(stop)
            && !self.at(TokenKindRef::Eof)
        {
            self.consume_label(&mut labels);
        }
        labels
    }

    fn consume_label(&mut self, labels: &mut Labels) {
        if self.match_keyword(Keyword::From) {
            if let Some(source) = self.expect_ident("expected provenance source after from") {
                labels.source = Some(source);
            }
            return;
        }
        if self.match_keyword(Keyword::Private) {
            labels.privacy = Some(Privacy::Private);
            return;
        }
        if self.match_keyword(Keyword::Public) {
            labels.privacy = Some(Privacy::Public);
            return;
        }
        if self.match_keyword(Keyword::Internal) {
            labels.privacy = Some(Privacy::Internal);
            return;
        }
        if self.match_keyword(Keyword::Sensitive) {
            labels.privacy = Some(Privacy::Sensitive);
            return;
        }
        if self.match_keyword(Keyword::Secret) {
            labels.privacy = Some(Privacy::Secret);
            return;
        }
        if self.match_keyword(Keyword::Regulated) {
            labels.privacy = Some(Privacy::Regulated);
            return;
        }
        if self.match_keyword(Keyword::Trusted) {
            labels.trust = Some(Trust::Trusted);
            return;
        }
        if self.match_keyword(Keyword::Untrusted) {
            labels.trust = Some(Trust::Untrusted);
            return;
        }
        if self.match_keyword(Keyword::Verified) {
            labels.trust = Some(Trust::Verified);
            return;
        }
        self.advance();
    }

    fn risk(&mut self) -> Risk {
        if let Some(name) = self.maybe_ident() {
            match name.as_str() {
                "low" => Risk::Low,
                "medium" => Risk::Medium,
                "high" => Risk::High,
                "critical" => Risk::Critical,
                _ => Risk::Low,
            }
        } else {
            Risk::Low
        }
    }

    fn permission_path(&mut self) -> Option<String> {
        let mut parts = Vec::new();
        if let Some(first) = self.expect_ident("expected permission name") {
            parts.push(first);
        }
        while self.match_symbol(Symbol::Dot) {
            let part = self.expect_ident("expected permission name after `.`")?;
            parts.push(part);
        }
        parts.last().cloned()
    }

    fn parse_path_until_line(&mut self) -> Option<String> {
        let path = self.collect_until_line_or(Symbol::RBrace);
        let path = compact_member_access(path.trim());
        if path.is_empty() {
            None
        } else {
            Some(path)
        }
    }

    fn collect_until_line_or(&mut self, stop: Symbol) -> String {
        let mut text = String::new();
        let mut paren_depth = 0usize;
        let mut brace_depth = 0usize;
        while !self.at(TokenKindRef::Eof) {
            if self.at(TokenKindRef::Newline) && paren_depth == 0 && brace_depth == 0 {
                break;
            }
            if self.at_symbol(stop) && paren_depth == 0 && brace_depth == 0 {
                break;
            }

            let token = self.advance().clone();
            match token.kind {
                TokenKind::Symbol(Symbol::LParen) => paren_depth += 1,
                TokenKind::Symbol(Symbol::RParen) => {
                    paren_depth = paren_depth.saturating_sub(1);
                }
                TokenKind::Symbol(Symbol::LBrace) => brace_depth += 1,
                TokenKind::Symbol(Symbol::RBrace) => {
                    brace_depth = brace_depth.saturating_sub(1);
                }
                _ => {}
            }
            text.push_str(&token.lexeme);
            text.push(' ');
        }
        self.match_kind(TokenKindRef::Newline);
        text.trim().to_string()
    }

    fn collect_until_metadata_or(&mut self, stop: Symbol) -> String {
        let mut text = String::new();
        while !self.at_symbol(stop) && !self.at(TokenKindRef::Eof) {
            if self.at(TokenKindRef::Newline) {
                self.match_kind(TokenKindRef::Newline);
                break;
            }
            if matches!(
                self.peek().kind,
                TokenKind::Keyword(Keyword::Requires | Keyword::Budget | Keyword::Rate)
            ) {
                break;
            }
            text.push_str(&self.advance().lexeme);
            text.push(' ');
        }
        text.trim().to_string()
    }

    fn collect_until_symbol(&mut self, stop: Symbol) -> String {
        let mut text = String::new();
        while !self.at_symbol(stop) && !self.at(TokenKindRef::Eof) {
            text.push_str(&self.advance().lexeme);
            text.push(' ');
        }
        text.trim().to_string()
    }

    fn collect_until_ident_text(&mut self, expected: &str, stop: Symbol) -> String {
        let mut text = String::new();
        let mut depth = 0usize;
        while !self.at(TokenKindRef::Newline)
            && !self.at_symbol(stop)
            && !self.at(TokenKindRef::Eof)
        {
            if depth == 0 && self.at_ident_text(expected) {
                break;
            }
            if self.at_symbol(Symbol::LParen) {
                depth += 1;
            } else if self.at_symbol(Symbol::RParen) {
                depth = depth.saturating_sub(1);
            }
            text.push_str(&self.advance().lexeme);
            text.push(' ');
        }
        text.trim().to_string()
    }

    fn skip_until_symbol(&mut self, symbol: Symbol) {
        while !self.at_symbol(symbol) && !self.at(TokenKindRef::Eof) {
            self.advance();
        }
    }

    fn consume_line(&mut self) {
        while !self.at(TokenKindRef::Newline) && !self.at(TokenKindRef::Eof) {
            self.advance();
        }
        self.match_kind(TokenKindRef::Newline);
    }

    fn skip_newlines(&mut self) {
        while self.match_kind(TokenKindRef::Newline) {}
    }

    fn synchronize_top_level(&mut self) {
        while !self.at(TokenKindRef::Newline) && !self.at(TokenKindRef::Eof) {
            self.advance();
        }
        self.skip_newlines();
    }

    fn expect_ident(&mut self, message: impl Into<String>) -> Option<String> {
        if let Some(name) = self.maybe_ident() {
            Some(name)
        } else {
            let token = self.peek().clone();
            self.diagnostics
                .push(Diagnostic::error("N0101", message, token.span));
            None
        }
    }

    fn expect_string(&mut self, message: impl Into<String>) -> Option<String> {
        match &self.peek().kind {
            TokenKind::String(value) => {
                let value = value.clone();
                self.advance();
                Some(value)
            }
            _ => {
                let token = self.peek().clone();
                self.diagnostics
                    .push(Diagnostic::error("N0101", message, token.span));
                None
            }
        }
    }

    fn maybe_ident(&mut self) -> Option<String> {
        match &self.peek().kind {
            TokenKind::Ident(name) => {
                let name = name.clone();
                self.advance();
                Some(name)
            }
            TokenKind::Keyword(keyword) => {
                let name = format!("{keyword:?}").to_ascii_lowercase();
                self.advance();
                Some(name)
            }
            _ => None,
        }
    }

    fn expect_symbol(&mut self, symbol: Symbol, message: impl Into<String>) -> Option<()> {
        if self.match_symbol(symbol) {
            Some(())
        } else {
            let token = self.peek().clone();
            self.diagnostics
                .push(Diagnostic::error("N0102", message, token.span));
            None
        }
    }

    fn is_label_start(&self) -> bool {
        matches!(
            self.peek().kind,
            TokenKind::Keyword(
                Keyword::From
                    | Keyword::Private
                    | Keyword::Public
                    | Keyword::Internal
                    | Keyword::Sensitive
                    | Keyword::Secret
                    | Keyword::Regulated
                    | Keyword::Trusted
                    | Keyword::Untrusted
                    | Keyword::Verified
            )
        )
    }

    fn break_on_top_level_keyword_in_block(&mut self, block_kind: &str) -> bool {
        if let TokenKind::Keyword(keyword) = self.peek().kind {
            if is_top_level_keyword(keyword) {
                if block_kind == "impl" && keyword == Keyword::Fn {
                    return false;
                }
                let token = self.peek().clone();
                self.diagnostics.push(
                    Diagnostic::error(
                        "N0103",
                        format!("expected `}}` before `{}` declaration", token.lexeme),
                        token.span,
                    )
                    .with_reason(format!(
                        "a `{block_kind}` block cannot contain top-level declarations"
                    ))
                    .with_help("close the current block before starting the next declaration"),
                );
                return true;
            }
        }
        false
    }

    fn match_keyword(&mut self, keyword: Keyword) -> bool {
        if self.at_keyword(keyword) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn match_ident_text(&mut self, expected: &str) -> bool {
        match &self.peek().kind {
            TokenKind::Ident(name) if name == expected => {
                self.advance();
                true
            }
            TokenKind::Keyword(keyword)
                if format!("{keyword:?}").eq_ignore_ascii_case(expected) =>
            {
                self.advance();
                true
            }
            _ => false,
        }
    }

    fn at_ident_text(&self, expected: &str) -> bool {
        match &self.peek().kind {
            TokenKind::Ident(name) => name == expected,
            TokenKind::Keyword(keyword) => format!("{keyword:?}").eq_ignore_ascii_case(expected),
            _ => false,
        }
    }

    fn at_keyword(&self, keyword: Keyword) -> bool {
        matches!(self.peek().kind, TokenKind::Keyword(actual) if actual == keyword)
    }

    fn match_symbol(&mut self, symbol: Symbol) -> bool {
        if self.at_symbol(symbol) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn at_symbol(&self, symbol: Symbol) -> bool {
        matches!(self.peek().kind, TokenKind::Symbol(actual) if actual == symbol)
    }

    fn match_kind(&mut self, kind: TokenKindRef) -> bool {
        if self.at(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn at(&self, kind: TokenKindRef) -> bool {
        matches!(
            (&self.peek().kind, kind),
            (TokenKind::Newline, TokenKindRef::Newline) | (TokenKind::Eof, TokenKindRef::Eof)
        )
    }

    fn starts_assignment(&self) -> bool {
        matches!(self.peek().kind, TokenKind::Ident(_))
            && matches!(
                self.tokens.get(self.index + 1).map(|token| &token.kind),
                Some(TokenKind::Symbol(Symbol::Eq))
            )
    }

    fn advance(&mut self) -> &'a Token {
        if self.index < self.tokens.len() {
            self.index += 1;
        }
        self.previous()
    }

    fn previous(&self) -> &'a Token {
        &self.tokens[self.index.saturating_sub(1)]
    }

    fn peek(&self) -> &'a Token {
        self.tokens
            .get(self.index)
            .unwrap_or_else(|| self.tokens.last().expect("parser needs eof token"))
    }
}

#[derive(Debug, Clone, Copy)]
enum TokenKindRef {
    Newline,
    Eof,
}

fn is_top_level_keyword(keyword: Keyword) -> bool {
    matches!(
        keyword,
        Keyword::Module
            | Keyword::Use
            | Keyword::Permission
            | Keyword::Role
            | Keyword::Policy
            | Keyword::Type
            | Keyword::Enum
            | Keyword::Fn
            | Keyword::Workflow
            | Keyword::Action
            | Keyword::Connector
            | Keyword::Service
            | Keyword::Test
    )
}

fn privacy_from_text(text: &str) -> Option<Privacy> {
    if text.contains("regulated") {
        Some(Privacy::Regulated)
    } else if text.contains("sensitive") {
        Some(Privacy::Sensitive)
    } else if text.contains("secret") {
        Some(Privacy::Secret)
    } else if text.contains("private") {
        Some(Privacy::Private)
    } else if text.contains("internal") {
        Some(Privacy::Internal)
    } else if text.contains("public") {
        Some(Privacy::Public)
    } else {
        None
    }
}

fn trust_from_text(text: &str) -> Option<Trust> {
    if text.contains("untrusted") {
        Some(Trust::Untrusted)
    } else if text.contains("verified") {
        Some(Trust::Verified)
    } else if text.contains("trusted") {
        Some(Trust::Trusted)
    } else {
        None
    }
}

fn compact_duration_units(text: &str) -> String {
    text.replace(" ms", "ms")
        .replace(" s", "s")
        .replace(" m", "m")
        .replace(" h", "h")
}

fn source_from_rule(text: &str) -> Option<String> {
    let parts: Vec<_> = text.split_whitespace().collect();
    parts
        .windows(2)
        .find(|pair| pair[0] != "->" && pair[1] == "->")
        .map(|pair| pair[0].to_string())
}

fn target_from_rule(text: &str) -> Option<String> {
    text.split("->")
        .nth(1)
        .map(compact_member_access)
        .and_then(|right| right.split_whitespace().next().map(str::to_string))
}

fn tenant_from_rule(text: &str) -> Option<String> {
    let parts: Vec<_> = text.split_whitespace().collect();
    parts
        .windows(3)
        .find(|window| window[0] == "for" && window[1] == "tenant")
        .map(|window| window[2].to_string())
}

fn route_from_rule(text: &str) -> Option<PolicyRouteCondition> {
    let parts: Vec<_> = text.split_whitespace().collect();
    parts
        .windows(4)
        .find(|window| window[0] == "when" && window[1] == "route")
        .map(|window| PolicyRouteCondition {
            method: window[2].to_ascii_uppercase(),
            path: trim_quotes(window[3]).to_string(),
        })
}

fn trim_quotes(text: &str) -> &str {
    text.strip_prefix('"')
        .and_then(|text| text.strip_suffix('"'))
        .unwrap_or(text)
}

fn compact_member_access(text: &str) -> String {
    text.replace(" . ", ".")
        .replace(". ", ".")
        .replace(" .", ".")
}
