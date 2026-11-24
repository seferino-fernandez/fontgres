//! What the file-system loads report: the faces added, the font files
//! skipped as already loaded, and the files and faces that failed. Loading
//! from the file system needs the `fs` feature.

#![cfg(feature = "fs")]

use std::path::Path;
use std::sync::Arc;

use fontgres_core::{Database, FontLoadFailure, FontLoadFailureReason, FontLoadReport, Source};

const TUFFY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fonts/Tuffy.ttf");

/// A directory holding `good.ttf`, a font, and `bad.ttf`, which is not one.
fn dir_with_a_font_and_a_file_that_is_not_one() -> std::io::Result<tempfile::TempDir> {
    let dir = tempfile::tempdir()?;
    std::fs::copy(TUFFY, dir.path().join("good.ttf"))?;
    std::fs::write(dir.path().join("bad.ttf"), "not a font")?;
    Ok(dir)
}

/// A failure as its path, its face index and whether the reason is a
/// malformed font.
type Failed<'a> = (Option<&'a Path>, Option<u32>, bool);

/// How many faces a load added, how many files it skipped, and its failures.
fn summary(report: &FontLoadReport) -> (usize, usize, Vec<Failed<'_>>) {
    let failures = report.failures().iter().map(|failure| {
        (
            failure.path.as_deref(),
            failure.face_index,
            matches!(failure.reason, FontLoadFailureReason::MalformedFont),
        )
    });
    (report.added().len(), report.skipped(), failures.collect())
}

/// The path and the kind of I/O error of a failure to read a whole file;
/// `None` for any other failure.
fn unreadable_file(failure: &FontLoadFailure) -> Option<(&Path, std::io::ErrorKind)> {
    match (&failure.path, failure.face_index, &failure.reason) {
        (Some(path), None, FontLoadFailureReason::UnreadableFile(error)) => {
            Some((path, error.kind()))
        }
        _ => None,
    }
}

/// A database that has loaded the directory
/// [`dir_with_a_font_and_a_file_that_is_not_one`] makes, with the directory,
/// the path of the file that is not a font, and how many faces loaded.
fn database_after_a_walk()
-> std::io::Result<(tempfile::TempDir, std::path::PathBuf, Database, usize)> {
    let dir = dir_with_a_font_and_a_file_that_is_not_one()?;
    let bad = dir.path().join("bad.ttf");
    let mut database = Database::new();
    let added = database.load_fonts_dir(dir.path())?.added().len();
    Ok((dir, bad, database, added))
}

/// The one failure of a report that has exactly one.
fn only_failure(report: &FontLoadReport) -> Option<&FontLoadFailure> {
    match report.failures() {
        [failure] => Some(failure),
        _ => None,
    }
}

#[test]
fn a_font_file_reports_its_face_and_is_skipped_the_next_time() {
    let mut database = Database::new();

    let first = database.load_font_file(TUFFY).unwrap();
    assert_eq!(summary(&first), (1, 0, vec![]));
    assert_eq!(first.added().collect::<Vec<_>>(), [database.faces()[0].id]);

    let second = database.load_font_file(TUFFY).unwrap();
    assert_eq!(summary(&second), (0, 1, vec![]));
    assert_eq!(database.len(), 1);
}

/// A readable file that is not a font is not an error of the load: it gives
/// a failure that names the file, the face and the reason. The database does
/// not skip the file afterwards, so every call reports the failure.
#[test]
fn a_file_that_is_not_a_font_reports_its_failure_on_every_call() {
    let dir = dir_with_a_font_and_a_file_that_is_not_one().unwrap();
    let bad = dir.path().join("bad.ttf");
    let mut database = Database::new();

    for call in ["first", "second"] {
        let report = database.load_font_file(&bad).unwrap();
        assert_eq!(
            summary(&report),
            (0, 0, vec![(Some(bad.as_path()), Some(0), true)]),
            "{call}"
        );
        assert_eq!(
            only_failure(&report).unwrap().to_string(),
            format!("cannot load face 0 of {}: malformed font", bad.display())
        );
    }
    assert!(database.is_empty());
}

