use super::*;
use crate::expr::CallRef;

impl<'a> Checker<'a> {
    pub(super) fn trust_flow(
        &mut self,
        raw: &RawExpr,
        expr: &Expr,
        env: &HashMap<String, Binding>,
    ) {
        for call in expr.calls() {
            if self.is_untrusted_ai_sink(&call) {
                self.untrusted_ai_prompt_args(raw, &call, env);
                continue;
            }

            if self.is_untrusted_external_sink(&call) {
                self.untrusted_call_args(raw, &call, env, "external service");
                continue;
            }

            if let Some(action_name) = high_risk_action_name(&call, &self.action_risks) {
                self.untrusted_call_args(
                    raw,
                    &call,
                    env,
                    &format!("high-risk action `{action_name}`"),
                );
            }
        }
    }

    pub(super) fn trust_assignment(
        &mut self,
        stmt: &LetStmt,
        expr: &Expr,
        env: &HashMap<String, Binding>,
    ) {
        if !matches!(stmt.labels.trust, Some(Trust::Trusted | Trust::Verified)) {
            return;
        }
        if !expr_contains_untrusted(expr, env, self) || is_trust_gateway_expr(expr) {
            return;
        }

        self.diagnostics.push(
            Diagnostic::error(
                "N2410",
                format!("binding `{}` cannot promote untrusted data to trusted", stmt.name),
                stmt.span.clone(),
            )
            .with_reason("trust labels must be earned through an explicit validation or review gateway")
            .with_help("wrap the source in sanitize(...), validate_trust(...), verify_trust(...), or require_human_review(...) before assigning a trusted/verified label"),
        );
    }

    pub(super) fn labels_for_let(
        &self,
        explicit: &Labels,
        expr: &Expr,
        env: &HashMap<String, Binding>,
    ) -> Labels {
        let inferred = self.expr_labels(expr, env).unwrap_or_default();
        merge_labels(explicit, &inferred)
    }

    pub(super) fn expr_labels(
        &self,
        expr: &Expr,
        env: &HashMap<String, Binding>,
    ) -> Option<Labels> {
        match expr {
            Expr::Ident(name) => env.get(name).map(|binding| self.binding_labels(binding)),
            Expr::Member { object, field } => self.member_labels(object, field, env),
            Expr::Try(inner) | Expr::Async(inner) | Expr::Await(inner) => {
                self.expr_labels(inner, env)
            }
            Expr::Binary { left, right, .. } => {
                let left_labels = self.expr_labels(left, env);
                let right_labels = self.expr_labels(right, env);
                match (left_labels, right_labels) {
                    (Some(left), Some(right)) => Some(combine_labels(&left, &right)),
                    (Some(labels), None) | (None, Some(labels)) => Some(labels),
                    (None, None) => None,
                }
            }
            Expr::Object(fields) => {
                let mut labels = None;
                for field in fields {
                    if let Some(field_labels) = self.expr_labels(&field.value, env) {
                        labels = Some(match labels {
                            Some(current) => combine_labels(&current, &field_labels),
                            None => field_labels,
                        });
                    }
                }
                labels
            }
            Expr::Call { callee, args } => {
                if is_privacy_gateway_expr(expr) {
                    let mut labels = labels_from_args(args, env, self).unwrap_or_default();
                    labels.source = Some("DerivedData".to_string());
                    labels.privacy = Some(Privacy::Public);
                    return Some(labels);
                }

                if is_trust_gateway_expr(expr) {
                    let mut labels = labels_from_args(args, env, self).unwrap_or_default();
                    labels.trust = Some(Trust::Trusted);
                    return Some(labels);
                }

                let mut labels = self.expr_labels(callee, env);
                for arg in args {
                    if let Some(arg_labels) = self.expr_labels(arg, env) {
                        labels = Some(match labels {
                            Some(current) => combine_labels(&current, &arg_labels),
                            None => arg_labels,
                        });
                    }
                }
                labels
            }
            Expr::String(_)
            | Expr::Bool(_)
            | Expr::Int(_)
            | Expr::Float(_)
            | Expr::Quantity(_, _) => None,
        }
    }

    fn binding_labels(&self, binding: &Binding) -> Labels {
        let mut labels = binding.labels.clone();
        if binding.secret {
            labels.privacy = Some(Privacy::Secret);
        }
        if let Some(ty) = &binding.ty {
            if let Some(type_labels) = self.type_ref_aggregate_labels(ty, &mut HashSet::new()) {
                labels = combine_labels(&labels, &type_labels);
            }
        }
        labels
    }

