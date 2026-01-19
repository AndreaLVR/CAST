use std::env;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;
use std::time::Instant;
use crc32fast::Hasher;

// Import implementations including the new Runtime wrappers and 7z utils
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

fn main() {
    let args: Vec<String> = env::args().collect();

    // --- 1. DYNAMIC EXECUTABLE NAME EXTRACTION ---
    let exe_path = Path::new(&args[0]);
    let exe_name = exe_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("cast");

    // --- 2. HELP FLAG CHECK ---
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print_usage(exe_name);
        return;
    }

    // --- ARGUMENT PARSING ---
    let use_multithread = args.iter().any(|arg| arg == "--multithread");
    let verify_flag = args.iter().any(|arg| arg == "-v" || arg == "--verify");

    // Chunk Size parsing
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

    // Dict Size parsing
    let mut dict_size_bytes: Option<u32> = None;
    if let Some(pos) = args.iter().position(|arg| arg == "--dict-size") {
        if pos + 1 < args.len() {
            let val = &args[pos+1];
            if let Some(s) = parse_size(val) {
                dict_size_bytes = Some(s as u32);
            } else {
                eprintln!("[!] Error: Invalid dict size format.");
                std::process::exit(1);
            }
        }
    }

    // Mode Parsing (Native vs 7Zip) - Default is now handled via Option logic below
    let mut mode_arg: Option<String> = None;
    if let Some(pos) = args.iter().position(|arg| arg == "--mode") {
        if pos + 1 < args.len() {
            mode_arg = Some(args[pos+1].to_lowercase());
        }
    }

    // Filter out args
    let clean_args: Vec<String> = args.iter()
        .filter(|arg| *arg != "--multithread" && *arg != "-v" && *arg != "--verify"
                      && *arg != "--chunk-size"
                      && *arg != "--dict-size"
                      && *arg != "--mode"
                      && args.iter().position(|x| x == *arg) != args.iter().position(|x| x == "--chunk-size").map(|p| p+1)
                      && args.iter().position(|x| x == *arg) != args.iter().position(|x| x == "--dict-size").map(|p| p+1)
                      && args.iter().position(|x| x == *arg) != args.iter().position(|x| x == "--mode").map(|p| p+1)
                      && *arg != "-h" && *arg != "--help")
        .cloned()
        .collect();

    if clean_args.len() < 2 {
        print_usage(exe_name);
        return;
    }

    let mode_or_file = &clean_args[1];

    println!("\n\n|--    CAST: Columnar Agnostic Structural Transformation (v{})    --|", env!("CARGO_PKG_VERSION"));
    println!("       Author: Andrea Olivari");
    println!("       GitHub: https://github.com/AndreaLVR/CAST\n");

    // DETERMINE BACKEND LOGIC
    let (use_7zip, backend_label) = match mode_arg.as_deref() {
        Some("native") => {
            (false, "Native (xz2)".to_string())
        },
        Some("7zip") => {
            if let Some(path) = try_find_7zip_path() {
                (true, format!("7-Zip (External) [Found at: {}]", path))
            } else {
                eprintln!("[!] CRITICAL ERROR: 7-Zip mode forced but executable not found.");

                if let Ok(env_path) = env::var("SEVEN_ZIP_PATH") {
                    eprintln!("    The environment variable SEVEN_ZIP_PATH is set to '{}', but this path seems invalid or not executable.", env_path);
                } else {
                    eprintln!("    Could not find '7z' or '7zz' in standard paths or PATH.");
                    eprintln!("    Please install 7-Zip or set the SEVEN_ZIP_PATH environment variable.");
                }
                std::process::exit(1);
            }
        },
        _ => {
            if mode_or_file == "-c" {
                if let Some(path) = try_find_7zip_path() {
                    println!("[*]  Auto-detected 7-Zip at: {}", path);
                    (true, format!("7-Zip (External) [Found at: {}]", path))
                } else {
                    (false, "Native (xz2) [Fallback]".to_string())
                }
            } else {
                (false, "Native (xz2) [Default]".to_string())
            }
        }
    };

    match mode_or_file.as_str() {
        "-c" => {
            if clean_args.len() < 4 {
                eprintln!("[!]  Missing output path.");
                print_usage(exe_name);
                return;
            }
            let input = &clean_args[2];
            let output = &clean_args[3];

            if !Path::new(input).exists() {
                 eprintln!("[!]  Error: Input file '{}' not found.", input);
                 std::process::exit(1);
            }

            let mode_display = if use_7zip {
                "MULTITHREAD (Implicit via 7-Zip)"
            } else if use_multithread {
                "MULTITHREAD"
            } else {
                "SOLID (SINGLE THREAD)"
            };

            println!("\n[*]  Starting Compression...");
            println!("       Input:       {}", input);
            println!("       Output:      {}", output);
            println!("       Backend:     {}", backend_label);
            println!("       Mode:        {}", mode_display);

            let final_dict = dict_size_bytes.unwrap_or(128 * 1024 * 1024);
            println!("       Dict Size:   {}", format_bytes(final_dict as usize));

            do_compress(input, output, use_multithread, chunk_size_bytes, final_dict, use_7zip);

            if verify_flag {
                println!("\n------------------------------------------------");
                println!("[*]  Starting Post-Compression Verification...");
                std::thread::sleep(std::time::Duration::from_millis(500));
                do_verify_standalone(output, use_7zip);
            }
        },
        "-d" => {
            if clean_args.len() < 4 {
                eprintln!("[!]  Missing output path.");
                print_usage(exe_name);
                return;
            }
            println!("\n[*]  Starting Decompression...");
            println!("      Backend:     {}", backend_label);
            do_decompress(&clean_args[2], &clean_args[3], use_7zip);
        },
        _ => {
            if verify_flag || Path::new(mode_or_file).exists() {
                let input_file = mode_or_file;
                if !Path::new(input_file).exists() {
                    eprintln!("[!]  Error: File '{}' not found.", input_file);
                    return;
                }
                println!("\n[*]  Starting Verification...");
                println!("       Backend:     {}", backend_label);
                do_verify_standalone(input_file, use_7zip);
            } else {
                eprintln!("[!]  Unknown command or file not found: {}", mode_or_file);
                print_usage(exe_name);
            }
        }
    }
}

