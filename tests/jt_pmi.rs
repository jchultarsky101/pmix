//! PMI extraction from a JT file: the NIST assembly's dimensions,
//! tolerances, and datums come out with the values the model states.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use pmix::model::{DimensionKind, DimensionTolerance, ToleranceKind, ToleranceModifier};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/jt/nist_mtc_assembly.jt")
}

#[test]
fn the_nist_assembly_yields_its_pmi() {
    let doc = pmix::extract(&fixture()).unwrap();

    assert_eq!(doc.source.format, "JT");
    assert!(
        doc.source
            .schema
            .as_deref()
            .is_some_and(|s| s.contains("10.5")),
        "{:?}",
        doc.source.schema
    );
    // The scene graph declares millimetres; PMI measures use it.
    assert_eq!(doc.units.length.as_deref(), Some("mm"));
    assert!(doc.diagnostics.is_empty(), "{:?}", doc.diagnostics);

    let s = &doc.semantic;
    assert_eq!(s.datums.len(), 15);
    assert_eq!(s.tolerances.len(), 16);
    assert!(s.dimensions.len() > 70, "{}", s.dimensions.len());

    // Every dimension states a value in a declared unit, and every
    // tolerance a magnitude.
    for d in &s.dimensions {
        let value = d.value.as_ref().expect("dimension has a value");
        let expected = match d.kind {
            DimensionKind::AngularSize | DimensionKind::AngularLocation => "deg",
            _ => "mm",
        };
        assert_eq!(value.unit, expected, "{}", d.meta.id);
    }
    for t in &s.tolerances {
        assert!(t.value.is_some(), "tolerance {} has no value", t.meta.id);
    }

    // The part is toleranced to datums A, B, and C.
    let labels: BTreeSet<&str> = s.datums.iter().map(|d| d.label.as_str()).collect();
    assert_eq!(labels, BTreeSet::from(["A", "B", "C"]));
    let systems: BTreeSet<&str> = s.datum_systems.iter().map(|d| d.text.as_str()).collect();
    assert_eq!(systems, BTreeSet::from(["A", "A|B", "A|B|C"]));

    // Every referenced datum system exists, and the three-datum one has
    // one datum per compartment in precedence order.
    let ids: BTreeSet<&str> = s.datum_systems.iter().map(|d| d.meta.id.as_str()).collect();
    for t in &s.tolerances {
        if let Some(system) = &t.datum_system {
            assert!(ids.contains(system.as_str()), "{system} is not defined");
        }
    }
    let abc = s
        .datum_systems
        .iter()
        .find(|d| d.text == "A|B|C")
        .expect("A|B|C");
    let order: Vec<&str> = abc
        .compartments
        .iter()
        .flat_map(|c| c.datums.iter().map(|d| d.datum.as_str()))
        .collect();
    // A datum reference names the datum record, which is named for
    // its letter.
    assert_eq!(order, ["datum:A", "datum:B", "datum:C"]);

    // The model uses position, perpendicularity, and flatness, some of
    // them at maximum material condition.
    let kinds: BTreeSet<&str> = s.tolerances.iter().map(|t| t.kind.as_str()).collect();
    assert_eq!(
        kinds,
        BTreeSet::from(["position", "perpendicularity", "flatness"])
    );
    assert!(
        s.tolerances
            .iter()
            .any(|t| t.modifiers.contains(&ToleranceModifier::MaximumMaterial)),
        "no tolerance at maximum material condition"
    );
    assert!(
        s.tolerances
            .iter()
            .any(|t| t.kind == ToleranceKind::Position
                && t.value.as_ref().is_some_and(|v| v.value == 0.1)),
        "no position tolerance of 0.1"
    );

    // Plus and minus deviations and an ISO fit both come through.
    let plus_minus = s
        .dimensions
        .iter()
        .filter(|d| matches!(d.tolerance, Some(DimensionTolerance::PlusMinus { .. })))
        .count();
    assert!(plus_minus > 50, "{plus_minus} toleranced dimensions");
    let fit = s
        .dimensions
        .iter()
        .find_map(|d| match &d.tolerance {
            Some(DimensionTolerance::LimitsAndFits { zone, grade, .. }) => Some((zone, grade)),
            _ => None,
        })
        .expect("an ISO fit");
    assert_eq!((fit.0.as_str(), fit.1.as_str()), ("H", "7"));

    // Nothing recognised was dropped, and ids are unique.
    assert!(doc.unknown.is_empty(), "{:?}", doc.unknown);
    let mut ids: Vec<&str> = s
        .dimensions
        .iter()
        .map(|d| d.meta.id.as_str())
        .chain(s.tolerances.iter().map(|t| t.meta.id.as_str()))
        .chain(s.datums.iter().map(|d| d.meta.id.as_str()))
        .collect();
    let count = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), count, "duplicate ids");
}

