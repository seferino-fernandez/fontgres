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

/// The paths of the faces in `database`, in load order. A face that was not
/// loaded from a file gives an empty path, which equals no expected path.
fn loaded_paths(database: &Database) -> Vec<std::path::PathBuf> {
    use fontgres_core::Source;

    database
        .faces()
        .iter()
        .map(|face| match &face.source {
            Source::File(path) => path.to_path_buf(),
            Source::Binary(_) => std::path::PathBuf::new(),
        })
        .collect()
}

/// A directory holding a copy of Tuffy under each of `names`, made in that
/// order; a name can put its copy in a subdirectory.
fn dir_with_copies<P: AsRef<Path>>(
    names: impl IntoIterator<Item = P>,
) -> std::io::Result<tempfile::TempDir> {
    let dir = tempfile::tempdir()?;
    for name in names {
        let path = dir.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(TUFFY, path)?;
    }
    Ok(dir)
}

/// A directory holding a copy of Tuffy.
fn dir_with_tuffy() -> std::io::Result<tempfile::TempDir> {
    dir_with_copies(["Tuffy.ttf"])
}

/// The paths `load_fonts_dir` loads from `dir`, in load order.
fn paths_loaded_from(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut database = Database::new();
    database.load_fonts_dir(dir);
    loaded_paths(&database)
}

/// A directory large enough to parse on several threads loads in the order
/// of its file names, with a file that is not a font skipped. The files are
/// created in an order that is neither that one nor its reverse, so a file
/// system that lists entries as they were created does not pass by accident.
#[test]
fn a_large_directory_loads_in_file_name_order() {
    let name = |index: usize| format!("Tuffy{index:02}.ttf");
    let dir = dir_with_copies((0..64).map(|step| name(step * 37 % 64))).unwrap();
    std::fs::write(dir.path().join("Tuffy31a.ttf"), "not a font").unwrap();

    let expected: Vec<_> = (0..64).map(|index| dir.path().join(name(index))).collect();
    assert_eq!(paths_loaded_from(dir.path()), expected);
}

/// Files and subdirectories sort together by name, compared as bytes, so an
/// uppercase name loads before a lowercase one and a subdirectory's fonts
/// load where its name falls.
#[test]
fn files_and_subdirectories_load_in_one_file_name_order() {
    let dir = dir_with_copies(["c.ttf", "b/z.ttf", "a.ttf", "b/y.ttf", "Z.ttf"]).unwrap();

    let expected =
        ["Z.ttf", "a.ttf", "b/y.ttf", "b/z.ttf", "c.ttf"].map(|name| dir.path().join(name));
    assert_eq!(paths_loaded_from(dir.path()), expected);
}

/// Two files hold the same face, so a query for it is a tie, which the face
/// loaded first wins: the one whose file name sorts first.
#[test]
fn a_tie_between_two_files_goes_to_the_first_file_name() {
    use fontgres_core::{Family, Query, Source};

    let dir = dir_with_copies(["b.ttf", "a.ttf", "c.ttf"]).unwrap();
    let mut database = Database::new();
    database.load_fonts_dir(dir.path());

    let families = [Family::Name("Tuffy")];
    let query = Query {
        families: &families,
        ..Query::default()
    };
    let winner = database.query(&query).map(|face| &face.source);
    assert!(
        matches!(winner, Some(Source::File(path)) if **path == dir.path().join("a.ttf")),
        "{winner:?}"
    );
}

#[test]
fn a_directory_loaded_twice_loads_its_fonts_once() {
    let dir = dir_with_tuffy().unwrap();
    let mut database = Database::new();
    database.load_fonts_dir(dir.path());
    database.load_fonts_dir(dir.path());
    assert_eq!(database.len(), 1);
}

/// A file source prints as the path it reads, so a logged face names its file.
#[test]
fn a_file_source_debug_prints_its_path() {
    let dir = dir_with_tuffy().unwrap();
    let mut database = Database::new();
    database.load_fonts_dir(dir.path());

    let printed = format!("{:?}", database.faces()[0].source);
    assert_eq!(printed, format!("File({:?})", dir.path().join("Tuffy.ttf")));
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

#[cfg(unix)]
mod symlinks {
    use super::{TUFFY, faces_in, loaded_paths};
    use fontgres_core::Database;
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
        assert_eq!(loaded_paths(&database), [root.join("Tuffy.ttf")]);
    }
}
