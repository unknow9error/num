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

    pub(super) fn actor_runtime_call(&mut self, raw: &RawExpr, expr: &Expr) {
        for call in expr.calls() {
            let [actor_name, handler_name] = call.path.as_slice() else {
                continue;
            };

            let Some(handlers) = self.actor_handlers.get(*actor_name) else {
                continue;
            };

            let detail = if handlers.contains_key(*handler_name) {
                format!(
                    "actor handler `{}.{}` cannot be executed yet",
                    actor_name, handler_name
                )
            } else {
                format!("actor `{actor_name}` has no executable handler `{handler_name}`")
            };
            self.diagnostics.push(
                Diagnostic::error("N2708", detail, raw.span.clone())
                    .with_reason("actor declarations are currently parser/formatter/IR metadata only")
                    .with_help("keep actor declarations for design documentation, but do not call actor handlers until actor runtime support lands"),
            );
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

            if is_scalar_validator(call_name) {
                self.scalar_validator_call(raw, call_name, &call.args, env);
                continue;
            }
            if is_hash_helper(call_name) {
                self.hash_helper_call(raw, call_name, &call.args, env);
                continue;
            }
            if is_bytes_xml_helper(call_name) {
                self.bytes_xml_helper_call(raw, call_name, &call.args, env);
                continue;
            }
            if is_document_helper(call_name) {
                self.document_helper_call(raw, call_name, &call.args, env);
                continue;
            }
            if is_datetime_duration_helper(call_name) {
                self.datetime_duration_call(raw, call_name, &call.args, env);
                continue;
            }
            if is_decimal_helper(call_name) {
                self.decimal_helper_call(raw, call_name, &call.args, env);
                continue;
            }
            if is_money_helper(call_name) {
                self.money_helper_call(raw, call_name, &call.args, env);
                continue;
            }
            if is_collection_helper(call_name) {
                self.collection_helper_call(raw, call_name, &call.args, env);
                continue;
            }

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

    fn scalar_validator_call(
        &mut self,
        raw: &RawExpr,
        call_name: &str,
        args: &[Expr],
        env: &HashMap<String, Binding>,
    ) {
        if args.len() != 1 {
            self.diagnostics.push(
                Diagnostic::error(
                    "N2705",
                    format!(
                        "validator `{call_name}` expects 1 argument, got {}",
                        args.len()
                    ),
                    raw.span.clone(),
                )
                .with_reason("scalar validators validate exactly one text value")
                .with_help("pass one Text expression to the validator"),
            );
            return;
        }

        let Some(actual_ty) = self.expr_type_in_context(
            &args[0],
            env,
            Some(&TypeRef {
                raw: "Text".to_string(),
            }),
        ) else {
            return;
        };
        if !self.types_compatible(
            &TypeRef {
                raw: "Text".to_string(),
            },
            &actual_ty,
        ) {
            self.diagnostics.push(
                Diagnostic::error(
                    "N2706",
                    format!(
                        "validator `{call_name}` argument has type `{}`, expected `Text`",
                        actual_ty.raw
                    ),
                    raw.span.clone(),
                )
                .with_reason("scalar validators operate on raw text input")
                .with_help("convert the value to Text before validating it"),
            );
        }

        if let Expr::String(value) = &args[0] {
            if let Err(reason) = validate_scalar_value(call_name, value) {
                self.diagnostics.push(
                    Diagnostic::error(
                        "N2707",
                        format!("invalid literal for `{call_name}`: {reason}"),
                        raw.span.clone(),
                    )
                    .with_reason("literal scalar values can be validated at compile time")
                    .with_help("fix the literal or validate dynamic user input at runtime"),
                );
            }
        }
    }

    fn hash_helper_call(
        &mut self,
        raw: &RawExpr,
        call_name: &str,
        args: &[Expr],
        env: &HashMap<String, Binding>,
    ) {
        if args.len() != 1 {
            self.diagnostics.push(
                Diagnostic::error(
                    "N2705",
                    format!(
                        "hash helper `{call_name}` expects 1 argument, got {}",
                        args.len()
                    ),
                    raw.span.clone(),
                )
                .with_reason("hash helpers digest exactly one Text or Bytes value")
                .with_help("pass one Text or Bytes expression to the helper"),
            );
            return;
        }

        let Some(actual_ty) = self.expr_type(&args[0], env) else {
            return;
        };
        if !matches!(actual_ty.raw.as_str(), "Text" | "Bytes") {
            self.diagnostics.push(
                Diagnostic::error(
                    "N2706",
                    format!(
                        "hash helper `{call_name}` argument has type `{}`, expected `Text` or `Bytes`",
                        actual_ty.raw
                    ),
                    raw.span.clone(),
                )
                .with_reason("hash helpers require explicit text or byte input")
                .with_help("convert the value to Text or Bytes before hashing"),
            );
        }
    }

    fn bytes_xml_helper_call(
        &mut self,
        raw: &RawExpr,
        call_name: &str,
        args: &[Expr],
        env: &HashMap<String, Binding>,
    ) {
        if args.len() != 1 {
            self.diagnostics.push(
                Diagnostic::error(
                    "N2705",
                    format!(
                        "bytes/xml helper `{call_name}` expects 1 argument, got {}",
                        args.len()
                    ),
                    raw.span.clone(),
                )
                .with_reason("Bytes and Xml helpers parse or format exactly one value")
                .with_help("pass one value with the helper input type"),
            );
            return;
        }

        let expected = bytes_xml_helper_param_types(call_name)
            .and_then(|mut params| params.pop())
            .expect("known bytes/xml helper must have one param type");
        let Some(actual_ty) = self.expr_type_in_context(&args[0], env, Some(&expected)) else {
            return;
        };
        if !self.types_compatible(&expected, &actual_ty) {
            self.diagnostics.push(
                Diagnostic::error(
                    "N2706",
                    format!(
                        "bytes/xml helper `{call_name}` argument has type `{}`, expected `{}`",
                        actual_ty.raw, expected.raw
                    ),
                    raw.span.clone(),
                )
                .with_reason("Bytes and Xml helpers use explicit Text, Bytes, and Xml boundaries")
                .with_help("parse text first or pass a value with the expected stdlib type"),
            );
            return;
        }

        if call_name == "xml_parse" {
            if let Expr::String(value) = &args[0] {
                if let Err(reason) = validate_xml_literal(value) {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "N2707",
                            format!("invalid literal for `{call_name}`: {reason}"),
                            raw.span.clone(),
                        )
                        .with_reason("literal XML values can be validated at compile time")
                        .with_help("pass XML text with at least one element tag"),
                    );
                }
            }
        }
    }

    fn document_helper_call(
        &mut self,
        raw: &RawExpr,
        call_name: &str,
        args: &[Expr],
        env: &HashMap<String, Binding>,
    ) {
        let expected =
            document_helper_param_types(call_name).expect("known document helper param types");
        if args.len() != expected.len() {
            self.diagnostics.push(
                Diagnostic::error(
                    "N2705",
                    format!(
                        "document helper `{call_name}` expects {} arguments, got {}",
                        expected.len(),
                        args.len()
                    ),
                    raw.span.clone(),
                )
                .with_reason("Document metadata construction uses the fixed first-slice shape")
                .with_help("pass id, name, mime_type, size_bytes, source, privacy, and trust"),
            );
            return;
        }

        for (index, (arg, expected_ty)) in args.iter().zip(expected.iter()).enumerate() {
            let Some(actual_ty) = self.expr_type_in_context(arg, env, Some(expected_ty)) else {
                continue;
            };
            if !self.types_compatible(expected_ty, &actual_ty) {
                self.diagnostics.push(
                    Diagnostic::error(
                        "N2706",
                        format!(
                            "document helper `{call_name}` argument {} has type `{}`, expected `{}`",
                            index + 1,
                            actual_ty.raw,
                            expected_ty.raw
                        ),
                        raw.span.clone(),
                    )
                    .with_reason("Document metadata fields have fixed Text and Int types")
                    .with_help("pass Text metadata fields and Int size_bytes"),
                );
            }
        }
    }

    fn datetime_duration_call(
        &mut self,
        raw: &RawExpr,
        call_name: &str,
        args: &[Expr],
        env: &HashMap<String, Binding>,
    ) {
        if args.len() != 1 {
            self.diagnostics.push(
                Diagnostic::error(
                    "N2705",
                    format!(
                        "date/time helper `{call_name}` expects 1 argument, got {}",
                        args.len()
                    ),
                    raw.span.clone(),
                )
                .with_reason("date/time helpers parse or format exactly one value")
                .with_help("pass one value with the helper input type"),
            );
            return;
        }

        let expected = datetime_duration_param_types(call_name)
            .and_then(|mut params| params.pop())
            .expect("known date/time helper must have one param type");
        let Some(actual_ty) = self.expr_type_in_context(&args[0], env, Some(&expected)) else {
            return;
        };
        if !self.types_compatible(&expected, &actual_ty) {
            self.diagnostics.push(
                Diagnostic::error(
                    "N2706",
                    format!(
                        "date/time helper `{call_name}` argument has type `{}`, expected `{}`",
                        actual_ty.raw, expected.raw
                    ),
                    raw.span.clone(),
                )
                .with_reason(
                    "date/time helpers use explicit DateTime and Duration<Hour> boundaries",
                )
                .with_help("parse text first or pass a value with the expected date/time type"),
            );
            return;
        }

        if let Expr::String(value) = &args[0] {
            if let Err(reason) = validate_datetime_duration_literal(call_name, value) {
                self.diagnostics.push(
                    Diagnostic::error(
                        "N2707",
                        format!("invalid literal for `{call_name}`: {reason}"),
                        raw.span.clone(),
                    )
                    .with_reason("literal date/time values can be validated at compile time")
                    .with_help("use explicit UTC ISO timestamps and hour durations"),
                );
            }
        }
    }

    fn decimal_helper_call(
        &mut self,
        raw: &RawExpr,
        call_name: &str,
        args: &[Expr],
        env: &HashMap<String, Binding>,
    ) {
        if args.len() != 1 {
            self.diagnostics.push(
                Diagnostic::error(
                    "N2705",
                    format!(
                        "decimal helper `{call_name}` expects 1 argument, got {}",
                        args.len()
                    ),
                    raw.span.clone(),
                )
                .with_reason("decimal helpers parse or format exactly one value")
                .with_help("pass one value with the helper input type"),
            );
            return;
        }

        let expected = decimal_helper_param_types(call_name)
            .and_then(|mut params| params.pop())
            .expect("known decimal helper must have one param type");
        let Some(actual_ty) = self.expr_type_in_context(&args[0], env, Some(&expected)) else {
            return;
        };
        if !self.types_compatible(&expected, &actual_ty) {
            self.diagnostics.push(
                Diagnostic::error(
                    "N2706",
                    format!(
                        "decimal helper `{call_name}` argument has type `{}`, expected `{}`",
                        actual_ty.raw, expected.raw
                    ),
                    raw.span.clone(),
                )
                .with_reason("decimal helpers use explicit Text and Decimal boundaries")
                .with_help("parse text first or pass a Decimal value"),
            );
            return;
        }

        if call_name == "decimal_parse" {
            if let Expr::String(value) = &args[0] {
                if let Err(reason) = validate_decimal_literal(value) {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "N2707",
                            format!("invalid literal for `{call_name}`: {reason}"),
                            raw.span.clone(),
                        )
                        .with_reason("literal Decimal values can be validated at compile time")
                        .with_help("use digits with at most one decimal point"),
                    );
                }
            }
        }
    }

    fn collection_helper_call(
        &mut self,
        raw: &RawExpr,
        call_name: &str,
        args: &[Expr],
        env: &HashMap<String, Binding>,
    ) {
        if matches!(
            call_name,
            "map_empty" | "set_empty" | "queue_empty" | "stack_empty" | "stream_empty"
        ) {
            if !args.is_empty() {
                self.diagnostics.push(
                    Diagnostic::error(
                        "N2705",
                        format!(
                            "collection helper `{call_name}` expects 0 arguments, got {}",
                            args.len()
                        ),
                        raw.span.clone(),
                    )
                    .with_reason("empty collection constructors infer their type from context")
                    .with_help("assign the empty constructor to an explicit collection type"),
                );
            }
            return;
        }

        let expected = collection_helper_param_types(call_name, args, env, self);
        let Some(expected) = expected else {
            self.diagnostics.push(
                Diagnostic::error(
                    "N2705",
                    format!("collection helper `{call_name}` received unsupported arguments"),
                    raw.span.clone(),
                )
                .with_reason("collection helpers require a typed collection as their first argument")
                .with_help("pass a compatible Map, Set, Queue, Stack, or Stream value with matching item arguments"),
            );
            return;
        };

        if args.len() != expected.len() {
            self.diagnostics.push(
                Diagnostic::error(
                    "N2705",
                    format!(
                        "collection helper `{call_name}` expects {} argument(s), got {}",
                        expected.len(),
                        args.len()
                    ),
                    raw.span.clone(),
                )
                .with_reason("collection helper arity is fixed by the helper name")
                .with_help("use map_contains/map_get/map_insert/map_remove or set_contains/set_insert/set_remove with the documented arity"),
            );
            return;
        }

        for (index, (arg, expected_ty)) in args.iter().zip(expected.iter()).enumerate() {
            let Some(actual_ty) = self.expr_type_in_context(arg, env, Some(expected_ty)) else {
                continue;
            };
            if !self.types_compatible(expected_ty, &actual_ty) {
                self.diagnostics.push(
                    Diagnostic::error(
                        "N2706",
                        format!(
                            "collection helper `{call_name}` argument {} has type `{}`, expected `{}`",
                            index + 1,
                            actual_ty.raw,
                            expected_ty.raw
                        ),
                        raw.span.clone(),
                    )
                    .with_reason("Map/Set operations preserve their generic key and value types")
                    .with_help("pass keys and values compatible with the collection type parameters"),
                );
            }
        }
    }

    fn money_helper_call(
        &mut self,
        raw: &RawExpr,
        call_name: &str,
        args: &[Expr],
        env: &HashMap<String, Binding>,
    ) {
        let expected = money_helper_param_types(call_name, args, env, self);
        let Some(expected) = expected else {
            self.diagnostics.push(
                Diagnostic::error(
                    "N2705",
                    format!("money helper `{call_name}` received unsupported arguments"),
                    raw.span.clone(),
                )
                .with_reason("Money conversion requires an explicit ExchangeRate<From, To> boundary")
                .with_help("assign exchange_rate(...) to ExchangeRate<From, To>, then pass Money<From> to convert_money"),
            );
            return;
        };

        if args.len() != expected.len() {
            self.diagnostics.push(
                Diagnostic::error(
                    "N2705",
                    format!(
                        "money helper `{call_name}` expects {} argument(s), got {}",
                        expected.len(),
                        args.len()
                    ),
                    raw.span.clone(),
                )
                .with_reason("Money helpers use fixed, explicit exchange-rate signatures")
                .with_help(
                    "use exchange_rate(from, to, rate, source) or convert_money(amount, rate)",
                ),
            );
            return;
        }

        for (index, (arg, expected_ty)) in args.iter().zip(expected.iter()).enumerate() {
            let Some(actual_ty) = self.expr_type_in_context(arg, env, Some(expected_ty)) else {
                continue;
            };
            if !self.types_compatible(expected_ty, &actual_ty) {
                self.diagnostics.push(
                    Diagnostic::error(
                        "N2706",
                        format!(
                            "money helper `{call_name}` argument {} has type `{}`, expected `{}`",
                            index + 1,
                            actual_ty.raw,
                            expected_ty.raw
                        ),
                        raw.span.clone(),
                    )
                    .with_reason("Money conversion must match the source currency carried by the ExchangeRate type")
                    .with_help("use an ExchangeRate<From, To> whose From currency matches the Money<From> amount"),
                );
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
            | "hash_sha256_base64"
            | "hash_sha256_hex"
            | "bytes_from_base64"
            | "bytes_from_text"
            | "bytes_len"
            | "bytes_to_base64"
            | "docx_metadata"
            | "docx_parse_metadata"
            | "document_metadata"
            | "document_extraction_error"
            | "document_extraction_metadata"
            | "datetime_format_iso"
            | "datetime_parse_iso"
            | "decimal_format"
            | "decimal_parse"
            | "exchange_rate"
            | "convert_money"
            | "duration_format_hours"
            | "duration_parse_hours"
            | "map_contains"
            | "map_empty"
            | "map_get"
            | "map_insert"
            | "map_remove"
            | "image_metadata"
            | "image_parse_metadata"
            | "ocr_result"
            | "extracted_document_text"
            | "pdf_metadata"
            | "pdf_parse_metadata"
            | "spreadsheet_metadata"
            | "spreadsheet_parse_metadata"
            | "spreadsheet_sheet_metadata"
            | "set_contains"
            | "set_empty"
            | "set_insert"
            | "set_remove"
            | "validate_email"
            | "validate_phone_number"
            | "validate_trust"
            | "validate_url"
            | "validate_uuid"
            | "verify_trust"
            | "xml_parse"
            | "xml_to_text"
    )
}
