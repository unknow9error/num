use crate::connectors::{
    ConnectorArgLabel, ConnectorCallContext, ConnectorError, ConnectorExecutor,
    DemoConnectorExecutor,
};
use crate::cost::{CostEntry, CostLedger};
use crate::execution::{ActionExecutor, MemoryIdempotencyStore, RetryPolicy};
use crate::observability::{RuntimeTraceEvent, RuntimeTraceKind};
use crate::rate_limit::{parse_rate_limit, RateLimiter};
use crate::{redaction, ActionSpec, Money, RiskLevel, RuntimeError, SecurityContext};
use num_compiler::ast::{
    Declaration, Labels, MatchBinding, MatchPattern, Module, Privacy, RawExpr, Stmt, Trust,
};
use num_compiler::expr::{self, BinaryOp, Expr};
use std::collections::{BTreeSet, HashMap};
use std::time::Duration;

macro_rules! runtime_println {
    ($runtime:expr) => {
        if $runtime.output_enabled {
            println!();
        }
    };
    ($runtime:expr, $($arg:tt)*) => {
        if $runtime.output_enabled {
            println!($($arg)*);
        }
    };
}

macro_rules! runtime_print {
    ($runtime:expr, $($arg:tt)*) => {
        if $runtime.output_enabled {
            print!($($arg)*);
        }
    };
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Money(i128, String),
    Brand(String, Box<Value>),
    Secret(Box<Value>),
    Uncertain(Box<Value>, f64),
    List(Vec<Value>),
    Struct(String, HashMap<String, Value>),
    Enum(String, String, Option<Box<Value>>),
    Quantity(f64, String),
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Null => write!(f, "null"),
            Value::Bool(b) => write!(f, "{}", b),
            Value::Int(i) => write!(f, "{}", i),
            Value::Float(fl) => write!(f, "{}", fl),
            Value::String(s) => write!(f, "\"{}\"", s),
            Value::Money(amount, currency) => write!(f, "{}.00 {}", amount / 100, currency),
            Value::Brand(_, value) => write!(f, "{value}"),
            Value::Secret(_) => write!(f, "{}", redaction::REDACTION_MARKER),
            Value::Uncertain(val, conf) => write!(f, "Uncertain({}, confidence: {:.2})", val, conf),
            Value::List(items) => {
                write!(f, "[")?;
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, "]")
            }
            Value::Struct(name, fields) => {
                let mut keys = fields.keys().collect::<Vec<_>>();
                keys.sort();
                if name == "Object" {
                    write!(f, "{{ ")?;
                } else {
                    write!(f, "{} {{ ", name)?;
                }
                for (index, key) in keys.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    let value = fields.get(*key).expect("sorted key must exist");
                    write!(f, "{}: {}", key, value)?;
                }
                write!(f, "}}")
            }
            Value::Enum(name, variant, Some(payload)) => {
                write!(f, "{}::{}({})", name, variant, payload)
            }
            Value::Enum(name, variant, None) => write!(f, "{}::{}", name, variant),
            Value::Quantity(amount, unit) => write!(f, "{} {}", amount, unit),
        }
    }
}

fn retry_policy(raw: Option<&str>) -> RetryPolicy {
    let Some(raw) = raw else {
        return RetryPolicy::none();
    };
    raw.split(|ch: char| !ch.is_ascii_digit())
        .find(|part| !part.is_empty())
        .and_then(|part| part.parse::<u32>().ok())
        .map(RetryPolicy::attempts)
        .unwrap_or_else(RetryPolicy::none)
}

fn runtime_risk(risk: num_compiler::ast::Risk) -> RiskLevel {
    match risk {
        num_compiler::ast::Risk::Low => RiskLevel::Low,
        num_compiler::ast::Risk::Medium => RiskLevel::Medium,
        num_compiler::ast::Risk::High => RiskLevel::High,
        num_compiler::ast::Risk::Critical => RiskLevel::Critical,
    }
}

fn parse_money_limit(raw: Option<&str>) -> Option<Money> {
    let raw = raw?.trim();
    let mut parts = raw.split_whitespace();
    let amount = parse_decimal_minor_units(parts.next()?)?;
    let currency = parts.next().unwrap_or("USD").to_string();
    Some(Money {
        minor_units: amount,
        currency,
    })
}

fn parse_decimal_minor_units(raw: &str) -> Option<i128> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let negative = raw.starts_with('-');
    let raw = raw.trim_start_matches('-');
    let (major, minor) = raw.split_once('.').unwrap_or((raw, ""));
    let major = major.parse::<i128>().ok()?;
    let mut minor_digits = minor.chars().take(2).collect::<String>();
    while minor_digits.len() < 2 {
        minor_digits.push('0');
    }
    let minor = if minor_digits.is_empty() {
        0
    } else {
        minor_digits.parse::<i128>().ok()?
    };
    let amount = major.checked_mul(100)?.checked_add(minor)?;
    Some(if negative { -amount } else { amount })
}

fn parse_timeout(raw: Option<&str>) -> Option<Duration> {
    let normalized = raw?.trim().replace(' ', "");
    if normalized.is_empty() {
        return None;
    }

    if let Some(value) = normalized.strip_suffix("ms") {
        return value.parse::<u64>().ok().map(Duration::from_millis);
    }
    if let Some(value) = normalized.strip_suffix('s') {
        return value.parse::<u64>().ok().map(Duration::from_secs);
    }
    if let Some(value) = normalized.strip_suffix('m') {
        return value
            .parse::<u64>()
            .ok()
            .and_then(|minutes| minutes.checked_mul(60))
            .map(Duration::from_secs);
    }
    if let Some(value) = normalized.strip_suffix('h') {
        return value
            .parse::<u64>()
            .ok()
            .and_then(|hours| hours.checked_mul(60 * 60))
            .map(Duration::from_secs);
    }
    normalized.parse::<u64>().ok().map(Duration::from_millis)
}

fn value_to_idempotency_key(value: Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Int(value) => value.to_string(),
        Value::Float(value) => value.to_string(),
        Value::String(value) => value,
        Value::Money(minor_units, currency) => format!("{minor_units}:{currency}"),
        Value::Brand(name, value) => {
            format!("{name}:{}", value_to_idempotency_key(*value))
        }
        Value::Secret(_) => redaction::REDACTION_MARKER.to_string(),
        Value::Uncertain(value, confidence) => {
            format!("{}:{confidence}", value_to_idempotency_key(*value))
        }
        Value::List(items) => {
            let items = items
                .into_iter()
                .map(value_to_idempotency_key)
                .collect::<Vec<_>>();
            format!("[{}]", items.join(","))
        }
        Value::Struct(name, fields) => {
            let mut pairs = fields
                .into_iter()
                .map(|(key, value)| format!("{key}={}", value_to_idempotency_key(value)))
                .collect::<Vec<_>>();
            pairs.sort();
            format!("{name}{{{}}}", pairs.join(","))
        }
        Value::Enum(name, variant, payload) => match payload {
            Some(payload) => format!("{name}.{variant}({})", value_to_idempotency_key(*payload)),
            None => format!("{name}.{variant}"),
        },
        Value::Quantity(amount, unit) => format!("{amount}:{unit}"),
    }
}

fn runtime_error_to_string(error: RuntimeError) -> String {
    error.message()
}

fn connector_error_to_runtime_error(method: &str, error: ConnectorError) -> RuntimeError {
    RuntimeError::ConnectorFailed {
        method: method.to_string(),
        code: error.code,
        message: error.message,
        retryable: error.retryable,
    }
}

fn current_user_value(security: &SecurityContext) -> Value {
    let mut fields = HashMap::new();
    fields.insert("id".to_string(), Value::String(security.actor.clone()));
    fields.insert("tenant".to_string(), Value::String(security.tenant.clone()));
    fields.insert(
        "request_id".to_string(),
        Value::String(security.request_id.clone()),
    );
    fields.insert(
        "correlation_id".to_string(),
        Value::String(security.correlation_id.clone()),
    );
    Value::Struct("Actor".to_string(), fields)
}

fn privacy_label(labels: &Labels) -> Option<String> {
    labels.privacy.map(|privacy| {
        match privacy {
            Privacy::Public => "public",
            Privacy::Internal => "internal",
            Privacy::Private => "private",
            Privacy::Sensitive => "sensitive",
            Privacy::Secret => "secret",
            Privacy::Regulated => "regulated",
        }
        .to_string()
    })
}

