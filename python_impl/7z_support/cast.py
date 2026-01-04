import lzma
import re
import struct
import zlib
import os
import shutil
import subprocess
from collections import Counter
from typing import List, Tuple, Union, Optional


class CASTCompressor:
    """
    Handles the compression of structured data using the CAST (Columnar Agnostic Structural Transformation) algorithm.
    Includes Latin-1 fallback for binary-dirty CSVs.

    SYSTEM MODE SUPPORT:
    If '7z' is found (via SEVEN_ZIP_PATH or PATH), it is used as the backend compressor
    instead of Python's native lzma module. This significantly improves performance.
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

        # 7-Zip Backend Detection
        self.seven_zip_path = self._find_7z()
        if self.seven_zip_path:
            self.mode_name += " (System 7z)"

    def _find_7z(self) -> Optional[str]:
        # Priority 1: Environment Variable
        env_path = os.environ.get("SEVEN_ZIP_PATH")
        if env_path and os.path.exists(env_path):
            return env_path

        # Priority 2: System PATH
        sys_path = shutil.which("7z") or shutil.which("7za")
        return sys_path

    def _7z_compress(self, data: bytes) -> bytes:
        """Pipes data to external 7z executable for compression."""
        try:
            # Arguments ported from Rust implementation
            cmd = [
                self.seven_zip_path,
                "a",
                "-txz",
                "-mx=9",
                "-mmt=on",
                "-m0=lzma2:d128m",
                "-y",
                "-bb0",
                "-si",
                "-so",
                "-an"
            ]

            # Suppress stderr to keep CLI clean
            process = subprocess.Popen(
                cmd,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.DEVNULL
            )
            compressed_data, _ = process.communicate(input=data)

            if process.returncode != 0 or not compressed_data:
                raise Exception(f"7z returned code {process.returncode}")

            return compressed_data
        except Exception as e:
            print(f"[!] 7z Backend failed ({e}). Falling back to native LZMA.")
            # Fallback to native if 7z crashes
            return lzma.compress(data, preset=9 | lzma.PRESET_EXTREME)

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
            base_mode = "Aggressive"
        else:
            self.active_pattern = self.regex_strict
            base_mode = "Strict"

        # Update mode name preserving 7z tag
        if "7z" in self.mode_name:
            self.mode_name = f"{base_mode} (System 7z)"
        else:
            self.mode_name = base_mode

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

        # --- 1. DECODING & LATIN-1 CHECK ---
        is_latin1 = False

        if isinstance(input_data, bytes):
            if self._is_likely_binary(input_data):
                return self._create_passthrough(input_data, "Passthrough [Binary]")
            try:
                text_data = input_data.decode("utf-8")
            except UnicodeDecodeError:
                try:
                    # Fallback critico per file come BX_Books.csv
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
        unique_limit = num_lines * (0.40 if "Aggressive" in self.mode_name else 0.25)

        for line in lines:
            if not line:
                continue
            skeleton, vars_found = self._mask_line(line)

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

        # If 7z is available, we force UNIFIED mode because 7z is a stream compressor
        # and handles mixed data better than splitting into small chunks for lzma python.
        if not self.seven_zip_path:
            # Only use Python splitting logic if we don't have 7z
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

        # Remap IDs for Unified
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

        # --- 4. SERIALIZATION ---
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

        # Inject Latin-1 Flag (Bit 0x80)
        if is_latin1:
            id_mode_flag |= 0x80

        # --- SEPARATORS ---
        if decision_mode == "SPLIT":
            ROW_SEP = b"\x00"
            COL_SEP = b"\xff\xff"
            ESCAPE_NEEDED = False
        else:
            ROW_SEP = b"\x00"
            COL_SEP = b"\x02"  # Safe unified separator
            ESCAPE_NEEDED = True

            B_ESC = b"\x01"
            B_ROW = b"\x00"
            B_COL = b"\x02"

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
                        # Apply escaping
                        v_bytes = v_bytes.replace(B_ESC, SEQ_ESC)
                        v_bytes = v_bytes.replace(B_ROW, SEQ_ROW)
                        v_bytes = v_bytes.replace(B_COL, SEQ_COL)

                    encoded_values.append(v_bytes)

                col_blob = ROW_SEP.join(encoded_values)
                vars_buffer.extend(col_blob)
                vars_buffer.extend(COL_SEP)

        # --- 5. COMPRESSION ---
        if decision_mode == "SPLIT":
            # This path is usually only taken if 7z is NOT present
            c_reg = lzma.compress(raw_registry, preset=9)
            c_ids = lzma.compress(raw_ids, preset=9)
            c_vars = lzma.compress(vars_buffer, preset=9 | lzma.PRESET_EXTREME)
            return c_reg, c_ids, c_vars, id_mode_flag, self.mode_name
        else:
            # Unified Mode (Default for 7z)
            len_reg = len(raw_registry)
            len_ids = len(raw_ids)
            internal_header = struct.pack("<II", len_reg, len_ids)
            solid_block = internal_header + raw_registry + raw_ids + vars_buffer

            if self.seven_zip_path:
                # SYSTEM MODE: Use 7-Zip
                c_solid = self._7z_compress(solid_block)
            else:
                # NATIVE MODE: Use LZMA module
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

        if self.seven_zip_path:
            c_vars = self._7z_compress(data_bytes)
        else:
            c_vars = lzma.compress(data_bytes, preset=9 | lzma.PRESET_EXTREME)

        return b"", b"", c_vars, 255, reason


class CASTDecompressor:
    """
    Handles the decompression of CAST-encoded data streams.
    Fully compatible with Rust Latin-1 Flag fix.
    Supports 7-Zip backend for decompression if available.
    """

    def __init__(self):
        # We need to find 7z for decompression too
        self.seven_zip_path = self._find_7z()

    def _find_7z(self) -> Optional[str]:
        env_path = os.environ.get("SEVEN_ZIP_PATH")
        if env_path and os.path.exists(env_path): return env_path
        return shutil.which("7z") or shutil.which("7za")

    def _7z_decompress(self, data: bytes) -> bytes:
        try:
            # 7z e -txz -si -so
            cmd = [self.seven_zip_path, "e", "-txz", "-si", "-so"]
            process = subprocess.Popen(
                cmd,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.DEVNULL
            )
            out, _ = process.communicate(input=data)
            if process.returncode != 0: raise Exception("Non-zero exit")
            return out
        except:
            # Silent fallback to native
            return lzma.decompress(data)

    def _decompress_payload(self, data: bytes) -> bytes:
        """Decides whether to use 7z or native lzma."""
        if self.seven_zip_path:
            return self._7z_decompress(data)
        return lzma.decompress(data)

    def decompress(
            self,
            c_registry: bytes,
            c_ids: bytes,
            c_vars: bytes,
            expected_crc: Optional[int] = None,
            id_mode_flag: int = 0,
    ) -> bytes:
        if id_mode_flag == 255:
            data = self._decompress_payload(c_vars)
            if expected_crc is not None and zlib.crc32(data) != expected_crc:
                print("CRC ERROR (Passthrough)!")
            return data

        # --- 1. FLAG PARSING ---
        is_latin1 = (id_mode_flag & 0x80) != 0
        real_id_flag = id_mode_flag & 0x7F

        is_unified = len(c_registry) == 0 and len(c_ids) == 0
        REG_SEP = "\x1e"

        if is_unified:
            ROW_SEP = b"\x00"
            COL_SEP = b"\x02"
            ESCAPE_NEEDED = True

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

        # --- 2. DECOMPRESSION & PARSING ---
        if is_unified:
            # Here we use the backend-aware decompression
            full_payload = self._decompress_payload(c_vars)

            len_reg, len_ids = struct.unpack("<II", full_payload[:8])
            offset = 8
            reg_data_bytes = full_payload[offset: offset + len_reg]
            offset += len_reg

            if real_id_flag == 3:
                ids_data_bytes = b""
            else:
                ids_data_bytes = full_payload[offset: offset + len_ids]
                offset += len_ids
            vars_data_bytes = full_payload[offset:]

            reg_data = reg_data_bytes.decode("utf-8")
            skeletons = reg_data.split(REG_SEP)

            if real_id_flag == 3:
                template_ids = []
            else:
                template_ids = self._unpack_ids(ids_data_bytes, real_id_flag)

            raw_columns = vars_data_bytes.split(COL_SEP)
        else:
            # Legacy Split Mode (Native LZMA only usually, but let's be safe)
            reg_payload = lzma.decompress(c_registry)
            reg_data = reg_payload.decode("utf-8")
            skeletons = reg_data.split(REG_SEP)

            ids_data = lzma.decompress(c_ids)
            template_ids = self._unpack_ids(ids_data, real_id_flag)

            vars_data = self._decompress_payload(c_vars)
            raw_columns = vars_data.split(COL_SEP)

        if raw_columns and not raw_columns[-1]:
            raw_columns.pop()

        columns_storage = {}
        col_idx_counter = 0

        skeleton_parts_cache = []
        for s in skeletons:
            parts = [p.encode("utf-8") for p in s.split("\x00")]
            skeleton_parts_cache.append(parts)

        # --- 3. COLUMN RECONSTRUCTION ---
        for t_id, skel in enumerate(skeletons):
            num_vars = skel.count("\x00")
            columns_storage[t_id] = []

            for _ in range(num_vars):
                if col_idx_counter < len(raw_columns):
                    col_bytes = raw_columns[col_idx_counter]
                    raw_vals = col_bytes.split(ROW_SEP)

                    if ESCAPE_NEEDED:
                        decoded_vals = []
                        for v in raw_vals:
                            v = v.replace(SEQ_COL, B_COL)
                            v = v.replace(SEQ_ROW, B_ROW)
                            v = v.replace(SEQ_ESC, B_ESC)
                            decoded_vals.append(v)
                        columns_storage[t_id].append(iter(decoded_vals))
                    else:
                        columns_storage[t_id].append(iter(raw_vals))

                    col_idx_counter += 1

        # --- 4. STREAM RECONSTRUCTION ---
        reconstructed_fragments = []
        buf_append = reconstructed_fragments.append

        if real_id_flag == 3:
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

