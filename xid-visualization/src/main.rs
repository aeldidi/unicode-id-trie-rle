//! Generates an SVG visualization of the XID_Start and XID_Continue-only
//! distribution in the Unicode codepoint space.
//!
//! The image is a dense 1024x1088 grid with one pixel per codepoint.
//! Codepoints are arranged from left-to-right with 1024 per line.
//!
//! CLI usage:
//! - `xid-visualization [output.svg]` (defaults to `xid-visualization.svg`)
//! - `cargo run -p xid-visualization -- [output.svg]`
//!
//! The tool prints the legend, mapping, and counts to stdout.

use std::{
    collections::{BTreeMap, HashSet},
    env,
    fs::File,
    io::{self, BufWriter, Write},
    path::{Path, PathBuf},
};

use unicode_id_trie_rle_derived_core_properties as derived_core_properties;

const MAX_CODEPOINT: u32 = 0x10FFFF;
const IMAGE_WIDTH: u32 = 1024;
const IMAGE_HEIGHT: u32 = ((MAX_CODEPOINT + 1) as u32) / IMAGE_WIDTH;
const DEFAULT_OUTPUT: &str = "xid-visualization.svg";

const USAGE: &str = "Usage: xid-visualization [output.svg]\n\nDefaults:\n  output xid-visualization.svg";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Class {
    Background,
    ContinueOnly,
    Start,
}

#[derive(Clone, Copy)]
struct Rgb {
    r: u8,
    g: u8,
    b: u8,
}

impl Rgb {
    fn hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }
}

struct Palette {
    background: Rgb,
    continue_only: Rgb,
    start: Rgb,
}

struct Args {
    output: PathBuf,
}

struct Stats {
    start: usize,
    continue_count: usize,
    start_only: usize,
    continue_only: usize,
    none: usize,
}

fn main() {
    let args = match parse_args() {
        Ok(args) => args,
        Err(err) => {
            eprintln!("error: {err}\n\n{USAGE}");
            std::process::exit(2);
        }
    };

    if let Err(err) = run(&args) {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run(args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let derived_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("DerivedCoreProperties.txt");
    let props = {
        let file = File::open(&derived_path)?;
        derived_core_properties::parse(file)?
    };

    let palette = Palette {
        background: Rgb {
            r: 0x00,
            g: 0x00,
            b: 0x00,
        },
        continue_only: Rgb {
            r: 0xff,
            g: 0xb4,
            b: 0x00,
        },
        start: Rgb {
            r: 0x00,
            g: 0x66,
            b: 0xff,
        },
    };

    write_svg(&args.output, &props, &palette)?;

    let stats = compute_stats(&props);
    print_report(args, &derived_path, &palette, &stats);
    Ok(())
}

fn parse_args() -> Result<Args, String> {
    let mut output: Option<PathBuf> = None;
    let mut args = env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            _ => {
                if arg.starts_with('-') {
                    return Err(format!("unknown flag: {arg}"));
                }
                if output.is_none() {
                    output = Some(PathBuf::from(arg));
                } else {
                    return Err(format!("unexpected argument: {arg}"));
                }
            }
        }
    }

    Ok(Args {
        output: output.unwrap_or_else(|| PathBuf::from(DEFAULT_OUTPUT)),
    })
}

