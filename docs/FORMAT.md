# How a `.DS_Store` file works, byte by byte

Apple never published this format. What follows is what the community worked out by reverse
engineering (credits at the end), checked against real files, and walked through with the sample
file `crates/dsxray/tests/fixtures/small.DS_Store` (16,388 bytes, 16 records). You can follow along
in the browser: open the [dsxray page](https://sadmansakib811.github.io/dsxray/), click **Try a
sample**, then click parts of the byte map.

All numbers are big-endian. The file has two layers:

1. **A buddy allocator.** The file is a heap of blocks whose sizes are powers of two. A header
   points at the allocator's own block, which lists where every block is.
2. **A B-tree of records**, stored in those blocks. Each record says one thing about one file name:
   "the icon of `holiday.jpg` sits at x=64, y=112".

## 1. The header (36 bytes)

```
00000000: 0000 0001 4275 6431 0000 0800 0000 0800  ....Bud1........
00000010: 0000 0800 0000 100c 0000 0087 0000 200b  .............. .
00000020: 0000 0000                                ....
```

| Bytes | Value | Meaning |
|---|---|---|
| 0–3 | `00 00 00 01` | Always 1. It sits *in front of* the allocator's address space: every offset in the file is counted from byte 4. |
| 4–7 | `Bud1` | Magic: "buddy allocator, version 1". |
| 8–11 | `0x800` | Offset of the allocator's own block (so file position `0x800 + 4`). |
| 12–15 | `0x800` | Its size: 2,048 bytes. |
| 16–19 | `0x800` | The offset again. dsxray rejects files where the two copies differ. |
| 20–35 | … | 16 bytes nobody has explained. |

## 2. The allocator block

```
00000804: 0000 0003 0000 0000 0000 080b 0000 0025  ...............%
00000814: 0000 200d 0000 0000 …
```

- `3`: the number of blocks, then a 4-byte field that is always zero.
- The **block table**: one 32-bit address per block. The low 5 bits are log₂ of the block's size,
  the rest is its offset. Blocks are aligned to their size, so the low bits are free to reuse:

  | Block | Address | Offset | Size | What it holds |
  |---|---|---|---|---|
  | 0 | `0x080b` | `0x800` | 2¹¹ = 2,048 | the allocator itself |
  | 1 | `0x0025` | `0x20` | 2⁵ = 32 | the tree header (`DSDB`) |
  | 2 | `0x200d` | `0x2000` | 2¹³ = 8,192 | the tree's only node |

  The table is written in groups of 256 addresses; the rest of the last group is zero.
- A **table of contents**: a count, then entries of (1-byte name length, name, block number).
  There is normally one entry: `DSDB` → block 1.
- **Free lists**: 32 lists (one per block size), each a count and that many offsets. Readers can
  skip them.

## 3. The tree header (`DSDB`, 20 bytes)

```
00000024: 0000 0002 0000 0000 0000 0010 0000 0001 0000 1000
```

| Field | Value | Meaning |
|---|---|---|
| root | 2 | block number of the root node |
| levels | 0 | levels of branch nodes above the leaves (0: the root is a leaf) |
| records | 16 | how many records the tree holds |
| nodes | 1 | how many nodes it has |
| page size | `0x1000` | 4,096, in every file seen so far |

## 4. Tree nodes

Every node starts with two numbers, `P` and a count:

- `P = 0`: a **leaf**. `count` records follow.
- `P ≠ 0`: a **branch**. Each of the `count` records is preceded by the block number of the child
  that holds the records sorting before it, and `P` is the right-most child.

Reading a branch as *child, record, child, record, …, P* gives every record in sorted order (by file
name, then by code).

```
00002004: 0000 0000 0000 0010 …      P = 0 (leaf), 16 records
```

Small folders have one leaf. Of 77 real Finder files on one Mac, 67 had a single leaf and 10 had a
branch over 2 to 7 leaves.

## 5. A record

The icon position of `holiday.jpg`, 54 bytes at file offset 8,710:

```
0000 000b                         name length: 11 UTF-16 code units
0068 006f 006c 0069 0064 0061     "holiday.jpg" in UTF-16 big-endian
0079 002e 006a 0070 0067
496c 6f63                         code: "Iloc" (icon location)
626c 6f62                         type: "blob"
0000 0010                         blob length: 16
0000 0040 0000 0070               x = 64, y = 112
ffff ffff ffff 0000               always this, meaning unknown
```

The **type** says how long the value is:

| Type | Value |
|---|---|
| `bool` | 1 byte |
| `long` | 4-byte integer |
| `shor` | 2-byte integer, stored in 4 bytes |
| `type` | 4-character code |
| `comp` | 8-byte integer (sizes) |
| `dutc` | 8-byte timestamp: 1/65,536 seconds since 1 January 1904, UTC |
| `ustr` | 4-byte length in UTF-16 units, then the text |
| `blob` | 4-byte length in bytes, then the bytes |

A type a reader doesn't know means it can't find where the record ends, so the rest of the node is
lost. dsxray stops there and reports it, keeping the records before it.

## 6. The codes that leak the most

| Code | Type | What it stores |
|---|---|---|
| `Iloc` | blob | icon position: two 32-bit coordinates |
| `bwsp` | blob | window settings, a binary property list (`bplist00`) |
| `icvp` | blob | icon view settings, a binary property list |
| `lsvp`, `lsvP`, `lsvC` | blob | list view settings and columns, binary property lists |
| `cmmt` | ustr | the Spotlight comment typed in Get Info |
| `ptbL`, `ptbN` | ustr | in the Trash: where an item came from and its original name |
| `modD`, `moDD` | dutc | a cached modification date |
| `logS`, `lg1S`, `phyS`, `ph1S` | comp | cached folder sizes |
| `vstl` | type | the view style: `icnv` icon, `Nlsv` list, `clmv` column, `glyv` gallery |

Every record carries a file name, whatever its code. That's the leak: a website that ships its
`.DS_Store` lists every file in that folder, linked or not, and the Trash's `.DS_Store` keeps the
original folders and names of deleted files. `dsxray scan` finds these files before they go out.

## Reading untrusted files safely

A `.DS_Store` can come from anywhere, so dsxray treats every number in it as hostile:

- every read is bounds-checked against its block and the file (the parser has no `unsafe` code);
- a length can't make it allocate more than the bytes that are really there;
- the tree walk refuses to visit a node twice (a crafted file can point a node at itself) and stops
  at 32 levels;
- damage inside the tree ends the walk with a reason, and the records read so far are kept.

The test suite parses every truncation of the fixtures and 20,000 randomly damaged copies; none
may panic.

## Credits

The format was reverse-engineered by others long before dsxray:

- Mark Mentovai's original notes on the format,
- Wim Lewis's `Mac::Finder::DSStore` and its format description
  ([DSStoreFormat.pod](https://metacpan.org/dist/Mac-Finder-DSStore/view/DSStoreFormat.pod)),
- Alastair Houghton's Python library [`ds_store`](https://github.com/al45tair/ds_store), used here
  to write the test fixtures and as the reference in the differential test.

dsxray's part: a hardened Rust reader, the meaning of each code in plain words, the byte map, and
checking the reader against `ds_store` on real files (`scripts/differential.py`: 79 files, 1,962
records, all identical). One thing found on the way: `ds_store` 1.3.3 writes broken trees once a
node splits (the same leaf gets linked many times, and its own reader then returns 121,855 records
for 300 inserted), so the multi-level test fixture is packed by hand in `scripts/make_fixtures.py`.
