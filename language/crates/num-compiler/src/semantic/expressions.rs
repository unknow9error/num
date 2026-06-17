use super::*;

impl<'a> Checker<'a> {
    pub(super) fn binary_expr(
        &mut self,
        raw: &RawExpr,
        expr: &Expr,
        env: &HashMap<String, Binding>,
    ) {
        match expr {
            Expr::Binary { left, op, right } => {
                self.binary_expr(raw, left, env);
                self.binary_expr(raw, right, env);
                self.binary_operands(raw, *op, left, right, env);
            }
            Expr::Call { callee, args } => {
                self.binary_expr(raw, callee, env);
                for arg in args {
                    self.binary_expr(raw, arg, env);
                }
            }
            Expr::Try(inner) => self.binary_expr(raw, inner, env),
            Expr::Member { object, .. } => self.binary_expr(raw, object, env),
            Expr::Object(fields) => {
                for field in fields {
                    self.binary_expr(raw, &field.value, env);
                }
            }
            Expr::Async(inner) | Expr::Await(inner) => self.binary_expr(raw, inner, env),
            Expr::Quantity(_, _)
            | Expr::Ident(_)
            | Expr::String(_)
            | Expr::Bool(_)
            | Expr::Int(_)
            | Expr::Float(_) => {}
        }
    }

