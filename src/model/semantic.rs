//! Semantic-layer records (ADR 0002).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::measure::{Direction, Measure, Placement};

/// Defines an enumeration that serialises as a string, with known values
/// mapped to canonical `snake_case` names and unrecognised source values
/// kept verbatim in `Other` (ADR 0002, rule 6).
macro_rules! string_enum {
    ($(#[$m:meta])* $name:ident { $($variant:ident => $s:literal),* $(,)? }) => {
        $(#[$m])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub enum $name {
            $($variant,)*
            /// A value not in the canonical list, kept as written.
            Other(String),
        }

        impl $name {
            /// Canonical name, or the verbatim source value for `Other`.
            pub fn as_str(&self) -> &str {
                match self {
                    $(Self::$variant => $s,)*
                    Self::Other(s) => s,
                }
            }

            /// Parse a canonical name; anything else becomes `Other`.
            pub fn parse(s: &str) -> Self {
                match s {
                    $($s => Self::$variant,)*
                    other => Self::Other(other.to_owned()),
                }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let s = String::deserialize(d)?;
                Ok(Self::parse(&s))
            }
        }
    };
}

/// Fields shared by every semantic record (ADR 0002, rules 1 to 5).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Meta {
    /// Content-derived, unique within the document.
    pub id: String,
    /// Whether the structured fields came from semantic entities or were
    /// inferred from annotation text.
    pub origin: Origin,
    /// Ids of presentation-layer annotations that display this record.
    pub presentation: Vec<String>,
    /// Attributes recognised but not interpreted.
    pub unmapped: Vec<Unmapped>,
    /// Source entities, e.g. `"#1752"`. Excluded from comparison.
    pub source_refs: Vec<String>,
}

/// Where a record's structured content came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    #[default]
    Semantic,
    Text,
}

/// An attribute the reader saw but could not interpret.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Unmapped {
    pub attribute: String,
    pub raw: String,
}

// ----- Feature ------------------------------------------------------------

/// A portion of the part that PMI applies to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Feature {
    #[serde(flatten)]
    pub meta: Meta,
    pub kind: FeatureKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub geometry: Vec<GeometryRef>,
    /// Child features of a composite.
    pub members: Vec<String>,
    /// Pattern count for `n×` features when stated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
}

string_enum! {
    /// What kind of feature a [`Feature`] is.
    FeatureKind {
        Face => "face",
        Edge => "edge",
        Vertex => "vertex",
        Axis => "axis",
        CenterPlane => "center_plane",
        CenterPoint => "center_point",
        Center => "center",
        Apex => "apex",
        Tangent => "tangent",
        Derived => "derived",
        AllAround => "all_around",
        Between => "between",
        Composite => "composite",
        CompositeGroup => "composite_group",
        DatumTargetArea => "datum_target_area",
        AllOver => "all_over",
        ParallelOffset => "parallel_offset",
        GeometricAlignment => "geometric_alignment",
        PerpendicularTo => "perpendicular_to",
        Mixed => "mixed",
    }
}

/// A B-rep entity a feature is made of.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeometryRef {
    pub kind: GeometryKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface: Option<SurfaceKind>,
    /// Source entity reference. Excluded from comparison.
    pub source_ref: String,
}

string_enum! {
    /// Topological kind of a [`GeometryRef`].
    GeometryKind {
        Face => "face",
        Edge => "edge",
        Vertex => "vertex",
        Point => "point",
        Curve => "curve",
        Surface => "surface",
    }
}

string_enum! {
    /// Surface type of a face.
    SurfaceKind {
        Plane => "plane",
        Cylinder => "cylinder",
        Cone => "cone",
        Sphere => "sphere",
        Torus => "torus",
        BSpline => "bspline",
        Revolution => "revolution",
        Extrusion => "extrusion",
        Offset => "offset",
    }
}

// ----- Datums -------------------------------------------------------------

/// A datum: a theoretically exact reference established from features.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Datum {
    #[serde(flatten)]
    pub meta: Meta,
    /// The letter(s), e.g. `"A"`.
    pub label: String,
    /// Datum feature ids.
    pub features: Vec<String>,
    pub targets: Vec<DatumTarget>,
}

/// A datum target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatumTarget {
    /// E.g. `"A1"`.
    pub label: String,
    pub kind: DatumTargetKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diameter: Option<Measure>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<Measure>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<Measure>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<Placement>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feature: Option<String>,
    /// Attributes recognised but not interpreted.
    pub unmapped: Vec<Unmapped>,
}

string_enum! {
    /// Shape of a [`DatumTarget`].
    DatumTargetKind {
        Point => "point",
        Line => "line",
        Rectangle => "rectangle",
        Circle => "circle",
        CircularLine => "circular_line",
        Area => "area",
    }
}

