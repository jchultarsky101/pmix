//! The PMI Manager element (specification section 8.3).
//!
//! A PMI segment holds one PMI Manager element whose fields are laid out
//! in sequence, so everything before the entities has to be parsed to
//! reach them: design groups, associations, user attributes, the string
//! table, and model views.
//!
//! Two details differ from the specification's figures and were taken
//! from real files instead: a property atom's hidden flag is one byte
//! rather than four, and the text of an entity is referenced by index
//! into the string table.

use std::fmt;

use super::codec::{Cursor, Predictor};
use super::file::Guid;

/// Object type identifier of the PMI Manager element.
pub const PMI_MANAGER: Guid = Guid([
    0x49, 0x72, 0x35, 0xce, 0xfb, 0x38, 0xd1, 0x11, 0xa5, 0x06, 0x00, 0x60, 0x97, 0xbd, 0xc6, 0xe1,
]);

/// A malformed PMI element.
#[derive(Debug, Clone, PartialEq)]
pub struct PmiError {
    pub offset: usize,
    pub message: String,
}

impl fmt::Display for PmiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PMI element at byte {}: {}", self.offset, self.message)
    }
}

impl std::error::Error for PmiError {}

type Result<T> = std::result::Result<T, PmiError>;

/// What a PMI entity is (specification table 56).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntityKind {
    Pmi,
    Weld,
    SpotWeld,
    LineWeld,
    GrooveWeld,
    FilletWeld,
    SlotWeld,
    EdgeWeld,
    ArcSpotWeld,
    ResistanceSpotWeld,
    ResistanceSeamWeld,
    StructuralAdhesiveBead,
    StructuralAdhesiveTape,
    StructuralAdhesiveDollop,
    MechanicalClinchConnector,
    SurfaceFinish,
    MeasurementPoint,
    DatumLocator,
    CertificationPoint,
    GeometricTolerancing,
    FeatureControlFrame,
    Dimension,
    DatumFeatureSymbol,
    DatumTarget,
    Note,
    FaceAttributeNote,
    ModelViewLabelNote,
    CoordinateSystem,
    ReferenceGeometry,
    ReferencePoint,
    ReferenceAxis,
    ReferencePlane,
    UserDefined,
    MeasurementLocator,
    DatumPoint,
    SurfaceVectorMeasurementPoint,
    HoleVectorMeasurementPoint,
    TrimmedSheetVectorMeasurementPoint,
    HemVectorMeasurementPoint,
    FastenerPmi,
    MaterialSpecification,
    ProcessSpecification,
    PartSpecification,
    BalloonNote,
    CircleCentre,
    CoordinateNote,
    AttributeNote,
    BundleOrDressingNote,
    CuttingPlaneSymbol,
    Crosshatch,
    EMarking,
    Organization,
    Region,
    Section,
    Centreline,
    FitDesignation,
    CompositeFeatureControlFrame,
    WeldNote,
    ReferenceCircle,
    ReferenceCylinder,
    PartTransform,
    CalloutDimension,
    ParameterDimension,
    ChamferDimension,
    ModelViewStyle,
    PmiTable,
    ParameterFitDesignation,
    CalloutFitDesignation,
    Feature,
    FeatureThread,
    FeatureArcWeld,
    FeatureDatum,
    FeatureDiscreteJoin,
    FeatureResistanceWeld,
    FeatureContinuousJoin,
    FeatureAdhesiveFill,
    FeatureSurfaceWeld,
    FeatureMeasurementLocator,
    FeatureGroupedWeld,
    FeatureLaserWeld,
    /// A code the specification does not list, kept so nothing is lost.
    Other(u16),
}

