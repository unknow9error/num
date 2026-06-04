use crate::{RuntimeError, SecurityContext, TenantId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TenantIsolationMode {
    Strict,
    Disabled,
}

impl TenantIsolationMode {
    pub fn enabled(self) -> bool {
        matches!(self, Self::Strict)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TenantGuard {
    mode: TenantIsolationMode,
}

impl TenantGuard {
    pub fn strict() -> Self {
        Self {
            mode: TenantIsolationMode::Strict,
        }
    }

    pub fn disabled() -> Self {
        Self {
            mode: TenantIsolationMode::Disabled,
        }
    }

    pub fn mode(&self) -> TenantIsolationMode {
        self.mode
    }

    pub fn ensure_access(
        &self,
        context: &SecurityContext,
        resource_tenant: &TenantId,
    ) -> Result<(), RuntimeError> {
        if !self.mode.enabled() || context.tenant == *resource_tenant {
            return Ok(());
        }
        Err(RuntimeError::TenantIsolationViolation {
            expected: context.tenant.clone(),
            actual: resource_tenant.clone(),
        })
    }
}

impl Default for TenantGuard {
    fn default() -> Self {
        Self::strict()
    }
}

#[cfg(test)]
mod tests {
    use super::TenantGuard;
    use crate::{RuntimeError, SecurityContext};
    use std::collections::BTreeSet;

    #[test]
    fn strict_tenant_guard_rejects_cross_tenant_access() {
        let guard = TenantGuard::strict();
        let context = security_context("tenant_a");

        let error = guard
            .ensure_access(&context, &"tenant_b".to_string())
            .unwrap_err();

        assert!(matches!(
            error,
            RuntimeError::TenantIsolationViolation { .. }
        ));
    }

    #[test]
    fn disabled_tenant_guard_allows_cross_tenant_access() {
        let guard = TenantGuard::disabled();
        let context = security_context("tenant_a");

        guard
            .ensure_access(&context, &"tenant_b".to_string())
            .unwrap();
    }

    fn security_context(tenant: &str) -> SecurityContext {
        SecurityContext {
            actor: "agent@example.com".to_string(),
            tenant: tenant.to_string(),
            permissions: BTreeSet::new(),
            correlation_id: "corr_1".to_string(),
            request_id: "req_1".to_string(),
        }
    }
}
