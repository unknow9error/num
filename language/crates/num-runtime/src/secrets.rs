use crate::{RuntimeError, SecretStore, SecretValue};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

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
}
