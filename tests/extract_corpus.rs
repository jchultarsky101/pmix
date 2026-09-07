//! Consistency of the STEP reader over the NIST corpus: every dimension,
//! tolerance, datum, and datum system entity becomes a record, nothing in
//! a handled family is left unconsumed, and spot values match the files.

use std::path::{Path, PathBuf};

use pmix::model::{DimensionKind, DimensionTolerance};
use pmix::step::p21;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/nist")
}

fn fixtures() -> Vec<PathBuf> {
    let mut v: Vec<_> = std::fs::read_dir(fixture_dir())
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "stp"))
        .collect();
    v.sort();
    v
}

#[test]
fn every_dimension_entity_becomes_a_record() {
    for path in fixtures() {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let bytes = std::fs::read(&path).unwrap();
        let ex = p21::parse_bytes(&bytes).unwrap();
        let doc = pmix::extract(&path).unwrap();

        let expected = [
            "DIMENSIONAL_SIZE",
            "DIMENSIONAL_SIZE_WITH_PATH",
            "DIMENSIONAL_SIZE_WITH_DATUM_FEATURE",
            "DIMENSIONAL_LOCATION",
            "DIMENSIONAL_LOCATION_WITH_PATH",
            "DIRECTED_DIMENSIONAL_LOCATION",
            "ANGULAR_SIZE",
            "ANGULAR_LOCATION",
        ]
        .iter()
        .map(|k| ex.count_of_type(k))
        .sum::<usize>();
        assert_eq!(
            doc.semantic.dimensions.len(),
            expected,
            "{name}: dimension count"
        );

        let with_tolerance = doc
            .semantic
            .dimensions
            .iter()
            .filter(|d| d.tolerance.is_some())
            .count();
        assert_eq!(
            with_tolerance,
            ex.count_of_type("PLUS_MINUS_TOLERANCE"),
            "{name}: tolerance count"
        );

        for d in &doc.semantic.dimensions {
            let want = match d.kind {
                DimensionKind::Size | DimensionKind::AngularSize => 1,
                DimensionKind::Location | DimensionKind::AngularLocation => 2,
            };
            assert_eq!(
                d.features.len(),
                want,
                "{name}: {} features on {:?}",
                d.meta.id,
                d.meta.source_refs
            );
        }

        // Tolerances: one record per instance in either form.
        let tolerance_entities = ex
            .instances()
            .filter(|i| {
                i.has_type("GEOMETRIC_TOLERANCE")
                    || i.type_names().any(|t| {
                        t.ends_with("_TOLERANCE")
                            && !matches!(
                                t,
                                "PLUS_MINUS_TOLERANCE" | "GEOMETRIC_TOLERANCE_RELATIONSHIP"
                            )
                    })
            })
            .filter(|i| !i.has_type("PLUS_MINUS_TOLERANCE"))
            .count();
        assert_eq!(
            doc.semantic.tolerances.len(),
            tolerance_entities,
            "{name}: tolerance count"
        );
        for t in &doc.semantic.tolerances {
            assert!(
                t.value.is_some(),
                "{name}: tolerance {:?} has no magnitude",
                t.meta.source_refs
            );
            assert_eq!(
                t.features.len(),
                1,
                "{name}: tolerance {:?} feature count",
                t.meta.source_refs
            );
        }
        let with_datums = doc
            .semantic
            .tolerances
            .iter()
            .filter(|t| t.datum_system.is_some())
            .count();
        let datum_ref_entities = ex.count_of_type("GEOMETRIC_TOLERANCE_WITH_DATUM_REFERENCE")
            + ex.instances()
                .filter(|i| {
                    !i.is_complex()
                        && i.parameters().len() == 5
                        && i.type_names().any(|t| t.ends_with("_TOLERANCE"))
                })
                .count();
        assert_eq!(
            with_datums, datum_ref_entities,
            "{name}: tolerances with datum references"
        );
        let composites = doc
            .semantic
            .tolerances
            .iter()
            .filter(|t| t.composite_of.is_some())
            .count();
        assert_eq!(
            composites,
            ex.count_of_type("GEOMETRIC_TOLERANCE_RELATIONSHIP"),
            "{name}: composite count"
        );

        // Datums and datum systems.
        assert_eq!(
            doc.semantic.datums.len(),
            ex.count_of_type("DATUM"),
            "{name}: datum count"
        );
        assert_eq!(
            doc.semantic.datum_systems.len(),
            ex.count_of_type("DATUM_SYSTEM"),
            "{name}: datum system count"
        );
        let targets: usize = doc.semantic.datums.iter().map(|d| d.targets.len()).sum();
        assert_eq!(
            targets,
            ex.count_of_type("DATUM_TARGET") + ex.count_of_type("PLACED_DATUM_TARGET_FEATURE"),
            "{name}: datum target count"
        );
        for ds in &doc.semantic.datum_systems {
            assert!(
                !ds.compartments.is_empty(),
                "{name}: empty datum system {:?}",
                ds.meta.source_refs
            );
            assert!(
                !ds.text.contains('?'),
                "{name}: unresolved datum in {}",
                ds.text
            );
        }

        // Presentation: one annotation per top-level callout plus one per
        // occurrence outside any callout; one view per camera; every
        // association consumed.
        let related: std::collections::HashSet<u64> = ex
            .of_type("DRAUGHTING_CALLOUT_RELATIONSHIP")
            .filter_map(|r| r.parameters().get(3).and_then(|p| p.as_ref()))
            .collect();
        let top_callouts = ex
            .of_type("DRAUGHTING_CALLOUT")
            .filter(|c| !related.contains(&c.id))
            .count();
        let in_callout: std::collections::HashSet<u64> = ex
            .of_type("DRAUGHTING_CALLOUT")
            .flat_map(|c| {
                let mut v = Vec::new();
                if let Some(p) = c.parameters().get(1) {
                    p.collect_refs(&mut v);
                }
                v
            })
            .collect();
        let standalone = ex
            .instances()
            .filter(|i| {
                i.type_names()
                    .any(|t| t.ends_with("_OCCURRENCE") && t.contains("ANNOTATION"))
                    && !in_callout.contains(&i.id)
            })
            .count();
        assert_eq!(
            doc.presentation.annotations.len(),
            top_callouts + standalone,
            "{name}: annotation count"
        );
        let cameras = ex.count_of_type("CAMERA_MODEL_D3")
            + ex.count_of_type("CAMERA_MODEL_D3_MULTI_CLIPPING");
        assert_eq!(doc.presentation.views.len(), cameras, "{name}: view count");
        for a in &doc.presentation.annotations {
            assert!(
                a.plane.is_some(),
                "{name}: annotation {:?} has no plane",
                a.source_refs
            );
            assert!(
                !a.parts.is_empty(),
                "{name}: annotation {:?} has no parts",
                a.source_refs
            );
        }
        let linked_semantic = doc
            .semantic
            .tolerances
            .iter()
            .map(|t| &t.meta)
            .chain(doc.semantic.dimensions.iter().map(|d| &d.meta))
            .chain(doc.semantic.datums.iter().map(|d| &d.meta))
            .filter(|m| !m.presentation.is_empty())
            .count();
        let with_semantic = doc
            .presentation
            .annotations
            .iter()
            .filter(|a| !a.semantic.is_empty())
            .count();
        let has_semantic_records = !doc.semantic.tolerances.is_empty()
            || !doc.semantic.dimensions.is_empty()
            || !doc.semantic.datums.is_empty();
        assert!(
            !has_semantic_records || (linked_semantic > 0 && with_semantic > 0),
            "{name}: no semantic/presentation links resolved"
        );

        // Nothing in a handled family may leak.
        let leaked: Vec<_> = doc
            .unknown
            .iter()
            .filter(|u| u.reason.contains("walker"))
            .map(|u| format!("{} {}", u.source_ref, u.kind))
            .collect();
        assert!(
            leaked.is_empty(),
            "{name}: unconsumed PMI entities: {leaked:?}"
        );

        // The corpus mixes metric and inch (ASME) models.
        assert!(
            matches!(doc.units.length.as_deref(), Some("mm" | "in")),
            "{name}: length unit {:?}",
            doc.units.length
        );
        assert!(
            doc.diagnostics
                .iter()
                .all(|d| !d.message.contains("undefined")),
            "{name}: {:?}",
            doc.diagnostics
        );
    }
}

