use crate::{RuntimeError, SecretStore, SecretValue};
use serde_json::json;
use serde_json::Value as JsonValue;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::time::Duration;

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
    InvalidResponse { reason: String },
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
        ExternalSecretError::InvalidResponse { reason } => RuntimeError::SecretInvalidResponse {
            backend: reference.backend().to_string(),
            reason,
        },
    }
}

#[derive(Clone)]
pub struct VaultSecretConfig {
    backend_id: String,
    address: String,
    mount: String,
    path_prefix: Option<String>,
    auth_method: String,
    token: SecretValue,
    timeout: Duration,
}

impl VaultSecretConfig {
    pub fn token(
        backend_id: impl Into<String>,
        address: impl Into<String>,
        mount: impl Into<String>,
        token: SecretValue,
    ) -> Self {
        Self {
            backend_id: backend_id.into(),
            address: address.into(),
            mount: mount.into(),
            path_prefix: None,
            auth_method: "token".to_string(),
            token,
            timeout: Duration::from_secs(5),
        }
    }

    pub fn with_path_prefix(mut self, path_prefix: impl Into<String>) -> Self {
        let path_prefix = path_prefix.into();
        self.path_prefix = (!path_prefix.trim_matches('/').is_empty()).then_some(path_prefix);
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn backend_id(&self) -> &str {
        &self.backend_id
    }

    pub fn address(&self) -> &str {
        &self.address
    }

    pub fn mount(&self) -> &str {
        &self.mount
    }

    pub fn path_prefix(&self) -> Option<&str> {
        self.path_prefix.as_deref()
    }

    pub fn auth_method(&self) -> &str {
        &self.auth_method
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }
}

impl std::fmt::Debug for VaultSecretConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VaultSecretConfig")
            .field("backend_id", &self.backend_id)
            .field("address", &self.address)
            .field("mount", &self.mount)
            .field("path_prefix", &self.path_prefix)
            .field("auth_method", &self.auth_method)
            .field("token", &"<redacted>")
            .field("timeout", &self.timeout)
            .finish()
    }
}

pub trait VaultSecretClient {
    fn read_secret(
        &self,
        config: &VaultSecretConfig,
        path: &str,
    ) -> Result<VaultHttpResponse, ExternalSecretError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultHttpResponse {
    pub status: u16,
    pub body: String,
}

#[derive(Debug, Clone, Default)]
pub struct HttpVaultSecretClient;

impl VaultSecretClient for HttpVaultSecretClient {
    fn read_secret(
        &self,
        config: &VaultSecretConfig,
        path: &str,
    ) -> Result<VaultHttpResponse, ExternalSecretError> {
        let endpoint = vault_http_endpoint(config, path)?;
        let token = config
            .token
            .expose_text()
            .map_err(|err| ExternalSecretError::Unavailable {
                reason: err.message(),
            })?;
        let mut stream =
            TcpStream::connect_timeout(&endpoint.address, config.timeout()).map_err(|err| {
                ExternalSecretError::Unavailable {
                    reason: format!("failed to connect to Vault: {err}"),
                }
            })?;
        stream
            .set_read_timeout(Some(config.timeout()))
            .map_err(|err| ExternalSecretError::Unavailable {
                reason: format!("failed to configure Vault read timeout: {err}"),
            })?;
        stream
            .set_write_timeout(Some(config.timeout()))
            .map_err(|err| ExternalSecretError::Unavailable {
                reason: format!("failed to configure Vault write timeout: {err}"),
            })?;
        let request = format!(
            "GET {} HTTP/1.1\r\nHost: {}\r\nX-Vault-Token: {}\r\nAccept: application/json\r\nConnection: close\r\n\r\n",
            endpoint.path, endpoint.host_header, token
        );
        stream
            .write_all(request.as_bytes())
            .map_err(|err| ExternalSecretError::Unavailable {
                reason: format!("failed to write Vault request: {err}"),
            })?;
        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .map_err(|err| ExternalSecretError::Unavailable {
                reason: format!("failed to read Vault response: {err}"),
            })?;
        parse_http_response(&response)
    }
}

pub struct VaultSecretBackend<C = HttpVaultSecretClient> {
    config: VaultSecretConfig,
    client: C,
}

impl VaultSecretBackend<HttpVaultSecretClient> {
    pub fn new(config: VaultSecretConfig) -> Self {
        Self {
            config,
            client: HttpVaultSecretClient,
        }
    }
}

impl<C> VaultSecretBackend<C> {
    pub fn with_client(config: VaultSecretConfig, client: C) -> Self {
        Self { config, client }
    }
}

impl<C: VaultSecretClient> ExternalSecretBackend for VaultSecretBackend<C> {
    fn backend_id(&self) -> &str {
        self.config.backend_id()
    }

