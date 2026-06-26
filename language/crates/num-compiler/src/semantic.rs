use crate::diagnostic::Diagnostic;
use crate::expr::{self, BinaryOp, Expr};
use crate::{ast::*, builtins};
use std::collections::{HashMap, HashSet};

mod brand_constructors;
mod calls;
mod enum_constructors;
mod expressions;
mod match_patterns;
mod option_constructors;
mod option_flow;
mod policies;
mod result_constructors;
mod result_flow;
mod trust_flow;
use option_constructors::{is_option_constructor_expr, is_option_constructor_name};
use policies::PolicySet;
use result_constructors::{is_result_constructor_expr, is_result_constructor_name};
use trust_flow::merge_labels;

pub fn check(module: &Module) -> Vec<Diagnostic> {
    Checker::new(module, 0).check()
}

pub fn check_service_route_for_tenant(
    module: &Module,
    service_name: &str,
    method: &str,
    path: &str,
    tenant: &str,
) -> Vec<Diagnostic> {
    let mut checker = Checker::new(module, 0).with_policy_tenant(tenant);
    let Some(service) = module.declarations.iter().find_map(|decl| match decl {
        Declaration::Service(service) if service.name == service_name => Some(service),
        _ => None,
    }) else {
        return checker.diagnostics;
    };
    let Some(route) = service
        .routes
        .iter()
        .find(|route| route.method.eq_ignore_ascii_case(method) && route.path == path)
    else {
        return checker.diagnostics;
    };

    checker.service_route(route);
    checker.diagnostics
}

#[allow(dead_code)]
pub(crate) fn check_declarations_from(
    module: &Module,
    first_local_declaration: usize,
) -> Vec<Diagnostic> {
    Checker::new(module, first_local_declaration).check()
}

#[derive(Debug, Clone)]
struct Binding {
    ty: Option<TypeRef>,
    labels: Labels,
    mutable: bool,
    uncertain: bool,
    option_checked: bool,
    result_checked: Option<ResultCheck>,
    secret: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CallableKind {
    Function,
    Workflow,
    Action,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResultCheck {
    Ok,
    Err,
}

#[derive(Debug, Clone)]
enum MatchDomain {
    Enum { name: String, variants: Vec<String> },
    Union(Vec<String>),
}

#[derive(Debug, Clone, Copy)]
struct CallableSignature<'a> {
    kind: CallableKind,
    params: &'a [Param],
    result: Option<&'a TypeRef>,
}

#[derive(Debug, Clone)]
struct TypeAlias {
    generic_params: Vec<String>,
    target: TypeRef,
    nominal: bool,
}

struct Checker<'a> {
    module: &'a Module,
    first_local_declaration: usize,
    diagnostics: Vec<Diagnostic>,
    permissions: HashSet<String>,
    action_permissions: HashMap<String, Vec<String>>,
    action_risks: HashMap<String, Risk>,
    callable_signatures: HashMap<String, CallableSignature<'a>>,
    policies: PolicySet<'a>,
    types: HashSet<String>,
    type_arities: HashMap<String, usize>,
    type_generic_params: HashMap<String, Vec<String>>,
    type_aliases: HashMap<String, TypeAlias>,
    type_fields: HashMap<String, HashMap<String, &'a Field>>,
    enum_variants: HashMap<String, Vec<String>>,
    enum_variant_payloads: HashMap<String, HashMap<String, Option<TypeRef>>>,
    external_namespaces: HashSet<String>,
    connector_methods: HashMap<String, HashMap<String, &'a ConnectorMethod>>,
    method_signatures: HashMap<String, HashMap<String, CallableSignature<'a>>>,
    policy_tenant: Option<String>,
}

impl<'a> Checker<'a> {
    fn new(module: &'a Module, first_local_declaration: usize) -> Self {
        let mut permissions = HashSet::new();
        let mut action_permissions = HashMap::new();
        let mut action_risks = HashMap::new();
        let mut callable_signatures = HashMap::new();
        let mut policies = PolicySet::new();
        let mut types = builtin_type_names();
        let mut type_arities = builtin_type_arities();
        let mut type_generic_params = HashMap::new();
        let mut type_aliases = HashMap::new();
        let mut type_fields = HashMap::new();
        let mut enum_variants = HashMap::new();
        let mut enum_variant_payloads = HashMap::new();
        let mut external_namespaces = HashSet::new();
        let mut connector_methods = HashMap::new();
        let mut method_signatures = HashMap::new();

        for decl in &module.declarations {
            match decl {
                Declaration::Permission(permission) => {
                    permissions.insert(permission.name.clone());
                }
                Declaration::Impl(imp) => {
                    for method in &imp.methods {
                        method_signatures
                            .entry(imp.target.clone())
                            .or_insert_with(HashMap::new)
                            .insert(
                                method.name.clone(),
                                CallableSignature {
                                    kind: CallableKind::Function,
                                    params: &method.params,
                                    result: method.result.as_ref(),
                                },
                            );
                    }
                }
                Declaration::Action(action) => {
                    action_permissions.insert(action.name.clone(), action.requires.clone());
                    action_risks.insert(action.name.clone(), action.risk);
                    callable_signatures.insert(
                        action.name.clone(),
                        CallableSignature {
                            kind: CallableKind::Action,
                            params: &action.params,
                            result: action.result.as_ref(),
                        },
                    );
                }
                Declaration::Function(function) => {
                    callable_signatures.insert(
                        function.name.clone(),
                        CallableSignature {
                            kind: CallableKind::Function,
                            params: &function.params,
                            result: function.result.as_ref(),
                        },
                    );
                }
                Declaration::Workflow(workflow) => {
                    callable_signatures.insert(
                        workflow.name.clone(),
                        CallableSignature {
                            kind: CallableKind::Workflow,
                            params: &workflow.params,
                            result: workflow.result.as_ref(),
                        },
                    );
                }
                Declaration::Policy(policy) => {
                    policies.extend_policy(policy);
                }
                Declaration::Type(ty) => {
                    types.insert(ty.name.clone());
                    type_arities.insert(ty.name.clone(), ty.generic_params.len());
                    type_generic_params.insert(ty.name.clone(), ty.generic_params.clone());
                    match &ty.body {
                        TypeBody::Struct(fields) => {
                            type_fields.insert(
                                ty.name.clone(),
                                fields
                                    .iter()
                                    .map(|field| (field.name.clone(), field))
                                    .collect(),
                            );
                        }
                        TypeBody::Alias(target) => {
                            type_aliases.insert(
                                ty.name.clone(),
                                TypeAlias {
                                    generic_params: ty.generic_params.clone(),
                                    target: target.clone(),
                                    nominal: is_brand_type(target),
                                },
                            );
                        }
                    }
                }
                Declaration::Enum(en) => {
                    types.insert(en.name.clone());
                    enum_variants.insert(
                        en.name.clone(),
                        en.variants
                            .iter()
                            .map(|variant| variant.name.clone())
                            .collect(),
                    );
                    enum_variant_payloads.insert(
                        en.name.clone(),
                        en.variants
                            .iter()
                            .map(|variant| (variant.name.clone(), variant.payload.clone()))
                            .collect(),
                    );
                }
                Declaration::Connector(connector) => {
                    external_namespaces.insert(connector.name.clone());
                    connector_methods.insert(
                        connector.name.clone(),
                        connector
                            .methods
                            .iter()
                            .map(|method| (method.name.clone(), method))
                            .collect(),
                    );
                }
                Declaration::Service(service) => {
                    external_namespaces.insert(service.name.clone());
                }
                _ => {}
            }
        }

        Self {
            module,
            first_local_declaration,
            diagnostics: Vec::new(),
            permissions,
            action_permissions,
            action_risks,
            callable_signatures,
            policies,
            types,
            type_arities,
            type_generic_params,
            type_aliases,
            type_fields,
            enum_variants,
            enum_variant_payloads,
            external_namespaces,
            connector_methods,
            method_signatures,
            policy_tenant: None,
        }
    }

    fn with_policy_tenant(mut self, tenant: &str) -> Self {
        self.policy_tenant = Some(tenant.to_string());
        self
    }

    fn check(mut self) -> Vec<Diagnostic> {
        self.duplicate_declarations();

        for decl in self
            .module
            .declarations
            .iter()
            .skip(self.first_local_declaration)
        {
            match decl {
                Declaration::Permission(_) => {}
                Declaration::Role(role) => self.role(role),
                Declaration::Policy(_) => {}
                Declaration::Type(ty) => self.type_decl(ty),
                Declaration::Enum(en) => self.enum_decl(en),
                Declaration::Function(function) => self.callable(function),
                Declaration::Workflow(workflow) => self.callable(workflow),
                Declaration::Action(action) => self.action(action),
                Declaration::Connector(connector) => self.connector(connector),
                Declaration::Service(service) => self.service(service),
                Declaration::Test(test) => self.test(test),
                Declaration::Impl(imp) => self.impl_decl(imp),
            }
        }

        self.diagnostics
    }

    fn enum_decl(&mut self, decl: &EnumDecl) {
        for variant in &decl.variants {
            if let Some(payload) = &variant.payload {
                self.known_type_ref(payload, &variant.span);
            }
        }
    }

    fn duplicate_declarations(&mut self) {
        let mut seen = HashSet::new();
        for decl in &self.module.declarations {
            if matches!(decl, Declaration::Impl(_)) {
                continue;
            }
            let name = decl.name();
            if !seen.insert(name.to_string()) {
                self.diagnostics.push(
                    Diagnostic::error(
                        "N1000",
                        format!("duplicate declaration `{name}`"),
                        decl.span().clone(),
                    )
                    .with_reason("top-level names are module-scoped")
                    .with_help("rename one declaration or move it into another module"),
                );
            }
        }
    }

    fn role(&mut self, role: &RoleDecl) {
        for permission in &role.allows {
            if !self.permissions.contains(permission) {
                self.diagnostics.push(
                    Diagnostic::error(
                        "N1100",
                        format!(
                            "role `{}` allows unknown permission `{permission}`",
                            role.name
                        ),
                        role.span.clone(),
                    )
                    .with_help("declare the permission before assigning it to a role"),
                );
            }
        }
    }

    fn type_decl(&mut self, ty: &TypeDecl) {
        self.duplicate_generic_params(ty);
        match &ty.body {
            TypeBody::Struct(type_fields) => {
                let mut fields = HashSet::new();
                for field in type_fields {
                    if !fields.insert(field.name.clone()) {
                        self.diagnostics.push(
                            Diagnostic::error(
                                "N1200",
                                format!("duplicate field `{}` in type `{}`", field.name, ty.name),
                                field.span.clone(),
                            )
                            .with_help("field names must be unique inside a type"),
                        );
                    }
                    self.known_type_ref_with_generics(&field.ty, &field.span, &ty.generic_params);
                }
            }
            TypeBody::Alias(alias) => {
                self.known_type_ref_with_generics(alias, &ty.span, &ty.generic_params)
            }
        }
    }

