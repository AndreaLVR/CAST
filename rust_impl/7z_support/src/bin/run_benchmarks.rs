use std::env;
use std::fs::File;
use std::io::{self, Read, Write, BufRead, BufReader};
use std::path::Path;
use std::time::Instant;
use crc32fast::Hasher;

// Import struct and the 7z helper function making it accessible
use cast::cast::{
    CASTCompressor,
    CASTDecompressor,
    compress_with_7z
};

// Struct to store results for final ranking
struct BenchmarkResult {
    name: String,
    size: usize,
    time: f64,
}

fn main() {
    let args: Vec<String> = env::args().collect();

    // [FIX] Added initial helper check
    if args.len() < 2 {
        print_bench_usage();
        return;
    }

    // 1. Parsing --chunk-size <SIZE>
    let mut chunk_size_bytes: Option<usize> = None;
    if let Some(pos) = args.iter().position(|arg| arg == "--chunk-size") {
        if pos + 1 < args.len() {
            let val = &args[pos+1];
            chunk_size_bytes = parse_size(val);
            if chunk_size_bytes.is_none() {
                eprintln!("[!]  Error: Invalid chunk size format: '{}'.", val);
                std::process::exit(1);
            }
        }
    }

    // 2. Parsing --dict-size <SIZE> (NEW)
    let mut dict_size_bytes: u32 = 128 * 1024 * 1024; // Default 128 MB
    if let Some(pos) = args.iter().position(|arg| arg == "--dict-size") {
        if pos + 1 < args.len() {
            let val = &args[pos+1];
            if let Some(s) = parse_size(val) {
                dict_size_bytes = s as u32;
            } else {
                eprintln!("[!]  Error: Invalid dict size format: '{}'.", val);
                std::process::exit(1);
            }
        }
    }

    // 3. Parsing --list
    let list_path_opt = args.windows(2)
        .find(|w| w[0] == "--list")
        .map(|w| w[1].clone());

    if list_path_opt.is_none() {
        eprintln!("[!]  ERROR: Missing '--list <file.txt>'");
        print_bench_usage(); // Suggest usage on error
        std::process::exit(1);
    }
    let list_path = list_path_opt.unwrap();

    // 4. Parsing --compare-with
    let competitors_opt = args.windows(2)
        .find(|w| w[0] == "--compare-with")
        .map(|w| w[1].clone());

    if competitors_opt.is_none() {
        eprintln!("[!]  ERROR: Missing '--compare-with <algos>'");
        print_bench_usage(); // Suggest usage on error
        std::process::exit(1);
    }
    let competitors_str = competitors_opt.unwrap();

    let competitors: Vec<&str> = if competitors_str == "all" {
        vec!["lzma2", "brotli", "zstd"]
    } else {
        competitors_str.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect()
    };

    if competitors.is_empty() {
        eprintln!("[!]  ERROR: Competitor list is empty.");
        std::process::exit(1);
    }

    println!("\n\n|--   CAST: Columnar Agnostic Structural Transformation   --|\n");
    // --- LOAD FILE LIST ---
    let mut files_to_test = Vec::new();
    println!("\n[*]  Reading list: {}", list_path);
    if let Ok(file) = File::open(&list_path) {
        for line in BufReader::new(file).lines() {
            if let Ok(l) = line {
                let t = l.trim();
                if !t.is_empty() && !t.starts_with('#') { files_to_test.push(t.to_string()); }
            }
        }
    } else {
        eprintln!("[!]  Cannot open list file.");
        std::process::exit(1);
    }

    if files_to_test.is_empty() {
        eprintln!("[!]  No valid files found in list.");
        std::process::exit(1);
    }

    // --- SUITE INFO ---
    let threads = num_cpus::get();
    println!("\nBENCHMARK SUITE");
    println!("--------------------------------------------------");
    println!("Mode:               MAX PERFORMANCE (Auto-Multithreaded: {} cores)", threads);
    if let Some(cs) = chunk_size_bytes {
        println!("CAST Chunking:      ACTIVE ({} per block)", format_bytes(cs));
    } else {
        println!("CAST Chunking:      DISABLED (Global Optimization)");
    }
    println!("LZMA Dict Size:     {}", format_bytes(dict_size_bytes as usize));
    println!("Competitors:        {:?} (Always Solid/Global)", competitors);
    println!("Files to test:      {}", files_to_test.len());
    println!("--------------------------------------------------\n");

    for file_path in files_to_test {
        if !Path::new(&file_path).exists() {
            eprintln!("[!]  SKIP (Not found): {}", file_path);
            continue;
        }

        println!("- FILE: {}", file_path);
        let metadata = std::fs::metadata(&file_path).unwrap();
        let file_len = metadata.len() as usize;
        println!("  Original size: {}", format_bytes(file_len));

        // --- PREVIEW SECTION ---
        if let Ok(f) = File::open(&file_path) {
            println!("  Preview (First 6 lines):");
            let reader = BufReader::new(f);
            for (i, line) in reader.lines().take(6).enumerate() {
                match line {
                    Ok(l) => {
                        let chars: Vec<char> = l.chars().collect();
                        let display = if chars.len() > 100 {
                            chars.into_iter().take(100).collect::<String>() + "..."
                        } else {
                            l
                        };
                        println!("    {}. {}", i + 1, display);
                    },
                    Err(_) => {
                        println!("    [!] Binary or Non-UTF8 content detected.");
                        break;
                    }
                }
            }
        }
        println!("{}", "-".repeat(60));

        let mut results = Vec::new();

        // =========================================================
        // 1. CAST EXECUTION (Chunked or Solid)
        // =========================================================
        if let Some(chunk_size) = chunk_size_bytes {
            // CAST Chunked (Pass dict_size)
            run_cast_chunked(&file_path, chunk_size, file_len, dict_size_bytes, &mut results);
        } else {
            // CAST Solid (Pass dict_size)
            let data = match std::fs::read(&file_path) {
                Ok(d) => d,
                Err(e) => { eprintln!("[!]  Read Error (CAST): {}", e); Vec::new() }
            };
            if !data.is_empty() {
                run_cast_solid(&data, dict_size_bytes, &mut results);
            }
        }

        // =========================================================
        // 2. COMPETITORS EXECUTION (Always Solid)
        // =========================================================
        if !competitors.is_empty() {
             let full_data = match std::fs::read(&file_path) {
                Ok(d) => d,
                Err(e) => { eprintln!("[!]  Read Error (Competitors): {}", e); Vec::new() }
            };

            if !full_data.is_empty() {
                for algo in &competitors {
                    // Pass dict_size to maintain parity with LZMA
                    run_competitor_solid(algo, &full_data, dict_size_bytes, &mut results);
                }
            }
        }

        // --- PRINT SUMMARY AND WINNER ---
        if results.is_empty() {
            println!("No algorithm completed the compression.");
            continue;
        }

        results.sort_by_key(|r| r.size);

        let winner = &results[0];
        let winner_size = winner.size;
        let winner_name = &winner.name;

        println!("{}", "-".repeat(70));

        for (i, res) in results.iter().enumerate() {
            let ratio = if res.size > 0 { file_len as f64 / res.size as f64 } else { 0.0 };
            let diff_vs_winner = res.size as i64 - winner_size as i64;
            let diff_str = if diff_vs_winner > 0 {
                format!("(+{} bytes)", format_num_simple(diff_vs_winner as usize))
            } else {
                "(WINNER)".to_string()
            };

            println!("{}. {:<15} : {:>15} | Ratio: {:.2}x | Time: {:.2}s | {}",
                i + 1,
                res.name,
                format_bytes(res.size),
                ratio,
                res.time,
                diff_str
            );
        }
        println!("{}", "-".repeat(70));

        // Final verdict
        if let Some(cast_res) = results.iter().find(|r| r.name.contains("CAST")) {
            if winner_name.contains("CAST") {
                if results.len() > 1 {
                    let runner_up_size = results[1].size;
                    let delta = runner_up_size - winner_size;
                    let improvement = (delta as f64 / runner_up_size as f64) * 100.0;
                    println!("RESULT: CAST WINS! Savings: {} bytes (+{:.2}%)", format_num_simple(delta), improvement);
                } else {
                    println!("RESULT: CAST WINS! (Sole competitor)");
                }
            } else {
                let delta = cast_res.size - winner_size;
                println!("RESULT: {} wins. CAST loses by {} bytes.", winner_name, format_num_simple(delta));
            }
        } else {
            println!("RESULT: {} wins. (CAST not present)", winner_name);
        }

        println!("\n");
    }
}

