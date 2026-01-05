import lzma
import re
import struct
import zlib
from collections import Counter
from typing import List, Tuple, Union, Optional


class CASTCompressor:
    """
    Handles the compression of structured data using the CAST (Columnar Agnostic Structural Transformation) algorithm.
    Includes Latin-1 fallback and 'Always Escaped' logic for binary safety.
    """

    # --- CONSTANTS (PRIVATE USE AREA) ---
    VAR_PLACEHOLDER = "\uE000"
    VAR_PLACEHOLDER_QUOTE = '"\uE000"'
    REG_SEP = "\uE001"

    def __init__(self) -> None:
        self.template_map = {}
        self.next_template_id = 0
        self.skeletons_list = []
        self.stream_template_ids = []
        self.columns_storage = {}

        # Regex
        self.regex_strict = re.compile(
            r'("(?:[^"\\]|\\.|"")*"|\-?\d+(?:\.\d+)?|0x[0-9a-fA-F]+)'
        )
        self.regex_aggressive = re.compile(r'("(?:[^"\\]|\\.|"")*"|[a-zA-Z0-9_.\-]+)')
        self.active_pattern = self.regex_strict
        self.mode_name = "Strict"

    def _is_likely_binary(self, data_sample: Union[bytes, str]) -> bool:
        if not data_sample:
            return False
        sample = data_sample[:4096]
        if isinstance(sample, str):
            sample = sample.encode("utf-8", errors="ignore")

        suspicious_chars = 0
        total_chars = len(sample)

        for byte_val in sample:
            if byte_val == 0 or (0 < byte_val < 32 and byte_val not in (9, 10, 13)):
                suspicious_chars += 1

        if total_chars > 0 and (suspicious_chars / total_chars > 0.01):
            return True
        return False

    def _analyze_best_strategy(self, text_sample: str) -> None:
        sample_lines = text_sample[:200000].splitlines()[:1000]
        if not sample_lines:
            return

        strict_templates = set()
        for line in sample_lines:
            # Placeholder sicuro per l'analisi
            skel = self.regex_strict.sub(lambda m: self.VAR_PLACEHOLDER, line)
            strict_templates.add(skel)

        ratio = len(strict_templates) / len(sample_lines)
        if ratio > 0.10:
            self.active_pattern = self.regex_aggressive
            self.mode_name = "Aggressive"
        else:
            self.active_pattern = self.regex_strict
            self.mode_name = "Strict"

    def _mask_line(self, line: str) -> Optional[Tuple[str, List[str]]]:
        # Fail-Safe: Se la riga contiene i nostri caratteri speciali, abortiamo
        if self.VAR_PLACEHOLDER in line or self.REG_SEP in line:
            return None

        variables = []

        def replace_callback(match):
            token = match.group(0)
            if token.startswith('"'):
                variables.append(token[1:-1])
                return self.VAR_PLACEHOLDER_QUOTE
            else:
                variables.append(token)
                return self.VAR_PLACEHOLDER

        masked_line = self.active_pattern.sub(replace_callback, line)
        return masked_line, variables

    def compress(
        self, input_data: Union[bytes, str]
    ) -> Tuple[bytes, bytes, bytes, int, str]:
        # --- 1. DECODING & LATIN-1 CHECK ---
        is_latin1 = False

        if isinstance(input_data, bytes):
            # Analisi entropia veloce
            if self._is_likely_binary(input_data):
                return self._create_passthrough(input_data, "Passthrough [Binary]")
            try:
                text_data = input_data.decode("utf-8")
            except UnicodeDecodeError:
                try:
                    text_data = input_data.decode("latin-1")
                    is_latin1 = True
                except:
                    return self._create_passthrough(
                        input_data, "Passthrough [DecodeFail]"
                    )
        else:
            text_data = input_data

        self._analyze_best_strategy(text_data)

        # --- 2. TEMPLATE EXTRACTION ---
        lines = text_data.splitlines(keepends=True)
        num_lines = len(lines)
        unique_limit = num_lines * (0.40 if self.mode_name == "Aggressive" else 0.25)

        for line in lines:
            if not line:
                continue

            result = self._mask_line(line)
            if result is None:
                # Collision detected -> Safe Fallback
                return self._create_passthrough(input_data, "Collision Protected")

            skeleton, vars_found = result

            if skeleton in self.template_map:
                t_id = self.template_map[skeleton]
            else:
                if self.next_template_id > unique_limit:
                    return self._create_passthrough(text_data, "Passthrough [Entropy]")

                t_id = self.next_template_id
                self.template_map[skeleton] = t_id
                self.skeletons_list.append(skeleton)
                self.columns_storage[t_id] = [[] for _ in range(len(vars_found))]
                self.next_template_id += 1

            self.stream_template_ids.append(t_id)
            current_columns = self.columns_storage[t_id]
            limit = min(len(vars_found), len(current_columns))
            for i in range(limit):
                current_columns[i].append(vars_found[i])

        # --- 3. HEURISTIC & OPTIMIZATION ---
        num_templates = len(self.skeletons_list)
        decision_mode = "UNIFIED"

        if num_templates < 256:
            sample_buffer = bytearray()
            count = 0
            for t_id in range(min(len(self.skeletons_list), 5)):
                for val_list in self.columns_storage[t_id]:
                    for v in val_list[:50]:
                        sample_buffer.extend(v.encode("utf-8"))
                        count += 1
                    if count > 2000:
                        break

            if len(sample_buffer) > 0:
                c_sample = zlib.compress(sample_buffer, level=1)
                if len(c_sample) > 0:
                    ratio = len(sample_buffer) / len(c_sample)
                    if ratio < 3.0:
                        decision_mode = "SPLIT"

        if decision_mode == "UNIFIED":
            id_counts = Counter(self.stream_template_ids)
            sorted_ids = [id_val for id_val, count in id_counts.most_common()]
            remap_table = {old_id: new_id for new_id, old_id in enumerate(sorted_ids)}

            new_skeletons_list = [None] * len(self.skeletons_list)
            for old_id, new_id in remap_table.items():
                new_skeletons_list[new_id] = self.skeletons_list[old_id]

            new_columns_storage = {}
            for old_id, new_id in remap_table.items():
                new_columns_storage[new_id] = self.columns_storage[old_id]

            new_stream_template_ids = [
                remap_table[tid] for tid in self.stream_template_ids
            ]

            self.skeletons_list = new_skeletons_list
            self.columns_storage = new_columns_storage
            self.stream_template_ids = new_stream_template_ids

        # --- 4. SERIALIZATION (ALWAYS ESCAPED) ---
        # Usiamo il separatore sicuro per il registro
        raw_registry = self.REG_SEP.join(self.skeletons_list).encode("utf-8")

        if num_templates == 1:
            raw_ids = b""
            id_mode_flag = 3
        elif num_templates < 256:
            raw_ids = struct.pack(
                f"<{len(self.stream_template_ids)}B", *self.stream_template_ids
            )
            id_mode_flag = 2
        elif num_templates > 65535:
            raw_ids = struct.pack(
                f"<{len(self.stream_template_ids)}I", *self.stream_template_ids
            )
            id_mode_flag = 1
        else:
            raw_ids = struct.pack(
                f"<{len(self.stream_template_ids)}H", *self.stream_template_ids
            )
            id_mode_flag = 0

        # Inject Latin-1 Flag (Bit 0x80)
        if is_latin1:
            id_mode_flag |= 0x80

        # Constants for Byte Stuffing
        B_ESC = b"\x01"
        B_ROW = b"\x00"
        B_COL = b"\x02"

        SEQ_ESC = b"\x01\x01"
        SEQ_ROW = b"\x01\x00"
        SEQ_COL = b"\x01\x03"

        # FIX: Always use Escaped separators (Safe Mode)
        ROW_SEP = b"\x00"
        COL_SEP = b"\x02"

        vars_buffer = bytearray()

        for t_id in range(len(self.skeletons_list)):
            columns = self.columns_storage[t_id]
            for values_list in columns:
                encoded_values = []
                for v in values_list:
                    v_bytes = v.encode("utf-8")

                    # Always apply escaping
                    v_bytes = v_bytes.replace(B_ESC, SEQ_ESC)
                    v_bytes = v_bytes.replace(B_ROW, SEQ_ROW)
                    v_bytes = v_bytes.replace(B_COL, SEQ_COL)

                    encoded_values.append(v_bytes)

                col_blob = ROW_SEP.join(encoded_values)
                vars_buffer.extend(col_blob)
                vars_buffer.extend(COL_SEP)

        # --- 5. COMPRESSION ---
        if decision_mode == "SPLIT":
            c_reg = lzma.compress(raw_registry, preset=9)
            c_ids = lzma.compress(raw_ids, preset=9)
            c_vars = lzma.compress(vars_buffer, preset=9 | lzma.PRESET_EXTREME)
            return c_reg, c_ids, c_vars, id_mode_flag, self.mode_name
        else:
            len_reg = len(raw_registry)
            len_ids = len(raw_ids)
            internal_header = struct.pack("<II", len_reg, len_ids)
            solid_block = internal_header + raw_registry + raw_ids + vars_buffer

            filters_unified = [
                {
                    "id": lzma.FILTER_LZMA2,
                    "preset": 9 | lzma.PRESET_EXTREME,
                    "dict_size": 128 * 1024 * 1024,
                }
            ]
            c_solid = lzma.compress(
                solid_block, check=lzma.CHECK_CRC32, filters=filters_unified
            )
            return b"", b"", c_solid, id_mode_flag, self.mode_name

    def _create_passthrough(
        self, data: Union[bytes, str], reason: str = "Passthrough"
    ) -> Tuple[bytes, bytes, bytes, int, str]:
        if isinstance(data, str):
            data_bytes = data.encode("utf-8")
        else:
            data_bytes = data

        c_vars = lzma.compress(data_bytes, preset=9 | lzma.PRESET_EXTREME)
        return b"", b"", c_vars, 255, reason