// --- HELPER PARSING ---

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
        "\nCAST (Columnar Agnostic Structural Transformation) CLI Tool (v{})\n\
        Author: Andrea Olivari\n\
        GitHub: https://github.com/AndreaLVR/CAST\n\n\
        Usage:\n  \
          {} [MODE] [INPUT] [OUTPUT] [OPTIONS]\n\n\
        Modes:\n  \
          -c <in> <out>      Compress input file to CAST format\n  \
          -d <in> <out>      Decompress CAST file to original format\n  \
          -v <file>          Verify the integrity of a CAST file\n\n\
        Options:\n  \
          --mode <TYPE>      Backend selection: 'native' or '7zip'\n                         (Default: 7zip for compression, Native for decompression)\n  \
          --multithread      Enable parallel compression for higher speed\n  \
          --chunk-size <S>   Split input in chunks (Compression RAM Saver) (e.g., 512MB). Default: Solid Mode\n  \
          --dict-size <S>    Set LZMA Dictionary size (Default: 128MB)\n  \
          -v, --verify       (During compression) Run an immediate integrity check\n  \
          -h, --help         Show this help message\n\n\
        Examples:\n  \
          {} -c data.csv archive.cast --mode 7zip -v\n  \
          {} -c large_log.log archive.cast --chunk-size 256MB -v\n  \
          {} -d archive.cast restored.log\n  \
          {} -v archive.cast",
        env!("CARGO_PKG_VERSION"),
        exe_name, exe_name, exe_name, exe_name, exe_name
    );
}

// --- COMPRESSION ---

