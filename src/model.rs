//! The JSON data model emitted by `pmix`.
//!
//! The goal of this model is to be *comparable*: two documents extracted
//! from different files (or different versions of the same file) should be
//! diffable field by field. To that end, every collection is sorted
//! deterministically before serialisation and identifiers are derived from
//! content rather than from the source file's internal entity numbering.

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
    /// Information about the file the PMI was extracted from.
    pub source: Source,
    /// The extracted annotations, sorted deterministically.
    pub annotations: Vec<Annotation>,
}

/// Provenance of an extraction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Source {
    /// File name (without directory) of the input.
    pub file_name: String,
    /// Detected input format, e.g. `"STEP"` or `"JT"`.
    pub format: String,
}

/// A single piece of PMI (a dimension, tolerance, datum, note, ...).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Annotation {
    /// Category of the annotation.
    pub kind: AnnotationKind,
    /// Human-readable text as it would appear on the drawing, if available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// Broad classification of PMI annotations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationKind {
    /// A linear, angular, or radial dimension, possibly with tolerances.
    Dimension,
    /// A geometric tolerance (feature control frame).
    GeometricTolerance,
    /// A datum feature or datum target.
    Datum,
    /// A surface finish symbol.
    SurfaceFinish,
    /// Free text note or flag note.
    Note,
    /// Anything the reader recognised but could not classify further.
    Other,
}
