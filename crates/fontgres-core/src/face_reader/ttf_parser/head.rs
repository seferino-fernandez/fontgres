//! The one field read from the
//! [Font Header Table](https://learn.microsoft.com/en-us/typography/opentype/spec/head).

use super::parser::Stream;

const MAC_STYLE_OFFSET: usize = 44;

/// The table's `macStyle` flags. The default has none set.
#[derive(Clone, Copy, Default)]
pub(crate) struct MacStyle(u16);

impl MacStyle {
    /// Reads the flags from the table's raw data, or `None` when the table
    /// is too short to hold them.
    pub(crate) fn parse(data: &[u8]) -> Option<Self> {
        Stream::read_at::<u16>(data, MAC_STYLE_OFFSET).map(Self)
    }

    /// Returns whether the face is flagged bold.
    #[inline]
    pub(crate) fn bold(self) -> bool {
        self.0 & 1 != 0
    }

    /// Returns whether the face is flagged italic.
    #[inline]
    pub(crate) fn italic(self) -> bool {
        self.0 & (1 << 1) != 0
    }
}
