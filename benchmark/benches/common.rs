use std::{fs, path::PathBuf};

use criterion::{BenchmarkId, Criterion};

const LENGTHS: [usize; 3] = [32, 128, 512];

struct TestCase {
    len: usize,
    input: String,
}

fn load_cases(percent: u8) -> Vec<TestCase> {
    let mut cases = Vec::with_capacity(LENGTHS.len());
    for len in LENGTHS {
        let ascii_percent = 100 - percent;
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("corpus")
            .join(format!("ascii-{ascii_percent}"))
            .join(format!("len{len}.txt"));
        let raw = fs::read_to_string(&path).unwrap_or_else(|err| {
            panic!("failed to read corpus {path:?}: {err}");
        });
        // Ensure no trailing newline sneaks in; keep identifiers valid.
        let input = raw.trim_end_matches('\n').to_owned();
        let char_len = input.chars().count();
        assert!(
            char_len == len,
            "corpus {path:?} expected length {len}, got {char_len}"
        );
        let non_ascii = input.chars().filter(|c| !c.is_ascii()).count();
        let diff = (non_ascii as isize)
            - ((len * percent as usize + 50) / 100) as isize;
        assert!(
            diff.abs() <= 1,
            "corpus {path:?} non-ascii count {non_ascii} too far from target {percent}% of {len}"
        );

        cases.push(TestCase { len, input });
    }

    cases
}

#[allow(dead_code)]
pub fn bench_full_suite(c: &mut Criterion, percent: u8) {
    let ascii_percent = 100 - percent;
    let label = format!("{ascii_percent}% ascii");
    let cases = load_cases(percent);
    let mut group = c.benchmark_group(label);
    for case in &cases {
        group.bench_with_input(
            BenchmarkId::new("baseline", case.len),
            &case.input,
            |b, i| b.iter(|| baseline::str_is_identifier(i)),
        );
        group.bench_with_input(
            BenchmarkId::new("delta-encoded", case.len),
            &case.input,
            |b, i| b.iter(|| delta_encoded::str_is_identifier(i)),
        );
        group.bench_with_input(
            BenchmarkId::new("run-indexed", case.len),
            &case.input,
            |b, i| b.iter(|| run_indexed::str_is_identifier(i)),
        );
        group.bench_with_input(
            BenchmarkId::new("unicode-id-start", case.len),
            &case.input,
            |b, i| b.iter(|| unicode_id_start_harness::str_is_identifier(i)),
        );
        group.bench_with_input(
            BenchmarkId::new("unicode-id-trie-rle", case.len),
            &case.input,
            |b, i| b.iter(|| unicode_id_trie_rle::str_is_identifier(i)),
        );
    }
    group.finish();
}