    fn type_ref_aggregate_labels(
        &self,
        ty: &TypeRef,
        visited: &mut HashSet<String>,
    ) -> Option<Labels> {
        let mut labels = self.intrinsic_type_labels(ty);
        let base_name = type_base_name(&ty.raw);
        if !visited.insert(base_name.clone()) {
            return labels;
        }

        let Some(fields) = self.type_fields.get(&base_name) else {
            visited.remove(&base_name);
            return labels;
        };
        for field in fields.values() {
            let mut field_labels = self.labels_for_declared_type(&field.ty, &field.labels);
            if let Some(nested_labels) = self.type_ref_aggregate_labels(&field.ty, visited) {
                field_labels = combine_labels(&field_labels, &nested_labels);
            }
            labels = Some(match labels {
                Some(current) => combine_labels(&current, &field_labels),
                None => field_labels,
            });
        }
        visited.remove(&base_name);
        labels
    }

    fn labels_for_declared_type(&self, ty: &TypeRef, explicit: &Labels) -> Labels {
        let mut labels = explicit.clone();
        if let Some(type_labels) = self.intrinsic_type_labels(ty) {
            labels = combine_labels(&labels, &type_labels);
        }
        labels
    }

    fn intrinsic_type_labels(&self, ty: &TypeRef) -> Option<Labels> {
        self.resolve_aliases(ty).is_secret().then(|| Labels {
            privacy: Some(Privacy::Secret),
            ..Labels::default()
        })
    }

    fn member_labels(
        &self,
        object: &Expr,
        field: &str,
        env: &HashMap<String, Binding>,
    ) -> Option<Labels> {
        let base_ty = self.expr_type(object, env)?;
        if self.has_method(&base_ty, field) {
            return self.expr_labels(object, env);
        }

        if self.is_uncertain_type(&base_ty)
            || self.is_option_type(&base_ty)
            || self.is_result_type(&base_ty)
        {
            return self.expr_labels(object, env);
        }

        let base_name = type_base_name(&base_ty.raw);
        self.type_fields
            .get(&base_name)
            .and_then(|fields| fields.get(field))
            .map(|field| self.labels_for_declared_type(&field.ty, &field.labels))
            .or_else(|| self.expr_labels(object, env))
    }

    fn is_untrusted_external_sink(&self, call: &CallRef<'_>) -> bool {
        matches!(call.path.as_slice(), ["external", ..])
    }

    fn is_untrusted_ai_sink(&self, call: &CallRef<'_>) -> bool {
        matches!(call.path.as_slice(), ["ai", ..])
    }

    fn untrusted_ai_prompt_args(
        &mut self,
        raw: &RawExpr,
        call: &CallRef<'_>,
        env: &HashMap<String, Binding>,
    ) {
        for arg in call.args {
            let Some(source) = first_untrusted_expr(arg, env, self) else {
                continue;
            };
            self.diagnostics.push(
                Diagnostic::error(
                    "N2412",
                    format!("untrusted data `{source}` cannot be sent to an AI call"),
                    raw.span.clone(),
                )
                .with_reason("untrusted prompts and retrieved content can carry prompt-injection instructions")
                .with_help("sanitize, validate, or require human review before passing this value to ai.*"),
            );
        }
    }

    fn untrusted_call_args(
        &mut self,
        raw: &RawExpr,
        call: &CallRef<'_>,
        env: &HashMap<String, Binding>,
        sink: &str,
    ) {
        for arg in call.args {
            let Some(source) = first_untrusted_expr(arg, env, self) else {
                continue;
            };
            self.diagnostics.push(
                Diagnostic::error(
                    "N2411",
                    format!("untrusted data `{source}` cannot flow into {sink}"),
                    raw.span.clone(),
                )
                .with_reason("untrusted inputs must be validated before they can affect real-world or external actions")
                .with_help("sanitize or verify the value into a trusted binding before passing it to this call"),
            );
        }
    }
}

fn labels_from_args(
    args: &[Expr],
    env: &HashMap<String, Binding>,
    checker: &Checker<'_>,
) -> Option<Labels> {
    let mut labels = None;
    for arg in args {
        if let Some(arg_labels) = checker.expr_labels(arg, env) {
            labels = Some(match labels {
                Some(current) => combine_labels(&current, &arg_labels),
                None => arg_labels,
            });
        }
    }
    labels
}

