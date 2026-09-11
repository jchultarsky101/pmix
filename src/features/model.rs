//! The document `pmix features` writes (ADR 0011, ADR 0012).
//!
//! This is base data for a comparison, not a comparison. Everything here
//! is stated so that something else — a reviewer, a language model, a
//! later tool — can say what changed between two of these documents
//! without having to guess at what a face is for.
//!
//! Three properties make that possible, and each is load-bearing:
//!
//! - **Parameters, not only identity.** A hole states its diameter and
//!   where it runs. "Ø4.5 became Ø5.0" cannot be recovered from two
//!   fingerprints, however good the reader of them is.
//! - **Every face is accounted for.** A face that went into no feature is
//!   listed as such, so that *not recognised* can never be mistaken for
//!   *not there*.
//! - **Canonical units and canonical placement.** Lengths are
//!   millimetres, angles degrees, and a feature's axis is stated in a
//!   fixed sign so that two exports of one design describe it the same
//!   way rather than in opposite directions.

use serde::{Deserialize, Serialize};

use crate::model::Source;

/// Version of the features document. Independent of the PMI document's
/// schema version: the two describe different things and will not change
/// together.
pub const SCHEMA_VERSION: u32 = 1;

/// What `pmix features` produces for one file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeatureDocument {
    /// Schema version, see [`SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Where the data came from. Excluded from comparison.
    pub source: Source,
    /// The units the numbers below are in, which are always the
    /// canonical ones. The file's own declared units are stated for
    /// provenance, not because anything here is in them.
    pub units: Units,
    /// One entry per solid the file contains, sorted by id.
    pub bodies: Vec<Body>,
    /// Reader warnings. Excluded from comparison.
    pub diagnostics: Vec<Diagnostic>,
}

/// The units a document's numbers are in, and the ones its file declared.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Units {
    /// Always `"mm"`.
    pub length: String,
    /// Always `"deg"`.
    pub angle: String,
    /// What the file itself declared, before conversion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub declared_length: Option<String>,
    /// What the file itself declared, before conversion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub declared_angle: Option<String>,
}

/// One solid, everything recognised in it, and everything not.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Body {
    /// Identity of the body, derived from the faces it is made of.
    pub id: String,
    /// A name the file gave the body, if any. Not part of the id: a name
    /// is a label a user can change without changing the shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// How many faces went into features and how many did not.
    pub faces: FaceCounts,
    /// How big the body is, in millimetres. Absent when the file locates
    /// nothing, as a tessellation-only export does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub envelope: Option<Envelope>,
    /// What was recognised, sorted by id.
    pub features: Vec<Feature>,
    /// Arrangements of those features that a rule recognised: bolt
    /// circles, rows, grids. Sorted by id, empty when nothing repeats.
    ///
    /// Derived, not read — no file says "bolt circle" — so each states
    /// the rule that produced it (ADR 0014).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub patterns: Vec<Pattern>,
    /// Every face that went into no feature, sorted by id. This is not a
    /// diagnostic: it is part of the answer.
    pub unassigned: Vec<UnassignedFace>,
}

/// The box around a body, and how big that makes it.
///
/// Stated in the body's own coordinates, so a part exported on its own
/// and the same part inside an assembly describe themselves the same
/// way; where the occurrence sits is the product document's business
/// (ADR 0014).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    /// The low corner of the axis-aligned box.
    pub min: [f64; 3],
    /// The high corner.
    pub max: [f64; 3],
    /// The box's three dimensions, largest first — the three numbers a
    /// catalogue asks for, in an order that does not depend on how the
    /// part happened to be oriented when it was exported.
    pub size: [f64; 3],
    /// Whether the box is a lower bound rather than the true extent.
    ///
    /// True when the body holds something that can reach past everything
    /// the file locates: an arc bulging beyond its own endpoints, a
    /// torus, or a face stated with no closed form. The box is then the
    /// smallest the body can be, not the size it is, and saying so is
    /// the difference between a measurement and a guess.
    pub approximate: bool,
}

/// How much of a body was recognised.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FaceCounts {
    pub total: usize,
    pub in_features: usize,
    pub unassigned: usize,
}

/// A face that no rule claimed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnassignedFace {
    /// Identity of the face, by the shared recipe (ADR 0004).
    pub id: String,
    /// What it lies on, named even when the file gives no closed form.
    pub surface: String,
}

/// What a rule recognised.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Feature {
    /// Identity of the feature, derived from the faces it is made of, so
    /// that two exports of one design name it the same way.
    pub id: String,
    pub kind: Kind,
    /// The faces this is made of, by their own ids, sorted.
    pub faces: Vec<String>,
    #[serde(flatten)]
    pub shape: Shape,
    /// Features sharing a face with this one. Where two readings of the
    /// same material are both defensible, both are reported and the
    /// overlap is stated rather than one being chosen (ADR 0011).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overlaps: Vec<String>,
    /// Features on the same axis as this one, nearest first: the wider
    /// stage of a counterbore names the hole it opens into.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub coaxial_with: Vec<String>,
}

