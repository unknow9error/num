use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<JsonValue>),
    Object(HashMap<String, JsonValue>),
}

impl JsonValue {
    pub fn get(&self, key: &str) -> Option<&JsonValue> {
        match self {
            JsonValue::Object(map) => map.get(key),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            JsonValue::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            JsonValue::Number(n) => Some(*n),
            _ => None,
        }
    }
}

pub struct JsonParser {
    chars: Vec<char>,
    pos: usize,
}

impl JsonParser {
    pub fn new(s: &str) -> Self {
        Self {
            chars: s.chars().collect(),
            pos: 0,
        }
    }

    pub fn parse(&mut self) -> Result<JsonValue, String> {
        self.skip_whitespace();
        if self.pos >= self.chars.len() {
            return Err("Unexpected EOF".to_string());
        }
        let ch = self.chars[self.pos];
        if ch == '{' {
            self.parse_object()
        } else if ch == '[' {
            self.parse_array()
        } else if ch == '"' {
            self.parse_string()
        } else if ch.is_ascii_digit() || ch == '-' {
            self.parse_number()
        } else if self.match_keyword("true") {
            Ok(JsonValue::Bool(true))
        } else if self.match_keyword("false") {
            Ok(JsonValue::Bool(false))
        } else if self.match_keyword("null") {
            Ok(JsonValue::Null)
        } else {
            Err(format!("Unexpected character: {}", ch))
        }
    }

    fn parse_object(&mut self) -> Result<JsonValue, String> {
        self.pos += 1;
        let mut obj = HashMap::new();
        self.skip_whitespace();
        if self.pos < self.chars.len() && self.chars[self.pos] == '}' {
            self.pos += 1;
            return Ok(JsonValue::Object(obj));
        }
        loop {
            self.skip_whitespace();
            let key = match self.parse_string()? {
                JsonValue::String(s) => s,
                _ => return Err("Expected string key".to_string()),
            };
            self.skip_whitespace();
            if self.pos >= self.chars.len() || self.chars[self.pos] != ':' {
                return Err("Expected ':'".to_string());
            }
            self.pos += 1;
            let val = self.parse()?;
            obj.insert(key, val);
            self.skip_whitespace();
            if self.pos < self.chars.len() && self.chars[self.pos] == ',' {
                self.pos += 1;
            } else if self.pos < self.chars.len() && self.chars[self.pos] == '}' {
                self.pos += 1;
                break;
            } else {
                return Err("Expected ',' or '}'".to_string());
            }
        }
        Ok(JsonValue::Object(obj))
    }

    fn parse_array(&mut self) -> Result<JsonValue, String> {
        self.pos += 1;
        let mut arr = Vec::new();
        self.skip_whitespace();
        if self.pos < self.chars.len() && self.chars[self.pos] == ']' {
            self.pos += 1;
            return Ok(JsonValue::Array(arr));
        }
        loop {
            let val = self.parse()?;
            arr.push(val);
            self.skip_whitespace();
            if self.pos < self.chars.len() && self.chars[self.pos] == ',' {
                self.pos += 1;
            } else if self.pos < self.chars.len() && self.chars[self.pos] == ']' {
                self.pos += 1;
                break;
            } else {
                return Err("Expected ',' or ']'".to_string());
            }
        }
        Ok(JsonValue::Array(arr))
    }

    fn parse_string(&mut self) -> Result<JsonValue, String> {
        if self.pos >= self.chars.len() || self.chars[self.pos] != '"' {
            return Err("Expected '\"'".to_string());
        }
        self.pos += 1;
        let mut s = String::new();
        while self.pos < self.chars.len() {
            let ch = self.chars[self.pos];
            if ch == '"' {
                self.pos += 1;
                return Ok(JsonValue::String(s));
            } else if ch == '\\' {
                self.pos += 1;
                if self.pos >= self.chars.len() {
                    return Err("Unterminated escape sequence".to_string());
                }
                let esc = self.chars[self.pos];
                self.pos += 1;
                match esc {
                    '"' => s.push('"'),
                    '\\' => s.push('\\'),
                    '/' => s.push('/'),
                    'b' => s.push('\x08'),
                    'f' => s.push('\x0c'),
                    'n' => s.push('\n'),
                    'r' => s.push('\r'),
                    't' => s.push('\t'),
                    'u' => {
                        if self.pos + 4 <= self.chars.len() {
                            let hex: String = self.chars[self.pos..self.pos + 4].iter().collect();
                            self.pos += 4;
                            if let Ok(code) = u32::from_str_radix(&hex, 16) {
                                if let Some(c) = char::from_u32(code) {
                                    s.push(c);
                                }
                            }
                        }
                    }
                    _ => s.push(esc),
                }
            } else {
                s.push(ch);
                self.pos += 1;
            }
        }
        Err("Unterminated string".to_string())
    }

    fn parse_number(&mut self) -> Result<JsonValue, String> {
        let mut s = String::new();
        while self.pos < self.chars.len() {
            let ch = self.chars[self.pos];
            if ch.is_ascii_digit() || ch == '.' || ch == '-' || ch == '+' || ch == 'e' || ch == 'E'
            {
                s.push(ch);
                self.pos += 1;
            } else {
                break;
            }
        }
        s.parse::<f64>()
            .map(JsonValue::Number)
            .map_err(|e| e.to_string())
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.chars.len() && self.chars[self.pos].is_whitespace() {
            self.pos += 1;
        }
    }

    fn match_keyword(&mut self, kw: &str) -> bool {
        let len = kw.len();
        if self.pos + len <= self.chars.len() {
            let s: String = self.chars[self.pos..self.pos + len].iter().collect();
            if s == kw {
                self.pos += len;
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_json_objects() {
        let json_str = r#"{"jsonrpc":"2.0","id":42,"method":"initialize","params":{"textDocument":{"uri":"file:///test.num"}}}"#;
        let mut parser = JsonParser::new(json_str);
        let val = parser.parse().unwrap();

        assert_eq!(val.get("jsonrpc").unwrap().as_str(), Some("2.0"));
        assert_eq!(val.get("id").unwrap().as_f64(), Some(42.0));
        assert_eq!(val.get("method").unwrap().as_str(), Some("initialize"));

        let params = val.get("params").unwrap();
        let doc = params.get("textDocument").unwrap();
        assert_eq!(doc.get("uri").unwrap().as_str(), Some("file:///test.num"));
    }

    #[test]
    fn parse_json_escaped_strings() {
        let json_str = r#"{"msg":"Hello \"world\"\nNew Line"}"#;
        let mut parser = JsonParser::new(json_str);
        let val = parser.parse().unwrap();
        assert_eq!(
            val.get("msg").unwrap().as_str(),
            Some("Hello \"world\"\nNew Line")
        );
    }
}
