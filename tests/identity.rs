//! Identity across exports (ADR 0004): the same design element gets the
//! same id in different exports of the same design.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn nist(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/nist")
        .join(name)
}

fn semantic_ids(doc: &pmix::PmiDocument) -> BTreeSet<String> {
    let s = &doc.semantic;
    s.dimensions
        .iter()
        .map(|d| d.meta.id.clone())
        .chain(s.tolerances.iter().map(|t| t.meta.id.clone()))
        .chain(s.datums.iter().map(|d| d.meta.id.clone()))
        .chain(s.datum_systems.iter().map(|d| d.meta.id.clone()))
        .collect()
}

fn annotation_ids(doc: &pmix::PmiDocument) -> BTreeSet<String> {
    doc.presentation
        .annotations
        .iter()
        .map(|a| a.id.clone())
        .collect()
}

/// `(previous, current, minimum shared semantic ids as a fraction, minimum shared annotation ids)`.
const PAIRS: &[(&str, &str, f64, f64)] = &[
    (
        "previous/nist_ctc_04_asme1_ap242-e1.stp",
        "nist_ctc_04_asme1_ap242-e2.stp",
        1.0,
        1.0,
    ),
    (
        "previous/nist_ftc_11_asme1_ap242-e2.stp",
        "nist_ftc_11_asme1_ap242-e3.stp",
        1.0,
        1.0,
    ),
    (
        "previous/nist_ftc_08_asme1_ap242-e1-tg.stp",
        "nist_ftc_08_asme1_ap242-e4-tg.stp",
        1.0,
        1.0,
    ),
    // A genuine re-export from a newer CAD version. The two editions
    // differ in what they model, not only in how they write it: the
    // newer one splits faces, and states as a distance between two
    // features what the older stated as a size on one. So the records
    // that do not pair are mostly not paired because they are not the
    // same records. Recorded baseline 51/63 semantic and 25/58
    // annotations on 2026-09-09; of the 33 annotations that do not pair,
    // 30 are linked to semantic records, so they follow those rather
    // than failing on their own account.
    (
        "previous/nist_stc_09_asme1_ap242-e3.stp",
        "nist_stc_09_asme1_ap242-e4.stp",
        0.8,
        0.4,
    ),
];

#[test]
fn ids_are_stable_across_reexports() {
    for (prev, cur, min_semantic, min_annotations) in PAIRS {
        let a = pmix::extract(&nist(prev)).unwrap();
        let b = pmix::extract(&nist(cur)).unwrap();
        let (sa, sb) = (semantic_ids(&a), semantic_ids(&b));
        let shared = sa.intersection(&sb).count();
        let denom = sa.len().min(sb.len());
        if denom > 0 {
            let frac = shared as f64 / denom as f64;
            eprintln!("{cur}: semantic ids shared {shared}/{denom} ({frac:.2})");
            assert!(
                frac >= *min_semantic,
                "{cur}: semantic id overlap {frac:.2} < {min_semantic}"
            );
        }
        let (aa, ab) = (annotation_ids(&a), annotation_ids(&b));
        let shared = aa.intersection(&ab).count();
        let denom = aa.len().min(ab.len()).max(1);
        let frac = shared as f64 / denom as f64;
        eprintln!("{cur}: annotation ids shared {shared}/{denom} ({frac:.2})");
        assert!(
            frac >= *min_annotations,
            "{cur}: annotation id overlap {frac:.2} < {min_annotations}"
        );
    }
}

#[test]
fn readable_ids_for_datums_systems_and_views() {
    let doc = pmix::extract(&nist("nist_ctc_01_asme1_ap242-e1.stp")).unwrap();
    let mut datums: Vec<&str> = doc
        .semantic
        .datums
        .iter()
        .map(|d| d.meta.id.as_str())
        .collect();
    datums.sort();
    assert_eq!(datums, ["datum:A", "datum:B", "datum:C"]);
    let systems: BTreeSet<&str> = doc
        .semantic
        .datum_systems
        .iter()
        .map(|d| d.meta.id.as_str())
        .collect();
    assert!(
        systems.contains("dsys:A|B|C") && systems.contains("dsys:A"),
        "{systems:?}"
    );
    assert_eq!(doc.presentation.views[0].id, "view:MBD_0");
    for t in &doc.semantic.tolerances {
        assert!(t.meta.id.starts_with("tol:"), "{}", t.meta.id);
        if let Some(ds) = &t.datum_system {
            assert!(
                ds.starts_with("dsys:"),
                "datum system reference not remapped: {ds}"
            );
        }
        for f in &t.features {
            assert!(f.starts_with("feat:"), "{f}");
        }
    }
    for a in &doc.presentation.annotations {
        for s in &a.semantic {
            assert!(
                s.starts_with("tol:")
                    || s.starts_with("dim:")
                    || s.starts_with("datum:")
                    || s.starts_with("dsys:"),
                "annotation link not remapped: {s}"
            );
        }
        for v in &a.views {
            assert!(v.starts_with("view:"), "{v}");
        }
    }
}

#[test]
fn identity_ignores_value_changes() {
    // Two synthetic files that differ only in a tolerance value must give
    // the same ids for every record.
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/synthetic");
    let base = std::fs::read_to_string(dir.join("tolerance_datum_basics.stp")).unwrap();
    let changed = base.replace(
        "LENGTH_MEASURE(0.1),#1) QUALIFIED",
        "LENGTH_MEASURE(0.2),#1) QUALIFIED",
    );
    assert_ne!(base, changed);
    let tmp = std::env::temp_dir().join("pmix-identity-changed.stp");
    std::fs::write(&tmp, changed).unwrap();
    let a = pmix::extract(&dir.join("tolerance_datum_basics.stp")).unwrap();
    let b = pmix::extract(&tmp).unwrap();
    assert_eq!(semantic_ids(&a), semantic_ids(&b));
    let va = a
        .semantic
        .tolerances
        .iter()
        .find(|t| t.meta.id.starts_with("tol:") && t.value.as_ref().unwrap().value == 0.1)
        .unwrap();
    let vb = b
        .semantic
        .tolerances
        .iter()
        .find(|t| t.meta.id == va.meta.id)
        .unwrap();
    assert_eq!(
        vb.value.as_ref().unwrap().value,
        0.2,
        "same id, changed value"
    );
}
