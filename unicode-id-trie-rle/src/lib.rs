#![doc = include_str!("../README.md")]
#![cfg_attr(not(test), no_std)]
const IDENTIFIER_OTHER: u8 = 0;
const IDENTIFIER_START: u8 = 1;
const IDENTIFIER_CONTINUE: u8 = 2;
const START_CODEPOINT: u32 = 0x80;

include!(concat!(env!("OUT_DIR"), "/table.rs"));

const BLOCK_MASK: u32 = (1 << SHIFT) - 1;
const LOWER_MASK: u32 = (1 << LOWER_BITS) - 1;
const ASCII_TABLE: [u8; 128] = ascii_table();

#[derive(Clone, Copy)]
struct Leaf {
    offset: usize,
    len: usize,
}

/// A Unicode identifier class, as returned by [unicode_identifier_class]. Use
/// the [UnicodeIdentifierClass::is_start] and
/// [UnicodeIdentifierClass::is_continue] methods to query specific properties.
pub struct UnicodeIdentifierClass(u8);

impl UnicodeIdentifierClass {
    /// Returns whether or not the codepoint was one of the `*_Start`
    /// identifiers.
    #[inline]
    pub fn is_start(&self) -> bool {
        self.0 & IDENTIFIER_START != 0
    }

    /// Returns whether or not the codepoint was one of the `*_Continue`
    /// identifiers.
    #[inline]
    pub fn is_continue(&self) -> bool {
        self.0 & IDENTIFIER_CONTINUE != 0
    }
}

#[inline]
fn load_leaf(idx: usize) -> Leaf {
    debug_assert!(idx + 1 < LEAF_OFFSETS.len());
    let start = LEAF_OFFSETS[idx] as usize;
    let end = LEAF_OFFSETS[idx + 1] as usize;
    Leaf {
        offset: start,
        len: end - start,
    }
}

#[inline]
fn leaf_value(leaf: Leaf, offset: u16) -> UnicodeIdentifierClass {
    debug_assert!(leaf.len >= 2);
    let runs = &LEAF_RUN_STARTS[leaf.offset..leaf.offset + leaf.len];
    let values = &LEAF_RUN_VALUES[leaf.offset..leaf.offset + leaf.len];
    // runs are ascending with runs[0] == 0 and a sentinel at the end.
    let idx = runs.partition_point(|&start| start <= offset);
    UnicodeIdentifierClass(values[idx.saturating_sub(1)])
}

/// Returns whether the codepoint specified has the properties `ID_Start`,
/// `XID_Start` or the properties `ID_Continue` or `XID_Continue`.
#[inline]
pub fn unicode_identifier_class(cp: char) -> UnicodeIdentifierClass {
    // ASCII fast path via table to avoid unpredictable branches.
    if (cp as u32) < START_CODEPOINT {
        return UnicodeIdentifierClass(ASCII_TABLE[cp as usize]);
    }

    if (cp as u32) >= 0x100000 {
        return UnicodeIdentifierClass(IDENTIFIER_OTHER);
    }

    let cp = cp as u32;
    let block = cp >> SHIFT;
    debug_assert!(block < BLOCK_COUNT as u32);
    let top = (block >> LOWER_BITS) as usize;
    let bottom = (block & LOWER_MASK) as usize;
    let level2_idx = LEVEL1_TABLE[top] as usize;
    let leaf_idx = LEVEL2_TABLES[level2_idx * LOWER_SIZE + bottom] as usize;
    let leaf = load_leaf(leaf_idx);
    let offset = (cp & BLOCK_MASK) as u16;
    leaf_value(leaf, offset)
}

const fn ascii_table() -> [u8; 128] {
    let mut table = [0u8; 128];
    let mut c = b'A';
    while c <= b'Z' {
        table[c as usize] = IDENTIFIER_START | IDENTIFIER_CONTINUE;
        c += 1;
    }
    c = b'a';
    while c <= b'z' {
        table[c as usize] = IDENTIFIER_START | IDENTIFIER_CONTINUE;
        c += 1;
    }
    c = b'0';
    while c <= b'9' {
        table[c as usize] = IDENTIFIER_CONTINUE;
        c += 1;
    }
    table[b'_' as usize] = IDENTIFIER_CONTINUE;
    table
}