    fn duplicate_generic_params(&mut self, ty: &TypeDecl) {
        let mut seen = HashSet::new();
        for param in &ty.generic_params {
            if !seen.insert(param.clone()) {
                self.diagnostics.push(
                    Diagnostic::error(
                        "N1202",
                        format!(
                            "duplicate generic parameter `{}` in type `{}`",
                            param, ty.name
                        ),
                        ty.span.clone(),
                    )
                    .with_help("generic parameter names must be unique inside a type declaration"),
                );
            }
        }
    }

    fn action(&mut self, action: &ActionDecl) {
        for param in &action.params {
            self.known_type_ref(&param.ty, &param.span);
        }
        if let Some(result) = &action.result {
            self.known_type_ref(result, &action.span);
        }
        for permission in &action.requires {
            self.known_permission(permission, &action.span);
        }

        if action.risk >= Risk::High && !contains_audit(&action.body) {
            self.diagnostics.push(
                Diagnostic::error(
                    "N2001",
                    format!(
                        "high-risk action `{}` must write an audit event",
                        action.name
                    ),
                    action.span.clone(),
                )
                .with_reason("actions with real-world effects need traceability")
                .with_help("add audit(\"event_name\", ...) inside the action body"),
            );
        }

        if action.risk >= Risk::High && action.rollback.is_none() {
            self.diagnostics.push(
                Diagnostic::warning(
                    "N2002",
                    format!("high-risk action `{}` has no rollback", action.name),
                    action.span.clone(),
                )
                .with_reason("saga transactions can only compensate actions with rollback metadata")
                .with_help("add rollback compensating_action(args) to the action signature"),
            );
        }

        self.body(
            &action.body,
            &action.params,
            &action.requires,
            action.result.as_ref(),
            &action.span,
            None,
        );
    }

    fn connector(&mut self, connector: &ConnectorDecl) {
        let mut seen = HashSet::new();
        for method in &connector.methods {
            if !seen.insert(method.name.clone()) {
                self.diagnostics.push(
                    Diagnostic::error(
                        "N2702",
                        format!(
                            "duplicate connector method `{}.{}`",
                            connector.name, method.name
                        ),
                        method.span.clone(),
                    )
                    .with_help("connector method names must be unique inside a connector"),
                );
            }

            for param in &method.params {
                self.known_type_ref(&param.ty, &param.span);
            }
            if let Some(result) = &method.result {
                self.known_type_ref(result, &method.span);
            }
        }
    }

    fn service(&mut self, service: &ServiceDecl) {
        let mut seen_routes = HashSet::new();
        for route in &service.routes {
            let route_key = format!("{} {}", route.method, route.path);
            if !seen_routes.insert(route_key.clone()) {
                self.diagnostics.push(
                    Diagnostic::error(
                        "N2800",
                        format!("duplicate service route `{route_key}`"),
                        route.span.clone(),
                    )
                    .with_help("service routes must be unique by method and path"),
                );
            }

            self.service_route(route);
        }
    }

    fn service_route(&mut self, route: &ServiceRoute) {
        for permission in &route.requires {
            self.known_permission(permission, &route.span);
        }

        let params = route
            .input
            .as_ref()
            .map(|input| {
                self.known_type_ref(&input.ty, &input.span);
                vec![Param {
                    name: input.name.clone(),
                    ty: input.ty.clone(),
                    labels: input.labels.clone(),
                    span: input.span.clone(),
                }]
            })
            .unwrap_or_default();

        self.body(
            &route.body,
            &params,
            &route.requires,
            None,
            &route.span,
            None,
        );
    }

    fn callable(&mut self, callable: &CallableDecl) {
        for param in &callable.params {
            self.known_type_ref(&param.ty, &param.span);
        }
        if let Some(result) = &callable.result {
            self.known_type_ref(result, &callable.span);
        }
        for permission in &callable.requires {
            self.known_permission(permission, &callable.span);
        }
        self.body(
            &callable.body,
            &callable.params,
            &callable.requires,
            callable.result.as_ref(),
            &callable.span,
            None,
        );
    }

    fn impl_decl(&mut self, decl: &ImplDecl) {
        let type_ref = TypeRef {
            raw: decl.target.clone(),
        };
        self.known_type_ref(&type_ref, &decl.span);
        for method in &decl.methods {
            self.method(decl.target.clone(), method);
        }
    }

    fn method(&mut self, target_type: String, method: &CallableDecl) {
        for param in &method.params {
            self.known_type_ref(&param.ty, &param.span);
        }
        if let Some(result) = &method.result {
            self.known_type_ref(result, &method.span);
        }
        for permission in &method.requires {
            self.known_permission(permission, &method.span);
        }
        self.method_body(
            &target_type,
            &method.body,
            &method.params,
            &method.requires,
            method.result.as_ref(),
            &method.span,
        );
    }

    fn method_body(
        &mut self,
        target_type: &str,
        body: &[Stmt],
        params: &[Param],
        declared_requires: &[String],
        expected_return: Option<&TypeRef>,
        callable_span: &crate::span::Span,
    ) {
        let mut env = self.base_env();
        let mut granted: HashSet<String> = declared_requires.iter().cloned().collect();

        // Inject implicit self binding:
        let self_ty = TypeRef {
            raw: target_type.to_string(),
        };
        env.insert(
            "self".to_string(),
            Binding {
                ty: Some(self_ty.clone()),
                labels: Labels::default(),
                mutable: false,
                uncertain: self.is_uncertain_type(&self_ty),
                option_checked: false,
                result_checked: None,
                secret: self.resolve_aliases(&self_ty).is_secret(),
            },
        );

        for param in params {
            env.insert(
                param.name.clone(),
                Binding {
                    ty: Some(param.ty.clone()),
                    labels: param.labels.clone(),
                    mutable: false,
                    uncertain: self.is_uncertain_type(&param.ty),
                    option_checked: false,
                    result_checked: None,
                    secret: self.resolve_aliases(&param.ty).is_secret()
                        || param.labels.privacy == Some(Privacy::Secret),
                },
            );
        }

        self.statements(body, &mut env, &mut granted, expected_return, None);
        if let Some(expected) = expected_return {
            if !self.all_paths_return(body, &env) {
                self.diagnostics.push(
                    Diagnostic::error(
                        "N1306",
                        format!(
                            "not all paths return a value of type `{}`",
                            expected.raw
                        ),
                        body.last()
                            .map(Stmt::span)
                            .cloned()
                            .unwrap_or_else(|| callable_span.clone()),
                    )
                    .with_reason("callables with `-> Type` must return a value on every control-flow path")
                    .with_help("add a return statement after the conditional flow or return from every branch"),
                );
            }
        }
    }

    fn test(&mut self, test: &TestDecl) {
        self.body(&test.body, &[], &[], None, &test.span, Some(test.kind));
    }

    fn body(
        &mut self,
        body: &[Stmt],
        params: &[Param],
        declared_requires: &[String],
        expected_return: Option<&TypeRef>,
        callable_span: &crate::span::Span,
        test_kind: Option<TestKind>,
    ) {
        let mut env = self.base_env();
        let mut granted: HashSet<String> = declared_requires.iter().cloned().collect();

        for param in params {
            env.insert(
                param.name.clone(),
                Binding {
                    ty: Some(param.ty.clone()),
                    labels: param.labels.clone(),
                    mutable: false,
                    uncertain: self.is_uncertain_type(&param.ty),
                    option_checked: false,
                    result_checked: None,
                    secret: self.resolve_aliases(&param.ty).is_secret()
                        || param.labels.privacy == Some(Privacy::Secret),
                },
            );
        }

        self.statements(body, &mut env, &mut granted, expected_return, test_kind);

        if let Some(expected) = expected_return {
            if !self.all_paths_return(body, &env) {
                self.diagnostics.push(
                    Diagnostic::error(
                        "N1306",
                        format!(
                            "not all paths return a value of type `{}`",
                            expected.raw
                        ),
                        body.last()
                            .map(Stmt::span)
                            .cloned()
                            .unwrap_or_else(|| callable_span.clone()),
                    )
                    .with_reason("callables with `-> Type` must return a value on every control-flow path")
                    .with_help("add a return statement after the conditional flow or return from every branch"),
                );
            }
        }
    }

    fn statements(
        &mut self,
        body: &[Stmt],
        env: &mut HashMap<String, Binding>,
        granted: &mut HashSet<String>,
        expected_return: Option<&TypeRef>,
        test_kind: Option<TestKind>,
    ) {
        for stmt in body {
            match stmt {
                Stmt::Let(stmt) => self.let_stmt(stmt, env, granted, expected_return),
                Stmt::Assign(stmt) => self.assign_stmt(stmt, env, granted, expected_return),
                Stmt::Assert(stmt) => self.assert_stmt(stmt, env, granted, expected_return),
                Stmt::ExpectPolicy(stmt) => {
                    self.expect_policy_stmt(stmt, env, granted, expected_return, test_kind)
                }
                Stmt::ExpectWorkflow(stmt) => {
                    self.expect_workflow_stmt(stmt, env, granted, expected_return, test_kind)
                }
                Stmt::ExpectAudit(stmt) => {
                    self.expect_audit_stmt(stmt, env, granted, expected_return, test_kind)
                }
                Stmt::MockAi(stmt) => {
                    self.mock_ai_stmt(stmt, env, granted, expected_return, test_kind)
                }
                Stmt::MockConnector(stmt) => {
                    self.mock_connector_stmt(stmt, env, granted, expected_return, test_kind)
                }
                Stmt::Require(stmt) => {
                    self.known_permission(&stmt.permission, &stmt.span);
                    granted.insert(stmt.permission.clone());
                }
                Stmt::Transaction(stmt) => {
                    self.statements(&stmt.body, env, granted, expected_return, test_kind);
                    if stmt.saga {
                        self.saga_transaction(stmt);
                    }
                }
                Stmt::Scope(stmt) => {
                    let mut scope_env = env.clone();
                    self.statements(
                        &stmt.body,
                        &mut scope_env,
                        granted,
                        expected_return,
                        test_kind,
                    );
                }
                Stmt::If(stmt) => {
                    self.expr(&stmt.condition, env, granted, expected_return, None);
                    let checked_options = self.checked_option_bindings(&stmt.condition, env);
                    let checked_results = self.checked_result_bindings(&stmt.condition, env);
                    let mut then_env = env.clone();
                    for name in checked_options {
                        if let Some(binding) = then_env.get_mut(&name) {
                            binding.option_checked = true;
                        }
                    }
                    for (name, check) in checked_results {
                        if let Some(binding) = then_env.get_mut(&name) {
                            binding.result_checked = Some(check);
                        }
                    }
                    let mut else_env = env.clone();
                    for name in self.else_checked_option_bindings(&stmt.condition, env) {
                        if let Some(binding) = else_env.get_mut(&name) {
                            binding.option_checked = true;
                        }
                    }
                    for (name, check) in self.else_checked_result_bindings(&stmt.condition, env) {
                        if let Some(binding) = else_env.get_mut(&name) {
                            binding.result_checked = Some(check);
                        }
                    }
                    self.statements(
                        &stmt.then_body,
                        &mut then_env,
                        &mut granted.clone(),
                        expected_return,
                        test_kind,
                    );
                    self.statements(
                        &stmt.else_body,
                        &mut else_env,
                        &mut granted.clone(),
                        expected_return,
                        test_kind,
                    );
                }
                Stmt::Match(stmt) => {
                    self.match_stmt(stmt, env, granted, expected_return, test_kind)
                }
                Stmt::Return(expr) => self.return_stmt(expr, env, granted, expected_return),
                Stmt::Expr(expr) => self.expr_stmt(expr, env, granted, expected_return),
            }
        }
    }

