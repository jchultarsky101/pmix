//! Byte-level tokenizer for Part 21 text.

use super::value::{Id, Span};

/// A lexical token. Borrowed slices point into the source.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Token<'a> {
    /// Entity, section, or defined-type keyword (`CARTESIAN_POINT`, `DATA`,
    /// `END-ISO-10303-21`, user-defined `!FOO`). Case as in the source.
    Keyword(&'a [u8]),
    Integer(i64),
    Real(f64),
    /// Raw literal content between the quotes, escapes not yet decoded.
    String(&'a [u8]),
    /// Enumeration name without the surrounding dots.
    Enumeration(&'a [u8]),
    Reference(Id),
    /// Raw hex text between the double quotes.
    Binary(&'a [u8]),
    LParen,
    RParen,
    Comma,
    Semicolon,
    Equals,
    Dollar,
    Star,
}

/// A lexical error at a given location.
#[derive(Debug, Clone, PartialEq)]
pub struct LexError {
    pub span: Span,
    pub message: String,
}

pub struct Lexer<'a> {
    src: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a [u8]) -> Self {
        Self { src, pos: 0 }
    }

    /// Current byte offset.
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Skip a UTF-8 byte order mark at the current position, if present.
    pub fn skip_bom(&mut self) {
        if self.src[self.pos..].starts_with(b"\xEF\xBB\xBF") {
            self.pos += 3;
        }
    }

    /// Skip forward to just past the next `;` (or to end of input). Used
    /// for error recovery: after a bad record, resume at the next one.
    pub fn skip_past_semicolon(&mut self) {
        while self.pos < self.src.len() {
            let b = self.src[self.pos];
            self.pos += 1;
            match b {
                b';' => return,
                // Do not stop on a ';' inside a string literal.
                b'\'' => {
                    while self.pos < self.src.len() {
                        if self.src[self.pos] == b'\'' {
                            self.pos += 1;
                            if self.src.get(self.pos) == Some(&b'\'') {
                                self.pos += 1;
                                continue;
                            }
                            break;
                        }
                        self.pos += 1;
                    }
                }
                _ => {}
            }
        }
    }

    fn skip_whitespace_and_comments(&mut self) -> Result<(), LexError> {
        loop {
            while self.pos < self.src.len() && self.src[self.pos].is_ascii_whitespace() {
                self.pos += 1;
            }
            if self.src[self.pos..].starts_with(b"/*") {
                let start = self.pos;
                match self.src[self.pos + 2..].windows(2).position(|w| w == b"*/") {
                    Some(n) => self.pos += 2 + n + 2,
                    None => {
                        self.pos = self.src.len();
                        return Err(LexError {
                            span: Span {
                                start,
                                end: self.pos,
                            },
                            message: "unterminated comment".into(),
                        });
                    }
                }
                continue;
            }
            return Ok(());
        }
    }

    /// Next token with its span, or `None` at end of input.
    pub fn next_token(&mut self) -> Result<Option<(Token<'a>, Span)>, LexError> {
        self.skip_whitespace_and_comments()?;
        if self.pos >= self.src.len() {
            return Ok(None);
        }
        let start = self.pos;
        let b = self.src[self.pos];
        let simple = |this: &mut Self, tok| {
            this.pos += 1;
            Ok(Some((
                tok,
                Span {
                    start,
                    end: start + 1,
                },
            )))
        };
        match b {
            b'(' => simple(self, Token::LParen),
            b')' => simple(self, Token::RParen),
            b',' => simple(self, Token::Comma),
            b';' => simple(self, Token::Semicolon),
            b'=' => simple(self, Token::Equals),
            b'$' => simple(self, Token::Dollar),
            b'*' => simple(self, Token::Star),
            b'\'' => self.string(start),
            b'"' => self.binary(start),
            b'.' => self.enumeration(start),
            b'#' => self.reference(start),
            b'+' | b'-' | b'0'..=b'9' => self.number(start),
            b'A'..=b'Z' | b'a'..=b'z' | b'_' | b'!' => self.keyword(start),
            _ => {
                self.pos += 1;
                Err(LexError {
                    span: Span {
                        start,
                        end: self.pos,
                    },
                    message: format!("unexpected byte 0x{b:02X}"),
                })
            }
        }
    }

    fn string(&mut self, start: usize) -> Result<Option<(Token<'a>, Span)>, LexError> {
        // self.src[start] == b'\''
        let mut i = start + 1;
        loop {
            match self.src.get(i) {
                None => {
                    self.pos = self.src.len();
                    return Err(LexError {
                        span: Span {
                            start,
                            end: self.pos,
                        },
                        message: "unterminated string literal".into(),
                    });
                }
                Some(b'\'') => {
                    if self.src.get(i + 1) == Some(&b'\'') {
                        i += 2;
                        continue;
                    }
                    let content = &self.src[start + 1..i];
                    self.pos = i + 1;
                    return Ok(Some((
                        Token::String(content),
                        Span {
                            start,
                            end: self.pos,
                        },
                    )));
                }
                Some(_) => i += 1,
            }
        }
    }

    fn binary(&mut self, start: usize) -> Result<Option<(Token<'a>, Span)>, LexError> {
        let mut i = start + 1;
        while i < self.src.len() && self.src[i] != b'"' {
            i += 1;
        }
        if i >= self.src.len() {
            self.pos = self.src.len();
            return Err(LexError {
                span: Span {
                    start,
                    end: self.pos,
                },
                message: "unterminated binary literal".into(),
            });
        }
        let content = &self.src[start + 1..i];
        self.pos = i + 1;
        Ok(Some((
            Token::Binary(content),
            Span {
                start,
                end: self.pos,
            },
        )))
    }

    fn enumeration(&mut self, start: usize) -> Result<Option<(Token<'a>, Span)>, LexError> {
        let mut i = start + 1;
        while i < self.src.len() && (self.src[i].is_ascii_alphanumeric() || self.src[i] == b'_') {
            i += 1;
        }
        if self.src.get(i) != Some(&b'.') || i == start + 1 {
            self.pos = i.max(start + 1);
            return Err(LexError {
                span: Span {
                    start,
                    end: self.pos,
                },
                message: "malformed enumeration literal".into(),
            });
        }
        let name = &self.src[start + 1..i];
        self.pos = i + 1;
        Ok(Some((
            Token::Enumeration(name),
            Span {
                start,
                end: self.pos,
            },
        )))
    }

    fn reference(&mut self, start: usize) -> Result<Option<(Token<'a>, Span)>, LexError> {
        let mut i = start + 1;
        let mut value: Id = 0;
        let mut overflow = false;
        while i < self.src.len() && self.src[i].is_ascii_digit() {
            let d = Id::from(self.src[i] - b'0');
            match value.checked_mul(10).and_then(|v| v.checked_add(d)) {
                Some(v) => value = v,
                None => overflow = true,
            }
            i += 1;
        }
        self.pos = i;
        if i == start + 1 || overflow {
            return Err(LexError {
                span: Span { start, end: i },
                message: if overflow {
                    "instance name out of range".into()
                } else {
                    "malformed instance reference".into()
                },
            });
        }
        Ok(Some((Token::Reference(value), Span { start, end: i })))
    }

    fn number(&mut self, start: usize) -> Result<Option<(Token<'a>, Span)>, LexError> {
        let s = self.src;
        let mut i = start;
        if matches!(s[i], b'+' | b'-') {
            i += 1;
        }
        let int_start = i;
        while i < s.len() && s[i].is_ascii_digit() {
            i += 1;
        }
        let mut is_real = false;
        if i < s.len() && s[i] == b'.' {
            is_real = true;
            i += 1;
            while i < s.len() && s[i].is_ascii_digit() {
                i += 1;
            }
        }
        if i < s.len() && matches!(s[i], b'E' | b'e') {
            let mut j = i + 1;
            if j < s.len() && matches!(s[j], b'+' | b'-') {
                j += 1;
            }
            if j < s.len() && s[j].is_ascii_digit() {
                is_real = true;
                while j < s.len() && s[j].is_ascii_digit() {
                    j += 1;
                }
                i = j;
            }
        }
        self.pos = i;
        let span = Span { start, end: i };
        if int_start == i || (i == int_start + 1 && !s[int_start].is_ascii_digit()) {
            return Err(LexError {
                span,
                message: "malformed number".into(),
            });
        }
        // The slice is ASCII by construction.
        let text = std::str::from_utf8(&s[start..i]).unwrap();
        if !is_real {
            if let Ok(v) = text.parse::<i64>() {
                return Ok(Some((Token::Integer(v), span)));
            }
            // Fall through: integer too large, treat as real.
        }
        // Rust's float parser rejects "1." and "1.E5"; normalise those.
        let mut norm = String::with_capacity(text.len() + 1);
        let bytes = text.as_bytes();
        for (k, &c) in bytes.iter().enumerate() {
            norm.push(c as char);
            if c == b'.' && !bytes.get(k + 1).is_some_and(u8::is_ascii_digit) {
                norm.push('0');
            }
        }
        match norm.parse::<f64>() {
            Ok(v) => Ok(Some((Token::Real(v), span))),
            Err(_) => Err(LexError {
                span,
                message: "malformed number".into(),
            }),
        }
    }

    fn keyword(&mut self, start: usize) -> Result<Option<(Token<'a>, Span)>, LexError> {
        let mut i = start + 1;
        // Section markers contain hyphens (`END-ISO-10303-21`); entity
        // keywords are letters, digits and underscores.
        while i < self.src.len()
            && (self.src[i].is_ascii_alphanumeric() || matches!(self.src[i], b'_' | b'-'))
        {
            i += 1;
        }
        self.pos = i;
        Ok(Some((
            Token::Keyword(&self.src[start..i]),
            Span { start, end: i },
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(src: &str) -> Vec<Token<'_>> {
        let mut lx = Lexer::new(src.as_bytes());
        let mut out = Vec::new();
        while let Some((t, _)) = lx.next_token().unwrap() {
            out.push(t);
        }
        out
    }

    #[test]
    fn lexes_a_record() {
        let t = tokens("#10=CARTESIAN_POINT('p',(0.,-1.5E-3,2));");
        assert_eq!(
            t,
            vec![
                Token::Reference(10),
                Token::Equals,
                Token::Keyword(b"CARTESIAN_POINT"),
                Token::LParen,
                Token::String(b"p"),
                Token::Comma,
                Token::LParen,
                Token::Real(0.0),
                Token::Comma,
                Token::Real(-0.0015),
                Token::Comma,
                Token::Integer(2),
                Token::RParen,
                Token::RParen,
                Token::Semicolon,
            ]
        );
    }

    #[test]
    fn lexes_enums_unset_derived_typed_binary() {
        let t = tokens("(.T., $, *, LENGTH_MEASURE(1.), \"0F\")");
        assert_eq!(
            t,
            vec![
                Token::LParen,
                Token::Enumeration(b"T"),
                Token::Comma,
                Token::Dollar,
                Token::Comma,
                Token::Star,
                Token::Comma,
                Token::Keyword(b"LENGTH_MEASURE"),
                Token::LParen,
                Token::Real(1.0),
                Token::RParen,
                Token::Comma,
                Token::Binary(b"0F"),
                Token::RParen,
            ]
        );
    }

    #[test]
    fn strings_keep_doubled_quotes_and_semicolons() {
        let t = tokens("'it''s; here'");
        assert_eq!(t, vec![Token::String(b"it''s; here")]);
    }

    #[test]
    fn skips_comments_and_whitespace() {
        let t = tokens("  /* c */ DATA /* x */ ;\r\n");
        assert_eq!(t, vec![Token::Keyword(b"DATA"), Token::Semicolon]);
    }

    #[test]
    fn number_edge_cases() {
        assert_eq!(tokens("1.E5"), vec![Token::Real(1.0e5)]);
        assert_eq!(tokens("-7"), vec![Token::Integer(-7)]);
        assert_eq!(tokens("99999999999999999999"), vec![Token::Real(1e20)]);
    }

    #[test]
    fn reports_errors_with_spans() {
        let mut lx = Lexer::new(b"  @");
        let err = lx.next_token().unwrap_err();
        assert_eq!(err.span, Span { start: 2, end: 3 });
        let mut lx = Lexer::new(b"'open");
        assert!(lx.next_token().is_err());
    }

    #[test]
    fn skip_past_semicolon_ignores_strings() {
        let mut lx = Lexer::new(b"FOO('a;b') ; BAR;");
        lx.skip_past_semicolon();
        assert_eq!(lx.position(), 12);
    }
}
