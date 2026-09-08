//! Identity across formats (ADR 0004).
//!
//! A record whose identity is design intent rather than geometry gets the
//! same id from a STEP file and a JT file, because both readers use the
//! same recipe. A record anchored on geometry does not yet: the STEP
//! reader keys those on B-rep fingerprints and the JT reader has no
//! B-rep to fingerprint.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn step() -> pmix::PmiDocument {
    pmix::extract(&fixture("nist/nist_ctc_01_asme1_ap242-e1.stp")).unwrap()
}

fn jt() -> pmix::PmiDocument {
    pmix::extract(&fixture("jt/nist_mtc_assembly.jt")).unwrap()
}

/// Ids with their collision suffix removed. Two parts of an assembly each
/// state a datum A, and ADR 0004 leaves assemblies out of scope, so the
/// suffix is what tells those apart within one file.
fn without_suffix(id: &str) -> &str {
    match id.rsplit_once('-') {
        Some((base, n)) if n.chars().all(|c| c.is_ascii_digit()) => base,
        _ => id,
    }
}

#[test]
fn a_datum_is_named_for_its_letter_in_either_format() {
    let (s, j) = (step(), jt());
    let ids = |d: &pmix::PmiDocument| -> BTreeSet<String> {
        d.semantic
            .datums
            .iter()
            .map(|x| without_suffix(&x.meta.id).to_owned())
            .collect()
    };
    let from_step = ids(&s);
    let from_jt = ids(&j);
    assert_eq!(
        from_step,
        BTreeSet::from_iter(["datum:A".to_owned(), "datum:B".into(), "datum:C".into()])
    );
    assert_eq!(
        from_jt, from_step,
        "the two formats name datums differently"
    );
}

#[test]
fn a_datum_reference_frame_is_named_for_its_compartments_in_either_format() {
    let (s, j) = (step(), jt());
    let ids = |d: &pmix::PmiDocument| -> BTreeSet<String> {
        d.semantic
            .datum_systems
            .iter()
            .map(|x| without_suffix(&x.meta.id).to_owned())
            .collect()
    };
    // Both parts are toleranced to A and to A|B|C.
    let from_step = ids(&s);
    let from_jt = ids(&j);
    assert!(from_step.contains("dsys:A|B|C"), "{from_step:?}");
    assert!(from_jt.contains("dsys:A|B|C"), "{from_jt:?}");
    assert!(!from_step.is_disjoint(&from_jt));
    // And the frame really does name the datum records, in both.
    for d in [&s, &j] {
        let system = d
            .semantic
            .datum_systems
            .iter()
            .find(|x| x.text == "A|B|C")
            .expect("A|B|C");
        let referenced: Vec<&str> = system
            .compartments
            .iter()
            .flat_map(|c| c.datums.iter().map(|r| without_suffix(&r.datum)))
            .collect();
        assert_eq!(referenced, ["datum:A", "datum:B", "datum:C"]);
    }
}

#[test]
fn a_saved_view_is_named_for_its_name_in_either_format() {
    let j = jt();
    // The JT file carries the standard orientations; each is named for
    // itself, so a STEP file with a view of the same name would agree.
    let names: BTreeSet<&str> = j
        .presentation
        .views
        .iter()
        .map(|v| without_suffix(&v.id))
        .collect();
    assert!(names.contains("view:Top"), "{names:?}");
    assert!(names.contains("view:Front"), "{names:?}");
    // A name that is not plain enough to read as an id is hashed rather
    // than mangled, and that rule is shared too.
    let mbd = j
        .presentation
        .views
        .iter()
        .find(|v| v.name == "MBD-Trimetric #1")
        .expect("MBD-Trimetric #1");
    assert!(mbd.id.starts_with("view:") && !mbd.id.contains(' '));
}

#[test]
fn both_formats_use_one_vocabulary_of_id_prefixes() {
    let (s, j) = (step(), jt());
    let prefixes = |d: &pmix::PmiDocument| -> BTreeSet<String> {
        d.semantic
            .dimensions
            .iter()
            .map(|x| x.meta.id.clone())
            .chain(d.semantic.tolerances.iter().map(|x| x.meta.id.clone()))
            .chain(d.semantic.datums.iter().map(|x| x.meta.id.clone()))
            .chain(d.semantic.datum_systems.iter().map(|x| x.meta.id.clone()))
            .chain(d.presentation.annotations.iter().map(|x| x.id.clone()))
            .chain(d.presentation.views.iter().map(|x| x.id.clone()))
            .filter_map(|id| id.split(':').next().map(str::to_owned))
            .collect()
    };
    let from_jt = prefixes(&j);
    let from_step = prefixes(&s);
    let shared =
        BTreeSet::from_iter(["ann", "datum", "dim", "dsys", "tol", "view"].map(str::to_owned));
    assert_eq!(from_jt, shared);
    assert!(from_step.is_subset(&shared), "{from_step:?}");
}

/// The part of cross-format identity that is not solved. Recorded as a
/// test so that it is noticed the day it starts passing.
#[test]
fn a_dimension_is_not_yet_named_the_same_way_in_both_formats() {
    let (s, j) = (step(), jt());
    let keys = |d: &pmix::PmiDocument| -> BTreeSet<String> {
        d.semantic
            .dimensions
            .iter()
            .map(|x| x.meta.id.clone())
            .collect()
    };
    // Different parts, so no id could legitimately match; the point of
    // the assertion is the prefix vocabulary above, and this records
    // that nothing pretends to match on geometry it does not have.
    assert!(keys(&s).is_disjoint(&keys(&j)));
    assert!(
        !s.semantic.features.is_empty(),
        "the STEP reader anchors dimensions on features"
    );
    assert!(
        j.semantic.features.is_empty(),
        "the JT reader states no features, which is what blocks the match"
    );
}