    fn base_env(&self) -> HashMap<String, Binding> {
        let mut env = HashMap::new();
        env.insert(
            "current_user".to_string(),
            Binding {
                ty: Some(TypeRef {
                    raw: "Actor".to_string(),
                }),
                labels: Labels::default(),
                mutable: false,
                uncertain: false,
                option_checked: false,
                result_checked: None,
                secret: false,
            },
        );
        env
    }

    fn all_paths_return(&self, body: &[Stmt], env: &HashMap<String, Binding>) -> bool {
        body.iter().any(|stmt| self.stmt_always_returns(stmt, env))
    }

    fn stmt_always_returns(&self, stmt: &Stmt, env: &HashMap<String, Binding>) -> bool {
        match stmt {
            Stmt::Return(_) => true,
            Stmt::If(stmt) => {
                !stmt.then_body.is_empty()
                    && !stmt.else_body.is_empty()
                    && self.all_paths_return(&stmt.then_body, env)
                    && self.all_paths_return(&stmt.else_body, env)
            }
            Stmt::Match(stmt) => {
                !stmt.arms.is_empty()
                    && self.match_is_exhaustive(stmt, env)
                    && stmt
                        .arms
                        .iter()
                        .all(|arm| self.all_paths_return(&arm.body, env))
            }
            Stmt::Transaction(stmt) => self.all_paths_return(&stmt.body, env),
            Stmt::Scope(stmt) => self.all_paths_return(&stmt.body, env),
            Stmt::Let(_)
            | Stmt::Assign(_)
            | Stmt::Assert(_)
            | Stmt::ExpectPolicy(_)
            | Stmt::ExpectWorkflow(_)
            | Stmt::ExpectAudit(_)
            | Stmt::MockAi(_)
            | Stmt::MockConnector(_)
            | Stmt::Require(_)
            | Stmt::Expr(_) => false,
        }
    }

    fn let_stmt(
        &mut self,
        stmt: &LetStmt,
        env: &mut HashMap<String, Binding>,
        granted: &HashSet<String>,
        expected_return: Option<&TypeRef>,
    ) {
        let expr_text = stmt
            .expr
            .as_ref()
            .map(|expr| expr.text.as_str())
            .unwrap_or("");
        let ai_result = is_ai_call(expr_text);
        let ty_uncertain = stmt
            .ty
            .as_ref()
            .is_some_and(|ty| self.is_uncertain_type(ty));
        let explicit_non_uncertain = stmt.ty.as_ref().is_some() && !ty_uncertain;
        let inferred_ty = stmt.expr.as_ref().and_then(|expr| {
            expr::parse(&expr.text)
                .ok()
                .and_then(|parsed| self.expr_type_in_context(&parsed, env, stmt.ty.as_ref()))
        });

        if ai_result && explicit_non_uncertain {
            self.diagnostics.push(
                Diagnostic::error(
                    "N2100",
                    format!("AI result assigned to `{}` must be Uncertain<T>", stmt.name),
                    stmt.span.clone(),
                )
                .with_reason("AI answers are probabilistic and cannot be treated as facts")
                .with_help("use Uncertain<T>, check confidence, then pass result.value"),
            );
        }

        if let Some(expr) = &stmt.expr {
            self.expr(expr, env, granted, expected_return, stmt.ty.as_ref());
        }

        let secret = stmt
            .ty
            .as_ref()
            .is_some_and(|ty| self.resolve_aliases(ty).is_secret())
            || stmt.labels.privacy == Some(Privacy::Secret);
        let binding_ty = stmt.ty.clone().or(inferred_ty.clone());
        let binding_labels = if let Some(expr) = &stmt.expr {
            match expr::parse(&expr.text) {
                Ok(parsed) => {
                    self.trust_assignment(stmt, &parsed, env);
                    self.labels_for_let(&stmt.labels, &parsed, env)
                }
                Err(_) => stmt.labels.clone(),
            }
        } else {
            stmt.labels.clone()
        };

        if let (Some(declared), Some(inferred)) = (&stmt.ty, &inferred_ty) {
            if !self.types_compatible(declared, inferred) {
                self.diagnostics.push(
                    Diagnostic::error(
                        "N1300",
                        format!(
                            "cannot assign `{}` value to `{}` binding `{}`",
                            inferred.raw, declared.raw, stmt.name
                        ),
                        stmt.span.clone(),
                    )
                    .with_reason("connector and function results must match explicit binding types")
                    .with_help("change the binding type or call an expression that returns the expected type"),
                );
            }
        }

        env.insert(
            stmt.name.clone(),
            Binding {
                ty: binding_ty,
                labels: binding_labels,
                mutable: stmt.mutable,
                uncertain: ai_result || ty_uncertain,
                option_checked: false,
                result_checked: None,
                secret,
            },
        );
    }

    fn assign_stmt(
        &mut self,
        stmt: &AssignStmt,
        env: &mut HashMap<String, Binding>,
        granted: &HashSet<String>,
        expected_return: Option<&TypeRef>,
    ) {
        let Some(binding) = env.get(&stmt.name) else {
            self.diagnostics.push(
                Diagnostic::error(
                    "N1305",
                    format!("cannot assign to unknown binding `{}`", stmt.name),
                    stmt.span.clone(),
                )
                .with_reason("assignments must target a local `var` binding")
                .with_help(format!(
                    "declare `var {}` before assigning to it",
                    stmt.name
                )),
            );
            return;
        };

        let expected_expr = binding.ty.clone();
        self.expr(
            &stmt.expr,
            env,
            granted,
            expected_return,
            expected_expr.as_ref(),
        );

        if !binding.mutable {
            self.diagnostics.push(
                Diagnostic::error(
                    "N1304",
                    format!("cannot assign to immutable binding `{}`", stmt.name),
                    stmt.span.clone(),
                )
                .with_reason("only `var` bindings can be reassigned")
                .with_help(format!(
                    "declare `{}` with `var` if mutation is intended",
                    stmt.name
                )),
            );
        }

        let inferred_ty = expr::parse(&stmt.expr.text)
            .ok()
            .and_then(|parsed| self.expr_type_in_context(&parsed, env, expected_expr.as_ref()));
        if let (Some(declared), Some(inferred)) = (&binding.ty, &inferred_ty) {
            if !self.types_compatible(declared, inferred) {
                self.diagnostics.push(
                    Diagnostic::error(
                        "N1300",
                        format!(
                            "cannot assign `{}` value to `{}` binding `{}`",
                            inferred.raw, declared.raw, stmt.name
                        ),
                        stmt.span.clone(),
                    )
                    .with_reason("assignment expressions must match the binding type")
                    .with_help("assign a value with the declared binding type"),
                );
            }
        }

        if is_ai_call(&stmt.expr.text) && binding.ty.as_ref().is_some_and(|ty| !ty.is_uncertain()) {
            self.diagnostics.push(
                Diagnostic::error(
                    "N2100",
                    format!("AI result assigned to `{}` must be Uncertain<T>", stmt.name),
                    stmt.span.clone(),
                )
                .with_reason("AI answers are probabilistic and cannot be treated as facts")
                .with_help("use Uncertain<T>, check confidence, then pass result.value"),
            );
        }

        if binding.ty.is_none() {
            let inferred_labels = expr::parse(&stmt.expr.text)
                .ok()
                .and_then(|parsed| self.expr_labels(&parsed, env));
            if let Some(binding) = env.get_mut(&stmt.name) {
                binding.ty = inferred_ty;
                if let Some(labels) = inferred_labels {
                    binding.labels = merge_labels(&binding.labels, &labels);
                }
                binding.option_checked = false;
                binding.result_checked = None;
            }
        } else if let Some(binding) = env.get_mut(&stmt.name) {
            binding.option_checked = false;
            binding.result_checked = None;
        }
    }
}

impl<'a> Checker<'a> {
    fn expr(
        &mut self,
        expr: &RawExpr,
        env: &HashMap<String, Binding>,
        granted: &HashSet<String>,
        expected_return: Option<&TypeRef>,
        expected_expr: Option<&TypeRef>,
    ) {
        let Some(parsed) = self.parse_expr(expr) else {
            return;
        };
        self.secret_logging(expr, &parsed, env);
        self.uncertain_usage(expr, &parsed, env);
        self.external_privacy(expr, &parsed, env);
        self.trust_flow(expr, &parsed, env);
        self.action_permission(expr, &parsed, granted);
        self.external_namespace(expr, &parsed, env);
        self.connector_call(expr, &parsed, env);
        self.direct_call(expr, &parsed, env);
        self.method_call(expr, &parsed, env);
        self.brand_constructor(expr, &parsed, env, expected_expr);
        self.enum_constructor(expr, &parsed, env, expected_expr);
        self.option_constructor(expr, &parsed, env, expected_expr);
        self.result_constructor(expr, &parsed, env, expected_expr);
        self.try_expr(expr, &parsed, env, expected_return);
        self.async_expr(expr, &parsed, env);
        self.binary_expr(expr, &parsed, env);
        self.field_access(expr, &parsed, env);
    }

