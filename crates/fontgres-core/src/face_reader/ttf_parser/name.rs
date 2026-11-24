//! A [Naming Table](
//! https://docs.microsoft.com/en-us/typography/opentype/spec/name) implementation.

use super::Language;
use super::parser::{FromData, LazyArray16, Offset, Offset16, Stream};

/// A list of [name ID](https://docs.microsoft.com/en-us/typography/opentype/spec/name#name-ids)'s.
pub(crate) mod name_id {
    pub(crate) const FAMILY: u16 = 1;
    pub(crate) const POST_SCRIPT_NAME: u16 = 6;
    pub(crate) const TYPOGRAPHIC_FAMILY: u16 = 16;
}

/// A [platform ID](https://docs.microsoft.com/en-us/typography/opentype/spec/name#platform-ids).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum PlatformId {
    Unicode,
    Macintosh,
    Iso,
    Windows,
    Custom,
}

impl FromData for PlatformId {
    const SIZE: usize = 2;

    #[inline]
    fn parse(data: &[u8]) -> Option<Self> {
        match u16::parse(data)? {
            0 => Some(PlatformId::Unicode),
            1 => Some(PlatformId::Macintosh),
            2 => Some(PlatformId::Iso),
            3 => Some(PlatformId::Windows),
            4 => Some(PlatformId::Custom),
            _ => None,
        }
    }
}

#[inline]
fn is_unicode_encoding(platform_id: PlatformId, encoding_id: u16) -> bool {
    // https://docs.microsoft.com/en-us/typography/opentype/spec/name#windows-encoding-ids
    const WINDOWS_SYMBOL_ENCODING_ID: u16 = 0;
    const WINDOWS_UNICODE_BMP_ENCODING_ID: u16 = 1;

    match platform_id {
        PlatformId::Unicode => true,
        PlatformId::Windows => matches!(
            encoding_id,
            WINDOWS_SYMBOL_ENCODING_ID | WINDOWS_UNICODE_BMP_ENCODING_ID
        ),
        PlatformId::Macintosh | PlatformId::Iso | PlatformId::Custom => false,
    }
}

#[derive(Clone, Copy)]
struct NameRecord {
    platform_id: PlatformId,
    encoding_id: u16,
    language_id: u16,
    name_id: u16,
    length: u16,
    offset: Offset16,
}

impl FromData for NameRecord {
    const SIZE: usize = 12;

    #[inline]
    fn parse(data: &[u8]) -> Option<Self> {
        let mut s = Stream::new(data);
        Some(NameRecord {
            platform_id: s.read::<PlatformId>()?,
            encoding_id: s.read::<u16>()?,
            language_id: s.read::<u16>()?,
            name_id: s.read::<u16>()?,
            length: s.read::<u16>()?,
            offset: s.read::<Offset16>()?,
        })
    }
}

/// A [Name Record](https://docs.microsoft.com/en-us/typography/opentype/spec/name#name-records).
#[derive(Clone, Copy)]
pub(crate) struct Name<'a> {
    /// A platform ID.
    pub platform_id: PlatformId,
    /// A platform-specific encoding ID.
    pub encoding_id: u16,
    /// A language ID.
    pub language_id: u16,
    /// A [Name ID](https://docs.microsoft.com/en-us/typography/opentype/spec/name#name-ids).
    ///
    /// A predefined list of ID's can be found in the [`name_id`](name_id/index.html) module.
    pub id: u16,
    /// A raw name data.
    ///
    /// Can be in any encoding. Can be empty.
    pub data: &'a [u8],
}

impl Name<'_> {
    /// Checks that the current Name data has a Unicode encoding.
    #[inline]
    pub(crate) fn is_unicode(&self) -> bool {
        is_unicode_encoding(self.platform_id, self.encoding_id)
    }

    /// Returns a Name language.
    pub(crate) fn language(&self) -> Language {
        if self.platform_id == PlatformId::Windows {
            Language::windows_language(self.language_id)
        } else if self.platform_id == PlatformId::Macintosh
            && self.encoding_id == 0
            && self.language_id == 0
        {
            Language::English_UnitedStates
        } else {
            Language::Unknown
        }
    }
}

/// A list of face names.
#[derive(Clone, Copy, Default)]
pub(crate) struct Names<'a> {
    records: LazyArray16<'a, NameRecord>,
    storage: &'a [u8],
}

impl<'a> Names<'a> {
    /// Returns the names whose records can be read, in table order.
    ///
    /// A record that names a platform the format does not define, or a string
    /// outside the table's storage, is left out; the records after it are
    /// still read.
    pub(crate) fn iter(&self) -> impl Iterator<Item = Name<'a>> + '_ {
        (0..self.records.len()).filter_map(|index| self.get(index))
    }

    /// Returns the name at `index`, or `None` when its record cannot be read.
    fn get(&self, index: u16) -> Option<Name<'a>> {
        let record = self.records.get(index)?;
        let name_start = record.offset.to_usize();
        let name_end = name_start.checked_add(usize::from(record.length))?;
        let name = self.storage.get(name_start..name_end)?;
        Some(Name {
            platform_id: record.platform_id,
            encoding_id: record.encoding_id,
            language_id: record.language_id,
            id: record.name_id,
            data: name,
        })
    }
}

impl core::fmt::Debug for Names<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Names {{ ... }}")
    }
}

/// A [Naming Table](
/// https://docs.microsoft.com/en-us/typography/opentype/spec/name).
#[derive(Clone, Copy, Default, Debug)]
pub(crate) struct Table<'a> {
    /// A list of names.
    pub names: Names<'a>,
}

impl<'a> Table<'a> {
    /// Parses a table from raw data.
    pub(crate) fn parse(data: &'a [u8]) -> Option<Self> {
        // https://docs.microsoft.com/en-us/typography/opentype/spec/name#naming-table-version-1
        /// The last version of the table the format defines.
        const LAST_VERSION: u16 = 1;

        let mut s = Stream::new(data);
        let version = s.read::<u16>()?;
        let count = s.read::<u16>()?;
        let storage_offset = s.read::<Offset16>()?.to_usize();

        if version > LAST_VERSION {
            return None;
        }

        // Versions 0 and 1 are laid out alike this far. Version 1 puts its
        // language tags after the name records, and the storage offset
        // already counts them.
        let records = s.read_array16::<NameRecord>(count)?;

        if let Some(gap) = storage_offset.checked_sub(s.offset()) {
            s.advance(gap);
        }

        let storage = s.tail()?;

        Some(Table {
            names: Names { records, storage },
        })
    }
}
