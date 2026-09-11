//! Reading a file's product structure (ADR 0014).
//!
//! The two synthetic assemblies are the two shapes a bill of materials
//! takes: parts used once each, and one part used several times at
//! several places. The second is the one that matters, because a reader
//! that collapses occurrences answers "how many of these does it take?"
//! with one whatever the truth is.

use std::path::{Path, PathBuf};

use pmix::features;
use pmix::product::{self, Part, ProductDocument};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn read(name: &str) -> ProductDocument {
    product::read_path(&fixture(name)).expect("the fixture reads")
}

/// The part whose number is `number`.
fn part<'a>(doc: &'a ProductDocument, number: &str) -> &'a Part {
    doc.parts
        .iter()
        .find(|p| p.number.as_deref() == Some(number))
        .unwrap_or_else(|| panic!("no part numbered {number}"))
}

#[test]
fn an_assembly_names_every_part_and_every_occurrence() {
    let doc = read("synthetic/assembly_two_parts.stp");
    assert_eq!(doc.parts.len(), 3, "the assembly and its two components");
    assert_eq!(doc.relations.len(), 2);

    let plate = part(&doc, "SYN-PLATE");
    assert_eq!(plate.name.as_deref(), Some("base_plate"));
    assert_eq!(plate.revision.as_deref(), Some("A"));
    assert_eq!(plate.occurrences, 1);

    // A component and its assembly can carry different revisions, and
    // the document keeps them apart.
    assert_eq!(part(&doc, "SYN-BLOCK").revision.as_deref(), Some("B"));
    assert_eq!(part(&doc, "SYN-ASM-2").revision.as_deref(), Some("A"));
}

#[test]
fn the_assembly_is_the_one_part_nothing_uses() {
    let doc = read("synthetic/assembly_two_parts.stp");
    assert_eq!(doc.roots, vec![part(&doc, "SYN-ASM-2").id.clone()]);
    assert_eq!(part(&doc, "SYN-ASM-2").occurrences, 0);
}

#[test]
fn a_part_used_three_times_is_counted_three_times() {
    let doc = read("synthetic/assembly_repeated_part.stp");
    let pin = part(&doc, "SYN-PIN");
    assert_eq!(pin.occurrences, 3);

    // One part, three uses: the parts list says the pin once and the
    // relations say it three times.
    let uses: Vec<_> = doc.relations.iter().filter(|r| r.child == pin.id).collect();
    assert_eq!(uses.len(), 3);
}

#[test]
fn each_occurrence_states_where_it_sits() {
    let doc = read("synthetic/assembly_repeated_part.stp");
    let pin = part(&doc, "SYN-PIN");
    let mut origins: Vec<[f64; 3]> = doc
        .relations
        .iter()
        .filter(|r| r.child == pin.id)
        .map(|r| r.placement.as_ref().expect("a placement").origin)
        .collect();
    origins.sort_by(|a, b| a[0].total_cmp(&b[0]));
    assert_eq!(
        origins,
        vec![[8.0, 10.0, 6.0], [28.0, 10.0, 6.0], [48.0, 10.0, 6.0]]
    );
}

#[test]
fn two_uses_of_one_part_are_two_records() {
    let doc = read("synthetic/assembly_repeated_part.stp");
    let pin = part(&doc, "SYN-PIN");
    let ids: Vec<&str> = doc
        .relations
        .iter()
        .filter(|r| r.child == pin.id)
        .map(|r| r.id.as_str())
        .collect();
    let mut distinct = ids.clone();
    distinct.sort_unstable();
    distinct.dedup();
    assert_eq!(
        distinct.len(),
        ids.len(),
        "occurrences of one part must not collapse into one id: {ids:?}"
    );
}

#[test]
fn an_occurrence_carries_the_name_it_is_used_under() {
    let doc = read("synthetic/assembly_repeated_part.stp");
    let mut names: Vec<&str> = doc
        .relations
        .iter()
        .filter_map(|r| r.name.as_deref())
        .collect();
    names.sort_unstable();
    assert_eq!(
        names,
        vec!["pin_left", "pin_middle", "pin_right", "plate_1"]
    );
}

#[test]
fn a_file_holding_one_part_names_it_and_states_no_occurrence() {
    let doc = read("nist/nist_ctc_01_asme1_ap242-e1.stp");
    assert_eq!(doc.parts.len(), 1);
    assert!(doc.relations.is_empty());
    assert_eq!(doc.parts[0].number.as_deref(), Some("NIST Test Case 1"));
    assert_eq!(doc.roots.len(), 1);
}

