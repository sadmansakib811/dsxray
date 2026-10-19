//! Read Apple's `.DS_Store` files: every record, what it means, and where its bytes are.
//!
//! macOS Finder drops a `.DS_Store` file into every folder it opens. It stores icon positions,
//! window sizes, view settings, Spotlight comments and more, keyed by **file name**. So a stray
//! `.DS_Store` that gets uploaded with a website or committed to a repository tells anyone who reads
//! it which files sit in that folder, including the ones that were never linked or published.
//!
//! Apple never documented the format. It is a small "buddy allocator" (a file split into
//! power-of-two blocks) holding a B-tree of records. `docs/FORMAT.md` in the repository walks
//! through it byte by byte.
//!
//! ```
//! let bytes = include_bytes!("../tests/fixtures/small.DS_Store");
//! let store = dsxray::parse(bytes).unwrap();
//! assert!(store.file_names().contains(&"holiday.jpg"));
//! ```
//!
//! The parser never panics and never trusts a length or a pointer in the file: every read is
//! bounds-checked, the tree walk has a depth limit and refuses to visit a node twice. If the records'
//! tree breaks halfway, [`parse`] still returns what it read, with the reason in [`DsStore::problem`].

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod buddy;
mod codes;
mod cursor;
mod json;
mod tree;

use std::fmt;

pub use codes::{Description, describe};
pub use json::{inspect_json, to_json};

/// A four-character code such as `Iloc` or `blob`, kept as the raw bytes from the file.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FourCc(pub [u8; 4]);

impl fmt::Display for FourCc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for &b in &self.0 {
            if b.is_ascii_graphic() || b == b' ' {
                write!(f, "{}", b as char)?;
            } else {
                write!(f, "\\x{b:02x}")?;
            }
        }
        Ok(())
    }
}

impl fmt::Debug for FourCc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FourCc(\"{self}\")")
    }
}

/// One record: a setting Finder stored about one file name.
#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    /// The file or folder the record is about. `"."` means the folder that holds the `.DS_Store`.
    pub name: String,
    /// What the record is, e.g. `Iloc` (icon position) or `bwsp` (window settings).
    pub code: FourCc,
    /// The stored value.
    pub value: Value,
    /// Where the record starts in the file (byte offset).
    pub offset: usize,
    /// How many bytes the record takes, from its name length to the end of its value.
    pub length: usize,
}

/// A record's value. The variant names follow the type codes in the file.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// `bool`: one byte.
    Bool(bool),
    /// `long`: a 32-bit integer.
    Long(u32),
    /// `shor`: a 16-bit integer, stored in 4 bytes.
    Shor(u32),
    /// `type`: a four-character code, e.g. a view style like `icnv`.
    Type(FourCc),
    /// `ustr`: a UTF-16 string.
    Ustr(String),
    /// `comp`: a 64-bit integer (Finder uses it for sizes).
    Comp(u64),
    /// `dutc`: a timestamp in 1/65536 seconds since 1 January 1904.
    Dutc(u64),
    /// `blob`: raw bytes (often a binary property list).
    Blob(Vec<u8>),
}

impl Value {
    /// The type code as written in the file (`bool`, `long`, `blob`, …).
    pub fn type_code(&self) -> &'static str {
        match self {
            Value::Bool(_) => "bool",
            Value::Long(_) => "long",
            Value::Shor(_) => "shor",
            Value::Type(_) => "type",
            Value::Ustr(_) => "ustr",
            Value::Comp(_) => "comp",
            Value::Dutc(_) => "dutc",
            Value::Blob(_) => "blob",
        }
    }
}

/// The header of the records' B-tree (the `DSDB` block).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TreeInfo {
    /// Block number of the tree's root node.
    pub root: u32,
    /// Levels of branch nodes above the leaves (0 = the root itself is a leaf).
    pub levels: u32,
    /// How many records the file says it holds.
    pub records: u32,
    /// How many nodes the file says the tree has.
    pub nodes: u32,
    /// Page size the tree was built for (always 4096 in files seen so far).
    pub page_size: u32,
}

/// A stretch of the file and what it is, for byte maps and hex views.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Region {
    /// First byte.
    pub start: usize,
    /// One past the last byte.
    pub end: usize,
    /// `header`, `allocator`, `tree-header`, `leaf` or `branch`.
    pub kind: &'static str,
    /// A short human description.
    pub label: String,
}

