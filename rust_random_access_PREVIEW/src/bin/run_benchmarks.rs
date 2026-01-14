use std::env;
use std::fs::File;
// FIX 1: Aggiunto 'Read' qui sotto
use std::io::{self, BufRead, BufReader, Cursor, Write, Read};
use std::path::Path;
use std::time::Instant;
use crc32fast::Hasher;

// FIX 2: Importiamo i Trait necessari per chiamare .compress() sui competitor
use cast::cast::{NativeCompressor};

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

// Struct to store results for final ranking
struct BenchmarkResult {
    name: String,
    size: usize,
    time: f64,
}

fn main() {
    let args: Vec<String> = env::args().collect();

    // --- 1. DYNAMIC EXECUTABLE NAME EXTRACTION ---
    let exe_path = Path::new(&args[0]);
    let exe_name = exe_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("run_benchmarks");

    // --- 2. HELP FLAG CHECK ---
    if args.len() < 2 || args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print_bench_usage(exe_name);
        return;
    }

    let use_multithread = args.iter().any(|arg| arg == "--multithread");

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

    let mut dict_size_bytes: u32 = 128 * 1024 * 1024;
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

    let mut preferred_mode = "auto".to_string();
    if let Some(pos) = args.iter().position(|arg| arg == "--mode") {
        if pos + 1 < args.len() {
            preferred_mode = args[pos+1].to_lowercase();
        }
    }

    let list_path_opt = args.windows(2)
        .find(|w| w[0] == "--list")
        .map(|w| w[1].clone());

    if list_path_opt.is_none() {
        eprintln!("[!]  ERROR: Missing '--list <file.txt>'");
        print_bench_usage(exe_name);
        std::process::exit(1);
    }
    let list_path = list_path_opt.unwrap();

    let competitors_opt = args.windows(2)
        .find(|w| w[0] == "--compare-with")
        .map(|w| w[1].clone());

    if competitors_opt.is_none() {
        eprintln!("[!]  ERROR: Missing '--compare-with <algos>'");
        print_bench_usage(exe_name);
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

    println!("\n\n|--    CAST: Columnar Agnostic Structural Transformation (v{})    --|", env!("CARGO_PKG_VERSION"));
    println!("       Author: Andrea Olivari");
    println!("       GitHub: https://github.com/AndreaLVR/CAST\n");

    let (use_7zip, backend_label) = match preferred_mode.as_str() {
        "native" => (false, "Native (xz2)".to_string()),
        "7zip" => {
            if let Some(path) = try_find_7zip_path() {
                (true, format!("7-Zip (External) [Found at: {}]", path))
            } else {
                eprintln!("[!] CRITICAL ERROR: 7-Zip mode forced but executable not found.");
                std::process::exit(1);
            }
        },
        _ => {
            if let Some(path) = try_find_7zip_path() {
                (true, format!("7-Zip (External) [Found at: {}]", path))
            } else {
                (false, "Native (xz2) [Fallback]".to_string())
            }
        }
    };

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

    let threads = num_cpus::get();
    let mode_display = if use_7zip {
        "MULTITHREAD (Implicit via 7-Zip)".to_string()
    } else if use_multithread {
        format!("MULTITHREAD ({} threads)", threads)
    } else {
        "SOLID (1 thread)".to_string()
    };

    println!("\nBENCHMARK SUITE");
    println!("--------------------------------------------------");
    println!("Backend:            {}", backend_label);
    println!("Mode:               {}", mode_display);

    if let Some(cs) = chunk_size_bytes {
        println!("CAST Chunking:      ACTIVE (Target ~{})", format_bytes(cs));
    } else {
        println!("CAST Chunking:      DISABLED (Solid Mode)");
    }
    println!("LZMA Dict Size:     {}", format_bytes(dict_size_bytes as usize));
    println!("Competitors:        {:?} (Always Global/Solid)", competitors);
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

        if let Ok(f) = File::open(&file_path) {
             println!("  Preview (First 6 lines):");
             let reader = BufReader::new(f);
             for (i, line) in reader.lines().take(6).enumerate() {
                 match line {
                     Ok(l) => {
                         let chars: Vec<char> = l.chars().collect();
                         let display = if chars.len() > 100 {
                             chars.into_iter().take(100).collect::<String>() + "..."
                         } else { l };
                         println!("    {}. {}", i + 1, display);
                     },
                     Err(_) => { println!("    [!] Binary or Non-UTF8 content."); break; }
                 }
             }
        }
        println!("{}", "-".repeat(60));

        let mut results = Vec::new();

        // ---------------------------------------------------------
        // 1: CAST (Solid or Chunked)
        // ---------------------------------------------------------
        if let Some(chunk_size) = chunk_size_bytes {
            run_cast_chunked_test(&file_path, chunk_size, file_len, use_multithread, dict_size_bytes, use_7zip, &mut results);
        } else {
             let data = match std::fs::read(&file_path) {
                Ok(d) => d,
                Err(e) => { eprintln!("[!]  Read Error: {}", e); continue; }
            };
            run_cast_solid_test(&data, use_multithread, dict_size_bytes, use_7zip, &mut results);
        }

        // ---------------------------------------------------------
        // 2: COMPETITORS
        // ---------------------------------------------------------
        if !competitors.is_empty() {
            let full_data = match std::fs::read(&file_path) {
                Ok(d) => d,
                Err(e) => { eprintln!("[!]  Cannot read file for competitors: {}", e); Vec::new() }
            };

            if !full_data.is_empty() {
                for algo in &competitors {
                    run_competitor_solid(algo, &full_data, use_multithread, dict_size_bytes, use_7zip, &mut results);
                }
            }
        }

        if results.is_empty() {
            println!("No algorithm completed the compression.");
            continue;
        }

        // Sort by size (ascending)
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
                i + 1, res.name, format_bytes(res.size), ratio, res.time, diff_str
            );
        }
        println!("{}", "-".repeat(70));

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

