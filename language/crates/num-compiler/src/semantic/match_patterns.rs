use super::*;

impl<'a> Checker<'a> {
    pub(super) fn match_is_exhaustive(
        &self,
        stmt: &MatchStmt,
        env: &HashMap<String, Binding>,
    ) -> bool {
        if stmt
            .arms
            .iter()
            .any(|arm| arm.guard.is_none() && matches!(arm.pattern, MatchPattern::Wildcard))
        {
            return true;
        }

        let match_ty = expr::parse(&stmt.expr.text)
            .ok()
            .and_then(|parsed| self.expr_type(&parsed, env));
        let Some(domain) = match_ty.as_ref().and_then(|ty| self.match_domain(ty)) else {
            return false;
        };
        let labels = domain.labels();

        labels.iter().all(|variant| {
            stmt.arms.iter().any(|arm| {
                arm.guard.is_none() && arm.pattern.variant_name() == Some(variant.as_str())
            })
        })
    }

    pub(super) fn match_stmt(
        &mut self,
        stmt: &MatchStmt,
        env: &mut HashMap<String, Binding>,
        granted: &mut HashSet<String>,
        expected_return: Option<&TypeRef>,
        test_kind: Option<TestKind>,
    ) {
        self.expr(&stmt.expr, env, granted, expected_return, None);

        let parsed_match_expr = expr::parse(&stmt.expr.text).ok();
        let match_ty = parsed_match_expr
            .as_ref()
            .and_then(|parsed| self.expr_type(parsed, env));
        let domain = match_ty.as_ref().and_then(|ty| self.match_domain(ty));
        let labels = domain.as_ref().map(MatchDomain::labels);

        if match_ty.is_some() && domain.is_none() {
            let actual = match_ty
                .as_ref()
                .map(|ty| ty.raw.as_str())
                .unwrap_or("unknown");
            self.diagnostics.push(
                Diagnostic::error(
                    "N1400",
                    format!("cannot match on non-enum/non-union type `{actual}`"),
                    stmt.span.clone(),
                )
                .with_reason("match statements require an enum value or union alias value")
                .with_help("match on an enum, match on a union alias, or use if/else for boolean conditions"),
            );
        }

        if let Some(labels) = labels {
            self.validate_match_arms(stmt, &labels);
            self.validate_match_destructuring(stmt, domain.as_ref());
        }

        for arm in &stmt.arms {
            let mut arm_env = env.clone();
            self.narrow_match_arm(
                &mut arm_env,
                domain.as_ref(),
                parsed_match_expr.as_ref(),
                &arm.pattern,
            );
            self.bind_match_destructuring(&mut arm_env, domain.as_ref(), &arm.pattern);
            self.match_guard(arm, &arm_env, granted, expected_return);
            self.statements(
                &arm.body,
                &mut arm_env,
                &mut granted.clone(),
                expected_return,
                test_kind,
            );
        }
    }

    fn validate_match_arms(&mut self, stmt: &MatchStmt, labels: &[String]) {
        let mut unguarded_seen = HashSet::new();
        let mut has_wildcard = false;
        for arm in &stmt.arms {
            match &arm.pattern {
                MatchPattern::Wildcard => {
                    if arm.guard.is_none() && has_wildcard {
                        self.diagnostics.push(
                            Diagnostic::error(
                                "N1402",
                                "duplicate wildcard match arm",
                                arm.span.clone(),
                            )
                            .with_reason("only one wildcard arm can be reachable")
                            .with_help("remove the duplicate `_` arm"),
                        );
                    }
                    if arm.guard.is_none() {
                        has_wildcard = true;
                    }
                }
                MatchPattern::Variant { name, .. } => {
                    if !labels.contains(name) {
                        self.diagnostics.push(
                            Diagnostic::error(
                                "N1401",
                                format!("unknown match pattern `{name}`"),
                                arm.span.clone(),
                            )
                            .with_reason(
                                "match arm patterns must be enum variants or union member types",
                            )
                            .with_help("use a declared enum variant, a union member type, or add a wildcard `_` arm"),
                        );
                    }
                    if arm.guard.is_none() && !unguarded_seen.insert(name.clone()) {
                        self.diagnostics.push(
                            Diagnostic::error(
                                "N1402",
                                format!("duplicate match arm `{name}`"),
                                arm.span.clone(),
                            )
                            .with_reason("duplicate match arms are unreachable")
                            .with_help("remove the duplicate arm"),
                        );
                    }
                }
            }
        }

        if !has_wildcard {
            let missing: Vec<_> = labels
                .iter()
                .filter(|variant| !unguarded_seen.contains(*variant))
                .cloned()
                .collect();
            if !missing.is_empty() {
                self.diagnostics.push(
                    Diagnostic::error(
                        "N1403",
                        format!("non-exhaustive match, missing {}", missing.join(", ")),
                        stmt.span.clone(),
                    )
                    .with_reason("enum and union matches must cover every case or include `_`")
                    .with_help("add missing match arms or a wildcard `_` arm"),
                );
            }
        }
    }