/// Checks if a codepoint is a unicode identifier, defined by
/// Unicode Standard Annex #31.
///
/// This function implements the "Default Identifiers" specification,
/// specifically `UAX31-R1-1`, which does not add or modify any of the
/// character sequences or their properties. See the specification for more
/// details.
#[inline]
pub fn is_identifier(cp: &[char]) -> bool {
    if cp.is_empty() {
        return false;
    }

    if !unicode_identifier_class(cp[0]).is_start() {
        return false;
    }

    for (i, c) in cp.iter().enumerate() {
        if !unicode_identifier_class(*c).is_continue() {
            // the two special characters are only allowed in the
            // middle, not the end.
            if (*c != '\u{200c}' && *c != '\u{200d}') || i + 1 == cp.len() {
                return false;
            }
        }
    }

    true
}

/// Checks if a given string is a unicode identifier, defined by Unicode
/// Standard Annex #31.
///
/// This function implements the "Default Identifiers" specification,
/// specifically `UAX31-R1-1`, which does not add or modify any of the
/// character sequences or their properties. See the specification for more
/// details.
#[inline]
pub fn str_is_identifier(s: &str) -> bool {
    let mut iter = s.chars();
    let Some(first) = iter.next() else {
        return false;
    };

    if !unicode_identifier_class(first).is_start() {
        return false;
    }

    let mut iter = iter.peekable();
    while let Some(c) = iter.next() {
        if !unicode_identifier_class(c).is_continue() {
            // the two special characters are only allowed in the
            // middle, not the end.
            if (c != '\u{200c}' && c != '\u{200d}') || iter.peek().is_none() {
                return false;
            }
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::{fs::File, path::PathBuf, sync::OnceLock};

    const MAX_SCALAR: usize = 0x110000;

    fn derived_identifier_table() -> &'static [u8] {
        static TABLE: OnceLock<Box<[u8]>> = OnceLock::new();
        TABLE
            .get_or_init(|| {
                let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
                let derived_path =
                    manifest_dir.join("./DerivedCoreProperties.txt");
                let file = File::open(&derived_path).unwrap_or_else(|err| {
                    panic!("failed to open {}: {err}", derived_path.display())
                });

                let parsed =
                    unicode_id_trie_rle_derived_core_properties::parse(file)
                        .unwrap_or_else(|err| {
                            panic!("failed to parse derived data: {err}")
                        });
                let mut table = vec![0u8; MAX_SCALAR];
                for (ch, props) in parsed {
                    let mut bits = 0u8;
                    for prop in props {
                        if prop.contains("ID_Start") {
                            bits |= IDENTIFIER_START;
                        }
                        if prop.contains("ID_Continue") {
                            bits |= IDENTIFIER_CONTINUE;
                        }
                    }

                    table[ch as usize] = bits;
                }

                table.into_boxed_slice()
            })
            .as_ref()
    }

    #[test]
    fn unicode_identifier_class_matches_derived_core_properties() {
        let table = derived_identifier_table();
        for cp in 0..=0x10ffff {
            let Some(ch) = char::from_u32(cp) else {
                continue;
            };
            let expected = table[ch as usize];
            if cp >= 0x100000 {
                assert_eq!(
                    expected, 0,
                    "derived data marks unsupported codepoint U+{cp:06X}"
                );
            }

            let class = unicode_identifier_class(ch);
            assert_eq!(
                class.is_start(),
                expected & IDENTIFIER_START != 0,
                "ID_Start mismatch at U+{cp:04X}"
            );
            assert_eq!(
                class.is_continue(),
                expected & IDENTIFIER_CONTINUE != 0,
                "ID_Continue mismatch at U+{cp:04X}"
            );
        }
    }

    proptest! {
        #[test]
        fn unicode_identifier_class_proptest(cp in any::<char>()) {
            let expected = derived_identifier_table()[cp as usize];
            if (cp as u32) >= 0x100000 {
                prop_assert_eq!(
                    expected, 0,
                    "derived data marks unsupported codepoint U+{:06X}",
                    cp as u32
                );
            }

            let class = unicode_identifier_class(cp);
            prop_assert_eq!(
                class.is_start(),
                expected & IDENTIFIER_START != 0,
                "ID_Start mismatch at U+{:04X}",
                cp as u32
            );
            prop_assert_eq!(
                class.is_continue(),
                expected & IDENTIFIER_CONTINUE != 0,
                "ID_Continue mismatch at U+{:04X}",
                cp as u32
            );
        }
    }

    proptest! {
        #[test]
        fn str_and_slice_identifier_agree(chars in prop::collection::vec(any::<char>(), 0..16)) {
            let string: String = chars.iter().copied().collect();
            prop_assert_eq!(
                str_is_identifier(&string),
                is_identifier(&chars),
                "str/is_identifier disagreement on {:?}",
                string
            );
        }
    }
}
