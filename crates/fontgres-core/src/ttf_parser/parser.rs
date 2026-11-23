//! Binary parsing utils.

use core::convert::TryInto;

/// A trait for parsing raw binary data of fixed size.
///
/// This is a low-level, internal trait that should not be used directly.
pub(crate) trait FromData: Sized {
    /// Object's raw data size.
    ///
    /// Not always the same as `mem::size_of`.
    const SIZE: usize;

    /// Parses an object from a raw data.
    fn parse(data: &[u8]) -> Option<Self>;
}

impl FromData for u16 {
    const SIZE: usize = 2;

    #[inline]
    fn parse(data: &[u8]) -> Option<Self> {
        data.try_into().ok().map(u16::from_be_bytes)
    }
}

impl FromData for u32 {
    const SIZE: usize = 4;

    #[inline]
    fn parse(data: &[u8]) -> Option<Self> {
        data.try_into().ok().map(u32::from_be_bytes)
    }
}

/// A safe u32 to usize casting.
///
/// Rust doesn't implement `From<u32> for usize`,
/// because it has to support 16 bit targets.
/// We don't, so we can allow this.
pub(super) trait NumFrom<T>: Sized {
    /// Converts u32 into usize.
    fn num_from(_: T) -> Self;
}

impl NumFrom<u32> for usize {
    #[inline]
    fn num_from(v: u32) -> Self {
        #[cfg(any(target_pointer_width = "32", target_pointer_width = "64"))]
        {
            v as usize
        }

        // compilation error on 16 bit targets
    }
}

/// A slice-like container that converts internal binary data only on access.
///
/// Array values are stored in a continuous data chunk.
#[derive(Clone, Copy)]
pub(crate) struct LazyArray16<'a, T> {
    data: &'a [u8],
    data_type: core::marker::PhantomData<T>,
}

impl<T> Default for LazyArray16<'_, T> {
    #[inline]
    fn default() -> Self {
        LazyArray16 {
            data: &[],
            data_type: core::marker::PhantomData,
        }
    }
}

impl<'a, T: FromData> LazyArray16<'a, T> {
    /// Creates a new `LazyArray`.
    #[inline]
    pub(crate) fn new(data: &'a [u8]) -> Self {
        LazyArray16 {
            data,
            data_type: core::marker::PhantomData,
        }
    }

    /// Returns a value at `index`.
    #[inline]
    pub(crate) fn get(&self, index: u16) -> Option<T> {
        self.data
            .chunks_exact(T::SIZE)
            .nth(usize::from(index))
            .and_then(T::parse)
    }

    /// Returns array's length.
    #[inline]
    pub(crate) fn len(&self) -> u16 {
        u16::try_from(self.data.chunks_exact(T::SIZE).len()).unwrap_or(u16::MAX)
    }

    /// Performs a binary search using specified closure.
    #[inline]
    pub(crate) fn binary_search_by<F>(&self, mut f: F) -> Option<(u16, T)>
    where
        F: FnMut(&T) -> core::cmp::Ordering,
    {
        // Based on Rust std implementation.

        use core::cmp::Ordering;

        let mut size = self.len();
        if size == 0 {
            return None;
        }

        let mut base: u16 = 0;
        while size > 1 {
            let half = size / 2;
            let mid = base.checked_add(half)?;
            // mid is always in [0, size), that means mid is >= 0 and < size.
            // mid >= 0: by definition
            // mid < size: mid = size / 2 + size / 4 + size / 8 ...
            let cmp = f(&self.get(mid)?);
            base = if cmp == Ordering::Greater { base } else { mid };
            size = size.checked_sub(half)?;
        }

        // base is always in [0, size) because base <= mid.
        let value = self.get(base)?;
        if f(&value) == Ordering::Equal {
            Some((base, value))
        } else {
            None
        }
    }
}

impl<'a, T: FromData> IntoIterator for LazyArray16<'a, T> {
    type Item = T;
    type IntoIter = LazyArrayIter16<'a, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        LazyArrayIter16 {
            data: self,
            index: 0,
        }
    }
}

/// An iterator over `LazyArray16`.
#[derive(Clone, Copy)]
pub(crate) struct LazyArrayIter16<'a, T> {
    data: LazyArray16<'a, T>,
    index: u16,
}

impl<T: FromData> Iterator for LazyArrayIter16<'_, T> {
    type Item = T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let value = self.data.get(self.index)?;
        self.index = self.index.checked_add(1)?;
        Some(value)
    }

    #[inline]
    fn count(self) -> usize {
        usize::from(self.data.len().saturating_sub(self.index))
    }
}

