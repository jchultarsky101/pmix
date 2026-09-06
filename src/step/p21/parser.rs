//! Recursive-descent parser over the token stream, with error recovery.

use std::collections::BTreeMap;
use std::fmt;

use super::exchange::{Diagnostic, Exchange, Header, Severity};
use super::lexer::{LexError, Lexer, Token};
use super::string;
use super::value::{Id, Instance, Parameter, Segment, Span};

/// A fatal parse failure: the input is not a Part 21 file at all.
///
/// Recoverable problems inside an otherwise valid file are reported as
/// [`Diagnostic`]s on the returned [`Exchange`] instead.
#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
    pub line: usize,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ParseError {}

/// Parse Part 21 text.
pub fn parse(src: &str) -> Result<Exchange, ParseError> {
    parse_bytes(src.as_bytes())
}

/// Parse Part 21 bytes. The input is treated as ASCII with the escapes of
/// ISO 10303-21; raw UTF-8 inside string literals is tolerated.
pub fn parse_bytes(src: &[u8]) -> Result<Exchange, ParseError> {
    Parser::new(src).parse_file()
}

/// Maximum number of diagnostics kept, to bound memory on garbage input.
const MAX_DIAGNOSTICS: usize = 10_000;

struct Parser<'a> {
    src: &'a [u8],
    lexer: Lexer<'a>,
    peeked: Option<(Token<'a>, Span)>,
    /// Start offsets of each line, for line-number lookup.
    line_starts: Vec<usize>,
    diagnostics: Vec<Diagnostic>,
    suppressed: usize,
}

/// Internal error while parsing one record; drives recovery.
enum RecordError {
    Lex(LexError),
    Syntax { message: String, span: Span },
    Eof,
}

impl From<LexError> for RecordError {
    fn from(e: LexError) -> Self {
        Self::Lex(e)
    }
}

type RResult<T> = Result<T, RecordError>;

impl<'a> Parser<'a> {
    fn new(src: &'a [u8]) -> Self {
        let mut line_starts = vec![0];
        line_starts.extend(
            src.iter()
                .enumerate()
                .filter(|(_, b)| **b == b'\n')
                .map(|(i, _)| i + 1),
        );
        Self {
            src,
            lexer: Lexer::new(src),
            peeked: None,
            line_starts,
            diagnostics: Vec::new(),
            suppressed: 0,
        }
    }

    fn line_of(&self, offset: usize) -> usize {
        self.line_starts.partition_point(|&s| s <= offset)
    }

    fn diag(&mut self, severity: Severity, message: impl Into<String>, span: Span) {
        if self.diagnostics.len() >= MAX_DIAGNOSTICS {
            self.suppressed += 1;
            return;
        }
        self.diagnostics.push(Diagnostic {
            severity,
            message: message.into(),
            span,
            line: self.line_of(span.start),
        });
    }

    fn fatal(&self, message: impl Into<String>, span: Span) -> ParseError {
        ParseError {
            message: message.into(),
            span,
            line: self.line_of(span.start),
        }
    }

    // ----- token access -------------------------------------------------

    fn peek(&mut self) -> RResult<Option<&(Token<'a>, Span)>> {
        if self.peeked.is_none() {
            self.peeked = self.lexer.next_token()?;
        }
        Ok(self.peeked.as_ref())
    }

    fn next(&mut self) -> RResult<(Token<'a>, Span)> {
        if let Some(t) = self.peeked.take() {
            return Ok(t);
        }
        self.lexer.next_token()?.ok_or(RecordError::Eof)
    }

    fn expect(&mut self, want: Token<'static>, what: &str) -> RResult<Span> {
        let (tok, span) = self.next()?;
        if tok == want {
            Ok(span)
        } else {
            Err(RecordError::Syntax {
                message: format!("expected {what}, found {}", describe(&tok)),
                span,
            })
        }
    }

    fn expect_keyword(&mut self, want: &str) -> RResult<Span> {
        let (tok, span) = self.next()?;
        match tok {
            Token::Keyword(k) if k.eq_ignore_ascii_case(want.as_bytes()) => Ok(span),
            other => Err(RecordError::Syntax {
                message: format!("expected `{want}`, found {}", describe(&other)),
                span,
            }),
        }
    }

