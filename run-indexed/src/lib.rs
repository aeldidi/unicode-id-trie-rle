const IDENTIFIER_OTHER: u8 = 0;
const IDENTIFIER_START: u8 = 1;
const IDENTIFIER_CONTINUE: u8 = 2;
const START_CODEPOINT: u32 = 0x80;

include!(concat!(env!("OUT_DIR"), "/table.rs"));
const INDEX_MASK: u32 = (1u32 << INDEX_BITS) - 1;
const ASCII_TABLE: [u8; 128] = ascii_table();

pub struct UnicodeIdentifierClass(u8);

impl UnicodeIdentifierClass {
    #[inline]
    pub fn is_start(&self) -> bool {
        self.0 & IDENTIFIER_START != 0
    }

    #[inline]
    pub fn is_continue(&self) -> bool {
        self.0 & IDENTIFIER_CONTINUE != 0
    }
}

#[inline]
fn load_run(runs: &[u8], idx: usize) -> (u32, u8) {
    // `build.rs` appends a sentinel run so idx + 1 is always in-bounds.
    let base = idx * 3;
    let (b0, b1, b2) = unsafe {
        (
            *runs.get_unchecked(base) as u32,
            *runs.get_unchecked(base + 1) as u32,
            *runs.get_unchecked(base + 2) as u32,
        )
    };
    let start = b0 | (b1 << 8) | ((b2 & 0x1f) << 16);
    let value = (b2 >> 5) as u8 & 3;
    (start, value)
}

#[inline]
fn block_index(block: usize) -> usize {
    debug_assert!(block < BLOCK_COUNT);
    let bit_offset = (block as u32) * INDEX_BITS;
    let byte = (bit_offset >> 3) as usize;
    let shift = (bit_offset & 7) as u32;
    let chunk = u32::from_le_bytes([
        BLOCK_INDEX[byte],
        BLOCK_INDEX[byte + 1],
        BLOCK_INDEX[byte + 2],
        BLOCK_INDEX[byte + 3],
    ]);
    ((chunk >> shift) & INDEX_MASK) as usize
}

/// Returns whether the codepoint specified has the properties `ID_Start`,
/// `XID_Start` or the properties `ID_Continue` or `XID_Continue`.
#[inline]
pub fn unicode_identifier_class(cp: char) -> UnicodeIdentifierClass {
    // ASCII fast path via table to avoid unpredictable branches.
    if (cp as u32) < START_CODEPOINT {
        return UnicodeIdentifierClass(ASCII_TABLE[cp as usize]);
    }

    if cp as u32 >= 0x100000 {
        return UnicodeIdentifierClass(IDENTIFIER_OTHER);
    }

    let cp = cp as u32;
    debug_assert!(cp >= START_CODEPOINT);

    let block = (cp >> SHIFT) as usize;
    let runs = &RUNS[..];
    let mut idx = block_index(block);
    let (mut run_start, mut run_value) = load_run(runs, idx);
    let (mut next_start, mut next_value) = load_run(runs, idx + 1);
    loop {
        if cp < next_start {
            if cp >= run_start {
                return UnicodeIdentifierClass(run_value);
            }
            return UnicodeIdentifierClass(IDENTIFIER_OTHER);
        }
        idx += 1;
        run_start = next_start;
        run_value = next_value;
        let (ns, nv) = load_run(runs, idx + 1);
        next_start = ns;
        next_value = nv;
    }
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
#[inline]
pub fn is_identifier(cp: &[char]) -> bool {
    if cp.len() == 0 {
        return false;
    }

    if !unicode_identifier_class(cp[0]).is_start() {
        return false;
    }

    for (i, c) in cp.into_iter().enumerate() {
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
#[inline]
pub fn str_is_identifier(s: &str) -> bool {
    if s.len() == 0 {
        return false;
    }

    if !unicode_identifier_class(
        s.chars().nth(0).expect("we already checked that len > 0"),
    )
    .is_start()
    {
        return false;
    }

    let cp = s.chars().collect::<Vec<_>>();
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
                    manifest_dir.join("../DerivedCoreProperties.txt");
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
