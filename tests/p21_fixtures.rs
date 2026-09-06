//! Parse every NIST AP242 fixture and check the parser's view of it.
//!
//! Expected values were recorded from a manual review of the first parse
//! and act as a regression net: any change in instance counts or
//! diagnostics must be deliberate.

use std::path::{Path, PathBuf};

use pmix::step::p21::{self, Severity};

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/nist")
}

fn fixtures() -> Vec<PathBuf> {
    let mut v: Vec<_> = std::fs::read_dir(fixture_dir())
        .expect("fixture directory exists")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "stp"))
        .collect();
    v.sort();
    v
}

/// (file name, instances, complex instances) as recorded on first parse.
const EXPECTED: &[(&str, usize, usize)] = &[
    ("nist_ctc_01_asme1_ap242-e1.stp", 4350, 53),
    ("nist_ctc_02_asme1_ap242-e2.stp", 22304, 105),
    ("nist_ctc_03_asme1_ap242-e2.stp", 5920, 71),
    ("nist_ctc_04_asme1_ap242-e2.stp", 20456, 28),
    ("nist_ctc_05_asme1_ap242-e1.stp", 13394, 21),
    ("nist_ftc_06_asme1_ap242-e2.stp", 10034, 165),
    ("nist_ftc_07_asme1_ap242-e2.stp", 17010, 72),
    ("nist_ftc_08_asme1_ap242-e2.stp", 10072, 142),
    ("nist_ftc_08_asme1_ap242-e4-tg.stp", 12632, 9),
    ("nist_ftc_09_asme1_ap242-e1.stp", 11662, 54),
    ("nist_ftc_10_asme1_ap242-e2.stp", 21230, 90),
    ("nist_ftc_11_asme1_ap242-e3.stp", 2032, 43),
    ("nist_stc_06_asme1_ap242-e3.stp", 11780, 57),
    ("nist_stc_07_asme1_ap242-e3.stp", 17470, 546),
    ("nist_stc_08_asme1_ap242-e3.stp", 11548, 88),
    ("nist_stc_09_asme1_ap242-e4.stp", 10609, 61),
    ("nist_stc_10_asme1_ap242-e2.stp", 12073, 298),
];

#[test]
fn corpus_is_present() {
    let names: Vec<String> = fixtures()
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    let expected: Vec<&str> = EXPECTED.iter().map(|e| e.0).collect();
    assert_eq!(names, expected, "fixture set changed; update EXPECTED");
}

#[test]
fn instance_counts_match_recorded_values() {
    for (name, instances, complex) in EXPECTED {
        let bytes = std::fs::read(fixture_dir().join(name)).unwrap();
        let ex = p21::parse_bytes(&bytes).unwrap();
        assert_eq!(ex.len(), *instances, "{name}: instance count");
        assert_eq!(
            ex.instances().filter(|i| i.is_complex()).count(),
            *complex,
            "{name}: complex instance count"
        );
        assert!(ex.diagnostics.is_empty(), "{name}: {:#?}", ex.diagnostics);
    }
}

#[test]
fn every_fixture_parses_with_ap242_schema() {
    for path in fixtures() {
        let bytes = std::fs::read(&path).unwrap();
        let ex = p21::parse_bytes(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let name = path.file_name().unwrap().to_string_lossy();
        assert_eq!(
            ex.header.schema_name().as_deref(),
            Some("AP242_MANAGED_MODEL_BASED_3D_ENGINEERING_MIM_LF"),
            "{name}"
        );
        assert!(ex.len() > 1000, "{name}: only {} instances", ex.len());
        let errors: Vec<_> = ex
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "{name}: {errors:#?}");
    }
}

/// Prints a per-file summary; run with `--nocapture` to see it.
#[test]
fn corpus_summary() {
    for path in fixtures() {
        let bytes = std::fs::read(&path).unwrap();
        let ex = p21::parse_bytes(&bytes).unwrap();
        let name = path.file_name().unwrap().to_string_lossy();
        let complex = ex.instances().filter(|i| i.is_complex()).count();
        println!(
            "{name}: {} instances, {} complex, {} types, {} complex kinds, {} diagnostics",
            ex.len(),
            complex,
            ex.type_counts().len(),
            ex.complex_type_counts().len(),
            ex.diagnostics.len()
        );
        for d in &ex.diagnostics {
            println!("    {d}");
        }
    }
}
