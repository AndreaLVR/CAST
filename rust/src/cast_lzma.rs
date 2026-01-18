use std::cmp;
use std::io::{Read, Write};
use std::path::Path;
use std::env;
use xz2::read::XzDecoder;
use xz2::write::XzEncoder;
use xz2::stream::{Stream, MtStreamBuilder, Check, LzmaOptions, Filters};
use std::process::{Command, Stdio};
use std::thread;

use crate::cast::{NativeCompressor, NativeDecompressor, CASTCompressor, CASTDecompressor};

const LZMA_PRESET_EXTREME: u32 = 0x80000000;

// ============================================================================
//  HELPER: 7-Zip Detection
// ============================================================================

pub fn get_7z_cmd() -> String {
    if let Ok(path) = env::var("SEVEN_ZIP_PATH") {
        return path.trim_matches('"').to_string();
    }

    // 2. Windows
    if cfg!(target_os = "windows") {
        let standard = r"C:\Program Files\7-Zip\7z.exe";
        if Path::new(standard).exists() {
            return standard.to_string();
        }
        return "7z.exe".to_string();
    }

    // 3. macOS
    if cfg!(target_os = "macos") {
        let common_paths = [
            "/opt/homebrew/bin/7zz", // Apple Silicon standard
            "/usr/local/bin/7zz",    // Intel standard
            "/usr/local/bin/7z",     // Legacy p7zip
        ];

        for path in common_paths {
            if Path::new(path).exists() {
                return path.to_string();
            }
        }

        return "7zz".to_string();
    }

    // 4. Fallback for Linux / Unix
    "7z".to_string()
}

pub fn try_find_7zip_path() -> Option<String> {
    let cmd = get_7z_cmd();
    // Simple check: try to run "7z" (or path) with no args or help
    // But simply checking if path exists (for absolute paths) or assume it's in PATH
    let exists = if cmd.contains("/") || cmd.contains("\\") {
        Path::new(&cmd).exists()
    } else {
        true
    };

    if exists {
        // Safe check trying to spawn it with "-h"
        if Command::new(&cmd).arg("-h").output().is_ok() {
            return Some(cmd);
        }
    }
    None
}


// ============================================================================
//  BACKEND 1: NATIVE (XZ2 Lib)
// ============================================================================

pub struct LzmaBackend {
    multithread: bool,
    dict_size: u32,
}

impl LzmaBackend {
    pub fn new(multithread: bool, dict_size: u32) -> Self {
        Self { multithread, dict_size }
    }
}

impl NativeCompressor for LzmaBackend {
    fn compress(&self, data: &[u8]) -> Vec<u8> {
        // EXACT LOGIC FROM ORIGINAL compress_buffer_native
        if data.is_empty() { return Vec::new(); }

        let effective_multithread = if self.multithread && (data.len() as u32) < self.dict_size {
            false
        } else {
            self.multithread
        };

        let mut opts = LzmaOptions::new_preset(9 | LZMA_PRESET_EXTREME).unwrap();
        opts.dict_size(self.dict_size); // Uses the passed dictionary size

        let mut filters = Filters::new();
        filters.lzma2(&opts);

        let estimated = data.len() / 2;
        let safe_capacity = cmp::min(estimated, self.dict_size as usize);
        let output_buffer = Vec::with_capacity(safe_capacity);
        let writer = std::io::BufWriter::new(output_buffer);

        if !effective_multithread {
            let stream = Stream::new_stream_encoder(&filters, Check::Crc32).expect("LZMA Init Error");
            let mut compressor = XzEncoder::new_stream(writer, stream);
            compressor.write_all(data).expect("LZMA Write Error");
            let finished = compressor.finish().expect("LZMA Finish Error");
            return finished.into_inner().expect("Buffer extraction error");
        }

        let threads = num_cpus::get() as u32;
        let stream = MtStreamBuilder::new()
            .threads(threads)
            .filters(filters)
            .check(Check::Crc32)
            .encoder()
            .expect("LZMA MT Init Error");

        let mut compressor = XzEncoder::new_stream(writer, stream);
        compressor.write_all(data).expect("LZMA MT Write Error");
        let finished = compressor.finish().expect("LZMA MT Finish Error");
        finished.into_inner().expect("Buffer extraction error")
    }
}

pub struct LzmaDecompressorBackend;

