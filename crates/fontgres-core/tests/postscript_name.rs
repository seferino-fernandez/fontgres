//! Loading a font that has no PostScript name.

use fontgres_core::Database;

/// `TuffyMissingPS.ttf` is Tuffy with every name ID 6 record removed and its
/// family renamed; `tests/fixtures/README.md` has the recipe.
const MISSING_PS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/TuffyMissingPS.ttf"
);

#[test]
fn a_face_without_a_postscript_name_loads_with_one_made_from_its_family() {
    let mut database = Database::new();
    database.load_font_data(std::fs::read(MISSING_PS).unwrap());

    let faces = database.faces();
    assert_eq!(faces.len(), 1);
    assert_eq!(faces[0].families[0].0, "Tuffy Missing PS");
    assert_eq!(faces[0].post_script_name, "Tuffy-Missing-PS");
}
