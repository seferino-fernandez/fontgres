//! Generic families from a fontconfig configuration. Only the Unix systems
//! that read fontconfig run it.

#![cfg(all(
    unix,
    feature = "fontconfig",
    not(any(target_os = "macos", target_os = "ios", target_os = "android"))
))]

use fontgres_core::{Database, Family};

/// Set in the child process that runs the assertions, with `FONTCONFIG_FILE`
/// pointing at the test's configuration.
const CHILD: &str = "FONTGRES_FONTCONFIG_TEST_CHILD";

/// A generic family resolves to the first installed family fontconfig
/// prefers for it, even when a later alias prefers another family.
///
/// The test writes a configuration, then runs itself again in a child
/// process whose environment names it: setting a variable in this process
/// could race with any thread reading the environment.
#[test]
fn generic_family_is_the_first_installed_preference() {
    if std::env::var_os(CHILD).is_some() {
        assert_generic_families();
        return;
    }

    let fonts = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fonts");
    let config = format!(
        r#"<?xml version="1.0"?>
<fontconfig>
  <dir>{}</dir>
  <alias>
    <family>serif</family>
    <prefer><family>Missing Serif</family><family>Tuffy</family></prefer>
  </alias>
  <alias>
    <family>serif</family>
    <prefer><family>Missing Symbols</family></prefer>
  </alias>
  <alias>
    <family>monospace</family>
    <prefer><family>Missing Mono</family></prefer>
  </alias>
</fontconfig>
"#,
        fonts.display()
    );
    let directory = tempfile::tempdir().unwrap();
    let config_path = directory.path().join("fonts.conf");
    std::fs::write(&config_path, config).unwrap();

    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "generic_family_is_the_first_installed_preference",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .env("FONTCONFIG_FILE", &config_path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    // Without --exact matching one test, the child would run none and pass.
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
}

fn assert_generic_families() {
    let mut database = Database::new();
    let report = database.load_system_fonts();

    assert_eq!(report.added().len(), 1);
    assert_eq!(database.len(), 1);
    assert_eq!(database.family_name(&Family::Serif), "Tuffy");
    // Without an installed preference, the family is the first one fontconfig tries.
    assert_eq!(database.family_name(&Family::Monospace), "Missing Mono");
    // A generic family without an alias keeps its default.
    assert_eq!(database.family_name(&Family::Cursive), "Comic Sans MS");
}
