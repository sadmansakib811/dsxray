# Changelog

## 0.1.0 (2026-10-19)

- Parser for `.DS_Store` files: buddy allocator, B-tree with branch nodes, all eight value types.
- Plain-language meanings for 36 record codes; decoded icon positions, binary property lists, dates,
  sizes, view styles, window frames and backgrounds.
- `dsxray FILE…` (table, `--json`, `--names`) and `dsxray scan DIR…` (exits 1 when it finds one).
- Browser page with a byte map and a hex view, built from the same parser as WebAssembly.
- Tests: fixtures from Python's `ds_store`, every truncation, 20,000 damaged copies, a self-looping
  node; differential test against `ds_store` on real files.
