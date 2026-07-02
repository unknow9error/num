use crate::{RuntimeError, SecretStore, SecretValue};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalSecretRef {
    backend: String,
    name: String,
}

impl ExternalSecretRef {
    pub fn new(backend: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            backend: backend.into(),
            name: name.into(),
        }
    }

    pub fn parse(value: &str) -> Result<Self, RuntimeError> {
        let Some(rest) = value.strip_prefix("secret://") else {
            return Err(RuntimeError::Storage(format!(
                "external secret reference `{}` must start with secret://",
                sanitize_secret_ref(value)
            )));
        };
        let Some((backend, name)) = rest.split_once('/') else {
            return Err(RuntimeError::Storage(format!(
                "external secret reference `{}` must include backend and name",
                sanitize_secret_ref(value)
            )));
        };
        if backend.is_empty() || name.is_empty() {
            return Err(RuntimeError::Storage(format!(
                "external secret reference `{}` must include non-empty backend and name",
                sanitize_secret_ref(value)
            )));
        }
        Ok(Self::new(backend, name))
    }

    pub fn backend(&self) -> &str {
        &self.backend
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

pub trait ExternalSecretBackend {
    fn backend_id(&self) -> &str;
    fn get_external_secret(&self, name: &str) -> Result<SecretValue, ExternalSecretError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExternalSecretError {
    Missing,
    Denied,
    Unavailable { reason: String },
}

pub struct ExternalSecretStore<B> {
    backend: B,
}

impl<B> ExternalSecretStore<B> {
    pub fn new(backend: B) -> Self {
        Self { backend }
    }
}

impl<B: ExternalSecretBackend> SecretStore for ExternalSecretStore<B> {
    fn put_secret(&mut self, name: &str, _value: SecretValue) -> Result<(), RuntimeError> {
        Err(RuntimeError::SecretUnavailable {
            backend: self.backend.backend_id().to_string(),
            reason: format!(
                "external secret `{}` is read-only through this adapter boundary",
                sanitize_secret_ref(name)
            ),
        })
    }

    fn get_secret(&self, name: &str) -> Result<SecretValue, RuntimeError> {
        let reference = ExternalSecretRef::parse(name)?;
        if reference.backend() != self.backend.backend_id() {
            return Err(RuntimeError::SecretUnavailable {
                backend: reference.backend().to_string(),
                reason: format!(
                    "configured adapter handles `{}`",
                    sanitize_secret_ref(self.backend.backend_id())
                ),
            });
        }
        self.backend
            .get_external_secret(reference.name())
            .map_err(|err| map_external_secret_error(&reference, err))
    }

    fn delete_secret(&mut self, name: &str) -> Result<(), RuntimeError> {
        Err(RuntimeError::SecretUnavailable {
            backend: self.backend.backend_id().to_string(),
            reason: format!(
                "external secret `{}` is read-only through this adapter boundary",
                sanitize_secret_ref(name)
            ),
        })
    }
}

#[derive(Debug, Clone)]
pub struct StubExternalSecretBackend {
    backend_id: String,
    values: HashMap<String, SecretValue>,
    denied: HashSet<String>,
    unavailable: Option<String>,
}

impl StubExternalSecretBackend {
    pub fn new(backend_id: impl Into<String>) -> Self {
        Self {
            backend_id: backend_id.into(),
            values: HashMap::new(),
            denied: HashSet::new(),
            unavailable: None,
        }
    }

    pub fn with_secret(mut self, name: impl Into<String>, value: SecretValue) -> Self {
        self.values.insert(name.into(), value);
        self
    }

    pub fn with_denied(mut self, name: impl Into<String>) -> Self {
        self.denied.insert(name.into());
        self
    }

    pub fn unavailable(mut self, reason: impl Into<String>) -> Self {
        self.unavailable = Some(reason.into());
        self
    }
}

impl ExternalSecretBackend for StubExternalSecretBackend {
    fn backend_id(&self) -> &str {
        &self.backend_id
    }

    fn get_external_secret(&self, name: &str) -> Result<SecretValue, ExternalSecretError> {
        if let Some(reason) = &self.unavailable {
            return Err(ExternalSecretError::Unavailable {
                reason: reason.clone(),
            });
        }
        if self.denied.contains(name) {
            return Err(ExternalSecretError::Denied);
        }
        self.values
            .get(name)
            .cloned()
            .ok_or(ExternalSecretError::Missing)
    }
}

fn map_external_secret_error(
    reference: &ExternalSecretRef,
    err: ExternalSecretError,
) -> RuntimeError {
    match err {
        ExternalSecretError::Missing => RuntimeError::SecretNotFound {
            name: format!("secret://{}/{}", reference.backend(), reference.name()),
        },
        ExternalSecretError::Denied => RuntimeError::SecretDenied {
            name: format!("secret://{}/{}", reference.backend(), reference.name()),
        },
        ExternalSecretError::Unavailable { reason } => RuntimeError::SecretUnavailable {
            backend: reference.backend().to_string(),
            reason,
        },
    }
}

#[derive(Debug, Clone, Default)]
pub struct MemorySecretStore {
    values: HashMap<String, SecretValue>,
}

impl MemorySecretStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl SecretStore for MemorySecretStore {
    fn put_secret(&mut self, name: &str, value: SecretValue) -> Result<(), RuntimeError> {
        self.values.insert(name.to_string(), value);
        Ok(())
    }

    fn get_secret(&self, name: &str) -> Result<SecretValue, RuntimeError> {
        self.values
            .get(name)
            .cloned()
            .ok_or_else(|| RuntimeError::SecretNotFound {
                name: name.to_string(),
            })
    }

    fn delete_secret(&mut self, name: &str) -> Result<(), RuntimeError> {
        self.values.remove(name);
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct FileSecretStore {
    root: PathBuf,
}

impl FileSecretStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn secret_path(&self, name: &str) -> PathBuf {
        self.root.join(format!("{}.json", safe_secret_name(name)))
    }
}

impl SecretStore for FileSecretStore {
    fn put_secret(&mut self, name: &str, value: SecretValue) -> Result<(), RuntimeError> {
        fs::create_dir_all(&self.root).map_storage()?;
        let path = self.secret_path(name);
        let temp_path = path.with_extension("json.tmp");
        let document = json!({
            "name": name,
            "encoding": "utf8",
            "value": value.expose_text()?,
        });
        let bytes = serde_json::to_vec_pretty(&document).map_storage()?;
        fs::write(&temp_path, bytes).map_storage()?;
        restrict_file_permissions(&temp_path)?;
        fs::rename(temp_path, path).map_storage()
    }

    fn get_secret(&self, name: &str) -> Result<SecretValue, RuntimeError> {
        let path = self.secret_path(name);
        if !path.exists() {
            return Err(RuntimeError::SecretNotFound {
                name: name.to_string(),
            });
        }
        let bytes = fs::read(path).map_storage()?;
        let value: serde_json::Value = serde_json::from_slice(&bytes).map_storage()?;
        let raw = value
            .get("value")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| RuntimeError::Storage("secret file missing value".to_string()))?;
        Ok(SecretValue::new(raw.as_bytes().to_vec()))
    }

    fn delete_secret(&mut self, name: &str) -> Result<(), RuntimeError> {
        let path = self.secret_path(name);
        if path.exists() {
            fs::remove_file(path).map_storage()?;
        }
        Ok(())
    }
}

fn safe_secret_name(name: &str) -> String {
    name.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn restrict_file_permissions(path: &PathBuf) -> Result<(), RuntimeError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path).map_storage()?.permissions();
        permissions.set_mode(0o600);
        fs::set_permissions(path, permissions).map_storage()?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

fn sanitize_secret_ref(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            '\r' | '\n' | '\t' => ' ',
            ch if ch.is_control() => ' ',
            ch => ch,
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

trait MapStorage<T> {
    fn map_storage(self) -> Result<T, RuntimeError>;
}

impl<T, E: std::fmt::Display> MapStorage<T> for Result<T, E> {
    fn map_storage(self) -> Result<T, RuntimeError> {
        self.map_err(|err| RuntimeError::Storage(err.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_test_dir(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("num_secret_{name}_{stamp}"))
    }

    #[test]
    fn memory_secret_store_round_trips_and_deletes_values() {
        let mut store = MemorySecretStore::new();
        store
            .put_secret("payment.api_key", SecretValue::new("sk_test"))
            .unwrap();

        let value = store.get_secret("payment.api_key").unwrap();
        assert_eq!(value.expose_text().unwrap(), "sk_test");
        assert_eq!(format!("{value:?}"), "SecretValue(<redacted>)");

        store.delete_secret("payment.api_key").unwrap();
        assert!(matches!(
            store.get_secret("payment.api_key"),
            Err(RuntimeError::SecretNotFound { .. })
        ));
    }

    #[test]
    fn file_secret_store_round_trips_values() {
        let root = unique_test_dir("file_round_trip");
        let mut store = FileSecretStore::new(&root);
        store
            .put_secret("mailer.token", SecretValue::new("mail_secret"))
            .unwrap();

        let value = store.get_secret("mailer.token").unwrap();
        assert_eq!(value.expose_text().unwrap(), "mail_secret");

        store.delete_secret("mailer.token").unwrap();
        assert!(matches!(
            store.get_secret("mailer.token"),
            Err(RuntimeError::SecretNotFound { .. })
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn external_secret_store_reads_from_stub_backend_without_leaking_values() {
        let store = ExternalSecretStore::new(
            StubExternalSecretBackend::new("vault")
                .with_secret("billing/api-key", SecretValue::new("external_secret_value")),
        );

        let value = store.get_secret("secret://vault/billing/api-key").unwrap();

        assert_eq!(value.expose_text().unwrap(), "external_secret_value");
        assert_eq!(format!("{value:?}"), "SecretValue(<redacted>)");
    }

    #[test]
    fn external_secret_store_distinguishes_missing_denied_and_unavailable() {
        let store = ExternalSecretStore::new(
            StubExternalSecretBackend::new("vault").with_denied("billing/denied"),
        );

        let missing = store
            .get_secret("secret://vault/billing/missing")
            .unwrap_err();
        assert_eq!(missing.kind(), "secret_not_found");
        assert!(matches!(missing, RuntimeError::SecretNotFound { .. }));

        let denied = store
            .get_secret("secret://vault/billing/denied")
            .unwrap_err();
        assert_eq!(denied.kind(), "secret_denied");
        assert!(matches!(denied, RuntimeError::SecretDenied { .. }));

        let unavailable = ExternalSecretStore::new(
            StubExternalSecretBackend::new("vault").unavailable("backend timeout"),
        )
        .get_secret("secret://vault/billing/api-key")
        .unwrap_err();
        assert_eq!(unavailable.kind(), "secret_unavailable");
        assert!(matches!(
            unavailable,
            RuntimeError::SecretUnavailable { .. }
        ));
    }

    #[test]
    fn external_secret_store_rejects_wrong_backend_and_writes() {
        let mut store = ExternalSecretStore::new(StubExternalSecretBackend::new("vault"));

        let wrong_backend = store
            .get_secret("secret://kms/billing/api-key")
            .unwrap_err();
        assert_eq!(wrong_backend.kind(), "secret_unavailable");
        assert!(!wrong_backend.message().contains("api-key"));

        let write_error = store
            .put_secret(
                "secret://vault/billing/api-key",
                SecretValue::new("new_secret"),
            )
            .unwrap_err();
        assert_eq!(write_error.kind(), "secret_unavailable");
        assert!(!write_error.message().contains("new_secret"));
    }
}
