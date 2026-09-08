//! JT (ISO 14306) support.
//!
//! - [`mod@file`] reads the file structure: header, table of contents, and
//!   segments, decompressing segment payloads on demand.
//! - [`mod@element`] walks the element stream inside a decompressed segment.
//! - [`mod@codec`] decodes the compressed integer packets that JT's
//!   topology and geometry tables are built from.
//! - [`mod@stt`] reads the Smart Topology Table, which abstracts a part's
//!   precise B-rep without a Parasolid reader.
//! - [`mod@pmi`] reads the PMI Manager element those segments carry.
//! - [`mod@property`] reads the scene graph's properties, which declare the
//!   unit that PMI measures are expressed in.
//! - [`mod@meta`] reads the metadata segments, where a part's own
//!   properties are stated.
//! - [`mod@semantic`] maps what those two produce onto the model of ADR 0002,
//!   and [`mod@presentation`] onto that of ADR 0003.
//! - [`mod@identity`] replaces the temporary ids of both with identity
//!   keys (ADR 0004).
//! - [`JtReader`] ties them together behind [`crate::Reader`].
//!
//! The reader is scoped to PMI (ADR 0009): it parses the structure of any
//! file and decodes only the segments that carry PMI and metadata.
//! Geometry segments are listed but never decoded.

pub mod codec;
pub mod element;
pub mod file;
pub mod identity;
pub mod meta;
pub mod pmi;
pub mod presentation;
pub mod property;
pub mod reader;
pub mod semantic;
pub mod stt;

pub use element::{Element, Elements};
pub use file::{ByteOrder, Guid, Header, Jt, ParseError, Segment, SegmentKind};
pub use pmi::{Entity, EntityKind, PmiManager};
pub use property::Properties;
pub use reader::JtReader;
