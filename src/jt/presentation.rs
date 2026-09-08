//! Mapping JT PMI onto the presentation model of ADR 0003.
//!
//! A JT PMI entity carries the lines that draw it, so an annotation's
//! geometry comes from the file rather than from a summary the writer
//! chose to include. What it says in words mostly does not survive:
//! producing systems write the glyphs of an annotation's text rather
//! than the text, so an annotation usually has a label and no text.
//!
//! Which annotations a saved view shows is stated by associations, and
//! those name their ends by CAD tag rather than by position, so
//! [`PmiManager::resolve`] does the lookup.

use std::collections::BTreeMap;

use crate::geometry::Geometry;
use crate::model::{
    Annotation, AnnotationKind, Camera, ContentId, Direction, Part, PartForm, Placement,
    Presentation, Projection, SavedView, Style, TextOrigin, Unmapped,
};
use crate::reader::ExtractOptions;

use super::pmi::{EndPoint, Entity, EntityKind, PmiManager, Tagged};
use super::property;
use super::semantic::{Links, source_ref};

/// The rounding applied to coordinates before hashing them. JT states no
/// uncertainty, so this is the finest difference the summary registers.
const QUANTUM: f64 = 1e-6;

/// Whether a PMI entity is something a person sees. A part transform
/// places a component and a model view style says how a view is drawn;
/// neither is an annotation.
fn is_drawn(kind: &EntityKind) -> bool {
    !matches!(kind, EntityKind::PartTransform | EntityKind::ModelViewStyle)
}

/// What a PMI entity looks like when displayed.
fn annotation_kind(entity: &Entity) -> AnnotationKind {
    match entity.kind {
        // A dimension's own kind says how it is drawn.
        EntityKind::Dimension | EntityKind::CalloutDimension | EntityKind::ChamferDimension => {
            match entity.property("type").map(str::trim) {
                Some("0") => AnnotationKind::CurvedDimension,
                Some("1") => AnnotationKind::LinearDimension,
                Some("2") => AnnotationKind::AngularDimension,
                Some("3") => AnnotationKind::RadialDimension,
                _ => AnnotationKind::GeneralDimension,
            }
        }
        // A feature control frame is drawn as its characteristic.
        EntityKind::FeatureControlFrame | EntityKind::CompositeFeatureControlFrame => {
            let kind = super::semantic::tolerance_kind(entity.property("characteristic"));
            AnnotationKind::parse(kind.as_str())
        }
        EntityKind::DatumFeatureSymbol => AnnotationKind::Datum,
        EntityKind::DatumTarget => AnnotationKind::DatumTarget,
        EntityKind::SurfaceFinish => AnnotationKind::SurfaceFinish,
        EntityKind::Note
        | EntityKind::FaceAttributeNote
        | EntityKind::ModelViewLabelNote
        | EntityKind::BalloonNote
        | EntityKind::CoordinateNote
        | EntityKind::AttributeNote
        | EntityKind::BundleOrDressingNote
        | EntityKind::WeldNote => AnnotationKind::Note,
        EntityKind::Weld
        | EntityKind::SpotWeld
        | EntityKind::LineWeld
        | EntityKind::GrooveWeld
        | EntityKind::FilletWeld
        | EntityKind::SlotWeld
        | EntityKind::EdgeWeld
        | EntityKind::ArcSpotWeld
        | EntityKind::ResistanceSpotWeld
        | EntityKind::ResistanceSeamWeld => AnnotationKind::Weld,
        // Drawing furniture: lines that locate or decorate rather than
        // state a requirement.
        EntityKind::ReferenceGeometry
        | EntityKind::ReferencePoint
        | EntityKind::ReferenceAxis
        | EntityKind::ReferencePlane
        | EntityKind::ReferenceCircle
        | EntityKind::ReferenceCylinder
        | EntityKind::Centreline
        | EntityKind::CircleCentre
        | EntityKind::Crosshatch
        | EntityKind::CuttingPlaneSymbol
        | EntityKind::CoordinateSystem
        | EntityKind::Section => AnnotationKind::SupplementalGeometry,
        ref other => AnnotationKind::Other(other.as_str()),
    }
}

/// Read a property written as space-separated numbers.
fn numbers(entity: &Entity, key: &str) -> Option<Vec<f64>> {
    let text = entity.property(key)?;
    text.split_whitespace()
        .map(|n| n.parse().ok())
        .collect::<Option<Vec<f64>>>()
        .filter(|v| !v.is_empty())
}