// --- CAST LOGIC ---

fn run_cast_solid(data: &[u8], dict_size: u32, results: &mut Vec<BenchmarkResult>) {
    let orig_len = data.len();
    print!("\n[*] Running CAST (Global)...");
    io::stdout().flush().unwrap();

    let start = Instant::now();
    // CHANGED: Pass dict_size
    let mut compressor = CASTCompressor::new(true, dict_size);
    let (r, i, v, flag, _) = compressor.compress(data);
    let duration = start.elapsed().as_secs_f64();
    let size = 17 + r.len() + i.len() + v.len();

    print_result(duration, size, orig_len);
    results.push(BenchmarkResult { name: "CAST (Global)".to_string(), size, time: duration });

    // Verify
    print!("[*] Verifying integrity... ");
    io::stdout().flush().unwrap();
    let mut h = Hasher::new();
    h.update(data);
    let expected_crc = h.finalize();
    let decompressor = CASTDecompressor;

    // [FIX] Handle Result type
    let check = std::panic::catch_unwind(|| {
        decompressor.decompress(&r, &i, &v, expected_crc, flag)
    });
    match check {
        Ok(Ok(res)) => if res == data { println!("[+] OK."); } else { println!("[!] FAIL (Mismatch)."); },
        Ok(Err(e)) => println!("[!] ERROR: {}", e),
        Err(_) => println!("[!] CRASH."),
    }
}

