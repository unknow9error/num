use crate::ast::{PolicyDecl, PolicyEffect, PolicyRule, Privacy, Trust};

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
        let mut allowed = false;
        for rule in &self.rules {
            if !rule_matches(rule, privacy, trust, source, target, tenant) {
                continue;
            }
            match rule.effect {
                PolicyEffect::Allow => allowed = true,
                PolicyEffect::Deny => return false,
            }
        }
        allowed
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
        let mut allowed = false;
        for rule in &self.rules {
            if !targets
                .iter()
                .any(|target| rule_matches(rule, privacy, trust, source, target, tenant))
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
    tenant: Option<&str>,
) -> bool {
    if rule.target.as_deref() != Some(target) {
        return false;
    }
    if let Some(rule_tenant) = rule.tenant.as_deref() {
        if tenant != Some(rule_tenant) {
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
    use super::PolicySet;
    use crate::{
        ast::{PolicyDecl, PolicyEffect, PolicyRule, Privacy, Trust},
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
            raw: String::new(),
            span: Span::synthetic("policy-test"),
        }
    }
}