pub(super) fn merge_labels(explicit: &Labels, inferred: &Labels) -> Labels {
    Labels {
        source: explicit.source.clone().or_else(|| inferred.source.clone()),
        trust: explicit.trust.or(inferred.trust),
        privacy: explicit.privacy.or(inferred.privacy),
    }
}

fn combine_labels(left: &Labels, right: &Labels) -> Labels {
    Labels {
        source: left.source.clone().or_else(|| right.source.clone()),
        trust: combine_trust(left.trust, right.trust),
        privacy: combine_privacy(left.privacy, right.privacy),
    }
}

fn combine_trust(left: Option<Trust>, right: Option<Trust>) -> Option<Trust> {
    match (left, right) {
        (Some(Trust::Untrusted), _) | (_, Some(Trust::Untrusted)) => Some(Trust::Untrusted),
        (Some(Trust::Trusted), _) | (_, Some(Trust::Trusted)) => Some(Trust::Trusted),
        (Some(Trust::Verified), _) | (_, Some(Trust::Verified)) => Some(Trust::Verified),
        (None, None) => None,
    }
}

fn combine_privacy(left: Option<Privacy>, right: Option<Privacy>) -> Option<Privacy> {
    match (left, right) {
        (Some(Privacy::Secret), _) | (_, Some(Privacy::Secret)) => Some(Privacy::Secret),
        (Some(Privacy::Regulated), _) | (_, Some(Privacy::Regulated)) => Some(Privacy::Regulated),
        (Some(Privacy::Sensitive), _) | (_, Some(Privacy::Sensitive)) => Some(Privacy::Sensitive),
        (Some(Privacy::Private), _) | (_, Some(Privacy::Private)) => Some(Privacy::Private),
        (Some(Privacy::Internal), _) | (_, Some(Privacy::Internal)) => Some(Privacy::Internal),
        (Some(Privacy::Public), _) | (_, Some(Privacy::Public)) => Some(Privacy::Public),
        (None, None) => None,
    }
}

fn high_risk_action_name<'a>(
    call: &CallRef<'a>,
    action_risks: &HashMap<String, Risk>,
) -> Option<&'a str> {
    let [action_name] = call.path.as_slice() else {
        return None;
    };
    action_risks
        .get(*action_name)
        .is_some_and(|risk| *risk >= Risk::High)
        .then_some(*action_name)
}

fn first_untrusted_expr(
    expr: &Expr,
    env: &HashMap<String, Binding>,
    checker: &Checker<'_>,
) -> Option<String> {
    match expr {
        Expr::Ident(name) => env.get(name).and_then(|binding| {
            (binding.labels.trust == Some(Trust::Untrusted)).then(|| name.clone())
        }),
        Expr::Member { object, field } => checker
            .member_labels(object, field, env)
            .and_then(|labels| (labels.trust == Some(Trust::Untrusted)).then(|| expr_label(expr)))
            .or_else(|| first_untrusted_expr(object, env, checker)),
        Expr::Call { callee, args } => {
            if is_trust_gateway_expr(expr) {
                return None;
            }
            first_untrusted_expr(callee, env, checker).or_else(|| {
                args.iter()
                    .find_map(|arg| first_untrusted_expr(arg, env, checker))
            })
        }
        Expr::Try(inner) | Expr::Async(inner) | Expr::Await(inner) => {
            first_untrusted_expr(inner, env, checker)
        }
        Expr::Binary { left, right, .. } => first_untrusted_expr(left, env, checker)
            .or_else(|| first_untrusted_expr(right, env, checker)),
        Expr::Object(fields) => fields
            .iter()
            .find_map(|field| first_untrusted_expr(&field.value, env, checker)),
        Expr::String(_) | Expr::Bool(_) | Expr::Int(_) | Expr::Float(_) | Expr::Quantity(_, _) => {
            None
        }
    }
}

fn expr_contains_untrusted(
    expr: &Expr,
    env: &HashMap<String, Binding>,
    checker: &Checker<'_>,
) -> bool {
    first_untrusted_expr(expr, env, checker).is_some()
}

fn is_trust_gateway_expr(expr: &Expr) -> bool {
    expr.calls().iter().any(|call| {
        matches!(
            call.path.as_slice(),
            ["sanitize"]
                | ["validate_trust"]
                | ["verify_trust"]
                | ["require_human_review"]
                | ["require_human_approval"]
        )
    })
}

fn is_privacy_gateway_expr(expr: &Expr) -> bool {
    expr.calls()
        .iter()
        .any(|call| matches!(call.path.as_slice(), ["anonymize"]))
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