    fn binary_operands(
        &mut self,
        raw: &RawExpr,
        op: BinaryOp,
        left: &Expr,
        right: &Expr,
        env: &HashMap<String, Binding>,
    ) {
        let Some(left_ty) = self.expr_type(left, env) else {
            return;
        };
        let Some(right_ty) = self.expr_type(right, env) else {
            return;
        };

        match op {
            BinaryOp::Or | BinaryOp::And => {
                if !is_bool_type(&self.resolve_aliases(&left_ty))
                    || !is_bool_type(&self.resolve_aliases(&right_ty))
                {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "N3003",
                            format!(
                                "boolean operator `{}` cannot combine `{}` and `{}`",
                                op.as_str(),
                                left_ty.raw,
                                right_ty.raw
                            ),
                            raw.span.clone(),
                        )
                        .with_reason("boolean operators require Bool operands")
                        .with_help("compare values first or pass Bool expressions"),
                    );
                }
            }
            BinaryOp::Equal | BinaryOp::NotEqual => {
                if !self.types_compatible(&left_ty, &right_ty) {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "N3002",
                            format!(
                                "equality operator `{}` cannot compare `{}` and `{}`",
                                op.as_str(),
                                left_ty.raw,
                                right_ty.raw
                            ),
                            raw.span.clone(),
                        )
                        .with_reason("equality comparisons require compatible operand types")
                        .with_help("compare values of the same type or convert explicitly"),
                    );
                }
            }
            BinaryOp::LessThan
            | BinaryOp::LessThanOrEqual
            | BinaryOp::GreaterThan
            | BinaryOp::GreaterThanOrEqual => {
                if !self.types_compatible(&left_ty, &right_ty)
                    || !is_ordered_type(&self.resolve_aliases(&left_ty))
                    || !is_ordered_type(&self.resolve_aliases(&right_ty))
                {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "N3001",
                            format!(
                                "comparison operator `{}` cannot compare `{}` and `{}`",
                                op.as_str(),
                                left_ty.raw,
                                right_ty.raw
                            ),
                            raw.span.clone(),
                        )
                        .with_reason("ordering comparisons require compatible ordered scalar values")
                        .with_help("compare values with the same numeric, decimal, date/time, or duration type"),
                    );
                }
            }
            BinaryOp::Add | BinaryOp::Subtract | BinaryOp::Multiply | BinaryOp::Divide => {
                if arithmetic_result_type(op, &left_ty, &right_ty).is_none() {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "N3004",
                            format!(
                                "arithmetic operator `{}` cannot combine `{}` and `{}`",
                                op.as_str(),
                                left_ty.raw,
                                right_ty.raw
                            ),
                            raw.span.clone(),
                        )
                        .with_reason("arithmetic requires compatible numeric types or explicit Money<C> rules")
                        .with_help("use matching numeric types, matching Money currencies for +/-, or Money multiplied/divided by a numeric value"),
                    );
                }
            }
        }
    }

    pub(super) fn field_access(
        &mut self,
        raw: &RawExpr,
        expr: &Expr,
        env: &HashMap<String, Binding>,
    ) {
        match expr {
            Expr::Member { object, field } => {
                if let Some(base_ty) = self.expr_type(object, env) {
                    if self.is_uncertain_type(&base_ty) {
                        self.uncertain_field(raw, &base_ty, field);
                    } else if self.is_option_type(&base_ty) {
                        self.option_field(raw, object, &base_ty, field, env);
                    } else if self.is_result_type(&base_ty) {
                        self.result_field(raw, object, &base_ty, field, env);
                    } else {
                        self.struct_field(raw, &base_ty, field);
                    }
                }
                self.field_access(raw, object, env);
            }
            Expr::Call { callee, args } => {
                self.field_access(raw, callee, env);
                for arg in args {
                    self.field_access(raw, arg, env);
                }
            }
            Expr::Binary { left, right, .. } => {
                self.field_access(raw, left, env);
                self.field_access(raw, right, env);
            }
            Expr::Try(inner) => self.field_access(raw, inner, env),
            Expr::Object(fields) => {
                for field in fields {
                    self.field_access(raw, &field.value, env);
                }
            }
            Expr::Async(inner) | Expr::Await(inner) => self.field_access(raw, inner, env),
            Expr::Quantity(_, _)
            | Expr::Ident(_)
            | Expr::String(_)
            | Expr::Bool(_)
            | Expr::Int(_)
            | Expr::Float(_) => {}
        }
    }

    fn uncertain_field(&mut self, raw: &RawExpr, base_ty: &TypeRef, field: &str) {
        if matches!(field, "confidence" | "value") {
            return;
        }

        self.diagnostics.push(
            Diagnostic::error(
                "N1301",
                format!("type `{}` has no field `{}`", base_ty.raw, field),
                raw.span.clone(),
            )
            .with_reason("field access must resolve against the value type")
            .with_help(
                "use `.confidence`, `.value`, or a field declared on the underlying value type",
            ),
        );
    }

    fn option_field(
        &mut self,
        raw: &RawExpr,
        object: &Expr,
        base_ty: &TypeRef,
        field: &str,
        env: &HashMap<String, Binding>,
    ) {
        if !matches!(field, "is_some" | "is_none" | "value") {
            self.diagnostics.push(
                Diagnostic::error(
                    "N1301",
                    format!("type `{}` has no field `{}`", base_ty.raw, field),
                    raw.span.clone(),
                )
                .with_reason("field access must resolve against the value type")
                .with_help("use `.is_some`, `.is_none`, `.value`, or a field declared on the unwrapped value type"),
            );
            return;
        }

        if field == "value" && !self.option_value_is_checked(object, env) {
            self.diagnostics.push(
                Diagnostic::error(
                    "N2301",
                    "Option<T>.value used without an is_some check",
                    raw.span.clone(),
                )
                .with_reason("Option<T> may be empty and must be checked before unwrapping")
                .with_help("guard the access with `if option.is_some { ... }`"),
            );
        }
    }

    fn result_field(
        &mut self,
        raw: &RawExpr,
        object: &Expr,
        base_ty: &TypeRef,
        field: &str,
        env: &HashMap<String, Binding>,
    ) {
        if !matches!(field, "is_ok" | "is_err" | "value" | "error") {
            self.diagnostics.push(
                Diagnostic::error(
                    "N1301",
                    format!("type `{}` has no field `{}`", base_ty.raw, field),
                    raw.span.clone(),
                )
                .with_reason("field access must resolve against the value type")
                .with_help("use `.is_ok`, `.is_err`, `.value`, or `.error`"),
            );
            return;
        }

        if field == "value" && !self.result_value_is_checked(object, env, ResultCheck::Ok) {
            self.diagnostics.push(
                Diagnostic::error(
                    "N2302",
                    "Result<T,E>.value used without an is_ok check",
                    raw.span.clone(),
                )
                .with_reason(
                    "Result<T,E> may contain an error and must be checked before unwrapping",
                )
                .with_help("guard the access with `if result.is_ok { ... }`"),
            );
        } else if field == "error" && !self.result_value_is_checked(object, env, ResultCheck::Err) {
            self.diagnostics.push(
                Diagnostic::error(
                    "N2302",
                    "Result<T,E>.error used without an is_err check",
                    raw.span.clone(),
                )
                .with_reason(
                    "Result<T,E> may contain a value and must be checked before reading the error",
                )
                .with_help("guard the access with `if result.is_err { ... }`"),
            );
        }
    }

    fn struct_field(&mut self, raw: &RawExpr, base_ty: &TypeRef, field: &str) {
        if self.has_method(base_ty, field) {
            return;
        }

        let base_name = type_base_name(&base_ty.raw);
        if base_name == "Actor" {
            if actor_member_type(field).is_none() {
                self.diagnostics.push(
                    Diagnostic::error(
                        "N1301",
                        format!("type `{}` has no field `{}`", base_ty.raw, field),
                        raw.span.clone(),
                    )
                    .with_reason("field access must resolve against declared type fields")
                    .with_help(
                        "use `.id`, `.tenant`, `.request_id`, or `.correlation_id` on current_user",
                    ),
                );
            }
            return;
        }
        let Some(fields) = self.type_fields.get(&base_name) else {
            return;
        };

        if !fields.contains_key(field) {
            self.diagnostics.push(
                Diagnostic::error(
                    "N1301",
                    format!("type `{}` has no field `{}`", base_ty.raw, field),
                    raw.span.clone(),
                )
                .with_reason("field access must resolve against declared type fields")
                .with_help("declare the field on the type or use an existing field"),
            );
        }
    }

    pub(super) fn expr_type(&self, expr: &Expr, env: &HashMap<String, Binding>) -> Option<TypeRef> {
        match expr {
            Expr::Ident(name) => env.get(name).and_then(|binding| binding.ty.clone()),
            Expr::String(_) => Some(TypeRef {
                raw: "Text".to_string(),
            }),
            Expr::Bool(_) => Some(TypeRef {
                raw: "Bool".to_string(),
            }),
            Expr::Int(_) => Some(TypeRef {
                raw: "Int".to_string(),
            }),
            Expr::Float(_) => Some(TypeRef {
                raw: "Float".to_string(),
            }),
            Expr::Object(_) => Some(TypeRef {
                raw: "Json".to_string(),
            }),
            Expr::Call { callee, args } => {
                if let Expr::Member { object, field } = callee.as_ref() {
                    if let Some(base_ty) = self.expr_type(object, env) {
                        if let Some(res) = self.method_result_type(&base_ty, field) {
                            return Some(res);
                        }
                    }
                }
                self.unbrand_result_type(callee, args, env)
                    .or_else(|| self.call_result_type(callee))
            }
            Expr::Try(inner) => {
                let inner_ty = self.expr_type(inner, env)?;
                self.result_ok_type(&inner_ty)
            }
            Expr::Member { object, field } => self.member_type(object, field, env),
            Expr::Binary { left, op, right } => {
                let left_ty = self.expr_type(left, env)?;
                let right_ty = self.expr_type(right, env)?;
                binary_result_type(*op, &left_ty, &right_ty)
            }
            Expr::Quantity(_, unit) => {
                if crate::builtins::symbol(unit)
                    .is_some_and(|sym| sym.kind == crate::builtins::BuiltinKind::Currency)
                {
                    Some(TypeRef {
                        raw: format!("Money<{}>", unit),
                    })
                } else if unit == "km" || unit == "Kilometer" {
                    Some(TypeRef {
                        raw: "Distance<Kilometer>".to_string(),
                    })
                } else if unit == "h" || unit == "Hour" {
                    Some(TypeRef {
                        raw: "Duration<Hour>".to_string(),
                    })
                } else if unit == "km/h" || unit == "KilometersPerHour" {
                    Some(TypeRef {
                        raw: "Speed<KilometersPerHour>".to_string(),
                    })
                } else if unit.contains('/') {
                    Some(TypeRef {
                        raw: format!("Speed<{}>", unit),
                    })
                } else {
                    Some(TypeRef {
                        raw: format!("Distance<{}>", unit),
                    })
                }
            }
            Expr::Async(inner) => self.expr_type(inner, env).map(|inner_ty| TypeRef {
                raw: format!("Task<{}>", inner_ty.raw),
            }),
            Expr::Await(inner) => {
                let inner_ty = self.expr_type(inner, env)?;
                self.task_inner_type(&inner_ty).or(Some(inner_ty))
            }
        }
    }

    pub(super) fn expr_type_in_context(
        &self,
        expr: &Expr,
        env: &HashMap<String, Binding>,
        expected: Option<&TypeRef>,
    ) -> Option<TypeRef> {
        if is_result_constructor_expr(expr) {
            return expected.filter(|ty| self.is_result_type(ty)).cloned();
        }
        if is_option_constructor_expr(expr) {
            return self.option_constructor_result_type(expr, env, expected);
        }
        if self.is_enum_constructor_expr(expr) {
            return self.enum_constructor_result_type(expr, expected);
        }
        if self.is_brand_constructor_expr(expr) {
            return self.brand_constructor_result_type_in_context(expr, env, expected);
        }
        self.expr_type(expr, env)
    }

    pub(super) fn call_param_types(&self, callee: &Expr) -> Option<Vec<TypeRef>> {
        let path = callee.path()?;
        match path.as_slice() {
            [call_name] => self
                .callable_signatures
                .get(*call_name)
                .map(|signature| {
                    signature
                        .params
                        .iter()
                        .map(|param| param.ty.clone())
                        .collect()
                })
                .or_else(|| self.enum_constructor_param_types(callee))
                .or_else(|| self.brand_constructor_param_types(callee)),
            [namespace, method] => self
                .connector_methods
                .get(*namespace)
                .and_then(|methods| methods.get(*method))
                .map(|method| method.params.iter().map(|param| param.ty.clone()).collect()),
            _ => None,
        }
    }

    fn call_result_type(&self, callee: &Expr) -> Option<TypeRef> {
        let path = callee.path()?;
        match path.as_slice() {
            [call_name] => self
                .callable_signatures
                .get(*call_name)
                .map(|signature| {
                    signature.result.cloned().unwrap_or(TypeRef {
                        raw: "Unit".to_string(),
                    })
                })
                .or_else(|| self.brand_constructor_result_type(callee)),
            [namespace, method] => self
                .connector_methods
                .get(*namespace)
                .and_then(|methods| methods.get(*method))
                .and_then(|method| method.result.clone()),
            _ => None,
        }
    }

    fn member_type(
        &self,
        object: &Expr,
        field: &str,
        env: &HashMap<String, Binding>,
    ) -> Option<TypeRef> {
        let base_ty = self.expr_type(object, env)?;
        if self.is_uncertain_type(&base_ty) {
            return uncertain_member_type(&self.resolve_aliases(&base_ty), field);
        }
        if self.is_option_type(&base_ty) {
            return option_member_type(&self.resolve_aliases(&base_ty), field);
        }
        if self.is_result_type(&base_ty) {
            return result_member_type(&self.resolve_aliases(&base_ty), field);
        }
        let base_name = type_base_name(&base_ty.raw);
        if base_name == "Actor" {
            return actor_member_type(field);
        }
        let args = generic_args(&base_ty.raw);
        let substitutions = self
            .type_generic_params
            .get(&base_name)
            .map(|params| type_param_substitutions(params, &args))
            .unwrap_or_default();
        self.type_fields
            .get(&base_name)
            .and_then(|fields| fields.get(field))
            .map(|field| substitute_type_params(&field.ty, &substitutions))
    }
}

