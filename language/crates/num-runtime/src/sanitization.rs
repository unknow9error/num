use crate::RuntimeError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSanitizationPolicy {
    pub trim: bool,
    pub strip_control_chars: bool,
    pub max_chars: Option<usize>,
    pub lowercase: bool,
    pub collapse_whitespace: bool,
    pub allowed_chars: Option<TextCharClass>,
}

impl Default for TextSanitizationPolicy {
    fn default() -> Self {
        Self {
            trim: true,
            strip_control_chars: true,
            max_chars: None,
            lowercase: false,
            collapse_whitespace: false,
            allowed_chars: None,
        }
    }
}

impl TextSanitizationPolicy {
    pub fn compose(&self, other: &Self) -> Self {
        Self {
            trim: self.trim || other.trim,
            strip_control_chars: self.strip_control_chars || other.strip_control_chars,
            max_chars: stricter_max_chars(self.max_chars, other.max_chars),
            lowercase: self.lowercase || other.lowercase,
            collapse_whitespace: self.collapse_whitespace || other.collapse_whitespace,
            allowed_chars: compose_char_class(self.allowed_chars, other.allowed_chars),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextCharClass {
    AlphaHyphen,
    Email,
    Identifier,
    PersonName,
}

impl TextCharClass {
    fn allows(self, ch: char) -> bool {
        match self {
            TextCharClass::AlphaHyphen => ch.is_alphabetic() || ch == '-',
            TextCharClass::Email => {
                ch.is_ascii_alphanumeric() || matches!(ch, '@' | '.' | '_' | '%' | '+' | '-')
            }
            TextCharClass::Identifier => ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'),
            TextCharClass::PersonName => {
                ch.is_alphabetic() || ch.is_whitespace() || matches!(ch, '\'' | '-')
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SanitizerPack {
    PlainText,
    Email,
    PersonName,
    Identifier,
}

impl SanitizerPack {
    pub fn named(name: &str) -> Result<Self, RuntimeError> {
        match name {
            "plain_text" | "text" => Ok(Self::PlainText),
            "email" => Ok(Self::Email),
            "person_name" | "name" => Ok(Self::PersonName),
            "identifier" | "id" => Ok(Self::Identifier),
            other => Err(RuntimeError::SanitizationFailed {
                reason: format!("unknown sanitizer pack '{other}'"),
            }),
        }
    }

    pub fn policy(self) -> TextSanitizationPolicy {
        match self {
            Self::PlainText => TextSanitizationPolicy {
                collapse_whitespace: true,
                max_chars: Some(4_000),
                ..TextSanitizationPolicy::default()
            },
            Self::Email => TextSanitizationPolicy {
                lowercase: true,
                max_chars: Some(254),
                allowed_chars: Some(TextCharClass::Email),
                ..TextSanitizationPolicy::default()
            },
            Self::PersonName => TextSanitizationPolicy {
                collapse_whitespace: true,
                max_chars: Some(120),
                allowed_chars: Some(TextCharClass::PersonName),
                ..TextSanitizationPolicy::default()
            },
            Self::Identifier => TextSanitizationPolicy {
                max_chars: Some(128),
                allowed_chars: Some(TextCharClass::Identifier),
                ..TextSanitizationPolicy::default()
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SanitizedText {
    pub value: String,
    pub original_chars: usize,
    pub trimmed: bool,
    pub stripped_control_chars: usize,
    pub truncated: bool,
    pub lowercased: bool,
    pub collapsed_whitespace: bool,
}

impl SanitizedText {
    pub fn changed(&self) -> bool {
        self.trimmed
            || self.stripped_control_chars > 0
            || self.truncated
            || self.lowercased
            || self.collapsed_whitespace
    }
}

pub trait TextSanitizer {
    fn sanitize_text(
        &self,
        input: &str,
        policy: &TextSanitizationPolicy,
    ) -> Result<SanitizedText, RuntimeError>;
}

#[derive(Debug, Clone, Default)]
pub struct DefaultTextSanitizer;

impl DefaultTextSanitizer {
    pub fn new() -> Self {
        Self
    }
}

impl TextSanitizer for DefaultTextSanitizer {
    fn sanitize_text(
        &self,
        input: &str,
        policy: &TextSanitizationPolicy,
    ) -> Result<SanitizedText, RuntimeError> {
        if policy.max_chars == Some(0) {
            return Err(RuntimeError::SanitizationFailed {
                reason: "max_chars must be greater than zero".to_string(),
            });
        }

        let original_chars = input.chars().count();
        let mut value = if policy.trim {
            input.trim().to_string()
        } else {
            input.to_string()
        };
        let trimmed = value.chars().count() != original_chars;

        let stripped_control_chars = if policy.strip_control_chars {
            let before = value.chars().count();
            value = value
                .chars()
                .filter(|ch| !ch.is_control() || matches!(ch, '\n' | '\r' | '\t'))
                .collect();
            before.saturating_sub(value.chars().count())
        } else {
            0
        };

        let collapsed_whitespace = if policy.collapse_whitespace {
            let collapsed = collapse_whitespace(&value);
            let changed = collapsed != value;
            value = collapsed;
            changed
        } else {
            false
        };

        let lowercased = if policy.lowercase {
            let lowered = value.to_lowercase();
            let changed = lowered != value;
            value = lowered;
            changed
        } else {
            false
        };

        let truncated = if let Some(max_chars) = policy.max_chars {
            let current_chars = value.chars().count();
            if current_chars > max_chars {
                value = value.chars().take(max_chars).collect();
                true
            } else {
                false
            }
        } else {
            false
        };

        if let Some(char_class) = policy.allowed_chars {
            if let Some(invalid) = value.chars().find(|ch| !char_class.allows(*ch)) {
                return Err(RuntimeError::SanitizationFailed {
                    reason: format!("character '{invalid}' is not allowed by {char_class:?}"),
                });
            }
        }

        Ok(SanitizedText {
            value,
            original_chars,
            trimmed,
            stripped_control_chars,
            truncated,
            lowercased,
            collapsed_whitespace,
        })
    }
}

pub fn sanitize_with_pack(input: &str, pack: SanitizerPack) -> Result<SanitizedText, RuntimeError> {
    DefaultTextSanitizer::new().sanitize_text(input, &pack.policy())
}

pub fn sanitize_with_packs(
    input: &str,
    packs: &[SanitizerPack],
) -> Result<SanitizedText, RuntimeError> {
    let policy = packs
        .iter()
        .map(|pack| pack.policy())
        .reduce(|left, right| left.compose(&right))
        .unwrap_or_default();
    DefaultTextSanitizer::new().sanitize_text(input, &policy)
}

fn stricter_max_chars(left: Option<usize>, right: Option<usize>) -> Option<usize> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn compose_char_class(
    left: Option<TextCharClass>,
    right: Option<TextCharClass>,
) -> Option<TextCharClass> {
    match (left, right) {
        (Some(TextCharClass::AlphaHyphen), Some(TextCharClass::AlphaHyphen)) => {
            Some(TextCharClass::AlphaHyphen)
        }
        (Some(TextCharClass::Email), Some(TextCharClass::Email)) => Some(TextCharClass::Email),
        (Some(TextCharClass::Identifier), Some(TextCharClass::Identifier)) => {
            Some(TextCharClass::Identifier)
        }
        (Some(TextCharClass::PersonName), Some(TextCharClass::PersonName)) => {
            Some(TextCharClass::PersonName)
        }
        (Some(TextCharClass::AlphaHyphen), Some(_))
        | (Some(_), Some(TextCharClass::AlphaHyphen)) => Some(TextCharClass::AlphaHyphen),
        (Some(TextCharClass::Identifier), Some(TextCharClass::Email))
        | (Some(TextCharClass::Email), Some(TextCharClass::Identifier)) => {
            Some(TextCharClass::Identifier)
        }
        (Some(TextCharClass::Identifier), Some(TextCharClass::PersonName))
        | (Some(TextCharClass::PersonName), Some(TextCharClass::Identifier))
        | (Some(TextCharClass::Email), Some(TextCharClass::PersonName))
        | (Some(TextCharClass::PersonName), Some(TextCharClass::Email)) => {
            Some(TextCharClass::AlphaHyphen)
        }
        (Some(left), None) | (None, Some(left)) => Some(left),
        (None, None) => None,
    }
}

fn collapse_whitespace(value: &str) -> String {
    let mut out = String::new();
    let mut last_was_whitespace = false;
    for ch in value.chars() {
        if ch.is_whitespace() {
            if !last_was_whitespace {
                out.push(' ');
            }
            last_was_whitespace = true;
        } else {
            out.push(ch);
            last_was_whitespace = false;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        sanitize_with_pack, sanitize_with_packs, DefaultTextSanitizer, SanitizerPack,
        TextCharClass, TextSanitizationPolicy, TextSanitizer,
    };
    use crate::RuntimeError;

    #[test]
    fn default_text_sanitizer_trims_and_strips_control_chars() {
        let sanitizer = DefaultTextSanitizer::new();
        let result = sanitizer
            .sanitize_text(
                "  hello\u{0000}\nworld\u{0008}  ",
                &TextSanitizationPolicy::default(),
            )
            .unwrap();

        assert_eq!(result.value, "hello\nworld");
        assert_eq!(result.stripped_control_chars, 2);
        assert!(result.trimmed);
        assert!(result.changed());
    }

    #[test]
    fn default_text_sanitizer_truncates_by_chars() {
        let sanitizer = DefaultTextSanitizer::new();
        let policy = TextSanitizationPolicy {
            max_chars: Some(4),
            ..TextSanitizationPolicy::default()
        };

        let result = sanitizer.sanitize_text("  abcdef  ", &policy).unwrap();

        assert_eq!(result.value, "abcd");
        assert!(result.truncated);
    }

    #[test]
    fn default_text_sanitizer_rejects_zero_max_chars() {
        let sanitizer = DefaultTextSanitizer::new();
        let policy = TextSanitizationPolicy {
            max_chars: Some(0),
            ..TextSanitizationPolicy::default()
        };

        let error = sanitizer.sanitize_text("value", &policy).unwrap_err();

        assert!(matches!(error, RuntimeError::SanitizationFailed { .. }));
    }

    #[test]
    fn sanitizer_pack_normalizes_email() {
        let result =
            sanitize_with_pack("  USER+Refund@Example.COM  ", SanitizerPack::Email).unwrap();

        assert_eq!(result.value, "user+refund@example.com");
        assert!(result.lowercased);
        assert!(result.trimmed);
    }

    #[test]
    fn sanitizer_pack_rejects_invalid_identifier_characters() {
        let error = sanitize_with_pack("unsafe/id", SanitizerPack::Identifier).unwrap_err();

        assert!(matches!(error, RuntimeError::SanitizationFailed { .. }));
    }

    #[test]
    fn sanitizer_packs_compose_stricter_policy() {
        let result = sanitize_with_packs(
            "  Customer\t\nName  ",
            &[SanitizerPack::PlainText, SanitizerPack::PersonName],
        )
        .unwrap();

        assert_eq!(result.value, "Customer Name");
        assert!(result.collapsed_whitespace);
    }

    #[test]
    fn sanitizer_policy_composition_keeps_stricter_max_chars() {
        let base = TextSanitizationPolicy {
            max_chars: Some(20),
            ..TextSanitizationPolicy::default()
        };
        let strict = TextSanitizationPolicy {
            max_chars: Some(5),
            allowed_chars: Some(TextCharClass::Identifier),
            ..TextSanitizationPolicy::default()
        };

        let policy = base.compose(&strict);
        let result = DefaultTextSanitizer::new()
            .sanitize_text("abcdef", &policy)
            .unwrap();

        assert_eq!(result.value, "abcde");
        assert_eq!(policy.max_chars, Some(5));
    }

    #[test]
    fn sanitizer_pack_names_are_validated() {
        assert_eq!(
            SanitizerPack::named("person_name").unwrap(),
            SanitizerPack::PersonName
        );
        assert!(matches!(
            SanitizerPack::named("unknown").unwrap_err(),
            RuntimeError::SanitizationFailed { .. }
        ));
    }
}
