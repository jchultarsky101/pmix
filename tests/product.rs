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

// --- how big the whole model is (ADR 0014) ---

/// A body states its shape in its own coordinates; only the placements
/// say where those coordinates sit. The pins stand 10 tall on a 6-thick
/// plate, so the assembly is 16 tall even though no body is.
#[test]
fn the_assembly_is_as_big_as_where_its_parts_are_put() {
    let doc = read("synthetic/assembly_repeated_part.stp");
    let envelope = doc.envelope.as_ref().expect("an envelope");
    assert_eq!(envelope.size, [60.0, 24.0, 16.0]);
    assert!(!envelope.approximate);
}

#[test]
fn a_second_part_placed_above_the_first_raises_the_assembly() {
    let doc = read("synthetic/assembly_two_parts.stp");
    let envelope = doc.envelope.as_ref().expect("an envelope");
    // A 40 x 24 x 6 plate with an 18-tall block standing on it.
    assert_eq!(envelope.size, [40.0, 24.0, 24.0]);
}

/// JT states the box itself, on the partition node. Reading it beats
/// computing one, and it is there even in a file exported without
/// precise geometry, where there is no body to measure.
#[test]
fn a_jt_file_states_its_own_envelope() {
    let doc = read("jt/nist_mtc_assembly.jt");
    let envelope = doc.envelope.as_ref().expect("an envelope");
    assert!(
        !envelope.approximate,
        "the file states it, so nothing is derived"
    );
    // 347.6 x 152.4 x 101.6 mm, which is 13.7 x 6 x 4 inches.
    assert!(
        (envelope.size[1] - 152.4).abs() < 0.01,
        "{:?}",
        envelope.size
    );
    assert!(
        (envelope.size[2] - 101.6).abs() < 0.01,
        "{:?}",
        envelope.size
    );
}

/// A part whose own box is a lower bound makes the assembly's one too.
#[test]
fn an_approximate_body_makes_the_assembly_approximate() {
    let doc = read("nist/nist_ctc_01_asme1_ap242-e1.stp");
    let envelope = doc.envelope.as_ref().expect("an envelope");
    assert!(envelope.approximate);
}

/// A JT part node points at its own topology segment through a
/// late-loaded property, so the body join is the mapping the file
/// already states rather than a walk up the graph.
#[test]
fn jt_bodies_go_under_the_parts_that_hold_them() {
    let doc = read("jt/nist_mtc_assembly.jt");
    let placed: usize = doc.parts.iter().map(|p| p.bodies.len()).sum();
    let shapes = features::read_path(&fixture("jt/nist_mtc_assembly.jt")).expect("it reads");
    assert_eq!(
        placed + doc.unattached.len(),
        shapes.bodies.len(),
        "no body may go missing between the two documents"
    );
    assert!(placed > 0, "some body should be placed");

    // And the ids agree, or the join names nothing.
    let from_features: std::collections::BTreeSet<&str> =
        shapes.bodies.iter().map(|b| b.id.as_str()).collect();
    for part in &doc.parts {
        for body in &part.bodies {
            assert!(
                from_features.contains(body.as_str()),
                "{body} is not a body"
            );
        }
    }
}

// --- who owns it, who approved it, how it is classified (ADR 0014) ---

/// The values reach the document, from assignments hung off three
/// different levels — the definition, the formation and the product —
/// all resolving to the one part.
#[test]
fn a_part_states_its_owner_its_approval_and_its_marking() {
    let doc = read("synthetic/part_identity.stp");
    let part = part(&doc, "SYN-BRKT-100");

    let roles: Vec<(&str, Option<&str>, Option<&str>)> = part
        .people
        .iter()
        .map(|p| {
            (
                p.role.as_str(),
                p.person.as_deref(),
                p.organisation.as_deref(),
            )
        })
        .collect();
    assert!(
        roles.contains(&(
            "design_owner",
            Some("Sample, Ada"),
            Some("Synthetic Engineering Works")
        )),
        "{roles:?}"
    );
    assert!(roles.contains(&(
        "creator",
        Some("Fixture, Bo"),
        Some("Synthetic Engineering Works")
    )));
    assert!(roles.contains(&(
        "design_supplier",
        Some("Vendor, Cy"),
        Some("Synthetic Castings Ltd")
    )));

    assert_eq!(part.approvals.len(), 1);
    let a = &part.approvals[0];
    assert_eq!(a.status, "approved");
    assert_eq!(a.level.as_deref(), Some("Released for production"));
    // CALENDAR_DATE is year, day, month: (2026,11,9) is 11 September.
    assert_eq!(a.date.as_deref(), Some("2026-09-11"));
    assert_eq!(a.by.len(), 1);
    assert_eq!(a.by[0].role, "Authorise release");
    assert_eq!(a.by[0].person.as_deref(), Some("Sample, Ada"));

    let c = part.classification.as_ref().expect("a classification");
    assert_eq!(c.level.as_deref(), Some("confidential"));
    assert_eq!(c.purpose.as_deref(), Some("commercial in confidence"));
    assert_eq!(part.categories, vec!["detail".to_string()]);
}

/// A real writer states the entities mostly empty: the classification
/// has a level and no name or purpose, the people and organisations
/// have roles and no names, the approval is `not_yet_approved` and its
/// date is zeros. What is stated is kept, what is blank is absent, and
/// a zeroed date is a blank rather than the first of January in year
/// nought.
#[test]
fn a_mostly_blank_record_keeps_exactly_what_it_states() {
    let doc = read("d2mi/827-9999-905.stp");
    assert_eq!(doc.parts.len(), 1, "{:?}", doc.parts);
    let part = &doc.parts[0];

    let c = part.classification.as_ref().expect("the entity is stated");
    assert_eq!(
        c.level.as_deref(),
        Some("confidential"),
        "the level is stated"
    );
    assert_eq!(c.name, None, "the name is blank");
    assert_eq!(c.purpose, None, "and so is the purpose");

    let mut roles: Vec<&str> = part.people.iter().map(|p| p.role.as_str()).collect();
    roles.sort();
    roles.dedup();
    assert_eq!(
        roles,
        vec![
            "classification_officer",
            "creator",
            "design_owner",
            "design_supplier"
        ]
    );
    assert!(
        part.people
            .iter()
            .all(|p| p.person.is_none() && p.organisation.is_none())
    );

    assert!(!part.approvals.is_empty());
    assert!(
        part.approvals
            .iter()
            .all(|a| a.status == "not_yet_approved")
    );
    assert!(
        part.approvals.iter().all(|a| a.date.is_none()),
        "a zeroed date is not a date: {:?}",
        part.approvals
    );
}

/// A file that states none of it says so by absence of the fields, and
/// nothing is invented. Every NIST MBE model is such a file.
#[test]
fn a_file_stating_no_identity_invents_none() {
    let doc = read("nist/nist_ctc_01_asme1_ap242-e1.stp");
    let part = &doc.parts[0];
    assert!(part.people.is_empty());
    assert!(part.approvals.is_empty());
    assert!(part.classification.is_none());
}

/// Both spellings of the assignments are one reader: the D2MI files use
/// AP203's `CC_DESIGN_*`, the synthetic one AP242's `APPLIED_*`.
#[test]
fn both_assignment_spellings_are_read() {
    let ap203 = read("d2mi/827-9999-907.stp");
    let ap242 = read("synthetic/part_identity.stp");
    assert!(!ap203.parts[0].people.is_empty(), "CC_DESIGN_* read");
    assert!(!ap242.parts[0].people.is_empty(), "APPLIED_* read");
}
