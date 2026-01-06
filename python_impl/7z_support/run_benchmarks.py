import argparse
import lzma
import os
import struct
import time
import zlib
from typing import List

# Optional dependencies
try:
    import zstandard as zstd
except ImportError:
    zstd = None

try:
    import brotli
except ImportError:
    brotli = None

# Import CAST classes
try:
    from cast import CASTCompressor, CASTDecompressor
except ImportError:
    print("[ERROR] File 'cast.py' not found in the current directory.")
    exit(1)


def format_bytes(n):
    return f"{n:,}"


def parse_human_size(size_str):
    """Parses a human readable size string (e.g. '100MB', '1GB') into bytes."""
    if not size_str:
        return None
    s = size_str.strip().upper()
    try:
        if s.endswith("GB"):
            return int(float(s[:-2]) * 1024 ** 3)
        elif s.endswith("MB"):
            return int(float(s[:-2]) * 1024 ** 2)
        elif s.endswith("KB"):
            return int(float(s[:-2]) * 1024)
        elif s.endswith("B"):
            return int(s[:-1])
        else:
            return int(s)
    except ValueError:
        return None


def load_file_list(list_path: str) -> List[str]:
    """Loads file list, ignoring comments."""
    paths = []
    if not os.path.exists(list_path):
        print(f"[ERROR] List file not found: {list_path}")
        return paths

    with open(list_path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line and not line.startswith("#"):
                line = line.strip('"').strip("'")
                paths.append(line)
    return paths


def main() -> None:
    parser = argparse.ArgumentParser(
        description="CAST Compression Benchmark Tool (Reference)"
    )

    # Input: List or Single file
    input_group = parser.add_mutually_exclusive_group(required=True)
    input_group.add_argument(
        "--list", type=str, help="Path to text file containing list of files."
    )
    input_group.add_argument("--file", type=str, help="Path to a single file to test.")

    # CAST Settings
    parser.add_argument(
        "--chunk-size",
        type=str,
        help="Apply chunking ONLY to CAST (e.g. '100MB'). Competitors remain solid.",
    )

    # CHANGED: Added dict-size argument
    parser.add_argument(
        "--dict-size",
        type=str,
        help="Set LZMA Dictionary Size (e.g. '128MB', '256MB'). Default: 128MB.",
    )

    # Competitors
    parser.add_argument("--lzma", action="store_true", help="Enable LZMA2 (XZ).")
    parser.add_argument("--brotli", action="store_true", help="Enable Brotli.")
    parser.add_argument("--zstd", action="store_true", help="Enable Zstandard.")
    parser.add_argument("--all", action="store_true", help="Enable ALL competitors.")

    args = parser.parse_args()

    RUN_LZMA = args.all or args.lzma
    RUN_BROTLI = args.all or args.brotli
    RUN_ZSTD = args.all or args.zstd

    # Parse settings
    CHUNK_SIZE = parse_human_size(args.chunk_size)
    DICT_SIZE = parse_human_size(args.dict_size)  # Can be None

    # Checks
    if RUN_BROTLI and not brotli:
        print("NOTE: 'brotli' module missing. Skipping.")
        RUN_BROTLI = False
    if RUN_ZSTD and not zstd:
        print("NOTE: 'zstandard' module missing. Skipping.")
        RUN_ZSTD = False

    files_to_test = []
    if args.list:
        files_to_test = load_file_list(args.list)
    elif args.file:
        files_to_test = [args.file]

    if not files_to_test:
        print("[!] No files to test.")
        return

    print(f"\nSTARTING CAST REFERENCE BENCHMARK SUITE")
    print(
        f"Competitors: LZMA={'ON' if RUN_LZMA else 'OFF'}, BROTLI={'ON' if RUN_BROTLI else 'OFF'}, ZSTD={'ON' if RUN_ZSTD else 'OFF'}"
    )

    # Config Info
    dict_info = format_bytes(DICT_SIZE) if DICT_SIZE else "Default (128MB)"
    if CHUNK_SIZE:
        print(
            f"CAST Config: CHUNKED ({format_bytes(CHUNK_SIZE)}) | Dict: {dict_info}"
        )
    else:
        print(f"CAST Config: SOLID (Single Block) | Dict: {dict_info}")
    print("=" * 75)

    for file_path in files_to_test:
        file_path = os.path.abspath(file_path)

        print(f"\n{'=' * 75}")
        print(f"FILE: {os.path.basename(file_path)}")
        print(f"PATH: {file_path}")
        print(f"{'-' * 75}")

        if not os.path.exists(file_path):
            print(f"[!] File not found: {file_path}")
            continue

        try:
            with open(file_path, "rb") as f:
                original_data = f.read()
        except Exception as e:
            print(f"[!] Read error: {e}")
            continue

        orig_len = len(original_data)
        if orig_len == 0:
            continue

        original_crc = zlib.crc32(original_data)
        print(f"Original: {format_bytes(orig_len)} bytes | CRC32: {original_crc:08X}")
        print("-" * 75)

        results = {}
        times = {}
        # Used to store the strategy name from CAST
        cast_strategy_used = "Unknown"

        # --- 1. LZMA (Always Solid) ---
        if RUN_LZMA:
            print("[1] LZMA (Extreme)... ", end="", flush=True)
            start = time.time()
            try:
                # CHANGED: Apply dict_size parity to competitor!
                if DICT_SIZE is not None:
                    # We must use filters to enforce dict_size
                    lzma_filters = [{
                        "id": lzma.FILTER_LZMA2,
                        "preset": 9 | lzma.PRESET_EXTREME,
                        "dict_size": DICT_SIZE
                    }]
                    c_data = lzma.compress(
                        original_data,
                        format=lzma.FORMAT_XZ,
                        check=lzma.CHECK_CRC32,
                        filters=lzma_filters
                    )
                else:
                    # Legacy/Default behavior
                    c_data = lzma.compress(
                        original_data, format=lzma.FORMAT_XZ, preset=9 | lzma.PRESET_EXTREME
                    )

                times["LZMA"] = time.time() - start
                results["LZMA"] = len(c_data)
                print(f"Done ({times['LZMA']:.2f}s)")
                del c_data
            except Exception as e:
                print(f"ERR: {e}")

        # --- 2. ZSTD (Always Solid) ---
        if RUN_ZSTD:
            print("[2] Zstd (Level 22)...  ", end="", flush=True)
            start = time.time()
            try:
                cctx = zstd.ZstdCompressor(level=22)
                c_data = cctx.compress(original_data)
                times["Zstd"] = time.time() - start
                results["Zstd"] = len(c_data)
                print(f"Done ({times['Zstd']:.2f}s)")
                del c_data
            except Exception as e:
                print(f"ERR: {e}")

        # --- 3. BROTLI (Always Solid) ---
        if RUN_BROTLI:
            print("[3] Brotli (Q 11)...    ", end="", flush=True)
            start = time.time()
            try:
                c_data = brotli.compress(
                    original_data, mode=brotli.MODE_GENERIC, quality=11
                )
                times["Brotli"] = time.time() - start
                results["Brotli"] = len(c_data)
                print(f"Done ({times['Brotli']:.2f}s)")
                del c_data
            except Exception as e:
                print(f"ERR: {e}")

        # --- 4. CAST (Solid or Chunked) ---
        mode_label = f"CAST ({'Chunked' if CHUNK_SIZE else 'Solid'})"
        print(f"[4] {mode_label:<15} ", end="", flush=True)
        start = time.time()
        try:
            full_blob = bytearray()

            # Set effective dict size for calls (Default 128MB if None)
            call_dict_size = DICT_SIZE if DICT_SIZE else 128 * 1024 * 1024

            if CHUNK_SIZE:
                # CHUNKED PROCESSING
                offset = 0
                while offset < len(original_data):
                    chunk = original_data[offset: offset + CHUNK_SIZE]
                    offset += CHUNK_SIZE

                    chunk_crc = zlib.crc32(chunk)
                    compressor = CASTCompressor()

                    # CHANGED: Pass dict_size
                    res = compressor.compress(chunk, dict_size=call_dict_size)

                    # Handle 5-element return tuple
                    if isinstance(res, tuple):
                        if len(res) >= 5:
                            c_reg, c_ids, c_vars, id_flag, s_name = res[:5]
                            cast_strategy_used = s_name
                        elif len(res) >= 4:
                            c_reg, c_ids, c_vars, id_flag = res[:4]
                        else:
                            raise ValueError("Invalid output length")
                    else:
                        raise ValueError("Invalid output type")

                    header = struct.pack(
                        "<IIIIB",
                        chunk_crc,
                        len(c_reg),
                        len(c_ids),
                        len(c_vars),
                        id_flag,
                    )
                    full_blob.extend(header)
                    full_blob.extend(c_reg)
                    full_blob.extend(c_ids)
                    full_blob.extend(c_vars)

                    del chunk, c_reg, c_ids, c_vars
            else:
                # SOLID PROCESSING
                compressor = CASTCompressor()
                # CHANGED: Pass dict_size
                res = compressor.compress(original_data, dict_size=call_dict_size)

                if isinstance(res, tuple):
                    if len(res) >= 5:
                        c_reg, c_ids, c_vars, id_flag, s_name = res[:5]
                        cast_strategy_used = s_name
                    elif len(res) >= 4:
                        c_reg, c_ids, c_vars, id_flag = res[:4]
                    else:
                        raise ValueError("Invalid output length")
                else:
                    raise ValueError("Invalid output type")

                header = struct.pack(
                    "<IIIIB", original_crc, len(c_reg), len(c_ids), len(c_vars), id_flag
                )
                full_blob.extend(header)
                full_blob.extend(c_reg)
                full_blob.extend(c_ids)
                full_blob.extend(c_vars)

            times["CAST"] = time.time() - start
            results["CAST"] = len(full_blob)

            # Save for verification
            out_path = file_path + ".cast"
            with open(out_path, "wb") as f:
                f.write(full_blob)

            print(f"Done ({times['CAST']:.2f}s) [{cast_strategy_used}]")

            del full_blob

        except Exception as e:
            print(f"\n[!] CAST Failed: {e}")

        # --- RANKING & DISPLAY ---
        print("-" * 75)
        valid = {k: v for k, v in results.items() if v != float("inf")}

        if not valid:
            print("No results.")
            continue

        sorted_res = sorted(valid.items(), key=lambda x: x[1])
        winner_name, winner_size = sorted_res[0]

        print(
            f"{'RANK':<4} {'ALGORITHM':<8} {'SIZE':>14} {'RATIO':>10} {'TIME':>10} {'NOTES'}"
        )

        for i, (name, size) in enumerate(sorted_res, 1):
            t = times.get(name, 0)
            ratio = orig_len / size if size > 0 else 0

            # Formatting without colors
            size_str = format_bytes(size)
            time_str = f"{t:.2f}s"

            if i == 1:
                note = "(WINNER)"
            else:
                diff = size - winner_size
                note = f"+{format_bytes(diff)} B"

            print(
                f"{i:<4} {name:<8} {size_str:>23} {ratio:>9.2f}x {time_str:>19} {note}"
            )

        # --- CAST VERIFICATION ---
        if "CAST" in results:
            print(f"\n[*] Verifying CAST Integrity...", end="", flush=True)
            time.sleep(0.5)
            try:
                # Simplified verification loop
                verified_ok = True
                bytes_verified = 0

                with open(out_path, "rb") as f_in:
                    chunk_idx = 0
                    while True:
                        head = f_in.read(17)
                        if not head:
                            break
                        if len(head) < 17:
                            verified_ok = False
                            break

                        chunk_idx += 1
                        crc, lr, li, lv, flg = struct.unpack("<IIIIB", head)
                        body = f_in.read(lr + li + lv)

                        if len(body) != (lr + li + lv):
                            verified_ok = False
                            break

                        dec = CASTDecompressor()
                        restored = dec.decompress(
                            body[:lr],
                            body[lr: lr + li],
                            body[lr + li:],
                            expected_crc=crc,
                            id_mode_flag=flg,
                        )

                        chunk_len = len(restored)
                        original_slice = original_data[
                                         bytes_verified: bytes_verified + chunk_len
                                         ]

                        if restored != original_slice:
                            verified_ok = False
                            break

                        bytes_verified += chunk_len
                        del restored, body

                if verified_ok and bytes_verified == orig_len:
                    print(f" OK (Bit-perfect)")
                else:
                    print(f" FAIL (Mismatch or Truncated)")

            except Exception as e:
                print(f" CRASH ({e})")

        # Cleanup input buffer
        del original_data

    print(f"\nBENCHMARK COMPLETED.")


if __name__ == "__main__":
    main()