/// An ordered datum reference frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatumSystem {
    #[serde(flatten)]
    pub meta: Meta,
    /// In precedence order.
    pub compartments: Vec<Compartment>,
    /// Canonical rendering such as `A|B(M)|C`.
    pub text: String,
}

/// One compartment of a datum reference frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Compartment {
    pub datums: Vec<DatumRef>,
    /// `true` for common datums such as `A-B`.
    pub common: bool,
}

/// A reference to a datum with its modifiers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatumRef {
    pub datum: String,
    pub modifiers: Vec<DatumModifier>,
    pub modifier_values: Vec<Measure>,
}

string_enum! {
    /// AP242 `datum_reference_modifier_type`.
    DatumModifier {
        MaximumMaterial => "maximum_material",
        LeastMaterial => "least_material",
        RegardlessOfSize => "regardless_of_size",
        Basic => "basic",
        Translation => "translation",
        Projected => "projected",
        Orientation => "orientation",
        Point => "point",
        Line => "line",
        Plane => "plane",
        FreeState => "free_state",
        ContactingFeature => "contacting_feature",
        DegreeOfFreedomX => "degree_of_freedom_x",
        DegreeOfFreedomY => "degree_of_freedom_y",
        DegreeOfFreedomZ => "degree_of_freedom_z",
        DegreeOfFreedomU => "degree_of_freedom_u",
        DegreeOfFreedomV => "degree_of_freedom_v",
        DegreeOfFreedomW => "degree_of_freedom_w",
        Distance => "distance",
        MajorDiameter => "major_diameter",
        MinorDiameter => "minor_diameter",
        PitchDiameter => "pitch_diameter",
        AnyCrossSection => "any_cross_section",
        AnyLongitudinalSection => "any_longitudinal_section",
        CircularOrCylindrical => "circular_or_cylindrical",
        AllAround => "all_around",
        AllOver => "all_over",
    }
}

// ----- Dimensions ---------------------------------------------------------

/// A dimension: a size of one feature or a location between two.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Dimension {
    #[serde(flatten)]
    pub meta: Meta,
    pub kind: DimensionKind,
    pub subtype: DimensionSubtype,
    /// Nominal value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Measure>,
    /// Lower and upper limit for range dimensions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limits: Option<Limits>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tolerance: Option<DimensionTolerance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qualifier: Option<DimensionQualifier>,
    pub modifiers: Vec<DimensionModifier>,
    /// One feature for a size, two for a location.
    pub features: Vec<String>,
    /// `true` for a directed location (AP242 `directed_dimensional_location`):
    /// the order of `features` is significant.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub directed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orientation: Option<Direction>,
    /// Feature along which a `_with_path` dimension is measured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decimal_places: Option<u8>,
    /// As displayed, e.g. `⌀12.5 ±0.05`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// Whether a dimension is a size or a location, linear or angular.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DimensionKind {
    Size,
    Location,
    AngularSize,
    AngularLocation,
}

impl DimensionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Size => "size",
            Self::Location => "location",
            Self::AngularSize => "angular_size",
            Self::AngularLocation => "angular_location",
        }
    }
}

string_enum! {
    /// The AP242 dimension `name`, normalised.
    DimensionSubtype {
        Linear => "linear",
        Diameter => "diameter",
        Radius => "radius",
        SphericalDiameter => "spherical_diameter",
        SphericalRadius => "spherical_radius",
        CurveLength => "curve_length",
        CurvedDistance => "curved_distance",
        Thickness => "thickness",
        Angle => "angle",
        ToroidalMajorDiameter => "toroidal_major_diameter",
        ToroidalMinorDiameter => "toroidal_minor_diameter",
        ToroidalHighMajorDiameter => "toroidal_high_major_diameter",
        ToroidalLowMajorDiameter => "toroidal_low_major_diameter",
        ToroidalHighMinorDiameter => "toroidal_high_minor_diameter",
        ToroidalLowMinorDiameter => "toroidal_low_minor_diameter",
        LinearCentreOuter => "linear_centre_outer",
        LinearCentreInner => "linear_centre_inner",
        LinearInnerOuter => "linear_inner_outer",
        LinearOuterCentre => "linear_outer_centre",
        LinearInnerCentre => "linear_inner_centre",
        LinearOuterInner => "linear_outer_inner",
        LinearInnerInner => "linear_inner_inner",
        LinearOuterOuter => "linear_outer_outer",
    }
}

