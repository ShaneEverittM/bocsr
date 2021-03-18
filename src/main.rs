use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, LineWriter, Write};
use std::process::Command;

use anyhow::anyhow;
use clap::App;
use itertools::Itertools;

use bocs::{cms::CountMinSketch, parser::Parser};

static CONFIDENCE: f64 = 99.0;

fn main() -> Result<(), anyhow::Error> {
    // Initialize paths
    let quads_path = &format!(
        "{}/epp-quads-{}.txt",
        std::env::temp_dir()
            .to_str()
            .ok_or_else(|| anyhow!("Invalid path"))?,
        std::process::id()
    );
    let unique_quads_path = &format!(
        "{}/epp-unique-quads-{}.txt",
        std::env::temp_dir()
            .to_str()
            .ok_or_else(|| anyhow!("Invalid path"))?,
        std::process::id()
    );

    // Read from cli
    let (k, exponent, out) = init_cli()?;

    // Get stdin_handle
    let stdin = std::io::stdin();
    let mut stdin_handle = stdin.lock();

    // Configure CMS
    let e = 1.0 / u32::pow(10, exponent) as f64;
    let mut cms = CountMinSketch::new(e, CONFIDENCE);

    // Create parser
    let mut parser = Parser::new();

    // Create temp buffer file to hold seen uv:op pairs
    let mut buffer_file = LineWriter::new(
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .open(quads_path)?,
    );

    let mut count = 0;

    // Parse info from BLANT
    while let Some(cms_info) = parser.parse_cms(&mut stdin_handle)? {
        count += 1;
        // Store uv:c:op in buffer file
        buffer_file
            .write_all(format!("{} {} {}\n", cms_info.uv, cms_info.c, cms_info.op).as_bytes())?;

        // Create uv:op pair and put it in CMS
        let uvop = format!("{}:{}", cms_info.uv, cms_info.op);
        cms.put(&uvop);
    }

    let range = (e * count as f64).floor() as u64;

    let mut log_file = OpenOptions::new()
        .write(true)
        .create(true)
        .open(&format!("{}/epp_stats.txt", out))?;

    writeln!(
        log_file,
        "Covered {} lines of input with k={}, e={} and a range of {}",
        count, k, e, range
    )?;

    // Use /usr/bin/sort to sort the seen uv:op pairs and eliminate duplicates
    Command::new("sort")
        .args(&["-u", "-k", "1", "-o", unique_quads_path, quads_path])
        .output()?;

    // Buffered reader to read in unique, seen, uv:op pairs to eliminate noise in the CMS
    let mut seen = BufReader::new(File::open(unique_quads_path)?);

    // Buffers
    let mut line = String::new();
    let mut output = String::new();
    let mut cur_uv = String::new();

    while let Ok(bytes) = seen.read_line(&mut line) {
        // Skip empty lines
        if bytes == 0 {
            break;
        }

        // Split into fields
        let (uv, c, op) = line
            .split_whitespace()
            .collect_tuple()
            .ok_or_else(|| anyhow!("Missing fields"))?;

        // If we see a new uv pair, dump output, move on
        if uv != cur_uv {
            if !cur_uv.is_empty() {
                println!("{}", output);
            }
            cur_uv = uv.to_owned();
            output = format!("{} {}", uv, c);
        }

        // If it in the CMS, add it to output associated with current uv pair
        if let Some(pred) = cms.get(&format!("{}:{}", uv, op)) {
            output += &format!("\t{}:{} {}", k, op, pred);
        }

        // Clear buffer
        line.clear();
    }
    println!("{}", output);

    // Clean up temp files
    std::fs::remove_file(unique_quads_path)?;
    std::fs::remove_file(quads_path)?;

    Ok(())
}

fn init_cli() -> Result<(usize, u32, String), anyhow::Error> {
    let matches = App::new("EPP")
        .version("0.4")
        .author("Shane Murphy, Elliott Allison, Maaz Adeeb")
        .arg_from_usage("-k <NUMBER> 'Sets the k-value that was used in BLANT'")
        .arg_from_usage("-e <NUMBER> 'Sets the error_rate to 1^-<NUMBER>'")
        .args_from_usage("-o <DIR> 'Sets the output dir")
        .get_matches();

    let k = matches
        .value_of("k")
        .expect("Must supply k value")
        .parse::<usize>()?;

    let e = matches
        .value_of("e")
        .expect("Must supply e value")
        .parse::<u32>()?;

    let out = matches
        .value_of("o")
        .expect("Must supply o value")
        .parse::<String>()?;

    Ok((k, e, out))
}