fn direction(entity: &Entity, key: &str) -> Option<Direction> {
    let v = numbers(entity, key)?;
    (v.len() == 3).then(|| Direction {
        x: v[0],
        y: v[1],
        z: v[2],
    })
}

/// The plane an annotation is drawn on.
///
/// Its origin is written in metres, JT's base unit, while the lines that
/// draw the annotation are in the unit the model declares, so the origin
/// is converted to match them. The axes are unit vectors and need none.
fn plane(entity: &Entity, per_metre: f64) -> Option<Placement> {
    let origin = numbers(entity, "DisplayPlane.origin")?;
    (origin.len() == 3).then(|| Placement {
        origin: [
            origin[0] * per_metre,
            origin[1] * per_metre,
            origin[2] * per_metre,
        ],
        axis: direction(entity, "DisplayPlane.zaxis"),
        ref_direction: direction(entity, "DisplayPlane.xaxis"),
    })
}

/// How an annotation is drawn, as far as the model records it.
fn style(entity: &Entity) -> Option<Style> {
    let colour = ["PMITextForegroundColor", "textColour", "PMIGeometryColor"]
        .iter()
        .find_map(|k| entity.property(k))
        .and_then(colour_name);
    let layer = entity.property("LAYER").map(str::to_owned);
    let style = Style {
        colour,
        line_font: entity.property("font").map(str::to_owned),
        line_width: None,
        layer,
    };
    (style != Style::default()).then_some(style)
}

/// JT writes a colour as `0x00bbggrr`; the model wants `#rrggbb`.
fn colour_name(text: &str) -> Option<String> {
    let packed = u32::from_str_radix(text.trim().strip_prefix("0x")?, 16).ok()?;
    let (b, g, r) = ((packed >> 16) & 0xff, (packed >> 8) & 0xff, packed & 0xff);
    Some(format!("#{r:02x}{g:02x}{b:02x}"))
}

/// Whether a string is text a person would read, as opposed to the glyph
/// indices a producing system writes when it stores the shape of the
/// text rather than the text itself.
fn is_readable(text: &str) -> bool {
    text.chars().any(|c| !c.is_control()) && !text.trim().is_empty()
}