    fn expr_stmt(
        &mut self,
        raw: &RawExpr,
        env: &HashMap<String, Binding>,
        granted: &HashSet<String>,
        expected_return: Option<&TypeRef>,
    ) {
        self.expr(raw, env, granted, expected_return, None);
        let Some(parsed) = self.parse_expr(raw) else {
            return;
        };
        if matches!(parsed, Expr::Async(_)) {
            self.diagnostics.push(
                Diagnostic::error(
                    "N2901",
                    "async task is created without an owner",
                    raw.span.clone(),
                )
                .with_reason("structured concurrency requires every async task to be owned")
                .with_help("bind the task with `let task: Task<T> = async ...` or await an existing Task<T>"),
            );
        }
    }

    fn return_stmt(
        &mut self,
        raw: &RawExpr,
        env: &HashMap<String, Binding>,
        granted: &HashSet<String>,
        expected_return: Option<&TypeRef>,
    ) {
        let has_value = !raw.text.trim().is_empty();

        if has_value {
            self.expr(raw, env, granted, expected_return, expected_return);
        }

        match (expected_return, has_value) {
            (Some(expected), true) => {
                let Ok(parsed) = expr::parse(&raw.text) else {
                    return;
                };
                let Some(actual) = self.expr_type_in_context(&parsed, env, Some(expected)) else {
                    return;
                };
                if !self.types_compatible(expected, &actual) {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "N1302",
                            format!(
                                "return value has type `{}`, expected `{}`",
                                actual.raw, expected.raw
                            ),
                            raw.span.clone(),
                        )
                        .with_reason("return expressions must match the callable result type")
                        .with_help("return a value with the declared result type or change the callable signature"),
                    );
                }
            }
            (Some(expected), false) => {
                self.diagnostics.push(
                    Diagnostic::error(
                        "N1302",
                        format!("missing return value of type `{}`", expected.raw),
                        raw.span.clone(),
                    )
                    .with_reason("this callable declares a result type")
                    .with_help("return a value with the declared result type"),
                );
            }
            (None, true) => {
                self.diagnostics.push(
                    Diagnostic::error(
                        "N1303",
                        "unexpected return value from Unit callable",
                        raw.span.clone(),
                    )
                    .with_reason("callables without `-> Type` return Unit")
                    .with_help("remove the return value or declare an explicit result type"),
                );
            }
            (None, false) => {}
        }
    }

    fn assert_stmt(
        &mut self,
        stmt: &AssertStmt,
        env: &HashMap<String, Binding>,
        granted: &HashSet<String>,
        expected_return: Option<&TypeRef>,
    ) {
        self.expr(&stmt.expr, env, granted, expected_return, None);
        let Some(parsed) = self.parse_expr(&stmt.expr) else {
            return;
        };
        let Some(actual) = self.expr_type(&parsed, env) else {
            return;
        };
        if !is_bool_type(&actual) {
            self.diagnostics.push(
                Diagnostic::error(
                    "N3100",
                    format!("assert expression must be Bool, found `{}`", actual.raw),
                    stmt.span.clone(),
                )
                .with_reason("tests can only assert boolean conditions")
                .with_help("compare values explicitly, for example `assert total == expected`"),
            );
        }
    }

    fn expect_policy_stmt(
        &mut self,
        stmt: &ExpectPolicyStmt,
        env: &HashMap<String, Binding>,
        granted: &HashSet<String>,
        expected_return: Option<&TypeRef>,
        test_kind: Option<TestKind>,
    ) {
        if test_kind != Some(TestKind::Policy) {
            self.diagnostics.push(
                Diagnostic::error(
                    "N3103",
                    "policy expectations are only allowed inside `test policy` blocks",
                    stmt.span.clone(),
                )
                .with_reason("expect_deny and expect_allow invert policy diagnostics")
                .with_help("wrap the expectation in `test policy \"name\" { ... }`"),
            );
        }

        let start = self.diagnostics.len();
        let mut inner_env = env.clone();
        let mut inner_granted = granted.clone();
        self.statements(
            &stmt.body,
            &mut inner_env,
            &mut inner_granted,
            expected_return,
            test_kind,
        );
        let diagnostics = self.diagnostics.split_off(start);
        let mut saw_policy_denial = false;
        for diagnostic in diagnostics {
            if diagnostic.code == "N2400" {
                saw_policy_denial = true;
            } else {
                self.diagnostics.push(diagnostic);
            }
        }

        match (stmt.outcome, saw_policy_denial) {
            (PolicyExpectation::Deny, true) | (PolicyExpectation::Allow, false) => {}
            (PolicyExpectation::Deny, false) => {
                self.diagnostics.push(
                    Diagnostic::error(
                        "N3101",
                        "expected policy denial, but the block was allowed",
                        stmt.span.clone(),
                    )
                    .with_reason("expect_deny requires at least one blocked data flow")
                    .with_help(
                        "send protected data to the target being tested or tighten the policy",
                    ),
                );
            }
            (PolicyExpectation::Allow, true) => {
                self.diagnostics.push(
                    Diagnostic::error(
                        "N3102",
                        "expected policy allow, but the block was denied",
                        stmt.span.clone(),
                    )
                    .with_reason(
                        "expect_allow requires the block to avoid policy-denial diagnostics",
                    )
                    .with_help(
                        "add an explicit allow policy or remove protected data from the flow",
                    ),
                );
            }
        }
    }

    fn expect_workflow_stmt(
        &mut self,
        stmt: &ExpectWorkflowStmt,
        env: &HashMap<String, Binding>,
        granted: &HashSet<String>,
        expected_return: Option<&TypeRef>,
        test_kind: Option<TestKind>,
    ) {
        if test_kind != Some(TestKind::Workflow) {
            self.diagnostics.push(
                Diagnostic::error(
                    "N3104",
                    "workflow expectations are only allowed inside `test workflow` blocks",
                    stmt.span.clone(),
                )
                .with_reason(
                    "expect_workflow_success and expect_workflow_failure execute workflow scenarios",
                )
                .with_help("wrap the expectation in `test workflow \"name\" { ... }`"),
            );
        }

        self.expr(&stmt.call, env, granted, expected_return, None);

        let Some(parsed) = self.parse_expr(&stmt.call) else {
            return;
        };
        let Some(call_name) = direct_workflow_expectation_call_name(&parsed) else {
            self.diagnostics.push(
                Diagnostic::error(
                    "N3105",
                    "workflow expectation must be a direct workflow call",
                    stmt.span.clone(),
                )
                .with_reason("workflow tests need an explicit scenario entrypoint")
                .with_help(
                    "call a workflow directly, for example `expect_workflow_success refund(request)`",
                ),
            );
            return;
        };

        if self
            .callable_signatures
            .get(call_name)
            .is_some_and(|signature| signature.kind == CallableKind::Workflow)
        {
            return;
        }

        self.diagnostics.push(
            Diagnostic::error(
                "N3105",
                format!("`{call_name}` is not a workflow"),
                stmt.span.clone(),
            )
            .with_reason("workflow expectations can only execute `workflow` declarations")
            .with_help("replace the call with a declared workflow"),
        );
    }

    fn expect_audit_stmt(
        &mut self,
        stmt: &ExpectAuditStmt,
        env: &HashMap<String, Binding>,
        granted: &HashSet<String>,
        expected_return: Option<&TypeRef>,
        test_kind: Option<TestKind>,
    ) {
        if test_kind != Some(TestKind::Workflow) {
            self.diagnostics.push(
                Diagnostic::error(
                    "N3117",
                    "audit expectations are only allowed inside `test workflow` blocks",
                    stmt.span.clone(),
                )
                .with_reason("expect_audit checks workflow audit side effects")
                .with_help("wrap the expectation in `test workflow \"name\" { ... }`"),
            );
        }

        self.expr(&stmt.event, env, granted, expected_return, None);
    }

    fn mock_ai_stmt(
        &mut self,
        stmt: &MockAiStmt,
        env: &HashMap<String, Binding>,
        granted: &HashSet<String>,
        expected_return: Option<&TypeRef>,
        test_kind: Option<TestKind>,
    ) {
        if test_kind != Some(TestKind::Ai) {
            self.diagnostics.push(
                Diagnostic::error(
                    "N3106",
                    "AI mocks are only allowed inside `test ai` blocks",
                    stmt.span.clone(),
                )
                .with_reason("mock_ai defines deterministic AI responses for AI tests")
                .with_help("wrap the mock in `test ai \"name\" { ... }`"),
            );
        }

        self.expr(&stmt.call, env, granted, expected_return, None);
        self.expr(&stmt.value, env, granted, expected_return, None);
        self.expr(&stmt.confidence, env, granted, expected_return, None);

        let Some(call_expr) = self.parse_expr(&stmt.call) else {
            return;
        };
        let Some(path) = direct_call_path(&call_expr) else {
            self.diagnostics.push(
                Diagnostic::error(
                    "N3107",
                    "AI mock target must be a direct `ai.*` call",
                    stmt.span.clone(),
                )
                .with_reason("AI tests need an explicit connector method to override")
                .with_help(
                    "mock a call such as `mock_ai ai.classify(message) => Intent confidence 0.9`",
                ),
            );
            return;
        };

        let ["ai", method] = path.as_slice() else {
            self.diagnostics.push(
                Diagnostic::error(
                    "N3107",
                    "AI mock target must be a direct `ai.*` call",
                    stmt.span.clone(),
                )
                .with_reason("mock_ai is scoped to AI connector calls")
                .with_help("use an `ai.<method>(...)` connector call"),
            );
            return;
        };

        let Some(method_decl) = self
            .connector_methods
            .get("ai")
            .and_then(|methods| methods.get(*method))
        else {
            self.diagnostics.push(
                Diagnostic::error(
                    "N3108",
                    format!("AI connector has no method `{method}`"),
                    stmt.span.clone(),
                )
                .with_reason("AI mocks must target declared AI connector methods")
                .with_help(format!(
                    "add `{method}(...) -> Uncertain<T>` to connector `ai`"
                )),
            );
            return;
        };

        let Some(result) = &method_decl.result else {
            self.diagnostics.push(
                Diagnostic::error(
                    "N3109",
                    format!("AI connector method `{method}` must return `Uncertain<T>`"),
                    stmt.span.clone(),
                )
                .with_reason("AI mock values are wrapped with a deterministic confidence")
                .with_help("change the connector method result to `Uncertain<...>`"),
            );
            return;
        };

        let resolved_result = self.resolve_aliases(result);
        if !self.is_uncertain_type(&resolved_result) {
            self.diagnostics.push(
                Diagnostic::error(
                    "N3109",
                    format!("AI connector method `{method}` must return `Uncertain<T>`"),
                    stmt.span.clone(),
                )
                .with_reason("AI mock values are wrapped with a deterministic confidence")
                .with_help("change the connector method result to `Uncertain<...>`"),
            );
            return;
        }

        if let Some(inner_raw) = generic_arg(&resolved_result.raw) {
            let expected_value = TypeRef { raw: inner_raw };
            if let Ok(value_expr) = expr::parse(&stmt.value.text) {
                if let Some(actual_value) =
                    self.expr_type_in_context(&value_expr, env, Some(&expected_value))
                {
                    if !self.types_compatible(&expected_value, &actual_value) {
                        self.diagnostics.push(
                            Diagnostic::error(
                                "N3110",
                                format!(
                                    "AI mock value has type `{}`, expected `{}`",
                                    actual_value.raw, expected_value.raw
                                ),
                                stmt.span.clone(),
                            )
                            .with_reason("mock_ai value must match the inner type of Uncertain<T>")
                            .with_help(
                                "return a mock value compatible with the AI connector result",
                            ),
                        );
                    }
                }
            }
        }

        if let Ok(confidence_expr) = expr::parse(&stmt.confidence.text) {
            if let Some(confidence_ty) = self.expr_type(&confidence_expr, env) {
                if !matches!(confidence_ty.raw.as_str(), "Float" | "Int" | "Decimal") {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "N3111",
                            format!(
                                "AI mock confidence must be numeric, found `{}`",
                                confidence_ty.raw
                            ),
                            stmt.span.clone(),
                        )
                        .with_reason("Uncertain<T> confidence is a numeric runtime value")
                        .with_help("use a numeric confidence such as `0.91`"),
                    );
                }
            }
        }
    }

    fn mock_connector_stmt(
        &mut self,
        stmt: &MockConnectorStmt,
        env: &HashMap<String, Binding>,
        granted: &HashSet<String>,
        expected_return: Option<&TypeRef>,
        test_kind: Option<TestKind>,
    ) {
        if test_kind != Some(TestKind::Workflow) {
            self.diagnostics.push(
                Diagnostic::error(
                    "N3112",
                    "connector mocks are only allowed inside `test workflow` blocks",
                    stmt.span.clone(),
                )
                .with_reason(
                    "mock_connector defines deterministic external dependencies for workflow tests",
                )
                .with_help("wrap the mock in `test workflow \"name\" { ... }`"),
            );
        }

        self.expr(&stmt.call, env, granted, expected_return, None);
        self.expr(&stmt.value, env, granted, expected_return, None);

        let Some(call_expr) = self.parse_expr(&stmt.call) else {
            return;
        };
        let Some(path) = direct_call_path(&call_expr) else {
            self.diagnostics.push(
                Diagnostic::error(
                    "N3113",
                    "connector mock target must be a direct connector method call",
                    stmt.span.clone(),
                )
                .with_reason("workflow fixtures need an explicit connector method to override")
                .with_help("mock a call such as `mock_connector reports.render(id) => \"ok\"`"),
            );
            return;
        };

        let [namespace, method] = path.as_slice() else {
            self.diagnostics.push(
                Diagnostic::error(
                    "N3113",
                    "connector mock target must be a direct connector method call",
                    stmt.span.clone(),
                )
                .with_reason("mock_connector is scoped to declared connector methods")
                .with_help("use a two-part connector call such as `connector.method(...)`"),
            );
            return;
        };

        if *namespace == "ai" {
            self.diagnostics.push(
                Diagnostic::error(
                    "N3113",
                    "AI connector mocks must use `mock_ai`",
                    stmt.span.clone(),
                )
                .with_reason("AI mocks carry an explicit confidence value")
                .with_help("use `mock_ai ai.method(...) => Value confidence 0.9`"),
            );
            return;
        }

        let Some(method_decl) = self
            .connector_methods
            .get(*namespace)
            .and_then(|methods| methods.get(*method))
        else {
            self.diagnostics.push(
                Diagnostic::error(
                    "N3114",
                    format!("connector `{namespace}` has no method `{method}`"),
                    stmt.span.clone(),
                )
                .with_reason("connector mocks must target declared connector methods")
                .with_help(format!(
                    "add `{method}(...) -> ...` to connector `{namespace}`"
                )),
            );
            return;
        };

        let Some(result) = &method_decl.result else {
            self.diagnostics.push(
                Diagnostic::error(
                    "N3115",
                    format!("connector method `{namespace}.{method}` has no result to mock"),
                    stmt.span.clone(),
                )
                .with_reason("mock_connector provides a deterministic return value")
                .with_help("mock a connector method with an explicit result type"),
            );
            return;
        };

        if result.raw == "Unit" {
            self.diagnostics.push(
                Diagnostic::error(
                    "N3115",
                    format!("connector method `{namespace}.{method}` returns Unit"),
                    stmt.span.clone(),
                )
                .with_reason("Unit connector calls have no value to replace")
                .with_help("mock a connector method with a non-Unit result"),
            );
            return;
        }

        if let Ok(value_expr) = expr::parse(&stmt.value.text) {
            if let Some(actual_value) = self.expr_type_in_context(&value_expr, env, Some(result)) {
                if !self.types_compatible(result, &actual_value) {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "N3116",
                            format!(
                                "connector mock value has type `{}`, expected `{}`",
                                actual_value.raw, result.raw
                            ),
                            stmt.span.clone(),
                        )
                        .with_reason("mock_connector value must match the connector result type")
                        .with_help("return a mock value compatible with the connector schema"),
                    );
                }
            }
        }
    }

    fn parse_expr(&mut self, raw: &RawExpr) -> Option<Expr> {
        if raw.text.trim().is_empty() {
            return None;
        }

        match expr::parse(&raw.text) {
            Ok(expr) => Some(expr),
            Err(error) => {
                self.diagnostics.push(
                    Diagnostic::error(
                        "N3000",
                        format!("unsupported expression `{}`", raw.text),
                        raw.span.clone(),
                    )
                    .with_reason(error.message)
                    .with_help("use calls, member access, literals, boolean operators, or comparisons supported by the current expression parser"),
                );
                None
            }
        }
    }

    fn secret_logging(&mut self, raw: &RawExpr, expr: &Expr, env: &HashMap<String, Binding>) {
        if expr.direct_call_name() != Some("log") {
            return;
        }

        for (name, binding) in env {
            if binding.secret && expr.contains_ident(name) {
                self.diagnostics.push(
                    Diagnostic::error(
                        "N2200",
                        format!("secret `{name}` cannot be logged"),
                        raw.span.clone(),
                    )
                    .with_reason("secrets must not enter ordinary logs or audit payloads")
                    .with_help("log a stable key id or redacted fingerprint instead"),
                );
            }
        }
    }

    fn uncertain_usage(&mut self, raw: &RawExpr, expr: &Expr, env: &HashMap<String, Binding>) {
        if expr.contains_member_field("confidence")
            || expr.contains_member_field("value")
            || expr.contains_call_path(&["require_human_review"])
            || expr.contains_call_path(&["require_human_approval"])
        {
            return;
        }

        for (name, binding) in env {
            if binding.uncertain && expr.contains_ident(name) {
                self.diagnostics.push(
                    Diagnostic::error(
                        "N2300",
                        format!("uncertain value `{name}` used without confidence handling"),
                        raw.span.clone(),
                    )
                    .with_reason("Uncertain<T> cannot be used where a verified T is expected")
                    .with_help("check confidence and pass value.value only in the accepted branch"),
                );
            }
        }
    }

    fn external_privacy(&mut self, raw: &RawExpr, expr: &Expr, env: &HashMap<String, Binding>) {
        for call in expr.calls() {
            let Some(targets) = self.privacy_policy_targets(call.path.as_slice()) else {
                continue;
            };
            for arg in call.args {
                let Some(labels) = self.expr_labels(arg, env) else {
                    continue;
                };
                let privacy = labels.privacy;
                let trust = labels.trust;
                let source = labels.source.as_deref();
                if !matches!(
                    privacy,
                    Some(
                        Privacy::Private
                            | Privacy::Sensitive
                            | Privacy::Secret
                            | Privacy::Regulated
                    )
                ) {
                    continue;
                }

                if !self.policies.is_data_flow_allowed_to_any_for_tenant(
                    privacy,
                    trust,
                    source,
                    &targets,
                    self.policy_tenant.as_deref(),
                ) {
                    self.diagnostics.push(
                    Diagnostic::error(
                        "N2400",
                            format!(
                                "private data `{}` cannot be sent to `{}`",
                                expr_label(arg),
                                call.path.join(".")
                            ),
                        raw.span.clone(),
                    )
                        .with_reason("no allow policy permits this privacy/provenance/trust flow")
                        .with_help(format!(
                            "anonymize the value, send only a derived public value, or add an explicit policy for `{}`",
                            targets.last().unwrap_or(&targets[0])
                        )),
                );
                }
            }
        }
    }

    fn privacy_policy_targets(&self, path: &[&str]) -> Option<Vec<String>> {
        if path.first() == Some(&"external") {
            return Some(external_policy_targets(path));
        }
        let namespace = path.first()?;
        self.connector_methods
            .contains_key(*namespace)
            .then(|| connector_policy_targets(path))
    }

    fn saga_transaction(&mut self, stmt: &TransactionStmt) {
        for expr in flatten_exprs(&stmt.body) {
            let Ok(parsed) = expr::parse(&expr.text) else {
                continue;
            };
            let Some(action_name) = parsed.direct_call_name() else {
                continue;
            };
            if self
                .action_risks
                .get(action_name)
                .is_some_and(|risk| *risk >= Risk::High)
            {
                let rollback = self.module.declarations.iter().find_map(|decl| match decl {
                    Declaration::Action(action) if action.name == action_name => {
                        action.rollback.as_ref()
                    }
                    _ => None,
                });
                if rollback.is_none() {
                    self.diagnostics.push(
                        Diagnostic::warning(
                            "N2600",
                            format!("saga calls high-risk action `{action_name}` without rollback metadata"),
                            expr.span.clone(),
                        )
                        .with_help("add rollback metadata to the action declaration"),
                    );
                }
            }
        }
    }

    fn known_permission(&mut self, permission: &str, span: &crate::span::Span) {
        if !self.permissions.contains(permission) {
            self.diagnostics.push(
                Diagnostic::error(
                    "N1101",
                    format!("unknown permission `{permission}`"),
                    span.clone(),
                )
                .with_help("declare the permission at module level"),
            );
        }
    }

    fn known_type_ref(&mut self, ty: &TypeRef, span: &crate::span::Span) {
        self.known_type_ref_with_generics(ty, span, &[]);
    }

    fn known_type_ref_with_generics(
        &mut self,
        ty: &TypeRef,
        span: &crate::span::Span,
        generic_params: &[String],
    ) {
        for name in type_names(ty) {
            if generic_params.iter().any(|param| param == &name) {
                continue;
            }
            if self.types.contains(&name) {
                continue;
            }
            if builtins::symbol(&name)
                .is_some_and(|symbol| symbol.kind == builtins::BuiltinKind::Currency)
            {
                continue;
            }

            self.diagnostics.push(
                Diagnostic::error("N1201", format!("unknown type `{name}`"), span.clone())
                    .with_reason("num uses explicit module-scoped type declarations")
                    .with_help(format!(
                        "declare `type {name} {{ ... }}` or use a built-in type"
                    )),
            );
        }
        self.type_ref_arity(ty, span, generic_params);
    }

    fn type_ref_arity(
        &mut self,
        ty: &TypeRef,
        span: &crate::span::Span,
        generic_params: &[String],
    ) {
        for reference in type_references(&ty.raw) {
            if generic_params.iter().any(|param| param == &reference.name) {
                if !reference.args.is_empty() {
                    self.diagnostics.push(type_arity_diagnostic(
                        &reference.name,
                        0,
                        reference.args.len(),
                        span,
                    ));
                }
                continue;
            }

            let Some(expected) = self.type_arities.get(&reference.name).copied() else {
                continue;
            };
            if expected != reference.args.len() {
                self.diagnostics.push(type_arity_diagnostic(
                    &reference.name,
                    expected,
                    reference.args.len(),
                    span,
                ));
            }
        }
    }

    pub(super) fn types_compatible(&self, expected: &TypeRef, actual: &TypeRef) -> bool {
        let mut seen = HashSet::new();
        self.types_compatible_inner(expected, actual, &mut seen)
    }

    fn types_compatible_inner(
        &self,
        expected: &TypeRef,
        actual: &TypeRef,
        seen: &mut HashSet<(String, String)>,
    ) -> bool {
        if expected.raw == actual.raw {
            return true;
        }

        if !seen.insert((expected.raw.clone(), actual.raw.clone())) {
            return false;
        }

        let expected_union = union_members(&expected.raw);
        if expected_union.len() > 1 {
            return expected_union
                .iter()
                .map(|raw| TypeRef { raw: raw.clone() })
                .any(|member| self.types_compatible_inner(&member, actual, seen));
        }

        let actual_union = union_members(&actual.raw);
        if actual_union.len() > 1 {
            return actual_union
                .iter()
                .map(|raw| TypeRef { raw: raw.clone() })
                .all(|member| self.types_compatible_inner(expected, &member, seen));
        }

        if let Some(expanded_expected) = self.expand_alias(expected) {
            if self.types_compatible_inner(&expanded_expected, actual, seen) {
                return true;
            }
        }

        if let Some(expanded_actual) = self.expand_alias(actual) {
            if self.types_compatible_inner(expected, &expanded_actual, seen) {
                return true;
            }
        }

        false
    }

    fn expand_alias(&self, ty: &TypeRef) -> Option<TypeRef> {
        let base = type_base_name(&ty.raw);
        let alias = self.type_aliases.get(&base)?;
        if alias.nominal {
            return None;
        }

        let args = generic_args(&ty.raw);
        if args.len() != alias.generic_params.len() {
            return None;
        }
        let substitutions = type_param_substitutions(&alias.generic_params, &args);
        Some(substitute_type_params(&alias.target, &substitutions))
    }

    pub(super) fn resolve_aliases(&self, ty: &TypeRef) -> TypeRef {
        let mut current = ty.clone();
        let mut seen = HashSet::new();
        while seen.insert(current.raw.clone()) {
            let Some(expanded) = self.expand_alias(&current) else {
                break;
            };
            current = expanded;
        }
        current
    }

    pub(super) fn is_option_type(&self, ty: &TypeRef) -> bool {
        self.resolve_aliases(ty).is_option()
    }

    pub(super) fn is_result_type(&self, ty: &TypeRef) -> bool {
        self.resolve_aliases(ty).is_result()
    }

    pub(super) fn is_uncertain_type(&self, ty: &TypeRef) -> bool {
        self.resolve_aliases(ty).is_uncertain()
    }

    pub(super) fn is_task_type(&self, ty: &TypeRef) -> bool {
        self.resolve_aliases(ty).is_task()
    }

    pub(super) fn option_inner_type(&self, ty: &TypeRef) -> Option<TypeRef> {
        generic_arg(&self.resolve_aliases(ty).raw).map(|raw| TypeRef { raw })
    }

    pub(super) fn task_inner_type(&self, ty: &TypeRef) -> Option<TypeRef> {
        generic_arg(&self.resolve_aliases(ty).raw).map(|raw| TypeRef { raw })
    }

    pub(super) fn result_ok_type(&self, ty: &TypeRef) -> Option<TypeRef> {
        result_ok_type(&self.resolve_aliases(ty))
    }

    pub(super) fn result_err_type(&self, ty: &TypeRef) -> Option<TypeRef> {
        result_err_type(&self.resolve_aliases(ty))
    }

    fn try_expr(
        &mut self,
        raw: &RawExpr,
        expr: &Expr,
        env: &HashMap<String, Binding>,
        expected_return: Option<&TypeRef>,
    ) {
        match expr {
            Expr::Try(inner) => {
                self.try_expr(raw, inner, env, expected_return);

                let Some(inner_ty) = self.expr_type(inner, env) else {
                    return;
                };
                if !self.is_result_type(&inner_ty) {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "N2303",
                            format!("`?` cannot unwrap non-Result type `{}`", inner_ty.raw),
                            raw.span.clone(),
                        )
                        .with_reason("the `?` operator only propagates Result<T,E> errors")
                        .with_help(
                            "use `?` on a Result<T,E> expression or handle the value explicitly",
                        ),
                    );
                    return;
                }

                let Some(expected) = expected_return else {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "N2304",
                            "`?` requires the enclosing callable to return Result<_,E>",
                            raw.span.clone(),
                        )
                        .with_reason("error propagation needs a compatible Result return type")
                        .with_help("change the callable result type to Result<_,E> or handle the error explicitly"),
                    );
                    return;
                };

                if !self.is_result_type(expected) {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "N2304",
                            format!(
                                "`?` cannot be used in callable returning `{}`",
                                expected.raw
                            ),
                            raw.span.clone(),
                        )
                        .with_reason("error propagation needs a compatible Result return type")
                        .with_help("change the callable result type to Result<_,E> or handle the error explicitly"),
                    );
                    return;
                }

                let inner_err = self.result_err_type(&inner_ty);
                let expected_err = self.result_err_type(expected);
                if let (Some(inner_err), Some(expected_err)) = (inner_err, expected_err) {
                    if !self.types_compatible(&inner_err, &expected_err) {
                        self.diagnostics.push(
                            Diagnostic::error(
                                "N2304",
                                format!(
                                    "`?` propagates error `{}`, but enclosing callable returns error `{}`",
                                    inner_err.raw, expected_err.raw
                                ),
                                raw.span.clone(),
                            )
                            .with_reason("Result<T,E>? can only propagate the same error type E")
                            .with_help("use matching Result error types or convert the error before propagation"),
                        );
                    }
                }
            }
            Expr::Call { callee, args } => {
                self.try_expr(raw, callee, env, expected_return);
                for arg in args {
                    self.try_expr(raw, arg, env, expected_return);
                }
            }
            Expr::Member { object, .. } => self.try_expr(raw, object, env, expected_return),
            Expr::Binary { left, right, .. } => {
                self.try_expr(raw, left, env, expected_return);
                self.try_expr(raw, right, env, expected_return);
            }
            Expr::Object(fields) => {
                for field in fields {
                    self.try_expr(raw, &field.value, env, expected_return);
                }
            }
            Expr::Async(inner) | Expr::Await(inner) => {
                self.try_expr(raw, inner, env, expected_return)
            }
            Expr::Quantity(_, _)
            | Expr::Ident(_)
            | Expr::String(_)
            | Expr::Bool(_)
            | Expr::Int(_)
            | Expr::Float(_) => {}
        }
    }

    fn async_expr(&mut self, raw: &RawExpr, expr: &Expr, env: &HashMap<String, Binding>) {
        match expr {
            Expr::Await(inner) => {
                self.async_expr(raw, inner, env);

                let Some(inner_ty) = self.expr_type(inner, env) else {
                    return;
                };
                if !self.is_task_type(&inner_ty) {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "N2900",
                            format!("`await` cannot unwrap non-Task type `{}`", inner_ty.raw),
                            raw.span.clone(),
                        )
                        .with_reason("await only resolves Task<T> values created by async work")
                        .with_help("use `await` on a Task<T> binding or remove `await` from this expression"),
                    );
                }
            }
            Expr::Async(inner) => self.async_expr(raw, inner, env),
            Expr::Try(inner) => self.async_expr(raw, inner, env),
            Expr::Call { callee, args } => {
                self.async_expr(raw, callee, env);
                for arg in args {
                    self.async_expr(raw, arg, env);
                }
            }
            Expr::Member { object, .. } => self.async_expr(raw, object, env),
            Expr::Binary { left, right, .. } => {
                self.async_expr(raw, left, env);
                self.async_expr(raw, right, env);
            }
            Expr::Object(fields) => {
                for field in fields {
                    self.async_expr(raw, &field.value, env);
                }
            }
            Expr::Quantity(_, _)
            | Expr::Ident(_)
            | Expr::String(_)
            | Expr::Bool(_)
            | Expr::Int(_)
            | Expr::Float(_) => {}
        }
    }
}