    fn is_keyword(tok: &Token<'_>, want: &str) -> bool {
        matches!(tok, Token::Keyword(k) if k.eq_ignore_ascii_case(want.as_bytes()))
    }

    /// Recover after a bad record: drop the lookahead and skip past the next
    /// `;`, then report. Returns `Ok(false)` when the input is exhausted, so
    /// callers can stop looping.
    fn recover(&mut self, err: RecordError, context: &str) -> Result<bool, ParseError> {
        let (message, span) = match err {
            RecordError::Lex(e) => (e.message, e.span),
            RecordError::Syntax { message, span } => (message, span),
            RecordError::Eof => {
                let at = self.src.len();
                self.peeked = None;
                self.diag(
                    Severity::Error,
                    format!("unexpected end of file in {context}"),
                    Span { start: at, end: at },
                );
                return Ok(false);
            }
        };
        // If the offending token was the terminating `;` itself, the lexer
        // is already past it.
        let already_past = matches!(self.peeked, Some((Token::Semicolon, _)));
        self.peeked = None;
        if !already_past && self.src.get(span.start) != Some(&b';') {
            self.lexer.skip_past_semicolon();
        }
        let end = self.lexer.position();
        self.diag(
            Severity::Error,
            format!("{message}; skipped {context}"),
            Span {
                start: span.start,
                end: end.max(span.end),
            },
        );
        Ok(end < self.src.len())
    }

    // ----- grammar ------------------------------------------------------

    fn parse_file(mut self) -> Result<Exchange, ParseError> {
        // Optional UTF-8 byte order mark; offsets stay file-relative.
        self.lexer.skip_bom();

        match self.expect_keyword("ISO-10303-21") {
            Ok(_) => {}
            Err(_) => {
                let at = self.lexer.position();
                return Err(self.fatal(
                    "not a STEP Part 21 file: missing `ISO-10303-21;` marker",
                    Span {
                        start: 0,
                        end: at.min(self.src.len()),
                    },
                ));
            }
        }
        if let Err(e) = self.expect(Token::Semicolon, "`;`") {
            self.recover(e, "file marker")?;
        }

        let mut header = Header::default();
        let mut instances: BTreeMap<Id, Instance> = BTreeMap::new();
        let mut saw_header = false;
        let mut saw_data = false;

        loop {
            let (tok, span) = match self.next() {
                Ok(t) => t,
                Err(RecordError::Eof) => {
                    let at = self.src.len();
                    self.diag(
                        Severity::Warning,
                        "missing `END-ISO-10303-21;` at end of file",
                        Span { start: at, end: at },
                    );
                    break;
                }
                Err(e) => {
                    if !self.recover(e, "section")? {
                        break;
                    }
                    continue;
                }
            };
            match tok {
                Token::Keyword(k) if k.eq_ignore_ascii_case(b"HEADER") => {
                    self.expect_or_recover(Token::Semicolon, "`;`", "HEADER")?;
                    if saw_header {
                        self.diag(Severity::Warning, "duplicate HEADER section", span);
                    }
                    saw_header = true;
                    self.parse_header(&mut header)?;
                }
                Token::Keyword(k) if k.eq_ignore_ascii_case(b"DATA") => {
                    if let Err(e) = self.parse_data_section_start() {
                        self.recover(e, "DATA section start")?;
                    }
                    saw_data = true;
                    self.parse_data(&mut instances)?;
                }
                Token::Keyword(k) if k.eq_ignore_ascii_case(b"END-ISO-10303-21") => {
                    if let Err(e) = self.expect(Token::Semicolon, "`;`") {
                        self.recover(e, "end marker")?;
                    }
                    // Anything after the end marker is ignored, but noted.
                    if let Ok(Some((_, s))) = self.peek() {
                        let s = *s;
                        self.diag(
                            Severity::Warning,
                            "content after `END-ISO-10303-21;` ignored",
                            s,
                        );
                    }
                    break;
                }
                Token::Keyword(k)
                    if k.eq_ignore_ascii_case(b"ANCHOR")
                        || k.eq_ignore_ascii_case(b"REFERENCE")
                        || k.eq_ignore_ascii_case(b"SIGNATURE") =>
                {
                    let name = String::from_utf8_lossy(k).to_ascii_uppercase();
                    self.diag(
                        Severity::Warning,
                        format!(
                            "{name} section (ISO 10303-21 edition 3) is not interpreted; skipped"
                        ),
                        span,
                    );
                    self.skip_section()?;
                }
                other => {
                    let e = RecordError::Syntax {
                        message: format!("expected a section keyword, found {}", describe(&other)),
                        span,
                    };
                    if !self.recover(e, "section")? {
                        break;
                    }
                }
            }
        }

        if !saw_header {
            self.diag(Severity::Warning, "no HEADER section", Span::default());
        }
        if !saw_data {
            self.diag(Severity::Warning, "no DATA section", Span::default());
        }
        if self.suppressed > 0 {
            let n = self.suppressed;
            let at = self.src.len();
            self.diagnostics.push(Diagnostic {
                severity: Severity::Warning,
                message: format!("{n} further diagnostics suppressed"),
                span: Span { start: at, end: at },
                line: self.line_of(at),
            });
        }

        self.check_references(&instances);
        Ok(Exchange::new(header, instances, self.diagnostics))
    }

