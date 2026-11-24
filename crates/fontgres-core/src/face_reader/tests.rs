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
const TYPOGRAPHIC_SUBFAMILY: u16 = 17;
/// The `macStyle` flags of a `head` table.
const MAC_BOLD: u16 = 1;
const MAC_ITALIC: u16 = 1 << 1;
/// The `fsSelection` flags of an `OS/2` table.
const SELECTION_ITALIC: u16 = 1;
const SELECTION_OBLIQUE: u16 = 1 << 9;
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

/// Returns a version 0 `name` table with one record per (platform,
/// encoding, language, name ID, text). Macintosh text is stored one byte per
/// character, the byte being the character's code point, so `\u{8e}` stores
/// the byte Macintosh Roman reads as `é`; the rest is stored as UTF-16BE.
fn name_table(names: &[NameRecord<'_>]) -> Vec<u8> {
    versioned_name_table(0, names, &[])
}

/// Returns a `name` table that declares `version`, laid out as version 1
/// is: the name records, then the count of `language_tags` and a record for
/// each, then the storage. Without language tags that is the layout of
/// version 0.
fn versioned_name_table(version: u16, names: &[NameRecord<'_>], language_tags: &[&str]) -> Vec<u8> {
    let utf16 =
        |text: &str| -> Vec<u8> { text.encode_utf16().flat_map(u16::to_be_bytes).collect() };
    let mut records = Vec::new();
    let mut storage = Vec::new();
    for &(platform, encoding, language, name_id, text) in names {
        let bytes: Vec<u8> = if platform == MACINTOSH {
            text.chars()
                .map(|c| u8::try_from(u32::from(c)).unwrap())
                .collect()
        } else {
            utf16(text)
        };
        for field in [platform, encoding, language, name_id] {
            records.extend(field.to_be_bytes());
        }
        records.extend(be16(bytes.len()));
        records.extend(be16(storage.len()));
        storage.extend(bytes);
    }
    if !language_tags.is_empty() {
        records.extend(be16(language_tags.len()));
    }
    for tag in language_tags {
        let bytes = utf16(tag);
        records.extend(be16(bytes.len()));
        records.extend(be16(storage.len()));
        storage.extend(bytes);
    }
    let mut table = version.to_be_bytes().to_vec();
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

/// Returns a version 4 `OS/2` table declaring the weight class and
/// `fsSelection`, at normal width.
fn os2_weighing(weight: u16, selection: u16) -> Vec<u8> {
    os2_table(4, 96, weight, 5, selection)
}

/// Returns a `head` table with the `macStyle` flags.
fn head_table(mac_style: u16) -> Vec<u8> {
    let mut table = vec![0; 54];
    table[44..46].copy_from_slice(&mac_style.to_be_bytes());
    table
}

/// Reads a face of the family Font with the English `subfamily` name and
/// `tables`.
fn styled_face(subfamily: &str, tables: &[(&[u8; 4], Vec<u8>)]) -> FaceProperties {
    let names = name_table(&[english(FAMILY, "Font"), english(SUBFAMILY, subfamily)]);
    let mut tables = tables.to_vec();
    tables.push((b"name", names));
    parse(&font(TRUETYPE, &tables)).unwrap()
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

/// A version 1 table puts its language tags between the name records and
/// the storage.
#[test]
fn a_version_1_name_table_is_read() {
    let names = versioned_name_table(
        1,
        &[
            english(FAMILY, "Font"),
            english(POST_SCRIPT_NAME, "Font-One"),
        ],
        &["en-Latn", "fr"],
    );
    let face = parse(&font(TRUETYPE, &[(b"name", names)])).unwrap();
    assert_eq!(
        (face.families[0].0.as_str(), face.post_script_name.as_str()),
        ("Font", "Font-One")
    );
}

#[test]
fn a_name_table_of_a_version_the_format_does_not_define_gives_no_name() {
    let names = versioned_name_table(2, &[english(FAMILY, "Font")], &[]);
    let error = parse(&font(TRUETYPE, &[(b"name", names)])).unwrap_err();
    assert!(
        matches!(error, FontLoadFailureReason::NoFamilyName),
        "{error}"
    );
}

/// A record for a platform the format does not define is left out, and the
/// records after it are still read.
#[test]
fn a_name_record_of_an_unknown_platform_is_skipped() {
    let face = parse_names(&[
        (9, 0, 0, FAMILY, "Lost"),
        english(FAMILY, "Font"),
        (9, 0, 0, POST_SCRIPT_NAME, "Lost"),
        english(POST_SCRIPT_NAME, "Font-Regular"),
    ])
    .unwrap();
    assert_eq!(
        (face.families.len(), face.families[0].0.as_str()),
        (1, "Font")
    );
    assert_eq!(face.post_script_name, "Font-Regular");
}

/// A record whose string runs past the end of the table is left out, and
/// the records after it are still read.
#[test]
fn a_name_record_with_a_string_outside_the_table_is_skipped() {
    let mut names = name_table(&[english(FAMILY, "Lost"), english(FAMILY, "Font")]);
    // The first record starts after the 6-byte header; its string length is
    // the fifth of its six 16-bit fields.
    names[14..16].copy_from_slice(&u16::MAX.to_be_bytes());
    let face = parse(&font(TRUETYPE, &[(b"name", names)])).unwrap();
    assert_eq!(
        (face.families.len(), face.families[0].0.as_str()),
        (1, "Font")
    );
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

/// A `post` table cut short declares only the fields it still holds
/// whole: the italic angle ends at byte 8 and the fixed-pitch flag at 16.
#[test]
fn a_post_table_too_short_for_a_field_does_not_declare_it() {
    let cases = [
        (0, false, Style::Normal),
        (7, false, Style::Normal),
        (8, false, Style::Italic),
        (15, false, Style::Italic),
        (16, true, Style::Italic),
    ];
    for (len, monospaced, style) in cases {
        let mut post = post_table(0xFFF4_0000, 1);
        post.truncate(len);
        let face = named_face_with(*b"post", post);
        assert_eq!(
            (face.monospaced, face.style),
            (monospaced, style),
            "a table of {len} bytes"
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

/// The format allows weight classes from 1 to 1000 and they are kept, the
/// ones below 100 included. 0 declares no weight and anything above 1000 is
/// the heaviest.
#[test]
fn a_weight_class_outside_1_to_1000_is_brought_into_it() {
    let cases = [
        (0, 400),
        (1, 1),
        (5, 5),
        (99, 99),
        (1000, 1000),
        (1001, 1000),
        (u16::MAX, 1000),
    ];
    for (declared, weight) in cases {
        let face = styled_face("Regular", &[(b"OS/2", os2_weighing(declared, 0))]);
        assert_eq!(face.weight, Weight(weight), "weight class {declared}");
    }
}

#[test]
fn a_face_without_os2_takes_bold_and_italic_from_mac_style() {
    // Bit 2 flags an underline, which is neither.
    let cases = [
        (0, 400, Style::Normal),
        (MAC_BOLD, 700, Style::Normal),
        (MAC_ITALIC, 400, Style::Italic),
        (MAC_BOLD | MAC_ITALIC, 700, Style::Italic),
        (1 << 2, 400, Style::Normal),
    ];
    for (mac_style, weight, style) in cases {
        let face = styled_face("Regular", &[(b"head", head_table(mac_style))]);
        assert_eq!(
            (face.weight, face.style),
            (Weight(weight), style),
            "macStyle {mac_style}"
        );
    }
}

#[test]
fn mac_style_is_not_read_when_os2_declares_the_face() {
    let face = styled_face(
        "Medium",
        &[
            (b"OS/2", os2_weighing(400, 0)),
            (b"head", head_table(MAC_BOLD | MAC_ITALIC)),
        ],
    );
    assert_eq!((face.weight, face.style), (Weight::NORMAL, Style::Normal));
}

/// An `OS/2` table that cannot be used declares nothing, so `head` speaks
/// for the face as it does when there is no `OS/2` table.
#[test]
fn mac_style_is_read_when_os2_cannot_be_used() {
    let face = styled_face(
        "Regular",
        &[
            (b"OS/2", os2_table(6, 100, 300, 5, 0)),
            (b"head", head_table(MAC_BOLD | MAC_ITALIC)),
        ],
    );
    assert_eq!((face.weight, face.style), (Weight::BOLD, Style::Italic));
}

#[test]
fn a_head_table_too_short_for_mac_style_flags_nothing() {
    let mut head = head_table(MAC_BOLD | MAC_ITALIC);
    head.truncate(45);
    let face = styled_face("Regular", &[(b"head", head)]);
    assert_eq!((face.weight, face.style), (Weight::NORMAL, Style::Normal));
}

/// The word Bold makes a face bold only when the tables leave it at 400,
/// and only as a whole word.
#[test]
fn the_word_bold_in_the_subfamily_makes_a_normal_weight_bold() {
    // (declared weight class, subfamily, weight)
    let cases = [
        (400, "Bold", 700),
        (400, "bold italic", 700),
        (400, "Semi-Bold", 700),
        (0, "Bold", 700),
        (400, "Semibold", 400),
        (400, "Bolder", 400),
        (400, "Regular", 400),
        (400, "", 400),
        (500, "Bold", 500),
        (300, "Bold", 300),
    ];
    for (declared, subfamily, weight) in cases {
        let face = styled_face(subfamily, &[(b"OS/2", os2_weighing(declared, 0))]);
        assert_eq!(
            face.weight,
            Weight(weight),
            "weight class {declared}, subfamily {subfamily:?}"
        );
    }
}

#[test]
fn the_word_bold_makes_a_face_without_os2_bold() {
    assert_eq!(styled_face("Bold", &[]).weight, Weight::BOLD);
    assert_eq!(styled_face("Regular", &[]).weight, Weight::NORMAL);
}

/// The words Italic and Oblique give a face its style only when the tables
/// leave it normal.
#[test]
fn the_words_italic_and_oblique_in_the_subfamily_give_a_normal_face_its_style() {
    // (fsSelection, subfamily, style)
    let cases = [
        (0, "Italic", Style::Italic),
        (0, "Thin ITALIC", Style::Italic),
        (0, "Oblique", Style::Oblique),
        (0, "Bold Oblique", Style::Oblique),
        (0, "Italic Oblique", Style::Italic),
        (0, "Italics", Style::Normal),
        (0, "Regular", Style::Normal),
        (SELECTION_OBLIQUE, "Italic", Style::Oblique),
        (SELECTION_ITALIC, "Oblique", Style::Italic),
    ];
    for (selection, subfamily, style) in cases {
        let face = styled_face(subfamily, &[(b"OS/2", os2_weighing(400, selection))]);
        assert_eq!(
            face.style, style,
            "fsSelection {selection}, subfamily {subfamily:?}"
        );
    }
}

/// Of two English subfamily names the Unicode one is read, whichever comes
/// first in the table.
#[test]
fn the_words_are_read_from_a_unicode_subfamily_ahead_of_a_macintosh_roman_one() {
    let face = parse_names(&[
        english(FAMILY, "Font"),
        macintosh(MACINTOSH_ENGLISH, SUBFAMILY, "Bold"),
        english(SUBFAMILY, "Regular"),
    ])
    .unwrap();
    assert_eq!(face.weight, Weight::NORMAL);
}

#[test]
fn the_words_are_read_from_a_macintosh_roman_subfamily_when_it_is_the_english_one() {
    let face = parse_names(&[
        english(FAMILY, "Font"),
        french(SUBFAMILY, "Gras"),
        macintosh(MACINTOSH_ENGLISH, SUBFAMILY, "Bold"),
    ])
    .unwrap();
    assert_eq!(face.weight, Weight::BOLD);
}

#[test]
fn the_words_are_not_read_from_a_subfamily_in_an_encoding_that_is_not_decoded() {
    let face = parse_names(&[
        english(FAMILY, "Font"),
        (MACINTOSH, MACINTOSH_JAPANESE, 0, SUBFAMILY, "Bold"),
    ])
    .unwrap();
    assert_eq!(face.weight, Weight::NORMAL);
}

/// An italic angle in `post` is read before the subfamily's words.
#[test]
fn an_italic_angle_makes_a_face_named_oblique_italic() {
    let face = styled_face("Oblique", &[(b"post", post_table(0xFFF4_0000, 0))]);
    assert_eq!(face.style, Style::Italic);
}

/// The words are looked for in the typographic subfamily when the font has
/// one, in its English name.
#[test]
fn the_words_are_read_from_the_typographic_subfamily() {
    let face = parse_names(&[
        english(FAMILY, "Font"),
        english(SUBFAMILY, "Bold Italic"),
        french(TYPOGRAPHIC_SUBFAMILY, "Bold"),
        english(TYPOGRAPHIC_SUBFAMILY, "Light Oblique"),
    ])
    .unwrap();
    assert_eq!((face.weight, face.style), (Weight::NORMAL, Style::Oblique));
}

#[test]
fn the_words_are_read_from_the_english_subfamily() {
    let face = parse_names(&[
        english(FAMILY, "Font"),
        french(SUBFAMILY, "Gras"),
        english(SUBFAMILY, "Bold"),
    ])
    .unwrap();
    assert_eq!(face.weight, Weight::BOLD);
}
