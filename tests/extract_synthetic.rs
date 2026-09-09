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
fn assembly_properties() {
    check("assembly_properties");
}

/// An assembly states the same property names of every component, so the
/// part that states one belongs to its identity. Without it they are one
/// record with content-ordered suffixes, and adding a component
/// reshuffles the rest.
#[test]
fn assembly_properties_are_kept_apart_by_the_component_that_states_them() {
    use std::collections::BTreeSet;
    let doc = pmix::extract(&synthetic_dir().join("assembly_properties.stp")).expect("extracts");
    let ids: BTreeSet<&str> = doc.properties.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids.len(), doc.properties.len(), "no two share an id");
    assert!(
        ids.iter().all(|id| !id.contains('-')),
        "no id needed a collision suffix: {ids:?}"
    );

    let named = |name: &str| -> Vec<&str> {
        let mut parts: Vec<&str> = doc
            .properties
            .iter()
            .filter(|p| p.name == name)
            .filter_map(|p| p.part.as_deref())
            .collect();
        parts.sort();
        parts
    };
    assert_eq!(
        named("Material"),
        ["bracket_left", "pin_lower", "pin_upper"]
    );
    assert_eq!(
        named("Mass (g)"),
        ["bracket_left", "pin_lower", "pin_upper"]
    );

    // The two pins are the same product stating the same value, so only
    // the name of each occurrence tells them apart.
    let pins: Vec<&pmix::model::Property> = doc
        .properties
        .iter()
        .filter(|p| p.name == "Material" && p.part.as_deref() != Some("bracket_left"))
        .collect();
    assert_eq!(pins.len(), 2);
    assert_eq!(pins[0].value, pins[1].value);
    assert_ne!(pins[0].id, pins[1].id);

    // A part whose name reads plainly says so in the id.
    assert!(
        doc.properties
            .iter()
            .any(|p| p.id == "prop:pin_upper.Material"),
        "{ids:?}"
    );
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
    // `hole`, `hole 1`, and the single-member `pattern` all sit on the same
    // cylindrical face and merge into one feature (ADR 0004).
    assert_eq!(s.features.len(), 3);

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
    // Merged from `hole` and `hole 1`; the survivor is chosen by content hash.
    assert!(
        hole.name.as_deref().is_some_and(|n| n.starts_with("hole")),
        "{:?}",
        hole.name
    );
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
    // The radius on the pattern and the diameter on the hole resolve to
    // the same merged feature.
    assert_eq!(radius.features[0], dia.features[0]);

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

#[test]
fn presentation_basics() {
    check("presentation_basics");
}