    /// After the `DATA` keyword: optional edition-3 parameters
    /// `('name', ('schema'))`, then `;`.
    fn parse_data_section_start(&mut self) -> RResult<()> {
        if matches!(self.peek()?, Some((Token::LParen, _))) {
            self.next()?;
            self.parse_parameter_list_body()?;
        }
        self.expect(Token::Semicolon, "`;`")?;
        Ok(())
    }

    fn expect_or_recover(
        &mut self,
        want: Token<'static>,
        what: &str,
        context: &str,
    ) -> Result<(), ParseError> {
        if let Err(e) = self.expect(want, what) {
            self.recover(e, context)?;
        }
        Ok(())
    }

    /// Skip tokens until just after `ENDSEC;`.
    fn skip_section(&mut self) -> Result<(), ParseError> {
        loop {
            match self.next() {
                Ok((tok, _)) if Self::is_keyword(&tok, "ENDSEC") => {
                    self.expect_or_recover(Token::Semicolon, "`;`", "ENDSEC")?;
                    return Ok(());
                }
                Ok(_) => {}
                Err(e) => {
                    if !self.recover(e, "skipped section")? {
                        return Ok(());
                    }
                }
            }
        }
    }

    fn parse_header(&mut self, header: &mut Header) -> Result<(), ParseError> {
        loop {
            let (tok, span) = match self.next() {
                Ok(t) => t,
                Err(e) => {
                    if !self.recover(e, "header record")? {
                        return Ok(());
                    }
                    continue;
                }
            };
            match tok {
                Token::Keyword(k) if k.eq_ignore_ascii_case(b"ENDSEC") => {
                    self.expect_or_recover(Token::Semicolon, "`;`", "ENDSEC")?;
                    return Ok(());
                }
                Token::Keyword(k) => {
                    let keyword = String::from_utf8_lossy(k).to_ascii_uppercase();
                    let result = self.parse_parameter_list().and_then(|parameters| {
                        self.expect(Token::Semicolon, "`;`")?;
                        Ok(Segment {
                            keyword,
                            parameters,
                        })
                    });
                    match result {
                        Ok(seg) => header.entries.push(seg),
                        Err(e) => {
                            if !self.recover(e, "header record")? {
                                return Ok(());
                            }
                        }
                    }
                }
                other => {
                    let e = RecordError::Syntax {
                        message: format!("expected a header keyword, found {}", describe(&other)),
                        span,
                    };
                    if !self.recover(e, "header record")? {
                        return Ok(());
                    }
                }
            }
        }
    }