#[test]
fn the_nist_assembly_yields_its_annotations_and_views() {
    let doc = pmix::extract(&fixture()).unwrap();
    let p = &doc.presentation;
    assert!(p.annotations.len() > 300, "{}", p.annotations.len());
    assert_eq!(p.views.len(), 89);

    // The saved views the model was built with are all present.
    let names: BTreeSet<&str> = p.views.iter().map(|v| v.name.as_str()).collect();
    for expected in ["Top", "Front", "Isometric", "MBD-Trimetric #1"] {
        assert!(
            names.contains(expected),
            "{expected} missing from {names:?}"
        );
    }
    // Every view frames the model from somewhere.
    for v in &p.views {
        assert!(!v.name.is_empty(), "unnamed view {}", v.id);
        assert!(
            v.camera.placement.axis.is_some(),
            "{} has no direction",
            v.id
        );
    }
    // The views the engineer created to present the PMI each show some
    // of it. The rest are the standard orientations every part carries,
    // which are cameras and nothing more.
    let authored: Vec<_> = p
        .views
        .iter()
        .filter(|v| v.name.starts_with("MBD-"))
        .collect();
    assert_eq!(authored.len(), 16);
    for v in &authored {
        assert!(!v.annotations.is_empty(), "{} shows nothing", v.name);
    }
    let presets = [
        "Top",
        "Bottom",
        "Left",
        "Right",
        "Front",
        "Back",
        "Isometric",
    ];
    assert!(
        p.views
            .iter()
            .filter(|v| presets.contains(&v.name.as_str()))
            .all(|v| v.annotations.is_empty()),
        "an orientation preset carries PMI"
    );

    // Annotations are drawn as the PMI they present.
    let kinds: BTreeSet<&str> = p.annotations.iter().map(|a| a.kind.as_str()).collect();
    for expected in [
        "linear_dimension",
        "radial_dimension",
        "position",
        "flatness",
        "datum",
    ] {
        assert!(
            kinds.contains(expected),
            "{expected} missing from {kinds:?}"
        );
    }

    // An annotation that draws something has a plane, a summary of what
    // it draws, and a bounding box in the model's own units.
    let drawn: Vec<_> = p
        .annotations
        .iter()
        .filter(|a| a.geometry.polylines > 0)
        .collect();
    assert!(drawn.len() > 100, "{}", drawn.len());
    for a in &drawn {
        assert!(a.plane.is_some(), "{} has no plane", a.id);
        assert!(!a.geometry.hash.is_empty(), "{} has no hash", a.id);
        let b = a.geometry.bbox.as_ref().expect("bounding box");
        for i in 0..3 {
            assert!(b.min[i] <= b.max[i], "{} has an inverted box", a.id);
            assert!(b.max[i].abs() < 10_000.0, "{} is not in millimetres", a.id);
        }
        assert_eq!(a.parts.len(), 1);
        // Coordinates are withheld unless asked for.
        assert!(a.parts[0].polylines.is_none());
    }

    // The two layers are linked in both directions, and every id a link
    // names exists.
    let semantic_ids: BTreeSet<&str> = doc
        .semantic
        .dimensions
        .iter()
        .map(|d| d.meta.id.as_str())
        .chain(doc.semantic.tolerances.iter().map(|t| t.meta.id.as_str()))
        .chain(doc.semantic.datums.iter().map(|d| d.meta.id.as_str()))
        .collect();
    let linked = p.annotations.iter().filter(|a| !a.semantic.is_empty());
    let mut linked_count = 0;
    for a in linked {
        linked_count += 1;
        for id in &a.semantic {
            assert!(semantic_ids.contains(id.as_str()), "{id} is not defined");
        }
    }
    assert!(linked_count > 50, "{linked_count} annotations link to PMI");

    let view_ids: BTreeSet<&str> = p.views.iter().map(|v| v.id.as_str()).collect();
    let annotation_ids: BTreeSet<&str> = p.annotations.iter().map(|a| a.id.as_str()).collect();
    let mut shown = 0;
    for a in &p.annotations {
        for v in &a.views {
            shown += 1;
            assert!(view_ids.contains(v.as_str()), "{v} is not a view");
        }
    }
    assert!(shown > 150, "{shown} annotations placed in views");
    for v in &p.views {
        for a in &v.annotations {
            assert!(annotation_ids.contains(a.as_str()), "{a} is not defined");
        }
    }
}

#[test]
fn asking_for_geometry_adds_coordinates_and_changes_nothing_else() {
    let plain = pmix::extract(&fixture()).unwrap();
    let full = pmix::extract_with(
        &fixture(),
        &pmix::ExtractOptions {
            presentation_geometry: true,
        },
    )
    .unwrap();
    assert_eq!(plain.semantic, full.semantic);
    assert_eq!(plain.presentation.views, full.presentation.views);
    assert_eq!(
        plain.presentation.annotations.len(),
        full.presentation.annotations.len()
    );
    let with_coordinates = full
        .presentation
        .annotations
        .iter()
        .filter(|a| a.parts.iter().any(|p| p.polylines.is_some()))
        .count();
    assert!(with_coordinates > 100, "{with_coordinates}");
    // The summaries are the same either way.
    for (a, b) in plain
        .presentation
        .annotations
        .iter()
        .zip(&full.presentation.annotations)
    {
        assert_eq!(a.geometry, b.geometry);
        assert_eq!(a.id, b.id);
    }
}

