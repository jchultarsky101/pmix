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

#[cfg(test)]
mod tests {
    use super::*;

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
            value: "NEW200-041".into(),
        };
        assert_eq!(serde_json::to_value(&t).unwrap()["type"], "text");
    }
}
