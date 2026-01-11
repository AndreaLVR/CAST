use std::collections::{HashMap, HashSet, VecDeque};
use std::borrow::Cow;
use crc32fast::Hasher;

// ============================================================================
//  TRAITS FOR ABSTRACTION
// ============================================================================

pub trait NativeCompressor {
    fn compress(&self, data: &[u8]) -> Vec<u8>;
}

pub trait NativeDecompressor {
    fn decompress(&self, data: &[u8]) -> Vec<u8>;
}

// ============================================================================
//  CONSTANTS & CONFIG
// ============================================================================

// Safe Placeholders (Private Use Area)
const VAR_PLACEHOLDER: char = '\u{E000}';
const VAR_PLACEHOLDER_STR: &str = "\u{E000}";
const VAR_PLACEHOLDER_QUOTE: &str = "\"\u{E000}\"";
const REG_SEPARATOR: &str = "\u{E001}";


#[derive(Clone)]
struct ColumnBuffer {
    data: Vec<u8>,
    offsets: Vec<usize>
}

impl ColumnBuffer {
    fn new() -> Self {
        Self {
            data: Vec::new(),
            offsets: Vec::new()
        }
    }

    #[inline(always)]
    fn push(&mut self, s: &str) {
        self.data.extend_from_slice(s.as_bytes());
        self.offsets.push(self.data.len());
    }

    #[inline(always)]
    fn get(&self, index: usize) -> &[u8] {
        let start = if index == 0 { 0 } else { self.offsets[index - 1] };
        let end = self.offsets[index];
        if start > end { return &[]; }
        &self.data[start..end]
    }

    #[inline(always)]
    fn len(&self) -> usize {
        self.offsets.len()
    }
}


#[derive(Clone, Copy, PartialEq)]
enum ParsingMode { Strict, Aggressive }

#[inline(always)]
fn is_digit(b: u8) -> bool { b >= b'0' && b <= b'9' }

#[inline(always)]
fn is_hex_digit(b: u8) -> bool {
    (b >= b'0' && b <= b'9') || (b >= b'a' && b <= b'f') || (b >= b'A' && b <= b'F')
}

#[inline(always)]
fn is_aggr_char(b: u8) -> bool {
    (b >= b'a' && b <= b'z') || (b >= b'A' && b <= b'Z') ||
    (b >= b'0' && b <= b'9') || b == b'_' || b == b'.' || b == b'-' || b == b':'
}

// Helper per Binary Guard
#[inline(always)]
fn is_likely_binary(data: &[u8]) -> bool {
    let limit = std::cmp::min(data.len(), 4096);
    let sample = &data[..limit];
    let mut control_count = 0;
    for &b in sample {
        // 0..8 (Bin), 9..13 (Space safe), 14..31 (Bin), 127 (DEL safe-ish)
        if b < 9 || (b > 13 && b < 32) {
            control_count += 1;
        }
    }
    (control_count as f64 / limit as f64) > 0.01
}

#[inline(always)]
fn match_strict_number(bytes: &[u8]) -> usize {
    let len = bytes.len();
    let mut i = 0;
    if i < len && bytes[i] == b'-' { i += 1; }
    if i >= len || !is_digit(bytes[i]) { return 0; }
    while i < len && is_digit(bytes[i]) { i += 1; }
    if i + 1 < len && bytes[i] == b'.' {
        if is_digit(bytes[i+1]) {
            i += 2;
            while i < len && is_digit(bytes[i]) { i += 1; }
        }
    }
    i
}

#[inline(always)]
fn match_strict_hex(bytes: &[u8]) -> usize {
    if bytes.len() < 3 { return 0; }
    if bytes[0] == b'0' && bytes[1] == b'x' && is_hex_digit(bytes[2]) {
        let mut i = 3;
        while i < bytes.len() && is_hex_digit(bytes[i]) { i += 1; }
        return i;
    }
    0
}

