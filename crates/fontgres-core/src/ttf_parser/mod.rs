/*!
A minimal subset of [ttf-parser](https://github.com/harfbuzz/ttf-parser),
inlined from ttf-parser 0.25.1 (MIT / Apache-2.0).

Contains just enough of the original crate to parse font table records,
the `name` table and parts of the `OS/2` table.
*/

mod language;
pub mod name;
pub mod os2;
mod parser;

pub use language::Language;
pub use name::{PlatformId, name_id};
pub use os2::{Style, Width};
pub use parser::LazyArray16;

use parser::{FromData, NumFrom, Offset, Offset32, Stream};

/// A TrueType font magic.
///
/// https://docs.microsoft.com/en-us/typography/opentype/spec/otff#organization-of-an-opentype-font
#[derive(Clone, Copy, PartialEq, Debug)]
enum Magic {
    TrueType,
    OpenType,
    FontCollection,
}

impl FromData for Magic {
    const SIZE: usize = 4;

    #[inline]
    fn parse(data: &[u8]) -> Option<Self> {
        match u32::parse(data)? {
            0x00010000 | 0x74727565 => Some(Magic::TrueType),
            0x4F54544F => Some(Magic::OpenType),
            0x74746366 => Some(Magic::FontCollection),
            _ => None,
        }
    }
}

/// A 4-byte tag.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Tag(pub u32);

impl Tag {
    /// Creates a `Tag` from bytes.
    #[inline]
    pub const fn from_bytes(bytes: &[u8; 4]) -> Self {
        Tag(((bytes[0] as u32) << 24)
            | ((bytes[1] as u32) << 16)
            | ((bytes[2] as u32) << 8)
            | (bytes[3] as u32))
    }
}

impl FromData for Tag {
    const SIZE: usize = 4;

    #[inline]
    fn parse(data: &[u8]) -> Option<Self> {
        u32::parse(data).map(Tag)
    }
}

/// A raw table record.
#[derive(Clone, Copy)]
struct TableRecord {
    tag: Tag,
    offset: u32,
    length: u32,
}

impl FromData for TableRecord {
    const SIZE: usize = 16;

    #[inline]
    fn parse(data: &[u8]) -> Option<Self> {
        let mut s = Stream::new(data);
        let tag = s.read::<Tag>()?;
        s.skip::<u32>(); // checkSum
        Some(TableRecord {
            tag,
            offset: s.read::<u32>()?,
            length: s.read::<u32>()?,
        })
    }
}

/// A list of font face parsing errors.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FaceParsingError {
    /// An attempt to read out of bounds detected.
    ///
    /// Should occur only on malformed fonts.
    MalformedFont,

    /// Face data must start with `0x00010000`, `0x74727565`, `0x4F54544F` or `0x74746366`.
    UnknownMagic,

    /// The face index is larger than the number of faces in the font.
    FaceIndexOutOfBounds,
}

/// A raw font face.
///
/// Unlike a full font face, `RawFace` parses only face table records.
/// Meaning all you can get from this type is a raw (`&[u8]`) data of a requested table.
#[derive(Clone, Copy)]
pub struct RawFace<'a> {
    /// The input font file data.
    pub data: &'a [u8],
    /// An array of table records.
    table_records: LazyArray16<'a, TableRecord>,
}

