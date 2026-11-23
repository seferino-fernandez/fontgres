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
        .stdout(
            contains("faces")
                .and(contains("families"))
                .and(contains("query")),
        );
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

/// The directory holding the test font, Tuffy.ttf.
fn fonts_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../fontgres-core/tests/fonts")
}

/// `fontgres` with a clean environment, an empty config file in `dir`, and no
/// system fonts, so a test sees only the fonts it names.
#[expect(clippy::unwrap_used, reason = "unwrap is fine in test helpers")]
fn fontgres(dir: &tempfile::TempDir) -> Command {
    let config = dir.path().join("config.toml");
    if !config.exists() {
        std::fs::write(&config, "").unwrap();
    }
    let mut command = Command::cargo_bin("fontgres").unwrap();
    command
        .env_clear()
        .env("FONTGRES_CONFIG", &config)
        .arg("--no-system-fonts");
    command
}

/// The JSON a successful run of `command` prints.
#[expect(clippy::unwrap_used, reason = "unwrap is fine in test helpers")]
fn json_output(command: &mut Command) -> serde_json::Value {
    let output = command.output().unwrap();
    assert!(output.status.success(), "{output:?}");
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn faces_lists_the_fonts_in_a_font_dir() {
    let dir = tempfile::tempdir().unwrap();
    let faces = json_output(
        fontgres(&dir)
            .arg("--font-dir")
            .arg(fonts_dir())
            .args(["--output", "json", "faces"]),
    );
    assert_eq!(faces.as_array().map(Vec::len), Some(1), "{faces}");
    assert_eq!(faces[0]["family"], "Tuffy");
    assert_eq!(faces[0]["weight"], 500);
    assert_eq!(faces[0]["style"], "normal");
}

#[test]
fn faces_family_filter_drops_other_families() {
    let dir = tempfile::tempdir().unwrap();
    let faces = json_output(
        fontgres(&dir)
            .arg("--font-dir")
            .arg(fonts_dir())
            .args(["--output", "json", "faces", "--family", "Other"]),
    );
    assert_eq!(faces, serde_json::json!([]));
}

#[test]
fn families_lists_each_family_once() {
    let dir = tempfile::tempdir().unwrap();
    let tuffy = fonts_dir().join("Tuffy.ttf");
    let families = json_output(
        fontgres(&dir)
            .arg("--font-file")
            .arg(&tuffy)
            .arg("--font-file")
            .arg(&tuffy)
            .args(["--output", "json", "families"]),
    );
    assert_eq!(families, serde_json::json!(["Tuffy"]));
}

#[test]
fn table_and_csv_output_name_the_face() {
    let dir = tempfile::tempdir().unwrap();
    for format in ["table", "csv"] {
        fontgres(&dir)
            .arg("--font-dir")
            .arg(fonts_dir())
            .args(["--output", format, "faces"])
            .assert()
            .success()
            .stdout(contains("Tuffy").and(contains("PostScript name")));
    }
}

#[test]
fn query_prints_the_best_match_from_the_listed_families() {
    let dir = tempfile::tempdir().unwrap();
    let face = json_output(fontgres(&dir).arg("--font-dir").arg(fonts_dir()).args([
        "--output", "json", "query", "Missing", "Tuffy", "--weight", "700", "--style", "italic",
    ]));
    assert_eq!(face["family"], "Tuffy");
}

#[test]
fn query_without_a_match_fails_naming_the_families() {
    let dir = tempfile::tempdir().unwrap();
    fontgres(&dir)
        .arg("--font-dir")
        .arg(fonts_dir())
        .args(["query", "Missing", "Absent"])
        .assert()
        .failure()
        .stdout("")
        .stderr(contains("fontgres: no face matches Missing, Absent"));
}

#[test]
fn query_resolves_a_configured_generic_family() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("config.toml"),
        "[fonts.generic]\nmonospace = \"Tuffy\"\n",
    )
    .unwrap();
    let face = json_output(fontgres(&dir).arg("--font-dir").arg(fonts_dir()).args([
        "--output",
        "json",
        "query",
        "monospace",
    ]));
    assert_eq!(face["family"], "Tuffy");
}

