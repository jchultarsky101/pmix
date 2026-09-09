//! `pmix` extracts Product Manufacturing Information (PMI) from 3D model
//! files and produces a stable, comparable JSON representation of it.
//!
//! The crate is split into a library (this module) and a thin command-line
//! front end (`src/main.rs`). The library is the intended integration point
//! for anyone who wants to embed PMI extraction in their own tooling.
//!
//! - [`model`] is the JSON data model (ADR 0002, 0003, 0004).
//! - [`geometry`] summarises what an annotation draws, for either format.
//! - [`identity`] turns identity keys into ids, for either format (ADR 0004).
//! - [`step`] reads STEP AP242 files: a Part 21 parser plus PMI walkers.
//! - [`jt`] reads JT files: the file structure, the PMI Manager element,
//!   and the scene graph properties that declare the model units (ADR 0009).
//! - [`reader`] is the format-independent entry point.
//! - [`diff`] compares two documents (ADR 0005).
//!
//! # Status
//!
//! The STEP reader extracts the semantic layer (units, features,
//! dimensions, geometric tolerances, datums, datum systems) and the
//! presentation layer (annotations and saved views), with ids that survive
//! re-export (ADR 0004), and [`diff`] compares documents. The JT reader
//! extracts both layers and the model's properties, with the same
//! identity scheme; records named for design intent, such as datums and
//! saved views, get the same id from either format.

pub mod diff;
pub mod fingerprint;
pub mod format;
pub mod geometry;
pub mod identity;
pub mod jt;
pub mod model;
pub mod reader;
pub mod step;

pub use format::Format;
pub use model::PmiDocument;
pub use reader::{ExtractOptions, Reader, read_path, read_path_with};

use std::path::Path;

/// Errors produced while reading a model file or extracting PMI from it.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The input file's format could not be determined from its extension.
    #[error("unrecognised file format for `{0}`; expected a .stp/.step or .jt file")]
    UnknownFormat(String),

    /// The input is not a STEP Part 21 file at all.
    #[error("STEP parse error: {0}")]
    StepParse(#[from] step::p21::ParseError),

    /// The input is not a readable JT file.
    #[error("JT parse error: {0}")]
    JtParse(#[from] jt::ParseError),

    /// A JSON document written by `pmix extract` could not be read.
    #[error("invalid pmix JSON document: {0}")]
    Json(#[from] serde_json::Error),

    /// An I/O failure while reading the input.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Convenience alias used throughout the crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Read the file at `path`, detect its format, and extract the PMI it contains.
pub fn extract(path: &Path) -> Result<PmiDocument> {
    read_path(path)
}

/// [`extract`] with options.
pub fn extract_with(path: &Path, options: &ExtractOptions) -> Result<PmiDocument> {
    read_path_with(path, options)
}

/// Load a document: a `.json` file written by `pmix extract`, or a model
/// file, which is extracted.
pub fn load(path: &Path) -> Result<PmiDocument> {
    if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("json"))
    {
        let text = std::fs::read_to_string(path)?;
        return Ok(serde_json::from_str(&text)?);
    }
    extract(path)
}