#[test]
fn presentation_basics_semantics() {
    use pmix::model::{AnnotationKind, PartForm, TextOrigin};
    let doc = pmix::extract(&synthetic_dir().join("presentation_basics.stp")).unwrap();
    let p = &doc.presentation;
    assert!(doc.unknown.is_empty(), "{:?}", doc.unknown);
    assert!(doc.diagnostics.is_empty(), "{:?}", doc.diagnostics);
    assert_eq!(p.annotations.len(), 4);
    assert_eq!(p.views.len(), 2);

    let ann = |kind: AnnotationKind| {
        p.annotations
            .iter()
            .find(|a| a.kind == kind)
            .expect("annotation")
    };

    let pos = ann(AnnotationKind::Position);
    assert_eq!(pos.label.as_deref(), Some("Position.1"));
    assert_eq!(pos.text.as_deref(), Some("⌖ ⌀0.1 | A"));
    assert_eq!(pos.text_origin, Some(TextOrigin::Semantic));
    assert_eq!(pos.plane.as_ref().unwrap().origin, [20.0, 20.0, 0.0]);
    assert_eq!(
        (
            pos.geometry.polylines,
            pos.geometry.triangles,
            pos.geometry.points
        ),
        (1, 1, 8)
    );
    assert_eq!(pos.geometry.bbox.as_ref().unwrap().max, [24.0, 22.0, 0.0]);
    assert_eq!(
        pos.style.as_ref().unwrap().colour.as_deref(),
        Some("#ff0000")
    );
    assert_eq!(
        pos.style.as_ref().unwrap().layer.as_deref(),
        Some("PMI layer")
    );
    assert_eq!(pos.parts.len(), 1);
    assert_eq!(pos.parts[0].form, PartForm::Tessellated);
    assert!(
        pos.parts[0].polylines.is_none(),
        "geometry is summarised by default"
    );
    let tol = &doc.semantic.tolerances[0];
    assert_eq!(pos.semantic, vec![tol.meta.id.clone()]);
    assert_eq!(tol.meta.presentation, vec![pos.id.clone()]);
    assert_eq!(pos.features.len(), 1);
    assert_eq!(pos.views.len(), 2);

    let dia = ann(AnnotationKind::DiameterDimension);
    assert_eq!(dia.text.as_deref(), Some("⌀10 ±0.05"));
    assert_eq!(dia.text_origin, Some(TextOrigin::Explicit));
    let dim = &doc.semantic.dimensions[0];
    assert_eq!(dim.text.as_deref(), Some("⌀10 ±0.05"));
    assert_eq!(dia.semantic, vec![dim.meta.id.clone()]);

    let datum = ann(AnnotationKind::Datum);
    assert_eq!(datum.text.as_deref(), Some("A"));
    let ph = datum.placeholder.as_ref().unwrap();
    assert_eq!(ph.box_size, Some([6.0, 3.0]));
    assert_eq!(ph.role.as_deref(), Some("gps_data"));
    assert_eq!(ph.text_height.as_ref().unwrap().value, 3.0);
    assert_eq!(datum.leaders.len(), 1);
    assert_eq!(
        datum.leaders[0].points,
        vec![[8.0, -8.5, 0.0], [8.0, 0.0, 0.0]]
    );
    assert_eq!(
        datum.leaders[0].terminator.as_deref(),
        Some("internal_pair_forward_arrowhead")
    );
    // The related callout's polyline merged into the same annotation.
    assert_eq!(datum.parts.len(), 2);
    assert_eq!(datum.geometry.polylines, 1);
    assert_eq!(datum.semantic, vec![doc.semantic.datums[0].meta.id.clone()]);

    let axis = p
        .annotations
        .iter()
        .find(|a| a.kind.as_str() == "Axis")
        .expect("standalone occurrence");
    assert!(axis.semantic.is_empty());
    assert_eq!(axis.source_refs, vec!["#144".to_string()]);

    let view = |name: &str| p.views.iter().find(|v| v.name == name).expect("view");
    let mbd = view("MBD_A");
    assert_eq!(mbd.camera.projection.as_str(), "parallel");
    assert_eq!(mbd.camera.view_window, Some([200.0, 150.0]));
    assert_eq!(mbd.annotations.len(), 2);
    assert!(!mbd.default);
    let iso = view("Isometric");
    assert!(iso.default);
    assert_eq!(
        iso.annotations.len(),
        4,
        "mapped item resolves to the global draughting model"
    );

    let full = pmix::extract_with(
        &synthetic_dir().join("presentation_basics.stp"),
        &pmix::ExtractOptions {
            presentation_geometry: true,
        },
    )
    .unwrap();
    let pos = full
        .presentation
        .annotations
        .iter()
        .find(|a| a.kind == AnnotationKind::Position)
        .unwrap();
    assert_eq!(pos.parts[0].polylines.as_ref().unwrap()[0].len(), 5);
    assert_eq!(
        pos.parts[0].triangles.as_ref().unwrap(),
        &vec![[0u32, 1, 2]]
    );
}

#[test]
fn property_basics() {
    check("property_basics");
}