/// This file states its formation as
/// `PRODUCT_DEFINITION_FORMATION_WITH_SPECIFIED_SOURCE`, a subtype. A
/// reader that matches the plain type name alone reaches no product and
/// leaves every part in the NIST corpus anonymous.
#[test]
fn a_formation_stated_as_a_subtype_still_reaches_its_product() {
    let doc = read("nist/nist_ctc_01_asme1_ap242-e1.stp");
    assert!(
        doc.parts[0].number.is_some(),
        "the part should be named through the formation subtype"
    );
}

/// Every part in the corpus is either named, or the document says why
/// it is not. The second case is real: `nist_ftc_09` states four
/// products with an empty number, name and revision apiece.
#[test]
fn every_part_in_the_nist_corpus_is_named_or_accounted_for() {
    let dir = fixture("nist");
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).expect("the corpus is there") {
        let path = entry.expect("an entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("stp") {
            continue;
        }
        let doc = product::read_path(&path).expect("the file reads");
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            !doc.parts.is_empty(),
            "{name} should hold at least one part"
        );
        let anonymous = doc
            .parts
            .iter()
            .filter(|p| p.number.is_none() && p.name.is_none() && p.revision.is_none())
            .count();
        if anonymous > 1 {
            assert!(
                doc.diagnostics
                    .iter()
                    .any(|d| d.message.contains("no number")),
                "{name} leaves {anonymous} parts anonymous without saying so"
            );
        } else {
            assert!(
                doc.parts.iter().any(|p| p.number.is_some()),
                "{name} should name its part"
            );
        }
        checked += 1;
    }
    assert!(checked >= 17, "only checked {checked} files");
}

/// A part that cannot be identified is a part whose id rests on file
/// order, which is the one thing ADR 0004 forbids an id to rest on. The
/// reader cannot fix it, so it says so.
#[test]
fn parts_with_nothing_to_be_called_by_are_reported() {
    let doc = read("nist/nist_ftc_09_asme1_ap242-e1.stp");
    assert!(doc.parts.len() > 1);
    assert!(
        doc.diagnostics
            .iter()
            .any(|d| d.message.contains("no number")),
        "{:?}",
        doc.diagnostics
    );
}

#[test]
fn the_document_is_sorted_and_repeatable() {
    let a = read("synthetic/assembly_repeated_part.stp");
    let b = read("synthetic/assembly_repeated_part.stp");
    assert_eq!(a, b, "two reads of one file must agree");

    let ids: Vec<&str> = a.parts.iter().map(|p| p.id.as_str()).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(ids, sorted, "parts are sorted by id");

    let uses: Vec<&str> = a.relations.iter().map(|r| r.id.as_str()).collect();
    let mut sorted = uses.clone();
    sorted.sort_unstable();
    assert_eq!(uses, sorted, "relations are sorted by id");
}

/// A part's identity is what a person would call it, so it does not move
/// when the file's entity numbering does (ADR 0004).
#[test]
fn a_part_keys_on_its_number_name_and_revision() {
    let doc = read("synthetic/assembly_two_parts.stp");
    let renumbered = std::fs::read_to_string(fixture("synthetic/assembly_two_parts.stp"))
        .expect("the fixture reads")
        // Shift every reference and definition by a constant, which is
        // what a different exporter's numbering amounts to.
        .replace('#', "#1000");
    let shifted = product::from_step(renumbered.as_bytes(), "renumbered.stp")
        .expect("the renumbered file reads");
    let ids = |d: &ProductDocument| -> Vec<String> {
        let mut v: Vec<String> = d.parts.iter().map(|p| p.id.clone()).collect();
        v.sort();
        v
    };
    assert_eq!(ids(&doc), ids(&shifted));
}

// --- JT product structure (ADR 0014) ---

/// The same document, from the scene graph's node hierarchy instead of
/// from product definitions and assembly usages.
#[test]
fn a_jt_assembly_names_its_parts_and_its_occurrences() {
    let doc = read("jt/nist_mtc_assembly.jt");
    assert_eq!(doc.parts.len(), 14);
    assert_eq!(doc.relations.len(), 57);
    assert!(doc.diagnostics.is_empty(), "{:?}", doc.diagnostics);
    assert!(
        doc.parts.iter().all(|p| p.number.is_some()),
        "every part should be named"
    );
}

/// The occurrence suffix a writer appends is that *use* of the part, not
/// the part, and it changes between exports. A part's identity must not.
#[test]
fn a_jt_part_name_drops_the_occurrence_suffix() {
    let doc = read("jt/nist_mtc_assembly.jt");
    for part in &doc.parts {
        let name = part.number.as_deref().unwrap_or_default();
        assert!(!name.contains(';'), "{name} still carries its occurrence");
    }
}