fn run_cast_solid_test(data: &[u8], multithread: bool, dict_size: u32, use_7zip: bool, results: &mut Vec<BenchmarkResult>) {
    let orig_len = data.len();
    print!("\n[*] Running CAST (Solid)...");
    io::stdout().flush().unwrap();

    let start = Instant::now();

    let backend = if use_7zip {
        RuntimeLzmaCompressor::SevenZip(SevenZipBackend::new(dict_size))
    } else {
        RuntimeLzmaCompressor::Native(LzmaBackend::new(multithread, dict_size))
    };

    let mut compressor = CASTLzmaCompressor::new(backend);

    // Config: Infinite Chunk Size (Simulates Solid Mode)
    compressor.set_chunk_size(usize::MAX);

    let mut output = Vec::with_capacity(data.len() / 2);
    let mut cursor = Cursor::new(data);

    compressor.compress_stream(&mut cursor, &mut output).expect("Compression failed");

    let duration = start.elapsed().as_secs_f64();
    let size = output.len();

    print_result(duration, size, orig_len);
    results.push(BenchmarkResult { name: "CAST (Solid)".to_string(), size, time: duration });

    // Verify
    print!("    [Verifying... ");
    io::stdout().flush().unwrap();
    let mut h = Hasher::new();
    h.update(data);
    let expected_crc = h.finalize();

    let decompressor_backend = if use_7zip {
        RuntimeLzmaDecompressor::SevenZip(SevenZipDecompressorBackend)
    } else {
        RuntimeLzmaDecompressor::Native(LzmaDecompressorBackend)
    };
    let decompressor = CASTLzmaDecompressor::new(decompressor_backend);

    let mut restored = Vec::with_capacity(data.len());
    let mut cursor_read = Cursor::new(&output);

    match decompressor.decompress_stream(&mut cursor_read, &mut restored, None) {
        Ok(_) => {
             let mut h2 = Hasher::new();
             h2.update(&restored);
             if h2.finalize() == expected_crc {
                 println!("OK]");
             } else {
                 println!("FAIL - CRC Mismatch]");
             }
        },
        Err(e) => println!("ERROR: {}]", e),
    }
}

fn run_cast_chunked_test(file_path: &str, chunk_size_bytes: usize, file_len: usize, multithread: bool, dict_size: u32, use_7zip: bool, results: &mut Vec<BenchmarkResult>) {
    print!("\n[*] Running CAST (Chunked)...");
    io::stdout().flush().unwrap();

    let start = Instant::now();

    let backend = if use_7zip {
        RuntimeLzmaCompressor::SevenZip(SevenZipBackend::new(dict_size))
    } else {
        RuntimeLzmaCompressor::Native(LzmaBackend::new(multithread, dict_size))
    };

    let mut compressor = CASTLzmaCompressor::new(backend);

    // ESTIMATE ROWS
    let estimated_rows = std::cmp::max(1000, chunk_size_bytes / 200);
    compressor.set_chunk_size(estimated_rows);

    let f_in = File::open(file_path).expect("Cannot open file");
    let mut output = Vec::new();

    compressor.compress_stream(f_in, &mut output).expect("Compression failed");

    let duration = start.elapsed().as_secs_f64();
    let size = output.len();

    print_result(duration, size, file_len);
    results.push(BenchmarkResult { name: "CAST (Chunked)".to_string(), size, time: duration });

    // Verify Integrity
    print!("    [Verifying... ");
    io::stdout().flush().unwrap();

    // Calc Original CRC
    let mut h = Hasher::new();
    let mut f_orig = File::open(file_path).unwrap();
    let mut buf = [0u8; 65536];
    while let Ok(n) = f_orig.read(&mut buf) {
        if n == 0 { break; }
        h.update(&buf[..n]);
    }
    let expected_crc = h.finalize();

    let decompressor_backend = if use_7zip {
        RuntimeLzmaDecompressor::SevenZip(SevenZipDecompressorBackend)
    } else {
        RuntimeLzmaDecompressor::Native(LzmaDecompressorBackend)
    };
    let decompressor = CASTLzmaDecompressor::new(decompressor_backend);

    let mut cursor_read = Cursor::new(&output);
    let mut hashing_sink = HashingSink { hasher: Hasher::new() };

    match decompressor.decompress_stream(&mut cursor_read, &mut hashing_sink, None) {
         Ok(_) => {
             if hashing_sink.hasher.finalize() == expected_crc {
                 println!("OK]");
             } else {
                 println!("FAIL - CRC Mismatch]");
             }
         },
         Err(e) => println!("ERROR: {}]", e),
    }
}