fn trust_label(labels: &Labels) -> Option<String> {
    labels.trust.map(|trust| {
        match trust {
            Trust::Untrusted => "untrusted",
            Trust::Trusted => "trusted",
            Trust::Verified => "verified",
        }
        .to_string()
    })
}

#[derive(Debug, Clone)]
enum ExecSignal {
    Continue,
    Return(Value),
}

fn demo_security_context(permissions: Vec<String>) -> SecurityContext {
    SecurityContext {
        actor: "admin@company.com".to_string(),
        tenant: "default".to_string(),
        permissions: permissions.into_iter().collect::<BTreeSet<_>>(),
        correlation_id: "corr_demo".to_string(),
        request_id: "req_demo".to_string(),
    }
}

pub struct Runtime<'a> {
    module: &'a Module,
    scopes: Vec<HashMap<String, Value>>,
    security: SecurityContext,
    audits: Vec<String>,
    traces: Vec<RuntimeTraceEvent>,
    next_trace_sequence: u64,
    in_saga: bool,
    saga_rollbacks: Vec<RawExpr>,
    connectors: Box<dyn ConnectorExecutor + 'a>,
    ai_mocks: HashMap<String, Value>,
    connector_mocks: HashMap<String, Value>,
    idempotency: MemoryIdempotencyStore,
    costs: CostLedger,
    rate_limits: RateLimiter,
    last_error: Option<RuntimeError>,
    output_enabled: bool,
}

impl<'a> Runtime<'a> {
    pub fn new(module: &'a Module, permissions: Vec<String>) -> Self {
        Self::with_connectors(module, permissions, Box::new(DemoConnectorExecutor::new()))
    }

    pub fn with_security(module: &'a Module, security: SecurityContext) -> Self {
        Self::with_connectors_and_security(module, security, Box::new(DemoConnectorExecutor::new()))
    }

    pub fn with_connectors(
        module: &'a Module,
        permissions: Vec<String>,
        connectors: Box<dyn ConnectorExecutor + 'a>,
    ) -> Self {
        let security = demo_security_context(permissions);
        Self::with_connectors_and_security(module, security, connectors)
    }

    pub fn with_connectors_and_security(
        module: &'a Module,
        security: SecurityContext,
        connectors: Box<dyn ConnectorExecutor + 'a>,
    ) -> Self {
        let mut root_scope = HashMap::new();
        root_scope.insert("current_user".to_string(), current_user_value(&security));
        Self {
            module,
            scopes: vec![root_scope],
            security,
            audits: Vec::new(),
            traces: Vec::new(),
            next_trace_sequence: 1,
            in_saga: false,
            saga_rollbacks: Vec::new(),
            connectors,
            ai_mocks: HashMap::new(),
            connector_mocks: HashMap::new(),
            idempotency: MemoryIdempotencyStore::new(),
            costs: CostLedger::new(),
            rate_limits: RateLimiter::new(),
            last_error: None,
            output_enabled: true,
        }
    }

    pub fn security_context(&self) -> &SecurityContext {
        &self.security
    }

    pub fn cost_spent(&self, currency: &str) -> Money {
        self.costs.spent(currency)
    }

    pub fn cost_entry_count(&self) -> usize {
        self.costs.entries().len()
    }

    pub fn cost_entries(&self) -> &[CostEntry] {
        self.costs.entries()
    }

    pub fn audit_events(&self) -> &[String] {
        &self.audits
    }

    pub fn trace_events(&self) -> &[RuntimeTraceEvent] {
        &self.traces
    }

    pub fn last_error(&self) -> Option<&RuntimeError> {
        self.last_error.as_ref()
    }

    pub fn set_output_enabled(&mut self, enabled: bool) {
        self.output_enabled = enabled;
    }

    fn trace(&mut self, kind: RuntimeTraceKind, target: impl Into<String>, detail: Option<String>) {
        let sequence = self.next_trace_sequence;
        self.next_trace_sequence = self.next_trace_sequence.saturating_add(1);
        self.traces
            .push(RuntimeTraceEvent::new(sequence, kind, target, detail));
    }

    fn enter_budget_scope(&mut self, scope: impl Into<String>, raw: Option<&str>) -> bool {
        if let Some(budget) = parse_money_limit(raw) {
            let scope = scope.into();
            runtime_println!(
                self,
                "  [BUDGET] {} limit set to {}.{:02} {}",
                scope,
                budget.minor_units / 100,
                budget.minor_units.abs() % 100,
                budget.currency
            );
            self.costs.push_budget_scope(scope, budget);
            return true;
        }
        false
    }

    fn exit_budget_scope(&mut self, active: bool) {
        if active {
            self.costs.pop_budget_scope();
        }
    }

