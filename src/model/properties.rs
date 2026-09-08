//! Named property values (ADR 0007).
//!
//! Properties are the named values a file carries that are neither PMI nor
//! geometry: part numbers, revisions, suppliers, prices, and the CAx-IF
//! validation properties that let a consuming system check how it read the
//! PMI. They attach either to the whole part or to one PMI record.

use serde::{Deserialize, Serialize};

use super::semantic::Unmapped;

/// One named value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Property {
    /// Content-derived, unique within the document.
    pub id: String,
    /// The property's own name, e.g. `Part_Number` or `affected area`.
    pub name: String,
    /// The property definition's name when it differs from `name`, e.g.
    /// `pmi validation property` or `PLM__Part_Number`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    pub kind: PropertyKind,
    pub value: PropertyValue,
    /// The part that states this property, in a file that holds several.
    /// Absent when the file describes one part, or when the reader cannot
    /// tell which part a property belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<String>,
    /// The record this property is about; absent means the whole part.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applies_to: Option<String>,
    pub unmapped: Vec<Unmapped>,
    /// Source entities. Excluded from comparison.
    pub source_refs: Vec<String>,
}

/// Whether a property states design data or describes how the PMI was
/// written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PropertyKind {
    /// A property of the design: a part number, a supplier, a mass.
    User,
    /// A CAx-IF validation property, derived from the PMI it describes.
    /// Not compared by `pmix diff` (ADR 0007).
    Validation,
}

/// A property's typed value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PropertyValue {
    Text {
        value: String,
    },
    Integer {
        value: i64,
    },
    Number {
        value: f64,
    },
    /// A quantity in the unit the file declares, never converted.
    Measure {
        value: f64,
        unit: String,
    },
    Boolean {
        value: bool,
    },
}

impl PropertyValue {
    /// Read a text value, using a number when the text is written in
    /// plain decimal notation.
    ///
    /// Files routinely write counts, quantities, and prices as strings.
    /// Reading them as numbers makes them comparable, but only for text
    /// that spells a number and nothing else: an optional minus, then
    /// `0` or a digit string not starting with `0`, then optionally a
    /// decimal point and more digits. Integers must fit in 64 bits.
    ///
    /// So `12` becomes an integer, and `64.0` and `18.75` become numbers.
    /// A leading zero, an exponent, a separator, or stray whitespace keeps
    /// the text, because `007` is a serial number rather than the integer
    /// seven and `1e5` may be a part code. Trailing zeros after the point
    /// do not: `2.50` reads as `2.5`, because rejecting it would make a
    /// field's type depend on whether a measurement happened to be whole.
    pub fn from_text(text: &str) -> Self {
        let keep = || Self::Text {
            value: text.to_owned(),
        };
        if !is_plain_decimal(text) {
            return keep();
        }
        if !text.contains('.') {
            // Too many digits for an integer: keep the text rather than
            // round it away.
            return match text.parse::<i64>() {
                Ok(value) => Self::Integer { value },
                Err(_) => keep(),
            };
        }
        match text.parse::<f64>() {
            Ok(value) if value.is_finite() => Self::Number { value },
            _ => keep(),
        }
    }

    /// Canonical text, used for rendering and for content hashes.
    pub fn canonical(&self) -> String {
        match self {
            Self::Text { value } => value.clone(),
            Self::Integer { value } => value.to_string(),
            Self::Number { value } => value.to_string(),
            Self::Measure { value, unit } => format!("{value}{unit}"),
            Self::Boolean { value } => value.to_string(),
        }
    }
}

/// Is `text` plain decimal notation: `-?(0|[1-9][0-9]*)(\.[0-9]+)?`
fn is_plain_decimal(text: &str) -> bool {
    let body = text.strip_prefix('-').unwrap_or(text);
    let (integer, fraction) = match body.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (body, None),
    };
    let integer_ok = integer == "0"
        || (!integer.is_empty()
            && !integer.starts_with('0')
            && integer.bytes().all(|b| b.is_ascii_digit()));
    let fraction_ok = match fraction {
        None => true,
        Some(f) => !f.is_empty() && f.bytes().all(|b| b.is_ascii_digit()),
    };
    integer_ok && fraction_ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_decimal_text_is_read_as_a_number() {
        for (text, expected) in [
            ("12", PropertyValue::Integer { value: 12 }),
            ("-4", PropertyValue::Integer { value: -4 }),
            ("0", PropertyValue::Integer { value: 0 }),
            ("18.75", PropertyValue::Number { value: 18.75 }),
            ("0.1", PropertyValue::Number { value: 0.1 }),
            // A whole measurement is still a number, so a field's type
            // does not depend on the value it happens to carry.
            ("64.0", PropertyValue::Number { value: 64.0 }),
            (
                "32532.894525837815",
                PropertyValue::Number {
                    value: 32532.894525837815,
                },
            ),
        ] {
            assert_eq!(PropertyValue::from_text(text), expected, "{text:?}");
        }
    }

    #[test]
    fn text_that_is_not_plain_decimal_stays_text() {
        for text in [
            "007",                 // a leading zero belongs to the serial
            "+5",                  // leading sign
            "1e5",                 // exponent notation
            "3.",                  // no digits after the point
            "-.5",                 // no digits before it
            "1,234",               // thousands separator
            " 12",                 // whitespace
            "12 ",                 //
            "inf",                 // not a number's spelling
            "NaN",                 //
            "",                    // empty
            "SYN-004-REV-A",       //
            "true",                //
            "12mm",                //
            "9223372036854775808", // more digits than an integer holds
        ] {
            assert_eq!(
                PropertyValue::from_text(text),
                PropertyValue::Text {
                    value: text.to_owned()
                },
                "{text:?} should stay text"
            );
        }
    }

    #[test]
    fn values_are_tagged_and_round_trip() {
        let v = PropertyValue::Measure {
            value: 14.8,
            unit: "mm2".into(),
        };
        let json = serde_json::to_value(&v).unwrap();
        assert_eq!(json["type"], "measure");
        assert_eq!(json["unit"], "mm2");
        assert_eq!(serde_json::from_value::<PropertyValue>(json).unwrap(), v);
        assert_eq!(v.canonical(), "14.8mm2");

        let t = PropertyValue::Text {
            value: "SYN-004-REV-A".into(),
        };
        assert_eq!(serde_json::to_value(&t).unwrap()["type"], "text");
    }
}