/// A file that cannot be read is an error and leaves no trace: once the file
/// can be read, it loads.
#[test]
fn a_font_file_that_cannot_be_read_is_an_error_and_loads_later() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Tuffy.ttf");
    let mut database = Database::new();

    let error = database.load_font_file(&path).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    assert!(database.is_empty());

    std::fs::copy(TUFFY, &path).unwrap();
    let report = database.load_font_file(&path).unwrap();
    assert_eq!(summary(&report), (1, 0, vec![]));
}

/// A file source that cannot be read is a failure of the whole file: it has
/// a path and no face index, and its reason is the I/O error.
#[test]
fn a_file_source_that_cannot_be_read_reports_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("missing.ttf");
    let mut database = Database::new();

    let report = database.load_font_source(&Source::File(Arc::from(path.as_path())));
    assert_eq!(report.added().len(), 0);
    let failure = only_failure(&report).unwrap();
    assert_eq!(
        unreadable_file(failure),
        Some((path.as_path(), std::io::ErrorKind::NotFound))
    );
    assert_eq!(
        failure.to_string(),
        format!(
            "cannot load font file {}: {}",
            path.display(),
            failure.reason
        )
    );
    assert_ne!(failure.reason.to_string(), "");
}

/// A file loaded as a source is not remembered, so it loads on every call.
#[test]
fn a_file_source_loads_on_every_call() {
    let source = Source::File(Arc::from(Path::new(TUFFY)));
    let mut database = Database::new();

    for _ in 0..2 {
        let report = database.load_font_source(&source);
        assert_eq!(summary(&report), (1, 0, vec![]));
    }
    assert_eq!(database.len(), 2);
}

/// A directory load adds the fonts and reports the file that is not one.
/// Loading the directory again skips both: the walk does not read a file
/// again because it gave no face.
#[test]
fn a_directory_reports_its_faces_its_failures_and_then_its_skipped_files() {
    let dir = dir_with_a_font_and_a_file_that_is_not_one().unwrap();
    let bad = dir.path().join("bad.ttf");
    let mut database = Database::new();

    let first = database.load_fonts_dir(dir.path()).unwrap();
    assert_eq!(
        summary(&first),
        (1, 0, vec![(Some(bad.as_path()), Some(0), true)])
    );

    let second = database.load_fonts_dir(dir.path()).unwrap();
    assert_eq!(summary(&second), (0, 2, vec![]));
    assert_eq!(database.len(), 1);
}

/// After a directory load, loading its files by path skips the one that gave
/// a face and reads the other again, so the caller who names a file learns
/// why it gives no face.
#[test]
fn a_named_file_is_read_again_only_when_a_walk_got_no_face_from_it() {
    let (dir, bad, mut database, added) = database_after_a_walk().unwrap();
    assert_eq!(added, 1);

    let good = database
        .load_font_file(dir.path().join("good.ttf"))
        .unwrap();
    assert_eq!(summary(&good), (0, 1, vec![]));

    let again = database.load_font_file(&bad).unwrap();
    assert_eq!(
        summary(&again),
        (0, 0, vec![(Some(bad.as_path()), Some(0), true)])
    );
    assert_eq!(database.len(), 1);
}

/// A file that gave no face loads by path once it holds a font, and from
/// then on every load skips it.
#[test]
fn a_file_that_gave_no_face_loads_by_path_once_it_holds_a_font() {
    let (dir, bad, mut database, added) = database_after_a_walk().unwrap();
    assert_eq!(added, 1);

    std::fs::copy(TUFFY, &bad).unwrap();
    // The walk still passes over the file it could not load before.
    let walk = database.load_fonts_dir(dir.path()).unwrap();
    assert_eq!(summary(&walk), (0, 2, vec![]));

    let named = database.load_font_file(&bad).unwrap();
    assert_eq!(summary(&named), (1, 0, vec![]));
    let again = database.load_font_file(&bad).unwrap();
    assert_eq!(summary(&again), (0, 1, vec![]));
    assert_eq!(database.len(), 2);
}