    fn apply_rate_limit(&mut self, scope: &str, raw: Option<&str>) -> Result<(), String> {
        if let Some(raw) = raw {
            if let Some(limit) = parse_rate_limit(raw) {
                self.rate_limits
                    .check(scope.to_string(), limit)
                    .map_err(runtime_error_to_string)?;
                runtime_println!(
                    self,
                    "  [RATE LIMIT] {} allows {} per {:?}",
                    scope,
                    limit.max_requests,
                    limit.window
                );
            }
        }
        Ok(())
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn set_var(&mut self, name: String, val: Value) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, val);
        }
    }

    fn assign_var(&mut self, name: &str, val: Value) -> bool {
        for scope in self.scopes.iter_mut().rev() {
            if scope.contains_key(name) {
                scope.insert(name.to_string(), val);
                return true;
            }
        }
        false
    }

    fn get_var(&self, name: &str) -> Option<&Value> {
        for scope in self.scopes.iter().rev() {
            if let Some(val) = scope.get(name) {
                return Some(val);
            }
        }
        None
    }

    pub fn run_workflow(&mut self, name: &str, args: HashMap<String, Value>) -> Result<(), String> {
        runtime_println!(self, "\n=== Starting Workflow: {} ===", name);
        self.trace(
            RuntimeTraceKind::WorkflowStarted,
            name,
            Some(format!("{} args", args.len())),
        );
        let decl = self
            .module
            .declarations
            .iter()
            .find(|d| d.name() == name)
            .ok_or_else(|| format!("Workflow '{}' not found in module", name))?;

        let (params, budget, rate_limit, body) = match decl {
            Declaration::Workflow(workflow) => (
                workflow.params.clone(),
                workflow.budget.clone(),
                workflow.rate_limit.clone(),
                workflow.body.clone(),
            ),
            _ => return Err(format!("'{}' is not a workflow declaration", name)),
        };

        self.apply_rate_limit(&format!("workflow:{name}"), rate_limit.as_deref())?;
        let budget_scope = self.enter_budget_scope(format!("workflow:{name}"), budget.as_deref());
        self.push_scope();
        for param in &params {
            if let Some(val) = args.get(&param.name) {
                runtime_println!(self, "  [PARAM] {} = {}", param.name, val);
                self.set_var(param.name.clone(), val.clone());
            } else {
                self.pop_scope();
                self.exit_budget_scope(budget_scope);
                return Err(format!("Missing argument: {}", param.name));
            }
        }

        match self.exec_block(&body) {
            Ok(_) => {}
            Err(err) => {
                self.trace(RuntimeTraceKind::WorkflowFailed, name, Some(err.clone()));
                runtime_println!(self, "  [ERROR] Workflow failed: {}", err);
                if !self.saga_rollbacks.is_empty() {
                    runtime_println!(self, "\n=== Initiating Saga Compensations (Rollbacks) ===");
                    for rollback_expr in self.saga_rollbacks.clone().iter().rev() {
                        runtime_println!(
                            self,
                            "  [ROLLBACK] Executing compensation: {}",
                            rollback_expr.text
                        );
                        if let Err(rollback_err) = self.eval_expr(rollback_expr) {
                            runtime_println!(
                                self,
                                "    [WARNING] Compensation failed: {}",
                                rollback_err
                            );
                        }
                    }
                }
                self.pop_scope();
                self.exit_budget_scope(budget_scope);
                return Err(err);
            }
        }

        self.pop_scope();
        self.exit_budget_scope(budget_scope);
        runtime_println!(self, "=== Workflow Completed Successfully ===\n");
        self.trace(RuntimeTraceKind::WorkflowCompleted, name, None);
        if !self.audits.is_empty() {
            runtime_println!(self, "Audit Trail:");
            for audit in &self.audits {
                runtime_println!(self, "  - {}", audit);
            }
        }
        Ok(())
    }

    pub fn run_service_route(
        &mut self,
        service_name: &str,
        method: &str,
        path: &str,
        input: Option<Value>,
    ) -> Result<(), String> {
        runtime_println!(
            self,
            "\n=== Starting Service Route: {} {} {} ===",
            service_name,
            method,
            path
        );
        self.trace(
            RuntimeTraceKind::ServiceRouteStarted,
            format!("{service_name}:{method}:{path}"),
            None,
        );

        let decl = self
            .module
            .declarations
            .iter()
            .find(|decl| decl.name() == service_name)
            .ok_or_else(|| format!("Service '{}' not found in module", service_name))?;

        let (service_budget, service_rate_limit, routes) = match decl {
            Declaration::Service(service) => (
                service.budget.clone(),
                service.rate_limit.clone(),
                service.routes.clone(),
            ),
            _ => return Err(format!("'{}' is not a service declaration", service_name)),
        };

        let route = routes
            .iter()
            .find(|route| route.method.eq_ignore_ascii_case(method) && route.path == path)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "Route '{} {}' not found in service '{}'",
                    method, path, service_name
                )
            })?;

        let policy_diagnostics = num_compiler::semantic::check_service_route_for_tenant(
            self.module,
            service_name,
            method,
            path,
            &self.security.tenant,
        );
        if let Some(diagnostic) = policy_diagnostics
            .iter()
            .find(|diagnostic| diagnostic.is_error())
        {
            return Err(format!(
                "Policy Violation: route '{} {}' denied for tenant '{}': {}",
                method, path, self.security.tenant, diagnostic.message
            ));
        }

        self.apply_rate_limit(
            &format!("service:{service_name}:{}:{}", route.method, route.path),
            service_rate_limit.as_deref(),
        )?;
        let budget_scope = self.enter_budget_scope(
            format!("service:{service_name}:{}:{}", route.method, route.path),
            service_budget.as_deref(),
        );
        for permission in &route.requires {
            runtime_println!(self, "  [ROUTE REQUIRE] Permission: {}", permission);
            if !self.security.has_permission(permission) {
                self.exit_budget_scope(budget_scope);
                return Err(format!(
                    "Security Violation: Route '{} {}' requires permission '{}'",
                    method, path, permission
                ));
            }
        }

        self.push_scope();
        if let Some(route_input) = &route.input {
            let Some(value) = input else {
                self.pop_scope();
                self.exit_budget_scope(budget_scope);
                return Err(format!(
                    "Missing route input '{}' for {} {}",
                    route_input.name, method, path
                ));
            };
            runtime_println!(self, "  [INPUT] {} = {}", route_input.name, value);
            self.set_var(route_input.name.clone(), value);
        }

        if let Err(err) = self.exec_block(&route.body) {
            self.trace(
                RuntimeTraceKind::ServiceRouteFailed,
                format!("{service_name}:{method}:{path}"),
                Some(err.clone()),
            );
            self.pop_scope();
            self.exit_budget_scope(budget_scope);
            return Err(err);
        }
        self.pop_scope();
        self.exit_budget_scope(budget_scope);

        runtime_println!(self, "=== Service Route Completed Successfully ===\n");
        self.trace(
            RuntimeTraceKind::ServiceRouteCompleted,
            format!("{service_name}:{method}:{path}"),
            None,
        );
        if !self.audits.is_empty() {
            runtime_println!(self, "Audit Trail:");
            for audit in &self.audits {
                runtime_println!(self, "  - {}", audit);
            }
        }
        Ok(())
    }

    pub fn run_test(&mut self, name: &str) -> Result<(), String> {
        runtime_println!(self, "\n=== Starting Test: {} ===", name);
        let decl = self
            .module
            .declarations
            .iter()
            .find(|decl| matches!(decl, Declaration::Test(test) if test.name == name))
            .ok_or_else(|| format!("Test '{}' not found in module", name))?;
        let body = match decl {
            Declaration::Test(test) => test.body.clone(),
            _ => unreachable!(),
        };

        self.push_scope();
        let result = self.exec_block(&body);
        self.pop_scope();
        match result {
            Ok(_) => {
                runtime_println!(self, "=== Test Passed: {} ===\n", name);
                Ok(())
            }
            Err(err) => Err(format!("Test '{name}' failed: {err}")),
        }
    }

    fn exec_block(&mut self, body: &[Stmt]) -> Result<ExecSignal, String> {
        for stmt in body {
            self.trace(
                RuntimeTraceKind::StatementStarted,
                stmt_trace_target(stmt),
                None,
            );
            match self.exec_stmt(stmt)? {
                ExecSignal::Continue => {
                    self.trace(
                        RuntimeTraceKind::StatementCompleted,
                        stmt_trace_target(stmt),
                        None,
                    );
                }
                signal @ ExecSignal::Return(_) => {
                    self.trace(
                        RuntimeTraceKind::StatementCompleted,
                        stmt_trace_target(stmt),
                        Some("return".to_string()),
                    );
                    return Ok(signal);
                }
            }
        }
        Ok(ExecSignal::Continue)
    }

    fn exec_stmt(&mut self, stmt: &Stmt) -> Result<ExecSignal, String> {
        match stmt {
            Stmt::Let(let_stmt) => {
                let val = if let Some(expr) = &let_stmt.expr {
                    self.eval_expr(expr)?
                } else {
                    Value::Null
                };
                runtime_println!(self, "  [LET] {} = {}", let_stmt.name, val);
                self.set_var(let_stmt.name.clone(), val);
                Ok(ExecSignal::Continue)
            }
            Stmt::Assign(assign_stmt) => {
                let val = self.eval_expr(&assign_stmt.expr)?;
                runtime_println!(self, "  [ASSIGN] {} = {}", assign_stmt.name, val);
                if self.assign_var(&assign_stmt.name, val) {
                    Ok(ExecSignal::Continue)
                } else {
                    Err(format!("Unknown assignment target '{}'", assign_stmt.name))
                }
            }
            Stmt::Assert(assert_stmt) => {
                let value = self.eval_expr(&assert_stmt.expr)?;
                match value {
                    Value::Bool(true) => {
                        runtime_println!(
                            self,
                            "  [ASSERT] {} passed",
                            assert_stmt.expr.text.trim()
                        );
                        Ok(ExecSignal::Continue)
                    }
                    Value::Bool(false) => Err(format!(
                        "assertion failed: {}",
                        assert_stmt.expr.text.trim()
                    )),
                    other => Err(format!("assertion evaluated to non-boolean: {}", other)),
                }
            }
            Stmt::ExpectPolicy(expect_stmt) => {
                runtime_println!(
                    self,
                    "  [POLICY EXPECTATION] {} verified by compiler",
                    match expect_stmt.outcome {
                        num_compiler::ast::PolicyExpectation::Allow => "allow",
                        num_compiler::ast::PolicyExpectation::Deny => "deny",
                    }
                );
                Ok(ExecSignal::Continue)
            }
            Stmt::ExpectWorkflow(expect_stmt) => {
                let label = format_call_label(&expect_stmt.call.text);
                let result = self.eval_expr(&expect_stmt.call);
                match (expect_stmt.outcome, result) {
                    (num_compiler::ast::WorkflowExpectation::Success, Ok(_)) => {
                        runtime_println!(self, "  [WORKFLOW EXPECTATION] {label} succeeded");
                        Ok(ExecSignal::Continue)
                    }
                    (num_compiler::ast::WorkflowExpectation::Success, Err(err)) => Err(format!(
                        "expected workflow success for `{label}`, got: {err}"
                    )),
                    (num_compiler::ast::WorkflowExpectation::Failure, Ok(_)) => Err(format!(
                        "expected workflow failure for `{label}`, but it succeeded"
                    )),
                    (num_compiler::ast::WorkflowExpectation::Failure, Err(err)) => {
                        runtime_println!(
                            self,
                            "  [WORKFLOW EXPECTATION] {label} failed as expected: {err}"
                        );
                        Ok(ExecSignal::Continue)
                    }
                }
            }
            Stmt::ExpectAudit(expect_stmt) => {
                let expected = self.eval_expr(&expect_stmt.event)?.to_string();
                if self.audits.contains(&expected) {
                    runtime_println!(self, "  [AUDIT EXPECTATION] {expected} observed");
                    Ok(ExecSignal::Continue)
                } else {
                    Err(format!(
                        "expected audit event {expected}, observed [{}]",
                        self.audits.join(", ")
                    ))
                }
            }
            Stmt::MockAi(mock_stmt) => {
                let target = ai_mock_target(&mock_stmt.call.text)?;
                let value = self.eval_expr(&mock_stmt.value)?;
                let confidence = match self.eval_expr(&mock_stmt.confidence)? {
                    Value::Float(value) => value,
                    Value::Int(value) => value as f64,
                    other => {
                        return Err(format!(
                            "AI mock confidence evaluated to non-number: {}",
                            other
                        ))
                    }
                };
                let mocked = Value::Uncertain(Box::new(value), confidence);
                runtime_println!(self, "  [AI MOCK] {target} => {mocked}");
                self.ai_mocks.insert(target, mocked);
                Ok(ExecSignal::Continue)
            }
            Stmt::MockConnector(mock_stmt) => {
                let target = connector_mock_target(&mock_stmt.call.text)?;
                let value = self.eval_expr(&mock_stmt.value)?;
                runtime_println!(self, "  [CONNECTOR MOCK] {target} => {value}");
                self.connector_mocks.insert(target, value);
                Ok(ExecSignal::Continue)
            }
            Stmt::Require(req_stmt) => {
                let required = &req_stmt.permission;
                runtime_println!(self, "  [REQUIRE] Permission: {}", required);
                if self.security.has_permission(required) {
                    Ok(ExecSignal::Continue)
                } else {
                    Err(format!(
                        "Security Violation: Missing required permission '{}'",
                        required
                    ))
                }
            }
            Stmt::Transaction(tx_stmt) => {
                let was_saga = self.in_saga;
                let pre_rollbacks_len = self.saga_rollbacks.len();
                if tx_stmt.saga {
                    self.in_saga = true;
                    runtime_println!(self, "  [SAGA] Started transaction saga block");
                }
                self.push_scope();
                let res = self.exec_block(&tx_stmt.body);
                self.pop_scope();
                if tx_stmt.saga {
                    self.in_saga = was_saga;
                    match res {
                        Ok(signal) => {
                            runtime_println!(
                                self,
                                "  [SAGA] Completed transaction saga block successfully"
                            );
                            Ok(signal)
                        }
                        Err(err) => {
                            runtime_println!(
                                self,
                                "  [SAGA] Transaction saga block failed: {}. Initiating rollbacks.",
                                err
                            );
                            let rollbacks_to_run: Vec<RawExpr> =
                                self.saga_rollbacks.drain(pre_rollbacks_len..).collect();
                            for rollback_expr in rollbacks_to_run.into_iter().rev() {
                                runtime_println!(
                                    self,
                                    "  [SAGA] Rolling back: {}",
                                    rollback_expr.text
                                );
                                if let Err(rollback_err) = self.eval_expr(&rollback_expr) {
                                    runtime_println!(
                                        self,
                                        "  [SAGA] Rollback execution failed: {}",
                                        rollback_err
                                    );
                                }
                            }
                            Err(err)
                        }
                    }
                } else {
                    res
                }
            }
            Stmt::Scope(scope_stmt) => {
                runtime_println!(self, "  [SCOPE] Entering structured concurrency scope");
                self.push_scope();
                let res = self.exec_block(&scope_stmt.body);
                self.pop_scope();
                res
            }
            Stmt::If(if_stmt) => {
                let cond_val = self.eval_expr(&if_stmt.condition)?;
                let truth = match cond_val {
                    Value::Bool(b) => b,
                    _ => {
                        return Err(format!(
                            "If condition evaluated to non-boolean: {}",
                            cond_val
                        ))
                    }
                };
                runtime_println!(
                    self,
                    "  [IF] Condition '{}' evaluated to {}",
                    if_stmt.condition.text.trim(),
                    truth
                );
                self.push_scope();
                let signal = if truth {
                    self.exec_block(&if_stmt.then_body)?
                } else {
                    self.exec_block(&if_stmt.else_body)?
                };
                self.pop_scope();
                Ok(signal)
            }
            Stmt::Match(match_stmt) => {
                let value = self.eval_expr(&match_stmt.expr)?;
                let discriminator = match &value {
                    Value::Enum(_, variant, _) => Some(variant.as_str()),
                    Value::Struct(name, _) => Some(name.as_str()),
                    _ => None,
                };
                for arm in &match_stmt.arms {
                    if !match_pattern_matches(&arm.pattern, discriminator) {
                        continue;
                    }

                    self.push_scope();
                    self.bind_match_pattern(&arm.pattern, &value)?;
                    if let Some(guard) = &arm.guard {
                        let guard_value = self.eval_expr(guard)?;
                        let guard_matches = match guard_value {
                            Value::Bool(value) => value,
                            other => {
                                self.pop_scope();
                                return Err(format!(
                                    "Match guard evaluated to non-boolean: {other}"
                                ));
                            }
                        };
                        if !guard_matches {
                            self.pop_scope();
                            continue;
                        }
                    }

                    runtime_println!(self, "  [MATCH] {} matched", match_stmt.expr.text.trim());
                    let signal = self.exec_block(&arm.body)?;
                    self.pop_scope();
                    return Ok(signal);
                }
                Err(format!("No match arm for {}", value))
            }
            Stmt::Return(expr) => {
                let val = self.eval_expr(expr)?;
                runtime_println!(self, "  [RETURN] {}", val);
                Ok(ExecSignal::Return(val))
            }
            Stmt::Expr(expr) => {
                self.eval_expr(expr)?;
                Ok(ExecSignal::Continue)
            }
        }
    }

    fn bind_match_pattern(&mut self, pattern: &MatchPattern, value: &Value) -> Result<(), String> {
        let MatchPattern::Variant {
            payload, bindings, ..
        } = pattern
        else {
            return Ok(());
        };
        if let Some(binding_name) = payload {
            let Value::Enum(_, _, Some(payload)) = value else {
                return Err(format!("Cannot bind missing enum payload from: {value}"));
            };
            self.set_var(binding_name.clone(), payload.as_ref().clone());
            return Ok(());
        }
        if bindings.is_empty() {
            return Ok(());
        }

        let Value::Struct(type_name, fields) = value else {
            return Err(format!(
                "Cannot destructure non-struct match value: {value}"
            ));
        };

        for binding in bindings {
            self.bind_match_binding(binding, fields, type_name)?;
        }
        Ok(())
    }

    fn bind_match_binding(
        &mut self,
        binding: &MatchBinding,
        fields: &HashMap<String, Value>,
        type_name: &str,
    ) -> Result<(), String> {
        let Some(value) = fields.get(&binding.field) else {
            return Err(format!(
                "Field '{}' not found while destructuring {}",
                binding.field, type_name
            ));
        };

        if !binding.nested.is_empty() {
            let Value::Struct(nested_type, nested_fields) = value else {
                return Err(format!(
                    "Cannot nested-destructure non-struct field '{}': {}",
                    binding.field, value
                ));
            };
            for nested in &binding.nested {
                self.bind_match_binding(nested, nested_fields, nested_type)?;
            }
            return Ok(());
        }

        self.set_var(binding.name.clone(), value.clone());
        Ok(())
    }

    fn eval_expr(&mut self, expr: &RawExpr) -> Result<Value, String> {
        let parsed = expr::parse(&expr.text).map_err(|err| {
            format!(
                "Could not parse expression '{}': {}",
                expr.text, err.message
            )
        })?;
        self.eval_parsed_expr(&parsed)
    }

    fn eval_parsed_expr(&mut self, expr: &Expr) -> Result<Value, String> {
        match expr {
            Expr::Ident(name) => {
                if let Some(val) = self.get_var(name) {
                    return Ok(val.clone());
                }
                if let Some(value) = self.enum_constructor_value(name, &[])? {
                    return Ok(value);
                }
                Err(format!("Could not evaluate expression: '{}'", name))
            }
            Expr::String(value) => Ok(Value::String(value.clone())),
            Expr::Bool(value) => Ok(Value::Bool(*value)),
            Expr::Int(value) => Ok(Value::Int(*value)),
            Expr::Float(value) => Ok(Value::Float(*value)),
            Expr::Object(fields) => {
                let mut values = HashMap::new();
                for field in fields {
                    values.insert(field.name.clone(), self.eval_parsed_expr(&field.value)?);
                }
                Ok(Value::Struct("Object".to_string(), values))
            }
            Expr::Member { object, field } => {
                if let Expr::Ident(enum_name) = object.as_ref() {
                    if let Some(value) = self.qualified_enum_variant_value(enum_name, field, &[])? {
                        return Ok(value);
                    }
                }

                let obj_val = self.eval_parsed_expr(object)?;
                match obj_val {
                    Value::Struct(_, fields) => fields
                        .get(field)
                        .cloned()
                        .ok_or_else(|| format!("Field '{}' not found in struct", field)),
                    Value::Uncertain(val, conf) => match field.as_str() {
                        "confidence" => Ok(Value::Float(conf)),
                        "value" => Ok(*val),
                        _ => Err(format!("Uncertain value has no field '{}'", field)),
                    },
                    other => Err(format!(
                        "Cannot read field '{}' of non-struct: {}",
                        field, other
                    )),
                }
            }
            Expr::Call { callee, args } => {
                let values = args
                    .iter()
                    .map(|arg| self.eval_parsed_expr(arg))
                    .collect::<Result<Vec<_>, _>>()?;

                if let Expr::Member { object, field } = callee.as_ref() {
                    if let Expr::Ident(enum_name) = object.as_ref() {
                        if let Some(value) =
                            self.qualified_enum_variant_value(enum_name, field, &values)?
                        {
                            return Ok(value);
                        }
                    }
                    if let Ok(obj_val) = self.eval_parsed_expr(object) {
                        if let Some(value) = self.call_impl_method(obj_val, field, &values)? {
                            return Ok(value);
                        }
                    }
                }

                let Some(path) = callee.path() else {
                    return Err("call target must be a named function or method".to_string());
                };
                let name = path.join(".");
                self.call_builtin_or_declared(&name, values)
            }
            Expr::Try(inner) => self.eval_parsed_expr(inner),
            Expr::Binary { left, op, right } => {
                let left = self.eval_parsed_expr(left)?;
                let right = self.eval_parsed_expr(right)?;
                eval_binary(*op, left, right)
            }
            Expr::Quantity(value, unit) => {
                let parsed_val = value
                    .parse::<f64>()
                    .map_err(|err| format!("Invalid quantity float value '{value}': {err}"))?;
                if num_compiler::builtins::symbol(unit)
                    .is_some_and(|sym| sym.kind == num_compiler::builtins::BuiltinKind::Currency)
                {
                    Ok(Value::Money(
                        (parsed_val * 100.0).round() as i128,
                        unit.clone(),
                    ))
                } else {
                    Ok(Value::Quantity(parsed_val, unit.clone()))
                }
            }
            Expr::Async(inner) => self.eval_parsed_expr(inner),
            Expr::Await(inner) => self.eval_parsed_expr(inner),
        }
    }

    fn call_impl_method(
        &mut self,
        obj_val: Value,
        method_name: &str,
        args: &[Value],
    ) -> Result<Option<Value>, String> {
        let Value::Struct(type_name, _) = &obj_val else {
            return Ok(None);
        };
        let method_decl = self.module.declarations.iter().find_map(|decl| {
            if let Declaration::Impl(imp) = decl {
                if imp.target == *type_name {
                    if let Some(method) = imp.methods.iter().find(|m| m.name == method_name) {
                        return Some(method.clone());
                    }
                }
            }
            None
        });
        let Some(method) = method_decl else {
            return Ok(None);
        };

        runtime_println!(
            self,
            "  [METHOD CALL] Executing method {} on {}",
            method_name,
            type_name
        );
        let budget_scope = self.enter_budget_scope(
            format!("method:{type_name}:{method_name}"),
            method.budget.as_deref(),
        );
        self.push_scope();
        self.set_var("self".to_string(), obj_val);
        for (i, param) in method.params.iter().enumerate() {
            let val = args.get(i).cloned().unwrap_or(Value::Null);
            self.set_var(param.name.clone(), val);
        }
        let result = self.exec_block(&method.body);
        self.pop_scope();
        self.exit_budget_scope(budget_scope);
        match result? {
            ExecSignal::Continue => Ok(Some(Value::Null)),
            ExecSignal::Return(value) => Ok(Some(value)),
        }
    }

    fn qualified_enum_variant_value(
        &self,
        enum_name: &str,
        variant_name: &str,
        args: &[Value],
    ) -> Result<Option<Value>, String> {
        for decl in &self.module.declarations {
            let Declaration::Enum(en) = decl else {
                continue;
            };
            if en.name != enum_name {
                continue;
            }
            let Some(variant) = en
                .variants
                .iter()
                .find(|variant| variant.name == variant_name)
            else {
                return Ok(None);
            };
            return match variant.payload {
                Some(_) => {
                    if args.len() != 1 {
                        Err(format!(
                            "Enum variant '{}.{}' expects exactly one payload",
                            enum_name, variant_name
                        ))
                    } else {
                        Ok(Some(Value::Enum(
                            enum_name.to_string(),
                            variant_name.to_string(),
                            Some(Box::new(args[0].clone())),
                        )))
                    }
                }
                None => {
                    if args.is_empty() {
                        Ok(Some(Value::Enum(
                            enum_name.to_string(),
                            variant_name.to_string(),
                            None,
                        )))
                    } else {
                        Err(format!(
                            "Enum variant '{}.{}' expects no payload",
                            enum_name, variant_name
                        ))
                    }
                }
            };
        }
        Ok(None)
    }

    fn execute_action_body(&mut self, body: &[Stmt]) -> Result<(), String> {
        self.exec_block(body)?;
        Ok(())
    }

    fn resolve_idempotency_key(
        &mut self,
        raw: Option<&str>,
        span: &num_compiler::span::Span,
    ) -> Result<Option<String>, String> {
        let Some(raw) = raw.map(str::trim).filter(|raw| !raw.is_empty()) else {
            return Ok(None);
        };
        let value = self.eval_expr(&RawExpr {
            text: raw.to_string(),
            span: span.clone(),
        })?;
        Ok(Some(value_to_idempotency_key(value)))
    }

    fn call_builtin_or_declared(&mut self, name: &str, args: Vec<Value>) -> Result<Value, String> {
        if let Some(value) = self.enum_constructor_value(name, &args)? {
            return Ok(value);
        }

        if self.is_brand_constructor(name) {
            if args.len() != 1 {
                return Err(format!(
                    "Brand constructor '{}' expects exactly one argument",
                    name
                ));
            }
            return Ok(Value::Brand(
                name.to_string(),
                Box::new(args.into_iter().next().unwrap_or(Value::Null)),
            ));
        }

        // Runtime builtins that affect interpreter state.
        match name {
            "require_human_approval" | "require_human_review" => {
                let action = args.first().cloned().unwrap_or(Value::Null);
                runtime_println!(
                    self,
                    "    [HUMAN REVIEW] Requested approval for action: {}",
                    action
                );
                return Ok(Value::Null);
            }
            "reject" => {
                let reason = args.first().cloned().unwrap_or(Value::Null).to_string();
                return Err(format!("rejected: {reason}"));
            }
            "anonymize" | "sanitize" | "validate_trust" | "verify_trust" => {
                return Ok(args.into_iter().next().unwrap_or(Value::Null));
            }
            "unbrand" => {
                if args.len() != 1 {
                    return Err(format!(
                        "unbrand expects exactly one argument, got {}",
                        args.len()
                    ));
                }
                let value = args.into_iter().next().unwrap_or(Value::Null);
                let Value::Brand(_, inner) = value else {
                    return Err(format!("unbrand cannot unwrap non-branded value: {value}"));
                };
                return Ok(*inner);
            }
            "audit" => {
                let event = args.first().cloned().unwrap_or(Value::Null).to_string();
                let context = args.get(1).map(ToString::to_string);
                if let Some(context) = &context {
                    runtime_println!(
                        self,
                        "    [AUDIT] Logged audit event: {} {}",
                        event,
                        context
                    );
                } else {
                    runtime_println!(self, "    [AUDIT] Logged audit event: {}", event);
                }
                self.audits.push(event.clone());
                self.trace(RuntimeTraceKind::AuditLogged, event, context);
                return Ok(Value::Null);
            }
            _ => {}
        }

        if let Some(value) = self.ai_mocks.get(name).cloned() {
            runtime_println!(self, "    [AI MOCK] {name} -> {value}");
            self.trace(
                RuntimeTraceKind::ConnectorCalled,
                name,
                Some("mock_ai".to_string()),
            );
            return Ok(value);
        }

        if let Some(value) = self.connector_mocks.get(name).cloned() {
            runtime_println!(self, "    [CONNECTOR MOCK] {name} -> {value}");
            self.trace(
                RuntimeTraceKind::ConnectorCalled,
                name,
                Some("mock_connector".to_string()),
            );
            return Ok(value);
        }

        let connector_context = self.connector_call_context(name);
        if let Some(result) = self.connectors.call_with_context(&connector_context, &args) {
            self.trace(
                RuntimeTraceKind::ConnectorCalled,
                name,
                Some(format!(
                    "{} args capability={} tenant={} policy={}",
                    args.len(),
                    connector_context.capability,
                    connector_context.tenant,
                    connector_context.policy_decision
                )),
            );
            return result.map_err(|error| {
                self.trace(
                    RuntimeTraceKind::ConnectorCalled,
                    name,
                    Some(format!(
                        "error code={} retryable={}",
                        error.code, error.retryable
                    )),
                );
                let error = redaction::redact_connector_error(&error, &connector_context, &args);
                let runtime_error = connector_error_to_runtime_error(name, error);
                self.last_error = Some(runtime_error.clone());
                runtime_error_to_string(runtime_error)
            });
        }
        if self.is_declared_connector_call(name) {
            let runtime_error = connector_error_to_runtime_error(
                name,
                ConnectorError::missing_implementation(name),
            );
            self.last_error = Some(runtime_error.clone());
            return Err(runtime_error_to_string(runtime_error));
        }

        // Look up declared actions/functions in module
        let decl = match self.module.declarations.iter().find(|d| d.name() == name) {
            Some(d) => d,
            None => {
                runtime_print!(
                    self,
                    "    [MOCK CALL] Calling external function/action '{}' with args: [",
                    name
                );
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        runtime_print!(self, ", ");
                    }
                    runtime_print!(self, "{}", arg);
                }
                runtime_println!(self, "]");
                return Ok(Value::Null);
            }
        };

        match decl {
            Declaration::Action(action) => {
                self.trace(
                    RuntimeTraceKind::ActionCalled,
                    name,
                    Some(format!("{} args", args.len())),
                );
                // Check permissions
                for perm in &action.requires {
                    if !self.security.has_permission(perm) {
                        return Err(format!(
                            "Security Violation: Action '{}' requires permission '{}'",
                            name, perm
                        ));
                    }
                }
                runtime_println!(self, "  [ACTION CALL] Executing action: {}", name);

                // Check audit for high-risk action
                if action.risk >= num_compiler::ast::Risk::High {
                    runtime_println!(
                        self,
                        "    [TRACE] High-risk action checks for audit logs..."
                    );
                }

                // Bind parameters
                self.push_scope();
                for (i, param) in action.params.iter().enumerate() {
                    let val = args.get(i).cloned().unwrap_or(Value::Null);
                    self.set_var(param.name.clone(), val);
                }

                let idempotency_key =
                    self.resolve_idempotency_key(action.idempotency_key.as_deref(), &action.span)?;
                let action_spec = ActionSpec {
                    name: action.name.clone(),
                    required_permissions: action.requires.clone(),
                    risk: runtime_risk(action.risk),
                    timeout: parse_timeout(action.timeout.as_deref()),
                    rollback: action.rollback.clone(),
                    idempotency_key,
                    max_cost: parse_money_limit(action.cost.as_deref()),
                };
                let retry = retry_policy(action.retry.as_deref());
                let mut executor = ActionExecutor::new(std::mem::take(&mut self.idempotency));
                let execution = executor
                    .execute(&action_spec, retry, None, |attempt| {
                        if attempt > 1 {
                            runtime_println!(
                                self,
                                "    [RETRY] Attempt {attempt} for action: {}",
                                action.name
                            );
                        }
                        if let Some(cost) = &action_spec.max_cost {
                            self.costs.authorize(cost)?;
                        }
                        self.execute_action_body(&action.body)
                            .map(|_| Value::Null)
                            .map_err(|reason| RuntimeError::ActionFailed {
                                action: action.name.clone(),
                                reason,
                            })
                    })
                    .map_err(runtime_error_to_string);
                self.idempotency = executor.into_store();
                let execution_res: Result<(), String> = (|| {
                    let execution = execution?;
                    if execution.replayed {
                        runtime_println!(
                            self,
                            "    [IDEMPOTENCY] Replayed action result for {}",
                            action.name
                        );
                    } else if let Some(cost) = &action_spec.max_cost {
                        self.costs
                            .charge(action.name.clone(), cost.clone())
                            .map_err(runtime_error_to_string)?;
                        let total = self.costs.spent(&cost.currency);
                        runtime_println!(
                            self,
                            "    [COST] Charged {}.{} {}; total {}.{} {}",
                            cost.minor_units / 100,
                            format!("{:02}", cost.minor_units.abs() % 100),
                            cost.currency,
                            total.minor_units / 100,
                            format!("{:02}", total.minor_units.abs() % 100),
                            total.currency
                        );
                    }

                    // If inside a saga, register rollback compensation
                    if self.in_saga {
                        if let Some(rollback_str) = &action.rollback {
                            runtime_println!(
                                self,
                                "    [SAGA] Registered compensation template: {}",
                                rollback_str
                            );
                            let parsed = expr::parse(rollback_str).map_err(|err| {
                                format!(
                                    "Could not parse rollback expression '{}': {}",
                                    rollback_str, err.message
                                )
                            })?;

                            let evaluated_rollback_str = match &parsed {
                                Expr::Call { callee, args } => {
                                    let mut evaluated_args = Vec::new();
                                    for arg in args {
                                        let val = self.eval_parsed_expr(arg)?;
                                        evaluated_args.push(value_to_literal_str(&val));
                                    }
                                    let callee_path = callee
                                        .path()
                                        .ok_or_else(|| {
                                            "Rollback callee must be a path".to_string()
                                        })?
                                        .join(".");
                                    format!("{}({})", callee_path, evaluated_args.join(", "))
                                }
                                _ => rollback_str.clone(),
                            };
                            runtime_println!(
                                self,
                                "    [SAGA] Registered evaluated compensation: {}",
                                evaluated_rollback_str
                            );

                            self.saga_rollbacks.push(RawExpr {
                                text: evaluated_rollback_str,
                                span: action.span.clone(),
                            });
                        }
                    }
                    Ok(())
                })();
                self.pop_scope();
                execution_res?;
                Ok(Value::Null)
            }
            Declaration::Function(function) => {
                self.trace(
                    RuntimeTraceKind::FunctionCalled,
                    name,
                    Some(format!("{} args", args.len())),
                );
                runtime_println!(self, "  [FUNCTION CALL] Executing fn: {}", name);
                let budget_scope =
                    self.enter_budget_scope(format!("function:{name}"), function.budget.as_deref());
                self.push_scope();
                for (i, param) in function.params.iter().enumerate() {
                    let val = args.get(i).cloned().unwrap_or(Value::Null);
                    self.set_var(param.name.clone(), val);
                }
                let result = self.exec_block(&function.body);
                self.pop_scope();
                self.exit_budget_scope(budget_scope);
                match result? {
                    ExecSignal::Continue => Ok(Value::Null),
                    ExecSignal::Return(value) => Ok(value),
                }
            }
            Declaration::Workflow(workflow) => {
                runtime_println!(self, "  [WORKFLOW CALL] Executing workflow: {}", name);
                self.apply_rate_limit(&format!("workflow:{name}"), workflow.rate_limit.as_deref())?;
                let budget_scope =
                    self.enter_budget_scope(format!("workflow:{name}"), workflow.budget.as_deref());
                self.push_scope();
                for (i, param) in workflow.params.iter().enumerate() {
                    let val = args.get(i).cloned().unwrap_or(Value::Null);
                    self.set_var(param.name.clone(), val);
                }
                let result = self.exec_block(&workflow.body);
                self.pop_scope();
                self.exit_budget_scope(budget_scope);
                result.map(|_| Value::Null)
            }
            _ => Err(format!("'{}' is not a callable declaration", name)),
        }
    }

    fn is_declared_connector_call(&self, name: &str) -> bool {
        let Some((connector_name, method_name)) = name.split_once('.') else {
            return false;
        };
        self.module.declarations.iter().any(|decl| match decl {
            Declaration::Connector(connector) if connector.name == connector_name => connector
                .methods
                .iter()
                .any(|method| method.name == method_name),
            _ => false,
        })
    }

    fn connector_call_context(&self, name: &str) -> ConnectorCallContext {
        let (connector, method_name) = name
            .split_once('.')
            .map(|(connector, method)| (connector.to_string(), method.to_string()))
            .unwrap_or_else(|| ("external".to_string(), name.to_string()));
        let arg_labels = self.connector_arg_labels(name);
        let policy_decision = if arg_labels.is_empty() {
            "runtime_unlabeled".to_string()
        } else {
            "compile_time_checked".to_string()
        };

        ConnectorCallContext {
            connector,
            method_name,
            method: name.to_string(),
            capability: format!("connector:{name}"),
            actor: self.security.actor.clone(),
            tenant: self.security.tenant.clone(),
            correlation_id: self.security.correlation_id.clone(),
            request_id: self.security.request_id.clone(),
            policy_decision,
            arg_labels,
        }
    }

    fn connector_arg_labels(&self, name: &str) -> Vec<ConnectorArgLabel> {
        let Some((connector_name, method_name)) = name.split_once('.') else {
            return Vec::new();
        };
        self.module
            .declarations
            .iter()
            .find_map(|decl| match decl {
                Declaration::Connector(connector) if connector.name == connector_name => connector
                    .methods
                    .iter()
                    .find(|method| method.name == method_name)
                    .map(|method| {
                        method
                            .params
                            .iter()
                            .enumerate()
                            .map(|(index, param)| ConnectorArgLabel {
                                index,
                                name: param.name.clone(),
                                ty: param.ty.raw.clone(),
                                source: param.labels.source.clone(),
                                privacy: privacy_label(&param.labels),
                                trust: trust_label(&param.labels),
                            })
                            .collect()
                    }),
                _ => None,
            })
            .unwrap_or_default()
    }

    fn enum_constructor_value(&self, name: &str, args: &[Value]) -> Result<Option<Value>, String> {
        for decl in &self.module.declarations {
            let Declaration::Enum(en) = decl else {
                continue;
            };
            let Some(variant) = en.variants.iter().find(|variant| variant.name == name) else {
                continue;
            };
            return match &variant.payload {
                Some(_) => {
                    if args.len() != 1 {
                        Err(format!("Enum variant '{name}' expects exactly one payload"))
                    } else {
                        Ok(Some(Value::Enum(
                            en.name.clone(),
                            name.to_string(),
                            Some(Box::new(args[0].clone())),
                        )))
                    }
                }
                None => {
                    if args.is_empty() {
                        Ok(Some(Value::Enum(en.name.clone(), name.to_string(), None)))
                    } else {
                        Err(format!("Enum variant '{name}' expects no payload"))
                    }
                }
            };
        }
        Ok(None)
    }

    fn is_brand_constructor(&self, name: &str) -> bool {
        self.module.declarations.iter().any(|decl| match decl {
            Declaration::Type(type_decl) if type_decl.name == name => match &type_decl.body {
                num_compiler::ast::TypeBody::Alias(alias) => brand_base_type(&alias.raw).is_some(),
                _ => false,
            },
            _ => false,
        })
    }
}

