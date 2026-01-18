use std::env;
use std::fs::File;
use std::io::{self, BufRead, Write};
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

    println!("\n\n|--    CAST: Columnar Agnostic Structural Transformation (Random Access *PREVIEW* v{})    --|", env!("CARGO_PKG_VERSION"));
    println!("       Author: Andrea Olivari");
    println!("       GitHub: https://github.com/AndreaLVR/CAST/tree/main/rust_random_access_PREVIEW\n");

    // ==================================================================================
    //  BACKEND SELECTION LOGIC (Hybrid Strategy)
    // ==================================================================================

    let has_7zip = try_find_7zip_path().is_some();
    let user_forced_7zip = mode_arg.as_deref() == Some("7zip");
    let user_forced_native = mode_arg.as_deref() == Some("native");

    let use_7zip_comp = if user_forced_native {
        false
    } else if user_forced_7zip {
        if !has_7zip { eprintln!("[!] Error: 7-Zip mode forced but binary not found."); std::process::exit(1); }
        true
    } else {
        has_7zip
    };

    let use_7zip_decomp = if user_forced_7zip {
        if !has_7zip { eprintln!("[!] Error: 7-Zip mode forced but binary not found."); std::process::exit(1); }
        true
    } else {
        false
    };

    let backend_label_comp = if use_7zip_comp { "7-Zip (System)" } else { "Native (xz2)" };
    let backend_label_decomp = if use_7zip_decomp { "7-Zip (System)" } else { "Native (xz2)" };

    match mode_cmd.as_str() {
        "-c" => {
            if input_path.is_empty() || output_path.is_empty() {
                eprintln!("[!]  Error: Missing input or output path for compression.");
                print_usage(exe_name);
                return;
            }
            println!("\n[*]  Starting Compression...");
            println!("       Input:       {}", input_path);
            println!("       Output:      {}", output_path);
            println!("       Backend:     {}", backend_label_comp);

            let final_dict = dict_size_bytes.unwrap_or(128 * 1024 * 1024);
            do_compress(input_path, output_path, use_multithread, final_dict, chunk_size_bytes, use_7zip_comp);

            if verify_flag {
                println!("\n------------------------------------------------");
                println!("[*]  Verifying...");
                std::thread::sleep(std::time::Duration::from_millis(500));
                do_verify_stream(output_path, use_7zip_decomp);
            }
        },
        "-d" => {
            if input_path.is_empty() || output_path.is_empty() {
                eprintln!("[!]  Error: Missing input or output path for decompression.");
                print_usage(exe_name);
                return;
            }
            if let Some((s, e)) = target_rows {
                println!("\n[*]  Starting Partial Decompression (Rows {}-{})...", s+1, e+1);
            } else {
                println!("\n[*]  Starting Full Decompression...");
            }
            println!("       Backend:     {}", backend_label_decomp);
            do_decompress(input_path, output_path, target_rows, use_7zip_decomp);
        },
        "-v" | "--verify" => {
             let target = if !input_path.is_empty() { input_path } else { &args[2] };
             if target.is_empty() {
                 eprintln!("[!] Error: Missing file to verify.");
                 print_usage(exe_name);
                 return;
             }
             println!("\n[*]  Verifying: {}", target);
             println!("       Backend:     {}", backend_label_decomp);
             do_verify_stream(target, use_7zip_decomp);
        }
        _ => print_usage(exe_name),
    }
}

// [FIX] Nuova funzione smart per stimare la media
fn estimate_avg_row_size(path: &str) -> usize {
    let f = match File::open(path) {
        Ok(f) => f,
        Err(_) => return 200,
    };
    let reader = io::BufReader::new(f);
    let mut total_bytes = 0;
    let mut count = 0;

    for line in reader.lines().take(1000) {
        if let Ok(l) = line {
            total_bytes += l.len() + 1;
            count += 1;
        }
    }

    if count == 0 { return 200; }
    std::cmp::max(1, total_bytes / count)
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
    println!(
        "\nCAST (Columnar Agnostic Structural Transformation) CLI Tool (Random Access *PREVIEW* v{})\n\
        Author: Andrea Olivari\n\
        GitHub: https://github.com/AndreaLVR/CAST/tree/main/rust_random_access_PREVIEW\n\n\
        Usage:\n  \
          {} [MODE] [INPUT] [OUTPUT] [OPTIONS]\n\n\
        Modes:\n  \
          -c <in> <out>      Compress input file to CAST format\n  \
          -d <in> <out>      Decompress CAST file to original format\n  \
          -v <file>          Verify the integrity of a CAST file\n\n\
        Options:\n  \
          --mode <TYPE>      Backend selection: 'native' or '7zip'\n                         (Default: Hybrid - 7zip for Comp, Native for Decomp)\n  \
          --multithread      Enable parallel compression for higher speed\n  \
          --chunk-size <S>   Split input in chunks (e.g., 64MB) to enable Indexing & Random Access.\n                         Default: Solid Mode (Max Compression, NO INDEX/SEEKING))\n  \
          --dict-size <S>    Set LZMA Dictionary size (Default: 128MB)\n  \
          --rows <S-E>       (Decompression) Extract only specific row range (e.g. 100-200)\n  \
          -v, --verify       (Compression) Run an immediate integrity check\n  \
          -h, --help         Show this help message\n\n\
        Examples:\n  \
          {} -c data.csv archive.cast --mode 7zip\n  \
          {} -c big.log archive.cast --chunk-size 64MB\n  \
          {} -d archive.cast partial.log --rows 25000-26000\n  \
          {} -v archive.cast",
        env!("CARGO_PKG_VERSION"),
        exe_name, exe_name, exe_name, exe_name, exe_name
    );
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
        let avg_row_size = estimate_avg_row_size(input_path);
        let estimated_rows = std::cmp::max(100, bytes / avg_row_size);

        println!("       Chunking:    ACTIVE (Target ~{} bytes)", format_bytes(bytes));
        println!("                    - Sampled Avg Row Size: {} bytes", avg_row_size);
        println!("                    - Estimated Rows/Chunk: {}", estimated_rows);

        compressor.set_chunk_size(estimated_rows);
    } else {
        println!("       Chunking:    DEFAULT (Solid or ~100k rows)");
    }

    let result = compressor.compress_stream(f_in, &mut writer, |chunk_idx, bytes_read| {
        print!("\r       Processing Chunk #{} (Read: {})... ", chunk_idx, format_bytes(bytes_read as usize));
        std::io::stdout().flush().unwrap();
    });

    match result {
        Ok((bytes_in, bytes_out)) => {
            let ratio = if bytes_out > 0 { bytes_in as f64 / bytes_out as f64 } else { 0.0 };
            println!("\n[+]  Compression completed!");
            println!("       Total Input:    {}", format_bytes(bytes_in as usize));
            println!("       Total Output:   {}", format_bytes(bytes_out as usize));
            println!("       Ratio:          {:.2}x", ratio);
            println!("       Time:           {:.2}s", start_total.elapsed().as_secs_f64());
        },
        Err(e) => eprintln!("\n[!]  Error: {}", e),
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