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

/// A directory holding a copy of Tuffy.
fn dir_with_tuffy() -> std::io::Result<tempfile::TempDir> {
    let dir = tempfile::tempdir()?;
    std::fs::copy(TUFFY, dir.path().join("Tuffy.ttf"))?;
    Ok(dir)
}

#[test]
fn a_directory_loaded_twice_loads_its_fonts_once() {
    let dir = dir_with_tuffy().unwrap();
    let mut database = Database::new();
    database.load_fonts_dir(dir.path());
    database.load_fonts_dir(dir.path());
    assert_eq!(database.len(), 1);
}

/// Whichever of the directory and the file loads first, the font loads once.
#[test]
fn a_font_file_loaded_with_its_directory_loads_once() {
    let dir = dir_with_tuffy().unwrap();
    let font = dir.path().join("Tuffy.ttf");

    let mut dir_first = Database::new();
    dir_first.load_fonts_dir(dir.path());
    dir_first.load_font_file(&font).unwrap();
    assert_eq!(dir_first.len(), 1, "directory first");

    let mut file_first = Database::new();
    file_first.load_font_file(&font).unwrap();
    file_first.load_fonts_dir(dir.path());
    assert_eq!(file_first.len(), 1, "file first");
}

#[test]
fn a_directory_loaded_again_loads_the_fonts_added_since() {
    let dir = dir_with_tuffy().unwrap();
    let mut database = Database::new();
    database.load_fonts_dir(dir.path());
    std::fs::copy(TUFFY, dir.path().join("Tuffy2.ttf")).unwrap();
    database.load_fonts_dir(dir.path());
    assert_eq!(database.len(), 2);
}

#[test]
fn a_removed_face_stays_removed_when_its_directory_loads_again() {
    let dir = dir_with_tuffy().unwrap();
    let mut database = Database::new();
    database.load_fonts_dir(dir.path());
    let id = database.faces().next().unwrap().id;
    database.remove_face(id);
    database.load_fonts_dir(dir.path());
    assert!(database.is_empty());
}

#[cfg(unix)]
mod symlinks {
    use super::{TUFFY, faces_in};
    use fontgres_core::{Database, Source};
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

    /// Links the font in `real` from `link`, two sibling directories of one
    /// walk, and counts the faces the walk loads.
    fn faces_with_link_beside(real: &str, link: &str) -> std::io::Result<usize> {
        let dir = tempfile::tempdir()?;
        let target = dir.path().join(real).join("Tuffy.ttf");
        std::fs::create_dir(dir.path().join(real))?;
        std::fs::create_dir(dir.path().join(link))?;
        std::fs::copy(TUFFY, &target)?;
        symlink(&target, dir.path().join(link).join("Link.ttf"))?;
        Ok(faces_in(dir.path()))
    }

    /// Whichever of the file and the link the walk reaches first, the font
    /// loads once.
    #[test]
    fn a_font_and_a_link_to_it_in_one_walk_load_once() {
        assert_eq!(
            faces_with_link_beside("a", "b").unwrap(),
            1,
            "file walked first"
        );
        assert_eq!(
            faces_with_link_beside("b", "a").unwrap(),
            1,
            "link walked first"
        );
    }

    #[test]
    fn a_link_to_a_directory_loads_the_fonts_in_it() {
        let store = tempfile::tempdir().unwrap();
        std::fs::copy(TUFFY, store.path().join("Tuffy.ttf")).unwrap();
        let fonts = tempfile::tempdir().unwrap();
        symlink(store.path(), fonts.path().join("linked")).unwrap();

        assert_eq!(faces_in(fonts.path()), 1);
    }

    /// Links a directory of fonts from `link`, a sibling of the directory, and
    /// counts the faces the walk loads.
    fn faces_with_dir_link_beside(real: &str, link: &str) -> std::io::Result<usize> {
        let dir = tempfile::tempdir()?;
        std::fs::create_dir(dir.path().join(real))?;
        std::fs::copy(TUFFY, dir.path().join(real).join("Tuffy.ttf"))?;
        symlink(dir.path().join(real), dir.path().join(link))?;
        Ok(faces_in(dir.path()))
    }

    #[test]
    fn a_directory_and_a_link_to_it_in_one_walk_load_its_fonts_once() {
        assert_eq!(
            faces_with_dir_link_beside("a", "b").unwrap(),
            1,
            "directory walked first"
        );
        assert_eq!(
            faces_with_dir_link_beside("b", "a").unwrap(),
            1,
            "link walked first"
        );
    }

    /// Loads a directory of fonts and a directory of links to them, in two
    /// calls, and counts the faces.
    fn faces_with_links_loaded_after(links_first: bool) -> std::io::Result<usize> {
        let fonts = tempfile::tempdir()?;
        let target = fonts.path().join("Tuffy.ttf");
        std::fs::copy(TUFFY, &target)?;
        let links = tempfile::tempdir()?;
        symlink(&target, links.path().join("Link.ttf"))?;

        let mut database = Database::new();
        let order = if links_first {
            [links.path(), fonts.path()]
        } else {
            [fonts.path(), links.path()]
        };
        for dir in order {
            database.load_fonts_dir(dir);
        }
        Ok(database.len())
    }

    #[test]
    fn a_font_and_a_link_to_it_in_two_calls_load_once() {
        assert_eq!(
            faces_with_links_loaded_after(false).unwrap(),
            1,
            "fonts loaded first"
        );
        assert_eq!(
            faces_with_links_loaded_after(true).unwrap(),
            1,
            "links loaded first"
        );
    }

    /// A directory reached through a link, like `/tmp` on macOS, holding a
    /// font and a link to it. Returns the store that holds both and the
    /// linked path.
    fn linked_root() -> std::io::Result<(tempfile::TempDir, std::path::PathBuf)> {
        let store = tempfile::tempdir()?;
        let real = store.path().join("real");
        std::fs::create_dir(&real)?;
        std::fs::copy(TUFFY, real.join("Tuffy.ttf"))?;
        symlink(real.join("Tuffy.ttf"), real.join("Link.ttf"))?;
        let root = store.path().join("root");
        symlink(&real, &root)?;
        Ok((store, root))
    }

    #[test]
    fn a_font_and_a_link_to_it_under_a_linked_root_load_once() {
        let (_store, root) = linked_root().unwrap();
        assert_eq!(faces_in(&root), 1);
    }

    /// A font file that is not a link keeps the path the walk reached it by.
    #[test]
    fn a_font_keeps_the_path_it_was_found_at() {
        let (_store, root) = linked_root().unwrap();
        std::fs::remove_file(root.join("Link.ttf")).unwrap();

        let mut database = Database::new();
        database.load_fonts_dir(&root);
        let paths: Vec<_> = database
            .faces()
            .map(|face| {
                let Source::File(path) = &face.source else {
                    panic!("{:?} is not a file source", face.source);
                };
                path.clone()
            })
            .collect();
        assert_eq!(paths, [root.join("Tuffy.ttf")]);
    }
}