impl EntityKind {
    /// Map a specification entity type code.
    pub fn from_code(code: u16) -> Self {
        match code {
            0x0001 => Self::Pmi,
            0x0002 => Self::Weld,
            0x0004 => Self::SpotWeld,
            0x0008 => Self::LineWeld,
            0x0010 => Self::GrooveWeld,
            0x0011 => Self::FilletWeld,
            0x0012 => Self::SlotWeld,
            0x0014 => Self::EdgeWeld,
            0x0018 => Self::ArcSpotWeld,
            0x0020 => Self::ResistanceSpotWeld,
            0x0021 => Self::ResistanceSeamWeld,
            0x0022 => Self::StructuralAdhesiveBead,
            0x0024 => Self::StructuralAdhesiveTape,
            0x0028 => Self::StructuralAdhesiveDollop,
            0x0040 => Self::MechanicalClinchConnector,
            0x0041 => Self::SurfaceFinish,
            0x0042 => Self::MeasurementPoint,
            0x0044 => Self::DatumLocator,
            0x0048 => Self::CertificationPoint,
            0x0080 => Self::GeometricTolerancing,
            0x0081 => Self::FeatureControlFrame,
            0x0082 => Self::Dimension,
            0x0084 => Self::DatumFeatureSymbol,
            0x0088 => Self::DatumTarget,
            0x0100 => Self::Note,
            0x0101 => Self::FaceAttributeNote,
            0x0102 => Self::ModelViewLabelNote,
            0x0104 => Self::CoordinateSystem,
            0x0108 => Self::ReferenceGeometry,
            0x0110 => Self::ReferencePoint,
            0x0111 => Self::ReferenceAxis,
            0x0112 => Self::ReferencePlane,
            0x0114 => Self::UserDefined,
            0x0118 => Self::MeasurementLocator,
            0x0120 => Self::DatumPoint,
            0x0121 => Self::SurfaceVectorMeasurementPoint,
            0x0122 => Self::HoleVectorMeasurementPoint,
            0x0124 => Self::TrimmedSheetVectorMeasurementPoint,
            0x0128 => Self::HemVectorMeasurementPoint,
            0x0230 => Self::FastenerPmi,
            0x0231 => Self::MaterialSpecification,
            0x0232 => Self::ProcessSpecification,
            0x0233 => Self::PartSpecification,
            0x0235 => Self::BalloonNote,
            0x0238 => Self::CircleCentre,
            0x0239 => Self::CoordinateNote,
            0x0240 => Self::AttributeNote,
            0x0241 => Self::BundleOrDressingNote,
            0x0242 => Self::CuttingPlaneSymbol,
            0x0243 => Self::Crosshatch,
            0x0244 => Self::EMarking,
            0x0245 => Self::Organization,
            0x0246 => Self::Region,
            0x0305 => Self::Section,
            0x0306 => Self::Centreline,
            0x0307 => Self::FitDesignation,
            0x0308 => Self::CompositeFeatureControlFrame,
            0x0309 => Self::WeldNote,
            0x030A => Self::ReferenceCircle,
            0x030B => Self::ReferenceCylinder,
            0x030C => Self::PartTransform,
            0x030D => Self::CalloutDimension,
            0x030E => Self::ParameterDimension,
            0x030F => Self::ChamferDimension,
            0x0310 => Self::ModelViewStyle,
            0x0311 => Self::PmiTable,
            0x0312 => Self::ParameterFitDesignation,
            0x0313 => Self::CalloutFitDesignation,
            0x8000 => Self::Feature,
            0x8001 => Self::FeatureThread,
            0x8002 => Self::FeatureArcWeld,
            0x8003 => Self::FeatureDatum,
            0x8004 => Self::FeatureDiscreteJoin,
            0x8005 => Self::FeatureResistanceWeld,
            0x8006 => Self::FeatureContinuousJoin,
            0x8007 => Self::FeatureAdhesiveFill,
            0x8008 => Self::FeatureSurfaceWeld,
            0x8009 => Self::FeatureMeasurementLocator,
            0x800A => Self::FeatureGroupedWeld,
            0x800B => Self::FeatureLaserWeld,
            other => Self::Other(other),
        }
    }