    fn match_guard(
        &mut self,
        arm: &MatchArm,
        arm_env: &HashMap<String, Binding>,
        granted: &HashSet<String>,
        expected_return: Option<&TypeRef>,
    ) {
        let Some(guard) = &arm.guard else {
            return;
        };
        self.expr(guard, arm_env, granted, expected_return, None);
        let Some(parsed) = self.parse_expr(guard) else {
            return;
        };
        let Some(actual) = self.expr_type(&parsed, arm_env) else {
            return;
        };
        if !is_bool_type(&actual) {
            self.diagnostics.push(
                Diagnostic::error(
                    "N1406",
                    format!("match guard must be Bool, found `{}`", actual.raw),
                    guard.span.clone(),
                )
                .with_reason("match guard clauses decide whether a matched arm can run")
                .with_help("compare values explicitly, for example `if amount > 0`"),
            );
        }
    }

    fn validate_match_destructuring(&mut self, stmt: &MatchStmt, domain: Option<&MatchDomain>) {
        for arm in &stmt.arms {
            let MatchPattern::Variant {
                name,
                payload,
                bindings,
            } = &arm.pattern
            else {
                continue;
            };

            if payload.is_some() && !bindings.is_empty() {
                self.diagnostics.push(
                    Diagnostic::error(
                        "N1404",
                        format!("match pattern `{name}` mixes payload and field destructuring"),
                        arm.span.clone(),
                    )
                    .with_reason("enum payload and struct field patterns are distinct forms")
                    .with_help("use either `Variant(payload)` or `Type { field }`"),
                );
                continue;
            }

            if let Some(payload_pattern) = payload {
                self.validate_enum_payload_pattern(arm, name, payload_pattern, domain);
                continue;
            }

            if bindings.is_empty() {
                continue;
            }

            if !matches!(domain, Some(MatchDomain::Union(_))) {
                self.diagnostics.push(
                    Diagnostic::error(
                        "N1404",
                        format!("match pattern `{name}` cannot destructure this match domain"),
                        arm.span.clone(),
                    )
                    .with_reason("only structured union member patterns can destructure fields")
                    .with_help(
                        "match the case without `{ ... }` or use a union member struct type",
                    ),
                );
                continue;
            }

            let member_ty = TypeRef { raw: name.clone() };
            if self.type_fields.get(&type_base_name(name)).is_none() {
                self.diagnostics.push(
                    Diagnostic::error(
                        "N1404",
                        format!("match pattern `{name}` cannot destructure a non-struct type"),
                        arm.span.clone(),
                    )
                    .with_reason(
                        "destructuring requires declared fields on the matched member type",
                    )
                    .with_help("destructure a structured type or remove `{ ... }` from the arm"),
                );
                continue;
            }

            let mut seen_fields = HashSet::new();
            let mut seen_bindings = HashSet::new();
            for binding in bindings {
                self.validate_match_binding(
                    arm,
                    name,
                    &member_ty,
                    binding,
                    &mut seen_fields,
                    &mut seen_bindings,
                );
            }
        }
    }

