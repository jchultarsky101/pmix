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

#[test]
fn tolerance_datum_basics() {
    check("tolerance_datum_basics");
}

#[test]
fn tolerance_datum_basics_semantics() {
    use pmix::model::{DatumModifier, DatumTargetKind, ToleranceKind, ToleranceModifier, ZoneForm};
    let doc = pmix::extract(&synthetic_dir().join("tolerance_datum_basics.stp")).unwrap();
    let s = &doc.semantic;
    assert!(doc.unknown.is_empty(), "{:?}", doc.unknown);
    assert!(doc.diagnostics.is_empty(), "{:?}", doc.diagnostics);
    assert_eq!(s.datums.len(), 3);
    assert_eq!(s.datum_systems.len(), 4);
    assert_eq!(s.tolerances.len(), 6);

    let datum = |label: &str| s.datums.iter().find(|d| d.label == label).expect("datum");
    let a = datum("A");
    assert_eq!(a.features.len(), 1);
    assert_eq!(a.targets.len(), 1);
    let a1 = &a.targets[0];
    assert_eq!(a1.label, "A1");
    assert_eq!(a1.kind, DatumTargetKind::Point);
    assert_eq!(a1.diameter.as_ref().unwrap().value, 2.0);
    assert_eq!(a1.placement.as_ref().unwrap().origin, [10.0, 10.0, 0.0]);
    assert!(a1.feature.is_some());

    let system = |text: &str| {
        s.datum_systems
            .iter()
            .find(|d| d.text == text)
            .expect("datum system")
    };
    let drf = system("A|B(M)|C");
    assert_eq!(drf.compartments.len(), 3);
    assert_eq!(
        drf.compartments[1].datums[0].modifiers,
        vec![DatumModifier::MaximumMaterial]
    );
    assert_eq!(drf.compartments[1].datums[0].datum, datum("B").meta.id);
    let common = system("A-B");
    assert!(common.compartments[0].common);
    assert_eq!(common.compartments[0].datums.len(), 2);

    let tol = |kind: ToleranceKind| -> Vec<&pmix::model::GeometricTolerance> {
        s.tolerances.iter().filter(|t| t.kind == kind).collect()
    };
    let positions = tol(ToleranceKind::Position);
    assert_eq!(positions.len(), 2);
    let upper = positions.iter().find(|t| t.composite_of.is_none()).unwrap();
    let lower = positions.iter().find(|t| t.composite_of.is_some()).unwrap();
    assert_eq!(upper.value.as_ref().unwrap().value, 0.1);
    assert_eq!(upper.decimal_places, Some(2));
    assert_eq!(upper.modifiers, vec![ToleranceModifier::MaximumMaterial]);
    let zone = upper.zone.as_ref().unwrap();
    assert_eq!(zone.form, Some(ZoneForm::CylindricalOrCircular));
    assert_eq!(zone.projected.as_ref().unwrap().value, 10.0);
    assert_eq!(upper.datum_system.as_deref(), Some(drf.meta.id.as_str()));
    assert_eq!(upper.text.as_deref(), Some("⌖ ⌀0.1 Ⓟ10 Ⓜ | A | B Ⓜ | C"));
    assert_eq!(lower.composite_of.as_deref(), Some(upper.meta.id.as_str()));
    assert_eq!(lower.value.as_ref().unwrap().value, 0.05);

    let flat = &tol(ToleranceKind::Flatness)[0];
    let ub = flat.unit_basis.as_ref().unwrap();
    assert_eq!(
        (ub.length.value, ub.area_type.as_deref()),
        (25.0, Some("square"))
    );

    let perp = &tol(ToleranceKind::Perpendicularity)[0];
    assert_eq!(
        perp.zone.as_ref().unwrap().form,
        Some(ZoneForm::BetweenTwoEquidistantSurfaces)
    );
    assert_eq!(
        perp.datum_system.as_deref(),
        Some(system("A").meta.id.as_str())
    );

    let prof = &tol(ToleranceKind::SurfaceProfile)[0];
    assert_eq!(prof.unequally_disposed.as_ref().unwrap().value, 0.1);
    assert!(prof.modifiers.contains(&ToleranceModifier::FreeState));
    assert!(
        prof.modifiers
            .contains(&ToleranceModifier::UnequallyDisposed)
    );
    assert_eq!(prof.datum_system.as_deref(), Some(common.meta.id.as_str()));

    let runout = &tol(ToleranceKind::CircularRunout)[0];
    let angle = runout.zone.as_ref().unwrap().runout_angle.as_ref().unwrap();
    assert_eq!((angle.value, angle.unit.as_str()), (30.0, "deg"));
}