    /// Name used in output.
    pub fn as_str(&self) -> String {
        match self {
            Self::Pmi => "PMI".into(),
            Self::Weld => "weld".into(),
            Self::SpotWeld => "spot weld".into(),
            Self::LineWeld => "line weld".into(),
            Self::GrooveWeld => "groove weld".into(),
            Self::FilletWeld => "fillet weld".into(),
            Self::SlotWeld => "slot weld".into(),
            Self::EdgeWeld => "edge weld".into(),
            Self::ArcSpotWeld => "arc spot weld".into(),
            Self::ResistanceSpotWeld => "resistance spot weld".into(),
            Self::ResistanceSeamWeld => "resistance seam weld".into(),
            Self::StructuralAdhesiveBead => "structural adhesive bead".into(),
            Self::StructuralAdhesiveTape => "structural adhesive tape".into(),
            Self::StructuralAdhesiveDollop => "structural adhesive dollop".into(),
            Self::MechanicalClinchConnector => "mechanical clinch connector".into(),
            Self::SurfaceFinish => "surface finish".into(),
            Self::MeasurementPoint => "measurement point".into(),
            Self::DatumLocator => "datum locator".into(),
            Self::CertificationPoint => "certification point".into(),
            Self::GeometricTolerancing => "geometric tolerancing".into(),
            Self::FeatureControlFrame => "feature control frame".into(),
            Self::Dimension => "dimension".into(),
            Self::DatumFeatureSymbol => "datum feature symbol".into(),
            Self::DatumTarget => "datum target".into(),
            Self::Note => "note".into(),
            Self::FaceAttributeNote => "face attribute note".into(),
            Self::ModelViewLabelNote => "model view label note".into(),
            Self::CoordinateSystem => "coordinate system".into(),
            Self::ReferenceGeometry => "reference geometry".into(),
            Self::ReferencePoint => "reference point".into(),
            Self::ReferenceAxis => "reference axis".into(),
            Self::ReferencePlane => "reference plane".into(),
            Self::UserDefined => "user defined".into(),
            Self::MeasurementLocator => "measurement locator".into(),
            Self::DatumPoint => "datum point".into(),
            Self::SurfaceVectorMeasurementPoint => "surface vector measurement point".into(),
            Self::HoleVectorMeasurementPoint => "hole vector measurement point".into(),
            Self::TrimmedSheetVectorMeasurementPoint => {
                "trimmed sheet vector measurement point".into()
            }
            Self::HemVectorMeasurementPoint => "hem vector measurement point".into(),
            Self::FastenerPmi => "fastener PMI".into(),
            Self::MaterialSpecification => "material specification".into(),
            Self::ProcessSpecification => "process specification".into(),
            Self::PartSpecification => "part specification".into(),
            Self::BalloonNote => "balloon note".into(),
            Self::CircleCentre => "circle centre".into(),
            Self::CoordinateNote => "coordinate note".into(),
            Self::AttributeNote => "attribute note".into(),
            Self::BundleOrDressingNote => "bundle or dressing note".into(),
            Self::CuttingPlaneSymbol => "cutting plane symbol".into(),
            Self::Crosshatch => "crosshatch".into(),
            Self::EMarking => "e marking".into(),
            Self::Organization => "organization".into(),
            Self::Region => "region".into(),
            Self::Section => "section".into(),
            Self::Centreline => "centreline".into(),
            Self::FitDesignation => "fit designation".into(),
            Self::CompositeFeatureControlFrame => "composite feature control frame".into(),
            Self::WeldNote => "weld note".into(),
            Self::ReferenceCircle => "reference circle".into(),
            Self::ReferenceCylinder => "reference cylinder".into(),
            Self::PartTransform => "part transform".into(),
            Self::CalloutDimension => "callout dimension".into(),
            Self::ParameterDimension => "parameter dimension".into(),
            Self::ChamferDimension => "chamfer dimension".into(),
            Self::ModelViewStyle => "model view style".into(),
            Self::PmiTable => "PMI table".into(),
            Self::ParameterFitDesignation => "parameter fit designation".into(),
            Self::CalloutFitDesignation => "callout fit designation".into(),
            Self::Feature => "feature".into(),
            Self::FeatureThread => "feature thread".into(),
            Self::FeatureArcWeld => "feature arc weld".into(),
            Self::FeatureDatum => "feature datum".into(),
            Self::FeatureDiscreteJoin => "feature discrete join".into(),
            Self::FeatureResistanceWeld => "feature resistance weld".into(),
            Self::FeatureContinuousJoin => "feature continuous join".into(),
            Self::FeatureAdhesiveFill => "feature adhesive fill".into(),
            Self::FeatureSurfaceWeld => "feature surface weld".into(),
            Self::FeatureMeasurementLocator => "feature measurement locator".into(),
            Self::FeatureGroupedWeld => "feature grouped weld".into(),
            Self::FeatureLaserWeld => "feature laser weld".into(),
            Self::Other(c) => format!("type 0x{c:04x}"),
        }
    }
}

impl fmt::Display for EntityKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_str())
    }
}

/// One PMI entity.
#[derive(Debug, Clone, PartialEq)]
pub struct Entity {
    pub kind: EntityKind,
    /// The producing system's identifier for the entity.
    pub user_label: i32,
    /// Text shown on the annotation, resolved through the string table.
    ///
    /// Producing systems may write a glyph run here rather than readable
    /// text, in which case the strings are symbol indices and the shape
    /// of the text is carried by [`Entity::text_polylines`] instead.
    pub texts: Vec<String>,
    /// The lines that draw the annotation: its frame, leaders, and
    /// symbols, in world coordinates.
    pub polylines: Vec<Vec<[f64; 3]>>,
    /// The lines that draw the annotation's text.
    pub text_polylines: Vec<Vec<[f64; 3]>>,
    /// Key and value pairs describing the entity.
    pub properties: Vec<(String, String)>,
    /// The producing system's name for the entity type, if it gave one.
    pub type_name: Option<String>,
    /// Whether the producing system marked the entity valid.
    pub valid: bool,
}

