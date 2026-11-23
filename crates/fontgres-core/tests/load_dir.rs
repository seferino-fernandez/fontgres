//! Which files in a directory load as fonts. Loading from the file system
//! needs the `fs` feature.

#![cfg(feature = "fs")]

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

/// A directory large enough to parse on several threads keeps the order a
/// one-by-one load gives: the directory's order, with a file that is not a
/// font skipped.
#[test]
fn a_large_directory_loads_in_directory_order() {
    let dir = tempfile::tempdir().unwrap();
    for index in 0..64 {
        std::fs::copy(TUFFY, dir.path().join(format!("Tuffy{index:02}.ttf"))).unwrap();
    }
    std::fs::write(dir.path().join("Broken.ttf"), "not a font").unwrap();
    let expected: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| !path.ends_with("Broken.ttf"))
        .collect();

    let mut database = Database::new();
    database.load_fonts_dir(dir.path());
    let loaded: Vec<_> = database
        .faces()
        .map(|face| {
            let fontgres_core::Source::File(path) = &face.source else {
                panic!("{:?} is not a file source", face.source);
            };
            path.clone()
        })
        .collect();
    assert_eq!(loaded, expected);
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