// Helper writer that calculates CRC and discards data
struct HashingSink {
    hasher: Hasher
}
impl Write for HashingSink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.hasher.update(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> { Ok(()) }
}

// --- COMPETITORS LOGIC ---

fn run_competitor_solid(algo: &str, data: &[u8], multithread: bool, dict_size: u32, use_7zip: bool, results: &mut Vec<BenchmarkResult>) {
    let orig_len = data.len();
    match algo {
        "lzma2" => {
            let name = "LZMA2";
            print!("\n[*] Running {} (XZ - Global)...", name);
            io::stdout().flush().unwrap();
            let start = Instant::now();

            let backend = if use_7zip {
                RuntimeLzmaCompressor::SevenZip(SevenZipBackend::new(dict_size))
            } else {
                RuntimeLzmaCompressor::Native(LzmaBackend::new(multithread, dict_size))
            };

            // FIX: Ora 'compress' è visibile grazie all'import del Trait
            let c = backend.compress(data);

            let duration = start.elapsed().as_secs_f64();
            let size = c.len();
            print_result(duration, size, orig_len);
            results.push(BenchmarkResult { name: name.to_string(), size, time: duration });
        },
        "brotli" => {
            let name = "Brotli";
            print!("\n[*] Running {} (Q11 - Global)...", name);
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
            print!("\n[*] Running {} (L22 - Global)...", name);
            io::stdout().flush().unwrap();
            let start = Instant::now();
            let c = compress_zstd_max(data, multithread);
            let duration = start.elapsed().as_secs_f64();
            let size = c.len();
            print_result(duration, size, orig_len);
            results.push(BenchmarkResult { name: name.to_string(), size, time: duration });
        },
        _ => {}
    }
}

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

fn compress_zstd_max(data: &[u8], multithread: bool) -> Vec<u8> {
    let mut encoder = zstd::stream::write::Encoder::new(Vec::new(), 22).unwrap();
    if multithread {
        let threads = num_cpus::get() as u32;
        let _ = encoder.multithread(threads);
    }
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
        if i > 0 && i % 3 == 0 { result.push(','); }
        result.push(c);
    }
    format!("{} bytes", result.chars().rev().collect::<String>())
}

fn format_num_simple(n: usize) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 { result.push(','); }
        result.push(c);
    }
    result.chars().rev().collect::<String>()
}

fn print_bench_usage(exe_name: &str) {
    println!(
        "\nCAST (Columnar Agnostic Structural Transformation) Benchmarking Tool (v{})\n\
        Author: Andrea Olivari\n\
        Usage:\n  \
          {} --list <LIST> --compare-with <ALGOS> [OPTIONS]\n\n\
        Arguments:\n  \
          --list <file.txt>      File containing a list of paths to test (one per line)\n  \
          --compare-with <algos> Comma-separated list of competitors (e.g. 'lzma2,zstd')\n                         or 'all' for [lzma2, brotli, zstd]\n\n\
        Options:\n  \
          --mode <TYPE>          Backend selection: 'native' or '7zip' (Default: Auto-detect 7zip, fallback to native)\n  \
          --multithread          Enable multithreading compression for CAST and competitors\n  \
          --chunk-size <SIZE>    Set approximate chunk size for Random Access testing (e.g., 50MB)\n  \
          --dict-size <SIZE>     Set LZMA Dictionary Size (Default: 128MB)\n  \
          -h, --help             Show this help message\n",
        env!("CARGO_PKG_VERSION"), exe_name
    );
}