fn write_svg(
    path: &Path,
    props: &BTreeMap<u32, HashSet<String>>,
    palette: &Palette,
) -> io::Result<()> {
    let mut writer = BufWriter::new(File::create(path)?);

    let background = palette.background.hex();
    let start = palette.start.hex();
    let cont = palette.continue_only.hex();

    writeln!(writer, "<?xml version=\"1.0\" encoding=\"UTF-8\"?>")?;
    writeln!(
        writer,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\" shape-rendering=\"crispEdges\">",
        IMAGE_WIDTH, IMAGE_HEIGHT, IMAGE_WIDTH, IMAGE_HEIGHT
    )?;
    writeln!(writer, "  <defs>")?;
    writeln!(writer, "    <style>")?;
    writeln!(writer, "      .start {{ fill: {start}; }}")?;
    writeln!(writer, "      .cont {{ fill: {cont}; }}")?;
    writeln!(writer, "    </style>")?;
    writeln!(writer, "  </defs>")?;
    writeln!(
        writer,
        "  <rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" fill=\"{}\"/>",
        IMAGE_WIDTH, IMAGE_HEIGHT, background
    )?;

    for row in 0..IMAGE_HEIGHT {
        let y = row;
        let row_base = row * IMAGE_WIDTH;
        let mut run_start = 0u32;
        let mut run_class = 'run_class: {
            let set = if let Some(x) = props.get(&row_base) {
                x
            } else {
                break 'run_class Class::Background;
            };
            if set.contains("XID_Start") {
                Class::Start
            } else if set.contains("XID_Continue") {
                Class::ContinueOnly
            } else {
                Class::Background
            }
        };

        for col in 1..IMAGE_WIDTH {
            let class = 'class: {
                let set = if let Some(x) = props.get(&(row_base + col)) {
                    x
                } else {
                    break 'class Class::Background;
                };
                if set.contains("XID_Start") {
                    Class::Start
                } else if set.contains("XID_Continue") {
                    Class::ContinueOnly
                } else {
                    Class::Background
                }
            };
            if class != run_class {
                emit_run(&mut writer, run_class, y, run_start, col)?;
                run_class = class;
                run_start = col;
            }
        }

        emit_run(&mut writer, run_class, y, run_start, IMAGE_WIDTH)?;
    }

    writeln!(writer, "</svg>")?;
    Ok(())
}

fn emit_run<W: Write>(
    writer: &mut W,
    class: Class,
    y: u32,
    start_col: u32,
    end_col: u32,
) -> io::Result<()> {
    let class_name = match class {
        Class::Start => "start",
        Class::ContinueOnly => "cont",
        Class::Background => return Ok(()),
    };

    let width = end_col - start_col;
    writeln!(
        writer,
        "  <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"1\" class=\"{}\"/>",
        start_col, y, width, class_name
    )
}

fn compute_stats(props: &BTreeMap<u32, HashSet<String>>) -> Stats {
    let mut stats = Stats {
        start: 0,
        continue_count: 0,
        start_only: 0,
        continue_only: 0,
        none: 0,
    };

    for cp in 0..=MAX_CODEPOINT {
        let cp_props = &props.get(&cp);
        let has_start = cp_props.is_some_and(|x| x.contains("XID_Start"));
        let has_continue =
            cp_props.is_some_and(|x| x.contains("XID_Continue"));

        if has_start {
            stats.start += 1;
        }
        if has_continue {
            stats.continue_count += 1;
        }
        if has_start && !has_continue {
            stats.start_only += 1;
        }
        if has_continue && !has_start {
            stats.continue_only += 1;
        }
        if !has_start && !has_continue {
            stats.none += 1;
        }
    }

    stats
}

fn print_report(
    args: &Args,
    derived_path: &Path,
    palette: &Palette,
    stats: &Stats,
) {
    let background = palette.background.hex();
    let start = palette.start.hex();
    let cont = palette.continue_only.hex();

    println!("Output: {}", args.output.display());
    println!("Derived data: {}", derived_path.display());
    println!(
        "Image size: {}x{} px (one pixel per codepoint).",
        IMAGE_WIDTH, IMAGE_HEIGHT
    );
    println!(
        "Mapping: 1024 codepoints per row, left-to-right, then the next line (x = cp & 0x3FF, y = cp >> 10)."
    );
    println!("Colors:");
    println!("  XID_Start: {start}");
    println!("  XID_Continue only (not XID_Start): {cont}");
    println!("  None: {background}");
    println!("Counts:");
    println!("  XID_Start: {}", stats.start);
    println!("  XID_Continue only: {}", stats.continue_only);
    println!("  Codepoints with neither: {}", stats.none);
    println!(
        "  XID_Continue total (includes XID_Start): {}",
        stats.continue_count
    );
    println!("  XID_Start only: {}", stats.start_only);
}
