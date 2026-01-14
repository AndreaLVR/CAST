use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{Read, Write, BufRead, BufReader, Seek, SeekFrom};
// RIMOSSO: use std::borrow::Cow; (Non usato)
// RIMOSSO: use crc32fast::Hasher; (Non usato in questo file, ci pensa il backend LZMA)

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

const VAR_PLACEHOLDER: char = '\u{E000}';
const VAR_PLACEHOLDER_STR: &str = "\u{E000}";
const VAR_PLACEHOLDER_QUOTE: &str = "\"\u{E000}\"";
const REG_SEPARATOR: &str = "\u{E001}";

// Magic bytes for footer identification: "C", "A", "S", "T", version 1
const FOOTER_MAGIC: [u8; 5] = [b'C', b'A', b'S', b'T', 0x01];

// Configuration for Random Access Chunks
const DEFAULT_CHUNK_ROWS: usize = 200_000;

#[derive(Clone, Debug)]
pub struct RowGroupMetadata {
    pub start_offset: u64,
    pub compressed_size: u64,
    pub num_rows: u64,
    pub kind: u8, // 0 = CAST, 1 = RAW (Binary/Passthrough)
}

#[derive(Clone)]
struct ColumnBuffer {
    data: Vec<u8>,
    offsets: Vec<usize>
}

impl ColumnBuffer {
    fn new() -> Self {
        Self { data: Vec::new(), offsets: Vec::new() }
    }

