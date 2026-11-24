//! Choosing among the faces of one family by stretch, style and weight, as
//! CSS font matching does.

use std::sync::Arc;

use fontgres_core::{
    Database, FaceInfo, Family, ID, Language, Query, Source, Stretch, Style, Weight,
};

const FAMILY: &str = "Synthetic";

fn face(stretch: Stretch, style: Style, weight: u16) -> FaceInfo {
    FaceInfo {
        id: ID::dummy(),
        source: Source::Binary(Arc::new(Vec::<u8>::new())),
        index: 0,
        families: vec![(FAMILY.to_owned(), Language::English_UnitedStates)],
        post_script_name: format!("{FAMILY}-{stretch:?}-{style:?}-{weight}"),
        style,
        weight: Weight(weight),
        stretch,
        monospaced: false,
    }
}

/// Loads `faces` in order and returns the one the query picks, as an index
/// into `faces`.
fn pick(faces: &[FaceInfo], stretch: Stretch, style: Style, weight: u16) -> Option<usize> {
    let mut database = Database::new();
    let ids: Vec<ID> = faces
        .iter()
        .map(|face| database.push_face_info(face.clone()))
        .collect();
    let families = [Family::Name(FAMILY)];
    let query = Query {
        families: &families,
        weight: Weight(weight),
        stretch,
        style,
    };
    let id = database.query(&query)?.id;
    ids.iter().position(|&candidate| candidate == id)
}

fn pick_weight(weights: &[u16], query: u16) -> Option<u16> {
    let faces: Vec<FaceInfo> = weights
        .iter()
        .map(|&weight| face(Stretch::Normal, Style::Normal, weight))
        .collect();
    pick(&faces, Stretch::Normal, Style::Normal, query)
        .and_then(|index| weights.get(index).copied())
}

fn pick_style(styles: &[Style], query: Style) -> Option<Style> {
    let faces: Vec<FaceInfo> = styles
        .iter()
        .map(|&style| face(Stretch::Normal, style, 400))
        .collect();
    pick(&faces, Stretch::Normal, query, 400).and_then(|index| styles.get(index).copied())
}

fn pick_stretch(stretches: &[Stretch], query: Stretch) -> Option<Stretch> {
    let faces: Vec<FaceInfo> = stretches
        .iter()
        .map(|&stretch| face(stretch, Style::Normal, 400))
        .collect();
    pick(&faces, query, Style::Normal, 400).and_then(|index| stretches.get(index).copied())
}

#[test]
fn an_exact_weight_wins() {
    assert_eq!(pick_weight(&[300, 400, 500], 400), Some(400));
}

#[test]
fn a_weight_from_400_below_450_tries_500_first() {
    assert_eq!(pick_weight(&[300, 500], 400), Some(500));
    assert_eq!(pick_weight(&[300, 500], 449), Some(500));
}

#[test]
fn a_weight_from_450_to_500_tries_400_first() {
    assert_eq!(pick_weight(&[400, 600], 450), Some(400));
    assert_eq!(pick_weight(&[400, 600], 500), Some(400));
}

#[test]
fn a_weight_up_to_500_takes_the_nearest_lighter_then_the_nearest_heavier() {
    assert_eq!(pick_weight(&[100, 200, 600], 300), Some(200));
    assert_eq!(pick_weight(&[300, 600], 400), Some(300));
    assert_eq!(pick_weight(&[800, 600], 300), Some(600));
}

#[test]
fn a_weight_above_500_takes_the_nearest_heavier_then_the_nearest_lighter() {
    assert_eq!(pick_weight(&[300, 900, 800], 700), Some(800));
    assert_eq!(pick_weight(&[300, 600], 700), Some(600));
}

#[test]
fn the_preferred_side_wins_over_a_nearer_value_on_the_other_side() {
    assert_eq!(pick_weight(&[200, 350], 300), Some(200));
    assert_eq!(pick_weight(&[650, 800], 700), Some(800));
    let stretches = [Stretch::UltraCondensed, Stretch::SemiCondensed];
    assert_eq!(
        pick_stretch(&stretches, Stretch::Condensed),
        Some(Stretch::UltraCondensed)
    );
}

#[test]
fn a_weight_from_450_to_500_without_400_takes_the_nearest_lighter() {
    assert_eq!(pick_weight(&[300, 600], 480), Some(300));
    assert_eq!(pick_weight(&[300, 600], 420), Some(300));
}

#[test]
fn italic_falls_back_to_oblique_then_normal() {
    assert_eq!(
        pick_style(&[Style::Normal, Style::Oblique], Style::Italic),
        Some(Style::Oblique)
    );
    assert_eq!(
        pick_style(&[Style::Normal], Style::Italic),
        Some(Style::Normal)
    );
}

#[test]
fn oblique_falls_back_to_italic_then_normal() {
    assert_eq!(
        pick_style(&[Style::Normal, Style::Italic], Style::Oblique),
        Some(Style::Italic)
    );
    assert_eq!(
        pick_style(&[Style::Normal], Style::Oblique),
        Some(Style::Normal)
    );
}

#[test]
fn normal_falls_back_to_oblique_then_italic() {
    assert_eq!(
        pick_style(&[Style::Italic, Style::Oblique], Style::Normal),
        Some(Style::Oblique)
    );
    assert_eq!(
        pick_style(&[Style::Italic], Style::Normal),
        Some(Style::Italic)
    );
}

#[test]
fn a_narrow_stretch_takes_the_nearest_narrower_then_the_nearest_wider() {
    let narrow = [
        Stretch::UltraCondensed,
        Stretch::ExtraCondensed,
        Stretch::Expanded,
    ];
    assert_eq!(
        pick_stretch(&narrow, Stretch::Condensed),
        Some(Stretch::ExtraCondensed)
    );
    let wide = [Stretch::Expanded, Stretch::SemiExpanded];
    assert_eq!(
        pick_stretch(&wide, Stretch::Condensed),
        Some(Stretch::SemiExpanded)
    );
    assert_eq!(
        pick_stretch(&wide, Stretch::Normal),
        Some(Stretch::SemiExpanded)
    );
}

#[test]
fn a_wide_stretch_takes_the_nearest_wider_then_the_nearest_narrower() {
    let wide = [
        Stretch::UltraExpanded,
        Stretch::ExtraExpanded,
        Stretch::Condensed,
    ];
    assert_eq!(
        pick_stretch(&wide, Stretch::Expanded),
        Some(Stretch::ExtraExpanded)
    );
    let narrow = [Stretch::Condensed, Stretch::SemiCondensed];
    assert_eq!(
        pick_stretch(&narrow, Stretch::Expanded),
        Some(Stretch::SemiCondensed)
    );
}

#[test]
fn stretch_narrows_the_set_before_style_and_style_before_weight() {
    let faces = [
        face(Stretch::Condensed, Style::Italic, 700),
        face(Stretch::Normal, Style::Normal, 400),
        face(Stretch::Normal, Style::Italic, 300),
    ];
    assert_eq!(
        pick(&faces, Stretch::Condensed, Style::Normal, 400),
        Some(0)
    );
    assert_eq!(pick(&faces, Stretch::Normal, Style::Italic, 700), Some(2));
}

#[test]
fn the_first_loaded_of_equal_faces_wins() {
    let faces = [
        face(Stretch::Normal, Style::Normal, 400),
        face(Stretch::Normal, Style::Normal, 400),
    ];
    assert_eq!(pick(&faces, Stretch::Normal, Style::Normal, 400), Some(0));
}