/// Properties that describe how the annotation looks and that this
/// reader did not read into a field. Indexed families such as
/// `Leader[0].arrowLength` are left out: they describe one stroke of the
/// drawing, and the geometry already carries where every stroke goes.
fn attributes(entity: &Entity) -> BTreeMap<String, String> {
    const CONSUMED: &[&str] = &["Description", "LAYER", "font"];
    entity
        .properties
        .iter()
        .filter(|(k, v)| {
            !v.is_empty()
                && !k.contains('[')
                && !CONSUMED.contains(&k.as_str())
                // The plane became a field, and keys the producing system
                // reserves for its own bookkeeping say nothing about the
                // annotation.
                && !k.starts_with("DisplayPlane.")
                && !k.starts_with("____")
                && super::semantic::is_style(k)
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// Turn the PMI of one file into annotations and saved views.
pub fn build(
    managers: &[PmiManager],
    length: Option<&str>,
    links: &Links,
    options: &ExtractOptions,
) -> Presentation {
    let per_metre = length.and_then(property::per_metre).unwrap_or(1.0);
    let mut ids = ContentId::new();
    let mut out = Presentation::default();

    // Annotation and view ids, by where they came from, so the
    // associations between them can be turned into ids.
    let mut annotation_ids: BTreeMap<(usize, usize), String> = BTreeMap::new();
    let mut view_ids: BTreeMap<(usize, usize), String> = BTreeMap::new();

    for (m, manager) in managers.iter().enumerate() {
        for (e, entity) in manager.entities.iter().enumerate() {
            if !is_drawn(&entity.kind) {
                continue;
            }
            let kind = annotation_kind(entity);
            let label = entity.property("Description").map(str::to_owned);
            let text = {
                let readable: Vec<&str> = entity
                    .texts
                    .iter()
                    .map(String::as_str)
                    .filter(|t| is_readable(t))
                    .collect();
                (!readable.is_empty()).then(|| readable.join(" "))
            };
            let geometry = Geometry {
                polylines: entity.polylines.clone(),
                ..Default::default()
            };
            let summary = geometry.summary(QUANTUM);
            let parts = (!entity.polylines.is_empty())
                .then(|| Part {
                    form: PartForm::Polyline,
                    kind: kind.clone(),
                    geometry: summary.clone(),
                    polylines: options
                        .presentation_geometry
                        .then(|| entity.polylines.clone()),
                    vertices: None,
                    triangles: None,
                    source_ref: source_ref(m, entity),
                })
                .into_iter()
                .collect();
            // The glyph runs go into the id so that a change to the text
            // shows up even though the text itself cannot be read back.
            let glyphs = entity.texts.concat();
            let id = ids.make(
                "anno",
                &[
                    kind.as_str(),
                    label.as_deref().unwrap_or(""),
                    &summary.hash,
                    &glyphs,
                ],
            );
            annotation_ids.insert((m, e), id.clone());
            out.annotations.push(Annotation {
                id,
                kind,
                label,
                text_origin: text.is_some().then_some(TextOrigin::Explicit),
                text,
                plane: plane(entity, per_metre),
                placeholder: None,
                leaders: Vec::new(),
                geometry: summary,
                parts,
                style: style(entity),
                semantic: links.get(&(m, e)).cloned().unwrap_or_default(),
                features: Vec::new(),
                views: Vec::new(),
                attributes: attributes(entity),
                unmapped: if entity.valid {
                    Vec::new()
                } else {
                    // The producing system marked the entity unsound, so
                    // say so rather than presenting it as good.
                    vec![Unmapped {
                        attribute: "valid".into(),
                        raw: "0".into(),
                    }]
                },
                source_refs: vec![source_ref(m, entity)],
            });
        }

        for (v, view) in manager.model_views.iter().enumerate() {
            // Producing systems wrap a view name in quotation marks.
            let name = view
                .name
                .as_deref()
                .unwrap_or_default()
                .trim_matches('"')
                .to_owned();
            let distance = {
                let d: f64 = (0..3)
                    .map(|i| (view.eye_position[i] - view.target[i]).powi(2))
                    .sum();
                (d > 0.0).then(|| d.sqrt())
            };
            let id = ids.make("view", &[&name, &format!("{:?}", view.eye_direction)]);
            view_ids.insert((m, v), id.clone());
            out.views.push(SavedView {
                id,
                name,
                camera: Camera {
                    placement: Placement {
                        origin: view.eye_position,
                        axis: Some(Direction {
                            x: view.eye_direction[0],
                            y: view.eye_direction[1],
                            z: view.eye_direction[2],
                        }),
                        ref_direction: None,
                    },
                    // JT states no projection, so claiming one would be
                    // an invention.
                    projection: Projection::Other("unspecified".into()),
                    view_plane_distance: distance,
                    view_window: (view.viewport_diameter > 0.0)
                        .then_some([view.viewport_diameter, view.viewport_diameter]),
                    front_clip: None,
                    back_clip: None,
                },
                clipping_planes: Vec::new(),
                annotations: Vec::new(),
                default: view.active,
                unmapped: if view.angle == 0.0 {
                    Vec::new()
                } else {
                    // The model has no field for a roll about the eye
                    // direction, so keep it where it can still be seen.
                    vec![Unmapped {
                        attribute: "camera rotation".into(),
                        raw: format!("{} deg", view.angle),
                    }]
                },
                source_refs: vec![format!("pmi[{m}]#view{}", view.id)],
            });
        }
    }

    // An association puts an annotation in a view. The specification
    // reserves reason 98 for this, but producing systems also use 10,
    // "included in a PMI symbol", so the ends are what identify it.
    let mut in_view: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (m, manager) in managers.iter().enumerate() {
        for link in &manager.associations {
            if link.source.kind != EndPoint::GENERIC
                || link.destination.kind != EndPoint::MODEL_VIEW
            {
                continue;
            }
            let (Some(Tagged::Entity(e)), Some(Tagged::ModelView(v))) = (
                manager.resolve(link.source.index as i32),
                manager.resolve(link.destination.index as i32),
            ) else {
                continue;
            };
            let (Some(annotation), Some(view)) =
                (annotation_ids.get(&(m, e)), view_ids.get(&(m, v)))
            else {
                continue;
            };
            in_view
                .entry(view.clone())
                .or_default()
                .push(annotation.clone());
        }
    }
    for view in &mut out.views {
        if let Some(shown) = in_view.get(&view.id) {
            view.annotations = shown.clone();
            view.annotations.sort();
            view.annotations.dedup();
        }
    }
    let shown_in: BTreeMap<&String, Vec<&String>> =
        in_view.iter().fold(BTreeMap::new(), |mut acc, (v, list)| {
            for a in list {
                acc.entry(a).or_default().push(v);
            }
            acc
        });
    for annotation in &mut out.annotations {
        if let Some(views) = shown_in.get(&annotation.id) {
            annotation.views = views.iter().map(|v| (*v).clone()).collect();
            annotation.views.sort();
            annotation.views.dedup();
        }
    }

    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jt::pmi::{Association, EndPoint, ModelView};

    fn entity(kind: EntityKind, props: &[(&str, &str)]) -> Entity {
        Entity {
            kind,
            user_label: 1,
            texts: Vec::new(),
            polylines: Vec::new(),
            text_polylines: Vec::new(),
            properties: props
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
            type_name: None,
            valid: true,
        }
    }

    fn view(name: &str, active: bool) -> ModelView {
        ModelView {
            id: 1,
            name: Some(format!("\"{name}\"")),
            active,
            eye_direction: [0.0, 0.0, 1.0],
            angle: 0.0,
            eye_position: [0.0, 0.0, 10.0],
            target: [0.0; 3],
            viewport_diameter: 0.0,
        }
    }

    fn build_one(manager: PmiManager) -> Presentation {
        build(
            &[manager],
            Some("mm"),
            &Links::new(),
            &ExtractOptions::default(),
        )
    }

    #[test]
    fn a_dimension_is_drawn_as_the_kind_it_measures() {
        for (code, want) in [
            ("1", AnnotationKind::LinearDimension),
            ("2", AnnotationKind::AngularDimension),
            ("3", AnnotationKind::RadialDimension),
        ] {
            let e = entity(EntityKind::Dimension, &[("type", code)]);
            assert_eq!(annotation_kind(&e), want);
        }
        // A feature control frame is drawn as its characteristic.
        let fcf = entity(EntityKind::FeatureControlFrame, &[("characteristic", "3")]);
        assert_eq!(annotation_kind(&fcf), AnnotationKind::Position);
        // Lines that locate rather than require are supplemental.
        let centre = entity(EntityKind::Centreline, &[]);
        assert_eq!(
            annotation_kind(&centre),
            AnnotationKind::SupplementalGeometry
        );
    }

    #[test]
    fn a_transform_is_not_an_annotation() {
        let manager = PmiManager {
            entities: vec![
                entity(EntityKind::PartTransform, &[("Description", "move")]),
                entity(EntityKind::ModelViewStyle, &[]),
                entity(EntityKind::Dimension, &[("type", "1")]),
            ],
            cad_tags: vec![0, 1, 2],
            ..Default::default()
        };
        let out = build_one(manager);
        assert_eq!(out.annotations.len(), 1);
        assert_eq!(out.annotations[0].kind, AnnotationKind::LinearDimension);
    }

    #[test]
    fn the_plane_origin_is_converted_from_metres_to_model_units() {
        let e = entity(
            EntityKind::Dimension,
            &[
                ("type", "1"),
                ("DisplayPlane.origin", "0.02635 0.0365 0.025"),
                ("DisplayPlane.xaxis", "1 0 0"),
                ("DisplayPlane.zaxis", "0 0 1"),
            ],
        );
        let p = plane(&e, 1000.0).unwrap();
        assert!((p.origin[0] - 26.35).abs() < 1e-9, "{:?}", p.origin);
        assert!((p.origin[2] - 25.0).abs() < 1e-9, "{:?}", p.origin);
        // The axes are directions, so they are not scaled.
        assert_eq!(p.axis.unwrap().z, 1.0);
        assert_eq!(p.ref_direction.unwrap().x, 1.0);
        // A file in metres needs no conversion at all.
        assert_eq!(plane(&e, 1.0).unwrap().origin[0], 0.02635);
    }

    #[test]
    fn a_colour_is_unpacked_from_the_order_jt_writes_it() {
        // JT packs blue, green, red; the model wants red, green, blue.
        assert_eq!(colour_name("0x00123456").as_deref(), Some("#563412"));
        assert_eq!(colour_name("0x00000000").as_deref(), Some("#000000"));
        assert_eq!(colour_name("not a colour"), None);
    }

    #[test]
    fn glyph_runs_are_not_mistaken_for_text() {
        assert!(!is_readable("\u{2}"));
        assert!(!is_readable("   "));
        assert!(is_readable("Ø12.5"));
        let mut e = entity(EntityKind::Note, &[]);
        e.texts = vec!["\u{2}".into(), "\u{3}".into()];
        let out = build_one(PmiManager {
            entities: vec![e],
            cad_tags: vec![0],
            ..Default::default()
        });
        assert_eq!(out.annotations[0].text, None);
        assert_eq!(out.annotations[0].text_origin, None);
    }

    #[test]
    fn a_view_shows_the_annotations_associated_with_it() {
        let mut dimension = entity(EntityKind::Dimension, &[("type", "1")]);
        dimension.polylines = vec![vec![[0.0; 3], [1.0, 0.0, 0.0]]];
        let manager = PmiManager {
            model_views: vec![view("Top", true), view("Front", false)],
            entities: vec![dimension, entity(EntityKind::Note, &[])],
            // Views take the first tags, then the entities.
            cad_tags: vec![10, 11, 12, 13],
            associations: vec![
                // The first entity is shown in the first view.
                Association {
                    source: EndPoint {
                        kind: EndPoint::GENERIC,
                        index: 12,
                        indirect: true,
                    },
                    destination: EndPoint {
                        kind: EndPoint::MODEL_VIEW,
                        index: 10,
                        indirect: true,
                    },
                    reason: 10,
                },
                // An association between two entities is not membership.
                Association {
                    source: EndPoint {
                        kind: EndPoint::GENERIC,
                        index: 12,
                        indirect: true,
                    },
                    destination: EndPoint {
                        kind: EndPoint::GENERIC,
                        index: 13,
                        indirect: true,
                    },
                    reason: 10,
                },
            ],
            ..Default::default()
        };
        let out = build_one(manager);
        let top = out.views.iter().find(|v| v.name == "Top").unwrap();
        let front = out.views.iter().find(|v| v.name == "Front").unwrap();
        assert_eq!(top.annotations.len(), 1);
        assert!(front.annotations.is_empty());
        assert!(top.default && !front.default);
        // The link is recorded from both ends.
        let shown = out
            .annotations
            .iter()
            .find(|a| a.id == top.annotations[0])
            .unwrap();
        assert_eq!(shown.views, [top.id.as_str()]);
        assert_eq!(shown.kind, AnnotationKind::LinearDimension);
        // The camera looks from the eye position along the eye direction.
        assert_eq!(top.camera.placement.origin, [0.0, 0.0, 10.0]);
        assert_eq!(top.camera.view_plane_distance, Some(10.0));
    }

    #[test]
    fn geometry_is_summarised_by_default_and_given_in_full_on_request() {
        let mut e = entity(EntityKind::Dimension, &[("type", "1")]);
        e.polylines = vec![vec![[0.0; 3], [1.0, 0.0, 0.0], [1.0, 2.0, 0.0]]];
        let manager = PmiManager {
            entities: vec![e],
            cad_tags: vec![0],
            ..Default::default()
        };
        let summary = build_one(manager.clone());
        let a = &summary.annotations[0];
        assert_eq!((a.geometry.polylines, a.geometry.points), (1, 3));
        assert_eq!(a.parts.len(), 1);
        assert_eq!(a.parts[0].form, PartForm::Polyline);
        assert!(a.parts[0].polylines.is_none(), "coordinates by default");

        let full = build(
            &[manager],
            Some("mm"),
            &Links::new(),
            &ExtractOptions {
                presentation_geometry: true,
            },
        );
        assert_eq!(
            full.annotations[0].parts[0]
                .polylines
                .as_ref()
                .unwrap()
                .len(),
            1
        );
        // Asking for the coordinates does not change the summary.
        assert_eq!(full.annotations[0].geometry, a.geometry);
    }

    #[test]
    fn style_is_kept_apart_from_the_property_bag() {
        let e = entity(
            EntityKind::Dimension,
            &[
                ("type", "1"),
                ("LAYER", "1"),
                ("font", "Arial"),
                ("textColour", "0x00000000"),
                ("DisplayPlane.origin", "0 0 0"),
                ("____JtTkOriginReference___1", "1"),
                ("Leader[0].arrowLength", "3.5"),
                ("inspection", "1"),
            ],
        );
        let out = build_one(PmiManager {
            entities: vec![e],
            cad_tags: vec![0],
            ..Default::default()
        });
        let a = &out.annotations[0];
        let style = a.style.as_ref().unwrap();
        assert_eq!(style.layer.as_deref(), Some("1"));
        assert_eq!(style.line_font.as_deref(), Some("Arial"));
        assert_eq!(style.colour.as_deref(), Some("#000000"));
        // What became a field, the writer's bookkeeping, and per-stroke
        // detail all stay out of the bag.
        let kept: Vec<&str> = a.attributes.keys().map(String::as_str).collect();
        assert_eq!(kept, ["textColour"]);
    }
}