fn do_compress(input_path: &str, output_path: &str, multithread: bool, chunk_bytes_limit: Option<usize>, dict_size: u32, use_7zip: bool) {
    let start_total = Instant::now();
    let mut f_in = File::open(input_path).expect("Error opening input");
    let mut f_out = File::create(output_path).expect("Error creating output");
    let file_len = f_in.metadata().unwrap().len();

    let buffer_size = chunk_bytes_limit.unwrap_or(file_len as usize);
    let mut buffer = vec![0u8; buffer_size];

    let mut total_read = 0;
    let mut total_written = 0;
    let mut chunk_count = 0;

    println!("\n[*]  Starting stream processing...");

    loop {
        let mut current_read = 0;
        while current_read < buffer_size {
            let n = f_in.read(&mut buffer[current_read..]).expect("Error reading chunk");
            if n == 0 { break; }
            current_read += n;
        }
        if current_read == 0 { break; }

        chunk_count += 1;
        let chunk_data = &buffer[0..current_read];

        print!("\r       Processing Chunk #{} ({})... ", chunk_count, format_bytes(chunk_data.len()));
        io::stdout().flush().unwrap();

        // CRC
        let mut h = Hasher::new();
        h.update(chunk_data);
        let chunk_crc = h.finalize();

        // CAST Compression (Backend Selection)
        // Wraps the specific backend in the Runtime Enum
        let backend = if use_7zip {
            RuntimeLzmaCompressor::SevenZip(SevenZipBackend::new(dict_size))
        } else {
            RuntimeLzmaCompressor::Native(LzmaBackend::new(multithread, dict_size))
        };

        let mut compressor = CASTLzmaCompressor::new(backend);
        let (c_reg, c_ids, c_vars, id_flag, _) = compressor.compress(chunk_data);

        let mut header = Vec::new();
        header.extend_from_slice(&chunk_crc.to_le_bytes());
        header.extend_from_slice(&(c_reg.len() as u32).to_le_bytes());
        header.extend_from_slice(&(c_ids.len() as u32).to_le_bytes());
        header.extend_from_slice(&(c_vars.len() as u32).to_le_bytes());
        header.push(id_flag);

        f_out.write_all(&header).unwrap();
        f_out.write_all(&c_reg).unwrap();
        f_out.write_all(&c_ids).unwrap();
        f_out.write_all(&c_vars).unwrap();

        total_read += current_read;
        total_written += header.len() + c_reg.len() + c_ids.len() + c_vars.len();

        if chunk_bytes_limit.is_none() { break; }
    }

    drop(f_out);

    let ratio = if total_written > 0 { total_read as f64 / total_written as f64 } else { 0.0 };

    println!("\n[+]  Compression completed!");
    println!("       Total Input:    {}", format_bytes(total_read));
    println!("       Total Output:   {}", format_bytes(total_written));
    println!("       Ratio:          {:.2}x", ratio);
    println!("       Time:           {:.2}s", start_total.elapsed().as_secs_f64());
}

// --- DECOMPRESSION ---