impl Entity {
    /// The value stored under `key`, if the producing system wrote one.
    pub fn property(&self, key: &str) -> Option<&str> {
        self.properties
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// [`Entity::property`] parsed as a number.
    pub fn number(&self, key: &str) -> Option<f64> {
        self.property(key)?.trim().parse().ok()
    }

    /// [`Entity::property`] read as a flag, where anything but `0` is set.
    pub fn flag(&self, key: &str) -> bool {
        self.property(key).is_some_and(|v| v.trim() != "0")
    }

    /// Every property whose key ends with `suffix`, in key order, paired
    /// with the part of the key before it.
    pub fn ending_with<'a>(&'a self, suffix: &str) -> Vec<(&'a str, &'a str)> {
        let mut found: Vec<_> = self
            .properties
            .iter()
            .filter_map(|(k, v)| k.strip_suffix(suffix).map(|p| (p, v.as_str())))
            .collect();
        found.sort();
        found
    }
}

/// One end of an association: what kind of thing it names, and which one.
///
/// The specification packs both into a single integer: the low 24 bits
/// are the identifier and the next 7 say what it identifies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndPoint {
    /// Entity type code, from specification table 52.
    pub kind: u8,
    /// Index into the array of that kind, or a CAD tag when `indirect`.
    pub index: u32,
    /// Whether `index` names a CAD tag rather than a position.
    pub indirect: bool,
}

impl EndPoint {
    /// Type code of a B-rep vertex.
    pub const VERTEX: u8 = 14;
    /// Type code of a B-rep edge.
    pub const EDGE: u8 = 15;
    /// Type code of a B-rep face.
    pub const FACE: u8 = 16;
    /// Type code of a PMI model view.
    pub const MODEL_VIEW: u8 = 17;
    /// Type code of a generic PMI entity.
    pub const GENERIC: u8 = 18;

    /// Whether this end names a piece of a part's B-rep rather than an
    /// annotation.
    pub fn is_brep(&self) -> bool {
        matches!(self.kind, Self::VERTEX | Self::EDGE | Self::FACE)
    }

    fn unpack(raw: i32) -> Self {
        let raw = raw as u32;
        Self {
            kind: ((raw >> 24) & 0x7f) as u8,
            index: raw & 0x00ff_ffff,
            indirect: raw & 0x8000_0000 != 0,
        }
    }
}

/// A link between two entities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Association {
    pub source: EndPoint,
    pub destination: EndPoint,
    /// Why the two are linked, from specification table 53.
    pub reason: i32,
}

impl Association {
    /// The association that puts a PMI entity in a model view.
    pub const SHOWN_IN_VIEW: i32 = 98;
}

/// A saved view and the camera that frames it.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelView {
    pub id: i32,
    pub name: Option<String>,
    pub active: bool,
    /// The direction the camera looks along.
    pub eye_direction: [f64; 3],
    /// Rotation about the eye direction, in degrees.
    pub angle: f64,
    /// Where the camera looks from, in world coordinates.
    pub eye_position: [f64; 3],
    /// Where the camera looks at, in world coordinates.
    pub target: [f64; 3],
    /// Diameter of the largest circle the viewport can inscribe, which
    /// is how JT states the zoom level.
    pub viewport_diameter: f64,
}

/// The contents of a PMI Manager element.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PmiManager {
    pub version: u8,
    pub strings: Vec<String>,
    pub associations: Vec<Association>,
    pub model_views: Vec<ModelView>,
    pub entities: Vec<Entity>,
    /// How many design groups the element declares. `pmix` does not model
    /// them, but they take positions in [`PmiManager::cad_tag_index`].
    pub design_groups: usize,
    /// Where each thing in the element has its CAD tag, in the order the
    /// specification gives: model views, then design groups, then generic
    /// entities. The values are positions in [`PmiManager::cad_tags`],
    /// and an association names its ends by one of those positions, so
    /// this is what resolves them.
    pub cad_tag_index: Vec<i32>,
    /// The persistent identifiers the originating system gave its
    /// entities, PMI and B-rep alike. This is the list an association
    /// end indexes into when it names a face or an edge.
    pub cad_tags: Vec<i64>,
}

impl PmiManager {
    /// What the thing at position `tag` in the CAD tag list is, or
    /// `None` if it is not one of the PMI entities this reader models.
    pub fn resolve(&self, tag: i32) -> Option<Tagged> {
        let position = self.cad_tag_index.iter().position(|t| *t == tag)?;
        if position < self.model_views.len() {
            return Some(Tagged::ModelView(position));
        }
        let position = position - self.model_views.len();
        if position < self.design_groups {
            return Some(Tagged::DesignGroup);
        }
        let position = position - self.design_groups;
        (position < self.entities.len()).then_some(Tagged::Entity(position))
    }
}