fn brand_base_type(raw: &str) -> Option<String> {
    let inner = raw
        .trim()
        .strip_prefix("Brand<")
        .and_then(|value| value.strip_suffix('>'))?;
    split_top_level(inner, ',')
        .next()
        .map(|value| value.trim().to_string())
}

fn match_pattern_matches(pattern: &MatchPattern, discriminator: Option<&str>) -> bool {
    match pattern {
        MatchPattern::Wildcard => true,
        MatchPattern::Variant { name, .. } => discriminator == Some(name.as_str()),
    }
}

fn compare_ordered_values(op: &str, left: Value, right: Value) -> Result<Value, String> {
    let result = match (&left, &right) {
        (Value::Int(left), Value::Int(right)) => compare_i128(op, *left as i128, *right as i128),
        (Value::Int(left), Value::Float(right)) => compare_f64(op, *left as f64, *right),
        (Value::Float(left), Value::Int(right)) => compare_f64(op, *left, *right as f64),
        (Value::Float(left), Value::Float(right)) => compare_f64(op, *left, *right),
        (Value::Money(left_amount, left_currency), Value::Money(right_amount, right_currency))
            if left_currency == right_currency =>
        {
            compare_i128(op, *left_amount, *right_amount)
        }
        (Value::Quantity(left_amount, left_unit), Value::Quantity(right_amount, right_unit))
            if left_unit == right_unit =>
        {
            compare_f64(op, *left_amount, *right_amount)
        }
        _ => return Err(format!("Cannot compare {left} {op} {right}")),
    };
    Ok(Value::Bool(result))
}