impl NativeDecompressor for LzmaDecompressorBackend {
    fn decompress(&self, data: &[u8]) -> Vec<u8> {
        if data.is_empty() { return Vec::new(); }

        let mut decompressor = XzDecoder::new(data);

        let estimated = data.len().saturating_mul(6);

        let safe_capacity = std::cmp::min(estimated, 2 * 1024 * 1024 * 1024);

        let mut output = Vec::with_capacity(safe_capacity);
        decompressor.read_to_end(&mut output).expect("Decompression Error");
        output
    }
}


// ============================================================================
//  BACKEND 2: 7-ZIP (External Executable)
// ============================================================================

pub struct SevenZipBackend {
    dict_size: u32,
}

impl SevenZipBackend {
    pub fn new(dict_size: u32) -> Self {
        Self { dict_size }
    }
}

impl NativeCompressor for SevenZipBackend {
    fn compress(&self, data: &[u8]) -> Vec<u8> {
        // 1. QUICK CHECK
        if data.is_empty() { return Vec::new(); }

        let dict_arg = format!("-m0=lzma2:d{}b", self.dict_size);
        let cmd = get_7z_cmd();

        let mut child = Command::new(&cmd)
            .args(&["a", "-txz", "-mx=9", "-mmt=on", &dict_arg, "-si", "-so", "-an", "-y", "-bb0"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("Failed to spawn 7-Zip");

        let input_data = data.to_vec();
        let mut stdin = child.stdin.take().expect("Failed to open stdin");

        // 4. THREAD ANTI-DEADLOCK
        thread::spawn(move || {
            stdin.write_all(&input_data).ok();
        });

        // 5. OUTPUT READING (Main Thread)
        let mut output_data = Vec::new();
        if let Some(mut stdout) = child.stdout.take() {
            stdout.read_to_end(&mut output_data).expect("Failed to read 7z stdout");
        }

        // 6. CLOSE AND CHECK
        let status = child.wait().expect("Failed to wait on 7z");

        if !status.success() {
            panic!("7-Zip Compression Error: Process returned failure code");
        }

        output_data
    }
}

pub struct SevenZipDecompressorBackend;

impl NativeDecompressor for SevenZipDecompressorBackend {
    fn decompress(&self, data: &[u8]) -> Vec<u8> {
        if data.is_empty() { return Vec::new(); }

        let cmd = get_7z_cmd();

        let mut child = Command::new(&cmd)
            .args(&["e", "-txz", "-si", "-so", "-y", "-bb0"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("Failed to spawn 7-Zip");

        let input_data = data.to_vec();
        let mut stdin = child.stdin.take().expect("Failed to open stdin");

        thread::spawn(move || {
            stdin.write_all(&input_data).ok();
        });

        let estimated_size = data.len() * 5;
        let mut output_data = Vec::with_capacity(estimated_size);

        if let Some(mut stdout) = child.stdout.take() {
            if let Err(e) = stdout.read_to_end(&mut output_data) {
                eprintln!("Error reading 7z output: {}", e);
                return Vec::new();
            }
        }

        let status = child.wait().expect("Failed to wait on 7z");

        if status.success() {
            output_data
        } else {
            eprintln!("\n[!] CRITICAL ERROR: 7-Zip backend returned a failure status.");
            eprintln!("[!] The decompression process cannot continue safely.");
            std::process::exit(1);
        }
    }
}


// ============================================================================
//  RUNTIME ENUM WRAPPERS (To allow main to switch dynamically)
// ============================================================================

pub enum RuntimeLzmaCompressor {
    Native(LzmaBackend),
    SevenZip(SevenZipBackend),
}

impl NativeCompressor for RuntimeLzmaCompressor {
    fn compress(&self, data: &[u8]) -> Vec<u8> {
        match self {
            RuntimeLzmaCompressor::Native(b) => b.compress(data),
            RuntimeLzmaCompressor::SevenZip(b) => b.compress(data),
        }
    }
}

pub enum RuntimeLzmaDecompressor {
    Native(LzmaDecompressorBackend),
    SevenZip(SevenZipDecompressorBackend),
}

impl NativeDecompressor for RuntimeLzmaDecompressor {
    fn decompress(&self, data: &[u8]) -> Vec<u8> {
        match self {
            RuntimeLzmaDecompressor::Native(b) => b.decompress(data),
            RuntimeLzmaDecompressor::SevenZip(b) => b.decompress(data),
        }
    }
}

// ============================================================================
//  TYPE ALIASES FOR MAIN
// ============================================================================

pub type CASTLzmaCompressor = CASTCompressor<RuntimeLzmaCompressor>;
pub type CASTLzmaDecompressor = CASTDecompressor<RuntimeLzmaDecompressor>;