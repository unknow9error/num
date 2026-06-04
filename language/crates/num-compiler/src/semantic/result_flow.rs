use super::*;

impl<'a> Checker<'a> {
    pub(super) fn checked_result_bindings(
        &self,
        condition: &RawExpr,
        env: &HashMap<String, Binding>,
    ) -> HashMap<String, ResultCheck> {
        let Ok(expr) = expr::parse(&condition.text) else {
            return HashMap::new();
        };
        result_checks_implied_by(self, &expr, env, true)
    }

    pub(super) fn else_checked_result_bindings(
        &self,
        condition: &RawExpr,
        env: &HashMap<String, Binding>,
    ) -> HashMap<String, ResultCheck> {
        let Ok(expr) = expr::parse(&condition.text) else {
            return HashMap::new();
        };
        result_checks_implied_by(self, &expr, env, false)
    }

    pub(super) fn result_value_is_checked(
        &self,
        object: &Expr,
        env: &HashMap<String, Binding>,
        expected: ResultCheck,
    ) -> bool {
        let Expr::Ident(name) = object else {
            return false;
        };
        env.get(name)
            .and_then(|binding| binding.result_checked)
            .is_some_and(|actual| actual == expected)
    }
}

fn result_checks_implied_by(
    checker: &Checker<'_>,
    expr: &Expr,
    env: &HashMap<String, Binding>,
    truth: bool,
) -> HashMap<String, ResultCheck> {
    if let Some((name, check)) = result_check_member(checker, expr, env) {
        return HashMap::from([(
            name,
            if truth {
                check
            } else {
                invert_result_check(check)
            },
        )]);
    }

    match expr {
        Expr::Binary { left, op, right } => {
            let left = result_checks_implied_by(checker, left, env, truth);
            let right = result_checks_implied_by(checker, right, env, truth);
            match (op, truth) {
                (BinaryOp::And, true) | (BinaryOp::Or, false) => union_consistent(left, right),
                (BinaryOp::And, false) | (BinaryOp::Or, true) => intersection_matching(left, right),
                _ => HashMap::new(),
            }
        }
        Expr::Member { .. }
        | Expr::Call { .. }
        | Expr::Try(_)
        | Expr::Object(_)
        | Expr::Ident(_)
        | Expr::String(_)
        | Expr::Bool(_)
        | Expr::Int(_)
        | Expr::Float(_)
        | Expr::Quantity(_, _)
        | Expr::Async(_)
        | Expr::Await(_) => HashMap::new(),
    }
}

fn result_check_member(
    checker: &Checker<'_>,
    expr: &Expr,
    env: &HashMap<String, Binding>,
) -> Option<(String, ResultCheck)> {
    let Expr::Member { object, field } = expr else {
        return None;
    };
    let Expr::Ident(name) = object.as_ref() else {
        return None;
    };
    let binding = env.get(name)?;
    if !binding
        .ty
        .as_ref()
        .is_some_and(|ty| checker.is_result_type(ty))
    {
        return None;
    }
    match field.as_str() {
        "is_ok" => Some((name.clone(), ResultCheck::Ok)),
        "is_err" => Some((name.clone(), ResultCheck::Err)),
        _ => None,
    }
}

fn invert_result_check(check: ResultCheck) -> ResultCheck {
    match check {
        ResultCheck::Ok => ResultCheck::Err,
        ResultCheck::Err => ResultCheck::Ok,
    }
}

fn union_consistent(
    left: HashMap<String, ResultCheck>,
    right: HashMap<String, ResultCheck>,
) -> HashMap<String, ResultCheck> {
    let mut merged = left;
    let mut conflicts = HashSet::new();
    for (name, check) in right {
        if conflicts.contains(&name) {
            continue;
        }
        if let Some(existing) = merged.get(&name) {
            if *existing != check {
                merged.remove(&name);
                conflicts.insert(name);
            }
        } else {
            merged.insert(name, check);
        }
    }
    merged
}

fn intersection_matching(
    left: HashMap<String, ResultCheck>,
    right: HashMap<String, ResultCheck>,
) -> HashMap<String, ResultCheck> {
    left.into_iter()
        .filter(|(name, check)| right.get(name).is_some_and(|right| right == check))
        .collect()
}
