//! Reading thread designations out of a file's text (ADR 0014).
//!
//! One real designation exists in the public corpus, in a CAx-IF
//! validation property of a NIST model. It is the anchor: the grammar
//! is a standard, but this is the proof the grammar meets a real file.

use std::path::{Path, PathBuf};

use pmix::threads::{self, Standard};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn the_nist_model_states_four_m12_tapped_holes() {
    let doc = pmix::extract(&fixture("nist/nist_ctc_04_asme1_ap242-e2.stp")).expect("it reads");
    let found = threads::in_document(&doc);
    assert_eq!(found.len(), 1, "{found:?}");
    let t = &found[0];
    assert_eq!(t.designation, "M12x1.75-6H");
    assert_eq!(t.standard, Standard::Metric);
    assert_eq!(t.major_diameter, Some(12.0));
    assert_eq!(t.pitch, Some(1.75));
    assert_eq!(t.class.as_deref(), Some("6H"), "6H is an internal thread");
    assert_eq!(t.count, Some(4), "the callout says 4X");
    assert_eq!(t.found_in.kind, "property");
    assert!(
        t.text.contains("M12x1.75-6H"),
        "the whole text is carried: {}",
        t.text
    );
}

/// The rest of the corpus states no designation, and a reader that
/// found one there would be inventing it. Every other NIST file must
/// come back empty.
#[test]
fn no_other_nist_model_is_read_as_threaded() {
    let dir = fixture("nist");
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).expect("the corpus is there") {
        let path = entry.expect("an entry").path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if path.extension().and_then(|e| e.to_str()) != Some("stp") || name.contains("ctc_04") {
            continue;
        }
        let doc = pmix::extract(&path).expect("the file reads");
        let found = threads::in_document(&doc);
        assert!(found.is_empty(), "{name} should state no thread: {found:?}");
        checked += 1;
    }
    assert!(checked >= 15, "only checked {checked} files");
}

/// Two readings of one file agree, as every id in this project must
/// (ADR 0004).
#[test]
fn the_reading_is_repeatable() {
    let path = fixture("nist/nist_ctc_04_asme1_ap242-e2.stp");
    let a = threads::in_document(&pmix::extract(&path).unwrap());
    let b = threads::in_document(&pmix::extract(&path).unwrap());
    assert_eq!(a, b);
}
