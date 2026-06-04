use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResultConstructor {
    Ok,
    Err,
}

impl<'a> Checker<'a> {
    pub(super) fn result_constructor(
        &mut self,
        raw: &RawExpr,
        expr: &Expr,
        env: &HashMap<String, Binding>,
        expected: Option<&TypeRef>,
    ) {
        match expr {
            Expr::Call { callee, args } => {
                if let Some(constructor) = result_constructor_kind(callee) {
                    self.result_constructor_call(raw, constructor, args, env, expected);
                    return;
                }

                let param_types = self.call_param_types(callee);
                self.result_constructor(raw, callee, env, None);
                for (index, arg) in args.iter().enumerate() {
                    self.result_constructor(
                        raw,
                        arg,
                        env,
                        param_types.as_ref().and_then(|params| params.get(index)),
                    );
                }
            }
            Expr::Member { object, .. } => self.result_constructor(raw, object, env, None),
            Expr::Try(inner) => self.result_constructor(raw, inner, env, None),
            Expr::Binary { left, right, .. } => {
                self.result_constructor(raw, left, env, None);
                self.result_constructor(raw, right, env, None);
            }
            Expr::Object(fields) => {
                for field in fields {
                    self.result_constructor(raw, &field.value, env, None);
                }
            }
            Expr::Async(inner) | Expr::Await(inner) => self.result_constructor(raw, inner, env, expected),
            Expr::Quantity(_, _) | Expr::Ident(_) | Expr::String(_) | Expr::Bool(_) | Expr::Int(_) | Expr::Float(_) => {}
        }
    }

    fn result_constructor_call(
        &mut self,
        raw: &RawExpr,
        constructor: ResultConstructor,
        args: &[Expr],
        env: &HashMap<String, Binding>,
        expected: Option<&TypeRef>,
    ) {
        let Some(expected) = expected else {
            self.diagnostics.push(
                Diagnostic::error(
                    "N2305",
                    format!("`{}` requires an expected Result<T,E> type", constructor.name()),
                    raw.span.clone(),
                )
                .with_reason("Result constructors are context typed")
                .with_help("use the constructor in a typed return, typed binding, assignment, or typed argument position"),
            );
            return;
        };

        if !self.is_result_type(expected) {
            self.diagnostics.push(
                Diagnostic::error(
                    "N2305",
                    format!(
                        "`{}` cannot construct non-Result type `{}`",
                        constructor.name(),
                        expected.raw
                    ),
                    raw.span.clone(),
                )
                .with_reason("Ok(...) and Err(...) construct Result<T,E> values")
                .with_help("use a Result<T,E> expected type or return the plain value directly"),
            );
            return;
        }

        match constructor {
            ResultConstructor::Ok => self.ok_constructor(raw, args, env, expected),
            ResultConstructor::Err => self.err_constructor(raw, args, env, expected),
        }
    }

    fn ok_constructor(
        &mut self,
        raw: &RawExpr,
        args: &[Expr],
        env: &HashMap<String, Binding>,
        expected: &TypeRef,
    ) {
        let Some(ok_ty) = self.result_ok_type(expected) else {
            return;
        };
        if ok_ty.raw == "Unit" {
            if args.len() > 1 {
                self.diagnostics.push(result_constructor_arity_diagnostic(
                    raw,
                    "Ok",
                    "zero or one argument for Result<Unit,E>",
                ));
            }
        } else if args.len() != 1 {
            self.diagnostics.push(result_constructor_arity_diagnostic(
                raw,
                "Ok",
                "exactly one argument",
            ));
            return;
        }

        if let Some(arg) = args.first() {
            self.result_constructor(raw, arg, env, Some(&ok_ty));
            self.result_constructor_payload(raw, "Ok", arg, env, &ok_ty);
        }
    }

    fn err_constructor(
        &mut self,
        raw: &RawExpr,
        args: &[Expr],
        env: &HashMap<String, Binding>,
        expected: &TypeRef,
    ) {
        if args.len() != 1 {
            self.diagnostics.push(result_constructor_arity_diagnostic(
                raw,
                "Err",
                "exactly one argument",
            ));
            return;
        }

        let Some(err_ty) = self.result_err_type(expected) else {
            return;
        };
        let arg = &args[0];
        self.result_constructor(raw, arg, env, Some(&err_ty));
        self.result_constructor_payload(raw, "Err", arg, env, &err_ty);
    }

    fn result_constructor_payload(
        &mut self,
        raw: &RawExpr,
        constructor: &str,
        arg: &Expr,
        env: &HashMap<String, Binding>,
        expected: &TypeRef,
    ) {
        let Some(actual) = self.expr_type_in_context(arg, env, Some(expected)) else {
            return;
        };
        if !self.types_compatible(expected, &actual) {
            self.diagnostics.push(
                Diagnostic::error(
                    "N2306",
                    format!(
                        "`{constructor}` payload has type `{}`, expected `{}`",
                        actual.raw, expected.raw
                    ),
                    raw.span.clone(),
                )
                .with_reason("Result constructor payloads must match Result<T,E>")
                .with_help("pass a value with the expected Result payload type"),
            );
        }
    }
}

impl ResultConstructor {
    fn name(self) -> &'static str {
        match self {
            ResultConstructor::Ok => "Ok",
            ResultConstructor::Err => "Err",
        }
    }
}

pub(super) fn is_result_constructor_expr(expr: &Expr) -> bool {
    result_constructor_for_expr(expr).is_some()
}

fn result_constructor_for_expr(expr: &Expr) -> Option<ResultConstructor> {
    let Expr::Call { callee, .. } = expr else {
        return None;
    };
    result_constructor_kind(callee)
}

fn result_constructor_kind(callee: &Expr) -> Option<ResultConstructor> {
    match callee.path()?.as_slice() {
        ["Ok"] => Some(ResultConstructor::Ok),
        ["Err"] => Some(ResultConstructor::Err),
        _ => None,
    }
}

pub(super) fn is_result_constructor_name(name: &str) -> bool {
    matches!(name, "Ok" | "Err")
}

fn result_constructor_arity_diagnostic(
    raw: &RawExpr,
    constructor: &str,
    expected: &str,
) -> Diagnostic {
    Diagnostic::error(
        "N2305",
        format!("`{constructor}` expects {expected}"),
        raw.span.clone(),
    )
    .with_reason("Result constructor arity must match Result<T,E>")
    .with_help("pass the payload required by the constructor and expected Result type")
}