fn external_policy_targets(path: &[&str]) -> Vec<String> {
    let mut targets = vec!["ExternalApi".to_string()];
    if path.len() >= 2 {
        targets.push(path[..2].join("."));
    }
    if path.len() >= 3 {
        targets.push(path.join("."));
    }
    targets
}

fn connector_policy_targets(path: &[&str]) -> Vec<String> {
    let mut targets = vec!["ConnectorApi".to_string()];
    if let Some(namespace) = path.first() {
        targets.push((*namespace).to_string());
    }
    if path.len() >= 2 {
        targets.push(path.join("."));
    }
    targets
}

fn expr_label(expr: &Expr) -> String {
    match expr {
        Expr::Ident(name) => name.clone(),
        Expr::Member { object, field } => format!("{}.{}", expr_label(object), field),
        Expr::Call { callee, .. } => expr_label(callee),
        Expr::Try(inner) | Expr::Async(inner) | Expr::Await(inner) => expr_label(inner),
        Expr::Binary { .. } => "expression".to_string(),
        Expr::Object(_) => "object".to_string(),
        Expr::String(_) | Expr::Bool(_) | Expr::Int(_) | Expr::Float(_) | Expr::Quantity(_, _) => {
            "literal".to_string()
        }
    }
}

fn is_ai_call(text: &str) -> bool {
    compact_member_access(text).contains("ai.")
}

