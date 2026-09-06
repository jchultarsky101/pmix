//! Quantities with units.

use serde::{Deserialize, Serialize};

/// A value in the unit the source file declares. Never converted; the unit
/// is always explicit (ADR 0002, rule 7).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Measure {
    pub value: f64,
    /// Canonical unit name: `mm`, `m`, `in`, `deg`, `rad`, or the source
    /// name lower-cased when unrecognised.
    pub unit: String,
}

impl Measure {
    pub fn new(value: f64, unit: impl Into<String>) -> Self {
        Self {
            value,
            unit: unit.into(),
        }
    }

    /// Canonical text used when hashing for ids: shortest round-trip float
    /// representation plus unit.
    pub fn canonical(&self) -> String {
        format!("{}{}", self.value, self.unit)
    }
}

/// A unit direction vector.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Direction {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}
