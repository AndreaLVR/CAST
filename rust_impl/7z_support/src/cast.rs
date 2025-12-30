use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::{self, File};
use std::io::{Write};
use std::process::Command;
use std::env;

use regex::Regex;
use crc32fast::Hasher;
use rand::Rng;

use flate2::write::ZlibEncoder;
use flate2::Compression;


// Helper function to determine the 7z path
fn get_7z_cmd() -> String {
    // 1. Try reading the specific environment variable
    if let Ok(path) = env::var("SEVEN_ZIP_PATH") {
        return path;
    }

    // 2. Fallback: assume "7z" is in the system global PATH
    if cfg!(target_os = "windows") {
        "7z.exe".to_string()
    } else {
        "7z".to_string()
    }
}

// --- 7-ZIP HELPER FUNCTIONS ---

pub fn compress_with_7z(data: &[u8]) -> Vec<u8> {
    if data.is_empty() { return Vec::new(); }

    let pid = std::process::id();
    // Usa un random u32 per evitare collisioni se il PID viene riciclato velocemente o in thread
    let rnd = rand::thread_rng().gen::<u32>();
    let tmp_in = format!("temp_in_{}_{}.bin", pid, rnd);
    let tmp_out = format!("temp_out_{}_{}.xz", pid, rnd);

    // Pulizia preventiva (ignoriamo errori se i file non esistono)
    let _ = fs::remove_file(&tmp_in);
    let _ = fs::remove_file(&tmp_out);

    // 1. Scrittura Input (con check errori)
    {
        let mut f = File::create(&tmp_in).expect("CRITICAL: Cannot create temp input file");
        f.write_all(data).expect("CRITICAL: Cannot write data to temp input file");
        f.flush().unwrap();
    }

    let cmd = get_7z_cmd();

    // 2. Esecuzione 7z
    let output = Command::new(&cmd)
        // Nota: d128m usa MOLTA RAM. Se va in OOM, lo vedremo ora.
        .args(&["a", "-txz", "-mx=9", "-mmt=on", "-m0=lzma2:d128m", "-y", "-bb0", &tmp_out, &tmp_in])
        .output();

    // 3. Gestione Rigorosa dell'Output
    match output {
        Ok(out) => {
            if !out.status.success() {
                // Se 7z esce con codice != 0 (es. 2 = Fatal Error / OOM), STAMPA E PANICA.
                let stderr = String::from_utf8_lossy(&out.stderr);
                let _ = fs::remove_file(&tmp_in); // Tentativo di cleanup
                let _ = fs::remove_file(&tmp_out);

                // QUI STA LA DIFFERENZA: Blocchiamo tutto invece di ritornare vuoto.
                panic!("\n[!] 7-ZIP CRASHED! Exit Code: {}\n[!] STDERR: {}\n[!] Hint: Try reducing dictionary size (-m0=lzma2:d64m) or use Chunking.", out.status, stderr);
            }
        },
        Err(e) => {
            let _ = fs::remove_file(&tmp_in);
            panic!("\n[!] CRITICAL: Failed to execute 7z command '{}'. Error: {}", cmd, e);
        }
    }

    // 4. Lettura Risultato
    let result = match fs::read(&tmp_out) {
        Ok(d) => d,
        Err(e) => {
            let _ = fs::remove_file(&tmp_in);
            panic!("\n[!] CRITICAL: 7z finished successfully but output file '{}' is missing/unreadable. IO Error: {}", tmp_out, e);
        }
    };

    // Cleanup finale solo se tutto è andato bene
    let _ = fs::remove_file(&tmp_in);
    let _ = fs::remove_file(&tmp_out);

    result
}

fn decompress_with_7z(data: &[u8]) -> Vec<u8> {
    if data.is_empty() { return Vec::new(); }
    let pid = std::process::id();
    let tmp_in = format!("temp_dec_in_{}_{}.xz", pid, rand::thread_rng().gen::<u32>());

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
            eprintln!("[!] CRITICAL ERROR: Decompression failed executing '{}'. {}", cmd, e);
            Vec::new()
        },
    }
}

// --- UTILS ---
fn decode_python_latin1(data: &[u8]) -> String {
    data.iter().map(|&b| b as char).collect()
}

// --- COMPRESSOR (CAST Logic with 7z) ---