class CASTDecompressor:
    """
    Handles the decompression of CAST-encoded data streams.
    Safe & Lossless (Always Escaped).
    """

    # Constants for reconstruction
    VAR_PLACEHOLDER = "\uE000"
    REG_SEP = "\uE001"

    def decompress(
        self,
        c_registry: bytes,
        c_ids: bytes,
        c_vars: bytes,
        expected_crc: Optional[int] = None,
        id_mode_flag: int = 0,
    ) -> bytes:
        if id_mode_flag == 255:
            data = lzma.decompress(c_vars)
            if expected_crc is not None and zlib.crc32(data) != expected_crc:
                print("CRC ERROR (Passthrough)!")
            return data

        # --- 1. FLAG PARSING ---
        is_latin1 = (id_mode_flag & 0x80) != 0
        real_id_flag = id_mode_flag & 0x7F

        is_unified = len(c_registry) == 0 and len(c_ids) == 0

        # Constants (Always Escaped Mode)
        SEQ_ESC = b"\x01\x01"
        SEQ_ROW = b"\x01\x00"
        SEQ_COL = b"\x01\x03"

        B_ESC = b"\x01"
        B_ROW = b"\x00"
        B_COL = b"\x02"

        COL_SEP_BYTE = 0x02
        ROW_SEP_BYTE = 0x00

        # --- 2. DECOMPRESSION & PARSING ---
        if is_unified:
            full_payload = lzma.decompress(c_vars)
            len_reg, len_ids = struct.unpack("<II", full_payload[:8])
            offset = 8
            reg_data_bytes = full_payload[offset : offset + len_reg]
            offset += len_reg

            if real_id_flag == 3:
                ids_data_bytes = b""
            else:
                ids_data_bytes = full_payload[offset : offset + len_ids]
                offset += len_ids
            vars_data_bytes = full_payload[offset:]

            reg_data = reg_data_bytes.decode("utf-8")
            skeletons = reg_data.split(self.REG_SEP)

            if real_id_flag == 3:
                template_ids = []
            else:
                template_ids = self._unpack_ids(ids_data_bytes, real_id_flag)
        else:
            reg_payload = lzma.decompress(c_registry)
            reg_data = reg_payload.decode("utf-8")
            skeletons = reg_data.split(self.REG_SEP)

            ids_data = lzma.decompress(c_ids)
            template_ids = self._unpack_ids(ids_data, real_id_flag)

            vars_data_bytes = lzma.decompress(c_vars)

        # --- 3. COLUMN PARSING (MANUAL BYTE SCAN) ---
        # Python's split() is not enough for byte stuffing logic, need careful scan

        # 3.1 Find Columns
        raw_columns_offsets = []
        start = 0
        i = 0
        max_len = len(vars_data_bytes)

        while i < max_len:
            val = vars_data_bytes[i]
            if val == 0x01:  # Escape
                i += 2
            elif val == COL_SEP_BYTE:
                raw_columns_offsets.append((start, i))
                i += 1
                start = i
            else:
                i += 1

        if start < max_len:
            raw_columns_offsets.append((start, max_len))

        columns_storage = {}
        col_iter = iter(raw_columns_offsets)

        skeleton_parts_cache = []
        for s in skeletons:
            parts = [p.encode("utf-8") for p in s.split(self.VAR_PLACEHOLDER)]
            skeleton_parts_cache.append(parts)

        # 3.2 Extract Rows
        for t_id, skel in enumerate(skeletons):
            # Conta occorrenze del placeholder
            num_vars = skel.count(self.VAR_PLACEHOLDER)
            columns_storage[t_id] = []

            for _ in range(num_vars):
                try:
                    col_start, col_end = next(col_iter)
                    decoded_vals = []

                    curr = col_start
                    cell_start = curr

                    while curr < col_end:
                        val = vars_data_bytes[curr]
                        if val == 0x01:  # Escape
                            curr += 2
                        elif val == ROW_SEP_BYTE:
                            # Extract & Unescape
                            chunk = vars_data_bytes[cell_start:curr]
                            chunk = chunk.replace(SEQ_COL, B_COL)
                            chunk = chunk.replace(SEQ_ROW, B_ROW)
                            chunk = chunk.replace(SEQ_ESC, B_ESC)
                            decoded_vals.append(chunk)

                            curr += 1
                            cell_start = curr
                        else:
                            curr += 1

                    # Last cell
                    chunk = vars_data_bytes[cell_start:curr]
                    chunk = chunk.replace(SEQ_COL, B_COL)
                    chunk = chunk.replace(SEQ_ROW, B_ROW)
                    chunk = chunk.replace(SEQ_ESC, B_ESC)
                    decoded_vals.append(chunk)

                    columns_storage[t_id].append(iter(decoded_vals))

                except StopIteration:
                    break

        # --- 4. STREAM RECONSTRUCTION ---
        reconstructed_fragments = []
        buf_append = reconstructed_fragments.append

        if real_id_flag == 3:
            parts = skeleton_parts_cache[0]
            queues = columns_storage[0]
            # Assumiamo che tutte le colonne abbiano la stessa lunghezza
            if queues:
                for vars_tuple in zip(*queues):
                    row_components = [b""] * (len(parts) + len(vars_tuple))
                    row_components[::2] = parts
                    row_components[1::2] = vars_tuple
                    buf_append(b"".join(row_components))
            else:
                # No vars case (solo testo statico ripetuto)
                # Questo caso è raro in CAST ma possibile (log di sole righe fisse)
                pass  # TODO: Gestire righe fisse senza variabili se necessario

        else:
            for t_id in template_ids:
                parts = skeleton_parts_cache[t_id]
                queues = columns_storage[t_id]
                try:
                    current_vars = [next(q) for q in queues]
                    row_components = [b""] * (len(parts) + len(current_vars))
                    row_components[::2] = parts
                    row_components[1::2] = current_vars
                    buf_append(b"".join(row_components))
                except StopIteration:
                    break

        final_blob = b"".join(reconstructed_fragments)

        # --- 5. LATIN-1 RESTORATION ---
        if is_latin1:
            try:
                temp_str = final_blob.decode("utf-8")
                final_blob = temp_str.encode("latin-1")
            except Exception as e:
                print(f"[!] Warning: Latin-1 restoration failed: {e}")

        # --- 6. CRC CHECK ---
        if expected_crc is not None:
            calculated_crc = zlib.crc32(final_blob)
            if calculated_crc != expected_crc:
                raise ValueError(
                    f"CRC ERROR! Expected: {expected_crc}, Calculated: {calculated_crc}"
                )

        return final_blob

    def _unpack_ids(self, ids_bytes: bytes, mode: int) -> Tuple[int, ...]:
        num_bytes = len(ids_bytes)
        if num_bytes == 0:
            return ()
        if mode == 2:
            return struct.unpack(f"<{num_bytes}B", ids_bytes)
        elif mode == 1:
            return struct.unpack(f"<{num_bytes // 4}I", ids_bytes)
        else:
            return struct.unpack(f"<{num_bytes // 2}H", ids_bytes)
