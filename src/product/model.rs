//! The document `pmix product` writes (ADR 0014).
//!
//! This is a third question, beside what a file *says* (the PMI document)
//! and what a shape *is* (the features document): what a file
//! *contains*. A file holding one part answers it in one line. A file
//! holding an assembly answers it with a tree, and the tree is what
//! makes "find a substitute for this component" a question anyone can
//! ask, because until a body has a part number and a revision beside it
//! there is nothing to look up.
//!
//! Three properties matter here, and each is load-bearing:
//!
//! - **Occurrences are stated one by one.** A part used four times is
//!   four relations with four placements, not one relation with a
//!   quantity of four, because the four sit in different places and a
//!   consumer comparing two assemblies has to see which one moved.
//!   [`Part::occurrences`] counts them so that nobody has to.
//! - **The tree is stated as relations.** A subassembly used twice
//!   appears once as a part and twice as a relation, rather than being
//!   copied into a nested structure that would then have to be
//!   de-duplicated by anything reading it.
//! - **Identity is what a person would call the part**: its number, its
//!   name, and its revision. Not its geometry, because a subassembly has
//!   none, and not the file's entity numbering, which changes on every
//!   export (ADR 0004).

use serde::{Deserialize, Serialize};

use crate::features::Units;
use crate::model::{Placement, Source};

/// Version of the product document. Independent of the PMI and features
/// documents: the three describe different things and will not change
/// together.
pub const SCHEMA_VERSION: u32 = 1;

/// What `pmix product` produces for one file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProductDocument {
    /// Schema version, see [`SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Where the data came from. Excluded from comparison.
    pub source: Source,
    /// The units the placements below are in, which are always the
    /// canonical ones. The file's own declared units are stated for
    /// provenance, not because anything here is in them.
    pub units: Units,
    /// How big the whole model is, in millimetres, with every
    /// occurrence put where it sits. Absent when nothing locates it.
    ///
    /// This is the assembly's size, which is not the same as any one
    /// body's: a body states its own shape in its own coordinates, and
    /// what turns fourteen of those into one box is the placements
    /// (ADR 0014).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub envelope: Option<crate::features::Envelope>,
    /// Every distinct part the file names, sorted by id.
    pub parts: Vec<Part>,
    /// Every use of one part inside another, sorted by id.
    pub relations: Vec<Relation>,
    /// Bodies the reader could not put under any part, sorted by body
    /// id. Empty for most files; never omitted, because a body that
    /// belongs to no part is a fact a consumer has to see rather than a
    /// body that quietly went missing.
    pub unattached: Vec<Unattached>,
    /// The parts nothing uses, sorted. A well-formed assembly has one;
    /// a file holding several unrelated parts has several, which is
    /// stated rather than resolved.
    pub roots: Vec<String>,
    /// Reader warnings. Excluded from comparison.
    pub diagnostics: Vec<Diagnostic>,
}

/// One part: a version of a product, as the file defines it.
///
/// A part is a *product definition*, not a product. Two revisions of one
/// design are two parts here, because a substitute for revision C is not
/// necessarily a substitute for revision A, and a document that merged
/// them could not say which one an assembly uses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Part {
    /// Content-derived, unique within the document.
    pub id: String,
    /// The part number: what the file calls the product, which for most
    /// writers is the identifier a person would search for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<String>,
    /// The product's name, when it states one distinct from its number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The product's description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The revision, from the product definition formation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    /// How many times this part is used, counting every occurrence
    /// anywhere in the file. Zero for a part nothing uses, which is
    /// either the assembly's root or a part the file states and never
    /// places.
    pub occurrences: usize,
    /// The bodies this part is made of, by their ids in the features
    /// document, sorted. Empty for a subassembly, which has parts rather
    /// than geometry.
    ///
    /// One body however many times the part is used: a body is a shape,
    /// and how often that shape appears is what [`Part::occurrences`]
    /// says. The two counts differ on purpose.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bodies: Vec<String>,
    /// Source entities. Excluded from comparison.
    pub source_refs: Vec<String>,
}

/// One use of one part inside another: a single occurrence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Relation {
    /// Content-derived, unique within the document.
    pub id: String,
    /// The part that contains, by its id.
    pub parent: String,
    /// The part contained, by its id.
    pub child: String,
    /// What this occurrence is called where it is used — a reference
    /// designator, a balloon number, or whatever the writer put there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Where the child sits inside the parent, in millimetres. Absent
    /// when the file states the usage without a transformation, which
    /// happens and is not an error.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<Placement>,
    /// Source entities. Excluded from comparison.
    pub source_refs: Vec<String>,
}

/// A body that went under no part, and why.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Unattached {
    /// The body's id in the features document.
    pub body: String,
    /// What stopped it being placed.
    pub reason: String,
}

/// Something the reader could not do, said out loud.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub message: String,
}

impl ProductDocument {
    /// Sort every array as the schema requires, and fill in the counts
    /// that depend on the whole document.
    ///
    /// Called once, after the reader has found everything. The counts
    /// live on [`Part`] rather than being left to a consumer because a
    /// language model asked how many of a component an assembly uses
    /// should not have to tally a list to answer (ADR 0013).
    pub fn settle(&mut self) {
        for part in &mut self.parts {
            part.occurrences = self.relations.iter().filter(|r| r.child == part.id).count();
        }
        self.roots = self
            .parts
            .iter()
            .filter(|p| p.occurrences == 0)
            .map(|p| p.id.clone())
            .collect();
        for part in &mut self.parts {
            part.bodies.sort();
        }
        self.parts.sort_by(|a, b| a.id.cmp(&b.id));
        self.relations.sort_by(|a, b| a.id.cmp(&b.id));
        self.unattached.sort_by(|a, b| a.body.cmp(&b.body));
        self.roots.sort();
    }
}
