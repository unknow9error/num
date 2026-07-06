use crate::{hashing, redaction, SecretValue};
use serde_json::{json, Value as JsonValue};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JwtVerificationConfig {
    issuer: String,
    audience: String,
    allowed_algorithms: BTreeSet<String>,
    leeway_seconds: i64,
}

impl JwtVerificationConfig {
    pub fn new(issuer: impl Into<String>, audience: impl Into<String>) -> Self {
        Self {
            issuer: issuer.into(),
            audience: audience.into(),
            allowed_algorithms: ["HS256".to_string()].into_iter().collect(),
            leeway_seconds: 0,
        }
    }

    pub fn with_allowed_algorithms(
        mut self,
        algorithms: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.allowed_algorithms = algorithms
            .into_iter()
            .map(Into::into)
            .map(|algorithm| algorithm.trim().to_string())
            .filter(|algorithm| !algorithm.is_empty())
            .collect();
        self
    }

    pub fn with_leeway_seconds(mut self, seconds: i64) -> Self {
        self.leeway_seconds = seconds.max(0);
        self
    }

    pub fn issuer(&self) -> &str {
        &self.issuer
    }

    pub fn audience(&self) -> &str {
        &self.audience
    }

    pub fn allowed_algorithms(&self) -> &BTreeSet<String> {
        &self.allowed_algorithms
    }

    pub fn leeway_seconds(&self) -> i64 {
        self.leeway_seconds
    }

