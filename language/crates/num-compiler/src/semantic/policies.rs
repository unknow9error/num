use crate::ast::{PolicyDecl, PolicyEffect, PolicyRule, Privacy, Trust};

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct PolicyContext<'a> {
    pub tenant: Option<&'a str>,
    pub route: Option<RouteContext<'a>>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct RouteContext<'a> {
    pub method: &'a str,
    pub path: &'a str,
}

#[derive(Debug, Clone, Default)]
pub(super) struct PolicySet<'a> {
    rules: Vec<&'a PolicyRule>,
}

impl<'a> PolicySet<'a> {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn extend_policy(&mut self, policy: &'a PolicyDecl) {
        self.rules.extend(policy.rules.iter());
    }

    #[allow(dead_code)]
    pub(super) fn is_data_flow_allowed(
        &self,
        privacy: Option<Privacy>,
        trust: Option<Trust>,
        source: Option<&str>,
        target: &str,
    ) -> bool {
        self.is_data_flow_allowed_for_tenant(privacy, trust, source, target, None)
    }

    #[allow(dead_code)]
    pub(super) fn is_data_flow_allowed_for_tenant(
        &self,
        privacy: Option<Privacy>,
        trust: Option<Trust>,
        source: Option<&str>,
        target: &str,
        tenant: Option<&str>,
    ) -> bool {
        self.is_data_flow_allowed_in_context(
            privacy,
            trust,
            source,
            target,
            PolicyContext {
                tenant,
                route: None,
            },
        )
    }

    pub(super) fn is_data_flow_allowed_in_context(
        &self,
        privacy: Option<Privacy>,
        trust: Option<Trust>,
        source: Option<&str>,
        target: &str,
        context: PolicyContext<'_>,
    ) -> bool {
        self.matching_rules_allow(privacy, trust, source, &[target.to_string()], context)
    }

    #[allow(dead_code)]
    pub(super) fn is_data_flow_allowed_to_any(
        &self,
        privacy: Option<Privacy>,
        trust: Option<Trust>,
        source: Option<&str>,
        targets: &[String],
    ) -> bool {
        self.is_data_flow_allowed_to_any_for_tenant(privacy, trust, source, targets, None)
    }

    pub(super) fn is_data_flow_allowed_to_any_for_tenant(
        &self,
        privacy: Option<Privacy>,
        trust: Option<Trust>,
        source: Option<&str>,
        targets: &[String],
        tenant: Option<&str>,
    ) -> bool {
        self.is_data_flow_allowed_to_any_in_context(
            privacy,
            trust,
            source,
            targets,
            PolicyContext {
                tenant,
                route: None,
            },
        )
    }

    pub(super) fn is_data_flow_allowed_to_any_in_context(
        &self,
        privacy: Option<Privacy>,
        trust: Option<Trust>,
        source: Option<&str>,
        targets: &[String],
        context: PolicyContext<'_>,
    ) -> bool {
        self.matching_rules_allow(privacy, trust, source, targets, context)
    }

    fn matching_rules_allow(
        &self,
        privacy: Option<Privacy>,
        trust: Option<Trust>,
        source: Option<&str>,
        targets: &[String],
        context: PolicyContext<'_>,
    ) -> bool {
        let mut allowed = false;
        for rule in &self.rules {
            if !targets
                .iter()
                .any(|target| rule_matches(rule, privacy, trust, source, target, context))
            {
                continue;
            }
            match rule.effect {
                PolicyEffect::Allow => allowed = true,
                PolicyEffect::Deny => return false,
            }
        }
        allowed
    }
}