impl PmiManager {
    /// The tag the originating system knows this end's entity by.
    ///
    /// An end names its entity either by position or, as every file seen
    /// so far does for B-rep, by a place in the tag list. A face's tag is
    /// what the topology table's attributes carry, so this is the step
    /// from an annotation to the geometry it applies to.
    pub fn tag_of(&self, end: EndPoint) -> Option<i64> {
        end.indirect
            .then(|| self.cad_tags.get(end.index as usize).copied())
            .flatten()
            .filter(|t| *t != i64::MIN)
    }

    /// The faces of a part's B-rep that `entity` is associated with, as
    /// the tags that name them.
    ///
    /// `entity` is a position in [`PmiManager::entities`].
    pub fn faces_of(&self, entity: usize) -> Vec<u32> {
        let mut found: Vec<u32> = self
            .associations
            .iter()
            .filter_map(|a| {
                let (named, face) = match (a.source.kind, a.destination.kind) {
                    (EndPoint::GENERIC, EndPoint::FACE) => (a.source, a.destination),
                    (EndPoint::FACE, EndPoint::GENERIC) => (a.destination, a.source),
                    _ => return None,
                };
                if self.resolve(named.index as i32) != Some(Tagged::Entity(entity)) {
                    return None;
                }
                // A tag is written as 32 bits in every file seen so far,
                // and a face tag that did not fit would name the wrong
                // face rather than none, so it is dropped.
                u32::try_from(self.tag_of(face)?).ok()
            })
            .collect();
        found.sort_unstable();
        found.dedup();
        found
    }
}

/// What a CAD tag names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tagged {
    /// A position in [`PmiManager::model_views`].
    ModelView(usize),
    /// A position in [`PmiManager::entities`].
    Entity(usize),
    /// A design group, which `pmix` does not model.
    DesignGroup,
}

/// Cursor over the element's bytes.
struct Reader<'a> {
    d: &'a [u8],
    p: usize,
}

impl<'a> Reader<'a> {
    fn new(d: &'a [u8]) -> Self {
        Self { d, p: 0 }
    }

    fn err<T>(&self, message: impl Into<String>) -> Result<T> {
        Err(PmiError {
            offset: self.p,
            message: message.into(),
        })
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        match self.d.get(self.p..self.p + n) {
            Some(s) => {
                self.p += n;
                Ok(s)
            }
            None => self.err(format!(
                "wanted {n} bytes, {} remain",
                self.d.len().saturating_sub(self.p)
            )),
        }
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn skip(&mut self, n: usize) -> Result<()> {
        self.take(n).map(|_| ())
    }

    /// A count that must be plausible for the bytes that remain, so a
    /// misread offset fails here rather than allocating wildly.
    fn count(&mut self, per_item: usize) -> Result<usize> {
        let n = self.i32()?;
        if n < 0 {
            return self.err(format!("negative count {n}"));
        }
        let n = n as usize;
        let remaining = self.d.len().saturating_sub(self.p);
        if per_item > 0 && n.saturating_mul(per_item) > remaining {
            return self.err(format!(
                "count {n} needs at least {} bytes, {remaining} remain",
                n * per_item
            ));
        }
        Ok(n)
    }

    /// An `MbString`: a count of UTF-16 code units, then the units.
    fn string(&mut self) -> Result<String> {
        let n = self.count(2)?;
        let bytes = self.take(n * 2)?;
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        Ok(String::from_utf16_lossy(&units))
    }

    /// A property atom: a string followed by a one-byte hidden flag.
    fn atom(&mut self) -> Result<String> {
        let value = self.string()?;
        self.u8()?;
        Ok(value)
    }

    /// A `VecI32` or `VecF32`, skipped: a count of values then the values.
    fn skip_vec(&mut self) -> Result<usize> {
        let n = self.count(4)?;
        self.skip(n * 4)?;
        Ok(n)
    }

    /// A `VecI32` read for its values.
    fn ints(&mut self) -> Result<Vec<i32>> {
        let n = self.count(4)?;
        let bytes = self.take(n * 4)?;
        Ok(bytes
            .chunks_exact(4)
            .map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect())
    }

    fn f32(&mut self) -> Result<f64> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()) as f64)
    }

    /// Three 32-bit floats: a point or a direction.
    fn point(&mut self) -> Result<[f64; 3]> {
        Ok([self.f32()?, self.f32()?, self.f32()?])
    }

    /// A `VecF32` read as points. Generic PMI entities pack their
    /// coordinates as XYZ triples; a count that is not a whole number of
    /// points means the data is not what this reader expects, so it
    /// yields nothing rather than a misaligned reading.
    fn points(&mut self) -> Result<Vec<[f64; 3]>> {
        let n = self.count(4)?;
        let bytes = self.take(n * 4)?;
        if n % 3 != 0 {
            return Ok(Vec::new());
        }
        Ok(bytes
            .chunks_exact(12)
            .map(|c| {
                let f = |i: usize| f32::from_le_bytes([c[i], c[i + 1], c[i + 2], c[i + 3]]) as f64;
                [f(0), f(4), f(8)]
            })
            .collect())
    }
}

