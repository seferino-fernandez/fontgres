//! The order a database keeps its faces in, and what an ID says about it.

use std::sync::Arc;

use fontgres_core::{Database, FaceInfo, ID, Language, Source};

const TUFFY: &[u8] = include_bytes!("./fonts/Tuffy.ttf");

/// Adds Tuffy's face, renamed, to `database`: a face of `family` whose
/// `PostScript` name is `name`. The face arrives with the ID another database
/// gave it, which adding it replaces.
fn push(database: &mut Database, family: &str, name: &str) -> Option<ID> {
    let mut tuffy = Database::new();
    let loaded = tuffy.load_font_data(TUFFY.to_vec()).added().next()?;
    let mut face = tuffy.face(loaded)?.clone();
    face.families = vec![(family.to_owned(), Language::English_UnitedStates)];
    name.clone_into(&mut face.post_script_name);
    Some(database.push_face_info(face))
}

fn names<'a>(faces: impl IntoIterator<Item = &'a FaceInfo>) -> Vec<&'a str> {
    faces
        .into_iter()
        .map(|face| face.post_script_name.as_str())
        .collect()
}

#[test]
fn faces_come_back_in_the_order_they_were_added() {
    let mut database = Database::new();
    for name in ["c", "a", "b"] {
        push(&mut database, "Family", name).unwrap();
    }
    assert_eq!(database.load_font_data(TUFFY.to_vec()).added().len(), 1);
    push(&mut database, "Family", "d").unwrap();

    assert_eq!(names(database.faces()), ["c", "a", "b", "Tuffy", "d"]);
}

#[test]
fn ids_ascend_in_load_order_and_find_their_face() {
    let mut database = Database::new();
    let pushed = push(&mut database, "Family", "first").unwrap();
    let report = database.load_font_source(&Source::Binary(Arc::new(TUFFY)));
    let loaded: Vec<ID> = report.added().collect();
    let last = push(&mut database, "Family", "last").unwrap();

    let ids = [pushed, loaded[0], last];
    assert!(ids.is_sorted_by(|a, b| a < b), "{ids:?}");
    for (position, id) in ids.into_iter().enumerate() {
        let found = database.face(id).unwrap();
        assert_eq!(found.id, id);
        assert_eq!(
            found.post_script_name,
            database.faces()[position].post_script_name
        );
    }
}

#[test]
fn the_dummy_id_matches_no_face() {
    let mut database = Database::new();
    push(&mut database, "Family", "only").unwrap();

    assert!(database.face(ID::dummy()).is_none());
}

/// A load reports the faces it added and no others: the run of IDs starts
/// after the faces the database already held and ends before the next load's.
#[test]
fn a_load_reports_the_faces_it_added_as_one_run() {
    let mut database = Database::new();
    push(&mut database, "Family", "first").unwrap();
    let first = database.load_font_data(TUFFY.to_vec());
    let nothing = database.load_font_data(b"not a font".to_vec());
    let second = database.load_font_data(TUFFY.to_vec());

    let ids: Vec<ID> = database.faces().iter().map(|face| face.id).collect();
    assert_eq!(first.added().collect::<Vec<_>>(), [ids[1]]);
    assert_eq!(second.added().collect::<Vec<_>>(), [ids[2]]);
    assert_eq!(first.skipped(), 0);
    assert!(first.failures().is_empty());

    assert_eq!(nothing.added().next(), None);
    assert_eq!(nothing.failures().len(), 1);
}

#[test]
fn unequal_ids_display_as_different_strings() {
    let mut database = Database::new();
    let first = push(&mut database, "Family", "first").unwrap();
    let second = push(&mut database, "Family", "second").unwrap();

    assert_ne!(first.to_string(), second.to_string());
    assert_ne!(first.to_string(), ID::dummy().to_string());
    assert_ne!(first.to_string(), "");
}

#[test]
fn face_data_is_lent_with_the_index_of_the_face() {
    let mut database = Database::new();
    let report = database.load_font_source(&Source::Binary(Arc::new(TUFFY)));
    let ids: Vec<ID> = report.added().collect();

    let lent = database.with_face_data(ids[0], |data, index| (data.len(), index));
    assert_eq!(lent, Some((TUFFY.len(), 0)));
    assert_eq!(database.with_face_data(ID::dummy(), |_, _| ()), None);
}

#[test]
fn a_family_lists_its_faces_in_load_order_under_any_case() {
    let mut database = Database::new();
    for (family, name) in [("Family", "b"), ("Other", "x"), ("FAMILY", "a")] {
        push(&mut database, family, name).unwrap();
    }

    assert_eq!(names(database.family_faces("family")), ["b", "a"]);
    assert_eq!(database.family_faces("Missing").count(), 0);
}