/// A parsed `.DS_Store` file.
#[derive(Debug, Clone, PartialEq)]
pub struct DsStore {
    /// Every record, in the order the tree stores them (sorted by file name, then code).
    pub records: Vec<Record>,
    /// The B-tree's header.
    pub tree: TreeInfo,
    /// The parts of the file this parser identified, sorted by position.
    pub regions: Vec<Region>,
    /// The file's size in bytes.
    pub file_size: usize,
    /// Set when reading the tree stopped early; [`DsStore::records`] then holds what came before.
    pub problem: Option<Error>,
}

impl DsStore {
    /// The file and folder names the records mention, sorted and without duplicates.
    /// The folder itself (`"."`) is left out: these are the names a stranger learns.
    pub fn file_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.records.iter().map(|r| r.name.as_str()).filter(|n| *n != ".").collect();
        names.sort_unstable();
        names.dedup();
        names
    }
}

/// Why a file couldn't be read.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// The file doesn't start with the allocator header (`00 00 00 01` and `Bud1`).
    NotDsStore,
    /// A read went past the end of its block or of the file.
    Truncated {
        /// What was being read.
        what: &'static str,
        /// Byte offset where the read started.
        at: usize,
    },
    /// A block number that isn't in the allocator's block table, or a block outside the file.
    NoSuchBlock(u32),
    /// The table of contents has no `DSDB` entry, so there is no records tree.
    NoRecordTree,
    /// The tree is deeper than any real file could be.
    TreeTooDeep,
    /// A tree node is reachable twice: the file is corrupt, or crafted to loop forever.
    NodeLoop(u32),
    /// A value type this parser doesn't know; the rest of the node can't be read without its length.
    UnknownType {
        /// The unknown type code.
        code: FourCc,
        /// Byte offset of the type code.
        at: usize,
    },
    /// Header fields that contradict each other.
    Corrupt(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotDsStore => write!(f, "not a .DS_Store file (no \"Bud1\" header)"),
            Error::Truncated { what, at } => write!(f, "file ends too early: {what} at byte {at} doesn't fit"),
            Error::NoSuchBlock(n) => write!(f, "block {n} is missing or lies outside the file"),
            Error::NoRecordTree => write!(f, "no records tree (the table of contents has no DSDB entry)"),
            Error::TreeTooDeep => write!(f, "the records tree is impossibly deep"),
            Error::NodeLoop(n) => write!(f, "tree node {n} is reachable twice (corrupt or crafted file)"),
            Error::UnknownType { code, at } => write!(f, "unknown value type \"{code}\" at byte {at}"),
            Error::Corrupt(why) => write!(f, "corrupt file: {why}"),
        }
    }
}

impl std::error::Error for Error {}

/// Parse a `.DS_Store` file.
///
/// Fails only when the allocator or the tree header can't be read. Damage inside the tree is
/// reported in [`DsStore::problem`] together with the records read before it.
pub fn parse(data: &[u8]) -> Result<DsStore, Error> {
    let alloc = buddy::Allocator::read(data)?;
    let mut regions = vec![
        Region {
            start: 0,
            end: buddy::HEADER_LEN,
            kind: "header",
            label: "header: Bud1 magic and where the allocator's block is".into(),
        },
        Region {
            start: alloc.root.0,
            end: alloc.root.1,
            kind: "allocator",
            label: format!("allocator: block table ({} blocks), table of contents, free lists", alloc.blocks.len()),
        },
    ];
    let dsdb = alloc.find("DSDB").ok_or(Error::NoRecordTree)?;
    let (start, end) = alloc.block_range(dsdb)?;
    let mut c = alloc.block(dsdb)?;
    let tree = TreeInfo {
        root: c.u32("tree root")?,
        levels: c.u32("tree levels")?,
        records: c.u32("tree record count")?,
        nodes: c.u32("tree node count")?,
        page_size: c.u32("tree page size")?,
    };
    regions.push(Region {
        start,
        end,
        kind: "tree-header",
        label: format!("DSDB: tree header ({} records, {} nodes)", tree.records, tree.nodes),
    });

    let mut walk = tree::Walk::new(&alloc);
    let problem = walk.node(tree.root, 1).err();
    for n in &walk.nodes {
        let (kind, what) = if n.leaf { ("leaf", "leaf node") } else { ("branch", "branch node") };
        regions.push(Region {
            start: n.start,
            end: n.end,
            kind,
            label: format!("{what}: block {}, {} records", n.block, n.records),
        });
    }
    regions.sort_by_key(|r| (r.start, r.end));
    Ok(DsStore { records: walk.records, tree, regions, file_size: data.len(), problem })
}