#[inline(never)]
fn parse_line_manual<'a>(line: &'a str, mode: ParsingMode, buffer_vars: &mut Vec<&'a str>, buffer_skel: &mut String) -> bool {
    // FAIL-SAFE: Collision detection
    if line.contains(VAR_PLACEHOLDER) || line.contains(REG_SEPARATOR) {
        return false;
    }

    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    let mut last_struct_start = 0;

    while i < len {
        let b = bytes[i];

        // 1. QUOTED STRING
        if b == b'"' {
            let mut k = 1;
            let mut closed = false;
            let remaining = &bytes[i..];

            while k < remaining.len() {
                let curr = remaining[k];
                if curr == b'"' {
                    if k + 1 < remaining.len() && remaining[k+1] == b'"' {
                         k += 2;
                    } else {
                        k += 1;
                        closed = true;
                        break;
                    }
                } else if curr == b'\\' {
                    k += 2;
                } else {
                    k += 1;
                }
            }

            if closed {
                let matched_len = k;
                let end_content = if matched_len > 1 { matched_len - 1 } else { 1 };
                let content = &line[i+1 .. i+end_content];

                if i > last_struct_start { buffer_skel.push_str(&line[last_struct_start..i]); }
                buffer_vars.push(content);
                buffer_skel.push_str(VAR_PLACEHOLDER_QUOTE);

                i += matched_len;
                last_struct_start = i;
                continue;
            }
        }

        // 2. TOKENS
        let mut matched_len = 0;
        let remaining = &bytes[i..];

        if mode == ParsingMode::Aggressive {
            if is_aggr_char(b) {
                let mut k = 1;
                while k < remaining.len() && is_aggr_char(remaining[k]) { k += 1; }
                matched_len = k;
            }
        } else {
            matched_len = match_strict_hex(remaining);
            if matched_len == 0 {
                matched_len = match_strict_number(remaining);
            }
        }

        if matched_len > 0 {
            if i > last_struct_start { buffer_skel.push_str(&line[last_struct_start..i]); }

            let token = &line[i .. i+matched_len];
            buffer_vars.push(token);
            buffer_skel.push(VAR_PLACEHOLDER);

            i += matched_len;
            last_struct_start = i;
        } else {
            i += 1;
        }
    }

    if last_struct_start < len {
        buffer_skel.push_str(&line[last_struct_start..]);
    }

    true
}

// ============================================================================
//  UTILS
// ============================================================================

fn decode_python_latin1(data: &[u8]) -> String {
    data.iter().map(|&b| b as char).collect()
}

fn encode_back_to_latin1(utf8_data: Vec<u8>) -> Vec<u8> {
    let s = String::from_utf8(utf8_data).expect("CRITICAL: Failed to parse UTF-8 during Latin-1 restoration");
    s.chars().map(|c| c as u8).collect()
}

// ============================================================================
//  CAST COMPRESSOR (OPTIMIZED & SAFE)
// ============================================================================

pub struct CASTCompressor<C: NativeCompressor> {
    template_map: HashMap<String, u32>,
    skeletons_list: Vec<String>,
    stream_template_ids: Vec<u32>,
    columns_storage: HashMap<u32, Vec<ColumnBuffer>>,
    next_template_id: u32,
    mode: ParsingMode,
    backend: C, // Abstract Backend
}

impl<C: NativeCompressor> CASTCompressor<C> {
    // NEW: Constructor takes the backend instance instead of config
    pub fn new(backend: C) -> Self {
        CASTCompressor {
            template_map: HashMap::new(),
            skeletons_list: Vec::new(),
            stream_template_ids: Vec::new(),
            columns_storage: HashMap::new(),
            next_template_id: 0,
            mode: ParsingMode::Strict,
            backend,
        }
    }

    fn analyze_strategy(&mut self, text: &str) {
        let sample_limit = 1000;
        let mut strict_templates = HashSet::new();
        let mut line_count = 0;
        let mut temp_vars = Vec::with_capacity(16);
        let mut temp_skel = String::with_capacity(256);

        for line in text.lines().take(sample_limit) {
            line_count += 1;
            temp_vars.clear();
            temp_skel.clear();
            let line_sample = if line.len() > 16384 { &line[..16384] } else { line };
            // Analysis ignores collisions
            parse_line_manual(line_sample, ParsingMode::Strict, &mut temp_vars, &mut temp_skel);
            strict_templates.insert(temp_skel.clone());
        }

        if line_count == 0 { return; }
        let ratio = strict_templates.len() as f64 / line_count as f64;
        self.mode = if ratio > 0.10 { ParsingMode::Aggressive } else { ParsingMode::Strict };
    }

