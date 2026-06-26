pub fn validate_email(value: &str) -> Result<String, String> {
    let value = value.trim();
    let Some((local, domain)) = value.split_once('@') else {
        return Err("expected one `@` separator".to_string());
    };
    if local.is_empty() || domain.is_empty() || domain.contains('@') {
        return Err("expected non-empty local and domain parts".to_string());
    }
    if domain.starts_with('.') || domain.ends_with('.') || !domain.contains('.') {
        return Err("expected a dotted domain".to_string());
    }
    if !value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '@' | '.' | '_' | '%' | '+' | '-'))
    {
        return Err("expected conservative ASCII email characters".to_string());
    }
    Ok(value.to_string())
}

pub fn validate_url(value: &str) -> Result<String, String> {
    let value = value.trim();
    let rest = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"))
        .ok_or_else(|| "expected absolute http or https URL".to_string())?;
    let host = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .split('@')
        .last()
        .unwrap_or_default()
        .split(':')
        .next()
        .unwrap_or_default();
    if host.is_empty() || !host.contains('.') {
        return Err("expected a dotted host".to_string());
    }
    if !host
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-'))
    {
        return Err("expected conservative ASCII host characters".to_string());
    }
    Ok(value.to_string())
}

pub fn validate_uuid(value: &str) -> Result<String, String> {
    let value = value.trim();
    let parts = value.split('-').collect::<Vec<_>>();
    let lengths = [8, 4, 4, 4, 12];
    if parts.len() != lengths.len()
        || parts
            .iter()
            .zip(lengths)
            .any(|(part, len)| part.len() != len || !part.chars().all(|ch| ch.is_ascii_hexdigit()))
    {
        return Err("expected 8-4-4-4-12 hexadecimal UUID format".to_string());
    }
    Ok(value.to_ascii_lowercase())
}

pub fn validate_phone_number(value: &str) -> Result<String, String> {
    let value = value.trim();
    let digits = value.strip_prefix('+').unwrap_or(value);
    if digits.len() < 8 || digits.len() > 15 || !digits.chars().all(|ch| ch.is_ascii_digit()) {
        return Err("expected 8 to 15 digits with an optional leading `+`".to_string());
    }
    Ok(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_validators_accept_conservative_values() {
        assert_eq!(
            validate_email("  USER+refund@example.com ").unwrap(),
            "USER+refund@example.com"
        );
        assert_eq!(
            validate_url("https://example.com/refunds?id=1").unwrap(),
            "https://example.com/refunds?id=1"
        );
        assert_eq!(
            validate_uuid("550E8400-E29B-41D4-A716-446655440000").unwrap(),
            "550e8400-e29b-41d4-a716-446655440000"
        );
        assert_eq!(
            validate_phone_number("+77001234567").unwrap(),
            "+77001234567"
        );
    }

    #[test]
    fn scalar_validators_reject_out_of_scope_values() {
        assert!(validate_email("not-an-email").is_err());
        assert!(validate_url("ftp://example.com").is_err());
        assert!(validate_uuid("not-a-uuid").is_err());
        assert!(validate_phone_number("555").is_err());
    }
}
