use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::File;
use std::io::{Write, Read};
use std::borrow::Cow; // IMPORTANTE: Per correggere l'errore di tipi/lifetime
use std::cmp;
use regex::Regex;
use crc32fast::Hasher;
use num_format::{Locale, ToFormattedString};
use xz2::read::XzDecoder;
use xz2::write::XzEncoder;
use xz2::stream::{Stream, MtStreamBuilder, Check, LzmaOptions, Filters};

// --- UTILS ---

pub fn format_num(n: usize) -> String {
    n.to_formatted_string(&Locale::en)
}

fn decode_python_latin1(data: &[u8]) -> String {
    data.iter().map(|&b| b as char).collect()
}

// Funzione inversa per ripristinare i byte originali da UTF-8 "espanso"
fn encode_back_to_latin1(utf8_data: Vec<u8>) -> Vec<u8> {
    let s = String::from_utf8(utf8_data).expect("CRITICAL: Failed to parse UTF-8 during Latin-1 restoration");
    s.chars().map(|c| c as u8).collect()
}

pub fn create_chaotic_log(filename: &str, num_lines: usize) {
    println!("[*] Generazione file DEMO: {}...", filename);
    let mut f = File::create(filename).unwrap();
    let users = ["admin", "guest", "service_bot", "deploy_agent"];
    let actions = ["LOGIN", "LOGOUT", "PURCHASE", "VIEW", "ERROR_CHECK"];
    for i in 0..num_lines {
        let mode = i % 10;
        if mode < 6 {
            let line = format!(r#"{{"ts": {}, "u": "{}", "act": "{}", "lat": {}}}"#,
                i, users[i % 4], actions[i % 5], 10 + (i % 100));
            writeln!(f, "{}", line).unwrap();
        } else if mode < 8 {
            writeln!(f, "2023-12-20 [INFO] User {} performed {}\r", users[i % 4], actions[i % 5]).unwrap();
        } else {
            writeln!(f, "CRITICAL FAILURE at module_{}.c: line {} (Code: {})", i % 5, i % 100, i * 7).unwrap();
        }
    }
}

// --- FUNZIONI DI COMPRESSIONE NATIVE (xz2) ---

const LZMA_PRESET_EXTREME: u32 = 0x80000000;

pub fn compress_buffer_native(data: &[u8], multithread: bool) -> Vec<u8> {
    if data.is_empty() { return Vec::new(); }

    let dict_size = 128 * 1024 * 1024;

    // Smart Switch: Se i dati sono piccoli, inutile overhead dei thread
    let effective_multithread = if multithread && (data.len() as u32) < dict_size {
        false
    } else {
        multithread
    };

    // --- CONFIGURAZIONE ---
    let mut opts = LzmaOptions::new_preset(9 | LZMA_PRESET_EXTREME).unwrap();
    opts.dict_size(dict_size);

    let mut filters = Filters::new();
    filters.lzma2(&opts);

    // Allocazione buffer di output stimata
    let estimated = data.len() / 2;
    let cap_limit = dict_size as usize;
    let safe_capacity = cmp::min(estimated, cap_limit);
    let output_buffer = Vec::with_capacity(safe_capacity);

    let writer = std::io::BufWriter::new(output_buffer);

    if !effective_multithread {
        let stream = Stream::new_stream_encoder(&filters, Check::Crc32).expect("Errore init LZMA ST");
        let mut compressor = XzEncoder::new_stream(writer, stream);
        compressor.write_all(data).expect("Errore compressione ST");
        let finished = compressor.finish().expect("Errore finish ST");
        return finished.into_inner().expect("Error into_inner");
    }

    let threads = num_cpus::get() as u32;
    let stream = MtStreamBuilder::new()
        .threads(threads)
        .filters(filters)
        .check(Check::Crc32)
        .encoder()
        .expect("Errore init LZMA MT");

    let mut compressor = XzEncoder::new_stream(writer, stream);
    compressor.write_all(data).expect("Errore compressione MT");
    let finished = compressor.finish().expect("Errore finish MT");
    return finished.into_inner().expect("Error into_inner");
}

pub fn decompress_buffer_native(data: &[u8]) -> Vec<u8> {
    if data.is_empty() { return Vec::new(); }
    let mut decompressor = XzDecoder::new(data);
    let mut output = Vec::with_capacity(data.len() * 3);
    decompressor.read_to_end(&mut output).expect("Errore decompressione dati");
    output
}

// --- COMPRESSORE (Logica CAST) ---

pub struct CASTCompressor {
    template_map: HashMap<String, u32>,
    skeletons_list: Vec<String>,
    stream_template_ids: Vec<u32>,
    columns_storage: HashMap<u32, Vec<Vec<String>>>,
    next_template_id: u32,
    mode_name: String,
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

        // 2. Decoding Logic (con Latin-1 Check + Cow Fix)
        let (text_cow, is_latin1) = match std::str::from_utf8(input_data) {
            Ok(s) => (Cow::Borrowed(s), false), // OK UTF-8 (Zero copy)
            Err(_) => {
                // Fallback Latin-1 (Allocation)
                let s = decode_python_latin1(input_data);
                (Cow::Owned(s), true)
            }
        };

