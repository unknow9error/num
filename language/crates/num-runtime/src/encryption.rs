use crate::{hashing, redaction, RuntimeError, SecretValue};
use serde_json::{json, Value as JsonValue};
use std::collections::HashSet;
use std::marker::PhantomData;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncryptionMetadata {
    pub provider: String,
    pub algorithm: String,
    pub key_id: String,
    pub created_at_unix_seconds: u64,
    pub plaintext_type: String,
    pub privacy: String,
    pub trust: String,
}

impl EncryptionMetadata {
    pub fn new(
        provider: impl Into<String>,
        algorithm: impl Into<String>,
        key_id: impl Into<String>,
        plaintext_type: impl Into<String>,
    ) -> Self {
        Self {
            provider: provider.into(),
            algorithm: algorithm.into(),
            key_id: key_id.into(),
            created_at_unix_seconds: unix_seconds(SystemTime::now()),
            plaintext_type: plaintext_type.into(),
            privacy: "secret".to_string(),
            trust: "provider_encrypted".to_string(),
        }
    }

    pub fn with_created_at(mut self, created_at: SystemTime) -> Self {
        self.created_at_unix_seconds = unix_seconds(created_at);
        self
    }

    pub fn with_labels(mut self, privacy: impl Into<String>, trust: impl Into<String>) -> Self {
        self.privacy = privacy.into();
        self.trust = trust.into();
        self
    }