/// Cut a run of vertices into polylines at the given break points.
///
/// The specification packs every polyline of an entity into one array of
/// vertices and gives the index each one starts at, so consecutive breaks
/// delimit a polyline. A break that runs past the vertices is ignored.
fn split(breaks: &[usize], vertices: &[[f64; 3]]) -> Vec<Vec<[f64; 3]>> {
    breaks
        .windows(2)
        .filter_map(|w| {
            let (start, end) = (w[0], w[1].min(vertices.len()));
            (start < end).then(|| vertices[start..end].to_vec())
        })
        .filter(|p| p.len() > 1)
        .collect()
}

/// Parse the object data of a PMI Manager element.
pub fn parse(data: &[u8]) -> Result<PmiManager> {
    let mut r = Reader::new(data);
    let version = r.u8()?;
    r.skip(2)?; // empty field

    // Design groups, which pmix does not model but must be stepped over
    // and counted, because they take positions in the CAD tag order.
    let design_groups = r.count(8)?;
    for _ in 0..design_groups {
        r.skip(4)?;
        for _ in 0..r.count(12)? {
            match r.i32()? {
                1 => r.skip(4)?,
                2 => r.skip(8)?,
                3 => r.skip(4)?,
                _ => {}
            }
            r.skip(8)?;
        }
    }

    let mut associations = Vec::new();
    for _ in 0..r.count(20)? {
        let source = EndPoint::unpack(r.i32()?);
        r.skip(4)?; // the string naming the component that owns the source
        let reason = r.i32()?;
        let destination = EndPoint::unpack(r.i32()?);
        r.skip(4)?; // and the one that owns the destination
        associations.push(Association {
            source,
            destination,
            reason,
        });
    }

    for _ in 0..r.count(8)? {
        r.skip(8)?; // user attribute: key and value string identifiers
    }

    let string_count = r.count(4)?;
    let mut strings = Vec::with_capacity(string_count.min(4096));
    for _ in 0..string_count {
        strings.push(r.string()?);
    }
    let text_of = |id: u32, strings: &[String]| -> Option<String> {
        (id != u32::MAX)
            .then(|| strings.get(id as usize).cloned())
            .flatten()
    };

    let mut model_views = Vec::new();
    for _ in 0..r.count(60)? {
        let eye_direction = r.point()?;
        let angle = r.f32()?;
        let eye_position = r.point()?;
        let target = r.point()?;
        r.skip(12)?; // the model's rotation angles, which the camera implies
        let viewport_diameter = r.f32()?;
        r.skip(8)?; // empty fields
        let active = r.i32()? != 0;
        let id = r.i32()?;
        let name = text_of(r.u32()?, &strings);
        for _ in 0..r.count(2)? {
            r.atom()?;
            r.atom()?;
        }
        model_views.push(ModelView {
            id,
            name,
            active,
            eye_direction,
            angle,
            eye_position,
            target,
            viewport_diameter,
        });
    }

    let entity_count = r.count(20)?;
    let mut entities = Vec::with_capacity(entity_count.min(4096));
    for _ in 0..entity_count {
        let user_label = r.i32()?;
        if r.u8()? != 0 {
            r.skip(36)?; // 2D reference frame
        }
        r.skip(4)?; // text height
        let valid = r.u8()? != 0;

        let mut texts = Vec::new();
        let mut text_polylines = Vec::new();
        for _ in 0..r.count(4)? {
            let id = r.u32()?;
            r.skip(12 + 24)?; // font, empty fields, text box
            let count = r.count(2)?;
            let mut breaks = Vec::with_capacity(count.min(4096));
            for _ in 0..count {
                breaks.push(r.u16()? as usize);
            }
            text_polylines.extend(split(&breaks, &r.points()?));
            if let Some(t) = text_of(id, &strings) {
                texts.push(t);
            }
        }

        // Non-text polylines: segment indices, types, widths, coordinates.
        let count = r.count(4)?;
        let mut breaks = Vec::with_capacity(count.min(4096));
        for _ in 0..count {
            breaks.push(r.i32()?.max(0) as usize);
        }
        let n = r.count(2)?;
        r.skip(n * 2)?; // the line type of each run
        let n = r.count(2)?;
        r.skip(n * 2)?; // and its width
        let polylines = split(&breaks, &r.points()?);

        let mut properties = Vec::new();
        for _ in 0..r.count(2)? {
            let key = r.atom()?;
            let value = r.atom()?;
            properties.push((key, value));
        }

        let type_name = text_of(r.u32()?, &strings);
        r.skip(4)?; // parent type name identifier
        let kind = EntityKind::from_code(r.u16()?);
        r.skip(4)?; // parent type and user flags

        entities.push(Entity {
            kind,
            user_label,
            texts,
            polylines,
            text_polylines,
            properties,
            type_name,
            valid,
        });
    }

    // Polygon data has to be stepped over to reach the CAD tags. A file
    // that ends or malforms here still yields its PMI: the tags only
    // resolve associations, so losing them costs the view membership and
    // the B-rep an annotation applies to, and nothing else.
    let cad_tag_index = read_cad_tags(&mut r).unwrap_or_default();
    let cad_tags = read_cad_tag_pool(&mut r).unwrap_or_default();

    Ok(PmiManager {
        version,
        strings,
        associations,
        model_views,
        entities,
        design_groups,
        cad_tag_index,
        cad_tags,
    })
}

