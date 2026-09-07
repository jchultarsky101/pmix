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
    assert_eq!(order, ["A", "B", "C"]);

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
fn a_file_compared_with_itself_reports_no_change() {
    let doc = pmix::extract(&fixture()).unwrap();
    let report = pmix::diff::diff(&doc, &doc);
    assert!(report.is_empty(), "{}", report.render_text());
}
