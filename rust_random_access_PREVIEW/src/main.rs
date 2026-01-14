use std::env;
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;
use std::time::Instant;

use cast::cast_lzma::{
    LzmaBackend,
    LzmaDecompressorBackend,
    SevenZipBackend,
    SevenZipDecompressorBackend,
    RuntimeLzmaCompressor,
    RuntimeLzmaDecompressor,
    CASTLzmaCompressor,
    CASTLzmaDecompressor,
    try_find_7zip_path
};

struct SinkWriter;
impl Write for SinkWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> { Ok(buf.len()) }
    fn flush(&mut self) -> io::Result<()> { Ok(()) }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let exe_path = Path::new(&args[0]);
    let exe_name = exe_path.file_name().and_then(|s| s.to_str()).unwrap_or("cast");

    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print_usage(exe_name);
        return;
    }

    let use_multithread = args.iter().any(|arg| arg == "--multithread");
    let verify_flag = args.iter().any(|arg| arg == "-v" || arg == "--verify");

    let mut chunk_size_bytes: Option<usize> = None;
    if let Some(pos) = args.iter().position(|arg| arg == "--chunk-size") {
        if pos + 1 < args.len() {
            let val = &args[pos+1];
            chunk_size_bytes = parse_size(val);
            if chunk_size_bytes.is_none() {
                eprintln!("[!]  Error: Invalid chunk size format.");
                std::process::exit(1);
            }
        }
    }

    let mut dict_size_bytes: Option<u32> = None;
    if let Some(pos) = args.iter().position(|arg| arg == "--dict-size") {
        if pos + 1 < args.len() {
            let val = &args[pos+1];
            if let Some(s) = parse_size(val) {
                dict_size_bytes = Some(s as u32);
            } else {
                eprintln!("[!]  Error: Invalid dict size format.");
                std::process::exit(1);
            }
        }
    }

    let mut target_rows: Option<(u64, u64)> = None;
    if let Some(pos) = args.iter().position(|arg| arg == "--rows") {
        if pos + 1 < args.len() {
            let val = &args[pos+1];
            let parts: Vec<&str> = val.split('-').collect();
            if parts.len() == 2 {
                let user_start = parts[0].parse::<u64>().unwrap_or(1);
                let user_end = parts[1].parse::<u64>().unwrap_or(u64::MAX);
                let start = user_start.saturating_sub(1);
                let end = user_end.saturating_sub(1);
                target_rows = Some((start, end));
            } else {
                eprintln!("[!] Error: Invalid rows format. Use START-END (e.g., --rows 1-1000)");
                std::process::exit(1);
            }
        }
    }

    let mut mode_arg: Option<String> = None;
    if let Some(pos) = args.iter().position(|arg| arg == "--mode") {
        if pos + 1 < args.len() {
            mode_arg = Some(args[pos+1].to_lowercase());
        }
    }

    if args.len() < 3 && !args.contains(&"-h".to_string()) {
        print_usage(exe_name);
        return;
    }

    let command_idx = args.iter().position(|a| a.starts_with("-") && (a == "-c" || a == "-d" || a == "-v")).unwrap_or(0);
    if command_idx == 0 { print_usage(exe_name); return; }

    let mode_cmd = &args[command_idx];
    let input_path = if command_idx + 1 < args.len() { &args[command_idx+1] } else { "" };
    let output_path = if command_idx + 2 < args.len() { &args[command_idx+2] } else { "" };

    println!("\n\n|--    CAST: Columnar Agnostic Structural Transformation (v{})    --|", env!("CARGO_PKG_VERSION"));

    let (use_7zip, backend_label) = match mode_arg.as_deref() {
        Some("native") => (false, "Native (xz2)".to_string()),
        Some("7zip") => {
            if let Some(path) = try_find_7zip_path() {
                (true, format!("7-Zip (External) [Found at: {}]", path))
            } else { (false, "Native (xz2) [Fallback: 7z not found]".to_string()) }
        },
        _ => {
            if let Some(path) = try_find_7zip_path() {
                (true, format!("7-Zip (External) [Found at: {}]", path))
            } else { (false, "Native (xz2) [Fallback]".to_string()) }
        }
    };

    match mode_cmd.as_str() {
        "-c" => {
            println!("\n[*]  Starting Compression...");
            println!("       Input:       {}", input_path);
            println!("       Output:      {}", output_path);
            println!("       Backend:     {}", backend_label);

            let final_dict = dict_size_bytes.unwrap_or(128 * 1024 * 1024);
            do_compress(input_path, output_path, use_multithread, final_dict, chunk_size_bytes, use_7zip);

            if verify_flag {
                println!("\n[*]  Verifying...");
                do_verify_stream(output_path, use_7zip);
            }
        },
        "-d" => {
            if let Some((s, e)) = target_rows {
                println!("\n[*]  Starting Partial Decompression (Rows {}-{})...", s+1, e+1);
            } else {
                println!("\n[*]  Starting Full Decompression...");
            }
            println!("       Backend:     {}", backend_label);
            do_decompress(input_path, output_path, target_rows, use_7zip);
        },
        "-v" | "--verify" => {
             let target = if !input_path.is_empty() { input_path } else { &args[2] };
             println!("\n[*]  Verifying: {}", target);
             do_verify_stream(target, use_7zip);
        }
        _ => print_usage(exe_name),
    }
}

