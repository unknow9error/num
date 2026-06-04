use crate::diagnostic::Diagnostic;
use crate::span::Span;
use crate::token::{keyword, Symbol, Token, TokenKind};

#[derive(Debug, Clone)]
pub struct Lexed {
    pub tokens: Vec<Token>,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn lex(source_name: &str, source: &str) -> Lexed {
    Lexer::new(source_name, source).lex()
}

struct Lexer<'a> {
    source_name: &'a str,
    source: &'a str,
    chars: Vec<char>,
    index: usize,
    offset: usize,
    line: usize,
    column: usize,
    tokens: Vec<Token>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Lexer<'a> {
    fn new(source_name: &'a str, source: &'a str) -> Self {
        Self {
            source_name,
            source,
            chars: source.chars().collect(),
            index: 0,
            offset: 0,
            line: 1,
            column: 1,
            tokens: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    fn lex(mut self) -> Lexed {
        while let Some(ch) = self.peek() {
            match ch {
                ' ' | '\t' | '\r' => {
                    self.advance();
                }
                '\n' => self.newline(),
                '/' if self.peek_next() == Some('/') => self.line_comment(),
                '"' => self.string(),
                '0'..='9' => self.number(),
                'a'..='z' | 'A'..='Z' | '_' => self.ident(),
                _ => self.symbol(),
            }
        }

        self.tokens.push(Token::eof(
            self.source_name,
            self.source.len(),
            self.line,
            self.column,
        ));

        Lexed {
            tokens: self.tokens,
            diagnostics: self.diagnostics,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.index).copied()
    }

    fn peek_next(&self) -> Option<char> {
        self.chars.get(self.index + 1).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.index += 1;
        self.offset += ch.len_utf8();
        self.column += 1;
        Some(ch)
    }

    fn span(&self, start: usize, end: usize, line: usize, column: usize) -> Span {
        Span::new(self.source_name, start, end, line, column)
    }

    fn push(&mut self, kind: TokenKind, lexeme: String, start: usize, line: usize, column: usize) {
        self.tokens.push(Token {
            kind,
            lexeme,
            span: self.span(start, self.offset, line, column),
        });
    }

    fn newline(&mut self) {
        let start = self.offset;
        let line = self.line;
        let column = self.column;
        self.advance();
        self.tokens.push(Token {
            kind: TokenKind::Newline,
            lexeme: "\n".to_string(),
            span: self.span(start, self.offset, line, column),
        });
        self.line += 1;
        self.column = 1;
    }

    fn line_comment(&mut self) {
        while let Some(ch) = self.peek() {
            if ch == '\n' {
                break;
            }
            self.advance();
        }
    }

    fn ident(&mut self) {
        let start = self.offset;
        let line = self.line;
        let column = self.column;

        while matches!(
            self.peek(),
            Some('a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-')
        ) {
            self.advance();
        }

        let text = self.source[start..self.offset].to_string();
        let kind = keyword(&text)
            .map(TokenKind::Keyword)
            .unwrap_or_else(|| TokenKind::Ident(text.clone()));
        self.push(kind, text, start, line, column);
    }

    fn number(&mut self) {
        let start = self.offset;
        let line = self.line;
        let column = self.column;

        while matches!(self.peek(), Some('0'..='9' | '.')) {
            self.advance();
        }

        let text = self.source[start..self.offset].to_string();
        self.push(TokenKind::Number(text.clone()), text, start, line, column);
    }

    fn string(&mut self) {
        let start = self.offset;
        let line = self.line;
        let column = self.column;
        self.advance();
        let mut value = String::new();

        while let Some(ch) = self.peek() {
            if ch == '"' {
                self.advance();
                let lexeme = self.source[start..self.offset].to_string();
                self.push(TokenKind::String(value), lexeme, start, line, column);
                return;
            }

            if ch == '\\' {
                self.advance();
                match self.peek() {
                    Some('n') => {
                        value.push('\n');
                        self.advance();
                    }
                    Some('t') => {
                        value.push('\t');
                        self.advance();
                    }
                    Some('"') => {
                        value.push('"');
                        self.advance();
                    }
                    Some(other) => {
                        value.push(other);
                        self.advance();
                    }
                    None => break,
                }
                continue;
            }

            value.push(ch);
            self.advance();
        }

        self.diagnostics.push(
            Diagnostic::error(
                "N0001",
                "unterminated string literal",
                self.span(start, self.offset, line, column),
            )
            .with_help("close the string with a double quote"),
        );
    }

    fn symbol(&mut self) {
        let start = self.offset;
        let line = self.line;
        let column = self.column;
        let Some(ch) = self.advance() else { return };

        let symbol = match ch {
            '-' if self.peek() == Some('>') => {
                self.advance();
                Symbol::Arrow
            }
            '=' if self.peek() == Some('>') => {
                self.advance();
                Symbol::FatArrow
            }
            '=' if self.peek() == Some('=') => {
                self.advance();
                Symbol::EqEq
            }
            '!' if self.peek() == Some('=') => {
                self.advance();
                Symbol::BangEq
            }
            ':' => Symbol::Colon,
            ',' => Symbol::Comma,
            '.' => Symbol::Dot,
            '=' => Symbol::Eq,
            '{' => Symbol::LBrace,
            '}' => Symbol::RBrace,
            '(' => Symbol::LParen,
            ')' => Symbol::RParen,
            '<' if self.peek() == Some('=') => {
                self.advance();
                Symbol::LtEq
            }
            '<' => Symbol::Lt,
            '>' if self.peek() == Some('=') => {
                self.advance();
                Symbol::GtEq
            }
            '>' => Symbol::Gt,
            '|' if self.peek() == Some('|') => {
                self.advance();
                Symbol::PipePipe
            }
            '|' => Symbol::Pipe,
            '&' if self.peek() == Some('&') => {
                self.advance();
                Symbol::AmpAmp
            }
            '+' => Symbol::Plus,
            '-' => Symbol::Minus,
            '*' => Symbol::Star,
            '/' => Symbol::Slash,
            '?' => Symbol::Question,
            _ => {
                self.diagnostics.push(
                    Diagnostic::error(
                        "N0002",
                        format!("unexpected character `{ch}`"),
                        self.span(start, self.offset, line, column),
                    )
                    .with_help("remove the character or escape it inside a string literal"),
                );
                return;
            }
        };

        let lexeme = self.source[start..self.offset].to_string();
        self.push(TokenKind::Symbol(symbol), lexeme, start, line, column);
    }
}
