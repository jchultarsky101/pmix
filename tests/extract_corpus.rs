//! Consistency of the STEP reader over the NIST corpus: every dimension
//! entity becomes a dimension record, nothing in a handled family is left
//! unconsumed, and spot values match the files.

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

        let leaked: Vec<_> = doc
            .unknown
            .iter()
            .filter(|u| u.reason.contains("dimension"))
            .map(|u| format!("{} {}", u.source_ref, u.kind))
            .collect();
        assert!(
            leaked.is_empty(),
            "{name}: unconsumed dimension entities: {leaked:?}"
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
    // Pending families are reported, not dropped.
    assert!(
        doc.unknown
            .iter()
            .any(|u| u.kind.contains("GEOMETRIC_TOLERANCE"))
    );
    assert!(doc.unknown.iter().any(|u| u.kind == "DATUM"));
}