        // Otteniamo un &str valido (referenziato o posseduto)
        let text_slice = text_cow.as_ref();

        let active_regex = self.analyze_strategy(text_slice);

        let lines: Vec<&str> = text_slice.split_inclusive('\n').collect();
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
                    return self.create_passthrough(input_data, "Passthrough [Entropy]");
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

        // 4. Heuristic (Fast Xz level 1)
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
                let mut e = XzEncoder::new(Vec::new(), 1);
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
        let mut id_mode_flag; // Mutabile per flaggare

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

        // --- LATIN1 FLAG INJECTION ---
        if is_latin1 {
            id_mode_flag |= 0x80;
        }

        // --- SAFE SEPARATORS LOGIC ---
        let row_sep = b"\x00";
        let col_sep_split = b"\xFF\xFF";
        let col_sep_unified = b"\x02";
        let esc_char = b"\x01";

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
                                else if b == col_sep_unified[0] { vars_buffer.extend_from_slice(esc_seq_col); }
                                else { vars_buffer.push(b); }
                            }
                        } else { vars_buffer.extend_from_slice(v_bytes); }
                    }
                    vars_buffer.extend_from_slice(col_sep);
                }
            }
        }

        // 7. Compressione Finale
        if decision_mode == "SPLIT" {
            let c_reg = compress_buffer_native(&raw_registry, self.multithread);
            let c_ids = compress_buffer_native(&raw_ids, self.multithread);
            let c_vars = compress_buffer_native(&vars_buffer, self.multithread);
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
            let c_solid = compress_buffer_native(&solid, self.multithread);
            (Vec::new(), Vec::new(), c_solid, id_mode_flag, self.mode_name.clone())
        }
    }

    fn create_passthrough(&self, data: &[u8], reason: &str) -> (Vec<u8>, Vec<u8>, Vec<u8>, u8, String) {
        let c_vars = compress_buffer_native(data, self.multithread);
        (Vec::new(), Vec::new(), c_vars, 255, reason.to_string())
    }
}

// --- DECOMPRESSORE (Native) ---

pub struct CASTDecompressor;
impl CASTDecompressor {
    pub fn decompress(&self, c_reg: &[u8], c_ids: &[u8], c_vars: &[u8], expected_crc: u32, id_flag_raw: u8) -> Vec<u8> {
        if id_flag_raw == 255 { return decompress_buffer_native(c_vars); }

        // --- DECODE FLAG ---
        let is_latin1 = (id_flag_raw & 0x80) != 0;
        let id_flag = id_flag_raw & 0x7F; // Pulisce il flag
        // -----------------------

        let is_unified = c_reg.is_empty() && c_ids.is_empty();
        let reg_data_bytes;
        let mut ids_data_bytes = Vec::new();
        let vars_data_bytes;

        if is_unified {
            let full = decompress_buffer_native(c_vars);
            if full.len() < 8 { panic!("Corrupted Archive (Header)"); }
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
            reg_data_bytes = decompress_buffer_native(c_reg);
            if id_flag != 3 { ids_data_bytes = decompress_buffer_native(c_ids); }
            vars_data_bytes = decompress_buffer_native(c_vars);
        }

        let reg_len = reg_data_bytes.len();
        // Usiamo expect perché se fallisce qui, il file è corrotto o non è quello che pensiamo
        let reg_str = String::from_utf8(reg_data_bytes).expect("Registry corrupted (not UTF-8)");
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

        // --- STEP 1: Indicizzazione Colonne ---
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

        // --- STEP 2: Parsing Celle ---
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
                        // UNIFIED: Skip escapes
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

        // --- STEP 3: Ricostruzione ---
        let append_unescaped = |blob: &mut Vec<u8>, slice: &[u8]| {
            if is_unified {
                let mut k = 0;
                while k < slice.len() {
                    if slice[k] == 0x01 && k+1 < slice.len() {
                        let nb = slice[k+1];
                        if nb == 0x01 { blob.push(0x01); }
                        else if nb == 0x00 { blob.push(0x00); }
                        else if nb == 0x03 { blob.push(0x02); }
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
                    final_blob.extend_from_slice(part.as_bytes());
                    if idx < queues.len() {
                        if let Some((s, e)) = queues[idx].pop_front() {
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
                    final_blob.extend_from_slice(part.as_bytes());
                    if idx < queues.len() {
                        if let Some((s, e)) = queues[idx].pop_front() {
                             append_unescaped(&mut final_blob, &vars_data_bytes[s..e]);
                        }
                    }
                }
            }
        }

        // --- STEP 4: LATIN1 RESTORATION ---
        let final_data = if is_latin1 {
            encode_back_to_latin1(final_blob)
        } else {
            final_blob
        };

        let mut h = Hasher::new();
        h.update(&final_data);
        let crc = h.finalize();
        if crc != expected_crc { eprintln!("CRC ERROR! Atteso: {}, Calcolato: {}", expected_crc, crc); }
        final_data
    }
}