fn rule_matches(
    rule: &PolicyRule,
    privacy: Option<Privacy>,
    trust: Option<Trust>,
    source: Option<&str>,
    target: &str,
    context: PolicyContext<'_>,
) -> bool {
    if rule.target.as_deref() != Some(target) {
        return false;
    }
    if let Some(rule_tenant) = rule.tenant.as_deref() {
        if context.tenant != Some(rule_tenant) {
            return false;
        }
    }
    if let Some(rule_route) = &rule.route {
        let Some(route) = context.route else {
            return false;
        };
        if !route.method.eq_ignore_ascii_case(&rule_route.method) || route.path != rule_route.path {
            return false;
        }
    }
    if rule.privacy.is_some() && rule.privacy != privacy {
        return false;
    }
    if rule.trust.is_some() && rule.trust != trust {
        return false;
    }
    if let Some(rule_source) = rule.source.as_deref() {
        return source == Some(rule_source);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::{PolicyContext, PolicySet, RouteContext};
    use crate::{
        ast::{PolicyDecl, PolicyEffect, PolicyRouteCondition, PolicyRule, Privacy, Trust},
        span::Span,
    };

    #[test]
    fn composed_policies_allow_matching_flow() {
        let policy = policy(
            "AllowPublic",
            &[allow(
                Some(Privacy::Public),
                Some("PublicData"),
                "ExternalApi",
            )],
        );
        let mut policies = PolicySet::new();
        policies.extend_policy(&policy);

        assert!(policies.is_data_flow_allowed(
            Some(Privacy::Public),
            None,
            Some("PublicData"),
            "ExternalApi"
        ));
    }

    #[test]
    fn composed_policies_give_deny_precedence() {
        let allow_policy = policy(
            "AllowPrivate",
            &[allow(
                Some(Privacy::Private),
                Some("UserInput"),
                "ExternalApi",
            )],
        );
        let deny_policy = policy(
            "DenyPrivate",
            &[deny(
                Some(Privacy::Private),
                Some("UserInput"),
                "ExternalApi",
            )],
        );
        let mut policies = PolicySet::new();
        policies.extend_policy(&allow_policy);
        policies.extend_policy(&deny_policy);

        assert!(!policies.is_data_flow_allowed(
            Some(Privacy::Private),
            None,
            Some("UserInput"),
            "ExternalApi"
        ));
    }

    #[test]
    fn source_specific_rule_does_not_match_unlabeled_source() {
        let policy = policy(
            "AllowPrivateUserInput",
            &[allow(
                Some(Privacy::Private),
                Some("UserInput"),
                "ExternalApi",
            )],
        );
        let mut policies = PolicySet::new();
        policies.extend_policy(&policy);

        assert!(!policies.is_data_flow_allowed(Some(Privacy::Private), None, None, "ExternalApi"));
    }

    #[test]
    fn tenant_scoped_rule_requires_matching_tenant_context() {
        let policy = policy(
            "TenantScoped",
            &[allow_for_tenant(
                Some(Privacy::Private),
                Some("UserInput"),
                "ExternalApi",
                "tenant_a",
            )],
        );
        let mut policies = PolicySet::new();
        policies.extend_policy(&policy);

        assert!(policies.is_data_flow_allowed_for_tenant(
            Some(Privacy::Private),
            None,
            Some("UserInput"),
            "ExternalApi",
            Some("tenant_a")
        ));
        assert!(!policies.is_data_flow_allowed_for_tenant(
            Some(Privacy::Private),
            None,
            Some("UserInput"),
            "ExternalApi",
            Some("tenant_b")
        ));
        assert!(!policies.is_data_flow_allowed(
            Some(Privacy::Private),
            None,
            Some("UserInput"),
            "ExternalApi"
        ));
    }

    #[test]
    fn trust_scoped_rule_requires_matching_trust_label() {
        let policy = policy(
            "TrustScoped",
            &[allow_with_trust(
                Some(Privacy::Private),
                Some(Trust::Verified),
                Some("UserInput"),
                "ExternalApi",
            )],
        );
        let mut policies = PolicySet::new();
        policies.extend_policy(&policy);

        assert!(policies.is_data_flow_allowed(
            Some(Privacy::Private),
            Some(Trust::Verified),
            Some("UserInput"),
            "ExternalApi"
        ));
        assert!(!policies.is_data_flow_allowed(
            Some(Privacy::Private),
            Some(Trust::Trusted),
            Some("UserInput"),
            "ExternalApi"
        ));
        assert!(!policies.is_data_flow_allowed(
            Some(Privacy::Private),
            None,
            Some("UserInput"),
            "ExternalApi"
        ));
    }

    #[test]
    fn route_condition_requires_matching_route_context() {
        let policy = policy(
            "RouteScoped",
            &[allow_for_route(
                Some(Privacy::Private),
                Some("HttpBody"),
                "ExternalApi",
                "POST",
                "/documents",
            )],
        );
        let mut policies = PolicySet::new();
        policies.extend_policy(&policy);

        assert!(policies.is_data_flow_allowed_in_context(
            Some(Privacy::Private),
            None,
            Some("HttpBody"),
            "ExternalApi",
            PolicyContext {
                tenant: None,
                route: Some(RouteContext {
                    method: "post",
                    path: "/documents"
                }),
            }
        ));
        assert!(!policies.is_data_flow_allowed_in_context(
            Some(Privacy::Private),
            None,
            Some("HttpBody"),
            "ExternalApi",
            PolicyContext {
                tenant: None,
                route: Some(RouteContext {
                    method: "POST",
                    path: "/refunds"
                }),
            }
        ));
        assert!(!policies.is_data_flow_allowed(
            Some(Privacy::Private),
            None,
            Some("HttpBody"),
            "ExternalApi"
        ));
    }

    fn policy(name: &str, rules: &[PolicyRule]) -> PolicyDecl {
        PolicyDecl {
            name: name.to_string(),
            rules: rules.to_vec(),
            span: Span::synthetic("policy-test"),
        }
    }

    fn allow(privacy: Option<Privacy>, source: Option<&str>, target: &str) -> PolicyRule {
        rule(PolicyEffect::Allow, privacy, None, source, target, None)
    }

    fn allow_with_trust(
        privacy: Option<Privacy>,
        trust: Option<Trust>,
        source: Option<&str>,
        target: &str,
    ) -> PolicyRule {
        rule(PolicyEffect::Allow, privacy, trust, source, target, None)
    }

    fn allow_for_tenant(
        privacy: Option<Privacy>,
        source: Option<&str>,
        target: &str,
        tenant: &str,
    ) -> PolicyRule {
        rule(
            PolicyEffect::Allow,
            privacy,
            None,
            source,
            target,
            Some(tenant),
        )
    }

    fn allow_for_route(
        privacy: Option<Privacy>,
        source: Option<&str>,
        target: &str,
        method: &str,
        path: &str,
    ) -> PolicyRule {
        let mut rule = rule(PolicyEffect::Allow, privacy, None, source, target, None);
        rule.route = Some(PolicyRouteCondition {
            method: method.to_string(),
            path: path.to_string(),
        });
        rule
    }

    fn deny(privacy: Option<Privacy>, source: Option<&str>, target: &str) -> PolicyRule {
        rule(PolicyEffect::Deny, privacy, None, source, target, None)
    }

    fn rule(
        effect: PolicyEffect,
        privacy: Option<Privacy>,
        trust: Option<Trust>,
        source: Option<&str>,
        target: &str,
        tenant: Option<&str>,
    ) -> PolicyRule {
        PolicyRule {
            effect,
            privacy,
            trust,
            source: source.map(str::to_string),
            target: Some(target.to_string()),
            tenant: tenant.map(str::to_string),
            route: None,
            raw: String::new(),
            span: Span::synthetic("policy-test"),
        }
    }
}
