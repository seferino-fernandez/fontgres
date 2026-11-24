//! Reading a face's properties from its font tables, with fonts built here
//! byte by byte.

use alloc::{string::ToString, sync::Arc, vec, vec::Vec};

use super::{
    Database, FontLoadFailureReason, Language, Source, Stretch, Style, Weight, parse_face_info,
};

const UNICODE: u16 = 0;
const MACINTOSH: u16 = 1;
const WINDOWS: u16 = 3;
const WINDOWS_UNICODE_BMP: u16 = 1;
const ENGLISH_US: u16 = 0x0409;
const FRENCH: u16 = 0x040C;
const FAMILY: u16 = 1;
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
/// language, name ID, text); Macintosh text is stored as single bytes, the
/// rest as UTF-16BE.
fn name_table(names: &[(u16, u16, u16, u16, &str)]) -> Vec<u8> {
    let mut records = Vec::new();
    let mut storage = Vec::new();
    for &(platform, encoding, language, name_id, text) in names {
        let bytes: Vec<u8> = if platform == MACINTOSH {
            text.bytes().collect()
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

fn english_name(family: &str) -> Vec<u8> {
    name_table(&[(WINDOWS, WINDOWS_UNICODE_BMP, ENGLISH_US, FAMILY, family)])
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
fn named_face_with(tag: [u8; 4], table: Vec<u8>) -> super::FaceInfo {
    parse(font(
        TRUETYPE,
        &[(&tag, table), (b"name", english_name("Font"))],
    ))
    .unwrap()
}

fn parse(data: Vec<u8>) -> Result<super::FaceInfo, FontLoadFailureReason> {
    let data = Arc::new(data);
    let source = Source::Binary(Arc::<Vec<u8>>::clone(&data));
    parse_face_info(source, &data, 0)
}

#[test]
fn the_english_us_family_comes_first() {
    let names = name_table(&[
        (WINDOWS, WINDOWS_UNICODE_BMP, FRENCH, FAMILY, "Police"),
        (WINDOWS, WINDOWS_UNICODE_BMP, ENGLISH_US, FAMILY, "Font"),
    ]);
    let face = parse(font(TRUETYPE, &[(b"name", names)])).unwrap();
    assert_eq!(
        face.families,
        [
            ("Font".to_string(), Language::English_UnitedStates),
            ("Police".to_string(), Language::French_France),
        ]
    );
}

#[test]
fn a_unicode_platform_family_loads() {
    let names = name_table(&[(UNICODE, 3, 0, FAMILY, "Unicode Font")]);
    let face = parse(font(TRUETYPE, &[(b"name", names)])).unwrap();
    assert_eq!(face.families[0].0, "Unicode Font");
}

#[test]
fn an_opentype_font_loads() {
    let face = parse(font(*b"OTTO", &[(b"name", english_name("Font"))])).unwrap();
    assert_eq!(face.families[0].0, "Font");
}

#[test]
fn an_unknown_magic_is_malformed() {
    let error = parse(font(*b"abcd", &[(b"name", english_name("Font"))])).unwrap_err();
    assert!(
        matches!(error, FontLoadFailureReason::MalformedFont),
        "{error}"
    );
}

#[test]
fn a_font_without_a_family_is_unnamed() {
    let error = parse(font(TRUETYPE, &[(b"name", name_table(&[]))])).unwrap_err();
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
