use std::{
    collections::HashMap,
    env,
    error::Error,
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
};

const MAX_CODEPOINT: u32 = 0x0fffff; // decoder ignores codepoints beyond this
const START_CODEPOINT: u32 = 0x80;
const SHIFT: u32 = 10;
const TOP_BITS: u32 = 6;
const BYTES_PER_LINE: usize = 12;
const INDEX_BYTES_PER_LINE: usize = 16;

fn build_table() -> Result<Vec<u8>, Box<dyn Error>> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let derived = manifest_dir.join("./DerivedCoreProperties.txt");
    println!("cargo:rerun-if-changed={}", derived.display());
    println!("cargo:rerun-if-changed=build.rs");

    let file = File::open(&derived)?;
    let parsed = unicode_id_trie_rle_derived_core_properties::parse(file)?;

    let mut table = vec![0u8; (MAX_CODEPOINT + 1) as usize];
    for (ch, props) in parsed {
        if (ch as u32) > MAX_CODEPOINT {
            continue;
        }

        let mut bits = 0u8;
        for prop in props {
            if prop.contains("ID_Start") {
                bits |= 1;
            }
            if prop.contains("ID_Continue") {
                bits |= 2;
            }
        }
        table[ch as usize] = bits;
    }

    Ok(table)
}

fn build_runs(table: &[u8]) -> Vec<(u32, u8)> {
    let mut runs = Vec::with_capacity(1024);
    let end_cp = MAX_CODEPOINT + 1; // sentinel run start

    let mut run_start = START_CODEPOINT;
    let mut current = table[run_start as usize];
    for cp in (START_CODEPOINT + 1)..=end_cp {
        let value = if cp <= MAX_CODEPOINT {
            table[cp as usize]
        } else {
            0
        };

        if value != current {
            runs.push((run_start, current));
            run_start = cp;
            current = value;
        }
    }
    runs.push((run_start, current));
    if runs.last().map_or(true, |(s, _)| *s != end_cp) {
        runs.push((end_cp, 0));
    }

    runs
}

fn build_block_index(runs: &[(u32, u8)], block_count: u32) -> Vec<usize> {
    let mut block_index = vec![0usize; block_count as usize];
    let mut run_idx = 0usize;
    for block in 0..block_count {
        let block_start = block << SHIFT;
        while run_idx + 1 < runs.len() && runs[run_idx + 1].0 <= block_start {
            run_idx += 1;
        }
        block_index[block as usize] = run_idx;
    }

    block_index
}

fn emit_u8_array(
    writer: &mut BufWriter<File>,
    name: &str,
    data: &[u8],
    per_line: usize,
) -> Result<(), Box<dyn Error>> {
    writeln!(writer, "pub(crate) static {name}: [u8; {}] = [", data.len())?;
    for (idx, byte) in data.iter().enumerate() {
        if idx % per_line == 0 {
            write!(writer, "\t")?;
        }
        write!(writer, "0x{byte:02x},")?;
        if idx % per_line == per_line - 1 || idx + 1 == data.len() {
            writeln!(writer)?;
        } else {
            write!(writer, " ")?;
        }
    }
    writeln!(writer, "];")?;
    Ok(())
}

