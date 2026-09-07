//! JT (ISO 14306) support.
//!
//! - [`mod@file`] reads the file structure: header, table of contents, and
//!   segments, decompressing segment payloads on demand.
//! - [`mod@element`] walks the element stream inside a decompressed segment.
//!
//! The reader is scoped to PMI (ADR 0009): it parses the structure of any
//! file and decodes only the segments that carry PMI and metadata.
//! Geometry segments are listed but never decoded.

pub mod element;
pub mod file;

pub use element::{Element, Elements};
pub use file::{ByteOrder, Guid, Header, Jt, ParseError, Segment, SegmentKind};
