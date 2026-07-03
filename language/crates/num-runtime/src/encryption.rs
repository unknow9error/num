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
}
