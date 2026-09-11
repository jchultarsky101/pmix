//! Recognising manufacturing features from geometry (ADR 0011).
//!
//! This is not PMI. The semantic and presentation layers describe what a
//! file *states*; this describes what a shape *is*, and the two are kept
//! apart so that nothing downstream has to wonder which it is reading.
//!
//! Recognition runs over [`brep::Solid`], a boundary representation both
//! readers produce, so a rule is written once rather than once per
//! format. The rules themselves are local — a surface kind, the curves
//! bounding a face, what meets it across them — and deliberately few.
//! What no rule claims is reported as unclaimed rather than passed over.

pub mod brep;
pub mod compare;
pub mod envelope;
pub mod jt;
pub mod model;
pub mod patterns;
mod rules;
pub mod step;
pub mod surface;
pub mod view;

pub use model::{
    Body, Diagnostic, Envelope, FaceCounts, Feature, FeatureDocument, Kind, Pattern, PatternKind,
    SCHEMA_VERSION, Shape, UnassignedFace, Units,
};

use crate::model::ContentId;

/// Everything recognised in `solids`, in the order the solids were
/// given.
///
/// [`recognise`] sorts by id, which is what a document wants and what
/// anything needing to know *which* solid a body came from cannot use.
/// The product reader needs exactly that, to put each body under the
/// part whose shape representation held its shell (ADR 0014). Both go
/// through here, so a body has one id whichever asks for it — including
/// the ordinal a collision gets, which depends on what else was hashed
/// first and so on the order the solids are walked in.
pub fn recognise_in_order(solids: &[brep::Solid]) -> Vec<Body> {
    let mut ids = ContentId::new();
    solids
        .iter()
        .map(|s| rules::recognise(s, &mut ids))
        .collect()
}

/// Everything recognised in `solids`, as one document.
pub fn recognise(
    source: crate::model::Source,
    units: Units,
    solids: &[brep::Solid],
    diagnostics: Vec<Diagnostic>,
) -> FeatureDocument {
    let mut bodies = recognise_in_order(solids);
    bodies.sort_by(|a, b| a.id.cmp(&b.id));
    FeatureDocument {
        schema_version: SCHEMA_VERSION,
        source,
        units,
        bodies,
        diagnostics,
    }
}

/// Recognise the features in the file at `path`.
///
/// The format is detected from the extension, as it is for `extract`.
/// The units the numbers come out in are always the canonical ones; the
/// file's own are recorded beside them.
pub fn read_path(path: &std::path::Path) -> crate::Result<FeatureDocument> {
    let format = crate::Format::from_path(path)
        .ok_or_else(|| crate::Error::UnknownFormat(path.display().to_string()))?;
    let bytes = std::fs::read(path)?;
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    match format {
        crate::Format::Step => from_step(&bytes, &file_name),
        crate::Format::Jt => from_jt(&bytes, &file_name),
    }
}

/// Recognise the features in a STEP file's advanced B-rep.
pub fn from_step(bytes: &[u8], file_name: &str) -> crate::Result<FeatureDocument> {
    use crate::step::p21;
    let ex = p21::parse_bytes(bytes)?;
    let declared = crate::step::pmi::units_of(&ex);
    let scale =
        crate::fingerprint::Scale::of(declared.length.as_deref(), declared.angle.as_deref());
    let solids = step::solids(&ex, scale);
    let mut diagnostics = Vec::new();
    if solids.is_empty() {
        diagnostics.push(Diagnostic {
            message: "the file states no shell, so there is no geometry to recognise".into(),
        });
    }
    Ok(recognise(
        crate::model::Source {
            file_name: file_name.to_owned(),
            format: "STEP".into(),
            schema: ex.header.schema_name(),
            writer: ex.header.originating_system().map(str::to_owned),
            time_stamp: ex.header.time_stamp().map(str::to_owned),
        },
        canonical_units(declared),
        &solids,
        diagnostics,
    ))
}

/// Recognise the features in a JT file's smart topology table.
pub fn from_jt(bytes: &[u8], file_name: &str) -> crate::Result<FeatureDocument> {
    use crate::jt::{Elements, file::Jt, file::SegmentKind, property, stt};
    let file = Jt::parse(bytes)?;
    let mut diagnostics = Vec::new();
    let mut solids = Vec::new();
    for segment in file
        .segments()
        .iter()
        .filter(|s| s.kind == SegmentKind::Stt)
    {
        let data = match file.segment_data(segment) {
            Ok(data) => data,
            Err(e) => {
                diagnostics.push(Diagnostic {
                    message: format!("topology segment {} could not be read: {e}", segment.id),
                });
                continue;
            }
        };
        for element in Elements::new(&data) {
            if element.object_type != stt::STT_ELEMENT {
                continue;
            }
            match stt::parse(element.data) {
                Ok(t) => {
                    if let Some(why) = &t.stopped {
                        diagnostics.push(Diagnostic {
                            message: format!("a topology table was read only in part: {why}"),
                        });
                    }
                    solids.push(jt::solid(&t));
                }
                Err(e) => diagnostics.push(Diagnostic {
                    message: format!("a topology table could not be read: {e}"),
                }),
            }
        }
    }
    if solids.is_empty() {
        diagnostics.push(Diagnostic {
            message: "the file holds no topology segment, so there is no precise geometry to \
                      recognise; a JT written without B-rep carries only tessellation"
                .into(),
        });
    }
    // The scene graph declares the units the file's own numbers are in.
    // A topology table states metres whatever that says, so this is
    // recorded rather than applied.
    let mut scene = property::Properties::default();
    for segment in file
        .segments()
        .iter()
        .filter(|s| s.kind == SegmentKind::LogicalSceneGraph)
    {
        if let Ok(data) = file.segment_data(segment) {
            let read = property::read(&data, file.header.major);
            scene.by_element.extend(read.by_element);
        }
    }
    let declared = crate::model::Units {
        length: scene
            .find("JT_PROP_MEASUREMENT_UNITS")
            .and_then(property::unit_name)
            .map(str::to_owned),
        angle: None,
    };
    Ok(recognise(
        crate::model::Source {
            file_name: file_name.to_owned(),
            format: "JT".into(),
            schema: Some(format!("{}.{}", file.header.major, file.header.minor)),
            writer: None,
            time_stamp: None,
        },
        canonical_units(declared),
        &solids,
        diagnostics,
    ))
}

/// The canonical units, noting what the file declared instead.
fn canonical_units(declared: crate::model::Units) -> Units {
    Units {
        length: crate::units::LENGTH.to_owned(),
        angle: crate::units::ANGLE.to_owned(),
        declared_length: declared.length,
        declared_angle: declared.angle,
    }
}