/// A streaming binary parser.
#[derive(Clone, Default, Debug)]
pub(super) struct Stream<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> Stream<'a> {
    /// Creates a new `Stream` parser.
    #[inline]
    pub(super) fn new(data: &'a [u8]) -> Self {
        Stream { data, offset: 0 }
    }

    /// Returns the current offset.
    #[inline]
    pub(super) fn offset(&self) -> usize {
        self.offset
    }

    /// Returns the trailing data.
    ///
    /// Returns `None` when `Stream` is reached the end.
    #[inline]
    pub(super) fn tail(&self) -> Option<&'a [u8]> {
        self.data.get(self.offset..)
    }

    /// Advances by `FromData::SIZE`.
    ///
    /// Doesn't check bounds.
    #[inline]
    pub(super) fn skip<T: FromData>(&mut self) {
        self.advance(T::SIZE);
    }

    /// Advances by the specified `len`.
    ///
    /// Doesn't check bounds: past the end, every later read returns `None`.
    #[inline]
    pub(super) fn advance(&mut self, len: usize) {
        self.offset = self.offset.saturating_add(len);
    }

    /// Advances by the specified `len` and checks for bounds.
    #[inline]
    pub(super) fn advance_checked(&mut self, len: usize) -> Option<()> {
        if self
            .offset
            .checked_add(len)
            .is_some_and(|end| end <= self.data.len())
        {
            self.advance(len);
            Some(())
        } else {
            None
        }
    }

    /// Parses the type from the steam.
    ///
    /// Returns `None` when there is not enough data left in the stream
    /// or the type parsing failed.
    #[inline]
    pub(super) fn read<T: FromData>(&mut self) -> Option<T> {
        self.read_bytes(T::SIZE).and_then(T::parse)
    }

    /// Parses the type from the steam at offset.
    #[inline]
    pub(super) fn read_at<T: FromData>(data: &[u8], offset: usize) -> Option<T> {
        data.get(offset..offset.checked_add(T::SIZE)?)
            .and_then(T::parse)
    }

    /// Reads N bytes from the stream.
    #[inline]
    pub(super) fn read_bytes(&mut self, len: usize) -> Option<&'a [u8]> {
        let end = self.offset.checked_add(len)?;
        let v = self.data.get(self.offset..end)?;
        self.advance(len);
        Some(v)
    }

    /// Reads the next `count` types as a slice.
    #[inline]
    pub(super) fn read_array16<T: FromData>(&mut self, count: u16) -> Option<LazyArray16<'a, T>> {
        let len = usize::from(count).checked_mul(T::SIZE)?;
        self.read_bytes(len).map(LazyArray16::new)
    }
}

/// A common offset methods.
pub(super) trait Offset {
    /// Converts the offset to `usize`.
    fn to_usize(&self) -> usize;
}

/// A type-safe u16 offset.
#[derive(Clone, Copy, Debug)]
pub(super) struct Offset16(pub u16);

impl Offset for Offset16 {
    #[inline]
    fn to_usize(&self) -> usize {
        usize::from(self.0)
    }
}

impl FromData for Offset16 {
    const SIZE: usize = 2;

    #[inline]
    fn parse(data: &[u8]) -> Option<Self> {
        u16::parse(data).map(Offset16)
    }
}

/// A type-safe u32 offset.
#[derive(Clone, Copy, Debug)]
pub(super) struct Offset32(pub u32);

impl Offset for Offset32 {
    #[inline]
    fn to_usize(&self) -> usize {
        usize::num_from(self.0)
    }
}

impl FromData for Offset32 {
    const SIZE: usize = 4;

    #[inline]
    fn parse(data: &[u8]) -> Option<Self> {
        u32::parse(data).map(Offset32)
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::{LazyArray16, Stream};

    const WORDS: &[u8] = &[0x00, 0x01, 0x00, 0x05, 0x00, 0x09, 0xFF];

    #[test]
    fn a_lazy_array_reads_whole_values_only() {
        let array = LazyArray16::<u16>::new(WORDS);
        assert_eq!(array.len(), 3);
        assert_eq!(array.get(0), Some(1));
        assert_eq!(array.get(2), Some(9));
        assert_eq!(array.get(3), None);
        assert_eq!(array.get(u16::MAX), None);
    }

    #[test]
    fn a_lazy_array_iterates_its_values_then_stops() {
        let mut values = LazyArray16::<u16>::new(WORDS).into_iter();
        assert_eq!(values.count(), 3);
        assert_eq!(values.by_ref().collect::<Vec<_>>(), [1, 5, 9]);
        assert_eq!(values.next(), None);
        assert_eq!(values.next(), None);
    }

    #[test]
    fn a_binary_search_finds_only_present_values() {
        let array = LazyArray16::<u16>::new(WORDS);
        for (index, value) in [(0, 1), (1, 5), (2, 9)] {
            assert_eq!(
                array.binary_search_by(|v| v.cmp(&value)),
                Some((index, value))
            );
        }
        for missing in [0, 4, 10] {
            assert_eq!(array.binary_search_by(|v| v.cmp(&missing)), None);
        }
        assert_eq!(
            LazyArray16::<u16>::new(&[]).binary_search_by(|v| v.cmp(&1)),
            None
        );
    }

    #[test]
    fn a_read_that_overflows_the_offset_fails() {
        let mut stream = Stream::new(WORDS);
        assert_eq!(stream.read::<u16>(), Some(1));
        assert_eq!(stream.read_bytes(usize::MAX), None);
        assert!(stream.read_array16::<u16>(u16::MAX).is_none());
        assert_eq!(Stream::read_at::<u16>(WORDS, usize::MAX), None);
        assert_eq!(stream.advance_checked(usize::MAX), None);
        assert_eq!(stream.offset(), 2);
    }

    #[test]
    fn reads_past_an_advance_beyond_the_end_fail() {
        let mut stream = Stream::new(WORDS);
        stream.advance(usize::MAX);
        stream.advance(1);
        assert_eq!(stream.read::<u16>(), None);
        assert_eq!(stream.tail(), None);
    }

    #[test]
    fn advance_checked_allows_reaching_the_end_exactly() {
        let mut stream = Stream::new(WORDS);
        assert_eq!(stream.advance_checked(WORDS.len()), Some(()));
        assert_eq!(stream.tail(), Some(&[][..]));
        assert_eq!(stream.advance_checked(1), None);
    }
}
