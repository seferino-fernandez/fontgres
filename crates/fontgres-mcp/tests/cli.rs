//! MCP server CLI smoke tests via `assert_cmd`.

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;

#[test]
fn help_lists_verbosity_flags() {
    Command::cargo_bin("fontgres-mcp")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("--verbose").and(contains("--quiet")));
}

#[test]
fn completions_generates_static_script() {
    Command::cargo_bin("fontgres-mcp")
        .unwrap()
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(contains("fontgres-mcp"));
}

#[test]
fn completions_generates_nushell_script() {
    Command::cargo_bin("fontgres-mcp")
        .unwrap()
        .args(["completions", "nushell"])
        .assert()
        .success()
        .stdout(contains("export extern fontgres-mcp"));
}

#[test]
fn man_generates_pages() {
    let dir = std::env::temp_dir().join(format!("fontgres-mcp-man-test-{}", std::process::id()));
    Command::cargo_bin("fontgres-mcp")
        .unwrap()
        .args(["man", dir.to_str().unwrap()])
        .assert()
        .success();
    assert!(dir.join("fontgres-mcp.1").exists());
}

#[test]
fn dynamic_completion_registration() {
    // With COMPLETE set and no completion args, the binary prints its registration script.
    Command::cargo_bin("fontgres-mcp")
        .unwrap()
        .env("COMPLETE", "bash")
        .assert()
        .success()
        .stdout(contains("COMPLETE"));
}

#[test]
fn missing_explicit_config_file_is_an_error() {
    let missing = std::env::temp_dir().join(format!(
        "fontgres-mcp-missing-config-{}/config.toml",
        std::process::id()
    ));
    Command::cargo_bin("fontgres-mcp")
        .unwrap()
        .env_clear()
        .arg("--config")
        .arg(&missing)
        .write_stdin("")
        .assert()
        .failure()
        .stderr(contains(format!(
            "config file {} does not exist",
            missing.display()
        )));
}

#[test]
fn missing_env_config_file_is_an_error_naming_the_variable() {
    Command::cargo_bin("fontgres-mcp")
        .unwrap()
        .env_clear()
        .env("FONTGRES_CONFIG", "/nonexistent/fontgres/config.toml")
        .write_stdin("")
        .assert()
        .failure()
        .stderr(contains(
            "config file /nonexistent/fontgres/config.toml set by FONTGRES_CONFIG does not exist",
        ));
}

/// The server reads its own table strictly, where `fontgres` accepts it
/// unread.
#[test]
fn an_unknown_mcp_key_keeps_the_server_from_starting() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    std::fs::write(&config, "[fonts]\nsystem = false\n[mcp]\nbogus = 1\n").unwrap();
    Command::cargo_bin("fontgres-mcp")
        .unwrap()
        .env_clear()
        .arg("--config")
        .arg(&config)
        .write_stdin("")
        .assert()
        .failure()
        .stderr(contains("line 4, column 1: unknown field `bogus`"));
}

/// The server does not start on a font file that holds no font: it would
/// serve without a font its configuration names.
#[test]
fn a_named_font_file_that_is_not_a_font_keeps_the_server_from_starting() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    std::fs::write(&config, "").unwrap();
    let not_a_font = dir.path().join("not-a-font.ttf");
    std::fs::write(&not_a_font, "plain text").unwrap();
    Command::cargo_bin("fontgres-mcp")
        .unwrap()
        .env_clear()
        .arg("--config")
        .arg(&config)
        .arg("--no-system-fonts")
        .arg("--font-file")
        .arg(&not_a_font)
        .write_stdin("")
        .assert()
        .failure()
        .stderr(contains(format!(
            "no font faces in {}: malformed font",
            not_a_font.display()
        )));
}