    fn parse_data(&mut self, instances: &mut BTreeMap<Id, Instance>) -> Result<(), ParseError> {
        loop {
            let (tok, span) = match self.next() {
                Ok(t) => t,
                Err(e) => {
                    if !self.recover(e, "entity instance")? {
                        return Ok(());
                    }
                    continue;
                }
            };
            match tok {
                Token::Keyword(k) if k.eq_ignore_ascii_case(b"ENDSEC") => {
                    self.expect_or_recover(Token::Semicolon, "`;`", "ENDSEC")?;
                    return Ok(());
                }
                Token::Reference(id) => match self.parse_instance_body(id, span.start) {
                    Ok(inst) => {
                        if let Some(prev) = instances.get(&id) {
                            let prev_line = self.line_of(prev.span.start);
                            self.diag(
                                Severity::Error,
                                format!(
                                    "duplicate instance name #{id} (first defined on line {prev_line}); later definition ignored"
                                ),
                                inst.span,
                            );
                        } else {
                            instances.insert(id, inst);
                        }
                    }
                    Err(e) => {
                        if !self.recover(e, &format!("instance #{id}"))? {
                            return Ok(());
                        }
                    }
                },
                Token::Keyword(k)
                    if k.eq_ignore_ascii_case(b"END-ISO-10303-21")
                        || k.eq_ignore_ascii_case(b"DATA")
                        || k.eq_ignore_ascii_case(b"HEADER") =>
                {
                    // Missing ENDSEC; push the token back and let the caller
                    // handle the section keyword.
                    self.diag(
                        Severity::Warning,
                        "missing `ENDSEC;` before section keyword",
                        span,
                    );
                    self.peeked = Some((tok, span));
                    return Ok(());
                }
                other => {
                    let e = RecordError::Syntax {
                        message: format!("expected `#id=`, found {}", describe(&other)),
                        span,
                    };
                    if !self.recover(e, "entity instance")? {
                        return Ok(());
                    }
                }
            }
        }
    }

    /// After `#id` has been consumed: `= KEYWORD(...) ;` or `= ( ... ) ;`.
    fn parse_instance_body(&mut self, id: Id, start: usize) -> RResult<Instance> {
        self.expect(Token::Equals, "`=`")?;
        let (tok, span) = self.next()?;
        let segments = match tok {
            Token::Keyword(k) => vec![self.parse_segment_after_keyword(k)?],
            Token::LParen => {
                let mut segs = Vec::new();
                loop {
                    let (tok, span) = self.next()?;
                    match tok {
                        Token::Keyword(k) => segs.push(self.parse_segment_after_keyword(k)?),
                        Token::RParen => break,
                        other => {
                            return Err(RecordError::Syntax {
                                message: format!(
                                    "expected an entity keyword in complex instance, found {}",
                                    describe(&other)
                                ),
                                span,
                            });
                        }
                    }
                }
                if segs.is_empty() {
                    return Err(RecordError::Syntax {
                        message: "empty complex instance".into(),
                        span,
                    });
                }
                segs
            }
            other => {
                return Err(RecordError::Syntax {
                    message: format!(
                        "expected an entity keyword or `(`, found {}",
                        describe(&other)
                    ),
                    span,
                });
            }
        };
        let end = self.expect(Token::Semicolon, "`;`")?.end;
        Ok(Instance {
            id,
            segments,
            span: Span { start, end },
        })
    }

    fn parse_segment_after_keyword(&mut self, k: &[u8]) -> RResult<Segment> {
        let keyword = String::from_utf8_lossy(k).to_ascii_uppercase();
        let parameters = self.parse_parameter_list()?;
        Ok(Segment {
            keyword,
            parameters,
        })
    }

    /// `( p, p, ... )` including the parentheses.
    fn parse_parameter_list(&mut self) -> RResult<Vec<Parameter>> {
        self.expect(Token::LParen, "`(`")?;
        self.parse_parameter_list_body()
    }