fn contains_audit(body: &[Stmt]) -> bool {
    flatten_exprs(body)
        .into_iter()
        .any(|expr| expr.text.starts_with("audit") || expr.text.contains(" audit "))
}

fn flatten_exprs(body: &[Stmt]) -> Vec<&RawExpr> {
    let mut exprs = Vec::new();
    for stmt in body {
        match stmt {
            Stmt::Let(stmt) => {
                if let Some(expr) = &stmt.expr {
                    exprs.push(expr);
                }
            }
            Stmt::Assign(stmt) => exprs.push(&stmt.expr),
            Stmt::Assert(stmt) => exprs.push(&stmt.expr),
            Stmt::ExpectPolicy(stmt) => exprs.extend(flatten_exprs(&stmt.body)),
            Stmt::ExpectWorkflow(stmt) => exprs.push(&stmt.call),
            Stmt::ExpectAudit(stmt) => exprs.push(&stmt.event),
            Stmt::MockAi(stmt) => {
                exprs.push(&stmt.call);
                exprs.push(&stmt.value);
                exprs.push(&stmt.confidence);
            }
            Stmt::MockConnector(stmt) => {
                exprs.push(&stmt.call);
                exprs.push(&stmt.value);
            }
            Stmt::Require(_) => {}
            Stmt::Transaction(stmt) => exprs.extend(flatten_exprs(&stmt.body)),
            Stmt::Scope(stmt) => exprs.extend(flatten_exprs(&stmt.body)),
            Stmt::If(stmt) => {
                exprs.push(&stmt.condition);
                exprs.extend(flatten_exprs(&stmt.then_body));
                exprs.extend(flatten_exprs(&stmt.else_body));
            }
            Stmt::Match(stmt) => {
                exprs.push(&stmt.expr);
                for arm in &stmt.arms {
                    if let Some(guard) = &arm.guard {
                        exprs.push(guard);
                    }
                    exprs.extend(flatten_exprs(&arm.body));
                }
            }
            Stmt::Return(expr) | Stmt::Expr(expr) => exprs.push(expr),
        }
    }
    exprs
}

