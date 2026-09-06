//! Untyped value model for Part 21 entity instances.

use std::fmt;

use serde::Serialize;

/// Entity instance name, the number after `#`.
pub type Id = u64;

/// Byte range in the source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct Span {
    /// Offset of the first byte.
    pub start: usize,
    /// Offset one past the last byte.
    pub end: usize,
}

/// A single attribute value of an entity instance.
#[derive(Debug, Clone, PartialEq)]
pub enum Parameter {
    /// `$`: attribute value not provided.
    Unset,
    /// `*`: attribute value derived (redeclared) elsewhere.
    Derived,
    /// Integer literal.
    Integer(i64),
    /// Real literal.
    Real(f64),
    /// String literal, with Part 21 escapes decoded.
    String(String),
    /// Enumeration literal `.NAME.`, stored without the dots.
    Enumeration(String),
    /// Reference `#n` to another instance.
    Reference(Id),
    /// Typed (select) value such as `LENGTH_MEASURE(1.5)`.
    Typed {
        /// The defined-type keyword, upper case.
        keyword: String,
        /// The wrapped value.
        value: Box<Parameter>,
    },
    /// Aggregate `( ... )`.
    List(Vec<Parameter>),
    /// Binary literal `"..."`, kept as the raw hex text.
    Binary(String),
}

impl Parameter {
    /// The referenced instance id, if this is a `#n` reference.
    pub fn as_ref(&self) -> Option<Id> {
        match self {
            Self::Reference(id) => Some(*id),
            _ => None,
        }
    }

    /// The string content, if this is a string literal.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }

    /// The enumeration name, if this is an enumeration literal.
    pub fn as_enum(&self) -> Option<&str> {
        match self {
            Self::Enumeration(s) => Some(s),
            _ => None,
        }
    }

    /// The numeric value as `f64`, for integer or real literals. Looks
    /// through one level of typed wrapper (`LENGTH_MEASURE(1.5)`).
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Integer(i) => Some(*i as f64),
            Self::Real(r) => Some(*r),
            Self::Typed { value, .. } => value.as_f64(),
            _ => None,
        }
    }

    /// The integer value, if this is an integer literal.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Integer(i) => Some(*i),
            _ => None,
        }
    }

    /// The elements, if this is an aggregate.
    pub fn as_list(&self) -> Option<&[Parameter]> {
        match self {
            Self::List(items) => Some(items),
            _ => None,
        }
    }

    /// The inner value of a typed wrapper, or `self` if not wrapped.
    pub fn unwrap_typed(&self) -> &Parameter {
        match self {
            Self::Typed { value, .. } => value.unwrap_typed(),
            other => other,
        }
    }

    /// All instance ids referenced anywhere inside this value, in order.
    pub fn collect_refs(&self, out: &mut Vec<Id>) {
        match self {
            Self::Reference(id) => out.push(*id),
            Self::Typed { value, .. } => value.collect_refs(out),
            Self::List(items) => items.iter().for_each(|p| p.collect_refs(out)),
            _ => {}
        }
    }

    /// `true` for `$`.
    pub fn is_unset(&self) -> bool {
        matches!(self, Self::Unset)
    }
}

impl fmt::Display for Parameter {
    /// Renders the value in Part 21 syntax. Strings are shown with `''`
    /// escaping but otherwise as decoded UTF-8, which is friendlier for
    /// humans than the original `\X2\` escapes.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unset => f.write_str("$"),
            Self::Derived => f.write_str("*"),
            Self::Integer(i) => write!(f, "{i}"),
            Self::Real(r) => {
                if r.fract() == 0.0 && r.is_finite() && r.abs() < 1e15 {
                    write!(f, "{r:.0}.")
                } else {
                    write!(f, "{r}")
                }
            }
            Self::String(s) => write!(f, "'{}'", s.replace('\'', "''")),
            Self::Enumeration(e) => write!(f, ".{e}."),
            Self::Reference(id) => write!(f, "#{id}"),
            Self::Typed { keyword, value } => write!(f, "{keyword}({value})"),
            Self::List(items) => {
                f.write_str("(")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        f.write_str(",")?;
                    }
                    write!(f, "{item}")?;
                }
                f.write_str(")")
            }
            Self::Binary(b) => write!(f, "\"{b}\""),
        }
    }
}