fn actor_member_type(field: &str) -> Option<TypeRef> {
    match field {
        "id" | "tenant" | "request_id" | "correlation_id" => Some(TypeRef {
            raw: "Text".to_string(),
        }),
        _ => None,
    }
}

fn binary_result_type(op: BinaryOp, left: &TypeRef, right: &TypeRef) -> Option<TypeRef> {
    match op {
        BinaryOp::Or
        | BinaryOp::And
        | BinaryOp::Equal
        | BinaryOp::NotEqual
        | BinaryOp::LessThan
        | BinaryOp::LessThanOrEqual
        | BinaryOp::GreaterThan
        | BinaryOp::GreaterThanOrEqual => Some(TypeRef {
            raw: "Bool".to_string(),
        }),
        BinaryOp::Add | BinaryOp::Subtract | BinaryOp::Multiply | BinaryOp::Divide => {
            arithmetic_result_type(op, left, right)
        }
    }
}

fn uncertain_member_type(base_ty: &TypeRef, field: &str) -> Option<TypeRef> {
    match field {
        "confidence" => Some(TypeRef {
            raw: "Float".to_string(),
        }),
        "value" => generic_arg(&base_ty.raw).map(|raw| TypeRef { raw }),
        _ => None,
    }
}

fn option_member_type(base_ty: &TypeRef, field: &str) -> Option<TypeRef> {
    match field {
        "is_some" | "is_none" => Some(TypeRef {
            raw: "Bool".to_string(),
        }),
        "value" => generic_arg(&base_ty.raw).map(|raw| TypeRef { raw }),
        _ => None,
    }
}

fn result_member_type(base_ty: &TypeRef, field: &str) -> Option<TypeRef> {
    match field {
        "is_ok" | "is_err" => Some(TypeRef {
            raw: "Bool".to_string(),
        }),
        "value" => generic_args(&base_ty.raw)
            .first()
            .cloned()
            .map(|raw| TypeRef { raw }),
        "error" => generic_args(&base_ty.raw)
            .get(1)
            .cloned()
            .map(|raw| TypeRef { raw }),
        _ => None,
    }
}

impl<'a> Checker<'a> {
    pub(super) fn method_result_type(&self, base_ty: &TypeRef, field: &str) -> Option<TypeRef> {
        let base_name = type_base_name(&base_ty.raw);
        self.method_signatures
            .get(&base_name)
            .and_then(|methods| methods.get(field))
            .map(|sig| {
                sig.result.cloned().unwrap_or(TypeRef {
                    raw: "Unit".to_string(),
                })
            })
    }

    pub(super) fn has_method(&self, base_ty: &TypeRef, field: &str) -> bool {
        let base_name = type_base_name(&base_ty.raw);
        self.method_signatures
            .get(&base_name)
            .is_some_and(|methods| methods.contains_key(field))
    }
}