/// The kinds of feature this recognises. Deliberately short: a kind is
/// here only when a local rule settles it (ADR 0011).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// A cylindrical wall with material outside it.
    Hole,
    /// A wider, coaxial cylinder opening into a hole.
    Counterbore,
    /// A coaxial cone opening into a hole.
    Countersink,
    /// A cylindrical wall with material inside it: a shaft, pin, or pad.
    Boss,
    /// A blend filling an inside corner, leaving the surface smooth
    /// where two faces would otherwise meet at an edge.
    Fillet,
    /// The same blend on an outside corner, rounding the edge off.
    /// Machinists call this a round, and it is worth telling from a
    /// fillet because the two look nothing alike on the part.
    Round,
    /// A cone cutting the corner off where two faces meet, leaving an
    /// edge rather than a smooth join.
    Chamfer,
}

impl Kind {
    /// The name used in ids and in text output.
    pub fn name(self) -> &'static str {
        match self {
            Self::Hole => "hole",
            Self::Counterbore => "counterbore",
            Self::Countersink => "countersink",
            Self::Boss => "boss",
            Self::Fillet => "fillet",
            Self::Round => "round",
            Self::Chamfer => "chamfer",
        }
    }
}

/// Where a feature is and how big it is, in millimetres and degrees.
///
/// A line in space is named by a direction and the point on it closest
/// to the model origin, rather than by one of its ends, because which
/// end is "first" is an accident of how the file was written. The extent
/// then says which stretch of that line the feature occupies, measured
/// along the same direction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shape {
    /// Diameter in millimetres. Two of these are what makes "Ø4.5 became
    /// Ø5.0" sayable. Bores, shafts, and chamfers state one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diameter: Option<f64>,
    /// Blend radius in millimetres. A fillet is always called by its
    /// radius and never by a diameter, so it states this instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radius: Option<f64>,
    /// How deep a bore goes, in millimetres.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<f64>,
    /// How far a blend runs, in millimetres. A fillet has a length, not
    /// a depth: it travels along an edge rather than into the material,
    /// and calling that a depth would invite the wrong comparison.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<f64>,
    /// Whether it opens at both ends. `None` when the rule could not
    /// tell, which is stated rather than defaulted to either answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub through: Option<bool>,
    /// The direction of the axis, in a fixed sign.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axis: Option<[f64; 3]>,
    /// The point on the axis closest to the model origin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<[f64; 3]>,
    /// The stretch of the axis occupied, measured along `axis` from the
    /// model origin. Together with `position` this locates the feature
    /// completely, including a move along its own axis, which `position`
    /// alone cannot show.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extent: Option<[f64; 2]>,
    /// The full angle at the apex of a cone, in degrees.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angle: Option<f64>,
}

/// An arrangement of alike features.
///
/// The question a substitute part has to pass is whether it bolts where
/// the old one bolted, and four separate hole positions do not answer
/// it. This does.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pattern {
    /// Identity, derived from the members, which are themselves
    /// content-derived (ADR 0004).
    pub id: String,
    pub kind: PatternKind,
    /// The features this is made of, by their own ids, sorted.
    pub features: Vec<String>,
    pub count: usize,
    /// The size the members share, in millimetres.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diameter: Option<f64>,
    /// The direction the members run in, which they all share.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axis: Option<[f64; 3]>,
    /// A bolt circle's pitch circle diameter, in millimetres: the
    /// number a catalogue states it by.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pitch_circle_diameter: Option<f64>,
    /// Where a bolt circle turns about.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub centre: Option<[f64; 3]>,
    /// Where the first hole of a bolt circle sits, in degrees. Two
    /// flanges with the same pitch circle and different clocking do not
    /// interchange, so it is stated rather than left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clocking: Option<f64>,
    /// The spacing, in millimetres: one for a row, two for a grid.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pitch: Vec<f64>,
    /// How many in each direction.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub counts: Vec<usize>,
    /// What settled it. A recognition states its rule, so that a reader
    /// can weigh it rather than take it (ADR 0011).
    pub rule: String,
    /// Patterns sharing a feature with this one. Four holes at the
    /// corners of a square are a grid and a bolt circle, and both
    /// readings are stated rather than one being chosen.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overlaps: Vec<String>,
}

impl Pattern {
    /// A pattern with nothing filled in but its kind, for a recogniser
    /// to build on.
    pub(crate) fn empty() -> Self {
        Self {
            id: String::new(),
            kind: PatternKind::Row,
            features: Vec::new(),
            count: 0,
            diameter: None,
            axis: None,
            pitch_circle_diameter: None,
            centre: None,
            clocking: None,
            pitch: Vec::new(),
            counts: Vec::new(),
            rule: String::new(),
            overlaps: Vec::new(),
        }
    }
}

/// The arrangements this recognises.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatternKind {
    /// Alike features on one circle at an even angular pitch — a bolt
    /// circle, in the sense a flange has one.
    BoltCircle,
    /// Alike features on one line at an even pitch.
    Row,
    /// Alike features on a filled rectangular lattice.
    Grid,
}

impl PatternKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::BoltCircle => "bolt_circle",
            Self::Row => "row",
            Self::Grid => "grid",
        }
    }
}

/// Something the reader could not do, said out loud.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub message: String,
}
