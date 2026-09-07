//! STEP (ISO 10303) support.
//!
//! - [`p21`] parses the Part 21 exchange structure into an untyped entity
//!   graph. It knows nothing about any application protocol.
//! - [`pmi`] interprets AP242 PMI entities on top of that graph and fills
//!   the [`crate::model`].
//! - [`StepReader`] ties the two together behind [`crate::Reader`].

pub mod p21;
pub mod pmi;

use crate::model::PmiDocument;
use crate::{ExtractOptions, Reader, Result};

/// Reader for STEP AP242 files.
#[derive(Debug, Clone, Copy, Default)]
pub struct StepReader;

impl Reader for StepReader {
    fn read(&self, input: &[u8], file_name: &str, options: &ExtractOptions) -> Result<PmiDocument> {
        let exchange = p21::parse_bytes(input)?;
        tracing::debug!(
            instances = exchange.len(),
            diagnostics = exchange.diagnostics.len(),
            "parsed Part 21"
        );
        Ok(pmi::extract(&exchange, file_name, options))
    }
}