#[test]
fn ctc01_spot_values() {
    let doc = pmix::extract(&fixture_dir().join("nist_ctc_01_asme1_ap242-e1.stp")).unwrap();
    let dims = &doc.semantic.dimensions;
    // #120 = DIMENSIONAL_SIZE('diameter') 35 mm, tolerance -0.2 / 0.
    let d = dims
        .iter()
        .find(|d| d.meta.source_refs.contains(&"#120".to_string()))
        .expect("#120 extracted");
    assert_eq!(d.value.as_ref().unwrap().value, 35.0);
    assert_eq!(d.value.as_ref().unwrap().unit, "mm");
    let Some(DimensionTolerance::PlusMinus { lower, upper }) = &d.tolerance else {
        panic!("expected plus/minus on #120, got {:?}", d.tolerance);
    };
    assert_eq!((lower.value, upper.value), (-0.2, 0.0));
    // The angular location is in degrees.
    let a = dims
        .iter()
        .find(|d| d.kind == DimensionKind::AngularLocation)
        .unwrap();
    assert_eq!(a.value.as_ref().unwrap().unit, "deg");
    assert_eq!(a.value.as_ref().unwrap().value, 60.0);
    // CTC 01 has datums A, B, C and datum systems A|B|C and A.
    let mut labels: Vec<&str> = doc
        .semantic
        .datums
        .iter()
        .map(|d| d.label.as_str())
        .collect();
    labels.sort();
    assert_eq!(labels, ["A", "B", "C"]);
    let texts: Vec<&str> = doc
        .semantic
        .datum_systems
        .iter()
        .map(|d| d.text.as_str())
        .collect();
    assert!(
        texts.contains(&"A|B|C") && texts.contains(&"A"),
        "{texts:?}"
    );
    for d in &doc.semantic.datums {
        assert!(
            !d.features.is_empty(),
            "datum {} has no datum feature",
            d.label
        );
    }
    // #21 = position 0.75 to A|B|C.
    let t = doc
        .semantic
        .tolerances
        .iter()
        .find(|t| t.meta.source_refs.contains(&"#21".to_string()))
        .expect("#21 extracted");
    assert_eq!(t.kind.as_str(), "position");
    assert_eq!(t.value.as_ref().unwrap().value, 0.75);
    let ds = doc
        .semantic
        .datum_systems
        .iter()
        .find(|d| Some(&d.meta.id) == t.datum_system.as_ref())
        .expect("datum system resolved");
    assert_eq!(ds.text, "A|B|C");
}