fn run_cast_chunked(file_path: &str, chunk_size: usize, file_len: usize, dict_size: u32, results: &mut Vec<BenchmarkResult>) {
    print!("\n[*] Running CAST (Chunked)...");
    io::stdout().flush().unwrap();

    let mut f_in = File::open(file_path).expect("Error opening file");
    let mut buffer = vec![0u8; chunk_size];

    let mut total_time = 0.0;
    let mut total_size = 0;
    let mut chunks = 0;
    let mut verify_ok = true;

    loop {
        // Read chunk
        let mut current_read = 0;
        while current_read < chunk_size {
            let n = f_in.read(&mut buffer[current_read..]).unwrap();
            if n == 0 { break; }
            current_read += n;
        }
        if current_read == 0 { break; }

        let chunk_data = &buffer[0..current_read];
        chunks += 1;

        // Compress
        let start = Instant::now();
        // CHANGED: Pass dict_size
        let mut compressor = CASTCompressor::new(true, dict_size);
        let (r, i, v, flag, _) = compressor.compress(chunk_data);
        total_time += start.elapsed().as_secs_f64();

        // Accumulate Size (Header + Body)
        let chunk_output_size = 17 + r.len() + i.len() + v.len();
        total_size += chunk_output_size;

        // Immediate Verification
        let mut h = Hasher::new();
        h.update(chunk_data);
        let expected_crc = h.finalize();
        let decompressor = CASTDecompressor;

        // [FIX] Handle Result type
        let check = std::panic::catch_unwind(|| {
            decompressor.decompress(&r, &i, &v, expected_crc, flag)
        });
        match check {
            Ok(Ok(restored)) => if restored != chunk_data { verify_ok = false; },
            Ok(Err(_)) => { verify_ok = false; },
            Err(_) => { verify_ok = false; }
        }
    }

    print_result(total_time, total_size, file_len);
    if verify_ok { println!("    [Integrity: OK (Checked {} chunks)]", chunks); }
    else { println!("    [Integrity: FAILED]"); }

    results.push(BenchmarkResult { name: "CAST (Ck)".to_string(), size: total_size, time: total_time });
}