    fn validate_match_binding(
        &mut self,
        arm: &MatchArm,
        type_name: &str,
        owner_ty: &TypeRef,
        binding: &MatchBinding,
        seen_fields: &mut HashSet<String>,
        seen_bindings: &mut HashSet<String>,
    ) {
        if !seen_fields.insert(binding.field.clone()) {
            self.diagnostics.push(
                Diagnostic::error(
                    "N1405",
                    format!("duplicate destructured field `{}`", binding.field),
                    arm.span.clone(),
                )
                .with_reason("a match pattern can bind each field only once")
                .with_help("remove the duplicate field binding"),
            );
        }

        let Some((field_ty, _)) = self.struct_field_binding(owner_ty, &binding.field) else {
            self.diagnostics.push(
                Diagnostic::error(
                    "N1404",
                    format!("type `{type_name}` has no field `{}`", binding.field),
                    arm.span.clone(),
                )
                .with_reason("destructured fields must exist on the union member type")
                .with_help("use a declared field name or remove it from the pattern"),
            );
            return;
        };

        if binding.nested.is_empty() {
            if !seen_bindings.insert(binding.name.clone()) {
                self.diagnostics.push(
                    Diagnostic::error(
                        "N1405",
                        format!("duplicate match binding `{}`", binding.name),
                        arm.span.clone(),
                    )
                    .with_reason("bindings introduced by one pattern must be unique")
                    .with_help("rename one binding with `field: alias`"),
                );
            }
            return;
        }

        let Some(nested_type) = &binding.nested_type else {
            return;
        };
        let field_base = type_base_name(&field_ty.raw);
        if field_base != *nested_type {
            self.diagnostics.push(
                Diagnostic::error(
                    "N1404",
                    format!(
                        "field `{}` has type `{}`, not `{nested_type}`",
                        binding.field, field_ty.raw
                    ),
                    arm.span.clone(),
                )
                .with_reason("nested destructuring must name the field's structured type")
                .with_help(format!("use `{field_base}` for this nested pattern")),
            );
            return;
        }
        if self.type_fields.get(&field_base).is_none() {
            self.diagnostics.push(
                Diagnostic::error(
                    "N1404",
                    format!("match pattern `{field_base}` cannot destructure a non-struct type"),
                    arm.span.clone(),
                )
                .with_reason("nested destructuring requires declared fields")
                .with_help("destructure a structured type or bind the field directly"),
            );
            return;
        }

        let mut nested_seen_fields = HashSet::new();
        for nested in &binding.nested {
            self.validate_match_binding(
                arm,
                &field_base,
                &field_ty,
                nested,
                &mut nested_seen_fields,
                seen_bindings,
            );
        }
    }

    fn validate_enum_payload_pattern(
        &mut self,
        arm: &MatchArm,
        variant: &str,
        payload_pattern: &MatchPayloadPattern,
        domain: Option<&MatchDomain>,
    ) {
        if !matches!(domain, Some(MatchDomain::Enum { .. })) {
            self.diagnostics.push(
                Diagnostic::error(
                    "N1404",
                    format!("match pattern `{variant}` cannot bind a payload here"),
                    arm.span.clone(),
                )
                .with_reason("payload patterns are only valid for enum variants with payloads")
                .with_help("use `Variant(payload)` on an enum payload variant"),
            );
            return;
        }

        let Some(enum_ty) = enum_type_from_domain(domain) else {
            return;
        };
        match self.enum_variant_payload(&enum_ty, variant) {
            Some(Some(payload_ty)) => {
                if let MatchPayloadPattern::Destructure {
                    type_name,
                    bindings,
                } = payload_pattern
                {
                    self.validate_enum_payload_destructuring(
                        arm,
                        variant,
                        &payload_ty,
                        type_name,
                        bindings,
                    );
                }
            }
            Some(None) => self.diagnostics.push(
                Diagnostic::error(
                    "N1404",
                    format!("enum variant `{variant}` has no payload"),
                    arm.span.clone(),
                )
                .with_reason("payload binding requires a payload type in the enum declaration")
                .with_help("remove `(payload)` or declare a payload type on the variant"),
            ),
            None => {}
        }
    }

    fn validate_enum_payload_destructuring(
        &mut self,
        arm: &MatchArm,
        variant: &str,
        payload_ty: &TypeRef,
        type_name: &str,
        bindings: &[MatchBinding],
    ) {
        let payload_base = type_base_name(&payload_ty.raw);
        if payload_base != type_name {
            self.diagnostics.push(
                Diagnostic::error(
                    "N1404",
                    format!(
                        "enum variant `{variant}` payload has type `{}`, not `{type_name}`",
                        payload_ty.raw
                    ),
                    arm.span.clone(),
                )
                .with_reason("payload destructuring must name the variant payload struct type")
                .with_help(format!("use `{payload_base}` for this payload pattern")),
            );
            return;
        }
        if self.type_fields.get(&payload_base).is_none() {
            self.diagnostics.push(
                Diagnostic::error(
                    "N1404",
                    format!(
                        "match pattern `{variant}` cannot destructure non-struct payload `{}`",
                        payload_ty.raw
                    ),
                    arm.span.clone(),
                )
                .with_reason("payload destructuring requires a declared struct payload type")
                .with_help("bind the payload directly or use a struct payload type"),
            );
            return;
        }

        let mut seen_fields = HashSet::new();
        let mut seen_bindings = HashSet::new();
        for binding in bindings {
            self.validate_match_binding(
                arm,
                &payload_base,
                payload_ty,
                binding,
                &mut seen_fields,
                &mut seen_bindings,
            );
        }
    }

