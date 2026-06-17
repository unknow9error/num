use super::*;

impl<'a> Checker<'a> {
    pub(super) fn brand_constructor(
        &mut self,
        raw: &RawExpr,
        expr: &Expr,
        env: &HashMap<String, Binding>,
        expected: Option<&TypeRef>,
    ) {
        match expr {
            Expr::Call { callee, args } => {
                if let Some(alias_name) = self.brand_constructor_alias_name(callee) {
                    self.brand_constructor_call(raw, &alias_name, args, env, expected);
                }
                if is_unbrand_call(callee) {
                    self.unbrand_call(raw, args, env);
                }

                self.brand_constructor(raw, callee, env, None);
                let param_types = self.call_param_types(callee);
                for (index, arg) in args.iter().enumerate() {
                    let expected_arg = param_types.as_ref().and_then(|params| params.get(index));
                    self.brand_constructor(raw, arg, env, expected_arg);
                }
            }
            Expr::Member { object, .. } => self.brand_constructor(raw, object, env, None),
            Expr::Try(inner) => self.brand_constructor(raw, inner, env, None),
            Expr::Binary { left, right, .. } => {
                self.brand_constructor(raw, left, env, None);
                self.brand_constructor(raw, right, env, None);
            }
            Expr::Object(fields) => {
                for field in fields {
                    self.brand_constructor(raw, &field.value, env, None);
                }
            }
            Expr::Async(inner) | Expr::Await(inner) => {
                self.brand_constructor(raw, inner, env, expected);
            }
            Expr::Ident(_)
            | Expr::String(_)
            | Expr::Bool(_)
            | Expr::Int(_)
            | Expr::Float(_)
            | Expr::Quantity(_, _) => {}
        }
    }

    pub(super) fn is_brand_constructor_name(&self, name: &str) -> bool {
        self.type_aliases
            .get(name)
            .is_some_and(|alias| alias.nominal)
    }

    pub(super) fn is_brand_constructor_expr(&self, expr: &Expr) -> bool {
        let Expr::Call { callee, .. } = expr else {
            return false;
        };
        self.brand_constructor_alias_name(callee).is_some()
    }

    pub(super) fn brand_constructor_param_types(&self, callee: &Expr) -> Option<Vec<TypeRef>> {
        let alias_name = self.brand_constructor_alias_name(callee)?;
        Some(vec![self.concrete_brand_alias_base_type(&alias_name)?])
    }

    pub(super) fn brand_constructor_result_type(&self, callee: &Expr) -> Option<TypeRef> {
        let alias_name = self.brand_constructor_alias_name(callee)?;
        let alias = self.type_aliases.get(&alias_name)?;
        alias
            .generic_params
            .is_empty()
            .then_some(TypeRef { raw: alias_name })
    }

    pub(super) fn brand_constructor_result_type_in_context(
        &self,
        expr: &Expr,
        env: &HashMap<String, Binding>,
        expected: Option<&TypeRef>,
    ) -> Option<TypeRef> {
        let Expr::Call { callee, args } = expr else {
            return None;
        };
        let alias_name = self.brand_constructor_alias_name(callee)?;
        let alias = self.type_aliases.get(&alias_name)?;
        if alias.generic_params.is_empty() {
            return Some(TypeRef { raw: alias_name });
        }
        if let Some(expected) = expected {
            self.generic_brand_constructor(alias_name.as_str(), expected)
                .map(|constructor| constructor.result_ty)
        } else if let Some(arg) = args.first() {
            self.infer_brand_constructor_from_arg(&alias_name, arg, env)
                .map(|constructor| constructor.result_ty)
        } else {
            None
        }
    }

    pub(super) fn unbrand_result_type(
        &self,
        callee: &Expr,
        args: &[Expr],
        env: &HashMap<String, Binding>,
    ) -> Option<TypeRef> {
        if !is_unbrand_call(callee) {
            return None;
        }
        let [arg] = args else {
            return None;
        };
        let arg_ty = self.expr_type(arg, env)?;
        self.branded_value_base_type(&arg_ty)
    }

    pub(super) fn branded_value_base_type(&self, ty: &TypeRef) -> Option<TypeRef> {
        let alias_name = type_base_name(&ty.raw);
        let alias = self.type_aliases.get(&alias_name)?;
        if !alias.nominal {
            return None;
        }
        if alias.generic_params.is_empty() {
            return self.concrete_brand_alias_base_type(&alias_name);
        }
        let args = generic_args(&ty.raw);
        if args.len() != alias.generic_params.len() {
            return None;
        }
        let substitutions = type_param_substitutions(&alias.generic_params, &args);
        let resolved_target = substitute_type_params(&alias.target, &substitutions);
        brand_base_type(&self.resolve_aliases(&resolved_target))
    }