// --- COMPETITORS LOGIC (ALWAYS SOLID) ---

fn run_competitor_solid(algo: &str, data: &[u8], dict_size: u32, results: &mut Vec<BenchmarkResult>) {
    let orig_len = data.len();
    match algo {
        "lzma2" => {
            let name = "LZMA2";
            print!("\n[*]  Running {} (Global/Solid)...", name);
            io::stdout().flush().unwrap();
            let start = Instant::now();
            // CHANGED: Pass dict_size to your 7z helper
            let c = compress_with_7z(data, dict_size);
            let duration = start.elapsed().as_secs_f64();
            let size = c.len();
            print_result(duration, size, orig_len);
            results.push(BenchmarkResult { name: name.to_string(), size, time: duration });
        },
        "brotli" => {
            let name = "Brotli";
            print!("\n[*]  Running {} (Q11 - Global)...", name);
            io::stdout().flush().unwrap();
            let start = Instant::now();
            let c = compress_brotli_max(data);
            let duration = start.elapsed().as_secs_f64();
            let size = c.len();
            print_result(duration, size, orig_len);
            results.push(BenchmarkResult { name: name.to_string(), size, time: duration });
        },
        "zstd" => {
            let name = "Zstd";
            print!("\n[*]  Running {} (L22 - Global)...", name);
            io::stdout().flush().unwrap();
            let start = Instant::now();
            let c = compress_zstd_max(data);
            let duration = start.elapsed().as_secs_f64();
            let size = c.len();
            print_result(duration, size, orig_len);
            results.push(BenchmarkResult { name: name.to_string(), size, time: duration });
        },
        _ => {}
    }
}

// --- HELPERS ---

fn print_result(seconds: f64, size: usize, orig: usize) {
    let ratio = if size > 0 { orig as f64 / size as f64 } else { 0.0 };
    println!(" Done in {:>6.2}s | Size: {:>20} | Ratio: {:>6.2}x",
             seconds, format_bytes(size), ratio);
}

fn compress_brotli_max(data: &[u8]) -> Vec<u8> {
    let mut writer = brotli::CompressorWriter::new(Vec::new(), 4096, 11, 22);
    writer.write_all(data).unwrap();
    writer.into_inner()
}

fn compress_zstd_max(data: &[u8]) -> Vec<u8> {
    let mut encoder = zstd::stream::write::Encoder::new(Vec::new(), 22).unwrap();
    let threads = num_cpus::get() as u32;
    let _ = encoder.multithread(threads);
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
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
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    format!("{} bytes", result.chars().rev().collect::<String>())
}

fn format_num_simple(n: usize) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect::<String>()
}

// [ADDED] The missing Helper function
fn print_bench_usage() {
    println!(
        "\nCAST Benchmarking Harness (7-Zip Backend)\n\n\
        Usage:\n  \
          run_benchmarks --list <LIST> --compare-with <ALGOS> [OPTIONS]\n\n\
        Arguments:\n  \
          --list <file.txt>      File containing a list of paths to test (one per line)\n  \
          --compare-with <algos> Comma-separated list of competitors (e.g. 'lzma2,zstd')\n                         or 'all' for [lzma2, brotli, zstd]\n\n\
        Options:\n  \
          --chunk-size <SIZE>    Run CAST in Chunked Mode (e.g., 512MB, 1GB). Default: Solid Mode\n  \
          --dict-size <SIZE>     Set 7-Zip LZMA Dictionary Size (Default: 128MB)\n\n\
        Examples:\n  \
          run_benchmarks --list datasets.txt --compare-with lzma2\n  \
          run_benchmarks --list big_logs.txt --compare-with all --chunk-size 512MB --dict-size 256MB"
    );
}