/// Read the pool of persistent identifiers that follows the per-entity
/// indices (specification section 10.2.16).
///
/// A tag is either 32 or 64 bits wide and the two are stored in separate
/// vectors, so the type vector says which of the two each tag comes
/// from, in order.
fn read_cad_tag_pool(r: &mut Reader<'_>) -> Result<Vec<i64>> {
    r.u8()?; // version number
    r.i32()?; // the length in bytes, for a reader that wants to skip it
    r.i32()?; // version number
    let mut cursor = Cursor::new(&r.d[r.p..]);
    let mut packet = |predictor| {
        cursor
            .packet(predictor)
            .map_err(|e| PmiError {
                offset: 0,
                message: e.to_string(),
            })
            .map(|v| v.into_iter().map(i64::from).collect::<Vec<i64>>())
    };
    let types = packet(Predictor::None)?;
    let narrow = if types.contains(&1) {
        packet(Predictor::None)?
    } else {
        Vec::new()
    };
    // Sixty-four bit tags would need the Int64 codec, which no file has
    // asked for. Their positions are kept so the narrow tags after them
    // stay at the positions an association names.
    let wide = types.iter().filter(|t| **t == 2).count();
    let (mut from_narrow, mut out) = (0, Vec::with_capacity(types.len().min(1 << 16)));
    for kind in &types {
        match kind {
            1 => {
                let Some(tag) = narrow.get(from_narrow) else {
                    break;
                };
                from_narrow += 1;
                out.push(*tag);
            }
            // Nothing can be said about a tag that was not read, and a
            // wrong tag is worse than a missing one.
            _ => out.push(i64::MIN),
        }
    }
    if wide > 0 {
        tracing::debug!("{wide} CAD tags are 64 bit and were not read");
    }
    Ok(out)
}

