use std::cmp;
use std::io::{Read, Write};
use std::process::Command;
use std::fs::{self, File};
use std::path::Path;
use std::env;
use rand::Rng; // Assumo che rand sia nel Cargo.toml come nel vecchio progetto

use xz2::read::XzDecoder;
use xz2::write::XzEncoder;
use xz2::stream::{Stream, MtStreamBuilder, Check, LzmaOptions, Filters};

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

// MODIFICATO: Restituisce Option<String> con la path se trovato, altrimenti None
pub fn try_find_7zip_path() -> Option<String> {
    let cmd = get_7z_cmd();
    // Simple check: try to run "7z" (or path) with no args or help
    // But simply checking if path exists (for absolute paths) or assume it's in PATH
    let exists = if cmd.contains("/") || cmd.contains("\\") {
        Path::new(&cmd).exists()
    } else {
        // Se è solo un comando, proviamo a eseguirlo per vedere se il sistema lo risolve
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
        // EXACT LOGIC FROM ORIGINAL decompress_buffer_native
        if data.is_empty() { return Vec::new(); }
        let mut decompressor = XzDecoder::new(data);
        let mut output = Vec::with_capacity(data.len() * 3);
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
        if data.is_empty() { return Vec::new(); }

        let pid = std::process::id();
        let rnd = rand::thread_rng().gen::<u32>();
        let tmp_in = format!("temp_in_{}_{}.bin", pid, rnd);
        let tmp_out = format!("temp_out_{}_{}.xz", pid, rnd);

        let _ = fs::remove_file(&tmp_in);
        let _ = fs::remove_file(&tmp_out);

        {
            let mut f = File::create(&tmp_in).expect("Cannot create temp input");
            f.write_all(data).expect("Cannot write temp input");
            f.flush().unwrap();
            f.sync_all().unwrap();
        }

        // e.g., "-m0=lzma2:d134217728b" (7-Zip supports 'b' suffix for bytes)
        let dict_arg = format!("-m0=lzma2:d{}b", self.dict_size);

        let cmd = get_7z_cmd();
        let output = Command::new(&cmd)
            .args(&["a", "-txz", "-mx=9", "-mmt=on", &dict_arg, "-y", "-bb0", &tmp_out, &tmp_in])
            .output();

        match output {
            Ok(out) => {
                if !out.status.success() {
                    let _ = fs::remove_file(&tmp_in);
                    let _ = fs::remove_file(&tmp_out);
                    panic!("7-Zip Error: {}", String::from_utf8_lossy(&out.stderr));
                }
            },
            Err(e) => panic!("Exec Error: {}", e)
        }

        let result = fs::read(&tmp_out).unwrap_or_else(|_| Vec::new());

        let _ = fs::remove_file(&tmp_in);
        let _ = fs::remove_file(&tmp_out);
        result
    }
}

pub struct SevenZipDecompressorBackend;

impl NativeDecompressor for SevenZipDecompressorBackend {
    fn decompress(&self, data: &[u8]) -> Vec<u8> {
        if data.is_empty() { return Vec::new(); }
        let pid = std::process::id();
        let rnd = rand::thread_rng().gen::<u32>();
        let tmp_in = format!("temp_dec_in_{}_{}.xz", pid, rnd);

        let _ = fs::remove_file(&tmp_in);

        {
            let mut f = File::create(&tmp_in).unwrap();
            f.write_all(data).unwrap();
            f.flush().unwrap();
        }

        let cmd = get_7z_cmd();
        let output = Command::new(&cmd)
            .args(&["e", &tmp_in, "-so", "-y"])
            .output();

        let _ = fs::remove_file(&tmp_in);

        match output {
            Ok(o) => o.stdout,
            Err(e) => {
                eprintln!("Decompression Error: {}", e);
                Vec::new()
            },
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