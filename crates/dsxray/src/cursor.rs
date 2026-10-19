use crate::{Error, FourCc};

/// A big-endian reader over one block of the file. Every read is bounds-checked against the block,
/// and positions stay absolute (bytes from the start of the file), so errors and byte maps can
/// point at the exact byte.
pub(crate) struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
    end: usize,
}

impl<'a> Cursor<'a> {
    pub(crate) fn new(data: &'a [u8], start: usize, end: usize) -> Self {
        let end = end.min(data.len());
        Cursor { data, pos: start.min(end), end }
    }

    pub(crate) fn pos(&self) -> usize {
        self.pos
    }

    pub(crate) fn bytes(&mut self, n: usize, what: &'static str) -> Result<&'a [u8], Error> {
        if n > self.end - self.pos {
            return Err(Error::Truncated { what, at: self.pos });
        }
        let out = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(out)
    }

    pub(crate) fn u8(&mut self, what: &'static str) -> Result<u8, Error> {
        Ok(self.bytes(1, what)?[0])
    }

    pub(crate) fn u32(&mut self, what: &'static str) -> Result<u32, Error> {
        let b = self.bytes(4, what)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub(crate) fn u64(&mut self, what: &'static str) -> Result<u64, Error> {
        let b = self.bytes(8, what)?;
        Ok(u64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
    }

    pub(crate) fn fourcc(&mut self, what: &'static str) -> Result<FourCc, Error> {
        let b = self.bytes(4, what)?;
        Ok(FourCc([b[0], b[1], b[2], b[3]]))
    }

    /// A UTF-16 big-endian string of `units` code units. Broken surrogates become U+FFFD.
    pub(crate) fn utf16(&mut self, units: u32, what: &'static str) -> Result<String, Error> {
        let len = (units as usize).checked_mul(2).ok_or(Error::Truncated { what, at: self.pos })?;
        let b = self.bytes(len, what)?;
        let units: Vec<u16> = b.as_chunks::<2>().0.iter().map(|p| u16::from_be_bytes(*p)).collect();
        Ok(String::from_utf16_lossy(&units))
    }
}