impl<'a> RawFace<'a> {
    /// Creates a new [`RawFace`] from a raw data.
    ///
    /// `index` indicates the specific font face in a font collection.
    /// Use [`fonts_in_collection`] to get the total number of font faces.
    /// Set to 0 if unsure.
    pub fn parse(data: &'a [u8], index: u32) -> Result<Self, FaceParsingError> {
        // https://docs.microsoft.com/en-us/typography/opentype/spec/otff#organization-of-an-opentype-font

        let mut s = Stream::new(data);

        // Read **font** magic.
        let magic = s.read::<Magic>().ok_or(FaceParsingError::UnknownMagic)?;
        if magic == Magic::FontCollection {
            s.skip::<u32>(); // version
            let number_of_faces = s.read::<u32>().ok_or(FaceParsingError::MalformedFont)?;
            let offsets_len = usize::num_from(number_of_faces)
                .checked_mul(Offset32::SIZE)
                .ok_or(FaceParsingError::MalformedFont)?;
            let offsets = s
                .read_bytes(offsets_len)
                .ok_or(FaceParsingError::MalformedFont)?;
            if index >= number_of_faces {
                return Err(FaceParsingError::FaceIndexOutOfBounds);
            }
            // `index < number_of_faces`, so the offset lies within `offsets`.
            let face_offset =
                Stream::read_at::<Offset32>(offsets, usize::num_from(index) * Offset32::SIZE)
                    .ok_or(FaceParsingError::MalformedFont)?;
            // Face offset is from the start of the font data,
            // so we have to adjust it to the current parser offset.
            let face_offset = face_offset
                .to_usize()
                .checked_sub(s.offset())
                .ok_or(FaceParsingError::MalformedFont)?;
            s.advance_checked(face_offset)
                .ok_or(FaceParsingError::MalformedFont)?;

            // Read **face** magic.
            // Each face in a font collection also starts with a magic.
            let magic = s.read::<Magic>().ok_or(FaceParsingError::UnknownMagic)?;
            // And face in a font collection can't be another collection.
            if magic == Magic::FontCollection {
                return Err(FaceParsingError::UnknownMagic);
            }
        } else {
            // When reading from a regular font (not a collection) disallow index to be non-zero
            // Basically treat the font as a one-element collection
            if index != 0 {
                return Err(FaceParsingError::FaceIndexOutOfBounds);
            }
        }

        let num_tables = s.read::<u16>().ok_or(FaceParsingError::MalformedFont)?;
        s.advance(6); // searchRange (u16) + entrySelector (u16) + rangeShift (u16)
        let table_records = s
            .read_array16::<TableRecord>(num_tables)
            .ok_or(FaceParsingError::MalformedFont)?;

        Ok(RawFace {
            data,
            table_records,
        })
    }

    /// Returns the raw data of a selected table.
    pub fn table(&self, tag: Tag) -> Option<&'a [u8]> {
        let (_, table) = self
            .table_records
            .binary_search_by(|record| record.tag.cmp(&tag))?;
        let offset = usize::num_from(table.offset);
        let length = usize::num_from(table.length);
        let end = offset.checked_add(length)?;
        self.data.get(offset..end)
    }
}

impl core::fmt::Debug for RawFace<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "RawFace {{ ... }}")
    }
}

/// Returns the number of fonts stored in a TrueType font collection.
///
/// Returns `None` if a provided data is not a TrueType font collection.
#[inline]
pub fn fonts_in_collection(data: &[u8]) -> Option<u32> {
    let mut s = Stream::new(data);
    if s.read::<Magic>()? != Magic::FontCollection {
        return None;
    }

    s.skip::<u32>(); // version
    s.read::<u32>()
}

#[cfg(test)]
mod tests {
    use super::{FaceParsingError, RawFace};

    /// A face with no tables: the TrueType magic, `numTables = 0` and the
    /// three search fields.
    const EMPTY_FACE: [u8; 12] = [0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

    /// A `ttcf` header that declares `declared` faces, then `offsets`.
    fn collection(declared: u32, offsets: &[u32]) -> Vec<u8> {
        let mut data = b"ttcf".to_vec();
        data.extend_from_slice(&0x0001_0000_u32.to_be_bytes());
        data.extend_from_slice(&declared.to_be_bytes());
        for offset in offsets {
            data.extend_from_slice(&offset.to_be_bytes());
        }
        data
    }

    #[test]
    fn parses_each_face_of_a_collection() {
        // Header (12 bytes) and two offsets (8 bytes), then the two faces.
        let mut data = collection(2, &[20, 32]);
        data.extend_from_slice(&EMPTY_FACE);
        data.extend_from_slice(&EMPTY_FACE);
        RawFace::parse(&data, 0).unwrap();
        RawFace::parse(&data, 1).unwrap();
        assert_eq!(
            RawFace::parse(&data, 2).err(),
            Some(FaceParsingError::FaceIndexOutOfBounds)
        );
    }

    #[test]
    fn a_truncated_offset_table_is_malformed_whatever_the_index() {
        // Two faces declared, but the data ends after the first offset.
        let data = collection(2, &[16]);
        for index in [0, 5] {
            assert_eq!(
                RawFace::parse(&data, index).err(),
                Some(FaceParsingError::MalformedFont),
                "index {index}"
            );
        }
    }

    #[test]
    fn a_face_offset_inside_the_offset_table_is_malformed() {
        // The offset table ends at byte 20, so a face at 16 overlaps it.
        let mut data = collection(2, &[16, 20]);
        data.extend_from_slice(&EMPTY_FACE);
        assert_eq!(
            RawFace::parse(&data, 0).err(),
            Some(FaceParsingError::MalformedFont)
        );
        RawFace::parse(&data, 1).unwrap();
    }
}
