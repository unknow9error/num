use super::*;

impl<'a> Checker<'a> {
    pub(super) fn enum_constructor(
        &mut self,
        raw: &RawExpr,
        expr: &Expr,
        env: &HashMap<String, Binding>,
        expected: Option<&TypeRef>,
    ) {
        match expr {
            Expr::Ident(variant) if self.is_enum_constructor_name(variant) => {
                self.enum_constructor_call(raw, variant, &[], env, expected);
            }
            Expr::Call { callee, args } if self.enum_constructor_name(callee).is_some() => {
                let variant = self.enum_constructor_name(callee).unwrap();
                self.enum_constructor_call(raw, variant, args, env, expected);
            }
            Expr::Call { callee, args } => {
                let param_types = self.call_param_types(callee);
                self.enum_constructor(raw, callee, env, None);
                for (index, arg) in args.iter().enumerate() {
                    self.enum_constructor(
                        raw,
                        arg,
                        env,
                        param_types.as_ref().and_then(|params| params.get(index)),
                    );
                }
            }
            Expr::Member { object, .. } => self.enum_constructor(raw, object, env, None),
            Expr::Try(inner) => self.enum_constructor(raw, inner, env, None),
            Expr::Binary { left, right, .. } => {
                self.enum_constructor(raw, left, env, None);
                self.enum_constructor(raw, right, env, None);
            }
            Expr::Object(fields) => {
                for field in fields {
                    self.enum_constructor(raw, &field.value, env, None);
                }
            }
            Expr::Async(inner) | Expr::Await(inner) => {
                self.enum_constructor(raw, inner, env, expected)
            }
            Expr::Quantity(_, _)
            | Expr::Ident(_)
            | Expr::String(_)
            | Expr::Bool(_)
            | Expr::Int(_)
            | Expr::Float(_) => {}
        }
    }

    fn enum_constructor_call(
        &mut self,
        raw: &RawExpr,
        variant: &str,
        args: &[Expr],
        env: &HashMap<String, Binding>,
        expected: Option<&TypeRef>,
    ) {
        let inferred_expected;
        let expected = if let Some(expected) = expected {
            expected
        } else if let Some(inferred) = self.unique_enum_type_for_variant(variant) {
            inferred_expected = inferred;
            &inferred_expected
        } else {
            self.diagnostics.push(
                Diagnostic::error(
                    "N2320",
                    format!("enum variant `{variant}` requires an expected enum type"),
                    raw.span.clone(),
                )
                .with_reason("enum variant constructors are context typed")
                .with_help("use the constructor in a typed binding, return, assignment, or argument position"),
            );
            return;
        };

        let Some(payload) = self.enum_variant_payload(expected, variant) else {
            self.diagnostics.push(
                Diagnostic::error(
                    "N2320",
                    format!(
                        "variant `{variant}` does not belong to enum type `{}`",
                        expected.raw
                    ),
                    raw.span.clone(),
                )
                .with_reason("enum constructors must match the expected enum type")
                .with_help("use a variant declared on the expected enum"),
            );
            return;
        };

        match payload {
            Some(payload_ty) => {
                if args.len() != 1 {
                    self.diagnostics.push(enum_constructor_arity_diagnostic(
                        raw,
                        variant,
                        "exactly one argument",
                    ));
                    return;
                }
                let arg = &args[0];
                self.enum_constructor(raw, arg, env, Some(&payload_ty));
                let Some(actual) = self.expr_type_in_context(arg, env, Some(&payload_ty)) else {
                    return;
                };
                if !self.types_compatible(&payload_ty, &actual) {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "N2322",
                            format!(
                                "variant `{variant}` payload has type `{}`, expected `{}`",
                                actual.raw, payload_ty.raw
                            ),
                            raw.span.clone(),
                        )
                        .with_reason("enum constructor payloads must match variant declarations")
                        .with_help("pass a value with the declared payload type"),
                    );
                }
            }
            None => {
                if !args.is_empty() {
                    self.diagnostics.push(enum_constructor_arity_diagnostic(
                        raw,
                        variant,
                        "no arguments",
                    ));
                }
            }
        }
    }

    pub(super) fn is_enum_constructor_expr(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Ident(name) => self.is_enum_constructor_name(name),
            Expr::Call { callee, .. } => self.enum_constructor_name(callee).is_some(),
            _ => false,
        }
    }

    pub(super) fn is_enum_constructor_name(&self, name: &str) -> bool {
        self.enum_variant_payloads
            .values()
            .any(|variants| variants.contains_key(name))
    }

    pub(super) fn enum_constructor_param_types(&self, callee: &Expr) -> Option<Vec<TypeRef>> {
        let variant = self.enum_constructor_name(callee)?;
        let enum_ty = self.unique_enum_type_for_variant(variant)?;
        self.enum_variant_payload(&enum_ty, variant)?
            .map(|payload| vec![payload])
    }

    pub(super) fn enum_constructor_result_type(
        &self,
        expr: &Expr,
        expected: Option<&TypeRef>,
    ) -> Option<TypeRef> {
        let variant = match expr {
            Expr::Ident(name) if self.is_enum_constructor_name(name) => name.as_str(),
            Expr::Call { callee, .. } => self.enum_constructor_name(callee)?,
            _ => return None,
        };
        let inferred_expected;
        let expected = if let Some(expected) = expected {
            expected
        } else {
            inferred_expected = self.unique_enum_type_for_variant(variant)?;
            &inferred_expected
        };
        self.enum_variant_payload(expected, variant)
            .map(|_| expected.clone())
    }

    pub(super) fn enum_variant_payload(
        &self,
        enum_ty: &TypeRef,
        variant: &str,
    ) -> Option<Option<TypeRef>> {
        let resolved = self.resolve_aliases(enum_ty);
        let enum_name = type_base_name(&resolved.raw);
        self.enum_variant_payloads
            .get(&enum_name)
            .and_then(|variants| variants.get(variant))
            .cloned()
    }

    fn enum_constructor_name<'b>(&self, callee: &'b Expr) -> Option<&'b str> {
        match callee.path()?.as_slice() {
            [name] if self.is_enum_constructor_name(name) => Some(*name),
            _ => None,
        }
    }

    fn unique_enum_type_for_variant(&self, variant: &str) -> Option<TypeRef> {
        let mut matches = self
            .enum_variant_payloads
            .iter()
            .filter(|(_, variants)| variants.contains_key(variant));
        let (enum_name, _) = matches.next()?;
        if matches.next().is_some() {
            return None;
        }
        Some(TypeRef {
            raw: enum_name.clone(),
        })
    }
}

fn enum_constructor_arity_diagnostic(raw: &RawExpr, variant: &str, expected: &str) -> Diagnostic {
    Diagnostic::error(
        "N2321",
        format!("variant `{variant}` expects {expected}"),
        raw.span.clone(),
    )
    .with_reason("enum variant constructors must match declared payload arity")
    .with_help("pass the payload declared on the enum variant")
}
