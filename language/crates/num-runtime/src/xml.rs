pub fn validate_xml_document(raw: &str) -> Result<(), String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("XML value cannot be empty".to_string());
    }
    if !trimmed.starts_with('<') || !trimmed.ends_with('>') {
        return Err("XML value must start with '<' and end with '>'".to_string());
    }
    if trimmed
        .chars()
        .any(|ch| ch.is_control() && !ch.is_whitespace())
    {
        return Err("XML value contains unsupported control characters".to_string());
    }
    if !contains_tag_name(trimmed) {
        return Err("XML value must contain at least one element tag".to_string());
    }
    Ok(())
}

fn contains_tag_name(raw: &str) -> bool {
    let mut rest = raw;
    while let Some(offset) = rest.find('<') {
        rest = &rest[offset + 1..];
        if matches!(rest.chars().next(), Some('!') | Some('?') | Some('/')) {
            continue;
        }
        return rest
            .chars()
            .take_while(|ch| !ch.is_whitespace() && !matches!(ch, '/' | '>'))
            .any(|ch| ch == '_' || ch == ':' || ch == '-' || ch.is_ascii_alphanumeric());
    }
    false
}

#[cfg(test)]
mod tests {
    use super::validate_xml_document;

    #[test]
    fn accepts_minimal_xml_documents() {
        validate_xml_document("<root/>").unwrap();
        validate_xml_document("<?xml version=\"1.0\"?><root><item /></root>").unwrap();
    }

    #[test]
    fn rejects_empty_or_non_xml_text() {
        assert!(validate_xml_document("").is_err());
        assert!(validate_xml_document("hello").is_err());
        assert!(validate_xml_document("<!doctype note>").is_err());
    }
}
