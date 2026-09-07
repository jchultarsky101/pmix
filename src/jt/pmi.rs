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
    pub texts: Vec<String>,
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

/// A link between two entities.
#[derive(Debug, Clone, PartialEq)]
pub struct Association {
    pub source: i32,
    pub destination: i32,
    pub reason: i32,
}

/// A saved view.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelView {
    pub id: i32,
    pub name: Option<String>,
    pub active: bool,
}

/// The contents of a PMI Manager element.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PmiManager {
    pub version: u8,
    pub strings: Vec<String>,
    pub associations: Vec<Association>,
    pub model_views: Vec<ModelView>,
    pub entities: Vec<Entity>,
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

    /// A vector of 32-bit floats, skipped rather than decoded: `pmix`
    /// compares PMI, not the polylines that draw it.
    fn skip_floats(&mut self) -> Result<()> {
        let n = self.count(4)?;
        self.skip(n * 4)
    }
}

/// Parse the object data of a PMI Manager element.
pub fn parse(data: &[u8]) -> Result<PmiManager> {
    let mut r = Reader::new(data);
    let version = r.u8()?;
    r.skip(2)?; // empty field

    // Design groups, which pmix does not model but must be stepped over.
    for _ in 0..r.count(8)? {
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
        let source = r.i32()?;
        r.skip(4)?;
        let reason = r.i32()?;
        let destination = r.i32()?;
        r.skip(4)?;
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
        r.skip(12 + 4 + 36 + 4 + 4 + 4)?; // camera and empty fields
        let active = r.i32()? != 0;
        let id = r.i32()?;
        let name = text_of(r.u32()?, &strings);
        for _ in 0..r.count(2)? {
            r.atom()?;
            r.atom()?;
        }
        model_views.push(ModelView { id, name, active });
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
        for _ in 0..r.count(4)? {
            let id = r.u32()?;
            r.skip(12 + 24)?; // font, empty fields, text box
            let indices = r.count(2)?;
            r.skip(indices * 2)?;
            r.skip_floats()?;
            if let Some(t) = text_of(id, &strings) {
                texts.push(t);
            }
        }

        // Non-text polylines: segment indices, types, widths, coordinates.
        let n = r.count(4)?;
        r.skip(n * 4)?;
        let n = r.count(2)?;
        r.skip(n * 2)?;
        let n = r.count(2)?;
        r.skip(n * 2)?;
        r.skip_floats()?;

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
            properties,
            type_name,
            valid,
        });
    }

    Ok(PmiManager {
        version,
        strings,
        associations,
        model_views,
        entities,
    })
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
    fn truncated_input_reports_where_it_ran_out() {
        assert!(parse(&[]).is_err());
        let err = parse(&[2, 0]).unwrap_err();
        assert!(err.message.contains("wanted"), "{}", err.message);
    }
}