fn eval_binary(op: BinaryOp, left: Value, right: Value) -> Result<Value, String> {
    match op {
        BinaryOp::Or => match (left, right) {
            (Value::Bool(left), Value::Bool(right)) => Ok(Value::Bool(left || right)),
            (left, right) => Err(format!("Cannot apply || to {left} and {right}")),
        },
        BinaryOp::And => match (left, right) {
            (Value::Bool(left), Value::Bool(right)) => Ok(Value::Bool(left && right)),
            (left, right) => Err(format!("Cannot apply && to {left} and {right}")),
        },
        BinaryOp::Equal => Ok(Value::Bool(left == right)),
        BinaryOp::NotEqual => Ok(Value::Bool(left != right)),
        BinaryOp::LessThan => compare_ordered_values("<", left, right),
        BinaryOp::LessThanOrEqual => compare_ordered_values("<=", left, right),
        BinaryOp::GreaterThan => compare_ordered_values(">", left, right),
        BinaryOp::GreaterThanOrEqual => compare_ordered_values(">=", left, right),
        BinaryOp::Add => eval_arithmetic("+", left, right),
        BinaryOp::Subtract => eval_arithmetic("-", left, right),
        BinaryOp::Multiply => eval_arithmetic("*", left, right),
        BinaryOp::Divide => eval_arithmetic("/", left, right),
    }
}