#[test]
fn property_basics_semantics() {
    use pmix::model::{PropertyKind, PropertyValue};
    let doc = pmix::extract(&synthetic_dir().join("property_basics.stp")).unwrap();
    assert!(doc.unknown.is_empty(), "{:?}", doc.unknown);
    assert!(doc.diagnostics.is_empty(), "{:?}", doc.diagnostics);
    // Nine user properties, plus three validation values across two
    // validation representations.
    assert_eq!(doc.properties.len(), 12);

    let by_name = |name: &str| {
        doc.properties
            .iter()
            .find(|p| p.name == name)
            .unwrap_or_else(|| panic!("no property named {name}"))
    };
    // A property about the part says which part, so an assembly's
    // components do not collapse into one record. One about a PMI
    // record is about that record rather than about a part.
    assert!(
        doc.properties
            .iter()
            .filter(|p| p.applies_to.is_none())
            .all(|p| p.part.as_deref() == Some("pmix synthetic part")),
        "a product-level property names its part"
    );
    assert!(
        doc.properties
            .iter()
            .filter(|p| p.applies_to.is_some())
            .all(|p| p.part.is_none()),
        "a property about a record names no part"
    );

    // Product-level user properties, one per value type.
    let part = by_name("Part_Number");
    assert_eq!(part.kind, PropertyKind::User);
    assert_eq!(part.category.as_deref(), Some("PLM__Part_Number"));
    assert!(part.applies_to.is_none());
    assert_eq!(
        part.value,
        PropertyValue::Text {
            value: "SYN-004-REV-A".into()
        }
    );
    assert_eq!(
        by_name("Batch_Size").value,
        PropertyValue::Integer { value: 250 }
    );
    assert_eq!(
        by_name("Unit_Price").value,
        PropertyValue::Number { value: 18.75 }
    );
    assert_eq!(
        by_name("Release_Approved").value,
        PropertyValue::Boolean { value: true }
    );

    // A validation property attached to the tolerance, with a derived unit.
    let tolerance = &doc.semantic.tolerances[0].meta.id;
    let area = doc
        .properties
        .iter()
        .find(|p| p.name == "affected area")
        .expect("affected area");
    assert_eq!(area.kind, PropertyKind::Validation);
    assert_eq!(area.applies_to.as_deref(), Some(tolerance.as_str()));
    assert_eq!(
        area.value,
        PropertyValue::Measure {
            value: 120.5,
            unit: "mm2".into()
        }
    );
    // The other item of the same representation is its own property.
    let count = doc
        .properties
        .iter()
        .find(|p| p.name == "number of PMI presentation elements")
        .expect("element count");
    assert_eq!(count.category, area.category);
    assert_eq!(count.applies_to, area.applies_to);

    // Descriptive values that are exactly a number's own rendering are
    // read as numbers; formatting that a number would drop stays text.
    assert_eq!(
        by_name("Part_Count").value,
        PropertyValue::Integer { value: 12 }
    );
    assert_eq!(
        by_name("Nominal_Mass_kg").value,
        PropertyValue::Number { value: 2.5 }
    );
    assert_eq!(
        by_name("Serial_Number").value,
        PropertyValue::Text {
            value: "007".into()
        },
        "a leading zero is part of the serial, not a count"
    );
    assert_eq!(
        by_name("Legacy_Code").value,
        PropertyValue::Text {
            value: "1e5".into()
        }
    );
    assert_eq!(
        by_name("Bbox_X_mm").value,
        PropertyValue::Number { value: 64.0 },
        "a whole measurement is still a number, so the field's type is stable"
    );

    // Cubic millimetres resolve through the derived unit.
    let volume = doc
        .properties
        .iter()
        .find(|p| p.name == "volume measure")
        .expect("volume");
    assert_eq!(
        volume.value,
        PropertyValue::Measure {
            value: 1234.5,
            unit: "mm3".into()
        }
    );
    assert_eq!(volume.kind, PropertyKind::Validation);
}
