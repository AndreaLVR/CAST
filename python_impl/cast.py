import lzma
import re
import struct
import zlib
from collections import Counter
from typing import List, Tuple, Union, Optional


class CASTCompressor:
    """
    Handles the compression of structured data using the CAST (Columnar Agnostic Structural Transformation) algorithm.
    """

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
            skel = self.regex_strict.sub(lambda m: "\x00", line)
            strict_templates.add(skel)

        ratio = len(strict_templates) / len(sample_lines)
        if ratio > 0.10:
            self.active_pattern = self.regex_aggressive
            self.mode_name = "Aggressive"
        else:
            self.active_pattern = self.regex_strict
            self.mode_name = "Strict"

    def _mask_line(self, line: str) -> Tuple[str, List[str]]:
        variables = []

        def replace_callback(match):
            token = match.group(0)
            if token.startswith('"'):
                variables.append(token[1:-1])
                return '"\x00"'
            else:
                variables.append(token)
                return "\x00"

        masked_line = self.active_pattern.sub(replace_callback, line)
        return masked_line, variables

    def compress(
            self, input_data: Union[bytes, str]
    ) -> Tuple[bytes, bytes, bytes, int, str]:
        if isinstance(input_data, bytes):
            if self._is_likely_binary(input_data):
                print(" [Mode: Binary Passthrough] ", end="")
                return self._create_passthrough(input_data, "Passthrough [Binary]")
            try:
                text_data = input_data.decode("utf-8")
            except UnicodeDecodeError:
                try:
                    text_data = input_data.decode("latin-1")
                except:
                    return self._create_passthrough(
                        input_data, "Passthrough [DecodeFail]"
                    )
        else:
            text_data = input_data

        self._analyze_best_strategy(text_data)
        print(f"[Strategy: {self.mode_name}] ", end="", flush=True)

        lines = text_data.splitlines(keepends=True)
        num_lines = len(lines)
        unique_limit = num_lines * (0.40 if self.mode_name == "Aggressive" else 0.25)

        for line in lines:
            if not line:
                continue
            skeleton, vars_found = self._mask_line(line)

            if skeleton in self.template_map:
                t_id = self.template_map[skeleton]
            else:
                if self.next_template_id > unique_limit:
                    print(" [Fallback: Entropy Limit] ", end="")
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

        REG_SEP = "\x1e"
        raw_registry = REG_SEP.join(self.skeletons_list).encode("utf-8")

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

        # --- SEPARATORS & ESCAPING (COMPATIBILITY FIX) ---
        # Updated to match Rust "Safe Separators Logic"
        if decision_mode == "SPLIT":
            ROW_SEP = b"\x00"
            COL_SEP = b"\xff\xff"
            ESCAPE_NEEDED = False
        else:
            ROW_SEP = b"\x00"
            # FIX 1: Use 0x02 as Column Separator
            COL_SEP = b"\x02"
            ESCAPE_NEEDED = True

            # Helper bytes for escaping
            B_ESC = b"\x01"
            B_ROW = b"\x00"
            B_COL = b"\x02"  # The new col separator

            # Escape Sequences:
            # 0x01 -> 0x01 0x01
            # 0x00 -> 0x01 0x00
            # 0x02 -> 0x01 0x03
            SEQ_ESC = b"\x01\x01"
            SEQ_ROW = b"\x01\x00"
            SEQ_COL = b"\x01\x03"

        vars_buffer = bytearray()
        for t_id in range(len(self.skeletons_list)):
            columns = self.columns_storage[t_id]
            for values_list in columns:
                encoded_values = []
                for v in values_list:
                    v_bytes = v.encode("utf-8")
                    if ESCAPE_NEEDED:
                        # FIX 2: Apply escaping in correct order
                        # 1. Escape the escape char itself
                        v_bytes = v_bytes.replace(B_ESC, SEQ_ESC)
                        # 2. Escape Row Separator
                        v_bytes = v_bytes.replace(B_ROW, SEQ_ROW)
                        # 3. Escape Column Separator (0x02)
                        v_bytes = v_bytes.replace(B_COL, SEQ_COL)

                    encoded_values.append(v_bytes)

                col_blob = ROW_SEP.join(encoded_values)
                vars_buffer.extend(col_blob)
                vars_buffer.extend(COL_SEP)

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
    """

    def decompress(
            self,
            c_registry: bytes,
            c_ids: bytes,
            c_vars: bytes,
            expected_crc: Optional[int] = None,
            id_mode_flag: int = 0,
    ) -> bytes:
        if id_mode_flag == 255:
            return lzma.decompress(c_vars)

        is_unified = len(c_registry) == 0 and len(c_ids) == 0
        REG_SEP = "\x1e"

        # --- CONFIGURAZIONE SEPARATORI (MATCH RUST) ---
        if is_unified:
            ROW_SEP = b"\x00"
            # FIX 3: Use 0x02 as Column Separator
            COL_SEP = b"\x02"
            ESCAPE_NEEDED = True

            # Inverse Replacements
            # 0x01 0x03 -> 0x02
            # 0x01 0x00 -> 0x00
            # 0x01 0x01 -> 0x01
            SEQ_ESC = b"\x01\x01"
            SEQ_ROW = b"\x01\x00"
            SEQ_COL = b"\x01\x03"

            B_ESC = b"\x01"
            B_ROW = b"\x00"
            B_COL = b"\x02"
        else:
            ROW_SEP = b"\x00"
            COL_SEP = b"\xff\xff"
            ESCAPE_NEEDED = False

        if is_unified:
            full_payload = lzma.decompress(c_vars)
            len_reg, len_ids = struct.unpack("<II", full_payload[:8])
            offset = 8
            reg_data_bytes = full_payload[offset: offset + len_reg]
            offset += len_reg

            if id_mode_flag == 3:
                ids_data_bytes = b""
            else:
                ids_data_bytes = full_payload[offset: offset + len_ids]
                offset += len_ids
            vars_data_bytes = full_payload[offset:]

            reg_data = reg_data_bytes.decode("utf-8")
            skeletons = reg_data.split(REG_SEP)

            if id_mode_flag == 3:
                template_ids = []
            else:
                template_ids = self._unpack_ids(ids_data_bytes, id_mode_flag)

            raw_columns = vars_data_bytes.split(COL_SEP)
        else:
            reg_payload = lzma.decompress(c_registry)
            reg_data = reg_payload.decode("utf-8")
            skeletons = reg_data.split(REG_SEP)

            ids_data = lzma.decompress(c_ids)
            template_ids = self._unpack_ids(ids_data, id_mode_flag)

            vars_data = lzma.decompress(c_vars)
            raw_columns = vars_data.split(COL_SEP)

        if not raw_columns[-1]:
            raw_columns.pop()

        columns_storage = {}
        col_idx_counter = 0

        skeleton_parts_cache = []
        for s in skeletons:
            parts = [p.encode("utf-8") for p in s.split("\x00")]
            skeleton_parts_cache.append(parts)

        for t_id, skel in enumerate(skeletons):
            num_vars = skel.count("\x00")
            columns_storage[t_id] = []

            for _ in range(num_vars):
                if col_idx_counter < len(raw_columns):
                    col_bytes = raw_columns[col_idx_counter]
                    raw_vals = col_bytes.split(ROW_SEP)

                    if ESCAPE_NEEDED:
                        # FIX 4: Unescape logic (Byte replacement is efficient in Python)
                        # Order matters less for decompression if sequences are unique,
                        # but it's good practice to map sequences back to bytes.
                        decoded_vals = []
                        for v in raw_vals:
                            # 1. Restore Col Sep (0x01 0x03 -> 0x02)
                            v = v.replace(SEQ_COL, B_COL)
                            # 2. Restore Row Sep (0x01 0x00 -> 0x00)
                            v = v.replace(SEQ_ROW, B_ROW)
                            # 3. Restore Escape (0x01 0x01 -> 0x01)
                            v = v.replace(SEQ_ESC, B_ESC)
                            decoded_vals.append(v)

                        columns_storage[t_id].append(iter(decoded_vals))
                    else:
                        columns_storage[t_id].append(iter(raw_vals))

                    col_idx_counter += 1

        reconstructed_fragments = []
        buf_append = reconstructed_fragments.append

        if id_mode_flag == 3:
            parts = skeleton_parts_cache[0]
            queues = columns_storage[0]
            for vars_tuple in zip(*queues):
                row_components = [b""] * (len(parts) + len(vars_tuple))
                row_components[::2] = parts
                row_components[1::2] = vars_tuple
                buf_append(b"".join(row_components))

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