/// Step over the PMI polygon data and read the CAD tag order after it.
fn read_cad_tags(r: &mut Reader<'_>) -> Result<Vec<i32>> {
    r.u8()?; // version number
    let elements = r.count(4)?;
    let vertex_counts = r.ints()?;
    let bindings = r.ints()?;
    r.skip_vec()?; // the dimension of each polygon
    // Bindings are written for the non-empty elements only, three each,
    // so they are counted separately from the elements themselves.
    let mut filled = 0;
    for vertices in vertex_counts.iter().take(elements) {
        if *vertices <= 0 {
            continue;
        }
        r.skip_vec()?; // primitive types
        r.skip_vec()?; // primitive indices
        r.skip_vec()?; // vertex indices
        r.skip_vec()?; // vertices
        // Three bindings per non-empty element: colour, normal, texture.
        let binding = |which: usize| bindings.get(filled * 3 + which).copied().unwrap_or(0);
        let (colour, normal, texture) = (binding(0), binding(1), binding(2));
        filled += 1;
        if normal == 1 {
            r.skip_vec()?; // normals
        }
        if colour == 1 {
            r.skip_vec()?; // colours
        }
        if texture == 1 {
            r.skip_vec()?; // texture coordinates
        }
    }

    if r.u32()? != 1 {
        return Ok(Vec::new()); // the element carries no CAD tags
    }
    r.ints()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_kinds_follow_the_specification_table() {
        assert_eq!(EntityKind::from_code(0x0082), EntityKind::Dimension);
        assert_eq!(
            EntityKind::from_code(0x0081),
            EntityKind::FeatureControlFrame
        );
        assert_eq!(EntityKind::from_code(0x0100), EntityKind::Note);
        assert_eq!(EntityKind::from_code(0x0999), EntityKind::Other(0x0999));
        assert_eq!(EntityKind::Other(0x0305).as_str(), "type 0x0305");
    }

    #[test]
    fn a_wild_count_fails_instead_of_allocating() {
        // A version byte, an empty field, then a design group count that
        // no plausible file could satisfy.
        let mut data = vec![2u8, 0, 0];
        data.extend(i32::MAX.to_le_bytes());
        let err = parse(&data).unwrap_err();
        assert!(err.message.contains("count"), "{}", err.message);
    }

    #[test]
    fn an_association_end_unpacks_into_its_kind_and_index() {
        // A generic entity, index 5, named directly.
        let direct = EndPoint::unpack((EndPoint::GENERIC as i32) << 24 | 5);
        assert_eq!(direct.kind, EndPoint::GENERIC);
        assert_eq!(direct.index, 5);
        assert!(!direct.indirect);
        // A model view, index 63, named by CAD tag.
        let tagged =
            EndPoint::unpack(((EndPoint::MODEL_VIEW as u32) << 24 | 63 | 0x8000_0000) as i32);
        assert_eq!(tagged.kind, EndPoint::MODEL_VIEW);
        assert_eq!(tagged.index, 63);
        assert!(tagged.indirect);
    }

    #[test]
    fn a_cad_tag_names_a_view_then_a_group_then_an_entity() {
        let manager = PmiManager {
            model_views: vec![
                ModelView {
                    id: 1,
                    name: None,
                    active: false,
                    eye_direction: [0.0, 0.0, 1.0],
                    angle: 0.0,
                    eye_position: [0.0; 3],
                    target: [0.0; 3],
                    viewport_diameter: 0.0,
                };
                2
            ],
            design_groups: 1,
            entities: vec![
                Entity {
                    kind: EntityKind::Dimension,
                    user_label: 7,
                    texts: Vec::new(),
                    polylines: Vec::new(),
                    text_polylines: Vec::new(),
                    properties: Vec::new(),
                    type_name: None,
                    valid: true,
                };
                2
            ],
            // Where each thing has its tag, in the order the
            // specification gives, but pointing into the tag list
            // arbitrarily, which is why a lookup is needed at all.
            cad_tag_index: vec![40, 12, 99, 3, 71],
            ..Default::default()
        };
        assert_eq!(manager.resolve(40), Some(Tagged::ModelView(0)));
        assert_eq!(manager.resolve(12), Some(Tagged::ModelView(1)));
        assert_eq!(manager.resolve(99), Some(Tagged::DesignGroup));
        assert_eq!(manager.resolve(3), Some(Tagged::Entity(0)));
        assert_eq!(manager.resolve(71), Some(Tagged::Entity(1)));
        assert_eq!(manager.resolve(1000), None);
    }

    #[test]
    fn an_end_that_names_a_face_gives_up_its_tag() {
        let manager = PmiManager {
            cad_tags: vec![101, 202, 303],
            ..Default::default()
        };
        let face = EndPoint {
            kind: EndPoint::FACE,
            index: 1,
            indirect: true,
        };
        assert!(face.is_brep());
        assert_eq!(manager.tag_of(face), Some(202));
        // An end naming its entity by position holds no tag.
        assert_eq!(
            manager.tag_of(EndPoint {
                indirect: false,
                ..face
            }),
            None
        );
        // Nor does one past the end of the list.
        assert_eq!(manager.tag_of(EndPoint { index: 9, ..face }), None);
        // A sixty-four bit tag was not read, so it names nothing rather
        // than naming the wrong face.
        let wide = PmiManager {
            cad_tags: vec![i64::MIN],
            ..Default::default()
        };
        assert_eq!(wide.tag_of(EndPoint { index: 0, ..face }), None);
    }

    #[test]
    fn polylines_are_cut_at_the_indices_that_delimit_them() {
        let vertices: Vec<[f64; 3]> = (0..8).map(|i| [i as f64, 0.0, 0.0]).collect();
        // The specification's own example: two lines around a polyline.
        let lines = split(&[0, 2, 6, 8], &vertices);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].len(), 2);
        assert_eq!(lines[1].len(), 4);
        assert_eq!(lines[2].len(), 2);
        assert_eq!(lines[1][0], [2.0, 0.0, 0.0]);
        // A break past the end is clamped, and a single vertex is not a line.
        assert_eq!(split(&[0, 99], &vertices).len(), 1);
        assert!(split(&[0, 1], &vertices).is_empty());
        assert!(split(&[], &vertices).is_empty());
    }

    #[test]
    fn truncated_input_reports_where_it_ran_out() {
        assert!(parse(&[]).is_err());
        let err = parse(&[2, 0]).unwrap_err();
        assert!(err.message.contains("wanted"), "{}", err.message);
    }
}