    /// After `(` has been consumed: parameters up to and including `)`.
    fn parse_parameter_list_body(&mut self) -> RResult<Vec<Parameter>> {
        let mut out = Vec::new();
        if matches!(self.peek()?, Some((Token::RParen, _))) {
            self.next()?;
            return Ok(out);
        }
        loop {
            out.push(self.parse_parameter()?);
            let (tok, span) = self.next()?;
            match tok {
                Token::Comma => {}
                Token::RParen => return Ok(out),
                other => {
                    return Err(RecordError::Syntax {
                        message: format!("expected `,` or `)`, found {}", describe(&other)),
                        span,
                    });
                }
            }
        }
    }

    fn parse_parameter(&mut self) -> RResult<Parameter> {
        let (tok, span) = self.next()?;
        Ok(match tok {
            Token::Dollar => Parameter::Unset,
            Token::Star => Parameter::Derived,
            Token::Integer(i) => Parameter::Integer(i),
            Token::Real(r) => Parameter::Real(r),
            Token::String(raw) => Parameter::String(string::decode(raw)),
            Token::Enumeration(e) => {
                Parameter::Enumeration(String::from_utf8_lossy(e).to_ascii_uppercase())
            }
            Token::Reference(id) => Parameter::Reference(id),
            Token::Binary(b) => Parameter::Binary(String::from_utf8_lossy(b).into_owned()),
            Token::LParen => Parameter::List(self.parse_parameter_list_body()?),
            Token::Keyword(k) => {
                let keyword = String::from_utf8_lossy(k).to_ascii_uppercase();
                self.expect(Token::LParen, "`(` after typed parameter keyword")?;
                let mut inner = self.parse_parameter_list_body()?;
                let value = match inner.len() {
                    1 => inner.pop().unwrap(),
                    // A typed value carries exactly one parameter, but be
                    // lenient and keep whatever was there as a list.
                    _ => Parameter::List(inner),
                };
                Parameter::Typed {
                    keyword,
                    value: Box::new(value),
                }
            }
            other => {
                return Err(RecordError::Syntax {
                    message: format!("expected a parameter, found {}", describe(&other)),
                    span,
                });
            }
        })
    }

    /// Report references to instances that do not exist.
    fn check_references(&mut self, instances: &BTreeMap<Id, Instance>) {
        let mut missing = 0usize;
        let mut first: Option<(Id, Id, Span)> = None;
        for inst in instances.values() {
            for r in inst.references() {
                if !instances.contains_key(&r) {
                    missing += 1;
                    if first.is_none() {
                        first = Some((inst.id, r, inst.span));
                    }
                }
            }
        }
        if let Some((from, to, span)) = first {
            let more = if missing > 1 {
                format!(" ({} more)", missing - 1)
            } else {
                String::new()
            };
            self.diag(
                Severity::Warning,
                format!("#{from} references undefined instance #{to}{more}"),
                span,
            );
        }
    }
}

