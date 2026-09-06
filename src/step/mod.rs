//! STEP (ISO 10303) support.
//!
//! - [`p21`] parses the Part 21 exchange structure into an untyped entity
//!   graph. It knows nothing about any application protocol.
//! - Higher-level modules (added in later phases) interpret AP242 PMI
//!   entities on top of that graph.

pub mod p21;
