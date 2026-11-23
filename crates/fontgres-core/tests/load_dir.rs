//! Which files in a directory load as fonts.

use std::path::Path;

use fontgres_core::Database;

const TUFFY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fonts/Tuffy.ttf");

fn faces_in(dir: &Path) -> usize {
    let mut database = Database::new();
    database.load_fonts_dir(dir);
    database.len()
}

#[test]
fn font_extensions_match_in_any_case() {
    for name in ["Tuffy.Otf", "Tuffy.TtF", "Tuffy.ttc", "Tuffy.OTC"] {
        let dir = tempfile::tempdir().unwrap();
        std::fs::copy(TUFFY, dir.path().join(name)).unwrap();
        assert_eq!(faces_in(dir.path()), 1, "{name}");
    }
}

#[test]
fn other_extensions_do_not_load() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::copy(TUFFY, dir.path().join("Tuffy.woff2")).unwrap();
    std::fs::copy(TUFFY, dir.path().join("Tuffy")).unwrap();
    assert_eq!(faces_in(dir.path()), 0);
}

#[cfg(unix)]
mod symlinks {
    use super::{TUFFY, faces_in};
    use std::os::unix::fs::symlink;

    /// Content-addressed stores name files by hash and link font names to them.
    #[test]
    fn a_font_named_link_to_a_file_without_an_extension_loads() {
        let store = tempfile::tempdir().unwrap();
        let blob = store.path().join("0c4c8f45-6b1e-4f7a-9d2c-3e8b5a1f9d07");
        std::fs::copy(TUFFY, &blob).unwrap();
        let fonts = tempfile::tempdir().unwrap();
        symlink(&blob, fonts.path().join("Tuffy.ttf")).unwrap();

        assert_eq!(faces_in(fonts.path()), 1);
    }

    #[test]
    fn a_link_to_a_font_named_file_loads() {
        let store = tempfile::tempdir().unwrap();
        let target = store.path().join("Tuffy.ttf");
        std::fs::copy(TUFFY, &target).unwrap();
        let fonts = tempfile::tempdir().unwrap();
        symlink(&target, fonts.path().join("0c4c8f45")).unwrap();

        assert_eq!(faces_in(fonts.path()), 1);
    }

    #[test]
    fn a_link_and_its_target_both_without_a_font_name_do_not_load() {
        let store = tempfile::tempdir().unwrap();
        let blob = store.path().join("0c4c8f45");
        std::fs::copy(TUFFY, &blob).unwrap();
        let fonts = tempfile::tempdir().unwrap();
        symlink(&blob, fonts.path().join("Tuffy")).unwrap();

        assert_eq!(faces_in(fonts.path()), 0);
    }
}
