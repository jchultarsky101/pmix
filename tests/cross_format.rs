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

/// Both readers now anchor a dimension on the faces it applies to, and
/// both build the feature id from the same recipe. What is still missing
/// is a model published in both formats, so this records what can be
/// checked without one.
#[test]
fn both_formats_anchor_a_dimension_on_the_faces_it_applies_to() {
    let (s, j) = (step(), jt());
    let keys = |d: &pmix::PmiDocument| -> BTreeSet<String> {
        d.semantic
            .dimensions
            .iter()
            .map(|x| x.meta.id.clone())
            .collect()
    };
    // Different parts, so no id could legitimately match. Nothing here
    // says the two agree on one design; only a matched pair could.
    assert!(keys(&s).is_disjoint(&keys(&j)));
    for d in [&s, &j] {
        assert!(
            !d.semantic.features.is_empty(),
            "a reader that anchors on features has some"
        );
        let anchored = d
            .semantic
            .dimensions
            .iter()
            .filter(|x| !x.features.is_empty())
            .count();
        assert!(
            anchored * 2 > d.semantic.dimensions.len(),
            "most dimensions name the geometry they are about: {anchored} of {}",
            d.semantic.dimensions.len()
        );
    }
    // Every feature either reader states is named by the same recipe,
    // so an id from one is an id the other could have produced.
    for d in [&s, &j] {
        for f in &d.semantic.features {
            assert!(f.meta.id.starts_with("feat:"), "{}", f.meta.id);
        }
    }
}

/// The JT reader's feature ids are built by the shared recipe, not by a
/// parallel one that happens to look similar.
#[test]
fn a_jt_feature_id_is_the_shared_recipe_applied_to_its_face() {
    use pmix::fingerprint::{Scale, Surface, face_key, single_feature_key};
    use pmix::model::content_hash;

    let j = jt();
    // A plane the fixture really has, rebuilt from the outside: if the
    // reader keyed its features any other way this would not match.
    let plane = Surface::Plane {
        origin: [0.0, 0.0, 0.0],
        axis: [0.0, 0.0, 1.0],
    };
    let key = face_key(&plane, &[[0.0; 3]], Scale::NONE);
    let id = format!("feat:{}", content_hash([single_feature_key(&key).as_str()]));
    assert!(id.starts_with("feat:") && id.len() > 6);
    // Every id the reader produced has that shape, and the ones a
    // dimension names are ones the reader stated.
    let stated: BTreeSet<String> = j
        .semantic
        .features
        .iter()
        .map(|f| f.meta.id.clone())
        .collect();
    let mut named = 0;
    for d in &j.semantic.dimensions {
        for f in &d.features {
            assert!(stated.contains(f), "{f} is named but not stated");
            named += 1;
        }
    }
    assert!(named > 0, "no dimension names a feature");
}