fn eval_arithmetic(op: &str, left: Value, right: Value) -> Result<Value, String> {
    match (left, right) {
        (Value::Quantity(left, left_unit), Value::Quantity(right, right_unit)) => match op {
            "+" if left_unit == right_unit => Ok(Value::Quantity(left + right, left_unit)),
            "-" if left_unit == right_unit => Ok(Value::Quantity(left - right, left_unit)),
            "*" => {
                if (left_unit == "km/h" && right_unit == "h")
                    || (left_unit == "KilometersPerHour" && right_unit == "Hour")
                {
                    let res_unit = if left_unit == "km/h" {
                        "km"
                    } else {
                        "Kilometer"
                    };
                    Ok(Value::Quantity(left * right, res_unit.to_string()))
                } else if (left_unit == "h" && right_unit == "km/h")
                    || (left_unit == "Hour" && right_unit == "KilometersPerHour")
                {
                    let res_unit = if left_unit == "h" { "km" } else { "Kilometer" };
                    Ok(Value::Quantity(left * right, res_unit.to_string()))
                } else {
                    Err(format!(
                        "Cannot multiply quantity unit {left_unit} by {right_unit}"
                    ))
                }
            }
            "/" => {
                if (left_unit == "km" && right_unit == "h")
                    || (left_unit == "Kilometer" && right_unit == "Hour")
                {
                    let res_unit = if left_unit == "km" {
                        "km/h"
                    } else {
                        "KilometersPerHour"
                    };
                    Ok(Value::Quantity(left / right, res_unit.to_string()))
                } else if (left_unit == "km" && right_unit == "km/h")
                    || (left_unit == "Kilometer" && right_unit == "KilometersPerHour")
                {
                    let res_unit = if left_unit == "km" { "h" } else { "Hour" };
                    Ok(Value::Quantity(left / right, res_unit.to_string()))
                } else if left_unit == right_unit {
                    Ok(Value::Float(left / right))
                } else {
                    Err(format!(
                        "Cannot divide quantity unit {left_unit} by {right_unit}"
                    ))
                }
            }
            _ => Err(format!(
                "Cannot apply {op} to quantities with units {left_unit} and {right_unit}"
            )),
        },
        (Value::Money(left, left_currency), Value::Money(right, right_currency)) => match op {
            "+" if left_currency == right_currency => Ok(Value::Money(left + right, left_currency)),
            "-" if left_currency == right_currency => Ok(Value::Money(left - right, left_currency)),
            _ => Err(format!(
                "Cannot apply {op} to Money with currencies {left_currency} and {right_currency}"
            )),
        },
        (Value::Quantity(left, unit), Value::Int(right)) => match op {
            "*" => Ok(Value::Quantity(left * (right as f64), unit)),
            "/" => Ok(Value::Quantity(left / (right as f64), unit)),
            _ => Err(format!("Cannot apply {op} to quantity and integer")),
        },
        (Value::Int(left), Value::Quantity(right, unit)) => match op {
            "*" => Ok(Value::Quantity((left as f64) * right, unit)),
            _ => Err(format!("Cannot apply {op} to integer and quantity")),
        },
        (Value::Quantity(left, unit), Value::Float(right)) => match op {
            "*" => Ok(Value::Quantity(left * right, unit)),
            "/" => Ok(Value::Quantity(left / right, unit)),
            _ => Err(format!("Cannot apply {op} to quantity and float")),
        },
        (Value::Float(left), Value::Quantity(right, unit)) => match op {
            "*" => Ok(Value::Quantity(left * right, unit)),
            _ => Err(format!("Cannot apply {op} to float and quantity")),
        },
        (Value::Money(left, currency), Value::Int(right)) => match op {
            "*" => Ok(Value::Money(left * (right as i128), currency)),
            "/" => Ok(Value::Money(left / (right as i128), currency)),
            _ => Err(format!("Cannot apply {op} to money and integer")),
        },
        (Value::Int(left), Value::Money(right, currency)) => match op {
            "*" => Ok(Value::Money((left as i128) * right, currency)),
            _ => Err(format!("Cannot apply {op} to integer and money")),
        },
        (Value::Money(left, currency), Value::Float(right)) => match op {
            "*" => Ok(Value::Money(((left as f64) * right) as i128, currency)),
            "/" => Ok(Value::Money(((left as f64) / right) as i128, currency)),
            _ => Err(format!("Cannot apply {op} to money and float")),
        },
        (Value::Float(left), Value::Money(right, currency)) => match op {
            "*" => Ok(Value::Money((left * (right as f64)) as i128, currency)),
            _ => Err(format!("Cannot apply {op} to float and money")),
        },
        (Value::Int(left), Value::Int(right)) => match op {
            "+" => Ok(Value::Int(left + right)),
            "-" => Ok(Value::Int(left - right)),
            "*" => Ok(Value::Int(left * right)),
            "/" => Ok(Value::Int(left / right)),
            _ => unreachable!(),
        },
        (Value::Float(left), Value::Float(right)) => match op {
            "+" => Ok(Value::Float(left + right)),
            "-" => Ok(Value::Float(left - right)),
            "*" => Ok(Value::Float(left * right)),
            "/" => Ok(Value::Float(left / right)),
            _ => unreachable!(),
        },
        (Value::Int(left), Value::Float(right)) => {
            eval_arithmetic(op, Value::Float(left as f64), Value::Float(right))
        }
        (Value::Float(left), Value::Int(right)) => {
            eval_arithmetic(op, Value::Float(left), Value::Float(right as f64))
        }
        (left, right) => Err(format!("Cannot apply {op} to {left} and {right}")),
    }
}