fn emit_u16_array(
    writer: &mut BufWriter<File>,
    name: &str,
    data: &[u16],
    per_line: usize,
) -> Result<(), Box<dyn Error>> {
    writeln!(
        writer,
        "pub(crate) static {name}: [u16; {}] = [",
        data.len()
    )?;
    for (idx, val) in data.iter().enumerate() {
        if idx % per_line == 0 {
            write!(writer, "\t")?;
        }
        write!(writer, "0x{val:04x},")?;
        if idx % per_line == per_line - 1 || idx + 1 == data.len() {
            writeln!(writer)?;
        } else {
            write!(writer, " ")?;
        }
    }
    writeln!(writer, "];")?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let table = build_table()?;
    let runs = build_runs(&table);
    assert!(
        runs.len() < u16::MAX as usize,
        "run table too large for u16 index: {}",
        runs.len()
    );

    let block_count = (MAX_CODEPOINT >> SHIFT) + 1;
    let block_index = build_block_index(&runs, block_count);
    let block_bits = 32 - (block_count - 1).leading_zeros();
    assert!(
        block_bits > TOP_BITS,
        "TOP_BITS ({TOP_BITS}) must be smaller than block bit width ({block_bits})"
    );
    let lower_bits = block_bits - TOP_BITS;
    let lower_size = 1usize << lower_bits;
    let top_size = 1usize << TOP_BITS;

    let mut leaf_runs: Vec<(u16, u8)> = Vec::new();
    let mut leaf_offsets: Vec<u16> = Vec::new(); // start index into leaf_runs
    let mut leaf_map: HashMap<Vec<(u16, u8)>, u16> = HashMap::new();

    let mut block_to_leaf = Vec::with_capacity(block_count as usize);
    for block in 0..block_count {
        let block_start = block << SHIFT;
        let block_end = ((block + 1) << SHIFT).min(MAX_CODEPOINT + 1);

        let mut idx = block_index[block as usize];
        let mut local_runs = Vec::new();
        loop {
            let (start, value) = runs[idx];
            let next_start = runs[idx + 1].0;
            if next_start <= block_start {
                idx += 1;
                continue;
            }
            let run_from = start.max(block_start);
            if run_from < block_end {
                local_runs.push(((run_from - block_start) as u16, value));
            }
            if next_start >= block_end {
                break;
            }
            idx += 1;
        }

        local_runs.push(((block_end - block_start) as u16, 0));
        let leaf_id = if let Some(&id) = leaf_map.get(&local_runs) {
            id
        } else {
            let id =
                u16::try_from(leaf_map.len()).expect("leaf count fits in u16");
            let start = leaf_runs.len();
            leaf_offsets.push(start as u16);
            leaf_runs.extend_from_slice(&local_runs);
            leaf_map.insert(local_runs.clone(), id);
            id
        };

        block_to_leaf.push(leaf_id);
    }
    leaf_offsets.push(leaf_runs.len() as u16); // sentinel for computing leaf lengths

    let mut level2_map: HashMap<Vec<u16>, u16> = HashMap::new();
    let mut level2_tables: Vec<u16> = Vec::new();
    let mut level1_table = Vec::with_capacity(top_size);

    for top in 0..top_size {
        let mut table = vec![0u16; lower_size];
        for low in 0..lower_size {
            let block = (top << lower_bits) | low;
            table[low] = block_to_leaf[block];
        }

        let table_id = if let Some(&id) = level2_map.get(&table) {
            id
        } else {
            let id = u16::try_from(level2_map.len())
                .expect("level2 table count fits in u16");
            level2_map.insert(table.clone(), id);
            level2_tables.extend_from_slice(&table);
            id
        };
        level1_table.push(table_id);
    }

    let mut offsets = Vec::with_capacity(leaf_runs.len());
    let mut values = Vec::with_capacity(leaf_runs.len());
    for (start, value) in &leaf_runs {
        offsets.push(*start);
        values.push(*value);
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let out_path = out_dir.join("table.rs");
    let out_file = File::create(&out_path)?;
    let mut writer = BufWriter::new(out_file);

    writeln!(writer, "// Code generated by build.rs; DO NOT EDIT.")?;
    writeln!(writer, "pub(crate) const SHIFT: u32 = {SHIFT};")?;
    writeln!(
        writer,
        "pub(crate) const BLOCK_COUNT: usize = {};",
        block_count as usize
    )?;
    writeln!(writer, "pub(crate) const LOWER_BITS: u32 = {lower_bits};")?;
    writeln!(writer, "pub(crate) const LOWER_SIZE: usize = {lower_size};")?;

    emit_u16_array(
        &mut writer,
        "LEAF_OFFSETS",
        &leaf_offsets,
        INDEX_BYTES_PER_LINE / 2,
    )?;
    emit_u16_array(
        &mut writer,
        "LEAF_RUN_STARTS",
        &offsets,
        INDEX_BYTES_PER_LINE / 2,
    )?;
    emit_u8_array(&mut writer, "LEAF_RUN_VALUES", &values, BYTES_PER_LINE)?;
    emit_u16_array(
        &mut writer,
        "LEVEL2_TABLES",
        &level2_tables,
        INDEX_BYTES_PER_LINE / 2,
    )?;
    emit_u16_array(
        &mut writer,
        "LEVEL1_TABLE",
        &level1_table,
        INDEX_BYTES_PER_LINE / 2,
    )?;

    writer.flush()?;
    Ok(())
}
