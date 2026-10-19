//! The B-tree that holds the records, sorted by file name and then by record code.
//!
//! Every node starts with two numbers: `P` and a record count. `P = 0` marks a leaf, followed by its
//! records. Otherwise the node is a branch: each record is preceded by the block number of the
//! child that holds the smaller records, and `P` is the right-most child. Reading children before
//! their records gives the records in sorted order.

use std::collections::BTreeSet;

use crate::buddy::Allocator;
use crate::cursor::Cursor;
use crate::{Error, Record, Value};

/// No real file comes close: a 3-level tree already holds hundreds of thousands of records.
const MAX_DEPTH: u32 = 32;

pub(crate) struct Node {
    pub(crate) block: u32,
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) leaf: bool,
    pub(crate) records: u32,
}

pub(crate) struct Walk<'a, 'b> {
    alloc: &'b Allocator<'a>,
    visited: BTreeSet<u32>,
    pub(crate) records: Vec<Record>,
    pub(crate) nodes: Vec<Node>,
}

impl<'a, 'b> Walk<'a, 'b> {
    pub(crate) fn new(alloc: &'b Allocator<'a>) -> Self {
        Walk { alloc, visited: BTreeSet::new(), records: Vec::new(), nodes: Vec::new() }
    }

    pub(crate) fn node(&mut self, block: u32, depth: u32) -> Result<(), Error> {
        if depth > MAX_DEPTH {
            return Err(Error::TreeTooDeep);
        }
        if !self.visited.insert(block) {
            return Err(Error::NodeLoop(block));
        }
        let (start, end) = self.alloc.block_range(block)?;
        let mut c = self.alloc.block(block)?;
        let right = c.u32("node's right-most child")?;
        let count = c.u32("node's record count")?;
        self.nodes.push(Node { block, start, end, leaf: right == 0, records: count });
        for _ in 0..count {
            if right != 0 {
                let child = c.u32("child block number")?;
                self.node(child, depth + 1)?;
            }
            self.records.push(record(&mut c)?);
        }
        if right != 0 {
            self.node(right, depth + 1)?;
        }
        Ok(())
    }
}

/// One record: name length (UTF-16 units), name (UTF-16 big-endian), code, type, value.
pub(crate) fn record(c: &mut Cursor) -> Result<Record, Error> {
    let offset = c.pos();
    let units = c.u32("file name length")?;
    let name = c.utf16(units, "file name")?;
    let code = c.fourcc("record code")?;
    let kind = c.fourcc("value type")?;
    let value = match &kind.0 {
        b"bool" => Value::Bool(c.u8("bool value")? != 0),
        b"long" => Value::Long(c.u32("long value")?),
        b"shor" => Value::Shor(c.u32("shor value")?),
        b"type" => Value::Type(c.fourcc("type value")?),
        b"comp" => Value::Comp(c.u64("comp value")?),
        b"dutc" => Value::Dutc(c.u64("dutc value")?),
        b"ustr" => {
            let units = c.u32("ustr length")?;
            Value::Ustr(c.utf16(units, "ustr value")?)
        }
        b"blob" => {
            let len = c.u32("blob length")?;
            Value::Blob(c.bytes(len as usize, "blob value")?.to_vec())
        }
        _ => return Err(Error::UnknownType { code: kind, at: c.pos() - 4 }),
    };
    Ok(Record { name, code, value, offset, length: c.pos() - offset })
}
