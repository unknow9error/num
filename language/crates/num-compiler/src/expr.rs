#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Ident(String),
    String(String),
    Bool(bool),
    Int(i64),
    Float(f64),
    Object(Vec<ObjectField>),
    Member {
        object: Box<Expr>,
        field: String,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    Try(Box<Expr>),
    Binary {
        left: Box<Expr>,
        op: BinaryOp,
        right: Box<Expr>,
    },
    Quantity(String, String),
    Async(Box<Expr>),
    Await(Box<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObjectField {
    pub name: String,
    pub value: Expr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Or,
    And,
    Equal,
    NotEqual,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
    Add,
    Subtract,
    Multiply,
    Divide,
}

impl BinaryOp {
    pub fn as_str(self) -> &'static str {
        match self {
            BinaryOp::Or => "||",
            BinaryOp::And => "&&",
            BinaryOp::Equal => "==",
            BinaryOp::NotEqual => "!=",
            BinaryOp::LessThan => "<",
            BinaryOp::LessThanOrEqual => "<=",
            BinaryOp::GreaterThan => ">",
            BinaryOp::GreaterThanOrEqual => ">=",
            BinaryOp::Add => "+",
            BinaryOp::Subtract => "-",
            BinaryOp::Multiply => "*",
            BinaryOp::Divide => "/",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExprError {
    pub message: String,
}

impl Expr {
    pub fn contains_ident(&self, name: &str) -> bool {
        match self {
            Expr::Ident(candidate) => candidate == name,
            Expr::String(_) | Expr::Bool(_) | Expr::Int(_) | Expr::Float(_) | Expr::Quantity(_, _) => false,
            Expr::Object(fields) => fields.iter().any(|field| field.value.contains_ident(name)),
            Expr::Member { object, .. } => object.contains_ident(name),
            Expr::Call { callee, args } => {
                callee.contains_ident(name) || args.iter().any(|arg| arg.contains_ident(name))
            }
            Expr::Try(inner) | Expr::Async(inner) | Expr::Await(inner) => inner.contains_ident(name),
            Expr::Binary { left, right, .. } => {
                left.contains_ident(name) || right.contains_ident(name)
            }
        }
    }

    pub fn contains_member_field(&self, field: &str) -> bool {
        match self {
            Expr::Ident(_) | Expr::String(_) | Expr::Bool(_) | Expr::Int(_) | Expr::Float(_) | Expr::Quantity(_, _) => {
                false
            }
            Expr::Object(fields) => fields
                .iter()
                .any(|object_field| object_field.value.contains_member_field(field)),
            Expr::Member { object, field: f } => f == field || object.contains_member_field(field),
            Expr::Call { callee, args } => {
                callee.contains_member_field(field)
                    || args.iter().any(|arg| arg.contains_member_field(field))
            }
            Expr::Try(inner) | Expr::Async(inner) | Expr::Await(inner) => inner.contains_member_field(field),
            Expr::Binary { left, right, .. } => {
                left.contains_member_field(field) || right.contains_member_field(field)
            }
        }
    }

    pub fn contains_call_path(&self, expected: &[&str]) -> bool {
        match self {
            Expr::Call { callee, args } => {
                callee.path().is_some_and(|path| path == expected)
                    || callee.contains_call_path(expected)
                    || args.iter().any(|arg| arg.contains_call_path(expected))
            }
            Expr::Member { object, .. } => object.contains_call_path(expected),
            Expr::Try(inner) | Expr::Async(inner) | Expr::Await(inner) => inner.contains_call_path(expected),
            Expr::Binary { left, right, .. } => {
                left.contains_call_path(expected) || right.contains_call_path(expected)
            }
            Expr::Object(fields) => fields
                .iter()
                .any(|field| field.value.contains_call_path(expected)),
            Expr::Ident(_) | Expr::String(_) | Expr::Bool(_) | Expr::Int(_) | Expr::Float(_) | Expr::Quantity(_, _) => {
                false
            }
        }
    }

    pub fn direct_call_name(&self) -> Option<&str> {
        match self {
            Expr::Call { callee, .. } => match callee.as_ref() {
                Expr::Ident(name) => Some(name.as_str()),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn path(&self) -> Option<Vec<&str>> {
        match self {
            Expr::Ident(name) => Some(vec![name.as_str()]),
            Expr::Member { object, field } => {
                let mut path = object.path()?;
                path.push(field.as_str());
                Some(path)
            }
            _ => None,
        }
    }

    pub fn calls(&self) -> Vec<CallRef<'_>> {
        let mut calls = Vec::new();
        self.collect_calls(&mut calls);
        calls
    }

    fn collect_calls<'a>(&'a self, calls: &mut Vec<CallRef<'a>>) {
        match self {
            Expr::Call { callee, args } => {
                if let Some(path) = callee.path() {
                    calls.push(CallRef { path, args });
                }
                callee.collect_calls(calls);
                for arg in args {
                    arg.collect_calls(calls);
                }
            }
            Expr::Member { object, .. } => object.collect_calls(calls),
            Expr::Try(inner) | Expr::Async(inner) | Expr::Await(inner) => inner.collect_calls(calls),
            Expr::Binary { left, right, .. } => {
                left.collect_calls(calls);
                right.collect_calls(calls);
            }
            Expr::Object(fields) => {
                for field in fields {
                    field.value.collect_calls(calls);
                }
            }
            Expr::Ident(_) | Expr::String(_) | Expr::Bool(_) | Expr::Int(_) | Expr::Float(_) | Expr::Quantity(_, _) => {}
        }
    }
}

#[derive(Debug, Clone)]
pub struct CallRef<'a> {
    pub path: Vec<&'a str>,
    pub args: &'a [Expr],
}

pub fn parse(text: &str) -> Result<Expr, ExprError> {
    Parser::new(text).parse()
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Ident(String),
    String(String),
    Number(String),
    Dot,
    Comma,
    Colon,
    LBrace,
    RBrace,
    LParen,
    RParen,
    OrOr,
    AndAnd,
    EqEq,
    BangEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    Plus,
    Minus,
    Star,
    Slash,
    Question,
    Invalid(String),
    Eof,
}

struct Parser {
    tokens: Vec<Token>,
    index: usize,
}

impl Parser {
    fn new(text: &str) -> Self {
        Self {
            tokens: lex(text),
            index: 0,
        }
    }

    fn parse(mut self) -> Result<Expr, ExprError> {
        let expr = self.expression()?;
        if !matches!(self.peek(), Token::Eof) {
            return Err(self.error("unexpected token after expression"));
        }
        Ok(expr)
    }

    fn expression(&mut self) -> Result<Expr, ExprError> {
        self.or()
    }

    fn or(&mut self) -> Result<Expr, ExprError> {
        let mut expr = self.and()?;
        while self.match_token(&Token::OrOr) {
            let right = self.and()?;
            expr = Expr::Binary {
                left: Box::new(expr),
                op: BinaryOp::Or,
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn and(&mut self) -> Result<Expr, ExprError> {
        let mut expr = self.equality()?;
        while self.match_token(&Token::AndAnd) {
            let right = self.equality()?;
            expr = Expr::Binary {
                left: Box::new(expr),
                op: BinaryOp::And,
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn equality(&mut self) -> Result<Expr, ExprError> {
        let mut expr = self.comparison()?;
        loop {
            let op = if self.match_token(&Token::EqEq) {
                BinaryOp::Equal
            } else if self.match_token(&Token::BangEq) {
                BinaryOp::NotEqual
            } else {
                break;
            };
            let right = self.comparison()?;
            expr = Expr::Binary {
                left: Box::new(expr),
                op,
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn comparison(&mut self) -> Result<Expr, ExprError> {
        let mut expr = self.additive()?;
        loop {
            let op = if self.match_token(&Token::Lt) {
                BinaryOp::LessThan
            } else if self.match_token(&Token::LtEq) {
                BinaryOp::LessThanOrEqual
            } else if self.match_token(&Token::Gt) {
                BinaryOp::GreaterThan
            } else if self.match_token(&Token::GtEq) {
                BinaryOp::GreaterThanOrEqual
            } else {
                break;
            };
            let right = self.additive()?;
            expr = Expr::Binary {
                left: Box::new(expr),
                op,
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn additive(&mut self) -> Result<Expr, ExprError> {
        let mut expr = self.multiplicative()?;
        loop {
            let op = if self.match_token(&Token::Plus) {
                BinaryOp::Add
            } else if self.match_token(&Token::Minus) {
                BinaryOp::Subtract
            } else {
                break;
            };
            let right = self.multiplicative()?;
            expr = Expr::Binary {
                left: Box::new(expr),
                op,
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn multiplicative(&mut self) -> Result<Expr, ExprError> {
        let mut expr = self.postfix()?;
        loop {
            let op = if self.match_token(&Token::Star) {
                BinaryOp::Multiply
            } else if self.match_token(&Token::Slash) {
                BinaryOp::Divide
            } else {
                break;
            };
            let right = self.postfix()?;
            expr = Expr::Binary {
                left: Box::new(expr),
                op,
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn postfix(&mut self) -> Result<Expr, ExprError> {
        let mut expr = self.primary()?;
        loop {
            if self.match_token(&Token::Dot) {
                let Token::Ident(field) = self.advance().clone() else {
                    return Err(self.error("expected member name after `.`"));
                };
                expr = Expr::Member {
                    object: Box::new(expr),
                    field,
                };
                continue;
            }

            if self.match_token(&Token::LParen) {
                let mut args = Vec::new();
                if !self.at(&Token::RParen) {
                    if self.at_named_argument_start() {
                        args.push(self.named_argument_object()?);
                    } else {
                        loop {
                            args.push(self.expression()?);
                            if !self.match_token(&Token::Comma) {
                                break;
                            }
                        }
                    }
                }
                self.expect(&Token::RParen, "expected `)` after call arguments")?;
                expr = Expr::Call {
                    callee: Box::new(expr),
                    args,
                };
                continue;
            }

            if self.match_token(&Token::Question) {
                expr = Expr::Try(Box::new(expr));
                continue;
            }

            break;
        }
        Ok(expr)
    }

    fn named_argument_object(&mut self) -> Result<Expr, ExprError> {
        let mut fields = Vec::new();
        loop {
            let name = match self.advance().clone() {
                Token::Ident(name) | Token::String(name) => name,
                _ => return Err(self.error("expected named argument name")),
            };
            self.expect(&Token::Colon, "expected `:` after named argument name")?;
            let value = self.expression()?;
            fields.push(ObjectField { name, value });

            if !self.match_token(&Token::Comma) {
                break;
            }
            if !self.at_named_argument_start() {
                return Err(self.error("named arguments cannot be mixed with positional arguments"));
            }
        }
        Ok(Expr::Object(fields))
    }

    fn primary(&mut self) -> Result<Expr, ExprError> {
        match self.advance().clone() {
            Token::Ident(name) if name == "true" => Ok(Expr::Bool(true)),
            Token::Ident(name) if name == "false" => Ok(Expr::Bool(false)),
            Token::Ident(name) if name == "async" => {
                let inner = self.postfix()?;
                Ok(Expr::Async(Box::new(inner)))
            }
            Token::Ident(name) if name == "await" => {
                let inner = self.postfix()?;
                Ok(Expr::Await(Box::new(inner)))
            }
            Token::Ident(name) => Ok(Expr::Ident(name)),
            Token::Invalid(value) => Err(self.error(format!("invalid token `{value}`"))),
            Token::String(value) => Ok(Expr::String(value)),
            Token::Number(value) => {
                if let Token::Ident(unit) = self.peek() {
                    let mut unit = unit.clone();
                    self.advance(); // consume unit identifier
                    if matches!(self.peek(), Token::Slash) && matches!(self.tokens.get(self.index + 1), Some(Token::Ident(_))) {
                        self.advance(); // consume Slash
                        if let Token::Ident(next_unit) = self.advance() {
                            unit = format!("{}/{}", unit, next_unit);
                        }
                    }
                    Ok(Expr::Quantity(value, unit))
                } else if value.contains('.') {
                    value
                        .parse::<f64>()
                        .map(Expr::Float)
                        .map_err(|_| self.error("invalid float literal"))
                } else {
                    value
                        .parse::<i64>()
                        .map(Expr::Int)
                        .map_err(|_| self.error("invalid integer literal"))
                }
            }
            Token::LParen => {
                let expr = self.expression()?;
                self.expect(&Token::RParen, "expected `)` after expression")?;
                Ok(expr)
            }
            Token::LBrace => self.object_literal(),
            _ => Err(self.error("expected expression")),
        }
    }

    fn object_literal(&mut self) -> Result<Expr, ExprError> {
        let mut fields = Vec::new();
        if self.match_token(&Token::RBrace) {
            return Ok(Expr::Object(fields));
        }

        loop {
            let name = match self.advance().clone() {
                Token::Ident(name) | Token::String(name) => name,
                _ => return Err(self.error("expected object field name")),
            };
            self.expect(&Token::Colon, "expected `:` after object field name")?;
            let value = self.expression()?;
            fields.push(ObjectField { name, value });

            if !self.match_token(&Token::Comma) {
                break;
            }
            if self.at(&Token::RBrace) {
                break;
            }
        }

        self.expect(&Token::RBrace, "expected `}` after object literal")?;
        Ok(Expr::Object(fields))
    }

    fn expect(&mut self, token: &Token, message: &str) -> Result<(), ExprError> {
        if self.match_token(token) {
            Ok(())
        } else {
            Err(self.error(message))
        }
    }

    fn match_token(&mut self, token: &Token) -> bool {
        if self.at(token) {
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn at(&self, token: &Token) -> bool {
        std::mem::discriminant(self.peek()) == std::mem::discriminant(token)
    }

    fn at_named_argument_start(&self) -> bool {
        matches!(self.peek(), Token::Ident(_) | Token::String(_))
            && matches!(self.tokens.get(self.index + 1), Some(Token::Colon))
    }

    fn advance(&mut self) -> &Token {
        if self.index < self.tokens.len() {
            self.index += 1;
        }
        self.previous()
    }

    fn previous(&self) -> &Token {
        &self.tokens[self.index.saturating_sub(1)]
    }

    fn peek(&self) -> &Token {
        self.tokens.get(self.index).unwrap_or(&Token::Eof)
    }

    fn error(&self, message: impl Into<String>) -> ExprError {
        ExprError {
            message: message.into(),
        }
    }
}

fn lex(text: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = text.char_indices().peekable();

    while let Some((start, ch)) = chars.next() {
        match ch {
            ch if ch.is_whitespace() => {}
            '.' => tokens.push(Token::Dot),
            ',' => tokens.push(Token::Comma),
            ':' => tokens.push(Token::Colon),
            '{' => tokens.push(Token::LBrace),
            '}' => tokens.push(Token::RBrace),
            '(' => tokens.push(Token::LParen),
            ')' => tokens.push(Token::RParen),
            '+' => tokens.push(Token::Plus),
            '-' => tokens.push(Token::Minus),
            '*' => tokens.push(Token::Star),
            '/' => tokens.push(Token::Slash),
            '?' => tokens.push(Token::Question),
            '|' => {
                if chars.peek().copied().is_some_and(|(_, ch)| ch == '|') {
                    chars.next();
                    tokens.push(Token::OrOr);
                } else {
                    tokens.push(Token::Invalid("|".to_string()));
                }
            }
            '&' => {
                if chars.peek().copied().is_some_and(|(_, ch)| ch == '&') {
                    chars.next();
                    tokens.push(Token::AndAnd);
                } else {
                    tokens.push(Token::Invalid("&".to_string()));
                }
            }
            '=' => {
                if chars.peek().copied().is_some_and(|(_, ch)| ch == '=') {
                    chars.next();
                    tokens.push(Token::EqEq);
                } else {
                    tokens.push(Token::Invalid("=".to_string()));
                }
            }
            '!' => {
                if chars.peek().copied().is_some_and(|(_, ch)| ch == '=') {
                    chars.next();
                    tokens.push(Token::BangEq);
                } else {
                    tokens.push(Token::Invalid("!".to_string()));
                }
            }
            '<' => {
                if chars.peek().copied().is_some_and(|(_, ch)| ch == '=') {
                    chars.next();
                    tokens.push(Token::LtEq);
                } else {
                    tokens.push(Token::Lt);
                }
            }
            '>' => {
                if chars.peek().copied().is_some_and(|(_, ch)| ch == '=') {
                    chars.next();
                    tokens.push(Token::GtEq);
                } else {
                    tokens.push(Token::Gt);
                }
            }
            '"' => {
                let mut value = String::new();
                let mut escaped = false;
                for (_, ch) in chars.by_ref() {
                    if escaped {
                        value.push(ch);
                        escaped = false;
                    } else if ch == '\\' {
                        escaped = true;
                    } else if ch == '"' {
                        break;
                    } else {
                        value.push(ch);
                    }
                }
                tokens.push(Token::String(value));
            }
            ch if ch.is_ascii_digit() => {
                let mut end = start + ch.len_utf8();
                while let Some((next_index, next_ch)) = chars.peek().copied() {
                    if next_ch.is_ascii_digit() || next_ch == '.' {
                        chars.next();
                        end = next_index + next_ch.len_utf8();
                    } else {
                        break;
                    }
                }
                tokens.push(Token::Number(text[start..end].to_string()));
            }
            ch if ch.is_ascii_alphabetic() || ch == '_' => {
                let mut end = start + ch.len_utf8();
                while let Some((next_index, next_ch)) = chars.peek().copied() {
                    if next_ch.is_ascii_alphanumeric() || next_ch == '_' {
                        chars.next();
                        end = next_index + next_ch.len_utf8();
                    } else {
                        break;
                    }
                }
                tokens.push(Token::Ident(text[start..end].to_string()));
            }
            _ => tokens.push(Token::Invalid(ch.to_string())),
        }
    }

    tokens.push(Token::Eof);
    tokens
}

#[cfg(test)]
mod tests {
    use super::{parse, BinaryOp, Expr};

    #[test]
    fn parses_connector_call_with_member_argument() {
        let expr = parse("payment_gateway . refund ( payment . id , amount )").unwrap();
        let calls = expr.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].path, vec!["payment_gateway", "refund"]);
        assert_eq!(calls[0].args.len(), 2);
    }

    #[test]
    fn parses_confidence_comparison() {
        let expr = parse("risk.confidence < 0.85").unwrap();
        assert!(matches!(
            expr,
            Expr::Binary {
                op: BinaryOp::LessThan,
                ..
            }
        ));
    }

    #[test]
    fn parses_boolean_expression_precedence() {
        let expr = parse("risk.confidence >= 0.85 && approved == true").unwrap();
        assert!(matches!(
            expr,
            Expr::Binary {
                op: BinaryOp::And,
                ..
            }
        ));
    }

    #[test]
    fn parses_arithmetic_precedence() {
        let expr = parse("1 + 2 * 3").unwrap();
        assert!(matches!(
            expr,
            Expr::Binary {
                op: BinaryOp::Add,
                ..
            }
        ));
    }

    #[test]
    fn parses_result_try_postfix() {
        let expr = parse("users.find(id)?").unwrap();
        assert!(matches!(expr, Expr::Try(_)));
        assert_eq!(expr.calls()[0].path, vec!["users", "find"]);
    }

    #[test]
    fn parses_named_call_arguments_as_object_payload() {
        let expr =
            parse(r#"require_human_approval(action: "issue_refund", reason: "low")"#).unwrap();
        let Expr::Call { args, .. } = expr else {
            panic!("expected call expression");
        };
        assert_eq!(args.len(), 1);
        let Expr::Object(fields) = &args[0] else {
            panic!("expected named arguments to become object payload");
        };
        assert_eq!(fields[0].name, "action");
        assert_eq!(fields[1].name, "reason");
    }

    #[test]
    fn rejects_invalid_operator_token() {
        let error = parse("approved = true").unwrap_err();
        assert!(error.message.contains("unexpected token"));
    }

    #[test]
    fn parses_quantity_literals() {
        let expr = parse("10 km").unwrap();
        assert_eq!(expr, Expr::Quantity("10".to_string(), "km".to_string()));
        let expr = parse("15000 KZT").unwrap();
        assert_eq!(expr, Expr::Quantity("15000".to_string(), "KZT".to_string()));
    }

    #[test]
    fn parses_async_await() {
        let expr = parse("async fetch(id)").unwrap();
        assert!(matches!(expr, Expr::Async(_)));
        let expr = parse("await task").unwrap();
        assert!(matches!(expr, Expr::Await(_)));
    }
}
