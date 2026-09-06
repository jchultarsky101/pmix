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