    pub fn compress(&mut self, input_data: &[u8]) -> (Vec<u8>, Vec<u8>, Vec<u8>, u8, String) {
        // [FIX] BINARY GUARD
        if is_likely_binary(input_data) {
            return self.create_passthrough(input_data, "Binary Guard Detected");
        }

        let (text_cow, is_latin1) = match std::str::from_utf8(input_data) {
            Ok(s) => (Cow::Borrowed(s), false),
            Err(_) => {
                let s = decode_python_latin1(input_data);
                (Cow::Owned(s), true)
            }
        };

        let text_slice = text_cow.as_ref();
        self.analyze_strategy(text_slice);

        let lines = text_slice.split_inclusive('\n');
        let mut vars_cache: Vec<&str> = Vec::with_capacity(32);
        let mut skel_cache = String::with_capacity(512);

        let line_count_real = text_slice.as_bytes().iter().filter(|&&b| b == b'\n').count() + 1;
        let unique_limit = (line_count_real as f64 * if self.mode == ParsingMode::Aggressive { 0.40 } else { 0.25 }) as u32;

        for line in lines {
            if line.is_empty() { continue; }

            vars_cache.clear();
            skel_cache.clear();

            // Safe parsing
            if !parse_line_manual(line, self.mode, &mut vars_cache, &mut skel_cache) {
                 return self.create_passthrough(input_data, "Collision Protected");
            }

            let t_id;
            if let Some(&id) = self.template_map.get(&skel_cache) {
                t_id = id;
            } else {
                if self.next_template_id > unique_limit && self.next_template_id > 100 {
                    return self.create_passthrough(input_data, "Passthrough [Entropy]");
                }
                t_id = self.next_template_id;
                self.template_map.insert(skel_cache.clone(), t_id);
                self.skeletons_list.push(skel_cache.clone());
                self.columns_storage.insert(t_id, Vec::new());
                self.next_template_id += 1;
            }

            self.stream_template_ids.push(t_id);
            let cols = self.columns_storage.get_mut(&t_id).unwrap();

            if cols.is_empty() {
                for _ in 0..vars_cache.len() { cols.push(ColumnBuffer::new()); }
            }

            let limit = std::cmp::min(vars_cache.len(), cols.len());
            for i in 0..limit {
                cols[i].push(vars_cache[i]);
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
                        let limit_sample = std::cmp::min(col.len(), 50);
                        for k in 0..limit_sample {
                            sample_buffer.extend_from_slice(col.get(k));
                            collected += 1;
                        }
                    }
                }
                if collected > 2000 { break; }
            }
            if !sample_buffer.is_empty() {
                // Heuristic compression - Using backend just for the heuristic check
                // Note: The original code created a new XzEncoder(1) here.
                // We will use the backend to simulate this or assume backend handles it.
                // STRICTLY ADHERING TO "NO LOGIC CHANGE":
                // We use the backend to compress. The backend implementation must match what was here.
                let c_sample = self.backend.compress(&sample_buffer);
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
                if let Some(buf) = self.columns_storage.remove(old) {
                    new_cols.insert(new, buf);
                }
            }
            self.skeletons_list = new_skels;
            self.columns_storage = new_cols;
            self.stream_template_ids = self.stream_template_ids.iter().map(|id| remap[id]).collect();
        }

        // 6. Serialization
        let raw_registry = self.skeletons_list.join(REG_SEPARATOR).into_bytes();
        let mut raw_ids = Vec::new();
        let mut id_mode_flag;

        let total_rows = self.stream_template_ids.len() as u32;

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

        if is_latin1 { id_mode_flag |= 0x80; }

        // ALWAYS ESCAPED MODE
        let row_sep = b"\x00";
        let col_sep = b"\x02";
        let esc_char = b"\x01";

        let esc_seq_esc = b"\x01\x01";
        let esc_seq_sep = b"\x01\x00";
        let esc_seq_col = b"\x01\x03";

        let mut vars_buffer = Vec::with_capacity(input_data.len());

        for t_id in 0..self.skeletons_list.len() {
            if let Some(cols) = self.columns_storage.get(&(t_id as u32)) {
                for col_buf in cols {
                    for idx in 0..col_buf.len() {
                        if idx > 0 { vars_buffer.extend_from_slice(row_sep); }
                        let v_bytes = col_buf.get(idx);

                        // Byte Stuffing (Always)
                        for &b in v_bytes {
                            if b == esc_char[0] { vars_buffer.extend_from_slice(esc_seq_esc); }
                            else if b == row_sep[0] { vars_buffer.extend_from_slice(esc_seq_sep); }
                            else if b == col_sep[0] { vars_buffer.extend_from_slice(esc_seq_col); }
                            else { vars_buffer.push(b); }
                        }
                    }
                    vars_buffer.extend_from_slice(col_sep);
                }
            }
        }

        let mode_str = match self.mode {
            ParsingMode::Strict => "Strict",
            ParsingMode::Aggressive => "Aggressive"
        };

        // 7. Final compression (Delegated to Backend)
        if decision_mode == "SPLIT" {
            let c_reg = self.backend.compress(&raw_registry);
            let c_ids = self.backend.compress(&raw_ids);
            let c_vars = self.backend.compress(&vars_buffer);
            (c_reg, c_ids, c_vars, id_mode_flag, mode_str.to_string())
        } else {
            let len_reg = raw_registry.len() as u32;

            // [FIX SAFE] HYBRID LOGIC FOR BIT-PERFECT BACKWARDS COMPATIBILITY
            let len_ids = if (id_mode_flag & 0x7F) == 3 {
                 let has_vars = if let Some(cols) = self.columns_storage.get(&self.stream_template_ids[0]) {
                     !cols.is_empty()
                 } else { false };

                 if has_vars { 0 } else { total_rows }
            } else {
                raw_ids.len() as u32
            };

            let mut solid = Vec::new();
            solid.extend_from_slice(&len_reg.to_le_bytes());
            solid.extend_from_slice(&len_ids.to_le_bytes());
            solid.extend_from_slice(&raw_registry);
            solid.extend_from_slice(&raw_ids);
            solid.extend_from_slice(&vars_buffer);
            let c_solid = self.backend.compress(&solid);
            (Vec::new(), Vec::new(), c_solid, id_mode_flag, mode_str.to_string())
        }
    }

    fn create_passthrough(&self, data: &[u8], reason: &str) -> (Vec<u8>, Vec<u8>, Vec<u8>, u8, String) {
        println!("[!] Switching to Passthrough ({})", reason);
        let c_vars = self.backend.compress(data);
        (Vec::new(), Vec::new(), c_vars, 255, reason.to_string())
    }
}