fn describe(tok: &Token<'_>) -> String {
    match tok {
        Token::Keyword(k) => format!("keyword `{}`", String::from_utf8_lossy(k)),
        Token::Integer(i) => format!("integer `{i}`"),
        Token::Real(r) => format!("real `{r}`"),
        Token::String(_) => "a string".into(),
        Token::Enumeration(e) => format!("enumeration `.{}.`", String::from_utf8_lossy(e)),
        Token::Reference(id) => format!("reference `#{id}`"),
        Token::Binary(_) => "a binary literal".into(),
        Token::LParen => "`(`".into(),
        Token::RParen => "`)`".into(),
        Token::Comma => "`,`".into(),
        Token::Semicolon => "`;`".into(),
        Token::Equals => "`=`".into(),
        Token::Dollar => "`$`".into(),
        Token::Star => "`*`".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = "ISO-10303-21;
HEADER;
FILE_DESCRIPTION(('desc'),'2;1');
FILE_NAME('a.stp','2024-01-01T00:00:00',('me'),('org'),'pre','sys','auth');
FILE_SCHEMA(('AP242_MANAGED_MODEL_BASED_3D_ENGINEERING_MIM_LF { 1 0 10303 442 1 1 4 }'));
ENDSEC;
DATA;
#1=CARTESIAN_POINT('',(0.,1.,2.));
#2=(GEOMETRIC_TOLERANCE('FC1',$,#3,#4) GEOMETRIC_TOLERANCE_WITH_MODIFIERS((.MAXIMUM_MATERIAL_REQUIREMENT.)) POSITION_TOLERANCE());
#3=(LENGTH_MEASURE_WITH_UNIT() MEASURE_REPRESENTATION_ITEM('') MEASURE_WITH_UNIT(LENGTH_MEASURE(0.1),#5) REPRESENTATION_ITEM(''));
#4=SHAPE_ASPECT('','',#6,.T.);
#5=(LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.));
#6=PRODUCT_DEFINITION_SHAPE('','',#7);
#7=PRODUCT_DEFINITION('design','',#8,#9);
#8=PRODUCT_DEFINITION_FORMATION('','',#10);
#9=PRODUCT_DEFINITION_CONTEXT('part definition',#11,'design');
#10=PRODUCT('P','P','',(#12));
#11=APPLICATION_CONTEXT('managed model based 3d engineering');
#12=PRODUCT_CONTEXT('',#11,'mechanical');
ENDSEC;
END-ISO-10303-21;
";

    #[test]
    fn parses_minimal_file() {
        let ex = parse(MINIMAL).unwrap();
        assert_eq!(ex.len(), 12);
        assert!(ex.diagnostics.is_empty(), "{:?}", ex.diagnostics);
        assert_eq!(
            ex.header.schema_name().as_deref(),
            Some("AP242_MANAGED_MODEL_BASED_3D_ENGINEERING_MIM_LF")
        );
        assert_eq!(ex.header.file_name(), Some("a.stp"));
        assert_eq!(ex.header.description(), vec!["desc"]);
        assert_eq!(ex.header.implementation_level(), Some("2;1"));

        let p = ex.get(1).unwrap();
        assert_eq!(p.type_key(), "CARTESIAN_POINT");
        assert_eq!(p.parameters()[0], Parameter::String(String::new()));
        assert_eq!(
            p.parameters()[1],
            Parameter::List(vec![
                Parameter::Real(0.0),
                Parameter::Real(1.0),
                Parameter::Real(2.0)
            ])
        );

        let t = ex.get(2).unwrap();
        assert!(t.is_complex());
        assert!(t.has_type("position_tolerance"));
        assert_eq!(
            t.type_key(),
            "GEOMETRIC_TOLERANCE+GEOMETRIC_TOLERANCE_WITH_MODIFIERS+POSITION_TOLERANCE"
        );
        assert_eq!(
            t.attr("GEOMETRIC_TOLERANCE", 0).unwrap().as_str(),
            Some("FC1")
        );
        assert!(t.attr("GEOMETRIC_TOLERANCE", 1).unwrap().is_unset());
        assert_eq!(t.attr("GEOMETRIC_TOLERANCE", 2).unwrap().as_ref(), Some(3));
        let mods = t.attr("GEOMETRIC_TOLERANCE_WITH_MODIFIERS", 0).unwrap();
        assert_eq!(
            mods.as_list().unwrap()[0].as_enum(),
            Some("MAXIMUM_MATERIAL_REQUIREMENT")
        );

        let m = ex.get(3).unwrap();
        let v = m.attr("MEASURE_WITH_UNIT", 0).unwrap();
        assert!(matches!(v, Parameter::Typed { keyword, .. } if keyword == "LENGTH_MEASURE"));
        assert_eq!(v.as_f64(), Some(0.1));

        assert_eq!(ex.count_of_type("geometric_tolerance"), 1);
        assert_eq!(ex.of_type("REPRESENTATION_ITEM").count(), 1);
        assert_eq!(ex.referrers(3), vec![2]);
        assert_eq!(ex.complex_type_counts().len(), 3);
    }

    #[test]
    fn instance_display_round_trips_simple_records() {
        let ex = parse(MINIMAL).unwrap();
        assert_eq!(
            ex.get(1).unwrap().to_string(),
            "#1=CARTESIAN_POINT('',(0.,1.,2.));"
        );
        assert_eq!(
            ex.get(5).unwrap().to_string(),
            "#5=(LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.));"
        );
    }

    #[test]
    fn rejects_non_step_input() {
        let err = parse("hello world").unwrap_err();
        assert!(err.message.contains("not a STEP Part 21 file"), "{err}");
        assert!(parse("").is_err());
    }

    #[test]
    fn recovers_from_bad_records() {
        let src = "ISO-10303-21;\nHEADER;\nENDSEC;\nDATA;\n#1=GOOD('a');\n#2=BAD(1 2);\n#3=ALSO_BAD(#);\n#4=GOOD('b');\n#4=GOOD('dup');\n#5=REF(#99);\n#6=@;\nENDSEC;\nEND-ISO-10303-21;\n";
        let ex = parse(src).unwrap();
        let ids: Vec<_> = ex.instances().map(|i| i.id).collect();
        assert_eq!(ids, vec![1, 4, 5]);
        assert_eq!(ex.get(1).unwrap().parameters()[0].as_str(), Some("a"));
        assert_eq!(ex.get(4).unwrap().parameters()[0].as_str(), Some("b"));
        let errors: Vec<_> = ex
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| d.message.as_str())
            .collect();
        assert_eq!(errors.len(), 4, "{errors:?}");
        assert!(errors[0].contains("skipped instance #2"), "{errors:?}");
        assert!(errors[1].contains("skipped instance #3"), "{errors:?}");
        assert!(
            errors[2].contains("duplicate instance name #4"),
            "{errors:?}"
        );
        assert!(errors[3].contains("unexpected byte 0x40"), "{errors:?}");
        assert!(
            ex.diagnostics
                .iter()
                .any(|d| d.message.contains("undefined instance #99"))
        );
        assert!(ex.has_errors());
    }

    #[test]
    fn unterminated_string_does_not_panic_or_fail() {
        // An unterminated string swallows the rest of the file; the parser
        // must still return what it read and report the loss.
        let src = "ISO-10303-21;\nHEADER;\nENDSEC;\nDATA;\n#1=GOOD('a');\n#2=BAD('oops);\n#3=GOOD('b');\nENDSEC;\nEND-ISO-10303-21;\n";
        let ex = parse(src).unwrap();
        assert_eq!(ex.get(1).unwrap().parameters()[0].as_str(), Some("a"));
        assert!(ex.has_errors());
        assert!(
            ex.diagnostics
                .iter()
                .any(|d| d.message.contains("end of file"))
        );
    }

    #[test]
    fn tolerates_missing_end_marker_and_lowercase() {
        let src =
            "iso-10303-21;\nheader;\nendsec;\ndata;\n#1=cartesian_point('',(0.,0.,0.));\nendsec;\n";
        let ex = parse(src).unwrap();
        assert_eq!(ex.len(), 1);
        assert_eq!(ex.get(1).unwrap().type_key(), "CARTESIAN_POINT");
        assert!(
            ex.diagnostics
                .iter()
                .any(|d| d.message.contains("END-ISO-10303-21"))
        );
        assert!(!ex.has_errors());
    }

    #[test]
    fn skips_edition3_sections() {
        let src = "ISO-10303-21;\nHEADER;\nENDSEC;\nANCHOR;\n<a>=#1;\nENDSEC;\nDATA;\n#1=X();\nENDSEC;\nEND-ISO-10303-21;\n";
        let ex = parse(src).unwrap();
        assert_eq!(ex.len(), 1);
        assert!(ex.diagnostics.iter().any(|d| d.message.contains("ANCHOR")));
    }

    #[test]
    fn line_numbers_are_one_based() {
        let src =
            "ISO-10303-21;\nHEADER;\nENDSEC;\nDATA;\n#1=A();\n#2=B(;\nENDSEC;\nEND-ISO-10303-21;\n";
        let ex = parse(src).unwrap();
        let err = ex
            .diagnostics
            .iter()
            .find(|d| d.severity == Severity::Error)
            .unwrap();
        assert_eq!(err.line, 6);
    }
}
