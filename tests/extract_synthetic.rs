//! Controlled tests: each synthetic fixture exercises specific constructs
//! and must produce exactly the recorded JSON.
//!
//! To refresh an expectation after a deliberate change:
//! `cargo run -- extract tests/fixtures/synthetic/<name>.stp > tests/fixtures/synthetic/<name>.expected.json`

use std::path::{Path, PathBuf};

fn synthetic_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/synthetic")
}

fn check(name: &str) {
    let dir = synthetic_dir();
    let doc = pmix::extract(&dir.join(format!("{name}.stp"))).expect("extracts");
    let actual = serde_json::to_value(&doc).unwrap();
    let expected: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join(format!("{name}.expected.json"))).expect("expected json"),
    )
    .unwrap();
    if actual != expected {
        let a = serde_json::to_string_pretty(&actual).unwrap();
        let e = serde_json::to_string_pretty(&expected).unwrap();
        panic!("{name}: output differs from expected\n--- expected ---\n{e}\n--- actual ---\n{a}");
    }
}

#[test]
fn dimension_basics() {
    check("dimension_basics");
}

#[test]
fn dimension_basics_semantics() {
    use pmix::model::{
        DimensionKind, DimensionModifier, DimensionQualifier, DimensionSubtype, DimensionTolerance,
    };
    let doc = pmix::extract(&synthetic_dir().join("dimension_basics.stp")).unwrap();
    let s = &doc.semantic;
    assert_eq!(doc.units.length.as_deref(), Some("mm"));
    assert_eq!(doc.units.angle.as_deref(), Some("deg"));
    assert!(doc.unknown.is_empty(), "{:?}", doc.unknown);
    assert!(doc.diagnostics.is_empty(), "{:?}", doc.diagnostics);
    assert_eq!(s.dimensions.len(), 5);
    assert_eq!(s.features.len(), 5);

    let find = |f: &dyn Fn(&pmix::model::Dimension) -> bool| {
        s.dimensions
            .iter()
            .find(|d| f(d))
            .expect("dimension present")
    };
    let dia = find(&|d| {
        d.subtype == DimensionSubtype::Diameter
            && matches!(d.tolerance, Some(DimensionTolerance::PlusMinus { .. }))
    });
    assert_eq!(dia.kind, DimensionKind::Size);
    assert_eq!(dia.value.as_ref().unwrap().value, 12.5);
    assert_eq!(dia.decimal_places, Some(2));
    let Some(DimensionTolerance::PlusMinus { lower, upper }) = &dia.tolerance else {
        unreachable!()
    };
    assert_eq!((lower.value, upper.value), (-0.05, 0.05));
    let hole = s
        .features
        .iter()
        .find(|f| f.meta.id == dia.features[0])
        .unwrap();
    assert_eq!(hole.name.as_deref(), Some("hole"));
    assert_eq!(
        hole.geometry[0].surface.as_ref().map(|s| s.as_str()),
        Some("cylinder")
    );

    let basic = find(&|d| d.kind == DimensionKind::Location);
    assert_eq!(basic.value.as_ref().unwrap().value, 40.0);
    assert_eq!(basic.qualifier, Some(DimensionQualifier::Basic));
    assert_eq!(basic.features.len(), 2);

    let radius = find(&|d| d.subtype == DimensionSubtype::Radius);
    let lim = radius.limits.as_ref().unwrap();
    assert_eq!((lim.lower.value, lim.upper.value), (6.2, 6.3));
    let pattern = s
        .features
        .iter()
        .find(|f| f.meta.id == radius.features[0])
        .unwrap();
    assert_eq!(pattern.kind.as_str(), "composite");
    assert_eq!(pattern.members.len(), 1);

    let angle = find(&|d| d.kind == DimensionKind::AngularSize);
    assert_eq!(angle.value.as_ref().unwrap().unit, "deg");
    assert_eq!(angle.value.as_ref().unwrap().value, 30.0);

    let fit = find(&|d| matches!(d.tolerance, Some(DimensionTolerance::LimitsAndFits { .. })));
    let Some(DimensionTolerance::LimitsAndFits { form, grade, .. }) = &fit.tolerance else {
        unreachable!()
    };
    assert_eq!((form.as_str(), grade.as_str()), ("H", "7"));
    assert_eq!(fit.modifiers, vec![DimensionModifier::Statistical]);
}