impl DimensionSubtype {
    /// Map an AP242 dimension name string such as `"linear distance outer
    /// outer"` to a subtype. Unrecognised names are kept verbatim.
    pub fn from_ap242_name(name: &str) -> Self {
        let n = name.trim().to_ascii_lowercase();
        let canonical = match n.as_str() {
            "linear distance" => "linear",
            "diameter" => "diameter",
            "radius" => "radius",
            "spherical diameter" => "spherical_diameter",
            "spherical radius" => "spherical_radius",
            "curve length" => "curve_length",
            "curved distance" => "curved_distance",
            "thickness" => "thickness",
            "angle" | "angular distance" | "angular size" => "angle",
            "toroidal major diameter" => "toroidal_major_diameter",
            "toroidal minor diameter" => "toroidal_minor_diameter",
            "toroidal high major diameter" => "toroidal_high_major_diameter",
            "toroidal low major diameter" => "toroidal_low_major_diameter",
            "toroidal high minor diameter" => "toroidal_high_minor_diameter",
            "toroidal low minor diameter" => "toroidal_low_minor_diameter",
            "linear distance centre outer" => "linear_centre_outer",
            "linear distance centre inner" => "linear_centre_inner",
            "linear distance inner outer" => "linear_inner_outer",
            "linear distance outer centre" => "linear_outer_centre",
            "linear distance inner centre" => "linear_inner_centre",
            "linear distance outer inner" => "linear_outer_inner",
            "linear distance inner inner" => "linear_inner_inner",
            "linear distance outer outer" => "linear_outer_outer",
            _ => return Self::Other(name.to_owned()),
        };
        Self::parse(canonical)
    }
}

/// Lower and upper bound of a range dimension.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Limits {
    pub lower: Measure,
    pub upper: Measure,
}

/// Tolerance on a dimension.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DimensionTolerance {
    /// Explicit bounds, e.g. `-0.2 / +0.0`. Both are signed deviations.
    PlusMinus { lower: Measure, upper: Measure },
    /// ISO 286 style, e.g. `H7`.
    LimitsAndFits {
        form: String,
        zone: String,
        grade: String,
    },
}

string_enum! {
    /// How a dimension's value is to be read.
    DimensionQualifier {
        Basic => "basic",
        Reference => "reference",
        Maximum => "maximum",
        Minimum => "minimum",
    }
}

string_enum! {
    /// Dimension modifiers (AP242 rec. practice tables 7 and 8).
    DimensionModifier {
        ControlledRadius => "controlled_radius",
        Square => "square",
        Statistical => "statistical",
        ContinuousFeature => "continuous_feature",
        TwoPointSize => "two_point_size",
        LocalSize => "local_size",
        LeastSquares => "least_squares",
        MaximumInscribed => "maximum_inscribed",
        MinimumCircumscribed => "minimum_circumscribed",
        CircumferenceDiameter => "circumference_diameter",
        AreaDiameter => "area_diameter",
        VolumeDiameter => "volume_diameter",
        MaximumSize => "maximum_size",
        MinimumSize => "minimum_size",
        AverageSize => "average_size",
        MedianSize => "median_size",
        MidRangeSize => "mid_range_size",
        RangeOfSizes => "range_of_sizes",
        AnyRestrictedPortion => "any_restricted_portion",
        AnyCrossSection => "any_cross_section",
        SpecificFixedCrossSection => "specific_fixed_cross_section",
        CommonTolerance => "common_tolerance",
        FreeState => "free_state",
        Envelope => "envelope",
        Between => "between",
        Statistical3 => "statistical_tolerance",
    }
}

// ----- Geometric tolerances -----------------------------------------------

/// A geometric tolerance (a feature control frame).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeometricTolerance {
    #[serde(flatten)]
    pub meta: Meta,
    pub kind: ToleranceKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Measure>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zone: Option<Zone>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unequally_disposed: Option<Measure>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_value: Option<Measure>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit_basis: Option<UnitBasis>,
    pub modifiers: Vec<ToleranceModifier>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub datum_system: Option<String>,
    pub features: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub affected_plane: Option<Direction>,
    /// Parent frame when this is a lower segment of a composite.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub composite_of: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decimal_places: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

string_enum! {
    /// The fifteen AP242 geometric tolerance types.
    ToleranceKind {
        Angularity => "angularity",
        CircularRunout => "circular_runout",
        Coaxiality => "coaxiality",
        Concentricity => "concentricity",
        Cylindricity => "cylindricity",
        Flatness => "flatness",
        LineProfile => "line_profile",
        Parallelism => "parallelism",
        Perpendicularity => "perpendicularity",
        Position => "position",
        Roundness => "roundness",
        Straightness => "straightness",
        SurfaceProfile => "surface_profile",
        Symmetry => "symmetry",
        TotalRunout => "total_runout",
    }
}

/// Tolerance zone description.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Zone {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub form: Option<ZoneForm>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projected: Option<Measure>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runout_angle: Option<Measure>,
}

