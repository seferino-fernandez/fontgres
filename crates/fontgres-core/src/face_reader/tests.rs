//! Reading a face's properties from its font tables, with fonts built here
//! byte by byte.

use alloc::{string::ToString, vec, vec::Vec};

use super::{FaceProperties, Language, Stretch, read};
use crate::{Database, FontLoadFailureReason, Style, Weight};

const UNICODE: u16 = 0;
const MACINTOSH: u16 = 1;
const ISO: u16 = 2;
const WINDOWS: u16 = 3;
const MACINTOSH_ROMAN: u16 = 0;
const MACINTOSH_JAPANESE: u16 = 1;
const MACINTOSH_ENGLISH: u16 = 0;
const MACINTOSH_FRENCH: u16 = 1;
const WINDOWS_UNICODE_BMP: u16 = 1;
const ENGLISH_US: u16 = 0x0409;
const FRENCH: u16 = 0x040C;
const FAMILY: u16 = 1;
const SUBFAMILY: u16 = 2;
const POST_SCRIPT_NAME: u16 = 6;
const TYPOGRAPHIC_FAMILY: u16 = 16;
const TRUETYPE: [u8; 4] = [0, 1, 0, 0];

fn be16(value: usize) -> [u8; 2] {
    u16::try_from(value).unwrap().to_be_bytes()
}

/// Returns an sfnt font with `magic` and `tables`.
fn font(magic: [u8; 4], tables: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    font_at(0, magic, tables)
}

/// Returns an sfnt font with `magic` and `tables` that sits `start` bytes
/// into its data, as a face of a collection does: table offsets count from
/// the start of the data.
fn font_at(start: usize, magic: [u8; 4], tables: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    // Table records are sorted by tag, which a face searches them by.
    let mut tables = tables.to_vec();
    tables.sort_by_key(|(tag, _)| **tag);
    let mut data = magic.to_vec();
    data.extend(be16(tables.len()));
    data.extend([0; 6]);
    let mut offset = tables
        .len()
        .checked_mul(16)
        .and_then(|records| records.checked_add(12))
        .and_then(|header| header.checked_add(start))
        .unwrap();
    for (tag, body) in &tables {
        data.extend(*tag);
        data.extend([0; 4]);
        data.extend(u32::try_from(offset).unwrap().to_be_bytes());
        data.extend(u32::try_from(body.len()).unwrap().to_be_bytes());
        offset = offset.checked_add(body.len()).unwrap();
    }
    for (_, body) in &tables {
        data.extend(body);
    }
    data
}

/// Returns a format 0 `name` table with one record per (platform, encoding,
/// language, name ID, text). Macintosh text is stored one byte per character,
/// the byte being the character's code point, so `\u{8e}` stores the byte
/// Macintosh Roman reads as `é`; the rest is stored as UTF-16BE.
fn name_table(names: &[NameRecord<'_>]) -> Vec<u8> {
    let mut records = Vec::new();
    let mut storage = Vec::new();
    for &(platform, encoding, language, name_id, text) in names {
        let bytes: Vec<u8> = if platform == MACINTOSH {
            text.chars()
                .map(|c| u8::try_from(u32::from(c)).unwrap())
                .collect()
        } else {
            text.encode_utf16().flat_map(u16::to_be_bytes).collect()
        };
        for field in [platform, encoding, language, name_id] {
            records.extend(field.to_be_bytes());
        }
        records.extend(be16(bytes.len()));
        records.extend(be16(storage.len()));
        storage.extend(bytes);
    }
    let mut table = vec![0, 0];
    table.extend(be16(names.len()));
    table.extend(be16(records.len().checked_add(6).unwrap()));
    table.extend(records);
    table.extend(storage);
    table
}

/// One record of a `name` table: platform, encoding, language, name ID
/// and text.
type NameRecord<'a> = (u16, u16, u16, u16, &'a str);

/// A record in English (United States), stored as Windows stores Unicode.
const fn english(name_id: u16, text: &str) -> NameRecord<'_> {
    (WINDOWS, WINDOWS_UNICODE_BMP, ENGLISH_US, name_id, text)
}

