//! What a file contains: parts, revisions, and where each one sits
//! (ADR 0014).
//!
//! This is neither PMI nor geometry. The PMI document says what a file
//! *states* and the features document says what a shape *is*; this says
//! what the file *holds* — which parts, at what revision, used how many
//! times and in what arrangement.
//!
//! It is kept apart from the other two for the reason they are kept
//! apart from each other: the three answer different questions, change
//! at different rates, and are versioned separately. A consumer joins
//! them by part id and body id rather than by their all being in one
//! object.

pub mod model;
pub mod summary;

pub use model::{
    Approval, Classification, Diagnostic, Involvement, Part, ProductDocument, Relation,
    SCHEMA_VERSION, Unattached,
};
pub use summary::{Ambiguity, Attribute, BodySummary, Candidate, PartSummary, summarise};

use std::path::Path;

/// Read the product structure of the file at `path`.
///
/// The format is detected from the extension, as it is for `extract`.
/// Placements come out in millimetres whatever the file declared; the
/// file's own units are recorded beside them.
pub fn read_path(path: &Path) -> crate::Result<ProductDocument> {
    let format = crate::Format::from_path(path)
        .ok_or_else(|| crate::Error::UnknownFormat(path.display().to_string()))?;
    let bytes = std::fs::read(path)?;
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    match format {
        crate::Format::Step => from_step(&bytes, &file_name),
        crate::Format::Jt => from_jt(&bytes, &file_name),
    }
}

/// Read the product structure of a STEP file.
pub fn from_step(bytes: &[u8], file_name: &str) -> crate::Result<ProductDocument> {
    let ex = crate::step::p21::parse_bytes(bytes)?;
    let source = crate::model::Source {
        file_name: file_name.to_owned(),
        format: "STEP".into(),
        schema: ex.header.schema_name(),
        writer: ex.header.originating_system().map(str::to_owned),
        time_stamp: ex.header.time_stamp().map(str::to_owned),
    };
    Ok(crate::step::product::document(&ex, source))
}

/// Read the product structure of a JT file.
///
/// JT states its structure in the logical scene graph's node hierarchy:
/// a part node is a part and an instance node is an occurrence of one
/// (ADR 0014).
pub fn from_jt(bytes: &[u8], file_name: &str) -> crate::Result<ProductDocument> {
    let jt = crate::jt::file::Jt::parse(bytes)?;
    let source = crate::model::Source {
        file_name: file_name.to_owned(),
        format: "JT".into(),
        schema: Some(jt.header.version.clone()),
        writer: None,
        time_stamp: None,
    };
    Ok(crate::jt::product::document(&jt, source))
}
