//! Format-independent reader interface.

use std::path::Path;

use crate::model::PmiDocument;
use crate::{Error, Format, Result};

/// Options that change what a reader emits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExtractOptions {
    /// Emit full annotation geometry (coordinates and triangles) instead
    /// of only the summary (ADR 0003).
    pub presentation_geometry: bool,
}

/// A reader turns one input file into a [`PmiDocument`].
///
/// Readers never panic on malformed input and never drop recognised PMI
/// silently: problems become diagnostics or `unknown` records.
pub trait Reader {
    /// Read `input`, which was loaded from a file called `file_name` (used
    /// only for the document's `source` block).
    fn read(&self, input: &[u8], file_name: &str, options: &ExtractOptions) -> Result<PmiDocument>;
}

/// Read the file at `path` with the reader for its detected format.
pub fn read_path(path: &Path) -> Result<PmiDocument> {
    read_path_with(path, &ExtractOptions::default())
}

/// [`read_path`] with options.
pub fn read_path_with(path: &Path, options: &ExtractOptions) -> Result<PmiDocument> {
    let format =
        Format::from_path(path).ok_or_else(|| Error::UnknownFormat(path.display().to_string()))?;
    tracing::debug!(path = %path.display(), %format, "detected input format");
    let bytes = std::fs::read(path)?;
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    match format {
        Format::Step => crate::step::StepReader.read(&bytes, &file_name, options),
        Format::Jt => Err(Error::Unsupported(format)),
    }
}
