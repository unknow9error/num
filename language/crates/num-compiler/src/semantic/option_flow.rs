use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OptionCheck {
    Some,
    None,
}

impl<'a> Checker<'a> {
    pub(super) fn checked_option_bindings(
        &self,
        condition: &RawExpr,
        env: &HashMap<String, Binding>,
    ) -> HashSet<String> {
        let Ok(expr) = expr::parse(&condition.text) else {
            return HashSet::new();
        };
        option_checks_implied_by(self, &expr, env, true)
            .into_iter()
            .filter_map(|(name, check)| (check == OptionCheck::Some).then_some(name))
            .collect()
    }

    pub(super) fn else_checked_option_bindings(
        &self,
        condition: &RawExpr,
        env: &HashMap<String, Binding>,
    ) -> HashSet<String> {
        let Ok(expr) = expr::parse(&condition.text) else {
            return HashSet::new();
        };
        option_checks_implied_by(self, &expr, env, false)
            .into_iter()
            .filter_map(|(name, check)| (check == OptionCheck::Some).then_some(name))
            .collect()
    }

    pub(super) fn option_value_is_checked(
        &self,
        object: &Expr,
        env: &HashMap<String, Binding>,
    ) -> bool {
        let Expr::Ident(name) = object else {
            return false;
        };
        env.get(name).is_some_and(|binding| binding.option_checked)
    }
}

fn option_checks_implied_by(
    checker: &Checker<'_>,
    expr: &Expr,
    env: &HashMap<String, Binding>,
    truth: bool,
) -> HashMap<String, OptionCheck> {
    if let Some((name, check)) = option_check_member(checker, expr, env) {
        return HashMap::from([(
            name,
            if truth {
                check
            } else {
                invert_option_check(check)
            },
        )]);
    }

    match expr {
        Expr::Binary { left, op, right } => {
            let left = option_checks_implied_by(checker, left, env, truth);
            let right = option_checks_implied_by(checker, right, env, truth);
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

fn option_check_member(
    checker: &Checker<'_>,
    expr: &Expr,
    env: &HashMap<String, Binding>,
) -> Option<(String, OptionCheck)> {
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
        .is_some_and(|ty| checker.is_option_type(ty))
    {
        return None;
    }
    match field.as_str() {
        "is_some" => Some((name.clone(), OptionCheck::Some)),
        "is_none" => Some((name.clone(), OptionCheck::None)),
        _ => None,
    }
}

fn invert_option_check(check: OptionCheck) -> OptionCheck {
    match check {
        OptionCheck::Some => OptionCheck::None,
        OptionCheck::None => OptionCheck::Some,
    }
}

fn union_consistent(
    left: HashMap<String, OptionCheck>,
    right: HashMap<String, OptionCheck>,
) -> HashMap<String, OptionCheck> {
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
    left: HashMap<String, OptionCheck>,
    right: HashMap<String, OptionCheck>,
) -> HashMap<String, OptionCheck> {
    left.into_iter()
        .filter(|(name, check)| right.get(name).is_some_and(|right| right == check))
        .collect()
}