fn do_decompress(input_path: &str, output_path: &str, use_7zip: bool) {
    let start = Instant::now();
    let f_in = File::open(input_path).expect("Error opening archive");

    if f_in.metadata().unwrap().len() == 0 {
        eprintln!("[!] ERROR: Input file is empty (0 bytes).");
        return;
    }

    let mut reader = std::io::BufReader::new(f_in);
    let mut f_out = File::create(output_path).expect("Error creating output");

    // Backend Selection for Decompressor
    let backend = if use_7zip {
        RuntimeLzmaDecompressor::SevenZip(SevenZipDecompressorBackend)
    } else {
        RuntimeLzmaDecompressor::Native(LzmaDecompressorBackend)
    };

    let decompressor = CASTLzmaDecompressor::new(backend);
    let mut chunk_idx = 0;

    println!("\n[*]  Extracting stream...");

    loop {
        let mut header = [0u8; 17];
        match reader.read_exact(&mut header) {
            Ok(_) => {},
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                if chunk_idx == 0 {
                    eprintln!("[!] ERROR: File header missing or corrupted.");
                }
                break;
            },
            Err(e) => panic!("Error reading header: {}", e),
        };

        chunk_idx += 1;
        let expected_crc = u32::from_le_bytes(header[0..4].try_into().unwrap());
        let l_reg = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;
        let l_ids = u32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
        let l_vars = u32::from_le_bytes(header[12..16].try_into().unwrap()) as usize;
        let id_flag = header[16];

        let body_len = l_reg + l_ids + l_vars;
        let mut body_buffer = vec![0u8; body_len];
        reader.read_exact(&mut body_buffer).expect("Truncated file body");

        print!("\r      Extracting Chunk #{}... ", chunk_idx);
        io::stdout().flush().unwrap();

        let chunk_reg = &body_buffer[0 .. l_reg];
        let chunk_ids = &body_buffer[l_reg .. l_reg+l_ids];
        let chunk_vars = &body_buffer[l_reg+l_ids .. l_reg+l_ids+l_vars];

        match decompressor.decompress(chunk_reg, chunk_ids, chunk_vars, expected_crc, id_flag, &mut f_out) {
            Ok(_) => {},
            Err(e) => {
                eprintln!("\n[!]  CRASH: Decompression error at Chunk {}: {}", chunk_idx, e);
                std::process::exit(1);
            }
        }
    }

    if chunk_idx > 0 {
        println!("\n[+]  Decompression done in {:.2}s", start.elapsed().as_secs_f64());
    }
}

// --- VERIFICATION ---

fn do_verify_standalone(input_path: &str, use_7zip: bool) {
    let start = Instant::now();
    let f_in = File::open(input_path).expect("Error opening archive");
    let mut reader = std::io::BufReader::new(f_in);

    // Backend Selection for Verification
    let backend = if use_7zip {
        RuntimeLzmaDecompressor::SevenZip(SevenZipDecompressorBackend)
    } else {
        RuntimeLzmaDecompressor::Native(LzmaDecompressorBackend)
    };

    let decompressor = CASTLzmaDecompressor::new(backend);
    let mut chunk_idx = 0;

    println!("[*]  Verifying Stream Integrity (RAM Optimized)...");

    loop {
        let mut header = [0u8; 17];
        match reader.read_exact(&mut header) {
            Ok(_) => {},
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => panic!("Error reading header: {}", e),
        };

        chunk_idx += 1;
        let expected_crc = u32::from_le_bytes(header[0..4].try_into().unwrap());
        let l_reg = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;
        let l_ids = u32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
        let l_vars = u32::from_le_bytes(header[12..16].try_into().unwrap()) as usize;
        let id_flag = header[16];

        let body_len = l_reg + l_ids + l_vars;
        let mut body_buffer = vec![0u8; body_len];
        reader.read_exact(&mut body_buffer).expect("Truncated file in body");

        print!("\r       Verifying Chunk #{}... ", chunk_idx);
        io::stdout().flush().unwrap();

        let chunk_reg = &body_buffer[0 .. l_reg];
        let chunk_ids = &body_buffer[l_reg .. l_reg+l_ids];
        let chunk_vars = &body_buffer[l_reg+l_ids .. l_reg+l_ids+l_vars];

        let mut temp_buffer = Vec::new();

        match decompressor.decompress(chunk_reg, chunk_ids, chunk_vars, expected_crc, id_flag, &mut temp_buffer) {
            Ok(_) => {
                let mut h = Hasher::new();
                h.update(&temp_buffer);
                if h.finalize() != expected_crc {
                    println!("\n[!]   FAILURE: CRC Mismatch at Chunk {}!", chunk_idx);
                    std::process::exit(1);
                }
            },
            Err(e) => {
                println!("\n[!]   CRASH: Decompression error at Chunk {}: {}", chunk_idx, e);
                std::process::exit(1);
            }
        }
    }

    println!("\n[+]  FILE INTEGRITY VERIFIED. Chunks: {}. Time: {:.2}s", chunk_idx, start.elapsed().as_secs_f64());
}