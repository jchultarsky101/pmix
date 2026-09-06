//! `pmix` extracts Product Manufacturing Information (PMI) from 3D model
//! files and produces a stable, comparable JSON representation of it.
//!
//! The crate is split into a library (this module) and a thin command-line
//! front end (`src/main.rs`). The library is the intended integration point
//! for anyone who wants to embed PMI extraction in their own tooling.
//!
//! # Status
//!
//! The project is at the scaffolding stage. Format detection and the JSON
//! data model exist; the STEP and JT readers are not implemented yet.

pub mod format;
pub mod model;

pub use format::Format;
pub use model::PmiDocument;

use std::path::Path;

/// Errors produced while reading a model file or extracting PMI from it.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The input file's format could not be determined from its extension.
    #[error("unrecognised file format for `{0}`; expected a .stp/.step or .jt file")]
    UnknownFormat(String),

    /// The format is recognised but the reader for it is not available yet.
    #[error("{0} support is not implemented yet")]
    Unsupported(Format),

    /// An I/O failure while reading the input.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Convenience alias used throughout the crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Read the file at `path`, detect its format, and extract the PMI it contains.
pub fn extract(path: &Path) -> Result<PmiDocument> {
    let format =
        Format::from_path(path).ok_or_else(|| Error::UnknownFormat(path.display().to_string()))?;
    tracing::debug!(path = %path.display(), %format, "detected input format");

    // Fail early on unreadable input so callers get an I/O error rather than
    // an "unsupported" error for a file that does not exist.
    std::fs::metadata(path)?;

    Err(Error::Unsupported(format))
}