    fn narrow_match_arm(
        &self,
        arm_env: &mut HashMap<String, Binding>,
        domain: Option<&MatchDomain>,
        parsed_match_expr: Option<&Expr>,
        pattern: &MatchPattern,
    ) {
        let (
            Some(MatchDomain::Union(_)),
            Some(Expr::Ident(binding_name)),
            MatchPattern::Variant { name, .. },
        ) = (domain, parsed_match_expr, pattern)
        else {
            return;
        };

        if let Some(binding) = arm_env.get_mut(binding_name) {
            binding.ty = Some(TypeRef { raw: name.clone() });
        }
    }

    fn bind_match_destructuring(
        &self,
        arm_env: &mut HashMap<String, Binding>,
        domain: Option<&MatchDomain>,
        pattern: &MatchPattern,
    ) {
        if let (Some(MatchDomain::Enum { .. }), MatchPattern::Variant { name, payload, .. }) =
            (domain, pattern)
        {
            let Some(payload_pattern) = payload else {
                return;
            };
            let Some(enum_ty) = enum_type_from_domain(domain) else {
                return;
            };
            let Some(Some(payload_ty)) = self.enum_variant_payload(&enum_ty, name) else {
                return;
            };
            match payload_pattern {
                MatchPayloadPattern::Binding(binding_name) => {
                    arm_env.insert(
                        binding_name.clone(),
                        Binding {
                            ty: Some(payload_ty.clone()),
                            labels: Labels::default(),
                            mutable: false,
                            uncertain: self.is_uncertain_type(&payload_ty),
                            option_checked: false,
                            result_checked: None,
                            secret: self.resolve_aliases(&payload_ty).is_secret(),
                        },
                    );
                }
                MatchPayloadPattern::Destructure { bindings, .. } => {
                    for binding in bindings {
                        self.bind_match_binding(arm_env, &payload_ty, binding);
                    }
                }
            }
            return;
        }

        let (Some(MatchDomain::Union(_)), MatchPattern::Variant { name, bindings, .. }) =
            (domain, pattern)
        else {
            return;
        };

        for binding in bindings {
            let member_ty = TypeRef { raw: name.clone() };
            self.bind_match_binding(arm_env, &member_ty, binding);
        }
    }

    fn bind_match_binding(
        &self,
        arm_env: &mut HashMap<String, Binding>,
        owner_ty: &TypeRef,
        binding: &MatchBinding,
    ) {
        let Some((field_ty, labels)) = self.struct_field_binding(owner_ty, &binding.field) else {
            return;
        };
        if !binding.nested.is_empty() {
            for nested in &binding.nested {
                self.bind_match_binding(arm_env, &field_ty, nested);
            }
            return;
        }
        let secret =
            self.resolve_aliases(&field_ty).is_secret() || labels.privacy == Some(Privacy::Secret);
        arm_env.insert(
            binding.name.clone(),
            Binding {
                ty: Some(field_ty.clone()),
                labels,
                mutable: false,
                uncertain: self.is_uncertain_type(&field_ty),
                option_checked: false,
                result_checked: None,
                secret,
            },
        );
    }

    fn struct_field_binding(&self, ty: &TypeRef, field: &str) -> Option<(TypeRef, Labels)> {
        let base_name = type_base_name(&ty.raw);
        let args = generic_args(&ty.raw);
        let substitutions = self
            .type_generic_params
            .get(&base_name)
            .map(|params| type_param_substitutions(params, &args))
            .unwrap_or_default();
        self.type_fields
            .get(&base_name)
            .and_then(|fields| fields.get(field))
            .map(|field| {
                (
                    substitute_type_params(&field.ty, &substitutions),
                    field.labels.clone(),
                )
            })
    }

    fn match_domain(&self, ty: &TypeRef) -> Option<MatchDomain> {
        let resolved = self.resolve_aliases(ty);
        if let Some(variants) = self.enum_variants.get(&resolved.raw) {
            return Some(MatchDomain::Enum {
                name: resolved.raw.clone(),
                variants: variants.clone(),
            });
        }

        let members = union_members(&resolved.raw);
        (members.len() > 1).then_some(MatchDomain::Union(members))
    }
}

impl MatchDomain {
    fn labels(&self) -> Vec<String> {
        match self {
            MatchDomain::Enum { variants, .. } => variants.clone(),
            MatchDomain::Union(labels) => labels.clone(),
        }
    }
}

fn enum_type_from_domain(domain: Option<&MatchDomain>) -> Option<TypeRef> {
    match domain {
        Some(MatchDomain::Enum { name, .. }) => Some(TypeRef { raw: name.clone() }),
        _ => None,
    }
}

impl MatchPattern {
    fn variant_name(&self) -> Option<&str> {
        match self {
            MatchPattern::Variant { name, .. } => Some(name.as_str()),
            MatchPattern::Wildcard => None,
        }
    }
}