    fn get_external_secret(&self, name: &str) -> Result<SecretValue, ExternalSecretError> {
        if self.config.auth_method() != "token" {
            return Err(ExternalSecretError::Unavailable {
                reason: format!(
                    "Vault auth method `{}` is not supported by this adapter",
                    sanitize_secret_ref(self.config.auth_method())
                ),
            });
        }
        let response = self.client.read_secret(&self.config, name)?;
        match response.status {
            200 => extract_vault_kv_v2_secret(&response.body),
            403 => Err(ExternalSecretError::Denied),
            404 => Err(ExternalSecretError::Missing),
            500..=599 => Err(ExternalSecretError::Unavailable {
                reason: format!("Vault returned HTTP {}", response.status),
            }),
            status => Err(ExternalSecretError::InvalidResponse {
                reason: format!("Vault returned unsupported HTTP status {status}"),
            }),
        }
    }
}

struct VaultHttpEndpoint {
    address: std::net::SocketAddr,
    host_header: String,
    path: String,
}

fn vault_http_endpoint(
    config: &VaultSecretConfig,
    secret_name: &str,
) -> Result<VaultHttpEndpoint, ExternalSecretError> {
    let Some(rest) = config.address().strip_prefix("http://") else {
        return Err(ExternalSecretError::Unavailable {
            reason: "Vault HTTP transport currently supports http:// fixture/dev endpoints; configure production TLS outside this adapter".to_string(),
        });
    };
    let host_port = rest.split('/').next().unwrap_or(rest).trim();
    if host_port.is_empty() {
        return Err(ExternalSecretError::Unavailable {
            reason: "Vault address is missing host".to_string(),
        });
    }
    let socket = host_port
        .to_socket_addrs()
        .map_err(|err| ExternalSecretError::Unavailable {
            reason: format!("failed to resolve Vault address: {err}"),
        })?
        .next()
        .ok_or_else(|| ExternalSecretError::Unavailable {
            reason: "Vault address resolved no socket addresses".to_string(),
        })?;
    let mount = config.mount().trim_matches('/');
    if mount.is_empty() {
        return Err(ExternalSecretError::Unavailable {
            reason: "Vault mount is empty".to_string(),
        });
    }
    let secret_path = match config.path_prefix() {
        Some(prefix) => format!(
            "{}/{}",
            prefix.trim_matches('/'),
            secret_name.trim_matches('/')
        ),
        None => secret_name.trim_matches('/').to_string(),
    };
    Ok(VaultHttpEndpoint {
        address: socket,
        host_header: host_port.to_string(),
        path: format!(
            "/v1/{}/data/{}",
            percent_encode_path(mount),
            percent_encode_path(&secret_path)
        ),
    })
}

fn parse_http_response(response: &str) -> Result<VaultHttpResponse, ExternalSecretError> {
    let Some((head, body)) = response
        .split_once("\r\n\r\n")
        .or_else(|| response.split_once("\n\n"))
    else {
        return Err(ExternalSecretError::InvalidResponse {
            reason: "Vault response did not include HTTP headers".to_string(),
        });
    };
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or_else(|| ExternalSecretError::InvalidResponse {
            reason: "Vault response had no parseable HTTP status".to_string(),
        })?;
    Ok(VaultHttpResponse {
        status,
        body: body.to_string(),
    })
}

fn extract_vault_kv_v2_secret(body: &str) -> Result<SecretValue, ExternalSecretError> {
    let value: JsonValue =
        serde_json::from_str(body).map_err(|err| ExternalSecretError::InvalidResponse {
            reason: format!("Vault response was not valid JSON: {err}"),
        })?;
    let data = value
        .pointer("/data/data")
        .and_then(JsonValue::as_object)
        .ok_or_else(|| ExternalSecretError::InvalidResponse {
            reason: "Vault response missing data.data object".to_string(),
        })?;
    let secret = data
        .get("value")
        .or_else(|| data.get("secret"))
        .or_else(|| data.get("password"))
        .or_else(|| data.get("token"))
        .and_then(JsonValue::as_str)
        .ok_or_else(|| ExternalSecretError::InvalidResponse {
            reason: "Vault response missing string secret value field".to_string(),
        })?;
    Ok(SecretValue::new(secret.as_bytes().to_vec()))
}

fn percent_encode_path(value: &str) -> String {
    value
        .split('/')
        .filter(|part| !part.is_empty())
        .map(percent_encode_segment)
        .collect::<Vec<_>>()
        .join("/")
}

fn percent_encode_segment(value: &str) -> String {
    let mut output = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            output.push(byte as char);
        } else {
            output.push_str(&format!("%{byte:02X}"));
        }
    }
    output
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
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[derive(Debug, Clone)]
    struct MockVaultClient {
        response: Result<VaultHttpResponse, ExternalSecretError>,
    }

    impl VaultSecretClient for MockVaultClient {
        fn read_secret(
            &self,
            config: &VaultSecretConfig,
            path: &str,
        ) -> Result<VaultHttpResponse, ExternalSecretError> {
            assert_eq!(config.backend_id(), "vault");
            assert_eq!(config.address(), "http://vault.test:8200");
            assert_eq!(config.mount(), "secret");
            assert_eq!(config.auth_method(), "token");
            assert_eq!(path, "billing/api-key");
            self.response.clone()
        }
    }

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

    #[test]
    fn vault_secret_backend_reads_kv_v2_secret_without_leaking_token() {
        let config = VaultSecretConfig::token(
            "vault",
            "http://vault.test:8200",
            "secret",
            SecretValue::new("vault-token"),
        );
        let backend = VaultSecretBackend::with_client(
            config.clone(),
            MockVaultClient {
                response: Ok(VaultHttpResponse {
                    status: 200,
                    body: r#"{"data":{"data":{"value":"vault-secret-value"}}}"#.to_string(),
                }),
            },
        );

        let value = backend.get_external_secret("billing/api-key").unwrap();

        assert_eq!(value.expose_text().unwrap(), "vault-secret-value");
        assert!(!format!("{config:?}").contains("vault-token"));
        assert_eq!(format!("{value:?}"), "SecretValue(<redacted>)");
    }

    #[test]
    fn vault_secret_backend_maps_status_and_invalid_response_errors() {
        let config = VaultSecretConfig::token(
            "vault",
            "http://vault.test:8200",
            "secret",
            SecretValue::new("vault-token"),
        );
        for (status, expected) in [
            (403, ExternalSecretError::Denied),
            (404, ExternalSecretError::Missing),
            (
                503,
                ExternalSecretError::Unavailable {
                    reason: "Vault returned HTTP 503".to_string(),
                },
            ),
        ] {
            let backend = VaultSecretBackend::with_client(
                config.clone(),
                MockVaultClient {
                    response: Ok(VaultHttpResponse {
                        status,
                        body: "{}".to_string(),
                    }),
                },
            );
            assert_eq!(
                backend.get_external_secret("billing/api-key").unwrap_err(),
                expected
            );
        }

        let backend = VaultSecretBackend::with_client(
            config,
            MockVaultClient {
                response: Ok(VaultHttpResponse {
                    status: 200,
                    body: r#"{"data":{"data":{"metadata":"not-secret"}}}"#.to_string(),
                }),
            },
        );
        assert!(matches!(
            backend.get_external_secret("billing/api-key"),
            Err(ExternalSecretError::InvalidResponse { .. })
        ));
    }

    #[test]
    fn vault_http_client_reads_from_fixture_server() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0_u8; 512];
            loop {
                let read = stream.read(&mut buffer).unwrap();
                if read == 0 {
                    break;
                }
                bytes.extend_from_slice(&buffer[..read]);
                if bytes.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            let request = String::from_utf8(bytes).unwrap();
            assert!(request.starts_with("GET /v1/secret/data/apps/billing/api-key HTTP/1.1"));
            assert!(request.contains("X-Vault-Token: vault-token"));
            let body = r#"{"data":{"data":{"token":"from-fixture-server"}}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        let config = VaultSecretConfig::token(
            "vault",
            format!("http://{address}"),
            "secret",
            SecretValue::new("vault-token"),
        )
        .with_path_prefix("apps/billing");
        let backend = VaultSecretBackend::new(config);

        let value = backend.get_external_secret("api-key").unwrap();

        assert_eq!(value.expose_text().unwrap(), "from-fixture-server");
        handle.join().unwrap();
    }
}