/// The scene graph's transforms are in the unit the file declares, not
/// in JT's base unit of metres. Reading them as metres puts a 148mm
/// assembly 22 metres from the origin.
#[test]
fn jt_placements_are_inside_the_assembly() {
    let doc = read("jt/nist_mtc_assembly.jt");
    assert_eq!(doc.units.declared_length.as_deref(), Some("mm"));
    let placed: Vec<&[f64; 3]> = doc
        .relations
        .iter()
        .filter_map(|r| r.placement.as_ref().map(|p| &p.origin))
        .collect();
    // Not every occurrence states one: an instance with no geometric
    // transform attribute sits at its parent's origin, and the document
    // says the file stated no transformation rather than inventing an
    // identity for it. This file carries 36 transforms for 57 uses.
    assert_eq!(placed.len(), 36);
    for origin in placed {
        for v in origin {
            assert!(
                v.abs() < 1000.0,
                "a placement of {v} is metres read as millimetres: {origin:?}"
            );
        }
    }
}

/// A part used many times is one part and many occurrences, in JT as in
/// STEP. This assembly bolts the same screw in eleven places.
#[test]
fn a_jt_part_used_many_times_is_counted_once() {
    let doc = read("jt/nist_mtc_assembly.jt");
    let most = doc
        .parts
        .iter()
        .max_by_key(|p| p.occurrences)
        .expect("a part");
    assert!(
        most.occurrences > 5,
        "the most-used part is used {} times",
        most.occurrences
    );
    let total: usize = doc.parts.iter().map(|p| p.occurrences).sum();
    assert_eq!(total, doc.relations.len());
}

// --- the body-to-part join (ADR 0014, stage 1) ---

#[test]
fn every_body_goes_under_the_part_that_has_it() {
    let doc = read("synthetic/assembly_two_parts.stp");
    assert!(doc.unattached.is_empty(), "{:?}", doc.unattached);
    assert_eq!(part(&doc, "SYN-PLATE").bodies.len(), 1);
    assert_eq!(part(&doc, "SYN-BLOCK").bodies.len(), 1);
    // The assembly is parts, not geometry.
    assert!(part(&doc, "SYN-ASM-2").bodies.is_empty());
}

/// The two documents have to agree, or the join names nothing. They go
/// through one entry point so that they cannot drift apart.
#[test]
fn a_body_has_the_same_id_in_both_documents() {
    let name = "synthetic/assembly_repeated_part.stp";
    let product = read(name);
    let shapes = features::read_path(&fixture(name)).expect("the fixture reads");

    let mut from_product: Vec<String> = product
        .parts
        .iter()
        .flat_map(|p| p.bodies.iter().cloned())
        .collect();
    from_product.sort();
    let mut from_features: Vec<String> = shapes.bodies.iter().map(|b| b.id.clone()).collect();
    from_features.sort();
    assert_eq!(from_product, from_features);
}

/// A part used three times is one shape and three occurrences. The two
/// counts differ on purpose, and a consumer that confuses them reports
/// an assembly of three pins as an assembly of one.
#[test]
fn one_body_can_carry_many_occurrences() {
    let doc = read("synthetic/assembly_repeated_part.stp");
    let pin = part(&doc, "SYN-PIN");
    assert_eq!(pin.bodies.len(), 1, "one shape");
    assert_eq!(pin.occurrences, 3, "used three times");
}

#[test]
fn a_single_part_file_puts_its_body_under_its_part() {
    let doc = read("nist/nist_ctc_01_asme1_ap242-e1.stp");
    assert_eq!(doc.parts[0].bodies.len(), 1);
    assert!(doc.unattached.is_empty(), "{:?}", doc.unattached);
}

/// Nothing may go missing between the two documents: every body the
/// features document reports is either under a part or listed as
/// unattached with a reason.
#[test]
fn no_body_in_the_nist_corpus_goes_missing() {
    let dir = fixture("nist");
    for entry in std::fs::read_dir(&dir).expect("the corpus is there") {
        let path = entry.expect("an entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("stp") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let doc = product::read_path(&path).expect("the file reads");
        let shapes = features::read_path(&path).expect("the file reads");

        let placed: usize = doc.parts.iter().map(|p| p.bodies.len()).sum();
        assert_eq!(
            placed + doc.unattached.len(),
            shapes.bodies.len(),
            "{name}: {} bodies, {placed} placed, {} unattached",
            shapes.bodies.len(),
            doc.unattached.len()
        );
    }
}

/// A body that reaches no product definition is stated, not dropped.
#[test]
fn an_unattached_body_says_why() {
    for entry in std::fs::read_dir(fixture("nist")).expect("the corpus is there") {
        let path = entry.expect("an entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("stp") {
            continue;
        }
        let doc = product::read_path(&path).expect("the file reads");
        for u in &doc.unattached {
            assert!(!u.reason.is_empty(), "an unattached body must say why");
            assert!(u.body.starts_with("body:"), "{}", u.body);
        }
    }
}
