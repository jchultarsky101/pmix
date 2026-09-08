//! The JSON data model emitted by `pmix`, as defined in ADR 0002.
//!
//! The model is *comparable*: two documents extracted from different
//! exports of the same design should diff field by field. Every array is
//! sorted by id, ids are derived from content rather than from source
//! entity numbering, and fields that describe the extraction rather than
//! the design (`source`, `source_refs`, `diagnostics`) are marked so a
//! diff can ignore them.

mod id;
mod measure;
mod presentation;
mod properties;
mod semantic;

pub use id::{ContentHasher, ContentId, content_hash};
pub use measure::{Direction, Measure, Placement};
pub use presentation::*;
pub use properties::*;
pub use semantic::*;

use serde::{Deserialize, Serialize};

/// Version of the JSON schema produced by this crate. Bumped whenever the
/// shape of [`PmiDocument`] changes in a way that is not backwards
/// compatible.
pub const SCHEMA_VERSION: u32 = 1;

/// Top-level output of an extraction run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PmiDocument {
    /// Schema version, see [`SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Where the data came from. Excluded from comparison.
    pub source: Source,
    /// Units declared by the file for lengths and angles.
    pub units: Units,
    /// Named values that are neither PMI nor geometry (ADR 0007).
    pub properties: Vec<Property>,
    /// Machine-readable PMI.
    pub semantic: Semantic,
    /// Human-visible PMI (ADR 0003).
    pub presentation: Presentation,
    /// Recognised but unmapped content. Never silently dropped.
    pub unknown: Vec<Unknown>,
    /// Reader warnings. Excluded from comparison.
    pub diagnostics: Vec<Diagnostic>,
}

/// Provenance of an extraction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Source {
    /// File name (without directory) of the input.
    pub file_name: String,
    /// Detected input format, e.g. `"STEP"` or `"JT"`.
    pub format: String,
    /// Schema declared by the file, e.g. the AP242 schema name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    /// Exporting software as declared by the file, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub writer: Option<String>,
    /// Time stamp declared by the file, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_stamp: Option<String>,
}

/// Units declared by the file's global context.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Units {
    /// Length unit, e.g. `"mm"` or `"in"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<String>,
    /// Plane angle unit, e.g. `"deg"` or `"rad"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angle: Option<String>,
}

/// The semantic layer: what a machine can reason about.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Semantic {
    pub features: Vec<Feature>,
    pub datums: Vec<Datum>,
    pub datum_systems: Vec<DatumSystem>,
    pub dimensions: Vec<Dimension>,
    pub tolerances: Vec<GeometricTolerance>,
    pub notes: Vec<Note>,
    pub other: Vec<Other>,
}

impl Semantic {
    /// Sort every array by id, as the schema requires.
    pub fn sort(&mut self) {
        self.features.sort_by(|a, b| a.meta.id.cmp(&b.meta.id));
        self.datums.sort_by(|a, b| a.meta.id.cmp(&b.meta.id));
        self.datum_systems.sort_by(|a, b| a.meta.id.cmp(&b.meta.id));
        self.dimensions.sort_by(|a, b| a.meta.id.cmp(&b.meta.id));
        self.tolerances.sort_by(|a, b| a.meta.id.cmp(&b.meta.id));
        self.notes.sort_by(|a, b| a.meta.id.cmp(&b.meta.id));
        self.other.sort_by(|a, b| a.meta.id.cmp(&b.meta.id));
    }
}

/// Content the reader recognised as PMI-related but could not map.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Unknown {
    /// Which layer the content belongs to.
    pub layer: Layer,
    /// Source type name, e.g. the entity keyword or complex-instance key.
    pub kind: String,
    /// Why it was not mapped.
    pub reason: String,
    /// Source entity reference.
    pub source_ref: String,
    /// The raw source record, for humans.
    pub raw: String,
}

/// Model layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Layer {
    Semantic,
    Presentation,
    Properties,
}

/// A reader warning.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
}