    pub fn to_json(&self) -> JsonValue {
        json!({
            "issuer": self.issuer,
            "audience": self.audience,
            "allowed_algorithms": self.allowed_algorithms.iter().collect::<Vec<_>>(),
            "leeway_seconds": self.leeway_seconds,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedJwtClaims {
    pub issuer: String,
    pub subject: String,
    pub audience: Vec<String>,
    pub expires_at: i64,
    pub issued_at: Option<i64>,
    pub not_before: Option<i64>,
    pub tenant: Option<String>,
    pub roles: Vec<String>,
    pub algorithm: String,
    pub key_id: Option<String>,
    pub provenance: String,
    pub trust: String,
    pub custom: BTreeMap<String, JsonValue>,
}

impl VerifiedJwtClaims {
    pub fn to_json(&self) -> JsonValue {
        json!({
            "issuer": self.issuer,
            "subject": self.subject,
            "audience": self.audience,
            "expires_at": self.expires_at,
            "issued_at": self.issued_at,
            "not_before": self.not_before,
            "tenant": self.tenant,
            "roles": self.roles,
            "algorithm": self.algorithm,
            "key_id": self.key_id,
            "provenance": self.provenance,
            "trust": self.trust,
            "custom": self.custom,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JwtVerificationError {
    MissingToken,
    Malformed { reason: String },
    UnsupportedAlgorithm { algorithm: String },
    InvalidSignature,
    InvalidIssuer { expected: String },
    InvalidAudience { expected: String },
    Expired { expired_at: i64 },
    NotYetValid { not_before: i64 },
    MissingClaim { claim: String },
}

impl JwtVerificationError {
    pub fn kind(&self) -> &'static str {
        match self {
            JwtVerificationError::MissingToken => "jwt_missing",
            JwtVerificationError::Malformed { .. } => "jwt_malformed",
            JwtVerificationError::UnsupportedAlgorithm { .. } => "jwt_unsupported_algorithm",
            JwtVerificationError::InvalidSignature => "jwt_invalid_signature",
            JwtVerificationError::InvalidIssuer { .. } => "jwt_invalid_issuer",
            JwtVerificationError::InvalidAudience { .. } => "jwt_invalid_audience",
            JwtVerificationError::Expired { .. } => "jwt_expired",
            JwtVerificationError::NotYetValid { .. } => "jwt_not_yet_valid",
            JwtVerificationError::MissingClaim { .. } => "jwt_missing_claim",
        }
    }

    pub fn message(&self) -> String {
        match self {
            JwtVerificationError::MissingToken => "JWT bearer authorization is missing".to_string(),
            JwtVerificationError::Malformed { reason } => {
                format!("JWT is malformed: {}", redaction::redact_text(reason))
            }
            JwtVerificationError::UnsupportedAlgorithm { algorithm } => {
                format!("JWT algorithm '{algorithm}' is not allowed")
            }
            JwtVerificationError::InvalidSignature => "JWT signature is invalid".to_string(),
            JwtVerificationError::InvalidIssuer { expected } => {
                format!("JWT issuer does not match expected issuer '{expected}'")
            }
            JwtVerificationError::InvalidAudience { expected } => {
                format!("JWT audience does not include expected audience '{expected}'")
            }
            JwtVerificationError::Expired { expired_at } => {
                format!("JWT expired at {expired_at}")
            }
            JwtVerificationError::NotYetValid { not_before } => {
                format!("JWT is not valid before {not_before}")
            }
            JwtVerificationError::MissingClaim { claim } => {
                format!("JWT is missing required claim '{claim}'")
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
pub struct JwtVerifier {
    config: JwtVerificationConfig,
    secret: SecretValue,
}

impl JwtVerifier {
    pub fn new(config: JwtVerificationConfig, secret: SecretValue) -> Self {
        Self { config, secret }
    }

    pub fn config(&self) -> &JwtVerificationConfig {
        &self.config
    }

    pub fn verify(
        &self,
        token: &str,
        now_epoch_seconds: i64,
    ) -> Result<VerifiedJwtClaims, JwtVerificationError> {
        verify_hs_jwt(&self.config, &self.secret, token, now_epoch_seconds)
    }
}

pub fn bearer_token(value: Option<&str>) -> Result<&str, JwtVerificationError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Err(JwtVerificationError::MissingToken);
    };
    let Some(token) = value.strip_prefix("Bearer ") else {
        return Err(JwtVerificationError::Malformed {
            reason: "Authorization header must use Bearer scheme".to_string(),
        });
    };
    let token = token.trim();
    if token.is_empty() {
        return Err(JwtVerificationError::MissingToken);
    }
    Ok(token)
}

fn verify_hs_jwt(
    config: &JwtVerificationConfig,
    secret: &SecretValue,
    token: &str,
    now_epoch_seconds: i64,
) -> Result<VerifiedJwtClaims, JwtVerificationError> {
    let parts = token.split('.').collect::<Vec<_>>();
    let [encoded_header, encoded_payload, encoded_signature] = parts.as_slice() else {
        return Err(JwtVerificationError::Malformed {
            reason: "expected header.payload.signature".to_string(),
        });
    };
    let signing_input = format!("{encoded_header}.{encoded_payload}");
    let header = decode_json_part(encoded_header, "header")?;
    let payload = decode_json_part(encoded_payload, "payload")?;
    let signature = hashing::base64url_decode(encoded_signature).map_err(|reason| {
        JwtVerificationError::Malformed {
            reason: format!("invalid signature encoding: {reason}"),
        }
    })?;

    let algorithm = claim_string(&header, "alg")?;
    if !config.allowed_algorithms.contains(&algorithm) {
        return Err(JwtVerificationError::UnsupportedAlgorithm { algorithm });
    }
    if algorithm != "HS256" {
        return Err(JwtVerificationError::UnsupportedAlgorithm { algorithm });
    }
    let expected_signature = hashing::hmac_sha256(secret.expose(), signing_input.as_bytes());
    if !constant_time_eq(&signature, &expected_signature) {
        return Err(JwtVerificationError::InvalidSignature);
    }

    let issuer = claim_string(&payload, "iss")?;
    if issuer != config.issuer {
        return Err(JwtVerificationError::InvalidIssuer {
            expected: config.issuer.clone(),
        });
    }

    let audience = claim_audience(&payload)?;
    if !audience.iter().any(|aud| aud == &config.audience) {
        return Err(JwtVerificationError::InvalidAudience {
            expected: config.audience.clone(),
        });
    }

    let expires_at = claim_i64(&payload, "exp")?;
    if now_epoch_seconds > expires_at + config.leeway_seconds {
        return Err(JwtVerificationError::Expired {
            expired_at: expires_at,
        });
    }
    let not_before = optional_i64(&payload, "nbf")?;
    if not_before.is_some_and(|nbf| now_epoch_seconds + config.leeway_seconds < nbf) {
        return Err(JwtVerificationError::NotYetValid {
            not_before: not_before.unwrap(),
        });
    }

    let issued_at = optional_i64(&payload, "iat")?;
    let subject = claim_string(&payload, "sub")?;
    let roles = claim_string_list(&payload, "roles")?;
    let tenant = optional_string(&payload, "tenant")?;
    let key_id = optional_string(&header, "kid")?;
    let custom = custom_claims(&payload);

    Ok(VerifiedJwtClaims {
        issuer: issuer.clone(),
        subject,
        audience,
        expires_at,
        issued_at,
        not_before,
        tenant,
        roles,
        algorithm,
        key_id,
        provenance: format!("jwt:{issuer}"),
        trust: "verified".to_string(),
        custom,
    })
}

fn decode_json_part(encoded: &str, label: &str) -> Result<JsonValue, JwtVerificationError> {
    let bytes =
        hashing::base64url_decode(encoded).map_err(|reason| JwtVerificationError::Malformed {
            reason: format!("invalid {label} encoding: {reason}"),
        })?;
    serde_json::from_slice(&bytes).map_err(|err| JwtVerificationError::Malformed {
        reason: format!("invalid {label} JSON: {err}"),
    })
}

fn claim_string(value: &JsonValue, name: &str) -> Result<String, JwtVerificationError> {
    value
        .get(name)
        .and_then(JsonValue::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| JwtVerificationError::MissingClaim {
            claim: name.to_string(),
        })
}

fn optional_string(value: &JsonValue, name: &str) -> Result<Option<String>, JwtVerificationError> {
    let Some(raw) = value.get(name) else {
        return Ok(None);
    };
    raw.as_str()
        .map(|value| Some(value.to_string()))
        .ok_or_else(|| JwtVerificationError::Malformed {
            reason: format!("claim '{name}' must be a string"),
        })
}

fn claim_i64(value: &JsonValue, name: &str) -> Result<i64, JwtVerificationError> {
    value
        .get(name)
        .and_then(JsonValue::as_i64)
        .ok_or_else(|| JwtVerificationError::MissingClaim {
            claim: name.to_string(),
        })
}

fn optional_i64(value: &JsonValue, name: &str) -> Result<Option<i64>, JwtVerificationError> {
    let Some(raw) = value.get(name) else {
        return Ok(None);
    };
    raw.as_i64()
        .map(Some)
        .ok_or_else(|| JwtVerificationError::Malformed {
            reason: format!("claim '{name}' must be an integer timestamp"),
        })
}

fn claim_audience(value: &JsonValue) -> Result<Vec<String>, JwtVerificationError> {
    let Some(raw) = value.get("aud") else {
        return Err(JwtVerificationError::MissingClaim {
            claim: "aud".to_string(),
        });
    };
    if let Some(audience) = raw.as_str() {
        return Ok(vec![audience.to_string()]);
    }
    let Some(items) = raw.as_array() else {
        return Err(JwtVerificationError::Malformed {
            reason: "claim 'aud' must be a string or string array".to_string(),
        });
    };
    let mut audience = Vec::new();
    for item in items {
        let Some(item) = item.as_str() else {
            return Err(JwtVerificationError::Malformed {
                reason: "claim 'aud' array must contain only strings".to_string(),
            });
        };
        audience.push(item.to_string());
    }
    Ok(audience)
}

fn claim_string_list(value: &JsonValue, name: &str) -> Result<Vec<String>, JwtVerificationError> {
    let Some(raw) = value.get(name) else {
        return Ok(Vec::new());
    };
    if let Some(raw) = raw.as_str() {
        return Ok(raw
            .split(',')
            .map(str::trim)
            .filter(|role| !role.is_empty())
            .map(ToString::to_string)
            .collect());
    }
    let Some(items) = raw.as_array() else {
        return Err(JwtVerificationError::Malformed {
            reason: format!("claim '{name}' must be a string or string array"),
        });
    };
    let mut out = Vec::new();
    for item in items {
        let Some(item) = item.as_str() else {
            return Err(JwtVerificationError::Malformed {
                reason: format!("claim '{name}' array must contain only strings"),
            });
        };
        out.push(item.to_string());
    }
    Ok(out)
}

fn custom_claims(value: &JsonValue) -> BTreeMap<String, JsonValue> {
    const REGISTERED: &[&str] = &[
        "iss", "sub", "aud", "exp", "nbf", "iat", "jti", "tenant", "roles",
    ];
    value
        .as_object()
        .into_iter()
        .flat_map(|object| object.iter())
        .filter(|(key, _)| !REGISTERED.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0u8, |acc, (left, right)| acc | (left ^ right))
        == 0
}

#[cfg(test)]
pub fn sign_hs256_for_tests(header: JsonValue, payload: JsonValue, secret: &SecretValue) -> String {
    let encoded_header =
        hashing::base64url_encode_no_pad(serde_json::to_string(&header).unwrap().as_bytes());
    let encoded_payload =
        hashing::base64url_encode_no_pad(serde_json::to_string(&payload).unwrap().as_bytes());
    let signing_input = format!("{encoded_header}.{encoded_payload}");
    let signature = hashing::hmac_sha256(secret.expose(), signing_input.as_bytes());
    let encoded_signature = hashing::base64url_encode_no_pad(&signature);
    format!("{signing_input}.{encoded_signature}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> JwtVerificationConfig {
        JwtVerificationConfig::new("https://issuer.example", "num-api")
    }

    fn secret() -> SecretValue {
        SecretValue::new("test-signing-secret")
    }

    fn valid_token(exp: i64) -> String {
        sign_hs256_for_tests(
            json!({"alg": "HS256", "typ": "JWT", "kid": "jwt-test-key"}),
            json!({
                "iss": "https://issuer.example",
                "sub": "agent@example.com",
                "aud": ["num-api", "other-api"],
                "exp": exp,
                "iat": 1700000000,
                "tenant": "tenant_a",
                "roles": ["FinanceManager", "Auditor"],
                "scope": "refunds:write"
            }),
            &secret(),
        )
    }

    #[test]
    fn verifies_hs256_claims_with_trust_metadata() {
        let verifier = JwtVerifier::new(config(), secret());
        let claims = verifier
            .verify(&valid_token(4_102_444_800), 1_700_000_100)
            .unwrap();

        assert_eq!(claims.issuer, "https://issuer.example");
        assert_eq!(claims.subject, "agent@example.com");
        assert_eq!(claims.audience, vec!["num-api", "other-api"]);
        assert_eq!(claims.tenant.as_deref(), Some("tenant_a"));
        assert_eq!(claims.roles, vec!["FinanceManager", "Auditor"]);
        assert_eq!(claims.algorithm, "HS256");
        assert_eq!(claims.key_id.as_deref(), Some("jwt-test-key"));
        assert_eq!(claims.provenance, "jwt:https://issuer.example");
        assert_eq!(claims.trust, "verified");
        assert_eq!(claims.to_json()["custom"]["scope"], "refunds:write");
    }

    #[test]
    fn fails_closed_for_expired_wrong_audience_and_bad_signature() {
        let verifier = JwtVerifier::new(config(), secret());
        assert_eq!(
            verifier
                .verify(&valid_token(1_700_000_000), 1_700_000_100)
                .unwrap_err()
                .kind(),
            "jwt_expired"
        );

        let wrong_audience = sign_hs256_for_tests(
            json!({"alg": "HS256"}),
            json!({
                "iss": "https://issuer.example",
                "sub": "agent@example.com",
                "aud": "other-api",
                "exp": 4_102_444_800i64
            }),
            &secret(),
        );
        assert_eq!(
            verifier
                .verify(&wrong_audience, 1_700_000_100)
                .unwrap_err()
                .kind(),
            "jwt_invalid_audience"
        );

        let mut tampered = valid_token(4_102_444_800);
        tampered.pop();
        tampered.push('A');
        let error = verifier.verify(&tampered, 1_700_000_100).unwrap_err();
        assert!(matches!(
            error,
            JwtVerificationError::InvalidSignature | JwtVerificationError::Malformed { .. }
        ));
        assert!(!error.to_json().to_string().contains("test-signing-secret"));
    }

    #[test]
    fn rejects_none_or_unconfigured_algorithms() {
        let none_token = sign_hs256_for_tests(
            json!({"alg": "none"}),
            json!({
                "iss": "https://issuer.example",
                "sub": "agent@example.com",
                "aud": "num-api",
                "exp": 4_102_444_800i64
            }),
            &secret(),
        );
        let verifier = JwtVerifier::new(config(), secret());

        assert_eq!(
            verifier
                .verify(&none_token, 1_700_000_100)
                .unwrap_err()
                .kind(),
            "jwt_unsupported_algorithm"
        );
    }
}