pub struct CASTCompressor {
    template_map: HashMap<String, u32>,
    skeletons_list: Vec<String>,
    stream_template_ids: Vec<u32>,
    columns_storage: HashMap<u32, Vec<Vec<String>>>,
    next_template_id: u32,
    mode_name: String,
    #[allow(dead_code)]
    multithread: bool,
}

impl CASTCompressor {
    pub fn new(multithread: bool) -> Self {
        CASTCompressor {
            template_map: HashMap::new(),
            skeletons_list: Vec::new(),
            stream_template_ids: Vec::new(),
            columns_storage: HashMap::new(),
            next_template_id: 0,
            mode_name: "Strict".to_string(),
            multithread,
        }
    }

    fn analyze_strategy(&mut self, text: &str) -> Regex {
        let r_strict = Regex::new(r#"("(?:[^"\\]|\\.|"")*"|\-?\d+(?:\.\d+)?|0x[0-9a-fA-F]+)"#).unwrap();
        let r_aggr = Regex::new(r#"("(?:[^"\\]|\\.|"")*"|[a-zA-Z0-9_.\-]+)"#).unwrap();

        let sample_lines: Vec<&str> = text.lines().take(1000).collect();
        if sample_lines.is_empty() { return r_strict; }

        let mut strict_templates = HashSet::new();
        for line in &sample_lines {
            let skel = r_strict.replace_all(line, "\x00");
            strict_templates.insert(skel.to_string());
        }

        let ratio = strict_templates.len() as f64 / sample_lines.len() as f64;
        if ratio > 0.10 {
            self.mode_name = "Aggressive".to_string();
            r_aggr
        } else {
            self.mode_name = "Strict".to_string();
            r_strict
        }
    }

    pub fn compress(&mut self, input_data: &[u8]) -> (Vec<u8>, Vec<u8>, Vec<u8>, u8, String) {
        // 1. Binary Check
        let sample_len = std::cmp::min(input_data.len(), 4096);
        let bad_chars = input_data[..sample_len].iter()
            .filter(|&&b| b < 32 && b != 9 && b != 10 && b != 13).count();
        if sample_len > 0 && (bad_chars as f64 / sample_len as f64) > 0.01 {
             return self.create_passthrough(input_data, "Passthrough [Binary]");
        }

        // 2. Decoding Logic
        let text_data_owned;
        let text_data = match std::str::from_utf8(input_data) {
            Ok(s) => s,
            Err(_) => {
                text_data_owned = decode_python_latin1(input_data);
                &text_data_owned
            }
        };

        let active_regex = self.analyze_strategy(text_data);

        let lines: Vec<&str> = text_data.split_inclusive('\n').collect();
        let unique_limit = (lines.len() as f64 * if self.mode_name == "Aggressive" { 0.40 } else { 0.25 }) as u32;

        // 3. Processing Lines
        for line in lines {
            if line.is_empty() { continue; }

            let mut skeleton = String::with_capacity(line.len());
            let mut vars = Vec::new();
            let mut last_end = 0;

            for caps in active_regex.captures_iter(line) {
                let m = caps.get(0).unwrap();
                skeleton.push_str(&line[last_end..m.start()]);
                let token = m.as_str();
                if token.starts_with('"') {
                    if token.len() >= 2 { vars.push(token[1..token.len()-1].to_string()); }
                    else { vars.push(token.to_string()); }
                    skeleton.push_str("\"\x00\"");
                } else {
                    vars.push(token.to_string());
                    skeleton.push('\x00');
                }
                last_end = m.end();
            }
            skeleton.push_str(&line[last_end..]);

            let t_id;
            if let Some(&id) = self.template_map.get(&skeleton) {
                t_id = id;
            } else {
                if self.next_template_id > unique_limit {
                    return self.create_passthrough(text_data.as_bytes(), "Passthrough [Entropy]");
                }
                t_id = self.next_template_id;
                self.template_map.insert(skeleton.clone(), t_id);
                self.skeletons_list.push(skeleton);
                self.columns_storage.insert(t_id, vec![Vec::new(); vars.len()]);
                self.next_template_id += 1;
            }

            self.stream_template_ids.push(t_id);
            let cols = self.columns_storage.get_mut(&t_id).unwrap();
            let limit = std::cmp::min(vars.len(), cols.len());
            for i in 0..limit {
                cols[i].push(vars[i].clone());
            }
        }

        // 4. Heuristic
        let num_templates = self.skeletons_list.len();
        let mut decision_mode = "UNIFIED";
        if num_templates < 256 {
            let mut sample_buffer = Vec::new();
            let mut collected = 0;
            for t_id in 0..std::cmp::min(num_templates, 5) {
                if let Some(cols) = self.columns_storage.get(&(t_id as u32)) {
                    for col in cols {
                        for v in col.iter().take(50) {
                            sample_buffer.extend_from_slice(v.as_bytes());
                            collected += 1;
                        }
                        if collected > 2000 { break; }
                    }
                }
                if collected > 2000 { break; }
            }
            if !sample_buffer.is_empty() {
                let mut e = ZlibEncoder::new(Vec::new(), Compression::new(1));
                e.write_all(&sample_buffer).unwrap();
                let c_sample = e.finish().unwrap();
                if (sample_buffer.len() as f64 / c_sample.len() as f64) < 3.0 {
                    decision_mode = "SPLIT";
                }
            }
        }

        // 5. Unified Remapping
        if decision_mode == "UNIFIED" {
            let mut counts = HashMap::new();
            let mut first_appearance = HashMap::new();
            for (idx, &id) in self.stream_template_ids.iter().enumerate() {
                *counts.entry(id).or_insert(0) += 1;
                first_appearance.entry(id).or_insert(idx);
            }
            let mut sorted_ids: Vec<u32> = counts.keys().cloned().collect();
            sorted_ids.sort_by(|a, b| {
                let count_a = counts.get(a).unwrap();
                let count_b = counts.get(b).unwrap();
                if count_a != count_b { count_b.cmp(count_a) }
                else {
                    let idx_a = first_appearance.get(a).unwrap();
                    let idx_b = first_appearance.get(b).unwrap();
                    idx_a.cmp(idx_b)
                }
            });
            let mut remap = HashMap::new();
            for (new, &old) in sorted_ids.iter().enumerate() { remap.insert(old, new as u32); }
            let mut new_skels = vec![String::new(); num_templates];
            let mut new_cols = HashMap::new();
            for (old, &new) in &remap {
                new_skels[new as usize] = self.skeletons_list[*old as usize].clone();
                new_cols.insert(new, self.columns_storage.remove(old).unwrap());
            }
            self.skeletons_list = new_skels;
            self.columns_storage = new_cols;
            self.stream_template_ids = self.stream_template_ids.iter().map(|id| remap[id]).collect();
        }

        // 6. Serialization
        let reg_sep = "\x1E";
        let raw_registry = self.skeletons_list.join(reg_sep).into_bytes();
        let mut raw_ids = Vec::new();
        let id_mode_flag;
        if num_templates == 1 { id_mode_flag = 3; }
        else if num_templates < 256 {
            id_mode_flag = 2;
            for &id in &self.stream_template_ids { raw_ids.push(id as u8); }
        } else if num_templates > 65535 {
            id_mode_flag = 1;
            for &id in &self.stream_template_ids { raw_ids.extend_from_slice(&id.to_le_bytes()); }
        } else {
            id_mode_flag = 0;
            for &id in &self.stream_template_ids { raw_ids.extend_from_slice(&(id as u16).to_le_bytes()); }
        }

        // --- SAFE SEPARATORS LOGIC ---
        let row_sep = b"\x00";
        let col_sep_split = b"\xFF\xFF";

        // FIX: Use 0x02 for unified column separator to avoid collision with Escape (0x01)
        let col_sep_unified = b"\x02";

        let esc_char = b"\x01";

        // Escape Mappings:
        // 0x01 -> 0x01 0x01
        // 0x00 -> 0x01 0x00 (Row Separator found in data)
        // 0x02 -> 0x01 0x03 (Column Separator found in data)
        let esc_seq_esc = b"\x01\x01";
        let esc_seq_sep = b"\x01\x00";
        let esc_seq_col = b"\x01\x03";

        let mut vars_buffer = Vec::new();
        let is_unified = decision_mode == "UNIFIED";
        let col_sep = if is_unified { col_sep_unified.as_slice() } else { col_sep_split.as_slice() };

        for t_id in 0..self.skeletons_list.len() {
            if let Some(cols) = self.columns_storage.get(&(t_id as u32)) {
                for col in cols {
                    for (idx, v) in col.iter().enumerate() {
                        if idx > 0 { vars_buffer.extend_from_slice(row_sep); }
                        let v_bytes = v.as_bytes();
                        if is_unified {
                            for &b in v_bytes {
                                if b == esc_char[0] { vars_buffer.extend_from_slice(esc_seq_esc); }
                                else if b == row_sep[0] { vars_buffer.extend_from_slice(esc_seq_sep); }
                                else if b == col_sep_unified[0] { vars_buffer.extend_from_slice(esc_seq_col); } // Safety Fix
                                else { vars_buffer.push(b); }
                            }
                        } else { vars_buffer.extend_from_slice(v_bytes); }
                    }
                    vars_buffer.extend_from_slice(col_sep);
                }
            }
        }

        // 7. Final Compression
        if decision_mode == "SPLIT" {
            let c_reg = compress_with_7z(&raw_registry);
            let c_ids = compress_with_7z(&raw_ids);
            let c_vars = compress_with_7z(&vars_buffer);
            (c_reg, c_ids, c_vars, id_mode_flag, self.mode_name.clone())
        } else {
            let len_reg = raw_registry.len() as u32;
            let len_ids = raw_ids.len() as u32;
            let mut solid = Vec::new();
            solid.extend_from_slice(&len_reg.to_le_bytes());
            solid.extend_from_slice(&len_ids.to_le_bytes());
            solid.extend_from_slice(&raw_registry);
            solid.extend_from_slice(&raw_ids);
            solid.extend_from_slice(&vars_buffer);
            let c_solid = compress_with_7z(&solid);
            (Vec::new(), Vec::new(), c_solid, id_mode_flag, self.mode_name.clone())
        }
    }

    fn create_passthrough(&self, data: &[u8], reason: &str) -> (Vec<u8>, Vec<u8>, Vec<u8>, u8, String) {
        let c_vars = compress_with_7z(data);
        (Vec::new(), Vec::new(), c_vars, 255, reason.to_string())
    }
}

// --- DECOMPRESSOR (Zero-Alloc / Zero-Copy Logic with 7z) ---

pub struct CASTDecompressor;
impl CASTDecompressor {
    pub fn decompress(&self, c_reg: &[u8], c_ids: &[u8], c_vars: &[u8], expected_crc: u32, id_flag: u8) -> Vec<u8> {
        if id_flag == 255 { return decompress_with_7z(c_vars); }
        let is_unified = c_reg.is_empty() && c_ids.is_empty();
        let reg_data_bytes;
        let mut ids_data_bytes = Vec::new();
        let vars_data_bytes;

        if is_unified {
            let full = decompress_with_7z(c_vars);
            if full.len() < 8 { return Vec::new(); } // Safety check
            let len_reg = u32::from_le_bytes(full[0..4].try_into().unwrap()) as usize;
            let len_ids = u32::from_le_bytes(full[4..8].try_into().unwrap()) as usize;
            let mut off = 8;
            reg_data_bytes = full[off..off+len_reg].to_vec();
            off += len_reg;
            if id_flag != 3 {
                ids_data_bytes = full[off..off+len_ids].to_vec();
                off += len_ids;
            }
            vars_data_bytes = full[off..].to_vec();
        } else {
            reg_data_bytes = decompress_with_7z(c_reg);
            if id_flag != 3 { ids_data_bytes = decompress_with_7z(c_ids); }
            vars_data_bytes = decompress_with_7z(c_vars);
        }

        let reg_len = reg_data_bytes.len();
        let reg_str = String::from_utf8(reg_data_bytes).unwrap();
        let skeletons: Vec<&str> = reg_str.split("\x1E").collect();

        let mut template_ids = Vec::new();
        if id_flag == 2 {
            for &b in &ids_data_bytes { template_ids.push(b as usize); }
        } else if id_flag == 1 {
            for ch in ids_data_bytes.chunks_exact(4) { template_ids.push(u32::from_le_bytes(ch.try_into().unwrap()) as usize); }
        } else if id_flag == 0 {
            for ch in ids_data_bytes.chunks_exact(2) { template_ids.push(u16::from_le_bytes(ch.try_into().unwrap()) as usize); }
        }

        let col_sep_unified = b"\x02";
        let col_sep_split = b"\xFF\xFF";
        let sep = if is_unified { col_sep_unified.as_slice() } else { col_sep_split.as_slice() };

        // --- STEP 1: Column Indexing (Zero Copy) ---
        let mut raw_columns_offsets = Vec::new();
        let mut start = 0;
        let mut i = 0;
        let max_len = vars_data_bytes.len();

        while i <= max_len.saturating_sub(sep.len()) {
            if &vars_data_bytes[i..i+sep.len()] == sep {
                raw_columns_offsets.push((start, i));
                i += sep.len();
                start = i;
            } else { i += 1; }
        }
        if start < max_len { raw_columns_offsets.push((start, max_len)); }
        if let Some((s, e)) = raw_columns_offsets.last() { if s == e { raw_columns_offsets.pop(); } }

        // --- STEP 2: Cell Parsing (Zero Alloc) ---
        let mut columns_storage: Vec<Vec<VecDeque<(usize, usize)>>> = vec![Vec::new(); skeletons.len()];
        let mut col_iter = raw_columns_offsets.into_iter();
        let row_sep_byte = b"\x00"[0];

        for (t_idx, skel) in skeletons.iter().enumerate() {
            let num_vars = skel.matches('\x00').count();
            for _ in 0..num_vars {
                if let Some((col_start, col_end)) = col_iter.next() {
                    let mut deque = VecDeque::new();

                    if !is_unified {
                        let mut curr = col_start;
                        while curr < col_end {
                            let mut next_sep = curr;
                            while next_sep < col_end && vars_data_bytes[next_sep] != row_sep_byte {
                                next_sep += 1;
                            }
                            deque.push_back((curr, next_sep));
                            curr = next_sep + 1;
                        }
                    } else {
                        // Unified Parser
                        let mut curr = col_start;
                        let mut cell_start = curr;
                        while curr < col_end {
                             if vars_data_bytes[curr] == 0x01 {
                                 curr += 2;
                             } else if vars_data_bytes[curr] == row_sep_byte {
                                 deque.push_back((cell_start, curr));
                                 curr += 1;
                                 cell_start = curr;
                             } else {
                                 curr += 1;
                             }
                        }
                        deque.push_back((cell_start, curr));
                    }
                    columns_storage[t_idx].push(deque);
                }
            }
        }

        let skel_parts_cache: Vec<Vec<&str>> = skeletons.iter().map(|s| s.split("\x00").collect()).collect();
        let mut final_blob = Vec::with_capacity(vars_data_bytes.len() + reg_len);

        // --- STEP 3: Reconstruction ---

        // FIX: La closure ora accetta `blob` come argomento mutabile invece di catturarlo
        let append_unescaped = |blob: &mut Vec<u8>, slice: &[u8]| {
            if is_unified {
                let mut k = 0;
                while k < slice.len() {
                    if slice[k] == 0x01 && k+1 < slice.len() {
                        let nb = slice[k+1];
                        if nb == 0x01 { blob.push(0x01); }
                        else if nb == 0x00 { blob.push(0x00); } // Was 0x00 in data
                        else if nb == 0x03 { blob.push(0x02); } // Was 0x02 in data
                        k += 2;
                    } else {
                        blob.push(slice[k]);
                        k += 1;
                    }
                }
            } else {
                blob.extend_from_slice(slice);
            }
        };

        if id_flag == 3 {
            let parts = &skel_parts_cache[0];
            let queues = &mut columns_storage[0];
            let num_rows = if !queues.is_empty() { queues[0].len() } else { 0 };
            for _ in 0..num_rows {
                for (idx, part) in parts.iter().enumerate() {
                    // Accesso diretto a final_blob consentito qui
                    final_blob.extend_from_slice(part.as_bytes());
                    if idx < queues.len() {
                        if let Some((s, e)) = queues[idx].pop_front() {
                            // Passiamo &mut final_blob alla funzione
                            append_unescaped(&mut final_blob, &vars_data_bytes[s..e]);
                        }
                    }
                }
            }
        } else {
            for &t_id in &template_ids {
                let parts = &skel_parts_cache[t_id];
                let queues = &mut columns_storage[t_id];
                for (idx, part) in parts.iter().enumerate() {
                    // Accesso diretto a final_blob consentito qui
                    final_blob.extend_from_slice(part.as_bytes());
                    if idx < queues.len() {
                        if let Some((s, e)) = queues[idx].pop_front() {
                             // Passiamo &mut final_blob alla funzione
                             append_unescaped(&mut final_blob, &vars_data_bytes[s..e]);
                        }
                    }
                }
            }
        }

        let mut h = Hasher::new();
        h.update(&final_blob);
        let crc = h.finalize();
        if crc != expected_crc { eprintln!("CRC ERROR! Expected: {}, Calculated: {}", expected_crc, crc); }
        final_blob
    }
}