#[test]
fn query_rejects_an_out_of_range_weight() {
    let dir = tempfile::tempdir().unwrap();
    fontgres(&dir)
        .args(["query", "Tuffy", "--weight", "0"])
        .assert()
        .failure()
        .stderr(contains("--weight"));
}

#[test]
fn config_font_dirs_apply_without_flags() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("config.toml"),
        format!(
            "[fonts]\ndirs = [{:?}]\n",
            fonts_dir().display().to_string()
        ),
    )
    .unwrap();
    let families = json_output(fontgres(&dir).args(["--output", "json", "families"]));
    assert_eq!(families, serde_json::json!(["Tuffy"]));
}

#[test]
fn missing_font_dir_and_file_fail_naming_the_path() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing");
    fontgres(&dir)
        .arg("--font-dir")
        .arg(&missing)
        .arg("families")
        .assert()
        .failure()
        .stderr(contains(format!(
            "font directory {} does not exist",
            missing.display()
        )));
    fontgres(&dir)
        .arg("--font-file")
        .arg(missing.join("x.ttf"))
        .arg("families")
        .assert()
        .failure()
        .stderr(contains("cannot read font file").and(contains("x.ttf")));
}

#[test]
fn missing_explicit_config_file_errors_naming_the_path() {
    Command::cargo_bin("fontgres")
        .unwrap()
        .env_clear()
        .args(["--config", "/nonexistent/fontgres/config.toml", "families"])
        .assert()
        .failure()
        .stderr(contains(
            "config file /nonexistent/fontgres/config.toml does not exist",
        ));
}

#[test]
fn missing_env_config_file_errors_naming_the_variable() {
    Command::cargo_bin("fontgres")
        .unwrap()
        .env_clear()
        .env("FONTGRES_CONFIG", "/nonexistent/fontgres/config.toml")
        .args(["families"])
        .assert()
        .failure()
        .stderr(contains(
            "config file /nonexistent/fontgres/config.toml set by FONTGRES_CONFIG does not exist",
        ));
}

#[test]
fn invalid_env_value_errors_naming_the_variable() {
    let dir = tempfile::tempdir().unwrap();
    fontgres(&dir)
        .env("FONTGRES_FONTS_SYSTEM", "maybe")
        .args(["families"])
        .assert()
        .failure()
        .stderr(contains(r#"invalid FONTGRES_FONTS_SYSTEM value "maybe""#));
}

#[test]
fn toml_error_reports_its_position() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("config.toml"), "[fonts]\nsystem = yes\n").unwrap();
    fontgres(&dir)
        .args(["families"])
        .assert()
        .failure()
        .stderr(contains("line 2, column"));
}

#[test]
fn invalid_log_level_warns_and_falls_back_to_errors_only() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("config.toml"), "[log]\nlevel = \"loud\"\n").unwrap();
    fontgres(&dir).args(["families"]).assert().success().stderr(
        contains("fontgres: warning: ignoring invalid log level \"loud\"")
            .and(contains("using error")),
    );
}

/// fontdb logs through the `log` crate; tracing-subscriber's default
/// `tracing-log` feature forwards those records, so they obey the CLI's level.
#[test]
fn fontdb_log_warnings_reach_stderr_at_warn_level() {
    let dir = tempfile::tempdir().unwrap();
    let not_a_font = dir.path().join("not-a-font.ttf");
    std::fs::write(&not_a_font, "plain text").unwrap();
    let warning = "Failed to load a font face";
    fontgres(&dir)
        .arg("--font-file")
        .arg(&not_a_font)
        .args(["-v", "families"])
        .assert()
        .success()
        .stderr(contains(warning));
    fontgres(&dir)
        .arg("--font-file")
        .arg(&not_a_font)
        .args(["families"])
        .assert()
        .success()
        .stderr(contains(warning).not());
}