impl Serialize for Parameter {
    /// JSON form: numbers and strings map directly; `$` becomes `null`;
    /// everything else is a small tagged object so that the kind is never
    /// ambiguous (`{"ref": 12}`, `{"enum": "T"}`, `{"type": "LENGTH_MEASURE",
    /// "value": 1.5}`, `{"derived": true}`, `{"binary": "0F"}`).
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        match self {
            Self::Unset => s.serialize_none(),
            Self::Derived => {
                let mut m = s.serialize_map(Some(1))?;
                m.serialize_entry("derived", &true)?;
                m.end()
            }
            Self::Integer(i) => s.serialize_i64(*i),
            Self::Real(r) => s.serialize_f64(*r),
            Self::String(v) => s.serialize_str(v),
            Self::Enumeration(e) => {
                let mut m = s.serialize_map(Some(1))?;
                m.serialize_entry("enum", e)?;
                m.end()
            }
            Self::Reference(id) => {
                let mut m = s.serialize_map(Some(1))?;
                m.serialize_entry("ref", id)?;
                m.end()
            }
            Self::Typed { keyword, value } => {
                let mut m = s.serialize_map(Some(2))?;
                m.serialize_entry("type", keyword)?;
                m.serialize_entry("value", value)?;
                m.end()
            }
            Self::List(items) => items.serialize(s),
            Self::Binary(b) => {
                let mut m = s.serialize_map(Some(1))?;
                m.serialize_entry("binary", b)?;
                m.end()
            }
        }
    }
}

/// One `KEYWORD(param, ...)` record. A simple instance has exactly one; a
/// complex instance `#1=(A(...) B(...))` has one per listed supertype.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Segment {
    /// Entity type name, upper case.
    pub keyword: String,
    /// Attribute values in declaration order.
    pub parameters: Vec<Parameter>,
}

/// An entity instance from a `DATA` section.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Instance {
    /// The instance name `#id`.
    pub id: Id,
    /// One segment for a simple instance, several for a complex one.
    pub segments: Vec<Segment>,
    /// Location of the whole `#id=...;` record in the source.
    pub span: Span,
}

impl Instance {
    /// `true` if this is a complex (multi-segment) instance.
    pub fn is_complex(&self) -> bool {
        self.segments.len() > 1
    }

    /// `true` if any segment has the given entity type (case-insensitive).
    pub fn has_type(&self, keyword: &str) -> bool {
        self.segments
            .iter()
            .any(|s| s.keyword.eq_ignore_ascii_case(keyword))
    }

    /// The segment for the given entity type, if present.
    pub fn segment(&self, keyword: &str) -> Option<&Segment> {
        self.segments
            .iter()
            .find(|s| s.keyword.eq_ignore_ascii_case(keyword))
    }

    /// Entity type names of all segments, in file order.
    pub fn type_names(&self) -> impl Iterator<Item = &str> {
        self.segments.iter().map(|s| s.keyword.as_str())
    }

    /// A single key naming the instance's type: the keyword for a simple
    /// instance, or the segment keywords joined with `+` for a complex one.
    pub fn type_key(&self) -> String {
        self.type_names().collect::<Vec<_>>().join("+")
    }

    /// Parameters of a simple instance, or of the first segment.
    pub fn parameters(&self) -> &[Parameter] {
        self.segments
            .first()
            .map(|s| s.parameters.as_slice())
            .unwrap_or(&[])
    }

    /// The `n`th parameter of the segment with the given type, if any.
    pub fn attr(&self, keyword: &str, n: usize) -> Option<&Parameter> {
        self.segment(keyword)?.parameters.get(n)
    }

    /// All instance ids referenced from any parameter, in order, with
    /// duplicates retained.
    pub fn references(&self) -> Vec<Id> {
        let mut out = Vec::new();
        for seg in &self.segments {
            for p in &seg.parameters {
                p.collect_refs(&mut out);
            }
        }
        out
    }
}

impl fmt::Display for Instance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}=", self.id)?;
        if self.is_complex() {
            f.write_str("(")?;
            for (i, seg) in self.segments.iter().enumerate() {
                if i > 0 {
                    f.write_str(" ")?;
                }
                write!(f, "{seg}")?;
            }
            f.write_str(")")?;
        } else if let Some(seg) = self.segments.first() {
            write!(f, "{seg}")?;
        }
        f.write_str(";")
    }
}

impl fmt::Display for Segment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}(", self.keyword)?;
        for (i, p) in self.parameters.iter().enumerate() {
            if i > 0 {
                f.write_str(",")?;
            }
            write!(f, "{p}")?;
        }
        f.write_str(")")
    }
}
