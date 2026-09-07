//! Presentation-layer records (ADR 0003).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::measure::{Measure, Placement};
use super::semantic::Unmapped;

/// The presentation layer: what a human sees.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Presentation {
    pub annotations: Vec<Annotation>,
    pub views: Vec<SavedView>,
}

impl Presentation {
    /// Sort every array by id, as the schema requires.
    pub fn sort(&mut self) {
        self.annotations.sort_by(|a, b| a.id.cmp(&b.id));
        self.views.sort_by(|a, b| a.id.cmp(&b.id));
    }
}

/// One displayed callout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Annotation {
    /// Content-derived, unique within the document.
    pub id: String,
    /// The presented PMI type.
    pub kind: AnnotationKind,
    /// The vendor's callout name, e.g. `Position.1`. Ignored by the diff
    /// by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_origin: Option<TextOrigin>,
    /// The annotation plane.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plane: Option<Placement>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<Placeholder>,
    pub leaders: Vec<Leader>,
    /// Aggregate over all parts.
    pub geometry: GeometrySummary,
    pub parts: Vec<Part>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<Style>,
    /// Semantic records this annotation displays.
    pub semantic: Vec<String>,
    /// Features this annotation is attached to.
    pub features: Vec<String>,
    /// Saved views this annotation is part of.
    pub views: Vec<String>,
    /// Vendor properties with no home elsewhere (JT's property bag).
    pub attributes: BTreeMap<String, String>,
    pub unmapped: Vec<Unmapped>,
    /// Source entities. Excluded from comparison.
    pub source_refs: Vec<String>,
}

/// Where an annotation's text came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextOrigin {
    /// The file carries the string.
    Explicit,
    /// Rendered from the linked semantic record.
    Semantic,
}

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

string_enum! {
    /// The presented PMI type (rec. practice table 18 names, normalised).
    AnnotationKind {
        Position => "position",
        Flatness => "flatness",
        Straightness => "straightness",
        Roundness => "roundness",
        Cylindricity => "cylindricity",
        LineProfile => "line_profile",
        SurfaceProfile => "surface_profile",
        Angularity => "angularity",
        Perpendicularity => "perpendicularity",
        Parallelism => "parallelism",
        Symmetry => "symmetry",
        Concentricity => "concentricity",
        Coaxiality => "coaxiality",
        CircularRunout => "circular_runout",
        TotalRunout => "total_runout",
        LinearDimension => "linear_dimension",
        DiameterDimension => "diameter_dimension",
        RadialDimension => "radial_dimension",
        AngularDimension => "angular_dimension",
        GeneralDimension => "general_dimension",
        OrdinateDimension => "ordinate_dimension",
        CurvedDimension => "curved_dimension",
        Datum => "datum",
        DatumTarget => "datum_target",
        Label => "label",
        Note => "note",
        SurfaceFinish => "surface_finish",
        Weld => "weld",
        Placeholder => "placeholder",
        SupplementalGeometry => "supplemental_geometry",
    }
}

impl AnnotationKind {
    /// Map a rec. practice table 18 name such as `"profile of surface"` or
    /// `"diameter dimension"`. Unrecognised names are kept verbatim.
    pub fn from_presented_name(name: &str) -> Self {
        let n = name.trim().to_ascii_lowercase();
        let canonical = match n.as_str() {
            "position" => "position",
            "flatness" => "flatness",
            "straightness" => "straightness",
            "roundness" | "circularity" => "roundness",
            "cylindricity" => "cylindricity",
            "profile of line" | "line profile" => "line_profile",
            "profile of surface" | "surface profile" => "surface_profile",
            "angularity" => "angularity",
            "perpendicularity" => "perpendicularity",
            "parallelism" => "parallelism",
            "symmetry" => "symmetry",
            "concentricity" => "concentricity",
            "coaxiality" => "coaxiality",
            "circular runout" => "circular_runout",
            "total runout" => "total_runout",
            "linear dimension" => "linear_dimension",
            "diameter dimension" => "diameter_dimension",
            "radial dimension" | "radius dimension" => "radial_dimension",
            "angular dimension" => "angular_dimension",
            "general dimension" => "general_dimension",
            "ordinate dimension" => "ordinate_dimension",
            "curved dimension" => "curved_dimension",
            "datum" => "datum",
            "datum target" => "datum_target",
            "label" => "label",
            "note" | "general note" | "flag note" => "note",
            "surface finish" | "surface roughness" | "surface texture" => "surface_finish",
            "weld" | "weld symbol" => "weld",
            _ => return Self::Other(name.trim().to_owned()),
        };
        Self::parse(canonical)
    }
}

/// From `annotation_placeholder_occurrence`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Placeholder {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<Placement>,
    /// Width and height of the placeholder box.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub box_size: Option<[f64; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_height: Option<Measure>,
}

/// A leader line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Leader {
    pub points: Vec<[f64; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminator: Option<String>,
    /// `true` for an `auxiliary_leader_line`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub auxiliary: bool,
}

/// Summary of graphical geometry: enough to detect change without
/// carrying coordinates (ADR 0003).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct GeometrySummary {
    pub polylines: usize,
    pub triangles: usize,
    pub points: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bbox: Option<BBox>,
    /// Content hash of all coordinates, rounded to the file's uncertainty.
    pub hash: String,
}

/// Axis-aligned bounding box.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BBox {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

/// One occurrence within a callout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Part {
    pub form: PartForm,
    pub kind: AnnotationKind,
    pub geometry: GeometrySummary,
    /// Full coordinates, only with `--presentation-geometry`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub polylines: Option<Vec<Vec<[f64; 3]>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vertices: Option<Vec<[f64; 3]>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub triangles: Option<Vec<[u32; 3]>>,
    pub source_ref: String,
}

/// The form an occurrence takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartForm {
    Tessellated,
    Polyline,
    Placeholder,
    FillArea,
    Text,
}

/// Visual style.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Style {
    /// `#rrggbb` or a predefined colour name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colour: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_font: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_width: Option<Measure>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<String>,
}

/// A saved view: a camera and what it shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedView {
    pub id: String,
    pub name: String,
    pub camera: Camera,
    pub clipping_planes: Vec<Placement>,
    pub annotations: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub default: bool,
    pub unmapped: Vec<Unmapped>,
    pub source_refs: Vec<String>,
}

/// Camera parameters from `camera_model_d3` and `view_volume`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Camera {
    pub placement: Placement,
    pub projection: Projection,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view_plane_distance: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view_window: Option<[f64; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub front_clip: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub back_clip: Option<f64>,
}

string_enum! {
    /// Camera projection.
    Projection {
        Parallel => "parallel",
        Perspective => "perspective",
    }
}
