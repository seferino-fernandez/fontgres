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
use ttf_parser::{RawFace, Tag};

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
/// # Errors
///
/// [`FontLoadFailureReason::MalformedFont`] when the data is not a font or
/// holds no face at `index`, and [`FontLoadFailureReason::NoFamilyName`]
/// when the face has no family name that can be read.
pub(crate) fn read(data: &[u8], index: u32) -> Result<FaceProperties, FontLoadFailureReason> {
    let raw_face =
        RawFace::parse(data, index).map_err(|_cause| FontLoadFailureReason::MalformedFont)?;
    let (families, post_script_name) =
        names::read(&raw_face).ok_or(FontLoadFailureReason::NoFamilyName)?;
    let (mut style, weight, stretch) = parse_os2(&raw_face);
    let (monospaced, italic) = parse_post(&raw_face);

    if style == Style::Normal && italic {
        style = Style::Italic;
    }

    Ok(FaceProperties {
        families,
        post_script_name,
        style,
        weight,
        stretch,
        monospaced,
    })
}

fn parse_os2(raw_face: &RawFace<'_>) -> (Style, Weight, Stretch) {
    const OS2_TAG: Tag = Tag::from_bytes(*b"OS/2");
    let Some(table) = raw_face
        .table(OS2_TAG)
        .and_then(ttf_parser::os2::Table::parse)
    else {
        return (Style::Normal, Weight::NORMAL, Stretch::Normal);
    };

    let style = match table.style() {
        ttf_parser::Style::Normal => Style::Normal,
        ttf_parser::Style::Italic => Style::Italic,
        ttf_parser::Style::Oblique => Style::Oblique,
    };

    let weight = table.weight();
    let stretch = table.width();

    (style, Weight(weight.to_number()), stretch)
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
