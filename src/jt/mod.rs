//! JT (ISO 14306) support.
//!
//! - [`mod@file`] reads the file structure: header, table of contents, and
//!   segments, decompressing segment payloads on demand.
//! - [`mod@element`] walks the element stream inside a decompressed segment.
//! - [`mod@pmi`] reads the PMI Manager element those segments carry.
//! - [`mod@property`] reads the scene graph's properties, which declare the
//!   unit that PMI measures are expressed in.
//! - [`mod@semantic`] maps what those two produce onto the model of ADR 0002.
//! - [`JtReader`] ties them together behind [`crate::Reader`].
//!
//! The reader is scoped to PMI (ADR 0009): it parses the structure of any
//! file and decodes only the segments that carry PMI and metadata.
//! Geometry segments are listed but never decoded.

pub mod element;
pub mod file;
pub mod pmi;
pub mod property;
pub mod reader;
pub mod semantic;

pub use element::{Element, Elements};
pub use file::{ByteOrder, Guid, Header, Jt, ParseError, Segment, SegmentKind};
pub use pmi::{Entity, EntityKind, PmiManager};
pub use property::Properties;
pub use reader::JtReader;
