//! Matching family names in a query.

use std::sync::Arc;

use fontgres_core::{Database, Family, Query, Source};

const TUFFY: &[u8] = include_bytes!("./fonts/Tuffy.ttf");

#[test]
fn query_matches_family_names_ignoring_ascii_case() {
    let mut database = Database::new();
    let ids = database.load_font_source(&Source::Binary(Arc::new(TUFFY)));
    assert_eq!(ids.len(), 1);

    for name in ["Tuffy", "tuffy", "TUFFY", "tUfFy"] {
        let families = [Family::Name(name)];
        let query = Query {
            families: &families,
            ..Query::default()
        };
        assert_eq!(
            database.query(&query).map(|face| face.id),
            Some(ids[0]),
            "{name}"
        );
    }
}

#[test]
fn query_still_misses_a_different_family() {
    let mut database = Database::new();
    database.load_font_source(&Source::Binary(Arc::new(TUFFY)));

    let families = [Family::Name("Tuffy Sans")];
    let query = Query {
        families: &families,
        ..Query::default()
    };
    assert!(database.query(&query).is_none());
}
