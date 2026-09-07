use assert_cmd::Command;
use predicates::prelude::*;

fn pmix() -> Command {
    Command::cargo_bin("pmix").expect("binary builds")
}

#[test]
fn prints_help() {
    pmix()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("extract"));
}

#[test]
fn rejects_unknown_extension() {
    let dir = std::env::temp_dir();
    let file = dir.join("pmix-test-unknown.obj");
    std::fs::write(&file, b"").unwrap();

    pmix()
        .args(["extract", file.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unrecognised file format"));
}

#[test]
fn reports_missing_input() {
    pmix()
        .args(["extract", "does-not-exist.stp"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("does-not-exist.stp"));
}

#[test]
fn verbose_flag_logs_to_stderr() {
    pmix()
        .args(["-v", "extract", "does-not-exist.stp"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("extracting PMI"));
}

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/nist/nist_ftc_11_asme1_ap242-e3.stp"
);

#[test]
fn inspect_prints_summary() {
    pmix()
        .args(["inspect", FIXTURE])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "AP242_MANAGED_MODEL_BASED_3D_ENGINEERING_MIM_LF",
        ))
        .stdout(predicate::str::contains("2032 instances"))
        .stdout(predicate::str::contains("GEOMETRIC_TOLERANCE"));
}

#[test]
fn inspect_lists_instances_by_type_and_dumps_entities() {
    let out = pmix()
        .args(["inspect", FIXTURE, "--type", "datum"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    let first = text.lines().next().expect("at least one DATUM instance");
    assert!(
        first.starts_with('#') && first.contains("DATUM("),
        "{first}"
    );
    let id = first.trim_start_matches('#').split('=').next().unwrap();

    pmix()
        .args(["inspect", FIXTURE, "--entity", id, "--depth", "1", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"keyword\": \"DATUM\""))
        .stdout(predicate::str::contains("\"segments\""));
}

#[test]
fn inspect_rejects_non_step_input() {
    let dir = std::env::temp_dir();
    let file = dir.join("pmix-test-not-step.stp");
    std::fs::write(&file, b"this is not a step file").unwrap();
    pmix()
        .args(["inspect", file.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not a STEP Part 21 file"));
}

#[test]
fn extract_presentation_geometry_flag() {
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/synthetic/presentation_basics.stp"
    );
    pmix()
        .args(["extract", fixture])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"polylines\": 1"))
        .stdout(predicate::str::contains("\"vertices\"").not());
    pmix()
        .args(["extract", fixture, "--presentation-geometry"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"vertices\""));
}

#[test]
fn diff_exit_codes_and_output() {
    let base = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/synthetic/tolerance_datum_basics.stp"
    );
    // Identical: exit 0.
    pmix()
        .args(["diff", base, base])
        .assert()
        .code(0)
        .stdout(predicate::str::contains("summary:"));
    // Different: exit 1, the changed field is named.
    let text = std::fs::read_to_string(base).unwrap();
    let changed = text.replace(
        "LENGTH_MEASURE(0.1),#1) QUALIFIED",
        "LENGTH_MEASURE(0.2),#1) QUALIFIED",
    );
    let tmp = std::env::temp_dir().join("pmix-cli-diff-changed.stp");
    std::fs::write(&tmp, changed).unwrap();
    pmix()
        .args(["diff", base, tmp.to_str().unwrap()])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("value.value: 0.1 → 0.2"));
    pmix()
        .args(["diff", base, tmp.to_str().unwrap(), "--json"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("\"kind\": \"changed\""));
    // Error: exit 2.
    pmix()
        .args(["diff", base, "does-not-exist.stp"])
        .assert()
        .code(2);
}

#[test]
fn inspect_reads_a_jt_file() {
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/jt/nist_mtc_assembly.jt"
    );
    pmix()
        .args(["inspect", fixture])
        .assert()
        .success()
        .stdout(predicate::str::contains("JT 10.5"))
        .stdout(predicate::str::contains("107 segments"))
        .stdout(predicate::str::contains("PMI data"))
        .stdout(predicate::str::contains(
            "ce357249-38fb-11d1-a506-006097bdc6e1",
        ));

    // STEP-only options are refused rather than silently ignored.
    pmix()
        .args(["inspect", fixture, "--entity", "12"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("STEP files only"));

    // Extraction is not supported for JT yet, and says so.
    pmix()
        .args(["extract", fixture])
        .assert()
        .failure()
        .stderr(predicate::str::contains("JT"));
}