#[test]
fn scene_graph_properties_reach_the_document() {
    let doc = pmix::extract(&fixture()).unwrap();
    let by_name = |name: &str| {
        doc.properties
            .iter()
            .find(|p| p.name == name)
            .unwrap_or_else(|| panic!("{name} is missing"))
    };
    // The file names the assembly and states the units it is modelled in.
    assert!(matches!(
        &by_name("JT_PROP_MEASUREMENT_UNITS").value,
        pmix::model::PropertyValue::Text { value } if value == "Millimeters"
    ));
    assert!(doc.properties.iter().any(|p| p.name == "JT_PROP_NAME"));
    // Properties about the file rather than the design are marked so the
    // diff can leave them out.
    assert_eq!(
        by_name("JT_PROP_MEASUREMENT_UNITS").kind,
        pmix::model::PropertyKind::Validation
    );
}

#[test]
fn a_part_states_its_material_and_size() {
    let doc = pmix::extract(&fixture()).unwrap();
    let named = |name: &str| -> Vec<&pmix::model::Property> {
        doc.properties
            .iter()
            .filter(|p| p.name.trim_end_matches(':') == name)
            .collect()
    };
    // The assembly is built from several materials, each stated by the
    // part that is made of it.
    let materials: Vec<&str> = named("CAD_MATERIAL")
        .iter()
        .filter_map(|p| match &p.value {
            pmix::model::PropertyValue::Text { value } => Some(value.as_str()),
            _ => None,
        })
        .collect();
    assert!(materials.len() > 5, "{materials:?}");
    assert!(
        materials.iter().any(|m| m.contains("Steel")),
        "{materials:?}"
    );
    assert!(
        materials.iter().any(|m| m.contains("Aluminum")),
        "{materials:?}"
    );
    // A volume written as digits is read as a number, so it compares.
    let volumes = named("CAD_VOLUME");
    assert!(volumes.len() > 5);
    assert!(volumes.iter().all(|p| matches!(
        p.value,
        pmix::model::PropertyValue::Number { .. } | pmix::model::PropertyValue::Integer { .. }
    )));
    // These are design data, so the diff compares them.
    for p in named("CAD_MATERIAL").iter().chain(volumes.iter()) {
        assert_eq!(p.kind, pmix::model::PropertyKind::User);
    }
    // How the file was written is not, so the diff leaves it alone.
    for name in ["Translator Version", "LAYERFILTER000", "PMI_TYPE_TABLE"] {
        for p in named(name) {
            assert_eq!(p.kind, pmix::model::PropertyKind::Validation, "{name}");
        }
    }
    // Every part is named.
    assert!(named("Name").len() > 20);
}

#[test]
fn a_property_says_which_part_states_it() {
    let doc = pmix::extract(&fixture()).unwrap();
    let material = |p: &&pmix::model::Property| p.name.trim_end_matches(':') == "CAD_MATERIAL";
    let attached: Vec<&pmix::model::Property> = doc
        .properties
        .iter()
        .filter(material)
        .filter(|p| p.part.is_some())
        .collect();
    assert!(attached.len() >= 6, "{} materials attached", attached.len());

    // The parts are the ones the assembly is built from.
    let parts: BTreeSet<&str> = attached.iter().filter_map(|p| p.part.as_deref()).collect();
    assert!(
        parts.iter().any(|p| p.contains("HEX NUT")),
        "the fasteners are named: {parts:?}"
    );
    assert!(
        parts.iter().any(|p| p.contains("crada box")),
        "the housings are named: {parts:?}"
    );

    // Two parts made of different things are two facts, not one, and
    // they are told apart by id.
    let ids: BTreeSet<&str> = attached.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids.len(), attached.len(), "a material per part");

    // A volume belongs to the part it measures.
    let volumes: Vec<&pmix::model::Property> = doc
        .properties
        .iter()
        .filter(|p| p.name.trim_end_matches(':') == "CAD_VOLUME" && p.part.is_some())
        .collect();
    assert!(volumes.len() >= 5);
    // Most of the file's properties now say whose they are.
    let with_part = doc.properties.iter().filter(|p| p.part.is_some()).count();
    assert!(
        with_part * 2 > doc.properties.len(),
        "{with_part} of {} properties name a part",
        doc.properties.len()
    );
}

#[test]
fn a_file_compared_with_itself_reports_no_change() {
    let doc = pmix::extract(&fixture()).unwrap();
    let report = pmix::diff::diff(&doc, &doc);
    assert!(report.is_empty(), "{}", report.render_text());
}
