//! A parser for the Unicode Data `DerivedCoreProperties.txt`.
//! Call [`parse`] to get a [BTreeMap] from codepoint to a [HashSet] of the
//! properties it has.
//!
//! This crate is considered an implementation detail of `unicode-id-trie-rle`
//! and makes no guarantees about stability or correctness.

use std::{
    collections::{BTreeMap, HashSet},
    io::{self, BufRead, BufReader},
    num::ParseIntError,
};

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("there was an error reading the file: {0}")]
    IoError(#[from] io::Error),
    #[error(
        "there was an error parsing one of the codepoints in a range: {0}"
    )]
    IntParseRangeError(#[from] ParseIntError),
    #[error("one of the codepoints was outside the valid unicode range")]
    InvalidCodepoint,
    #[error("missing ';' delimiter in line: {0}")]
    MissingDelimiter(String),
}

/// Reads in data from a `DerivedCoreProperties.txt` file into a [BTreeMap]
/// from each codepoint to a [HashSet] of that codepoint's properties.
pub fn parse<R: io::Read>(
    reader: R,
) -> Result<BTreeMap<char, HashSet<String>>, Error> {
    let mut reader = BufReader::new(reader);
    let mut buf = String::new();
    let mut result: BTreeMap<char, HashSet<String>> = BTreeMap::new();
    loop {
        buf.clear();
        if reader.read_line(&mut buf)? == 0 {
            break;
        }

        if let Some(comment_start) = buf.find('#') {
            buf.truncate(comment_start);
        }
        let trimmed = buf.trim();
        if trimmed.is_empty() {
            continue;
        }

        let Some((codepoint_range, prop_name)) = trimmed.split_once(';')
        else {
            return Err(Error::MissingDelimiter(trimmed.to_string()));
        };

        let prop_name = prop_name.trim();
        let (start_range, end_range) = parse_range(codepoint_range)?;

        for cp in start_range..=end_range {
            let cp = match char::from_u32(cp) {
                Some(x) => x,
                None => return Err(Error::InvalidCodepoint),
            };

            if let Some(x) = result.get_mut(&cp) {
                x.insert(prop_name.to_string());
            } else {
                result.insert(cp, HashSet::from_iter([prop_name.to_string()]));
            }
        }
    }

    Ok(result)
}

fn parse_range(raw: &str) -> Result<(u32, u32), ParseIntError> {
    if let Some((start, end)) = raw.split_once("..") {
        Ok((
            u32::from_str_radix(start.trim(), 16)?,
            u32::from_str_radix(end.trim(), 16)?,
        ))
    } else {
        let single = u32::from_str_radix(raw.trim(), 16)?;
        Ok((single, single))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::io::{Cursor, Read};

    fn sample_range() -> impl Strategy<Value = (u32, u32)> {
        prop_oneof![
            (0u32..=0xD7FF, 0u32..=24).prop_map(|(start, len)| {
                let end = (start + len).min(0xD7FF);
                (start, end)
            }),
            (0xE000u32..=0x0FFF0, 0u32..=24).prop_map(|(start, len)| {
                let end = (start + len).min(0x0FFFF);
                (start, end)
            }),
        ]
    }

    proptest! {
        #[test]
        fn parse_correctly_parses_codepoint_ranges_and_applies_properties(cases in prop::collection::vec(sample_range(), 1..10)) {
            let mut contents = String::from("# Initial comment that should be ignored \n");
            contents.push_str(" \n");

            let mut expected: BTreeMap<char, HashSet<String>> = BTreeMap::new();

            for (idx, (start, end)) in cases.iter().enumerate() {
                let prop_name = format!("Prop{idx}");
                contents.push_str(&format!("  {start:04X}..{end:04X}; {prop_name}  # trailing comment\n"));
                contents.push('\n');
                for cp in *start..=*end {
                    expected
                        .entry(char::from_u32(cp).unwrap())
                        .or_default()
                        .insert(prop_name.clone());
                }

                let single_prop = format!("Single{idx}");
                contents.push_str(&format!("  {start:04X}; {single_prop} # inline comment\n\n"));
                expected
                    .entry(char::from_u32(*start).unwrap())
                    .or_default()
                    .insert(single_prop);
            }

            let parsed = parse(contents.as_bytes()).unwrap();
            prop_assert_eq!(parsed, expected);
        }
    }

    proptest! {
        #[test]
        fn parse_errors_on_invalid_codepoints(
            start in 0x0FFF00u32..=0x0FFFF0,
            extend in 1u32..=0x4000
        ) {
            let end = 0x10FFFF + extend;
            let contents = format!("{start:06X}..{end:06X}; InvalidRange\n");
            let err = parse(contents.as_bytes()).unwrap_err();
            prop_assert!(matches!(err, Error::InvalidCodepoint));
        }
    }

    #[derive(Debug, Clone)]
    struct FailingReader {
        data: Vec<u8>,
        cursor: usize,
        fail_at: usize,
    }

    impl FailingReader {
        fn new(data: &[u8], fail_at: usize) -> Self {
            Self {
                data: data.to_vec(),
                cursor: 0,
                fail_at,
            }
        }
    }

    impl Read for FailingReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.cursor >= self.fail_at {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "forced read failure",
                ));
            }

            let limit = self.fail_at.min(self.data.len());
            let remaining = limit.saturating_sub(self.cursor);
            let to_copy = remaining.min(buf.len());

            if to_copy == 0 {
                self.cursor = limit;
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "forced read failure",
                ));
            }

            buf[..to_copy].copy_from_slice(
                &self.data[self.cursor..self.cursor + to_copy],
            );
            self.cursor += to_copy;
            Ok(to_copy)
        }
    }

    const IO_ERROR_INPUT: &str = "0041; PropertyWithoutNewline";

    proptest! {
        #[test]
        fn parse_propagates_io_errors(fail_after in 0usize..IO_ERROR_INPUT.len()) {
            let reader = FailingReader::new(IO_ERROR_INPUT.as_bytes(), fail_after);
            let err = parse(reader).unwrap_err();
            prop_assert!(matches!(err, Error::IoError(_)));
        }
    }

    proptest! {
        #[test]
        fn parse_doesnt_crash(
            input in prop::collection::vec(any::<u8>(), 0..100_000),
        ) {
            let cursor = Cursor::new(input);
            let _ = parse(cursor);
        }
    }

    #[test]
    fn parse_breaks_cleanly_on_empty_input() {
        assert!(parse("".as_bytes()).unwrap().is_empty());
    }

    #[test]
    fn parse_errors_when_missing_semicolon() {
        let contents =
            "  0041  # just whitespace and a comment\n0042; Valid\n";
        let err = parse(contents.as_bytes()).unwrap_err();
        assert!(matches!(err, Error::MissingDelimiter(_)));
    }
}