/// A record in French, stored as Windows stores Unicode.
const fn french(name_id: u16, text: &str) -> NameRecord<'_> {
    (WINDOWS, WINDOWS_UNICODE_BMP, FRENCH, name_id, text)
}

/// A record in Macintosh Roman, in the Macintosh `language`.
const fn macintosh(language: u16, name_id: u16, text: &str) -> NameRecord<'_> {
    (MACINTOSH, MACINTOSH_ROMAN, language, name_id, text)
}

/// Reads the one face of a TrueType font whose only table is a `name` table
/// of `names`.
fn parse_names(names: &[NameRecord<'_>]) -> Result<FaceProperties, FontLoadFailureReason> {
    parse(&font(TRUETYPE, &[(b"name", name_table(names))]))
}

fn english_name(family: &str) -> Vec<u8> {
    name_table(&[english(FAMILY, family)])
}

/// Returns an `OS/2` table of `len` bytes declaring `version`, the weight
/// and width classes, and `fsSelection`.
fn os2_table(version: u16, len: usize, weight: u16, width: u16, selection: u16) -> Vec<u8> {
    let mut table = vec![0; len];
    table[0..2].copy_from_slice(&version.to_be_bytes());
    table[4..6].copy_from_slice(&weight.to_be_bytes());
    table[6..8].copy_from_slice(&width.to_be_bytes());
    table[62..64].copy_from_slice(&selection.to_be_bytes());
    table
}

/// Returns a `post` table with `italicAngle` and `isFixedPitch`.
fn post_table(italic_angle: u32, fixed_pitch: u32) -> Vec<u8> {
    let mut table = vec![0; 32];
    table[0..4].copy_from_slice(&0x0003_0000_u32.to_be_bytes());
    table[4..8].copy_from_slice(&italic_angle.to_be_bytes());
    table[12..16].copy_from_slice(&fixed_pitch.to_be_bytes());
    table
}

/// Returns the face of a TrueType font holding `table` under `tag` and an
/// English family name.
fn named_face_with(tag: [u8; 4], table: Vec<u8>) -> FaceProperties {
    parse(&font(
        TRUETYPE,
        &[(&tag, table), (b"name", english_name("Font"))],
    ))
    .unwrap()
}

fn parse(data: &[u8]) -> Result<FaceProperties, FontLoadFailureReason> {
    read(data, 0)
}

#[test]
fn the_english_us_family_comes_first() {
    let face = parse_names(&[french(FAMILY, "Police"), english(FAMILY, "Font")]).unwrap();
    assert_eq!(
        face.families,
        [
            ("Font".to_string(), Language::English_UnitedStates),
            ("Police".to_string(), Language::French_France),
        ]
    );
}

#[test]
fn the_typographic_family_replaces_the_family() {
    let face = parse_names(&[
        english(FAMILY, "Font Light"),
        english(SUBFAMILY, "Regular"),
        english(TYPOGRAPHIC_FAMILY, "Font"),
    ])
    .unwrap();
    assert_eq!(
        face.families,
        [("Font".to_string(), Language::English_UnitedStates)]
    );
}

/// The Macintosh Roman family stands in for the English one no Unicode
/// record gives. Only the first such record is read.
#[test]
fn a_macintosh_roman_family_is_read_when_no_unicode_family_is_english() {
    let face = parse_names(&[
        french(FAMILY, "Police"),
        macintosh(MACINTOSH_ENGLISH, SUBFAMILY, "Regular"),
        macintosh(MACINTOSH_ENGLISH, FAMILY, "\u{8e}t\u{8e}"),
        macintosh(MACINTOSH_FRENCH, FAMILY, "Autre"),
    ])
    .unwrap();
    assert_eq!(
        face.families,
        [
            ("été".to_string(), Language::English_UnitedStates),
            ("Police".to_string(), Language::French_France),
        ]
    );
}

#[test]
fn a_macintosh_roman_family_is_left_out_beside_an_english_unicode_one() {
    let face = parse_names(&[
        english(FAMILY, "Font"),
        macintosh(MACINTOSH_ENGLISH, FAMILY, "Mac Font"),
    ])
    .unwrap();
    assert_eq!(
        face.families,
        [("Font".to_string(), Language::English_UnitedStates)]
    );
}

/// Of the single-byte encodings only Macintosh Roman is decoded.
#[test]
fn a_family_in_an_encoding_that_is_not_decoded_is_no_family() {
    for (platform, encoding) in [(MACINTOSH, MACINTOSH_JAPANESE), (ISO, 0)] {
        let error = parse_names(&[(platform, encoding, 0, FAMILY, "Font")]).unwrap_err();
        assert!(
            matches!(error, FontLoadFailureReason::NoFamilyName),
            "platform {platform}, encoding {encoding}: {error}"
        );
    }
}

/// The first `PostScript` name in an encoding that is decoded is the face's,
/// whichever records come before it.
#[test]
fn the_postscript_name_is_the_first_one_that_can_be_decoded() {
    let face = parse_names(&[
        english(FAMILY, "Font"),
        (MACINTOSH, MACINTOSH_JAPANESE, 0, POST_SCRIPT_NAME, "Nope"),
        macintosh(MACINTOSH_ENGLISH, POST_SCRIPT_NAME, "Font-Mac"),
        english(POST_SCRIPT_NAME, "Font-Windows"),
    ])
    .unwrap();
    assert_eq!(face.post_script_name, "Font-Mac");
}

#[test]
fn a_face_without_a_postscript_name_gets_one_from_its_family() {
    let face = parse_names(&[english(FAMILY, "Two Words")]).unwrap();
    assert_eq!(face.post_script_name, "Two-Words");
}

#[test]
fn a_face_with_only_a_name_table_has_the_default_properties() {
    let face = parse_names(&[english(FAMILY, "Font")]).unwrap();
    assert_eq!(
        (face.style, face.weight, face.stretch, face.monospaced),
        (Style::Normal, Weight::NORMAL, Stretch::Normal, false)
    );
}

#[test]
fn a_unicode_platform_family_loads() {
    let names = name_table(&[(UNICODE, 3, 0, FAMILY, "Unicode Font")]);
    let face = parse(&font(TRUETYPE, &[(b"name", names)])).unwrap();
    assert_eq!(face.families[0].0, "Unicode Font");
}

#[test]
fn an_opentype_font_loads() {
    let face = parse(&font(*b"OTTO", &[(b"name", english_name("Font"))])).unwrap();
    assert_eq!(face.families[0].0, "Font");
}

#[test]
fn an_unknown_magic_is_malformed() {
    let error = parse(&font(*b"abcd", &[(b"name", english_name("Font"))])).unwrap_err();
    assert!(
        matches!(error, FontLoadFailureReason::MalformedFont),
        "{error}"
    );
}

#[test]
fn a_font_without_a_family_is_unnamed() {
    let error = parse(&font(TRUETYPE, &[(b"name", name_table(&[]))])).unwrap_err();
    assert!(
        matches!(error, FontLoadFailureReason::NoFamilyName),
        "{error}"
    );
}

#[test]
fn load_errors_read_as_sentences() {
    assert_eq!(
        FontLoadFailureReason::MalformedFont.to_string(),
        "malformed font"
    );
    assert_eq!(
        FontLoadFailureReason::NoFamilyName.to_string(),
        "font doesn't have a family name"
    );
}

/// Returns a collection of `faces`, each built by a function given where
/// the face starts in the data.
fn collection(faces: &[&dyn Fn(usize) -> Vec<u8>]) -> Vec<u8> {
    let mut data = b"ttcf".to_vec();
    data.extend(0x0001_0000_u32.to_be_bytes());
    data.extend(u32::try_from(faces.len()).unwrap().to_be_bytes());
    // The header so far, then one offset per face.
    let mut start = faces
        .len()
        .checked_mul(4)
        .and_then(|offsets| offsets.checked_add(12))
        .unwrap();
    let mut bodies = Vec::new();
    for face in faces {
        data.extend(u32::try_from(start).unwrap().to_be_bytes());
        let body = face(start);
        start = start.checked_add(body.len()).unwrap();
        bodies.extend(body);
    }
    data.extend(bodies);
    data
}

/// A collection whose first face has no family name and whose third does not
/// parse loads the second, and reports the other two by index and reason.
#[test]
fn a_collection_loads_the_faces_that_parse_and_reports_the_others() {
    let data = collection(&[
        &|start| font_at(start, TRUETYPE, &[(b"name", name_table(&[]))]),
        &|start| font_at(start, TRUETYPE, &[(b"name", english_name("Font"))]),
        &|start| font_at(start, *b"abcd", &[]),
    ]);
    let mut database = Database::new();
    let report = database.load_font_data(data);

    let added: Vec<_> = report.added().collect();
    assert_eq!(added.len(), 1);
    let face = database.face(added[0]).unwrap();
    assert_eq!((face.index, face.families[0].0.as_str()), (1, "Font"));
    assert_eq!(database.len(), 1);
    assert_eq!(report.skipped(), 0);

    let failures: Vec<_> = report
        .failures()
        .iter()
        .map(|failure| (failure.face_index, failure.to_string()))
        .collect();
    assert_eq!(
        failures,
        [
            (
                Some(0),
                "cannot load face 0: font doesn't have a family name".to_string()
            ),
            (Some(2), "cannot load face 2: malformed font".to_string()),
        ]
    );
    assert!(matches!(
        report.failures()[0].reason,
        FontLoadFailureReason::NoFamilyName
    ));
    assert!(matches!(
        report.into_failures()[1].reason,
        FontLoadFailureReason::MalformedFont
    ));
}

/// A collection that declares no face adds nothing and has nothing to report.
#[test]
fn a_collection_without_faces_reports_no_failure() {
    let mut database = Database::new();
    let report = database.load_font_data(collection(&[]));

    assert_eq!(report.added().len(), 0);
    assert!(report.failures().is_empty());
    assert!(database.is_empty());
}

#[test]
fn os2_declares_weight_width_and_style_for_each_version() {
    // (version, table length, weight, width, fsSelection, style, stretch)
    let cases = [
        (0, 78, 300, 1, 0, Style::Normal, Stretch::UltraCondensed),
        (1, 86, 700, 4, 1, Style::Italic, Stretch::SemiCondensed),
        (2, 96, 400, 6, 0, Style::Normal, Stretch::SemiExpanded),
        (3, 96, 400, 2, 0, Style::Normal, Stretch::ExtraCondensed),
        (
            4,
            96,
            400,
            9,
            1 << 9,
            Style::Oblique,
            Stretch::UltraExpanded,
        ),
        (5, 100, 900, 7, 0, Style::Normal, Stretch::Expanded),
    ];
    for (version, len, weight, width, selection, style, stretch) in cases {
        let os2 = os2_table(version, len, weight, width, selection);
        let face = named_face_with(*b"OS/2", os2);
        assert_eq!(
            (face.weight, face.stretch, face.style),
            (Weight(weight), stretch, style),
            "version {version}"
        );
    }
}

#[test]
fn an_oblique_flag_before_version_4_is_ignored() {
    let os2 = os2_table(3, 96, 400, 5, 1 << 9);
    let face = named_face_with(*b"OS/2", os2);
    assert_eq!(face.style, Style::Normal);
}

#[test]
fn an_unusable_os2_table_leaves_the_defaults() {
    for os2 in [os2_table(6, 100, 700, 3, 1), os2_table(1, 85, 700, 3, 1)] {
        let face = named_face_with(*b"OS/2", os2);
        assert_eq!(
            (face.weight, face.stretch, face.style),
            (Weight::NORMAL, Stretch::Normal, Style::Normal)
        );
    }
}

#[test]
fn post_declares_italic_and_monospaced() {
    let cases = [
        (0, 0, false, Style::Normal),
        (0, 1, true, Style::Normal),
        (0xFFF4_0000, 0, false, Style::Italic),
    ];
    for (italic_angle, fixed_pitch, monospaced, style) in cases {
        let post = post_table(italic_angle, fixed_pitch);
        let face = named_face_with(*b"post", post);
        assert_eq!((face.monospaced, face.style), (monospaced, style));
    }
}