// ============================================================================
//  CAST DECOMPRESSOR (OPTIMIZED)
// ============================================================================

pub struct CASTDecompressor<D: NativeDecompressor> {
    backend: D
}

impl<D: NativeDecompressor> CASTDecompressor<D> {
    pub fn new(backend: D) -> Self {
        Self { backend }
    }

    pub fn decompress(&self, c_reg: &[u8], c_ids: &[u8], c_vars: &[u8], expected_crc: u32, id_flag_raw: u8) -> Result<Vec<u8>, String> {
        if id_flag_raw == 255 {
            let data = self.backend.decompress(c_vars);
            return Ok(data);
        }

        let is_latin1 = (id_flag_raw & 0x80) != 0;
        let id_flag = id_flag_raw & 0x7F;

        let is_unified = c_reg.is_empty() && c_ids.is_empty();
        let reg_data_bytes;
        let mut ids_data_bytes = Vec::new();
        let vars_data_bytes;

        let mut num_rows_single_template_header = 0;

        if is_unified {
            let full = self.backend.decompress(c_vars);
            if full.len() < 8 { return Err("Corrupted Archive (Header)".to_string()); }
            let len_reg = u32::from_le_bytes(full[0..4].try_into().unwrap()) as usize;

            // This reads either ID length OR Row Count (depending on mode/content)
            let len_ids_or_rows = u32::from_le_bytes(full[4..8].try_into().unwrap()) as usize;

            let mut off = 8;
            if off + len_reg > full.len() { return Err("Corrupted Archive (Registry Length)".to_string()); }
            reg_data_bytes = full[off..off+len_reg].to_vec();
            off += len_reg;

            if id_flag != 3 {
                if off + len_ids_or_rows > full.len() { return Err("Corrupted Archive (IDs Length)".to_string()); }
                ids_data_bytes = full[off..off+len_ids_or_rows].to_vec();
                off += len_ids_or_rows;
            } else {
                // If mode 3, store it potentially as row count
                num_rows_single_template_header = len_ids_or_rows;
            }
            vars_data_bytes = full[off..].to_vec();
        } else {
            reg_data_bytes = self.backend.decompress(c_reg);
            if id_flag != 3 { ids_data_bytes = self.backend.decompress(c_ids); }
            vars_data_bytes = self.backend.decompress(c_vars);
        }

        let reg_str = String::from_utf8(reg_data_bytes).map_err(|_| "Registry corrupted (not UTF-8)".to_string())?;
        // Safe split
        let skeletons: Vec<&str> = reg_str.split(REG_SEPARATOR).collect();

        let mut template_ids = Vec::new();
        if id_flag == 2 {
            for &b in &ids_data_bytes { template_ids.push(b as usize); }
        } else if id_flag == 1 {
            for ch in ids_data_bytes.chunks_exact(4) { template_ids.push(u32::from_le_bytes(ch.try_into().unwrap()) as usize); }
        } else if id_flag == 0 {
            for ch in ids_data_bytes.chunks_exact(2) { template_ids.push(u16::from_le_bytes(ch.try_into().unwrap()) as usize); }
        }

        // DE-SERIALIZATION (ALWAYS ESCAPED)
        let col_sep = b"\x02";
        let row_sep = b"\x00";

        let mut raw_columns_offsets = Vec::new();
        let mut start = 0;
        let mut i = 0;
        let max_len = vars_data_bytes.len();

        // 1. Scan Column Boundaries
        while i < max_len {
            if vars_data_bytes[i] == 0x01 {
                i += 2;
            } else if vars_data_bytes[i] == col_sep[0] {
                raw_columns_offsets.push((start, i));
                i += 1;
                start = i;
            } else {
                i += 1;
            }
        }
        if start < max_len { raw_columns_offsets.push((start, max_len)); }

        let mut columns_storage: Vec<Vec<VecDeque<(usize, usize)>>> = vec![Vec::new(); skeletons.len()];
        let mut col_iter = raw_columns_offsets.into_iter();

        for (t_idx, skel) in skeletons.iter().enumerate() {
            let num_vars = skel.matches(VAR_PLACEHOLDER).count();
            for _ in 0..num_vars {
                if let Some((col_start, col_end)) = col_iter.next() {
                    let mut deque = VecDeque::new();
                    // 2. Scan Rows
                    let mut curr = col_start;
                    let mut cell_start = curr;
                    while curr < col_end {
                        if vars_data_bytes[curr] == 0x01 {
                            curr += 2;
                        } else if vars_data_bytes[curr] == row_sep[0] {
                            deque.push_back((cell_start, curr));
                            curr += 1;
                            cell_start = curr;
                        } else {
                            curr += 1;
                        }
                    }
                    deque.push_back((cell_start, curr));
                    columns_storage[t_idx].push(deque);
                }
            }
        }

        let skel_parts_cache: Vec<Vec<&str>> = skeletons.iter().map(|s| s.split(VAR_PLACEHOLDER_STR).collect()).collect();
        let mut final_blob = Vec::with_capacity(vars_data_bytes.len() + reg_str.len());

        let append_unescaped = |blob: &mut Vec<u8>, slice: &[u8]| {
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
        };

        // [FIX SAFE] Hybrid Logic
        let num_rows_single_template = if id_flag == 3 {
             // 1. Try Header (Fix for static files)
             let mut n = num_rows_single_template_header;

             // 2. Fallback (Legacy/Bit-Perfect Benchmark Case)
             if n == 0 && !columns_storage.is_empty() && !columns_storage[0].is_empty() {
                 n = columns_storage[0][0].len();
             }
             n
        } else { 0 };

        let mut reconstruct = |t_id: usize| {
             // Safe check index
             if t_id >= skel_parts_cache.len() { return; }

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
        };

        if id_flag == 3 {
            for _ in 0..num_rows_single_template { reconstruct(0); }
        } else {
            for &t_id in &template_ids { reconstruct(t_id); }
        }

        let final_data = if is_latin1 { encode_back_to_latin1(final_blob) } else { final_blob };

        let mut h = Hasher::new();
        h.update(&final_data);
        let crc = h.finalize();
        if crc != expected_crc { eprintln!("CRC Check Failed. Expected: {}, Got: {}", expected_crc, crc); }
        Ok(final_data)
    }
}
