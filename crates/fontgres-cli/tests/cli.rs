//! CLI smoke tests via `assert_cmd`.

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;

#[test]
fn help_lists_subcommands() {
    Command::cargo_bin("fontgres")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("completions").and(contains("man")));
}

#[test]
fn completions_generates_static_script() {
    Command::cargo_bin("fontgres")
        .unwrap()
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(contains("fontgres"));
}

#[test]
fn dynamic_completion_registration() {
    // With COMPLETE set and no completion args, the binary prints its registration script.
    Command::cargo_bin("fontgres")
        .unwrap()
        .env("COMPLETE", "bash")
        .assert()
        .success()
        .stdout(contains("COMPLETE"));
}

#[test]
fn completions_generates_nushell_script() {
    Command::cargo_bin("fontgres")
        .unwrap()
        .args(["completions", "nushell"])
        .assert()
        .success()
        .stdout(contains("export extern fontgres"));
}

#[test]
fn man_generates_pages() {
    let dir = std::env::temp_dir().join(format!("fontgres-man-test-{}", std::process::id()));
    Command::cargo_bin("fontgres")
        .unwrap()
        .args(["man", dir.to_str().unwrap()])
        .assert()
        .success();
    assert!(dir.join("fontgres.1").exists());
}

#[test]
fn help_lists_verbosity_flags() {
    Command::cargo_bin("fontgres")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("--verbose").and(contains("--quiet")));
}

#[test]
fn verbosity_flag_parses() {
    // `-v` parses on the top-level command; completions short-circuit before any client.
    Command::cargo_bin("fontgres")
        .unwrap()
        .args(["-v", "completions", "bash"])
        .assert()
        .success();
}