    fn brand_constructor_call(
        &mut self,
        raw: &RawExpr,
        alias_name: &str,
        args: &[Expr],
        env: &HashMap<String, Binding>,
        expected: Option<&TypeRef>,
    ) {
        if args.len() != 1 {
            self.diagnostics.push(
                Diagnostic::error(
                    "N2310",
                    format!("brand constructor `{alias_name}` expects exactly one argument"),
                    raw.span.clone(),
                )
                .with_reason("Brand aliases wrap one value of their base type")
                .with_help(format!(
                    "call `{alias_name}(value)` with a compatible payload"
                )),
            );
            return;
        }

        let constructor = self
            .resolve_brand_constructor(alias_name, expected)
            .or_else(|| {
                args.first()
                    .and_then(|arg| self.infer_brand_constructor_from_arg(alias_name, arg, env))
            });
        let Some(constructor) = constructor else {
            self.diagnostics.push(
                Diagnostic::error(
                    "N2312",
                    format!("generic brand constructor `{alias_name}` requires an expected type"),
                    raw.span.clone(),
                )
                .with_reason("generic Brand aliases need concrete type arguments from context")
                .with_help(format!(
                    "use `{alias_name}` in a typed context such as `let id: {alias_name}<User> = {alias_name}(value)`"
                )),
            );
            return;
        };
        let arg = &args[0];
        let Some(actual) = self.expr_type_in_context(arg, env, Some(&constructor.base_ty)) else {
            return;
        };
        if !self.types_compatible(&constructor.base_ty, &actual) {
            self.diagnostics.push(
                Diagnostic::error(
                    "N2311",
                    format!(
                        "brand constructor `{alias_name}` payload has type `{}`, expected `{}`",
                        actual.raw, constructor.base_ty.raw
                    ),
                    raw.span.clone(),
                )
                .with_reason("Brand<T,Tag> constructors must receive a value compatible with T")
                .with_help(
                    "convert or validate the raw value before constructing the branded type",
                ),
            );
        }
    }

    fn unbrand_call(&mut self, raw: &RawExpr, args: &[Expr], env: &HashMap<String, Binding>) {
        if args.len() != 1 {
            self.diagnostics.push(
                Diagnostic::error(
                    "N2313",
                    format!("unbrand expects exactly one argument, got {}", args.len()),
                    raw.span.clone(),
                )
                .with_reason("unbrand unwraps one Brand alias value")
                .with_help("call `unbrand(value)` with a branded alias value"),
            );
            return;
        }

        let Some(actual) = self.expr_type(&args[0], env) else {
            return;
        };
        if self.branded_value_base_type(&actual).is_none() {
            self.diagnostics.push(
                Diagnostic::error(
                    "N2314",
                    format!("unbrand cannot unwrap non-branded type `{}`", actual.raw),
                    raw.span.clone(),
                )
                .with_reason("only explicit Brand aliases can be unwrapped")
                .with_help("pass a value whose type is a Brand alias, for example `UserId`"),
            );
        }
    }

    fn brand_constructor_alias_name(&self, callee: &Expr) -> Option<String> {
        let path = callee.path()?;
        let [alias_name] = path.as_slice() else {
            return None;
        };
        self.is_brand_constructor_name(alias_name)
            .then(|| (*alias_name).to_string())
    }

    fn resolve_brand_constructor(
        &self,
        alias_name: &str,
        expected: Option<&TypeRef>,
    ) -> Option<ResolvedBrandConstructor> {
        let alias = self.type_aliases.get(alias_name)?;
        if !alias.nominal {
            return None;
        }
        if alias.generic_params.is_empty() {
            return self
                .concrete_brand_alias_base_type(alias_name)
                .map(|base_ty| ResolvedBrandConstructor {
                    base_ty,
                    result_ty: TypeRef {
                        raw: alias_name.to_string(),
                    },
                });
        }
        self.generic_brand_constructor(alias_name, expected?)
    }

    fn generic_brand_constructor(
        &self,
        alias_name: &str,
        expected: &TypeRef,
    ) -> Option<ResolvedBrandConstructor> {
        let alias = self.type_aliases.get(alias_name)?;
        if !alias.nominal || alias.generic_params.is_empty() {
            return None;
        }
        if type_base_name(&expected.raw) != alias_name {
            return None;
        }
        let args = generic_args(&expected.raw);
        if args.len() != alias.generic_params.len() {
            return None;
        }
        let substitutions = type_param_substitutions(&alias.generic_params, &args);
        let resolved_target = substitute_type_params(&alias.target, &substitutions);
        brand_base_type(&self.resolve_aliases(&resolved_target)).map(|base_ty| {
            ResolvedBrandConstructor {
                base_ty,
                result_ty: expected.clone(),
            }
        })
    }

    fn concrete_brand_alias_base_type(&self, alias_name: &str) -> Option<TypeRef> {
        let alias = self.type_aliases.get(alias_name)?;
        if !alias.nominal || !alias.generic_params.is_empty() {
            return None;
        }
        brand_base_type(&self.resolve_aliases(&alias.target))
    }

    fn infer_brand_constructor_from_arg(
        &self,
        alias_name: &str,
        arg: &Expr,
        env: &HashMap<String, Binding>,
    ) -> Option<ResolvedBrandConstructor> {
        let alias = self.type_aliases.get(alias_name)?;
        if !alias.nominal || alias.generic_params.is_empty() {
            return None;
        }
        let actual_arg_ty = self.expr_type(arg, env)?;
        let target_args = generic_args(&alias.target.raw);
        if target_args.len() != 2 {
            return None;
        }
        let base_pattern = &target_args[0];
        if alias.generic_params.contains(base_pattern) {
            let concrete_args: Vec<String> = alias
                .generic_params
                .iter()
                .map(|param| {
                    if param == base_pattern {
                        actual_arg_ty.raw.clone()
                    } else {
                        "Unit".to_string()
                    }
                })
                .collect();
            let result_raw = format!("{}<{}>", alias_name, concrete_args.join(", "));
            let expected_ty = TypeRef { raw: result_raw };
            return self.generic_brand_constructor(alias_name, &expected_ty);
        }
        None
    }
}

struct ResolvedBrandConstructor {
    base_ty: TypeRef,
    result_ty: TypeRef,
}

fn is_unbrand_call(callee: &Expr) -> bool {
    callee
        .path()
        .is_some_and(|path| path.as_slice() == ["unbrand"])
}

fn brand_base_type(ty: &TypeRef) -> Option<TypeRef> {
    let args = generic_args(&ty.raw);
    if type_base_name(&ty.raw) != "Brand" || args.len() != 2 {
        return None;
    }
    Some(TypeRef {
        raw: args[0].clone(),
    })
}