fn compact_member_access(text: &str) -> String {
    text.replace(" . ", ".")
        .replace(". ", ".")
        .replace(" .", ".")
}

fn builtin_type_names() -> HashSet<String> {
    [
        "Text",
        "Int",
        "Float",
        "Decimal",
        "Bool",
        "Date",
        "DateTime",
        "Duration",
        "Uuid",
        "Email",
        "PhoneNumber",
        "Url",
        "Json",
        "Actor",
        "Bytes",
        "Result",
        "Option",
        "Task",
        "List",
        "Map",
        "Set",
        "Brand",
        "Money",
        "Secret",
        "Uncertain",
        "Document",
        "Pdf",
        "Docx",
        "Image",
        "Unit",
        "Distance",
        "Speed",
        "Kilometer",
        "Hour",
        "KilometersPerHour",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn builtin_type_arities() -> HashMap<String, usize> {
    [
        ("Text", 0),
        ("Int", 0),
        ("Float", 0),
        ("Decimal", 0),
        ("Bool", 0),
        ("Date", 0),
        ("DateTime", 0),
        ("Duration", 1),
        ("Uuid", 0),
        ("Email", 0),
        ("PhoneNumber", 0),
        ("Url", 0),
        ("Json", 0),
        ("Actor", 0),
        ("Bytes", 0),
        ("Result", 2),
        ("Option", 1),
        ("Task", 1),
        ("List", 1),
        ("Map", 2),
        ("Set", 1),
        ("Brand", 2),
        ("Money", 1),
        ("Secret", 1),
        ("Uncertain", 1),
        ("Document", 0),
        ("Pdf", 0),
        ("Docx", 0),
        ("Image", 0),
        ("Unit", 0),
        ("Distance", 1),
        ("Speed", 1),
        ("Kilometer", 0),
        ("Hour", 0),
        ("KilometersPerHour", 0),
    ]
    .into_iter()
    .map(|(name, arity)| (name.to_string(), arity))
    .collect()
}

fn type_names(ty: &TypeRef) -> Vec<String> {
    strip_string_literals(&ty.raw)
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

fn strip_string_literals(raw: &str) -> String {
    let mut stripped = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '"' {
            while let Some(inner) = chars.next() {
                if inner == '\\' {
                    chars.next();
                    continue;
                }
                if inner == '"' {
                    break;
                }
            }
            continue;
        }
        stripped.push(ch);
    }
    stripped
}

#[derive(Debug, Clone)]
struct TypeReference {
    name: String,
    args: Vec<String>,
}

fn type_references(raw: &str) -> Vec<TypeReference> {
    let chars: Vec<char> = raw.chars().collect();
    let mut refs = Vec::new();
    collect_type_references(&chars, &mut refs);
    refs
}

fn type_base_name(raw: &str) -> String {
    raw.trim()
        .split_once('<')
        .map(|(base, _)| base.trim())
        .unwrap_or_else(|| raw.trim())
        .to_string()
}

fn is_brand_type(ty: &TypeRef) -> bool {
    type_base_name(&ty.raw) == "Brand"
}

fn union_members(raw: &str) -> Vec<String> {
    let mut members = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    let chars: Vec<(usize, char)> = raw.char_indices().collect();
    let mut index = 0usize;
    while index < chars.len() {
        let (offset, ch) = chars[index];
        match ch {
            '"' => {
                index += 1;
                while index < chars.len() {
                    match chars[index].1 {
                        '\\' => index += 2,
                        '"' => {
                            index += 1;
                            break;
                        }
                        _ => index += 1,
                    }
                }
                continue;
            }
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            '|' if depth == 0 => {
                let member = raw[start..offset].trim();
                if !member.is_empty() {
                    members.push(member.to_string());
                }
                start = offset + ch.len_utf8();
            }
            _ => {}
        }
        index += 1;
    }

    let member = raw[start..].trim();
    if !member.is_empty() {
        members.push(member.to_string());
    }
    members
}

fn type_param_substitutions(params: &[String], args: &[String]) -> HashMap<String, String> {
    params.iter().cloned().zip(args.iter().cloned()).collect()
}

fn substitute_type_params(ty: &TypeRef, substitutions: &HashMap<String, String>) -> TypeRef {
    if substitutions.is_empty() {
        return ty.clone();
    }

    let chars: Vec<char> = ty.raw.chars().collect();
    let mut index = 0usize;
    let mut raw = String::with_capacity(ty.raw.len());
    while index < chars.len() {
        match chars[index] {
            '"' => {
                raw.push(chars[index]);
                index += 1;
                while index < chars.len() {
                    raw.push(chars[index]);
                    if chars[index] == '\\' {
                        index += 1;
                        if index < chars.len() {
                            raw.push(chars[index]);
                        }
                    } else if chars[index] == '"' {
                        index += 1;
                        break;
                    }
                    index += 1;
                }
            }
            ch if is_type_ident_start(ch) => {
                let start = index;
                index += 1;
                while index < chars.len() && is_type_ident_continue(chars[index]) {
                    index += 1;
                }
                let ident = chars[start..index].iter().collect::<String>();
                raw.push_str(
                    substitutions
                        .get(&ident)
                        .map(String::as_str)
                        .unwrap_or(&ident),
                );
            }
            ch => {
                raw.push(ch);
                index += 1;
            }
        }
    }

    TypeRef { raw }
}

fn collect_type_references(chars: &[char], refs: &mut Vec<TypeReference>) {
    let mut index = 0usize;
    while index < chars.len() {
        match chars[index] {
            '"' => {
                index = skip_string_literal(chars, index + 1);
            }
            ch if is_type_ident_start(ch) => {
                let name_start = index;
                index += 1;
                while index < chars.len() && is_type_ident_continue(chars[index]) {
                    index += 1;
                }
                let name = chars[name_start..index].iter().collect::<String>();

                while index < chars.len() && chars[index].is_whitespace() {
                    index += 1;
                }

                let mut args = Vec::new();
                if index < chars.len() && chars[index] == '<' {
                    let (inner, next_index) = generic_inner(chars, index);
                    let synthetic = format!("Synthetic<{inner}>");
                    args = generic_args(&synthetic);
                    let inner_chars = inner.chars().collect::<Vec<_>>();
                    collect_type_references(&inner_chars, refs);
                    index = next_index;
                }

                refs.push(TypeReference { name, args });
            }
            _ => index += 1,
        }
    }
}

fn generic_inner(chars: &[char], start: usize) -> (String, usize) {
    let mut depth = 0usize;
    let mut index = start;
    let mut inner = String::new();
    while index < chars.len() {
        match chars[index] {
            '"' => {
                inner.push(chars[index]);
                index += 1;
                while index < chars.len() {
                    inner.push(chars[index]);
                    if chars[index] == '\\' {
                        index += 1;
                        if index < chars.len() {
                            inner.push(chars[index]);
                        }
                    } else if chars[index] == '"' {
                        index += 1;
                        break;
                    }
                    index += 1;
                }
            }
            '<' => {
                if depth > 0 {
                    inner.push('<');
                }
                depth += 1;
                index += 1;
            }
            '>' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return (inner, index + 1);
                }
                inner.push('>');
                index += 1;
            }
            ch => {
                inner.push(ch);
                index += 1;
            }
        }
    }
    (inner, index)
}

fn skip_string_literal(chars: &[char], mut index: usize) -> usize {
    while index < chars.len() {
        match chars[index] {
            '\\' => index += 2,
            '"' => return index + 1,
            _ => index += 1,
        }
    }
    index
}

fn is_type_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_type_ident_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn type_arity_diagnostic(
    name: &str,
    expected: usize,
    actual: usize,
    span: &crate::span::Span,
) -> Diagnostic {
    Diagnostic::error(
        "N1203",
        format!("type `{name}` expects {expected} generic argument(s), got {actual}"),
        span.clone(),
    )
    .with_reason("generic type references must match the declaration arity")
    .with_help("pass the required generic arguments or remove the extra arguments")
}

fn is_scalar_validator(name: &str) -> bool {
    matches!(
        name,
        "validate_email" | "validate_url" | "validate_uuid" | "validate_phone_number"
    )
}

fn scalar_validator_param_types(name: &str) -> Option<Vec<TypeRef>> {
    is_scalar_validator(name).then(|| {
        vec![TypeRef {
            raw: "Text".to_string(),
        }]
    })
}

fn scalar_validator_result_type(name: &str) -> Option<TypeRef> {
    let raw = match name {
        "validate_email" => "Email",
        "validate_url" => "Url",
        "validate_uuid" => "Uuid",
        "validate_phone_number" => "PhoneNumber",
        _ => return None,
    };
    Some(TypeRef {
        raw: raw.to_string(),
    })
}

