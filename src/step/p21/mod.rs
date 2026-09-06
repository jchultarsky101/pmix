//! ISO 10303-21 ("Part 21") exchange structure parser.
//!
//! A Part 21 file is a header followed by one or more `DATA` sections of
//! entity instances:
//!
//! ```text
//! ISO-10303-21;
//! HEADER;
//! FILE_DESCRIPTION(('...'),'2;1');
//! FILE_NAME('part.stp','2024-01-01T00:00:00',(''),(''),'','','');
//! FILE_SCHEMA(('AP242_MANAGED_MODEL_BASED_3D_ENGINEERING_MIM_LF'));
//! ENDSEC;
//! DATA;
//! #10=CARTESIAN_POINT('',(0.,0.,0.));
//! #11=(GEOMETRIC_TOLERANCE('','',#12,#13) POSITION_TOLERANCE());
//! ENDSEC;
//! END-ISO-10303-21;
//! ```
//!
//! This module produces an [`Exchange`]: the header plus every instance as an
//! untyped [`Instance`] made of one or more [`Segment`]s (one for a simple
//! instance, several for a complex instance such as `#11` above), with
//! parameters as [`Parameter`] values. It does not validate against any
//! EXPRESS schema; that interpretation happens in higher layers.
//!
//! Parsing is tolerant: a malformed instance is reported as a
//! [`Diagnostic`] and skipped, and the rest of the file is still read.
//! Real-world files, including the NIST test corpus, contain such errors.

mod exchange;
mod lexer;
mod parser;
mod string;
mod value;

pub use exchange::{Diagnostic, Exchange, Header, Severity};
pub use parser::{ParseError, parse, parse_bytes};
pub use value::{Id, Instance, Parameter, Segment, Span};