string_enum! {
    /// AP242 `tolerance_zone_form` names (rec. practice table 13), normalised.
    ZoneForm {
        CylindricalOrCircular => "cylindrical_or_circular",
        Spherical => "spherical",
        WithinCircle => "within_circle",
        BetweenTwoConcentricCircles => "between_two_concentric_circles",
        BetweenTwoEquidistantCurves => "between_two_equidistant_curves",
        WithinCylinder => "within_cylinder",
        BetweenTwoCoaxialCylinders => "between_two_coaxial_cylinders",
        BetweenTwoEquidistantSurfaces => "between_two_equidistant_surfaces",
        NonUniform => "non_uniform",
    }
}

/// Per-unit tolerance basis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnitBasis {
    pub length: Measure,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<Measure>,
    /// Shape of the unit area: `circular`, `rectangular`, `square`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub area_type: Option<String>,
}

string_enum! {
    /// AP242 `geometric_tolerance_modifier`.
    ToleranceModifier {
        MaximumMaterial => "maximum_material",
        LeastMaterial => "least_material",
        FreeState => "free_state",
        TangentPlane => "tangent_plane",
        Statistical => "statistical",
        CommonZone => "common_zone",
        AnyCrossSection => "any_cross_section",
        Circle => "circle",
        Reciprocity => "reciprocity",
        SeparateRequirement => "separate_requirement",
        EachRadialElement => "each_radial_element",
        LineElement => "line_element",
        NotConvex => "not_convex",
        MajorDiameter => "major_diameter",
        MinorDiameter => "minor_diameter",
        PitchDiameter => "pitch_diameter",
        UnequallyDisposed => "unequally_disposed",
        AllAround => "all_around",
        AllOver => "all_over",
    }
}

// ----- Notes and other ----------------------------------------------------

/// A semantic text note.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Note {
    #[serde(flatten)]
    pub meta: Meta,
    pub text: String,
    pub kind: NoteKind,
    pub features: Vec<String>,
}

/// Kind of a [`Note`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteKind {
    General,
    Flag,
}

/// A recognised PMI concept the model has no record for yet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Other {
    #[serde(flatten)]
    pub meta: Meta,
    pub kind: String,
    pub attributes: BTreeMap<String, String>,
    pub features: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_enums_round_trip_and_keep_unknown_values() {
        assert_eq!(ToleranceKind::parse("position"), ToleranceKind::Position);
        let odd = ToleranceKind::parse("wobble");
        assert_eq!(odd, ToleranceKind::Other("wobble".into()));
        assert_eq!(serde_json::to_string(&odd).unwrap(), "\"wobble\"");
        assert_eq!(
            serde_json::to_string(&ToleranceKind::Flatness).unwrap(),
            "\"flatness\""
        );
        let back: ToleranceKind = serde_json::from_str("\"flatness\"").unwrap();
        assert_eq!(back, ToleranceKind::Flatness);
    }

    #[test]
    fn dimension_subtype_maps_ap242_names() {
        assert_eq!(
            DimensionSubtype::from_ap242_name("diameter"),
            DimensionSubtype::Diameter
        );
        assert_eq!(
            DimensionSubtype::from_ap242_name("Linear Distance"),
            DimensionSubtype::Linear
        );
        assert_eq!(
            DimensionSubtype::from_ap242_name("linear distance outer outer"),
            DimensionSubtype::LinearOuterOuter
        );
        assert_eq!(
            DimensionSubtype::from_ap242_name("weird"),
            DimensionSubtype::Other("weird".into())
        );
    }

    #[test]
    fn meta_flattens_into_records() {
        let d = Dimension {
            meta: Meta {
                id: "dim:1".into(),
                source_refs: vec!["#1".into()],
                ..Default::default()
            },
            kind: DimensionKind::Size,
            subtype: DimensionSubtype::Diameter,
            value: Some(Measure::new(35.0, "mm")),
            limits: None,
            tolerance: Some(DimensionTolerance::PlusMinus {
                lower: Measure::new(-0.2, "mm"),
                upper: Measure::new(0.0, "mm"),
            }),
            qualifier: None,
            modifiers: vec![],
            features: vec!["feat:1".into()],
            directed: false,
            orientation: None,
            path: None,
            decimal_places: Some(1),
            text: None,
        };
        let json = serde_json::to_value(&d).unwrap();
        assert_eq!(json["id"], "dim:1");
        assert_eq!(json["origin"], "semantic");
        assert_eq!(json["tolerance"]["type"], "plus_minus");
        assert_eq!(json["tolerance"]["lower"]["value"], -0.2);
        assert!(json.get("limits").is_none());
        let back: Dimension = serde_json::from_value(json).unwrap();
        assert_eq!(back, d);
    }
}
