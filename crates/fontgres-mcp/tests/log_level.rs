//! MCP server default log level tests via `assert_cmd`.

#![expect(clippy::unwrap_used, reason = "unwrap is fine in test helpers")]

use assert_cmd::Command;
use predicates::Predicate;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;

const STARTUP_LINE: &str = "fontgres-mcp starting";

/// Runs the server against a config file holding `toml` and returns its
/// stderr. The startup line is logged before the server reads a request, and
/// the server exits when it reads EOF on its empty stdin, so the run cannot
/// hang. The exit status is not asserted: only the startup log matters.
fn startup_stderr(toml: &str, configure: impl FnOnce(&mut Command)) -> String {
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("config.toml");
    std::fs::write(&config, format!("[fonts]\nsystem = false\n{toml}")).unwrap();
    let mut command = Command::cargo_bin("fontgres-mcp").unwrap();
    command
        .env_clear()
        .env("NO_COLOR", "1")
        .arg("--config")
        .arg(&config)
        .write_stdin("");
    configure(&mut command);
    let output = command.output().unwrap();
    String::from_utf8(output.stderr).unwrap()
}

#[test]
fn unset_log_level_defaults_to_info() {
    let stderr = startup_stderr("", |_| {});
    assert!(contains(STARTUP_LINE).eval(&stderr), "stderr: {stderr}");
}

#[test]
fn quiet_flag_silences_startup_line() {
    let stderr = startup_stderr("", |command| {
        command.arg("-q");
    });
    assert!(
        contains(STARTUP_LINE).not().eval(&stderr),
        "stderr: {stderr}"
    );
}

#[test]
fn log_level_env_warn_hides_startup_line() {
    let stderr = startup_stderr("", |command| {
        command.env("FONTGRES_LOG_LEVEL", "warn");
    });
    assert!(
        contains(STARTUP_LINE).not().eval(&stderr),
        "stderr: {stderr}"
    );
}

#[test]
fn toml_log_level_warn_hides_startup_line() {
    let stderr = startup_stderr("[log]\nlevel = \"warn\"\n", |_| {});
    assert!(
        contains(STARTUP_LINE).not().eval(&stderr),
        "stderr: {stderr}"
    );
}

#[test]
fn verbosity_flag_beats_configured_level() {
    // `-vv` is info, which shows the startup line the configured `warn` hides.
    let stderr = startup_stderr("[log]\nlevel = \"warn\"\n", |command| {
        command.arg("-vv");
    });
    assert!(contains(STARTUP_LINE).eval(&stderr), "stderr: {stderr}");
}

#[test]
fn an_invalid_log_level_keeps_the_server_from_starting() {
    let stderr = startup_stderr("", |command| {
        command.env("FONTGRES_LOG_LEVEL", "loud");
    });
    assert!(
        contains(
            "invalid FONTGRES_LOG_LEVEL value \"loud\": expected trace, debug, info, warn, \
             error or off"
        )
        .eval(&stderr),
        "stderr: {stderr}"
    );
    assert!(
        contains(STARTUP_LINE).not().eval(&stderr),
        "stderr: {stderr}"
    );
}