    fn clear(&mut self) {
        self.data.clear();
        self.offsets.clear();
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

#[derive(Clone, Copy, PartialEq, Debug)]
enum ParsingMode { Strict, Aggressive }

// ============================================================================
//  PARSING HELPERS (UNCHANGED LOGIC)
// ============================================================================

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

#[inline(always)]
fn is_likely_binary(data: &[u8]) -> bool {
    let limit = std::cmp::min(data.len(), 4096);
    let sample = &data[..limit];
    let mut control_count = 0;
    for &b in sample {
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

#[allow(dead_code)]
fn decode_python_latin1(data: &[u8]) -> String {
    data.iter().map(|&b| b as char).collect()
}

// ============================================================================
//  CAST COMPRESSOR (UPDATED FOR ROW GROUPS)
// ============================================================================

pub struct CASTCompressor<C: NativeCompressor> {
    template_map: HashMap<String, u32>,
    skeletons_list: Vec<String>,
    stream_template_ids: Vec<u32>,
    columns_storage: HashMap<u32, Vec<ColumnBuffer>>,
    next_template_id: u32,
    mode: ParsingMode,
    backend: C,

    // NEW: Chunking State
    rows_in_current_block: usize,
    chunk_limit_rows: usize,
}

impl<C: NativeCompressor> CASTCompressor<C> {
    pub fn new(backend: C) -> Self {
        CASTCompressor {
            template_map: HashMap::new(),
            skeletons_list: Vec::new(),
            stream_template_ids: Vec::new(),
            columns_storage: HashMap::new(),
            next_template_id: 0,
            mode: ParsingMode::Strict,
            backend,
            rows_in_current_block: 0,
            chunk_limit_rows: DEFAULT_CHUNK_ROWS,
        }
    }

    pub fn set_chunk_size(&mut self, rows: usize) {
        self.chunk_limit_rows = rows;
    }

    fn reset_block_state(&mut self) {
        self.template_map.clear();
        self.skeletons_list.clear();
        self.stream_template_ids.clear();

        // Optimization: Don't deallocate columns, just clear them to reuse capacity
        for cols in self.columns_storage.values_mut() {
            for col in cols.iter_mut() {
                col.clear();
            }
        }
        self.next_template_id = 0;
        self.rows_in_current_block = 0;
    }

    fn analyze_strategy_from_sample(&mut self, text: &str) {
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
            parse_line_manual(line_sample, ParsingMode::Strict, &mut temp_vars, &mut temp_skel);
            strict_templates.insert(temp_skel.clone());
        }

        if line_count == 0 { return; }
        let ratio = strict_templates.len() as f64 / line_count as f64;
        self.mode = if ratio > 0.10 { ParsingMode::Aggressive } else { ParsingMode::Strict };
    }

    /// Helper to process one block and return compressed bytes + kind
    fn flush_current_block(&mut self) -> (Vec<u8>, u8) {
        if self.rows_in_current_block == 0 {
            return (Vec::new(), 0);
        }

        let num_templates = self.skeletons_list.len();
        let mut decision_mode = "UNIFIED";

        // Heuristic Logic
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
                let c_sample = self.backend.compress(&sample_buffer);
                if (sample_buffer.len() as f64 / (c_sample.len() as f64 + 1.0)) < 3.0 {
                    decision_mode = "SPLIT";
                }
            }
        }

        // Unified Remapping
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
             let mut old_cols = std::mem::take(&mut self.columns_storage);
             let mut new_cols = HashMap::new();

             for (old, &new) in &remap {
                 new_skels[new as usize] = self.skeletons_list[*old as usize].clone();
                 if let Some(buf) = old_cols.remove(old) {
                     new_cols.insert(new, buf);
                 }
             }
             self.skeletons_list = new_skels;
             self.columns_storage = new_cols;
             self.stream_template_ids = self.stream_template_ids.iter().map(|id| remap[id]).collect();
        }

        // Serialization
        let raw_registry = self.skeletons_list.join(REG_SEPARATOR).into_bytes();
        let mut raw_ids = Vec::new();
        // FIX: Rimosso 'mut', non viene modificato dopo l'inizializzazione condizionale
        let id_mode_flag;
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

        let row_sep = b"\x00";
        let col_sep = b"\x02";
        let esc_char = b"\x01";
        let esc_seq_esc = b"\x01\x01";
        let esc_seq_sep = b"\x01\x00";
        let esc_seq_col = b"\x01\x03";

        let mut vars_buffer = Vec::with_capacity(total_rows as usize * 50);

        for t_id in 0..self.skeletons_list.len() {
            if let Some(cols) = self.columns_storage.get(&(t_id as u32)) {
                for col_buf in cols {
                    for idx in 0..col_buf.len() {
                        if idx > 0 { vars_buffer.extend_from_slice(row_sep); }
                        let v_bytes = col_buf.get(idx);
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

        let len_reg = raw_registry.len() as u32;
        let len_ids = if (id_mode_flag & 0x7F) == 3 {
             let has_vars = if let Some(cols) = self.columns_storage.get(&self.stream_template_ids[0]) { !cols.is_empty() } else { false };
             if has_vars { 0 } else { total_rows }
        } else { raw_ids.len() as u32 };

        let mut solid = Vec::new();
        solid.push(id_mode_flag);
        solid.extend_from_slice(&len_reg.to_le_bytes());
        solid.extend_from_slice(&len_ids.to_le_bytes());
        solid.extend_from_slice(&raw_registry);
        solid.extend_from_slice(&raw_ids);
        solid.extend_from_slice(&vars_buffer);

        (self.backend.compress(&solid), 0)
    }

    pub fn compress_stream<R: Read, W: Write>(&mut self, input: R, mut output: W) -> std::io::Result<(u64, u64)> {
        let mut reader = BufReader::new(input);
        let mut row_groups = Vec::new();
        let mut global_offset = 0u64;
        let mut total_in = 0u64;  // Tracking
        let mut total_out = 0u64; // Tracking

        let mut initial_buf = Vec::with_capacity(4096);
        let mut buf = [0u8; 4096];
        let n = reader.read(&mut buf)?;

        initial_buf.extend_from_slice(&buf[..n]);
        let is_binary = if n > 0 { is_likely_binary(&initial_buf) } else { false };

        if is_binary {
            // Binary path: count initial_buf
            total_in += n as u64;

            println!("[!] Binary content detected. Switching to Passthrough Mode.");
            if !initial_buf.is_empty() {
                let compressed = self.backend.compress(&initial_buf);
                output.write_all(&compressed)?;
                total_out += compressed.len() as u64; // Tracking output

                row_groups.push(RowGroupMetadata {
                    start_offset: global_offset,
                    compressed_size: compressed.len() as u64,
                    num_rows: 0,
                    kind: 1,
                });
                global_offset += compressed.len() as u64;
            }
            loop {
                let mut chunk_buf = vec![0u8; 16 * 1024 * 1024];
                let n = reader.read(&mut chunk_buf)?;
                if n == 0 { break; }
                total_in += n as u64; // Tracking input

                let compressed = self.backend.compress(&chunk_buf[..n]);
                output.write_all(&compressed)?;
                total_out += compressed.len() as u64; // Tracking output

                row_groups.push(RowGroupMetadata {
                    start_offset: global_offset,
                    compressed_size: compressed.len() as u64,
                    num_rows: 0,
                    kind: 1,
                });
                global_offset += compressed.len() as u64;
            }
        } else {
            // Text path: combined_reader will re-read initial_buf, so we count inside loop
            if let Ok(s) = std::str::from_utf8(&initial_buf) { self.analyze_strategy_from_sample(s); }
            let combined_reader = std::io::Cursor::new(initial_buf).chain(reader);
            let mut line_reader = BufReader::new(combined_reader);
            let mut line_buf = String::new();
            let mut skel_cache = String::with_capacity(512);

            loop {
                line_buf.clear();
                let bytes_read = line_reader.read_line(&mut line_buf)?;
                if bytes_read == 0 { break; }

                total_in += bytes_read as u64; // Tracking input (includes newline)

                let line = line_buf.trim_end_matches(&['\r', '\n'][..]);
                if line.is_empty() { continue; }
                let mut vars_cache: Vec<&str> = Vec::with_capacity(32);
                skel_cache.clear();
                if !parse_line_manual(line, self.mode, &mut vars_cache, &mut skel_cache) { continue; }
                let t_id;
                if let Some(&id) = self.template_map.get(&skel_cache) { t_id = id; } else {
                    t_id = self.next_template_id;
                    self.template_map.insert(skel_cache.clone(), t_id);
                    self.skeletons_list.push(skel_cache.clone());
                    self.columns_storage.insert(t_id, Vec::new());
                    self.next_template_id += 1;
                }
                self.stream_template_ids.push(t_id);
                let cols = self.columns_storage.get_mut(&t_id).unwrap();
                if cols.is_empty() { for _ in 0..vars_cache.len() { cols.push(ColumnBuffer::new()); } }
                let limit = std::cmp::min(vars_cache.len(), cols.len());
                for i in 0..limit { cols[i].push(vars_cache[i]); }
                self.rows_in_current_block += 1;
                if self.rows_in_current_block >= self.chunk_limit_rows {
                    let (bytes, kind) = self.flush_current_block();
                    if !bytes.is_empty() {
                        output.write_all(&bytes)?;
                        total_out += bytes.len() as u64; // Tracking output

                        row_groups.push(RowGroupMetadata {
                            start_offset: global_offset,
                            compressed_size: bytes.len() as u64,
                            num_rows: self.rows_in_current_block as u64,
                            kind,
                        });
                        global_offset += bytes.len() as u64;
                    }
                    self.reset_block_state();
                }
            }
            if self.rows_in_current_block > 0 {
                let (bytes, kind) = self.flush_current_block();
                output.write_all(&bytes)?;
                total_out += bytes.len() as u64; // Tracking output

                row_groups.push(RowGroupMetadata {
                    start_offset: global_offset,
                    compressed_size: bytes.len() as u64,
                    num_rows: self.rows_in_current_block as u64,
                    kind,
                });
                global_offset += bytes.len() as u64;
            }
        }
        let footer_start = global_offset;
        let mut footer_bytes = Vec::new();
        footer_bytes.extend_from_slice(&(row_groups.len() as u32).to_le_bytes());
        for rg in row_groups {
            footer_bytes.extend_from_slice(&rg.start_offset.to_le_bytes());
            footer_bytes.extend_from_slice(&rg.compressed_size.to_le_bytes());
            footer_bytes.extend_from_slice(&rg.num_rows.to_le_bytes());
            footer_bytes.push(rg.kind);
        }
        footer_bytes.extend_from_slice(&footer_start.to_le_bytes());
        footer_bytes.extend_from_slice(&FOOTER_MAGIC);
        output.write_all(&footer_bytes)?;

        total_out += footer_bytes.len() as u64; // Tracking footer

        Ok((total_in, total_out))
    }
}

// ============================================================================
//  CAST DECOMPRESSOR (UPDATED FOR RANDOM ACCESS)
// ============================================================================

pub struct CASTDecompressor<D: NativeDecompressor> {
    backend: D
}

impl<D: NativeDecompressor> CASTDecompressor<D> {
    pub fn new(backend: D) -> Self {
        Self { backend }
    }

    /// [MODIFICATO] Accetta current_global_idx e target_rows per filtrare
    fn decompress_block_blob<W: Write>(&self, data: &[u8], writer: &mut W, current_global_idx: u64, target_rows: Option<(u64, u64)>) -> Result<(), String> {
        let decompressed = self.backend.decompress(data);
        if decompressed.is_empty() { return Ok(()); }
        if decompressed.len() < 9 { return Err("Block too short".to_string()); }

        let id_mode_flag = decompressed[0];
        let mut cursor = 1;
        let len_reg = u32::from_le_bytes(decompressed[cursor..cursor+4].try_into().unwrap()) as usize; cursor += 4;
        let len_ids = u32::from_le_bytes(decompressed[cursor..cursor+4].try_into().unwrap()) as usize; cursor += 4;
        if cursor + len_reg + len_ids > decompressed.len() { return Err("Corrupted Block Header".to_string()); }

        let reg_data = &decompressed[cursor .. cursor+len_reg]; cursor += len_reg;
        let ids_data = &decompressed[cursor .. cursor+len_ids]; cursor += len_ids;
        let vars_data = &decompressed[cursor..];

        let reg_str = std::str::from_utf8(reg_data).map_err(|_| "Registry not UTF-8")?;
        let skeletons: Vec<&str> = reg_str.split(REG_SEPARATOR).collect();

        let mut template_ids = Vec::with_capacity(len_ids);
        let flag_val = id_mode_flag & 0x7F;
        if flag_val == 3 { /* Single template */ }
        else if flag_val == 2 { for &b in ids_data { template_ids.push(b as usize); } }
        else if flag_val == 1 { for ch in ids_data.chunks_exact(4) { template_ids.push(u32::from_le_bytes(ch.try_into().unwrap()) as usize); } }
        else { for ch in ids_data.chunks_exact(2) { template_ids.push(u16::from_le_bytes(ch.try_into().unwrap()) as usize); } }

        let col_sep = b"\x02"; let row_sep = b"\x00";
        let mut raw_columns_offsets = Vec::new();
        let mut start = 0; let mut i = 0;
        while i < vars_data.len() {
            if vars_data[i] == 0x01 { i += 2; }
            else if vars_data[i] == col_sep[0] { raw_columns_offsets.push((start, i)); i += 1; start = i; }
            else { i += 1; }
        }
        if start < vars_data.len() { raw_columns_offsets.push((start, vars_data.len())); }

        let mut columns_storage: Vec<Vec<VecDeque<(usize, usize)>>> = vec![Vec::new(); skeletons.len()];
        let mut col_iter = raw_columns_offsets.into_iter();
        for (t_idx, skel) in skeletons.iter().enumerate() {
            let num_vars = skel.matches(VAR_PLACEHOLDER).count();
            for _ in 0..num_vars {
                if let Some((col_start, col_end)) = col_iter.next() {
                    let mut deque = VecDeque::new();
                    let mut curr = col_start; let mut cell_start = curr;
                    while curr < col_end {
                        if vars_data[curr] == 0x01 { curr += 2; }
                        else if vars_data[curr] == row_sep[0] { deque.push_back((cell_start, curr)); curr += 1; cell_start = curr; }
                        else { curr += 1; }
                    }
                    deque.push_back((cell_start, curr));
                    columns_storage[t_idx].push(deque);
                }
            }
        }

        let skel_parts: Vec<Vec<&str>> = skeletons.iter().map(|s| s.split(VAR_PLACEHOLDER_STR).collect()).collect();
        let count_flag3 = if flag_val == 3 {
            if !columns_storage.is_empty() && !columns_storage[0].is_empty() { columns_storage[0][0].len() } else { 0 }
        } else { 0 };

        let mut write_stream = |slice: &[u8]| { writer.write_all(slice).map_err(|e| e.to_string()) };

        // [MODIFICATO] Closure di ricostruzione con filtro
        // `should_write`: se true, scrive su output. Se false, consuma solo le code per non rompere il sync.
        let mut reconstruct = |t_id: usize, should_write: bool| -> Result<(), String> {
            if t_id >= skel_parts.len() { return Ok(()); }
            let parts = &skel_parts[t_id];
            let queues = &mut columns_storage[t_id];

            for (idx, part) in parts.iter().enumerate() {
                if should_write { write_stream(part.as_bytes())?; }

                if idx < queues.len() {
                    if let Some((s, e)) = queues[idx].pop_front() {
                        if should_write {
                            let slice = &vars_data[s..e];
                            let mut k = 0;
                            while k < slice.len() {
                                if slice[k] == 0x01 && k+1 < slice.len() {
                                    let nb = slice[k+1];
                                    let b = if nb == 0x01 { 0x01 } else if nb == 0x00 { 0x00 } else { 0x02 };
                                    write_stream(&[b])?; k += 2;
                                } else { write_stream(&[slice[k]])?; k += 1; }
                            }
                        }
                    }
                }
            }
            if should_write { write_stream(b"\n")?; }
            Ok(())
        };

        // [MODIFICATO] Loop di ricostruzione che controlla l'indice di riga
        let mut local_row_counter = 0;

        let mut process_row = |id: usize| -> Result<(), String> {
            let actual_idx = current_global_idx + local_row_counter;
            let write_this = if let Some((start, end)) = target_rows {
                actual_idx >= start && actual_idx <= end
            } else { true };

            reconstruct(id, write_this)?;
            local_row_counter += 1;
            Ok(())
        };

        if flag_val == 3 { for _ in 0..count_flag3 { process_row(0)?; } }
        else { for &id in &template_ids { process_row(id)?; } }

        Ok(())
    }

    pub fn decompress_stream<R: Read + Seek, W: Write>(&self, mut input: R, mut output: W, target_rows: Option<(u64, u64)>) -> Result<(), String> {
        input.seek(SeekFrom::End(-13)).map_err(|_| "Seek failed")?;
        let mut footer_tail = [0u8; 13];
        input.read_exact(&mut footer_tail).map_err(|_| "Read footer tail failed")?;
        if &footer_tail[8..13] != &FOOTER_MAGIC { return Err("Invalid CAST file (Missing Magic Footer)".to_string()); }

        let footer_offset = u64::from_le_bytes(footer_tail[0..8].try_into().unwrap());
        input.seek(SeekFrom::Start(footer_offset)).map_err(|_| "Seek footer failed")?;

        let mut count_buf = [0u8; 4];
        if input.read_exact(&mut count_buf).is_err() { return Err("Empty Footer".to_string()); }
        let num_groups = u32::from_le_bytes(count_buf);

        let mut groups = Vec::with_capacity(num_groups as usize);
        let mut entry_buf = [0u8; 25];
        for _ in 0..num_groups {
            input.read_exact(&mut entry_buf).map_err(|_| "Read group meta failed")?;
            groups.push(RowGroupMetadata {
                start_offset: u64::from_le_bytes(entry_buf[0..8].try_into().unwrap()),
                compressed_size: u64::from_le_bytes(entry_buf[8..16].try_into().unwrap()),
                num_rows: u64::from_le_bytes(entry_buf[16..24].try_into().unwrap()),
                kind: entry_buf[24],
            });
        }

        let mut current_row_start = 0u64;

        for group in groups {
            let group_rows = group.num_rows;
            let group_end_row = current_row_start + group_rows;

            let should_process = if let Some((req_start, req_end)) = target_rows {
                if group_rows > 0 { group_end_row > req_start && current_row_start <= req_end } else { false }
            } else { true };

            if should_process {
                input.seek(SeekFrom::Start(group.start_offset)).map_err(|_| "Seek group failed")?;
                let mut handle = input.by_ref().take(group.compressed_size);
                let mut buffer = Vec::with_capacity(group.compressed_size as usize);
                handle.read_to_end(&mut buffer).map_err(|_| "Read block failed")?;

                if group.kind == 1 {
                    // Raw Blocks cannot be filtered line-by-line easily without re-parsing.
                    // Assuming we dump the whole raw block if it overlaps.
                    let raw = self.backend.decompress(&buffer);
                    output.write_all(&raw).map_err(|e| e.to_string())?;
                } else {
                    // CAST BLOCK: Now passing row info for filtering
                    self.decompress_block_blob(&buffer, &mut output, current_row_start, target_rows)?;
                }
            }
            current_row_start += group_rows;
        }
        Ok(())
    }
}