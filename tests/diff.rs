//! `pmix diff` (ADR 0005): matching by id, field-level changes, exclusions.

use std::path::{Path, PathBuf};

use pmix::diff::{ChangeKind, diff};

fn synthetic(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/synthetic")
        .join(name)
}

fn nist(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/nist")
        .join(name)
}

fn edited(name: &str, from: &str, to: &str) -> PathBuf {
    let text = std::fs::read_to_string(synthetic(name)).unwrap();
    let changed = text.replace(from, to);
    assert_ne!(text, changed, "edit did not apply");
    // Tests run in parallel: one file per distinct edit.
    let tag = pmix::model::content_hash([from, to]);
    let tmp = std::env::temp_dir().join(format!("pmix-diff-{tag}-{name}"));
    std::fs::write(&tmp, changed).unwrap();
    tmp
}

#[test]
fn identical_documents_have_no_changes() {
    let a = pmix::extract(&synthetic("tolerance_datum_basics.stp")).unwrap();
    let report = diff(&a, &a);
    assert!(report.is_empty(), "{}", report.render_text());
    assert_eq!(
        report.summary.changed + report.summary.added + report.summary.removed,
        0
    );
    assert!(report.summary.unchanged > 5);
}

#[test]
fn a_changed_value_is_one_changed_record() {
    let a = pmix::extract(&synthetic("tolerance_datum_basics.stp")).unwrap();
    let b = pmix::extract(&edited(
        "tolerance_datum_basics.stp",
        "LENGTH_MEASURE(0.1),#1) QUALIFIED",
        "LENGTH_MEASURE(0.2),#1) QUALIFIED",
    ))
    .unwrap();
    let report = diff(&a, &b);
    let changed: Vec<_> = report
        .changes
        .iter()
        .filter(|c| c.kind == ChangeKind::Changed)
        .collect();
    assert_eq!(changed.len(), 1, "{}", report.render_text());
    assert_eq!(changed[0].collection, "tolerances");
    let paths: Vec<&str> = changed[0].fields.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["text", "value.value"], "{}", report.render_text());
    assert_eq!(report.summary.added + report.summary.removed, 0);
    let text = report.render_text();
    assert!(text.contains("value.value: 0.1 → 0.2"), "{text}");
    assert!(text.contains("summary:"), "{text}");
}

#[test]
fn a_removed_datum_reference_is_a_change_not_a_replacement() {
    // Drop datum C from the A|B(M)|C frame: the tolerance keeps its id and
    // reports its datum_system field; the old frame goes, a new one comes.
    let a = pmix::extract(&synthetic("tolerance_datum_basics.stp")).unwrap();
    let b = pmix::extract(&edited(
        "tolerance_datum_basics.stp",
        "#73=DATUM_SYSTEM('DRF 1','',#16,.F.,(#70,#71,#72));",
        "#73=DATUM_SYSTEM('DRF 1','',#16,.F.,(#70,#71));",
    ))
    .unwrap();
    let report = diff(&a, &b);
    let tol_changes: Vec<_> = report
        .changes
        .iter()
        .filter(|c| c.collection == "tolerances")
        .collect();
    assert!(
        tol_changes.iter().all(|c| c.kind == ChangeKind::Changed),
        "{}",
        report.render_text()
    );
    assert!(
        tol_changes
            .iter()
            .any(|c| c.fields.iter().any(|f| f.path == "datum_system"))
    );
    assert!(
        report
            .changes
            .iter()
            .any(|c| c.collection == "datum_systems"
                && c.kind == ChangeKind::Removed
                && c.id == "dsys:A|B|C")
    );
    assert!(
        report
            .changes
            .iter()
            .any(|c| c.collection == "datum_systems"
                && c.kind == ChangeKind::Added
                && c.id == "dsys:A|B")
    );
}

#[test]
fn json_round_trip_and_reexport_pair() {
    // A JSON document written by extract loads back and diffs cleanly.
    let doc = pmix::extract(&nist("nist_ctc_01_asme1_ap242-e1.stp")).unwrap();
    let tmp = std::env::temp_dir().join("pmix-diff-ctc01.json");
    std::fs::write(&tmp, serde_json::to_string(&doc).unwrap()).unwrap();
    let loaded = pmix::load(&tmp).unwrap();
    assert!(diff(&doc, &loaded).is_empty());

    // The genuine re-export: the report is consistent with the identity
    // baseline and never panics.
    let a = pmix::extract(&nist("previous/nist_stc_09_asme1_ap242-e3.stp")).unwrap();
    let b = pmix::extract(&nist("nist_stc_09_asme1_ap242-e4.stp")).unwrap();
    let report = diff(&a, &b);
    assert!(report.summary.unchanged > 20, "{:?}", report.summary);
    assert!(report.summary.added > 0 && report.summary.removed > 0);
    let json = serde_json::to_string(&report).unwrap();
    let back: pmix::diff::DiffReport = serde_json::from_str(&json).unwrap();
    assert_eq!(back, report);
}