/// The root of a directory load must be readable; the database is unchanged
/// when it is not.
#[test]
fn a_directory_that_cannot_be_read_is_an_error() {
    let dir = dir_with_a_font_and_a_file_that_is_not_one().unwrap();
    let mut database = Database::new();

    let missing = database
        .load_fonts_dir(dir.path().join("missing"))
        .unwrap_err();
    assert_eq!(missing.kind(), std::io::ErrorKind::NotFound);

    let a_file = database.load_fonts_dir(dir.path().join("good.ttf"));
    assert!(a_file.is_err(), "{a_file:?}");
    assert!(database.is_empty());

    // The font was not remembered by the failed loads.
    let report = database.load_fonts_dir(dir.path()).unwrap();
    assert_eq!(report.added().len(), 1);
}

#[cfg(unix)]
mod unix {
    use std::os::unix::fs::{PermissionsExt, symlink};

    use super::{
        Database, Path, TUFFY, dir_with_a_font_and_a_file_that_is_not_one, only_failure, summary,
        unreadable_file,
    };

    /// Takes every permission from `path`, and returns whether that keeps
    /// this process from reading it: a privileged user reads it anyway.
    fn make_unreadable(path: &Path) -> std::io::Result<bool> {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o000))?;
        let readable = if path.is_dir() {
            std::fs::read_dir(path).is_ok()
        } else {
            std::fs::File::open(path).is_ok()
        };
        Ok(!readable)
    }

    /// A walk that reaches one file by two paths loads it once and counts the
    /// second path as skipped.
    #[test]
    fn a_second_path_to_a_file_in_one_walk_counts_as_skipped() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::copy(TUFFY, dir.path().join("Tuffy.ttf")).unwrap();
        symlink(dir.path().join("Tuffy.ttf"), dir.path().join("Link.ttf")).unwrap();
        let mut database = Database::new();

        let report = database.load_fonts_dir(dir.path()).unwrap();
        assert_eq!(summary(&report), (1, 1, vec![]));
    }

    /// Only the root of a walk must be readable. A subdirectory that is not
    /// adds nothing and is not reported.
    #[test]
    fn a_subdirectory_that_cannot_be_read_adds_nothing_and_is_not_reported() {
        let dir = dir_with_a_font_and_a_file_that_is_not_one().unwrap();
        std::fs::remove_file(dir.path().join("bad.ttf")).unwrap();
        let locked = dir.path().join("locked");
        std::fs::create_dir(&locked).unwrap();
        std::fs::copy(TUFFY, locked.join("Tuffy.ttf")).unwrap();
        if !make_unreadable(&locked).unwrap() {
            return;
        }
        let mut database = Database::new();

        let report = database.load_fonts_dir(dir.path());
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(summary(&report.unwrap()), (1, 0, vec![]));
    }

    /// A file the walk finds and cannot read is a failure of the whole file,
    /// and the walk goes on to the files after it.
    #[test]
    fn a_file_a_walk_cannot_read_is_reported_and_the_walk_goes_on() {
        let dir = tempfile::tempdir().unwrap();
        let locked = dir.path().join("a.ttf");
        std::fs::copy(TUFFY, &locked).unwrap();
        std::fs::copy(TUFFY, dir.path().join("b.ttf")).unwrap();
        if !make_unreadable(&locked).unwrap() {
            return;
        }
        let mut database = Database::new();

        let report = database.load_fonts_dir(dir.path()).unwrap();
        assert_eq!(report.added().len(), 1);
        assert_eq!(
            only_failure(&report).and_then(unreadable_file),
            Some((locked.as_path(), std::io::ErrorKind::PermissionDenied))
        );
    }
}