fn compare_i128(op: &str, left: i128, right: i128) -> bool {
    match op {
        "<" => left < right,
        "<=" => left <= right,
        ">" => left > right,
        ">=" => left >= right,
        _ => false,
    }
}

fn compare_f64(op: &str, left: f64, right: f64) -> bool {
    match op {
        "<" => left < right,
        "<=" => left <= right,
        ">" => left > right,
        ">=" => left >= right,
        _ => false,
    }
}

fn stmt_trace_target(stmt: &Stmt) -> String {
    match stmt {
        Stmt::Let(stmt) => format!("let {}", stmt.name),
        Stmt::Assign(stmt) => format!("assign {}", stmt.name),
        Stmt::Assert(stmt) => format!("assert {}", stmt.expr.text),
        Stmt::ExpectPolicy(stmt) => match stmt.outcome {
            num_compiler::ast::PolicyExpectation::Allow => "expect_allow".to_string(),
            num_compiler::ast::PolicyExpectation::Deny => "expect_deny".to_string(),
        },
        Stmt::ExpectWorkflow(stmt) => match stmt.outcome {
            num_compiler::ast::WorkflowExpectation::Success => {
                "expect_workflow_success".to_string()
            }
            num_compiler::ast::WorkflowExpectation::Failure => {
                "expect_workflow_failure".to_string()
            }
        },
        Stmt::ExpectAudit(stmt) => format!("expect_audit {}", stmt.event.text),
        Stmt::MockAi(stmt) => format!("mock_ai {}", format_call_label(&stmt.call.text)),
        Stmt::MockConnector(stmt) => {
            format!("mock_connector {}", format_call_label(&stmt.call.text))
        }
        Stmt::Require(stmt) => format!("require {}", stmt.permission),
        Stmt::Transaction(stmt) => {
            if stmt.saga {
                "transaction saga".to_string()
            } else {
                "transaction".to_string()
            }
        }
        Stmt::If(_) => "if".to_string(),
        Stmt::Scope(_) => "scope".to_string(),
        Stmt::Match(_) => "match".to_string(),
        Stmt::Return(expr) => format!("return {}", expr.text),
        Stmt::Expr(expr) => expr.text.clone(),
    }
}

