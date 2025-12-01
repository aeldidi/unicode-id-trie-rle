# `unicode-id-trie-rle`

An implementation of
[Unicode Standard Annex #31](https://www.unicode.org/reports/tr31/) for
determining if a string is a valid Unicode identifier. Alternatively, a
library which allows querying `char` values for the Unicode properties
`ID_Start` and `ID_Continue`. This implementation folds the `XID_*`
properties into the `ID_*` properties (because `XID_*` entries are a subset
and are stored under the same bits), so characters that are `ID_*` but **not**
`XID_*` will also be treated as identifier starts/continues. UAX #31 defines
default identifiers in terms of `XID_Start`/`XID_Continue`, so keep this
superset behavior in mind. In practice, the difference only matters for
workloads that require XID closure under normalization; most identifier
consumers will be unaffected.

This crate is `no_std` for consumers; it only enables `std` when running tests
or benchmarks.

## Comparisons to `unicode-id-start`

When developing this library, I used the
[`unicode-id-start`](https://github.com/oxc-project/unicode-id-start) library
as a benchmark, since it seemed to be the state of the art at the time, both
in terms of storage size and performance. My measurements were performed in
version 1.0.0 of this library, and commit
`d0ab8e55bc03bf60178688fbb7d9f0ce48bca94f` of `unicode-id-start`.

This library uses 6713 bytes of static storage for its generated tables, while
`unicode-id-start` uses 10396 bytes. In benchmarks, this library mostly excels
on ASCII-heavy workloads, especially small inputs. For pure ASCII 32‑byte
strings it's ~4–5x faster, and it stays ahead (though by less) on longer
all‑ASCII strings. At smaller ASCII-dominated inputs (e.g., 90% ASCII, 32
bytes), it still leads, but by a negligible amount. `unicode-id-start` shines
anywhere non‑ASCII is more prevelant and on longer strings in general: with
mixed or non‑ASCII text it’s routinely 2–4x faster across sizes, and even at
high ASCII mixes (90%) it pulls ahead once strings reach 128–512 bytes.

Overall, it's not a straight "win" in performance, but in typical workloads for
parsers (ASCII dominated, short identifiers), I think it manages to pull ahead.

Benchmarks were run on a Ryzen 7 3800X with 16GiB of ram by running
`cargo criterion` at the git repository's root. The output and raw
machine-readable values are available in the git respository in the
`benchmark-results` folder, but I would take these with a grain of salt since
there a many factors which can influence a benchmark.

## Technique

This implementation, like most, uses a trie to store the codepoints, with the
difference that the codepoint space is partitioned into blocks which allows the
leaf nodes to be deduplicated, saving much more space than most
implementations.

Specifically, we use the most significant 10 bits of the codepoint as the block
index, giving 1024-codepoint blocks. That block index is split into `top` and
`bottom` indices, where `top` is the 6 most significant bits of the block index
and `bottom` is the 4 least significant bits. We index the level 1 table with
`top`, which yields an id for a level 2 table. The level 2 table is a 2-D array
(flattened in the generated source) storing the leaf id for every possible
`bottom` value. This makes level 2 effectively `[[u16; 16]]`, since there are
16 unique 4-bit strings. We get the leaf for a specific block via
`LEVEL2_TABLES[level2_id * 16 + bottom]`. The level 2 tables themselves are
deduplicated: multiple `top` values can share the same table when all 16 blocks
under them point to the same leaves.

Each leaf represents one 1024-codepoint block. We only store one instance of
each unique leaf: if two blocks have the same run layout of identifier bits,
they share the same leaf id. To enable deduplication, leaf runs are expressed
relative to the start of their block, so every leaf run list starts with `0`
and ends with a sentinel at the end of the block.

The generated leaf tables are split so we can pack everything densely:

- `LEAF_OFFSETS` gives, for each leaf id, the start index into the run arrays
  (with a sentinel at the end to recover lengths).
- `LEAF_RUN_STARTS` stores the start of each run within a leaf, relative to the
  start of its 1024-codepoint block. The first entry is always `0`, and the
  last entry is the block length (sentinel).
- `LEAF_RUN_VALUES` stores the run value bits, aligned with
  `LEAF_RUN_STARTS`. Bit `0x1` means `ID_Start`, bit `0x2` means
  `ID_Continue`. Runs are contiguous in both arrays for each leaf, so looking
  up a codepoint offset is just a partition point into that slice.

You'll notice if you analyze the data that many leaf nodes also share
identical layouts once you canonicalize the runs to be relative to some
starting point. These are also deduplicated, and each entry in the level 2
which has a non-unique leaf node points to the same physical leaf in the leaf
tables.

## Changelog

### 1.0

- Initial release, contains support for Unicode 17.0.0

## License

Because this library generates code from the Unicode Database, specifically
`DerivedCoreProperties.txt`, the generated files are subject to the terms of
the Unicode License V3, which at the time of writing can be found at
https://www.unicode.org/license.txt, or in the git repository this software is
distributed at.

Every other file is in the public domain. I have also licensed them under
0BSD, Creative Commons 0 1.0, and Unlicense for those who prefer those. A copy
of the 0BSD license is available in the git repository this software is
distributed at as well.
