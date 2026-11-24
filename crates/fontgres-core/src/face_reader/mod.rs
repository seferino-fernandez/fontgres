//! Reads a face's properties from the bytes of a font.
//!
//! [`read`] is the one way from font data to what a database knows about a
//! face: its names, style, weight, stretch and whether it is monospaced. The
//! table parsing in `ttf_parser` and the name decoding in `names` are
//! private to this module.

mod names;
#[cfg(test)]
mod tests;
mod ttf_parser;

#[cfg(not(feature = "std"))]
use alloc::{string::String, vec::Vec};

pub use ttf_parser::{Language, Width as Stretch};

use crate::{FontLoadFailureReason, Style, Weight};
use ttf_parser::{MacStyle, RawFace, Tag, os2};

/// What a font's tables say about one face.
#[derive(Debug)]
pub(crate) struct FaceProperties {
    /// The face's family names with their languages, English (United States)
    /// first when the font has one. Never empty.
    pub(crate) families: Vec<(String, Language)>,
    /// The face's `PostScript` name, made from the first family when the
    /// font has none.
    pub(crate) post_script_name: String,
    pub(crate) style: Style,
    pub(crate) weight: Weight,
    pub(crate) stretch: Stretch,
    pub(crate) monospaced: bool,
}

/// Returns how many faces the font in `data` holds: the number a collection
/// declares, and one for any other data.
pub(crate) fn face_count(data: &[u8]) -> u32 {
    ttf_parser::fonts_in_collection(data).unwrap_or(1)
}

/// Reads the properties of the face at `index` in the font in `data`.
///
/// A font that is not a collection has one face, at index 0.
///
/// The `OS/2` table declares the weight, style and stretch. A face without
/// one that can be used takes bold and italic from the `macStyle` flags of
/// its `head` table. A face those leave at normal weight or normal style
/// takes them from the words Bold, Italic and Oblique in its subfamily name.
///
/// # Errors
///
/// [`FontLoadFailureReason::MalformedFont`] when the data is not a font or
/// holds no face at `index`, and [`FontLoadFailureReason::NoFamilyName`]
/// when the face has no family name that can be read.
pub(crate) fn read(data: &[u8], index: u32) -> Result<FaceProperties, FontLoadFailureReason> {
    const OS2_TAG: Tag = Tag::from_bytes(*b"OS/2");
    const HEAD_TAG: Tag = Tag::from_bytes(*b"head");

    let raw_face =
        RawFace::parse(data, index).map_err(|_cause| FontLoadFailureReason::MalformedFont)?;
    let names = names::read(&raw_face).ok_or(FontLoadFailureReason::NoFamilyName)?;
    let os2 = raw_face.table(OS2_TAG).and_then(os2::Table::parse);
    let (monospaced, slanted) = parse_post(&raw_face);
    // `head` speaks only for a face `OS/2` says nothing about.
    let mac_style = match os2 {
        Some(_) => MacStyle::default(),
        None => raw_face
            .table(HEAD_TAG)
            .and_then(MacStyle::parse)
            .unwrap_or_default(),
    };

    Ok(FaceProperties {
        style: style(os2, slanted, mac_style, &names.subfamily),
        weight: weight(os2, mac_style, &names.subfamily),
        stretch: os2.map_or(Stretch::Normal, |table| table.width()),
        monospaced,
        families: names.families,
        post_script_name: names.post_script_name,
    })
}

/// Returns the face's weight: what `OS/2` declares, else bold when
/// `mac_style` flags it. A face that leaves at normal weight is bold when
/// its `subfamily` name has the word Bold.
fn weight(os2: Option<os2::Table<'_>>, mac_style: MacStyle, subfamily: &str) -> Weight {
    let declared = match os2 {
        Some(table) => declared_weight(table.weight_class()),
        None if mac_style.bold() => Weight::BOLD,
        None => Weight::NORMAL,
    };
    if declared == Weight::NORMAL && has_word(subfamily, "bold") {
        Weight::BOLD
    } else {
        declared
    }
}

/// Returns the weight a `usWeightClass` of `class` stands for.
///
/// The format allows 1 to 1000. A class of 0 declares no weight, so the face
/// is of normal weight, and a class above 1000 is the heaviest weight.
fn declared_weight(class: u16) -> Weight {
    const HEAVIEST: u16 = 1000;

    if class == 0 {
        Weight::NORMAL
    } else {
        Weight(class.min(HEAVIEST))
    }
}

/// Returns the face's style: what `OS/2` declares, else italic when `post`
/// gives an italic angle (`slanted`) or `mac_style` flags it. A face those
/// leave normal is italic or oblique when its `subfamily` name has that word.
fn style(
    os2: Option<os2::Table<'_>>,
    slanted: bool,
    mac_style: MacStyle,
    subfamily: &str,
) -> Style {
    let declared = os2.map_or(Style::Normal, |table| table.style());
    if declared != Style::Normal {
        declared
    } else if slanted || mac_style.italic() || has_word(subfamily, "italic") {
        Style::Italic
    } else if has_word(subfamily, "oblique") {
        Style::Oblique
    } else {
        Style::Normal
    }
}

/// Returns whether `word` is one of the words of `name`, ignoring ASCII
/// case. A word is a run of letters and digits, so `Semibold` does not have
/// the word `bold` and `Semi Bold` does.
fn has_word(name: &str, word: &str) -> bool {
    name.split(|c: char| !c.is_alphanumeric())
        .any(|part| part.eq_ignore_ascii_case(word))
}

/// Returns whether the `post` table declares the face monospaced, and whether
/// it gives an italic angle.
///
/// A face without the table, or with one too short to hold a field, does not
/// declare that field.
fn parse_post(raw_face: &RawFace<'_>) -> (bool, bool) {
    const POST_TAG: Tag = Tag::from_bytes(*b"post");
    /// `italicAngle`, a 16.16 fixed-point number.
    const ITALIC_ANGLE: core::ops::Range<usize> = 4..8;
    /// `isFixedPitch`, a 32-bit number.
    const IS_FIXED_PITCH: core::ops::Range<usize> = 12..16;

    let Some(data) = raw_face.table(POST_TAG) else {
        return (false, false);
    };
    let is_set = |field: core::ops::Range<usize>| {
        data.get(field)
            .is_some_and(|bytes| bytes.iter().any(|&byte| byte != 0))
    };

    (is_set(IS_FIXED_PITCH), is_set(ITALIC_ANGLE))
}
