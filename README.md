This repository outlines my experiments in writing code to determine
if some array of codepoints conforms to Unicode Standard Annex #31,
which is an official reccomendation for different kinds of
identifiers.

I plan to write an article detailing my findings here and documenting the
experience. I will update the README accordingly once that's finished to link
to it.

## Methodology

Many solutions exist in the space already, but I wanted to see
if I could come up with a solution which used less than half of the
binary space as the status quo, and what the performance
characteristics would be of such an implementation. The implementation
I used as a benchmark during development was the Rust crate
`unicode-id-start`, which uses a trie and roughly 10 KiB of static
storage, meaning my goal was to fit my implementation in under 5 KiB.

The smallest I was able to get was the implementation found in
`delta-encoded` and uses only 2487 bytes in a table, however the
performance is, from my measurements 2 orders of magnitude slower than
even a naive implementation, and thus likely not suitable for general
purpose use. Rather, I consider the `unicode-id-trie-rle` implementation to be
the result of this experiment, and uses a trie in combination with run length
encoding deduplicated fixed sized blocks to bring the static table size down to
6713 bytes. The performance of this implementation is, by my measurements
faster than `unicode-id-start` when the input is more ASCII than not, and
slower by as much as 2x when the input is mostly not ASCII. I included a C
and Go version as well, in the `go/` and `c/` folders respectively.

Below I briefly describe each implementation.

### `baseline`

The entire problem can be reduced to finding an efficient way to store
and lookup 2 bits of data per Unicode codepoint, one bit for
codepoints which have the property `ID_Start` or `XID_Start`, and one
bit for codepoints which have the property `ID_Continue` or
`XID_Continue`. The `baseline` implementation just stores these bits
in continuous memory, taking up 34816 bytes of static storage, and
using the codepoint value as a bit index into this memory.

### `run-indexed`

This is a simple implementation which run-length-encodes the entire table and
includes an index so the entire table doesn't have to be traversed every time.

It is included for a later article I plan to write detailing the whole process.

### `unicode-id-trie-rle`

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


### `delta-encoded`

This version encodes the bits using
[delta encoding](https://en.wikipedia.org/wiki/Delta_encoding).
Specifically, I encode each run as a tuple of
`(delta, run length, 2 bit payload)`, where the delta and run length are LEB128
encoded integers and the 2 bits are packed in memory after.

The delta value is the number of codepoints in between the previous run and
this run, while the run length is the number of codepoints which the 2 bit
payload applies to.

This implementation only uses 2479 bytes (!), but is roughly two orders of
magnitude slower than the other implementations.

Run length encoding in a similarly-packed way yielded a roughly 2x larger size
since the input data is sparse.

## Tests and benchmarks

- Rust crates (`baseline`, `run-indexed`, `delta-encoded`,
  `unicode-id-trie-rle`, `unicode-id-start`) share the same harness:
  `cargo test` re-parses `DerivedCoreProperties.txt` with the
  `derived_core_properties` crate, rebuilds the expected
  `ID_Start`/`ID_Continue` table for every scalar value, and checks each
  `unicode_identifier_class`; property tests also assert the string and slice
  entry points agree. The parser crate itself is fuzzed with property tests and
  explicit error cases.
- Go port (`go/`): `go test ./...` re-derives the reference table from
  `DerivedCoreProperties.txt` and walks every codepoint, failing on any
  mismatch with `UnicodeIdentifierClass`.
- C port (`c/`): generate `unicode_data.h` with `generate.c`, then build and
  run `unicode_identifiers_test.c`; it re-parses `DerivedCoreProperties.txt`
  and checks every codepoint against the generated table.
- Benchmarks live in the `benchmark/` crate. Run `cargo bench` to drive
  Criterion over fixed corpora in `benchmark/corpus/ascii-{pct}/len{len}.txt`,
  covering 32/128/512 character strings at 0/10/50/90/100% ASCII mixes. Results
  are checked in under `benchmark-results/` (human-readable and
  machine-readable). The machine readable results are only generated when
  running `cargo criterion` instead of `cargo bench`.

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