fn validate_scalar_value(validator: &str, value: &str) -> Result<(), String> {
    match validator {
        "validate_email" => validate_email_literal(value),
        "validate_url" => validate_url_literal(value),
        "validate_uuid" => validate_uuid_literal(value),
        "validate_phone_number" => validate_phone_number_literal(value),
        _ => Ok(()),
    }
}

fn validate_email_literal(value: &str) -> Result<(), String> {
    let value = value.trim();
    let Some((local, domain)) = value.split_once('@') else {
        return Err("expected one `@` separator".to_string());
    };
    if local.is_empty() || domain.is_empty() || domain.contains('@') {
        return Err("expected non-empty local and domain parts".to_string());
    }
    if domain.starts_with('.') || domain.ends_with('.') || !domain.contains('.') {
        return Err("expected a dotted domain".to_string());
    }
    if !value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '@' | '.' | '_' | '%' | '+' | '-'))
    {
        return Err("expected conservative ASCII email characters".to_string());
    }
    Ok(())
}

fn validate_url_literal(value: &str) -> Result<(), String> {
    let value = value.trim();
    let rest = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"))
        .ok_or_else(|| "expected absolute http or https URL".to_string())?;
    let host = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .split('@')
        .last()
        .unwrap_or_default()
        .split(':')
        .next()
        .unwrap_or_default();
    if host.is_empty() || !host.contains('.') {
        return Err("expected a dotted host".to_string());
    }
    if !host
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-'))
    {
        return Err("expected conservative ASCII host characters".to_string());
    }
    Ok(())
}

fn validate_uuid_literal(value: &str) -> Result<(), String> {
    let parts = value.split('-').collect::<Vec<_>>();
    let lengths = [8, 4, 4, 4, 12];
    if parts.len() != lengths.len()
        || parts
            .iter()
            .zip(lengths)
            .any(|(part, len)| part.len() != len || !part.chars().all(|ch| ch.is_ascii_hexdigit()))
    {
        return Err("expected 8-4-4-4-12 hexadecimal UUID format".to_string());
    }
    Ok(())
}

fn validate_phone_number_literal(value: &str) -> Result<(), String> {
    let value = value.trim();
    let digits = value.strip_prefix('+').unwrap_or(value);
    if digits.len() < 8 || digits.len() > 15 || !digits.chars().all(|ch| ch.is_ascii_digit()) {
        return Err("expected 8 to 15 digits with an optional leading `+`".to_string());
    }
    Ok(())
}

fn types_compatible(expected: &TypeRef, actual: &TypeRef) -> bool {
    expected.raw == actual.raw
}

fn arithmetic_result_type(op: BinaryOp, left: &TypeRef, right: &TypeRef) -> Option<TypeRef> {
    match op {
        BinaryOp::Add | BinaryOp::Subtract => {
            if is_numeric_type(left) && types_compatible(left, right) {
                return Some(left.clone());
            }
            if is_money_type(left) && types_compatible(left, right) {
                return Some(left.clone());
            }
            if is_distance_type(left) && types_compatible(left, right) {
                return Some(left.clone());
            }
            if is_duration_type(left) && types_compatible(left, right) {
                return Some(left.clone());
            }
            if is_speed_type(left) && types_compatible(left, right) {
                return Some(left.clone());
            }
            None
        }
        BinaryOp::Multiply => {
            if is_numeric_type(left) && types_compatible(left, right) {
                return Some(left.clone());
            }
            if is_money_type(left) && is_numeric_type(right) {
                return Some(left.clone());
            }
            if is_numeric_type(left) && is_money_type(right) {
                return Some(right.clone());
            }
            if (is_speed_type(left) && is_duration_type(right))
                || (is_duration_type(left) && is_speed_type(right))
            {
                let speed_unit = if is_speed_type(left) {
                    generic_arg(&left.raw)
                } else {
                    generic_arg(&right.raw)
                };
                let duration_unit = if is_duration_type(left) {
                    generic_arg(&left.raw)
                } else {
                    generic_arg(&right.raw)
                };
                if speed_unit.as_deref() == Some("KilometersPerHour")
                    && duration_unit.as_deref() == Some("Hour")
                {
                    return Some(TypeRef {
                        raw: "Distance<Kilometer>".to_string(),
                    });
                }
            }
            if is_numeric_type(right) {
                if is_distance_type(left) || is_duration_type(left) || is_speed_type(left) {
                    return Some(left.clone());
                }
            }
            if is_numeric_type(left) {
                if is_distance_type(right) || is_duration_type(right) || is_speed_type(right) {
                    return Some(right.clone());
                }
            }
            None
        }
        BinaryOp::Divide => {
            if is_numeric_type(left) && types_compatible(left, right) {
                return Some(left.clone());
            }
            if is_money_type(left) && is_numeric_type(right) {
                return Some(left.clone());
            }
            if is_distance_type(left) && is_duration_type(right) {
                let dist_unit = generic_arg(&left.raw);
                let dur_unit = generic_arg(&right.raw);
                if dist_unit.as_deref() == Some("Kilometer") && dur_unit.as_deref() == Some("Hour")
                {
                    return Some(TypeRef {
                        raw: "Speed<KilometersPerHour>".to_string(),
                    });
                }
            }
            if is_distance_type(left) && is_speed_type(right) {
                let dist_unit = generic_arg(&left.raw);
                let speed_unit = generic_arg(&right.raw);
                if dist_unit.as_deref() == Some("Kilometer")
                    && speed_unit.as_deref() == Some("KilometersPerHour")
                {
                    return Some(TypeRef {
                        raw: "Duration<Hour>".to_string(),
                    });
                }
            }
            if is_distance_type(left) && types_compatible(left, right) {
                return Some(TypeRef {
                    raw: "Float".to_string(),
                });
            }
            if is_duration_type(left) && types_compatible(left, right) {
                return Some(TypeRef {
                    raw: "Float".to_string(),
                });
            }
            if is_speed_type(left) && types_compatible(left, right) {
                return Some(TypeRef {
                    raw: "Float".to_string(),
                });
            }
            if is_numeric_type(right) {
                if is_distance_type(left) || is_duration_type(left) || is_speed_type(left) {
                    return Some(left.clone());
                }
            }
            None
        }
        BinaryOp::Or
        | BinaryOp::And
        | BinaryOp::Equal
        | BinaryOp::NotEqual
        | BinaryOp::LessThan
        | BinaryOp::LessThanOrEqual
        | BinaryOp::GreaterThan
        | BinaryOp::GreaterThanOrEqual => None,
    }
}

fn generic_arg(raw: &str) -> Option<String> {
    generic_args(raw).into_iter().next()
}

fn generic_args(raw: &str) -> Vec<String> {
    let Some(start) = raw.find('<') else {
        return Vec::new();
    };
    let Some(end) = raw.rfind('>') else {
        return Vec::new();
    };
    if end <= start + 1 {
        return Vec::new();
    }

    let inner = &raw[start + 1..end];
    let mut args = Vec::new();
    let mut depth = 0usize;
    let mut arg_start = 0usize;
    for (index, ch) in inner.char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                let arg = inner[arg_start..index].trim();
                if !arg.is_empty() {
                    args.push(arg.to_string());
                }
                arg_start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    let arg = inner[arg_start..].trim();
    if !arg.is_empty() {
        args.push(arg.to_string());
    }
    args
}

fn result_ok_type(ty: &TypeRef) -> Option<TypeRef> {
    generic_args(&ty.raw)
        .first()
        .cloned()
        .map(|raw| TypeRef { raw })
}

fn result_err_type(ty: &TypeRef) -> Option<TypeRef> {
    generic_args(&ty.raw)
        .get(1)
        .cloned()
        .map(|raw| TypeRef { raw })
}

fn is_ordered_type(ty: &TypeRef) -> bool {
    matches!(
        ty.raw.as_str(),
        "Int" | "Float" | "Decimal" | "Date" | "DateTime"
    ) || is_money_type(ty)
        || is_distance_type(ty)
        || is_duration_type(ty)
        || is_speed_type(ty)
}

fn is_distance_type(ty: &TypeRef) -> bool {
    ty.raw.starts_with("Distance<") && generic_arg(&ty.raw).is_some()
}

fn is_duration_type(ty: &TypeRef) -> bool {
    ty.raw.starts_with("Duration<") && generic_arg(&ty.raw).is_some()
}

fn is_speed_type(ty: &TypeRef) -> bool {
    ty.raw.starts_with("Speed<") && generic_arg(&ty.raw).is_some()
}

fn is_bool_type(ty: &TypeRef) -> bool {
    ty.raw == "Bool"
}

fn direct_workflow_expectation_call_name(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Call { callee, .. } => match callee.as_ref() {
            Expr::Ident(name) => Some(name.as_str()),
            _ => None,
        },
        _ => None,
    }
}

fn direct_call_path(expr: &Expr) -> Option<Vec<&str>> {
    match expr {
        Expr::Call { callee, .. } => callee.path(),
        _ => None,
    }
}

fn is_numeric_type(ty: &TypeRef) -> bool {
    matches!(ty.raw.as_str(), "Int" | "Float" | "Decimal")
}

fn is_money_type(ty: &TypeRef) -> bool {
    ty.raw.starts_with("Money<") && generic_arg(&ty.raw).is_some()
}

fn root_member_names(expr: &Expr) -> Vec<String> {
    let mut names = Vec::new();
    collect_root_member_names(expr, &mut names);
    names.sort();
    names.dedup();
    names
}

fn collect_root_member_names(expr: &Expr, names: &mut Vec<String>) {
    match expr {
        Expr::Member { object, .. } => {
            if let Some(path) = expr.path() {
                if path.len() > 1 {
                    names.push(path[0].to_string());
                }
            }
            collect_root_member_names(object, names);
        }
        Expr::Call { callee, args } => {
            collect_root_member_names(callee, names);
            for arg in args {
                collect_root_member_names(arg, names);
            }
        }
        Expr::Binary { left, right, .. } => {
            collect_root_member_names(left, names);
            collect_root_member_names(right, names);
        }
        Expr::Try(inner) | Expr::Async(inner) | Expr::Await(inner) => {
            collect_root_member_names(inner, names)
        }
        Expr::Object(fields) => {
            for field in fields {
                collect_root_member_names(&field.value, names);
            }
        }
        Expr::Ident(_)
        | Expr::String(_)
        | Expr::Bool(_)
        | Expr::Int(_)
        | Expr::Float(_)
        | Expr::Quantity(_, _) => {}
    }
}
