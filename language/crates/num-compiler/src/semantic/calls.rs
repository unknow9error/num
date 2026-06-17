use super::*;

impl<'a> Checker<'a> {
    pub(super) fn action_permission(
        &mut self,
        raw: &RawExpr,
        expr: &Expr,
        granted: &HashSet<String>,
    ) {
        for call in expr.calls() {
            let [action_name] = call.path.as_slice() else {
                continue;
            };
            let Some(required) = self.action_permissions.get(*action_name) else {
                continue;
            };

            for permission in required {
                if !granted.contains(permission) {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "N2500",
                            format!("action `{action_name}` requires permission `{permission}`"),
                            raw.span.clone(),
                        )
                        .with_reason("external actions must be guarded by an explicit require statement or callable requires clause")
                        .with_help(format!("add `require Permission.{permission} for current_user` before calling the action")),
                    );
                }
            }
        }
    }

    pub(super) fn external_namespace(
        &mut self,
        raw: &RawExpr,
        expr: &Expr,
        env: &HashMap<String, Binding>,
    ) {
        for namespace in root_member_names(expr) {
            if env.contains_key(&namespace)
                || self.external_namespaces.contains(&namespace)
                || is_builtin_runtime_namespace(&namespace)
                || self
                    .module
                    .declarations
                    .iter()
                    .any(|decl| decl.name() == namespace)
            {
                continue;
            }

            self.diagnostics.push(
                Diagnostic::error(
                    "N2700",
                    format!("unknown external namespace `{namespace}`"),
                    raw.span.clone(),
                )
                .with_reason("external calls must be declared as connector or service dependencies")
                .with_help(format!(
                    "add `connector {namespace} {{ ... }}` or call a declared action/function"
                )),
            );
        }
    }

    pub(super) fn connector_call(
        &mut self,
        raw: &RawExpr,
        expr: &Expr,
        env: &HashMap<String, Binding>,
    ) {
        for call in expr.calls() {
            let [namespace, method] = call.path.as_slice() else {
                continue;
            };

            let Some(methods) = self.connector_methods.get(*namespace) else {
                continue;
            };

            let Some(method_decl) = methods.get(*method) else {
                self.diagnostics.push(
                    Diagnostic::error(
                        "N2702",
                        format!("connector `{}` has no method `{}`", namespace, method),
                        raw.span.clone(),
                    )
                    .with_reason("external calls must match declared connector schemas")
                    .with_help(format!(
                        "add `{}(...) -> ...` to connector `{}` or call a declared method",
                        method, namespace
                    )),
                );
                continue;
            };

            if call.args.len() != method_decl.params.len() {
                self.diagnostics.push(
                    Diagnostic::error(
                        "N2703",
                        format!(
                            "connector method `{}.{}` expects {} argument(s), got {}",
                            namespace,
                            method,
                            method_decl.params.len(),
                            call.args.len()
                        ),
                        raw.span.clone(),
                    )
                    .with_reason("connector calls must match the declared method arity")
                    .with_help("pass exactly the parameters declared in the connector schema"),
                );
                continue;
            }

            for (arg, param) in call.args.iter().zip(method_decl.params.iter()) {
                let Some(actual_ty) = self.expr_type_in_context(arg, env, Some(&param.ty)) else {
                    continue;
                };
                if !self.types_compatible(&param.ty, &actual_ty) {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "N2704",
                            format!(
                                "connector argument `{}` for `{}.{}` has type `{}`, expected `{}`",
                                param.name, namespace, method, actual_ty.raw, param.ty.raw
                            ),
                            raw.span.clone(),
                        )
                        .with_reason("connector calls must match declared parameter types")
                        .with_help("pass a value with the connector parameter type"),
                    );
                }
            }
        }
    }

    pub(super) fn direct_call(
        &mut self,
        raw: &RawExpr,
        expr: &Expr,
        env: &HashMap<String, Binding>,
    ) {
        for call in expr.calls() {
            let [call_name] = call.path.as_slice() else {
                continue;
            };

            if is_builtin_runtime_function(call_name)
                || is_result_constructor_name(call_name)
                || is_option_constructor_name(call_name)
                || self.is_enum_constructor_name(call_name)
                || self.is_brand_constructor_name(call_name)
            {
                continue;
            }

            let Some(signature) = self.callable_signatures.get(*call_name) else {
                self.diagnostics.push(
                    Diagnostic::error(
                        "N2701",
                        format!("unknown callable `{call_name}`"),
                        raw.span.clone(),
                    )
                    .with_reason("function and action calls must resolve to an explicit declaration")
                    .with_help(format!(
                        "declare `fn {call_name}(...) {{ ... }}` or `action {call_name}(...) {{ ... }}`"
                    )),
                );
                continue;
            };

            if call.args.len() != signature.params.len() {
                self.diagnostics.push(
                    Diagnostic::error(
                        "N2705",
                        format!(
                            "{} `{}` expects {} argument(s), got {}",
                            callable_kind_label(signature.kind),
                            call_name,
                            signature.params.len(),
                            call.args.len()
                        ),
                        raw.span.clone(),
                    )
                    .with_reason("call arguments must match the declared callable arity")
                    .with_help("pass exactly the parameters declared by the callable"),
                );
                continue;
            }

            for (arg, param) in call.args.iter().zip(signature.params.iter()) {
                let Some(actual_ty) = self.expr_type_in_context(arg, env, Some(&param.ty)) else {
                    continue;
                };
                if !self.types_compatible(&param.ty, &actual_ty) {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "N2706",
                            format!(
                                "argument `{}` for {} `{}` has type `{}`, expected `{}`",
                                param.name,
                                callable_kind_label(signature.kind),
                                call_name,
                                actual_ty.raw,
                                param.ty.raw
                            ),
                            raw.span.clone(),
                        )
                        .with_reason("call arguments must match declared parameter types")
                        .with_help("pass a value with the callable parameter type"),
                    );
                }
            }
        }
    }

    pub(super) fn method_call(
        &mut self,
        raw: &RawExpr,
        expr: &Expr,
        env: &HashMap<String, Binding>,
    ) {
        self.check_method_calls(raw, expr, env);
    }

    fn check_method_calls(&mut self, raw: &RawExpr, expr: &Expr, env: &HashMap<String, Binding>) {
        match expr {
            Expr::Call { callee, args } => {
                if let Expr::Member { object, field } = callee.as_ref() {
                    if let Some(base_ty) = self.expr_type(object, env) {
                        if self.has_method(&base_ty, field) {
                            self.check_method_call(raw, &base_ty, field, args, env);
                        }
                    }
                }
                self.check_method_calls(raw, callee, env);
                for arg in args {
                    self.check_method_calls(raw, arg, env);
                }
            }
            Expr::Member { object, .. } => self.check_method_calls(raw, object, env),
            Expr::Try(inner) => self.check_method_calls(raw, inner, env),
            Expr::Binary { left, right, .. } => {
                self.check_method_calls(raw, left, env);
                self.check_method_calls(raw, right, env);
            }
            Expr::Object(fields) => {
                for field in fields {
                    self.check_method_calls(raw, &field.value, env);
                }
            }
            Expr::Async(inner) | Expr::Await(inner) => {
                self.check_method_calls(raw, inner, env);
            }
            Expr::Ident(_)
            | Expr::String(_)
            | Expr::Bool(_)
            | Expr::Int(_)
            | Expr::Float(_)
            | Expr::Quantity(_, _) => {}
        }
    }

    fn check_method_call(
        &mut self,
        raw: &RawExpr,
        base_ty: &TypeRef,
        method_name: &str,
        args: &[Expr],
        env: &HashMap<String, Binding>,
    ) {
        let base_name = type_base_name(&base_ty.raw);
        let Some(methods) = self.method_signatures.get(&base_name) else {
            return;
        };
        let Some(signature) = methods.get(method_name) else {
            return;
        };

        if args.len() != signature.params.len() {
            self.diagnostics.push(
                Diagnostic::error(
                    "N2705",
                    format!(
                        "method `{}.{}` expects {} argument(s), got {}",
                        base_name,
                        method_name,
                        signature.params.len(),
                        args.len()
                    ),
                    raw.span.clone(),
                )
                .with_reason("call arguments must match the declared method arity")
                .with_help("pass exactly the parameters declared by the method"),
            );
            return;
        }

        for (arg, param) in args.iter().zip(signature.params.iter()) {
            let Some(actual_ty) = self.expr_type_in_context(arg, env, Some(&param.ty)) else {
                continue;
            };
            if !self.types_compatible(&param.ty, &actual_ty) {
                self.diagnostics.push(
                    Diagnostic::error(
                        "N2706",
                        format!(
                            "argument `{}` for method `{}.{}` has type `{}`, expected `{}`",
                            param.name, base_name, method_name, actual_ty.raw, param.ty.raw
                        ),
                        raw.span.clone(),
                    )
                    .with_reason("call arguments must match declared parameter types")
                    .with_help("pass a value with the method parameter type"),
                );
            }
        }
    }
}

fn callable_kind_label(kind: CallableKind) -> &'static str {
    match kind {
        CallableKind::Function => "function",
        CallableKind::Workflow => "workflow",
        CallableKind::Action => "action",
    }
}

fn is_builtin_runtime_namespace(name: &str) -> bool {
    matches!(name, "Permission" | "ai" | "external")
}

fn is_builtin_runtime_function(name: &str) -> bool {
    matches!(
        name,
        "audit"
            | "anonymize"
            | "log"
            | "reject"
            | "require_human_approval"
            | "require_human_review"
            | "sanitize"
            | "unbrand"
            | "validate_trust"
            | "verify_trust"
    )
}
