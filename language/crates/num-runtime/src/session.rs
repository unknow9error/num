use crate::{hashing, redaction, SecretValue};
use serde_json::{json, Value as JsonValue};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionVerificationConfig {
    cookie_name: String,
    leeway_seconds: i64,
}

impl SessionVerificationConfig {
    pub fn new(cookie_name: impl Into<String>) -> Self {
        Self {
            cookie_name: cookie_name.into(),
            leeway_seconds: 0,
        }
    }

    pub fn with_leeway_seconds(mut self, seconds: i64) -> Self {
        self.leeway_seconds = seconds.max(0);
        self
    }

    pub fn cookie_name(&self) -> &str {
        &self.cookie_name
    }

    pub fn leeway_seconds(&self) -> i64 {
        self.leeway_seconds
    }

    pub fn to_json(&self) -> JsonValue {
        json!({
            "cookie_name": self.cookie_name,
            "leeway_seconds": self.leeway_seconds,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedSession {
    pub id: String,
    pub actor: String,
    pub tenant: String,
    pub roles: Vec<String>,
    pub expires_at: i64,
    pub issued_at: Option<i64>,
    pub provenance: String,
    pub trust: String,
    pub custom: BTreeMap<String, JsonValue>,
}

impl VerifiedSession {
    pub fn to_json(&self) -> JsonValue {
        json!({
            "id": self.id,
            "actor": self.actor,
            "tenant": self.tenant,
            "roles": self.roles,
            "expires_at": self.expires_at,
            "issued_at": self.issued_at,
            "provenance": self.provenance,
            "trust": self.trust,
            "custom": self.custom,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionVerificationError {
    MissingCookie { cookie_name: String },
    Malformed { reason: String },
    InvalidSignature,
    Expired { expired_at: i64 },
    MissingClaim { claim: String },
}

impl SessionVerificationError {
    pub fn kind(&self) -> &'static str {
        match self {
            SessionVerificationError::MissingCookie { .. } => "session_missing",
            SessionVerificationError::Malformed { .. } => "session_malformed",
            SessionVerificationError::InvalidSignature => "session_invalid_signature",
            SessionVerificationError::Expired { .. } => "session_expired",
            SessionVerificationError::MissingClaim { .. } => "session_missing_claim",
        }
    }

    pub fn message(&self) -> String {
        match self {
            SessionVerificationError::MissingCookie { cookie_name } => {
                format!("Signed session cookie '{cookie_name}' is missing")
            }
            SessionVerificationError::Malformed { reason } => {
                format!(
                    "Signed session cookie is malformed: {}",
                    redaction::redact_text(reason)
                )
            }
            SessionVerificationError::InvalidSignature => {
                "Signed session cookie signature is invalid".to_string()
            }
            SessionVerificationError::Expired { expired_at } => {
                format!("Signed session cookie expired at {expired_at}")
            }
            SessionVerificationError::MissingClaim { claim } => {
                format!("Signed session cookie is missing required claim '{claim}'")
            }
        }
    }

    pub fn to_json(&self) -> JsonValue {
        json!({
            "kind": self.kind(),
            "message": self.message(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct SessionVerifier {
    config: SessionVerificationConfig,
    secret: SecretValue,
}

impl SessionVerifier {
    pub fn new(config: SessionVerificationConfig, secret: SecretValue) -> Self {
        Self { config, secret }
    }

    pub fn config(&self) -> &SessionVerificationConfig {
        &self.config
    }

    pub fn verify_cookie_header(
        &self,
        cookie_header: Option<&str>,
        now_epoch_seconds: i64,
    ) -> Result<VerifiedSession, SessionVerificationError> {
        let value = session_cookie_value(cookie_header, self.config.cookie_name())?;
        self.verify(value, now_epoch_seconds)
    }

    pub fn verify(
        &self,
        value: &str,
        now_epoch_seconds: i64,
    ) -> Result<VerifiedSession, SessionVerificationError> {
        verify_signed_session(&self.config, &self.secret, value, now_epoch_seconds)
    }
}

pub fn session_cookie_value<'a>(
    cookie_header: Option<&'a str>,
    cookie_name: &str,
) -> Result<&'a str, SessionVerificationError> {
    let Some(header) = cookie_header
        .map(str::trim)
        .filter(|header| !header.is_empty())
    else {
        return Err(SessionVerificationError::MissingCookie {
            cookie_name: cookie_name.to_string(),
        });
    };

    for part in header.split(';') {
        let Some((name, value)) = part.trim().split_once('=') else {
            continue;
        };
        if name.trim() == cookie_name {
            let value = value.trim();
            if value.is_empty() {
                return Err(SessionVerificationError::MissingCookie {
                    cookie_name: cookie_name.to_string(),
                });
            }
            return Ok(value);
        }
    }

    Err(SessionVerificationError::MissingCookie {
        cookie_name: cookie_name.to_string(),
    })
}

fn verify_signed_session(
    config: &SessionVerificationConfig,
    secret: &SecretValue,
    value: &str,
    now_epoch_seconds: i64,
) -> Result<VerifiedSession, SessionVerificationError> {
    let parts = value.split('.').collect::<Vec<_>>();
    let [encoded_payload, encoded_signature] = parts.as_slice() else {
        return Err(SessionVerificationError::Malformed {
            reason: "expected payload.signature".to_string(),
        });
    };
    let payload_bytes = hashing::base64url_decode(encoded_payload).map_err(|reason| {
        SessionVerificationError::Malformed {
            reason: format!("invalid payload encoding: {reason}"),
        }
    })?;
    let payload = serde_json::from_slice::<JsonValue>(&payload_bytes).map_err(|err| {
        SessionVerificationError::Malformed {
            reason: format!("invalid payload JSON: {err}"),
        }
    })?;
    let signature = hashing::base64url_decode(encoded_signature).map_err(|reason| {
        SessionVerificationError::Malformed {
            reason: format!("invalid signature encoding: {reason}"),
        }
    })?;
    let expected_signature = hashing::hmac_sha256(secret.expose(), encoded_payload.as_bytes());
    if !constant_time_eq(&signature, &expected_signature) {
        return Err(SessionVerificationError::InvalidSignature);
    }

    let id = claim_string(&payload, "id")?;
    let actor = claim_string(&payload, "actor")?;
    let tenant = claim_string(&payload, "tenant")?;
    let expires_at = claim_i64(&payload, "exp")?;
    if now_epoch_seconds > expires_at + config.leeway_seconds {
        return Err(SessionVerificationError::Expired {
            expired_at: expires_at,
        });
    }
    let roles = claim_string_list(&payload, "roles")?;
    let issued_at = optional_i64(&payload, "iat")?;
    let mut custom = BTreeMap::new();
    if let JsonValue::Object(fields) = payload {
        for (key, value) in fields {
            if !matches!(
                key.as_str(),
                "id" | "actor" | "tenant" | "exp" | "iat" | "roles"
            ) {
                custom.insert(key, value);
            }
        }
    }

    Ok(VerifiedSession {
        id: id.clone(),
        actor,
        tenant,
        roles,
        expires_at,
        issued_at,
        provenance: format!("session:{id}"),
        trust: "verified".to_string(),
        custom,
    })
}

fn claim_string(payload: &JsonValue, claim: &str) -> Result<String, SessionVerificationError> {
    payload
        .get(claim)
        .and_then(JsonValue::as_str)
        .map(ToString::to_string)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| SessionVerificationError::MissingClaim {
            claim: claim.to_string(),
        })
}

fn claim_i64(payload: &JsonValue, claim: &str) -> Result<i64, SessionVerificationError> {
    payload
        .get(claim)
        .and_then(JsonValue::as_i64)
        .ok_or_else(|| SessionVerificationError::MissingClaim {
            claim: claim.to_string(),
        })
}

fn optional_i64(payload: &JsonValue, claim: &str) -> Result<Option<i64>, SessionVerificationError> {
    match payload.get(claim) {
        Some(value) => {
            value
                .as_i64()
                .map(Some)
                .ok_or_else(|| SessionVerificationError::Malformed {
                    reason: format!("claim '{claim}' must be an integer"),
                })
        }
        None => Ok(None),
    }
}

fn claim_string_list(
    payload: &JsonValue,
    claim: &str,
) -> Result<Vec<String>, SessionVerificationError> {
    let Some(values) = payload.get(claim).and_then(JsonValue::as_array) else {
        return Err(SessionVerificationError::MissingClaim {
            claim: claim.to_string(),
        });
    };
    let mut out = Vec::new();
    for value in values {
        let Some(role) = value
            .as_str()
            .map(str::trim)
            .filter(|role| !role.is_empty())
        else {
            return Err(SessionVerificationError::Malformed {
                reason: format!("claim '{claim}' must contain only non-empty strings"),
            });
        };
        out.push(role.to_string());
    }
    Ok(out)
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let diff = left
        .iter()
        .zip(right.iter())
        .fold(0u8, |acc, (left, right)| acc | (left ^ right));
    diff == 0
}

#[cfg(test)]
pub fn sign_session_for_tests(secret: &str, payload: JsonValue) -> String {
    let encoded_payload = hashing::base64url_encode_no_pad(payload.to_string().as_bytes());
    let signature = hashing::hmac_sha256(secret.as_bytes(), encoded_payload.as_bytes());
    format!(
        "{encoded_payload}.{}",
        hashing::base64url_encode_no_pad(&signature)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verifier() -> SessionVerifier {
        SessionVerifier::new(
            SessionVerificationConfig::new("num_session"),
            SecretValue::new("session-secret"),
        )
    }

    fn payload(exp: i64) -> JsonValue {
        json!({
            "id": "sess_123",
            "actor": "agent@example.com",
            "tenant": "tenant_a",
            "roles": ["FinanceManager"],
            "exp": exp,
            "iat": 1_700_000_000,
        })
    }

    #[test]
    fn verifies_signed_session_cookie() {
        let cookie = sign_session_for_tests("session-secret", payload(4_102_444_800));
        let session = verifier()
            .verify_cookie_header(
                Some(&format!("other=1; num_session={cookie}")),
                1_700_000_100,
            )
            .unwrap();

        assert_eq!(session.id, "sess_123");
        assert_eq!(session.actor, "agent@example.com");
        assert_eq!(session.tenant, "tenant_a");
        assert_eq!(session.roles, vec!["FinanceManager"]);
        assert_eq!(session.provenance, "session:sess_123");
        assert_eq!(session.trust, "verified");
    }

    #[test]
    fn rejects_missing_expired_and_tampered_cookies() {
        let missing = verifier()
            .verify_cookie_header(None, 1_700_000_100)
            .unwrap_err();
        assert_eq!(missing.kind(), "session_missing");

        let expired = sign_session_for_tests("session-secret", payload(1_700_000_000));
        let expired = verifier().verify(&expired, 1_700_000_100).unwrap_err();
        assert_eq!(expired.kind(), "session_expired");

        let mut tampered = sign_session_for_tests("session-secret", payload(4_102_444_800));
        tampered.push('x');
        let tampered = verifier().verify(&tampered, 1_700_000_100).unwrap_err();
        assert_eq!(tampered.kind(), "session_invalid_signature");
    }
}