fn parse_size(input: &str) -> Option<usize> {
    let input = input.trim().to_uppercase();
    let digits: String = input.chars().take_while(|c| c.is_digit(10)).collect();
    let unit_part: String = input.chars().skip(digits.len()).collect();
    if digits.is_empty() { return None; }
    let num = digits.parse::<usize>().ok()?;
    match unit_part.trim() {
        "GB" | "G" => Some(num * 1024 * 1024 * 1024),
        "MB" | "M" => Some(num * 1024 * 1024),
        "KB" | "K" => Some(num * 1024),
        "B"  | ""  => Some(num),
        _ => None,
    }
}

fn format_bytes(n: usize) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 { result.push(','); }
        result.push(c);
    }
    format!("{} bytes", result.chars().rev().collect::<String>())
}

fn print_usage(exe_name: &str) {
    println!("Usage: {} -c <in> <out> [OPTIONS]", exe_name);
}

fn do_compress(input_path: &str, output_path: &str, multithread: bool, dict_size: u32, chunk_bytes: Option<usize>, use_7zip: bool) {
    let start_total = Instant::now();
    let f_in = File::open(input_path).expect("Error opening input");
    let f_out = File::create(output_path).expect("Error creating output");
    let mut writer = std::io::BufWriter::with_capacity(1024 * 1024, f_out);

    let backend = if use_7zip {
        RuntimeLzmaCompressor::SevenZip(SevenZipBackend::new(dict_size))
    } else {
        RuntimeLzmaCompressor::Native(LzmaBackend::new(multithread, dict_size))
    };

    let mut compressor = CASTLzmaCompressor::new(backend);

    if let Some(bytes) = chunk_bytes {
        let estimated_rows = std::cmp::max(100, bytes / 200);
        println!("       Chunking:    ACTIVE (Target ~{} bytes -> ~{} rows/block)", bytes, estimated_rows);
        compressor.set_chunk_size(estimated_rows);
    } else {
        println!("       Chunking:    DEFAULT (Solid or ~200k rows)");
    }

    match compressor.compress_stream(f_in, &mut writer) {
        Ok((bytes_in, bytes_out)) => {
            let ratio = if bytes_out > 0 { bytes_in as f64 / bytes_out as f64 } else { 0.0 };
            println!("\n[+]  Compression completed!");
            println!("       Total Input:    {}", format_bytes(bytes_in as usize));
            println!("       Total Output:   {}", format_bytes(bytes_out as usize));
            println!("       Ratio:          {:.2}x", ratio);
            println!("       Time:           {:.2}s", start_total.elapsed().as_secs_f64());
        },
        Err(e) => eprintln!("[!]  Error: {}", e),
    }
}

fn do_decompress(input_path: &str, output_path: &str, target_rows: Option<(u64, u64)>, use_7zip: bool) {
    let start = Instant::now();
    let f_in = File::open(input_path).expect("Error opening archive");
    let f_out = File::create(output_path).expect("Error creating output");
    let mut writer = std::io::BufWriter::with_capacity(4 * 1024 * 1024, f_out);

    let backend = if use_7zip {
        RuntimeLzmaDecompressor::SevenZip(SevenZipDecompressorBackend)
    } else {
        RuntimeLzmaDecompressor::Native(LzmaDecompressorBackend)
    };

    let decompressor = CASTLzmaDecompressor::new(backend);

    match decompressor.decompress_stream(f_in, &mut writer, target_rows) {
        Ok(_) => {
             writer.flush().unwrap();
             println!("[+]  Decompression done in {:.2}s", start.elapsed().as_secs_f64());
        },
        Err(e) => eprintln!("[!]  Error: {}", e),
    }
}

fn do_verify_stream(input_path: &str, use_7zip: bool) {
    let f_in = File::open(input_path).expect("Error opening archive");
    let backend = if use_7zip {
        RuntimeLzmaDecompressor::SevenZip(SevenZipDecompressorBackend)
    } else {
        RuntimeLzmaDecompressor::Native(LzmaDecompressorBackend)
    };
    let decompressor = CASTLzmaDecompressor::new(backend);
    let mut sink = SinkWriter;

    match decompressor.decompress_stream(f_in, &mut sink, None) {
        Ok(_) => println!("[+]  Integrity Verified."),
        Err(e) => println!("[!]  Verification Failed: {}", e),
    }
}