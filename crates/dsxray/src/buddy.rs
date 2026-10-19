//! The buddy allocator: the container around the records.
//!
//! The file is split into blocks whose sizes are powers of two. A 36-byte header points at the
//! allocator's own block, which lists every block's address, a table of contents (name -> block
//! number, normally just `DSDB`) and the free lists. All offsets in the file are counted from byte 4,
//! not byte 0: the first four bytes (`00 00 00 01`) sit in front of the allocator's address space.

use crate::Error;
use crate::cursor::Cursor;

/// Bytes 0..4 alignment, 4..8 `Bud1`, then root offset, root size, a copy of the root offset and
/// 16 bytes nobody has explained yet.
pub(crate) const HEADER_LEN: usize = 36;

pub(crate) struct Allocator<'a> {
    data: &'a [u8],
    /// Byte range of the allocator's own block.
    pub(crate) root: (usize, usize),
    /// The block table. Each entry is an offset whose low 5 bits hold log2 of the block's size.
    pub(crate) blocks: Vec<u32>,
    /// Table of contents: name -> block number.
    toc: Vec<(Vec<u8>, u32)>,
}

impl<'a> Allocator<'a> {
    pub(crate) fn read(data: &'a [u8]) -> Result<Self, Error> {
        if data.len() < HEADER_LEN || data[0..4] != [0, 0, 0, 1] || &data[4..8] != b"Bud1" {
            return Err(Error::NotDsStore);
        }
        let mut h = Cursor::new(data, 8, HEADER_LEN);
        let offset = h.u32("allocator block offset")?;
        let size = h.u32("allocator block size")?;
        if h.u32("copy of the allocator block offset")? != offset {
            return Err(Error::Corrupt("the two copies of the allocator block's offset differ"));
        }
        let start = offset as u64 + 4;
        if start >= data.len() as u64 {
            return Err(Error::Truncated { what: "allocator block", at: start.min(usize::MAX as u64) as usize });
        }
        let end = (start + size as u64).min(data.len() as u64) as usize;
        let start = start as usize;

        let mut c = Cursor::new(data, start, end);
        let count = c.u32("block count")?;
        c.u32("unknown field after the block count")?;
        let mut blocks = Vec::new();
        for _ in 0..count {
            blocks.push(c.u32("block address")?);
        }
        // The table is written in groups of 256 addresses; the unused rest of the last group is zero.
        let padding = (count as u64).div_ceil(256) * 256 - count as u64;
        c.bytes((padding * 4) as usize, "block table padding")?;

        let entries = c.u32("table of contents size")?;
        let mut toc = Vec::new();
        for _ in 0..entries {
            let len = c.u8("table of contents name length")?;
            let name = c.bytes(len as usize, "table of contents name")?.to_vec();
            toc.push((name, c.u32("table of contents block number")?));
        }
        Ok(Allocator { data, root: (start, end), blocks, toc })
    }

    pub(crate) fn find(&self, name: &str) -> Option<u32> {
        self.toc.iter().find(|(n, _)| n == name.as_bytes()).map(|(_, b)| *b)
    }

    /// The byte range of block `n`, cut at the end of the file.
    pub(crate) fn block_range(&self, n: u32) -> Result<(usize, usize), Error> {
        let addr = *self.blocks.get(n as usize).ok_or(Error::NoSuchBlock(n))?;
        let start = (addr & !0x1f) as u64 + 4;
        let size = 1u64 << (addr & 0x1f);
        let len = self.data.len() as u64;
        if start >= len {
            return Err(Error::NoSuchBlock(n));
        }
        Ok((start as usize, (start + size).min(len) as usize))
    }

    pub(crate) fn block(&self, n: u32) -> Result<Cursor<'a>, Error> {
        let (start, end) = self.block_range(n)?;
        Ok(Cursor::new(self.data, start, end))
    }
}