fn format_call_label(expr: &str) -> String {
    expr.trim()
        .replace(" . ", ".")
        .replace(". ", ".")
        .replace(" .", ".")
        .replace(" ( ", "(")
        .replace(" (", "(")
        .replace("( ", "(")
        .replace(" )", ")")
        .replace(" , ", ", ")
        .replace(" ,", ",")
}

fn ai_mock_target(expr: &str) -> Result<String, String> {
    let expr = format_call_label(expr);
    let Some(paren) = expr.find('(') else {
        return Err(format!("AI mock target is not a call: {expr}"));
    };
    let target = expr[..paren].trim();
    if target.starts_with("ai.") && target.len() > "ai.".len() {
        Ok(target.to_string())
    } else {
        Err(format!("AI mock target must be an ai.* call: {expr}"))
    }
}

fn connector_mock_target(expr: &str) -> Result<String, String> {
    let expr = format_call_label(expr);
    let Some(paren) = expr.find('(') else {
        return Err(format!("Connector mock target is not a call: {expr}"));
    };
    let target = expr[..paren].trim();
    if target.starts_with("ai.") {
        return Err(format!("AI connector mocks must use mock_ai: {expr}"));
    }
    if target.contains('.') {
        Ok(target.to_string())
    } else {
        Err(format!(
            "Connector mock target must be a connector method call: {expr}"
        ))
    }
}

fn split_top_level(input: &str, delimiter: char) -> impl Iterator<Item = &str> {
    struct SplitTopLevel<'a> {
        input: &'a str,
        delimiter: char,
        offset: usize,
    }

    impl<'a> Iterator for SplitTopLevel<'a> {
        type Item = &'a str;

        fn next(&mut self) -> Option<Self::Item> {
            if self.offset > self.input.len() {
                return None;
            }

            let start = self.offset;
            let mut depth = 0usize;
            let mut in_string = false;
            let mut escaped = false;
            for (relative, ch) in self.input[start..].char_indices() {
                if in_string {
                    if escaped {
                        escaped = false;
                    } else if ch == '\\' {
                        escaped = true;
                    } else if ch == '"' {
                        in_string = false;
                    }
                    continue;
                }

                match ch {
                    '"' => in_string = true,
                    '<' => depth += 1,
                    '>' => depth = depth.saturating_sub(1),
                    ch if ch == self.delimiter && depth == 0 => {
                        let end = start + relative;
                        self.offset = end + ch.len_utf8();
                        return Some(&self.input[start..end]);
                    }
                    _ => {}
                }
            }

            self.offset = self.input.len() + 1;
            Some(&self.input[start..])
        }
    }

    SplitTopLevel {
        input,
        delimiter,
        offset: 0,
    }
}

fn value_to_literal_str(val: &Value) -> String {
    match val {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::String(s) => format!("\"{}\"", s.replace('"', "\\\"")),
        Value::Money(amount, currency) => {
            let major = (*amount as f64) / 100.0;
            format!("{} {}", major, currency)
        }
        Value::Quantity(amount, unit) => {
            format!("{} {}", amount, unit)
        }
        Value::Brand(_, inner) => value_to_literal_str(inner),
        Value::Secret(_) => format!("\"{}\"", redaction::REDACTION_MARKER),
        Value::Uncertain(inner, _) => value_to_literal_str(inner),
        Value::List(items) => {
            let mut parts = Vec::new();
            for item in items {
                parts.push(value_to_literal_str(item));
            }
            format!("[{}]", parts.join(", "))
        }
        Value::Struct(name, fields) => {
            let mut parts = Vec::new();
            for (k, v) in fields {
                parts.push(format!("{}: {}", k, value_to_literal_str(v)));
            }
            if name == "Object" {
                format!("{{ {} }}", parts.join(", "))
            } else {
                format!("{} {{ {} }}", name, parts.join(", "))
            }
        }
        Value::Enum(name, variant, payload) => match payload {
            Some(p) => format!("{}::{}({})", name, variant, value_to_literal_str(p)),
            None => format!("{}::{}", name, variant),
        },
    }
}
