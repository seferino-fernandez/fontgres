//! Generic families from a fontconfig configuration. Only the Unix systems
//! that read fontconfig run it.

#![cfg(all(
    unix,
    feature = "fontconfig",
    not(any(target_os = "macos", target_os = "ios", target_os = "android"))
))]

use fontgres_core::{Database, Family};

/// A generic family resolves to the first installed family fontconfig
/// prefers for it, even when a later alias prefers another family.
#[test]
fn generic_family_is_the_first_installed_preference() {
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
    // This file holds one test, and nextest runs each test in its own process.
    std::env::set_var("FONTCONFIG_FILE", &config_path);

    let mut database = Database::new();
    database.load_system_fonts();

    assert_eq!(database.len(), 1);
    assert_eq!(database.family_name(&Family::Serif), "Tuffy");
    // Without an installed preference, the family is the first one fontconfig tries.
    assert_eq!(database.family_name(&Family::Monospace), "Missing Mono");
    // A generic family without an alias keeps its default.
    assert_eq!(database.family_name(&Family::Cursive), "Comic Sans MS");
}