    pub fn to_json(&self) -> JsonValue {
        json!({
            "provider": self.provider,
            "algorithm": self.algorithm,
            "key_id": self.key_id,
            "created_at_unix_seconds": self.created_at_unix_seconds,
            "plaintext_type": self.plaintext_type,
            "privacy": self.privacy,
            "trust": self.trust,
        })
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct Encrypted<T = SecretValue> {
    metadata: EncryptionMetadata,
    ciphertext: Vec<u8>,
    _marker: PhantomData<T>,
}

impl<T> Encrypted<T> {
    pub fn new(metadata: EncryptionMetadata, ciphertext: impl Into<Vec<u8>>) -> Self {
        Self {
            metadata,
            ciphertext: ciphertext.into(),
            _marker: PhantomData,
        }
    }

    pub fn metadata(&self) -> &EncryptionMetadata {
        &self.metadata
    }

    pub fn ciphertext(&self) -> &[u8] {
        &self.ciphertext
    }

    pub fn into_parts(self) -> (EncryptionMetadata, Vec<u8>) {
        (self.metadata, self.ciphertext)
    }

    pub fn to_redacted_json(&self) -> JsonValue {
        json!({
            "kind": "Encrypted",
            "metadata": self.metadata.to_json(),
            "ciphertext": redaction::REDACTION_MARKER,
            "ciphertext_bytes": self.ciphertext.len(),
            "ciphertext_sha256": hashing::sha256_hex(&self.ciphertext),
        })
    }
}

impl<T> std::fmt::Debug for Encrypted<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Encrypted")
            .field("metadata", &self.metadata)
            .field("ciphertext", &redaction::REDACTION_MARKER)
            .field("ciphertext_bytes", &self.ciphertext.len())
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decrypted<T = SecretValue> {
    value: T,
    pub privacy: String,
    pub trust: String,
    pub key_id: String,
    pub provider: String,
}

impl<T> Decrypted<T> {
    pub fn new(
        value: T,
        provider: impl Into<String>,
        key_id: impl Into<String>,
        privacy: impl Into<String>,
        trust: impl Into<String>,
    ) -> Self {
        Self {
            value,
            provider: provider.into(),
            key_id: key_id.into(),
            privacy: privacy.into(),
            trust: trust.into(),
        }
    }

    pub fn value(&self) -> &T {
        &self.value
    }

    pub fn into_value(self) -> T {
        self.value
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncryptionProviderError {
    KeyDenied,
    KeyNotFound,
    UnsupportedAlgorithm { algorithm: String },
    InvalidEnvelope { reason: String },
    Unavailable { reason: String },
}

pub trait EncryptionProvider {
    fn provider_id(&self) -> &str;

    fn encrypt(
        &self,
        key_id: &str,
        algorithm: &str,
        plaintext: &SecretValue,
        associated_data: &[u8],
    ) -> Result<Vec<u8>, EncryptionProviderError>;

    fn decrypt(
        &self,
        envelope: &Encrypted<SecretValue>,
        associated_data: &[u8],
    ) -> Result<SecretValue, EncryptionProviderError>;
}

pub struct EncryptionBoundary<P> {
    provider: P,
    default_algorithm: String,
}

impl<P> EncryptionBoundary<P> {
    pub fn new(provider: P, default_algorithm: impl Into<String>) -> Self {
        Self {
            provider,
            default_algorithm: default_algorithm.into(),
        }
    }

    pub fn provider(&self) -> &P {
        &self.provider
    }
}

impl<P: EncryptionProvider> EncryptionBoundary<P> {
    pub fn encrypt_secret(
        &self,
        plaintext: SecretValue,
        key_id: impl AsRef<str>,
        plaintext_type: impl AsRef<str>,
        associated_data: &[u8],
    ) -> Result<Encrypted<SecretValue>, RuntimeError> {
        let key_id = key_id.as_ref();
        let metadata = EncryptionMetadata::new(
            self.provider.provider_id(),
            &self.default_algorithm,
            key_id,
            plaintext_type.as_ref(),
        );
        let ciphertext = self
            .provider
            .encrypt(key_id, &self.default_algorithm, &plaintext, associated_data)
            .map_err(|err| map_provider_error(self.provider.provider_id(), key_id, err))?;
        Ok(Encrypted::new(metadata, ciphertext))
    }

    pub fn decrypt_secret(
        &self,
        envelope: &Encrypted<SecretValue>,
        associated_data: &[u8],
    ) -> Result<Decrypted<SecretValue>, RuntimeError> {
        if envelope.metadata().provider != self.provider.provider_id() {
            return Err(RuntimeError::EncryptionInvalidEnvelope {
                provider: envelope.metadata().provider.clone(),
                reason: format!(
                    "configured provider `{}` cannot decrypt envelope from `{}`",
                    self.provider.provider_id(),
                    envelope.metadata().provider
                ),
            });
        }
        let plaintext = self
            .provider
            .decrypt(envelope, associated_data)
            .map_err(|err| {
                map_provider_error(
                    self.provider.provider_id(),
                    envelope.metadata().key_id.as_str(),
                    err,
                )
            })?;
        Ok(Decrypted::new(
            plaintext,
            self.provider.provider_id(),
            envelope.metadata().key_id.clone(),
            "secret",
            format!("decrypted:{}", self.provider.provider_id()),
        ))
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct KmsEncryptionConfig {
    provider_id: String,
    default_algorithm: String,
    credential_env: Vec<String>,
}

impl KmsEncryptionConfig {
    pub fn new(provider_id: impl Into<String>, default_algorithm: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
            default_algorithm: default_algorithm.into(),
            credential_env: Vec::new(),
        }
    }

    pub fn with_credential_env(
        mut self,
        names: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.credential_env = normalize_env_names(names);
        self
    }

    pub fn provider_id(&self) -> &str {
        &self.provider_id
    }

    pub fn default_algorithm(&self) -> &str {
        &self.default_algorithm
    }

    pub fn credential_env(&self) -> &[String] {
        &self.credential_env
    }

    pub fn to_json(&self) -> JsonValue {
        json!({
            "provider": self.provider_id,
            "default_algorithm": self.default_algorithm,
            "credential_env": self.credential_env,
        })
    }
}

impl std::fmt::Debug for KmsEncryptionConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KmsEncryptionConfig")
            .field("provider_id", &self.provider_id)
            .field("default_algorithm", &self.default_algorithm)
            .field("credential_env", &self.credential_env)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KmsKeyId {
    raw: String,
}

impl KmsKeyId {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, EncryptionProviderError> {
        let value = value.as_ref().trim();
        if value.is_empty() {
            return Err(EncryptionProviderError::InvalidEnvelope {
                reason: "KMS key id is empty".to_string(),
            });
        }
        if looks_like_raw_key_material(value) {
            return Err(EncryptionProviderError::InvalidEnvelope {
                reason: "KMS key id looks like raw key material".to_string(),
            });
        }
        Ok(Self {
            raw: value.to_string(),
        })
    }

    pub fn as_str(&self) -> &str {
        &self.raw
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KmsEncryptRequest<'a> {
    pub provider_id: &'a str,
    pub key_id: &'a str,
    pub algorithm: &'a str,
    pub plaintext: &'a SecretValue,
    pub associated_data: &'a [u8],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KmsDecryptRequest<'a> {
    pub provider_id: &'a str,
    pub key_id: &'a str,
    pub algorithm: &'a str,
    pub ciphertext: &'a [u8],
    pub associated_data: &'a [u8],
}

pub trait KmsClient {
    fn encrypt(&self, request: KmsEncryptRequest<'_>) -> Result<Vec<u8>, EncryptionProviderError>;
    fn decrypt(
        &self,
        request: KmsDecryptRequest<'_>,
    ) -> Result<SecretValue, EncryptionProviderError>;
}

#[derive(Debug, Clone)]
pub struct KmsEncryptionProvider<C> {
    config: KmsEncryptionConfig,
    client: C,
}

impl<C> KmsEncryptionProvider<C> {
    pub fn new(config: KmsEncryptionConfig, client: C) -> Self {
        Self { config, client }
    }

    pub fn config(&self) -> &KmsEncryptionConfig {
        &self.config
    }

    pub fn client(&self) -> &C {
        &self.client
    }
}

impl<C: KmsClient> EncryptionProvider for KmsEncryptionProvider<C> {
    fn provider_id(&self) -> &str {
        self.config.provider_id()
    }

    fn encrypt(
        &self,
        key_id: &str,
        algorithm: &str,
        plaintext: &SecretValue,
        associated_data: &[u8],
    ) -> Result<Vec<u8>, EncryptionProviderError> {
        let key_id = KmsKeyId::parse(key_id)?;
        let algorithm = if algorithm.trim().is_empty() {
            self.config.default_algorithm()
        } else {
            algorithm
        };
        self.client.encrypt(KmsEncryptRequest {
            provider_id: self.config.provider_id(),
            key_id: key_id.as_str(),
            algorithm,
            plaintext,
            associated_data,
        })
    }

    fn decrypt(
        &self,
        envelope: &Encrypted<SecretValue>,
        associated_data: &[u8],
    ) -> Result<SecretValue, EncryptionProviderError> {
        let metadata = envelope.metadata();
        let key_id = KmsKeyId::parse(&metadata.key_id)?;
        self.client.decrypt(KmsDecryptRequest {
            provider_id: self.config.provider_id(),
            key_id: key_id.as_str(),
            algorithm: &metadata.algorithm,
            ciphertext: envelope.ciphertext(),
            associated_data,
        })
    }
}

#[derive(Debug, Clone)]
pub struct FakeKmsClient {
    keys: HashSet<String>,
    denied_keys: HashSet<String>,
    unavailable: Option<String>,
}

impl FakeKmsClient {
    pub fn new() -> Self {
        Self {
            keys: HashSet::new(),
            denied_keys: HashSet::new(),
            unavailable: None,
        }
    }

    pub fn with_key(mut self, key_id: impl Into<String>) -> Self {
        self.keys.insert(key_id.into());
        self
    }

    pub fn with_denied_key(mut self, key_id: impl Into<String>) -> Self {
        self.denied_keys.insert(key_id.into());
        self
    }

    pub fn unavailable(mut self, reason: impl Into<String>) -> Self {
        self.unavailable = Some(reason.into());
        self
    }

    fn check_key(&self, key_id: &str) -> Result<(), EncryptionProviderError> {
        if let Some(reason) = &self.unavailable {
            return Err(EncryptionProviderError::Unavailable {
                reason: reason.clone(),
            });
        }
        if self.denied_keys.contains(key_id) {
            return Err(EncryptionProviderError::KeyDenied);
        }
        if !self.keys.contains(key_id) {
            return Err(EncryptionProviderError::KeyNotFound);
        }
        Ok(())
    }
}

impl Default for FakeKmsClient {
    fn default() -> Self {
        Self::new()
    }
}

impl KmsClient for FakeKmsClient {
    fn encrypt(&self, request: KmsEncryptRequest<'_>) -> Result<Vec<u8>, EncryptionProviderError> {
        self.check_key(request.key_id)?;
        let mut ciphertext = format!("num-fake-kms-v1:{}:", request.provider_id).into_bytes();
        ciphertext.extend(kms_auth_tag(
            request.provider_id,
            request.key_id,
            request.algorithm,
            request.associated_data,
            request.plaintext.expose(),
        ));
        ciphertext.push(b':');
        ciphertext.extend(xor_with_kms_keystream(
            request.provider_id,
            request.key_id,
            request.algorithm,
            request.associated_data,
            request.plaintext.expose(),
        ));
        Ok(ciphertext)
    }

    fn decrypt(
        &self,
        request: KmsDecryptRequest<'_>,
    ) -> Result<SecretValue, EncryptionProviderError> {
        self.check_key(request.key_id)?;
        let prefix = format!("num-fake-kms-v1:{}:", request.provider_id);
        let Some(ciphertext) = request.ciphertext.strip_prefix(prefix.as_bytes()) else {
            return Err(EncryptionProviderError::InvalidEnvelope {
                reason: "missing fake KMS envelope prefix".to_string(),
            });
        };
        let Some((tag, ciphertext)) = ciphertext.split_at_checked(64) else {
            return Err(EncryptionProviderError::InvalidEnvelope {
                reason: "missing fake KMS authentication tag".to_string(),
            });
        };
        let Some(ciphertext) = ciphertext.strip_prefix(b":") else {
            return Err(EncryptionProviderError::InvalidEnvelope {
                reason: "malformed fake KMS authentication tag".to_string(),
            });
        };
        let plaintext = xor_with_kms_keystream(
            request.provider_id,
            request.key_id,
            request.algorithm,
            request.associated_data,
            ciphertext,
        );
        if tag
            != kms_auth_tag(
                request.provider_id,
                request.key_id,
                request.algorithm,
                request.associated_data,
                &plaintext,
            )
        {
            return Err(EncryptionProviderError::InvalidEnvelope {
                reason: "fake KMS envelope authentication failed".to_string(),
            });
        }
        Ok(SecretValue::new(plaintext))
    }
}

#[derive(Debug, Clone)]
pub struct TestEncryptionProvider {
    provider_id: String,
    algorithms: HashSet<String>,
    denied_keys: HashSet<String>,
    unavailable: Option<String>,
}

impl TestEncryptionProvider {
    pub fn new(provider_id: impl Into<String>) -> Self {
        let mut algorithms = HashSet::new();
        algorithms.insert("NUM-TEST-XOR-SHA256".to_string());
        Self {
            provider_id: provider_id.into(),
            algorithms,
            denied_keys: HashSet::new(),
            unavailable: None,
        }
    }

    pub fn with_algorithm(mut self, algorithm: impl Into<String>) -> Self {
        self.algorithms.insert(algorithm.into());
        self
    }

    pub fn deny_key(mut self, key_id: impl Into<String>) -> Self {
        self.denied_keys.insert(key_id.into());
        self
    }

    pub fn unavailable(mut self, reason: impl Into<String>) -> Self {
        self.unavailable = Some(reason.into());
        self
    }
}

impl EncryptionProvider for TestEncryptionProvider {
    fn provider_id(&self) -> &str {
        &self.provider_id
    }

    fn encrypt(
        &self,
        key_id: &str,
        algorithm: &str,
        plaintext: &SecretValue,
        associated_data: &[u8],
    ) -> Result<Vec<u8>, EncryptionProviderError> {
        self.ensure_available_key_and_algorithm(key_id, algorithm)?;
        let mut ciphertext = b"num-test-envelope-v1:".to_vec();
        ciphertext.extend(test_auth_tag(
            key_id,
            algorithm,
            associated_data,
            plaintext.expose(),
        ));
        ciphertext.push(b':');
        ciphertext.extend(xor_with_test_keystream(
            key_id,
            algorithm,
            associated_data,
            plaintext.expose(),
        ));
        Ok(ciphertext)
    }

    fn decrypt(
        &self,
        envelope: &Encrypted<SecretValue>,
        associated_data: &[u8],
    ) -> Result<SecretValue, EncryptionProviderError> {
        let metadata = envelope.metadata();
        self.ensure_available_key_and_algorithm(&metadata.key_id, &metadata.algorithm)?;
        let Some(ciphertext) = envelope.ciphertext().strip_prefix(b"num-test-envelope-v1:") else {
            return Err(EncryptionProviderError::InvalidEnvelope {
                reason: "missing test envelope prefix".to_string(),
            });
        };
        let Some((tag, ciphertext)) = ciphertext.split_at_checked(64) else {
            return Err(EncryptionProviderError::InvalidEnvelope {
                reason: "missing test envelope authentication tag".to_string(),
            });
        };
        let Some(ciphertext) = ciphertext.strip_prefix(b":") else {
            return Err(EncryptionProviderError::InvalidEnvelope {
                reason: "malformed test envelope authentication tag".to_string(),
            });
        };
        let plaintext = xor_with_test_keystream(
            &metadata.key_id,
            &metadata.algorithm,
            associated_data,
            ciphertext,
        );
        if tag
            != test_auth_tag(
                &metadata.key_id,
                &metadata.algorithm,
                associated_data,
                &plaintext,
            )
        {
            return Err(EncryptionProviderError::InvalidEnvelope {
                reason: "test envelope authentication failed".to_string(),
            });
        }
        Ok(SecretValue::new(plaintext))
    }
}

impl TestEncryptionProvider {
    fn ensure_available_key_and_algorithm(
        &self,
        key_id: &str,
        algorithm: &str,
    ) -> Result<(), EncryptionProviderError> {
        if let Some(reason) = &self.unavailable {
            return Err(EncryptionProviderError::Unavailable {
                reason: reason.clone(),
            });
        }
        if self.denied_keys.contains(key_id) {
            return Err(EncryptionProviderError::KeyDenied);
        }
        if !self.algorithms.contains(algorithm) {
            return Err(EncryptionProviderError::UnsupportedAlgorithm {
                algorithm: algorithm.to_string(),
            });
        }
        Ok(())
    }
}

fn map_provider_error(provider: &str, key_id: &str, err: EncryptionProviderError) -> RuntimeError {
    match err {
        EncryptionProviderError::KeyDenied => RuntimeError::EncryptionDenied {
            provider: provider.to_string(),
            key_id: key_id.to_string(),
        },
        EncryptionProviderError::KeyNotFound => RuntimeError::EncryptionUnavailable {
            provider: provider.to_string(),
            reason: format!("key `{key_id}` not found"),
        },
        EncryptionProviderError::UnsupportedAlgorithm { algorithm } => {
            RuntimeError::EncryptionInvalidEnvelope {
                provider: provider.to_string(),
                reason: format!("algorithm `{algorithm}` is not supported"),
            }
        }
        EncryptionProviderError::InvalidEnvelope { reason } => {
            RuntimeError::EncryptionInvalidEnvelope {
                provider: provider.to_string(),
                reason,
            }
        }
        EncryptionProviderError::Unavailable { reason } => RuntimeError::EncryptionUnavailable {
            provider: provider.to_string(),
            reason,
        },
    }
}

fn unix_seconds(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs()
}

fn xor_with_test_keystream(
    key_id: &str,
    algorithm: &str,
    associated_data: &[u8],
    input: &[u8],
) -> Vec<u8> {
    let seed = format!(
        "{}:{}:{}",
        key_id,
        algorithm,
        hashing::sha256_hex(associated_data)
    );
    let digest = hashing::sha256_hex(seed.as_bytes()).into_bytes();
    input
        .iter()
        .enumerate()
        .map(|(index, byte)| byte ^ digest[index % digest.len()])
        .collect()
}

fn test_auth_tag(
    key_id: &str,
    algorithm: &str,
    associated_data: &[u8],
    plaintext: &[u8],
) -> Vec<u8> {
    let mut material = Vec::new();
    material.extend(key_id.as_bytes());
    material.push(0);
    material.extend(algorithm.as_bytes());
    material.push(0);
    material.extend(associated_data);
    material.push(0);
    material.extend(plaintext);
    hashing::sha256_hex(&material).into_bytes()
}

fn kms_auth_tag(
    provider_id: &str,
    key_id: &str,
    algorithm: &str,
    associated_data: &[u8],
    plaintext: &[u8],
) -> Vec<u8> {
    let mut material = Vec::new();
    material.extend(provider_id.as_bytes());
    material.push(0);
    material.extend(key_id.as_bytes());
    material.push(0);
    material.extend(algorithm.as_bytes());
    material.push(0);
    material.extend(associated_data);
    material.push(0);
    material.extend(plaintext);
    hashing::sha256_hex(&material).into_bytes()
}

fn xor_with_kms_keystream(
    provider_id: &str,
    key_id: &str,
    algorithm: &str,
    associated_data: &[u8],
    input: &[u8],
) -> Vec<u8> {
    let seed = format!(
        "{}:{}:{}:{}",
        provider_id,
        key_id,
        algorithm,
        hashing::sha256_hex(associated_data)
    );
    let digest = hashing::sha256_hex(seed.as_bytes()).into_bytes();
    input
        .iter()
        .enumerate()
        .map(|(index, byte)| byte ^ digest[index % digest.len()])
        .collect()
}

fn looks_like_raw_key_material(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.starts_with("raw:")
        || lower.starts_with("base64:")
        || lower.contains("private key")
        || lower.contains("begin ")
        || lower.contains("secret=")
        || lower.contains("password=")
}

fn normalize_env_names(names: impl IntoIterator<Item = impl Into<String>>) -> Vec<String> {
    let mut names = names
        .into_iter()
        .map(Into::into)
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypts_and_decrypts_secret_through_provider_boundary() {
        let boundary = EncryptionBoundary::new(
            TestEncryptionProvider::new("test-kms"),
            "NUM-TEST-XOR-SHA256",
        );

        let envelope = boundary
            .encrypt_secret(
                SecretValue::new("card-token-123"),
                "tenant-a/payments",
                "Text",
                b"tenant-a",
            )
            .expect("encrypts");

        assert_eq!(envelope.metadata().provider, "test-kms");
        assert_eq!(envelope.metadata().algorithm, "NUM-TEST-XOR-SHA256");
        assert_eq!(envelope.metadata().key_id, "tenant-a/payments");
        assert_eq!(envelope.metadata().plaintext_type, "Text");
        assert_eq!(envelope.metadata().privacy, "secret");
        assert!(!format!("{envelope:?}").contains("card-token-123"));

        let decrypted = boundary
            .decrypt_secret(&envelope, b"tenant-a")
            .expect("decrypts");
        assert_eq!(decrypted.value().expose_text().unwrap(), "card-token-123");
        assert_eq!(decrypted.privacy, "secret");
        assert_eq!(decrypted.trust, "decrypted:test-kms");
    }

    #[test]
    fn redacted_json_never_contains_ciphertext_or_plaintext() {
        let metadata = EncryptionMetadata::new("test-kms", "NUM-TEST-XOR-SHA256", "key-a", "Text")
            .with_created_at(UNIX_EPOCH);
        let envelope = Encrypted::<SecretValue>::new(metadata, b"ciphertext-with-secret-token");

        let redacted = envelope.to_redacted_json();

        assert_eq!(redacted["ciphertext"], redaction::REDACTION_MARKER);
        assert_eq!(redacted["ciphertext_bytes"], 28);
        assert_eq!(redacted["metadata"]["key_id"], "key-a");
        assert!(!redacted
            .to_string()
            .contains("ciphertext-with-secret-token"));
    }

    #[test]
    fn fails_closed_for_denied_keys_and_provider_mismatch() {
        let boundary = EncryptionBoundary::new(
            TestEncryptionProvider::new("test-kms").deny_key("blocked"),
            "NUM-TEST-XOR-SHA256",
        );

        let error = boundary
            .encrypt_secret(SecretValue::new("secret"), "blocked", "Text", b"")
            .expect_err("denied key should fail");
        assert_eq!(error.kind(), "encryption_denied");

        let metadata = EncryptionMetadata::new("other-kms", "NUM-TEST-XOR-SHA256", "key-a", "Text");
        let envelope = Encrypted::<SecretValue>::new(metadata, b"ciphertext");
        let error = boundary
            .decrypt_secret(&envelope, b"")
            .expect_err("wrong provider should fail");
        assert_eq!(error.kind(), "encryption_invalid_envelope");
    }

    #[test]
    fn associated_data_must_match_to_decrypt() {
        let boundary = EncryptionBoundary::new(
            TestEncryptionProvider::new("test-kms"),
            "NUM-TEST-XOR-SHA256",
        );
        let envelope = boundary
            .encrypt_secret(
                SecretValue::new("tenant-bound"),
                "key-a",
                "Text",
                b"tenant-a",
            )
            .expect("encrypts");

        let decrypted = boundary
            .decrypt_secret(&envelope, b"tenant-b")
            .expect_err("associated data mismatch should fail closed");

        assert_eq!(decrypted.kind(), "encryption_invalid_envelope");
    }

    #[test]
    fn kms_provider_uses_envelope_metadata_without_raw_keys() {
        let config = KmsEncryptionConfig::new("aws-kms", "KMS-AEAD-SHA256")
            .with_credential_env(["AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY"]);
        let provider = KmsEncryptionProvider::new(
            config.clone(),
            FakeKmsClient::new().with_key("alias/tenant-a/payments"),
        );
        let boundary = EncryptionBoundary::new(provider, config.default_algorithm());

        let envelope = boundary
            .encrypt_secret(
                SecretValue::new("payment-token-456"),
                "alias/tenant-a/payments",
                "Text",
                b"tenant-a",
            )
            .expect("KMS encrypts");

        assert_eq!(envelope.metadata().provider, "aws-kms");
        assert_eq!(envelope.metadata().algorithm, "KMS-AEAD-SHA256");
        assert_eq!(envelope.metadata().key_id, "alias/tenant-a/payments");
        assert_eq!(envelope.metadata().trust, "provider_encrypted");
        assert!(!format!("{envelope:?}").contains("payment-token-456"));

        let decrypted = boundary
            .decrypt_secret(&envelope, b"tenant-a")
            .expect("KMS decrypts");
        assert_eq!(
            decrypted.value().expose_text().unwrap(),
            "payment-token-456"
        );
        assert_eq!(decrypted.privacy, "secret");
        assert_eq!(decrypted.trust, "decrypted:aws-kms");
    }

    #[test]
    fn kms_provider_maps_key_and_availability_failures_to_structured_errors() {
        let denied = EncryptionBoundary::new(
            KmsEncryptionProvider::new(
                KmsEncryptionConfig::new("aws-kms", "KMS-AEAD-SHA256"),
                FakeKmsClient::new()
                    .with_key("alias/allowed")
                    .with_denied_key("alias/blocked"),
            ),
            "KMS-AEAD-SHA256",
        );
        let denied_error = denied
            .encrypt_secret(SecretValue::new("secret"), "alias/blocked", "Text", b"")
            .expect_err("denied key should fail");
        assert_eq!(denied_error.kind(), "encryption_denied");

        let missing_error = denied
            .encrypt_secret(SecretValue::new("secret"), "alias/missing", "Text", b"")
            .expect_err("missing key should fail");
        assert_eq!(missing_error.kind(), "encryption_unavailable");

        let unavailable = EncryptionBoundary::new(
            KmsEncryptionProvider::new(
                KmsEncryptionConfig::new("aws-kms", "KMS-AEAD-SHA256"),
                FakeKmsClient::new()
                    .with_key("alias/allowed")
                    .unavailable("kms endpoint unavailable"),
            ),
            "KMS-AEAD-SHA256",
        );
        let unavailable_error = unavailable
            .encrypt_secret(SecretValue::new("secret"), "alias/allowed", "Text", b"")
            .expect_err("unavailable KMS should fail");
        assert_eq!(unavailable_error.kind(), "encryption_unavailable");
        assert!(!unavailable_error.to_json().to_string().contains("secret"));
    }

    #[test]
    fn kms_key_ids_reject_raw_key_material_and_config_debug_uses_env_names_only() {
        let raw_key = "raw:super-secret-key-material";
        let config = KmsEncryptionConfig::new("aws-kms", "KMS-AEAD-SHA256")
            .with_credential_env([" AWS_SECRET_ACCESS_KEY ", "AWS_SECRET_ACCESS_KEY"]);
        let provider = KmsEncryptionProvider::new(
            config.clone(),
            FakeKmsClient::new().with_key("alias/allowed"),
        );
        let boundary = EncryptionBoundary::new(provider, "KMS-AEAD-SHA256");

        let error = boundary
            .encrypt_secret(SecretValue::new("secret"), raw_key, "Text", b"")
            .expect_err("raw key material should be rejected as a key id");

        assert_eq!(error.kind(), "encryption_invalid_envelope");
        assert!(!error.message().contains("super-secret-key-material"));
        assert_eq!(
            config.credential_env(),
            &["AWS_SECRET_ACCESS_KEY".to_string()]
        );
        assert!(!format!("{config:?}").contains("super-secret-key-material"));
        assert_eq!(
            config.to_json()["credential_env"][0],
            "AWS_SECRET_ACCESS_KEY"
        );
    }
}
