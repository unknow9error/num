use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OptionConstructor {
    Some,
    None,
}

impl<'a> Checker<'a> {
    pub(super) fn option_constructor(
        &mut self,
        raw: &RawExpr,
        expr: &Expr,
        env: &HashMap<String, Binding>,
        expected: Option<&TypeRef>,
    ) {
        match expr {
            Expr::Ident(name) if name == "None" => {
                self.none_constructor(raw, expected);
            }
            Expr::Call { callee, args } => {
                if let Some(constructor) = option_constructor_kind(callee) {
                    self.option_constructor_call(raw, constructor, args, env, expected);
                    return;
                }

                let param_types = self.call_param_types(callee);
                self.option_constructor(raw, callee, env, None);
                for (index, arg) in args.iter().enumerate() {
                    self.option_constructor(
                        raw,
                        arg,
                        env,
                        param_types.as_ref().and_then(|params| params.get(index)),
                    );
                }
            }
            Expr::Member { object, .. } => self.option_constructor(raw, object, env, None),
            Expr::Try(inner) => self.option_constructor(raw, inner, env, None),
            Expr::Binary { left, right, .. } => {
                self.option_constructor(raw, left, env, None);
                self.option_constructor(raw, right, env, None);
            }
            Expr::Object(fields) => {
                for field in fields {
                    self.option_constructor(raw, &field.value, env, None);
                }
            }
            Expr::Async(inner) | Expr::Await(inner) => {
                self.option_constructor(raw, inner, env, expected)
            }
            Expr::Quantity(_, _)
            | Expr::Ident(_)
            | Expr::String(_)
            | Expr::Bool(_)
            | Expr::Int(_)
            | Expr::Float(_) => {}
        }
    }

    fn option_constructor_call(
        &mut self,
        raw: &RawExpr,
        constructor: OptionConstructor,
        args: &[Expr],
        env: &HashMap<String, Binding>,
        expected: Option<&TypeRef>,
    ) {
        match constructor {
            OptionConstructor::Some => self.some_constructor(raw, args, env, expected),
            OptionConstructor::None => {
                if !args.is_empty() {
                    self.diagnostics.push(option_constructor_arity_diagnostic(
                        raw,
                        "None",
                        "no arguments",
                    ));
                    return;
                }
                self.none_constructor(raw, expected);
            }
        }
    }

    fn some_constructor(
        &mut self,
        raw: &RawExpr,
        args: &[Expr],
        env: &HashMap<String, Binding>,
        expected: Option<&TypeRef>,
    ) {
        if expected.is_none() {
            if args.len() != 1 {
                self.diagnostics.push(option_constructor_arity_diagnostic(
                    raw,
                    "Some",
                    "exactly one argument",
                ));
                return;
            }
            self.option_constructor(raw, &args[0], env, None);
            return;
        }

        let Some(expected) = expected else {
            self.diagnostics
                .push(expected_option_diagnostic(raw, "Some"));
            return;
        };

        if !self.is_option_type(expected) {
            self.diagnostics
                .push(non_option_diagnostic(raw, "Some", expected));
            return;
        }

        if args.len() != 1 {
            self.diagnostics.push(option_constructor_arity_diagnostic(
                raw,
                "Some",
                "exactly one argument",
            ));
            return;
        }

        let Some(inner_ty) = self.option_inner_type(expected) else {
            return;
        };
        let arg = &args[0];
        self.option_constructor(raw, arg, env, Some(&inner_ty));
        let Some(actual) = self.expr_type_in_context(arg, env, Some(&inner_ty)) else {
            return;
        };
        if !self.types_compatible(&inner_ty, &actual) {
            self.diagnostics.push(
                Diagnostic::error(
                    "N2308",
                    format!(
                        "`Some` payload has type `{}`, expected `{}`",
                        actual.raw, inner_ty.raw
                    ),
                    raw.span.clone(),
                )
                .with_reason("Option constructor payloads must match Option<T>")
                .with_help("pass a value with the expected Option payload type"),
            );
        }
    }

    fn none_constructor(&mut self, raw: &RawExpr, expected: Option<&TypeRef>) {
        let Some(expected) = expected else {
            self.diagnostics
                .push(expected_option_diagnostic(raw, "None"));
            return;
        };

        if !self.is_option_type(expected) {
            self.diagnostics
                .push(non_option_diagnostic(raw, "None", expected));
        }
    }

    pub(super) fn option_constructor_result_type(
        &self,
        expr: &Expr,
        env: &HashMap<String, Binding>,
        expected: Option<&TypeRef>,
    ) -> Option<TypeRef> {
        if let Some(expected) = expected.filter(|ty| self.is_option_type(ty)) {
            return Some(expected.clone());
        }

        let Expr::Call { callee, args } = expr else {
            return None;
        };
        if option_constructor_kind(callee)? != OptionConstructor::Some || args.len() != 1 {
            return None;
        }
        let inner_ty = self.expr_type(&args[0], env)?;
        Some(TypeRef {
            raw: format!("Option<{}>", inner_ty.raw),
        })
    }
}

fn option_constructor_kind(callee: &Expr) -> Option<OptionConstructor> {
    match callee.path()?.as_slice() {
        ["Some"] => Some(OptionConstructor::Some),
        ["None"] => Some(OptionConstructor::None),
        _ => None,
    }
}

pub(super) fn is_option_constructor_expr(expr: &Expr) -> bool {
    match expr {
        Expr::Ident(name) if name == "None" => true,
        Expr::Call { callee, .. } => option_constructor_kind(callee).is_some(),
        _ => false,
    }
}

pub(super) fn is_option_constructor_name(name: &str) -> bool {
    matches!(name, "Some" | "None")
}

fn expected_option_diagnostic(raw: &RawExpr, constructor: &str) -> Diagnostic {
    Diagnostic::error(
        "N2307",
        format!("`{constructor}` requires an expected Option<T> type"),
        raw.span.clone(),
    )
    .with_reason("Option constructors are context typed")
    .with_help("use the constructor in a typed binding, assignment, argument, or return position")
}

fn non_option_diagnostic(raw: &RawExpr, constructor: &str, expected: &TypeRef) -> Diagnostic {
    Diagnostic::error(
        "N2307",
        format!(
            "`{constructor}` cannot construct non-Option type `{}`",
            expected.raw
        ),
        raw.span.clone(),
    )
    .with_reason("Some(...) and None construct Option<T> values")
    .with_help("use an Option<T> expected type or pass the plain value directly")
}

fn option_constructor_arity_diagnostic(
    raw: &RawExpr,
    constructor: &str,
    expected: &str,
) -> Diagnostic {
    Diagnostic::error(
        "N2307",
        format!("`{constructor}` expects {expected}"),
        raw.span.clone(),
    )
    .with_reason("Option constructor arity must match Option<T>")
    .with_help("pass the payload required by the constructor and expected Option type")
}
