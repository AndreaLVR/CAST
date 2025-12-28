import argparse
import lzma
import os
import struct
import time
import zlib
from typing import List

try:
    import zstandard as zstd
except ImportError:
    zstd = None

try:
    import brotli
except ImportError:
    brotli = None

# Import classes from core file
try:
    from cast import CASTCompressor, CASTDecompressor
except ImportError:
    print("[ERROR] File 'cast.py' not found in the current directory.")
    exit(1)


def load_file_list(list_path: str) -> List[str]:
    """
    Loads the list of files from a text file, cleaning lines and removing comments.

    :param list_path: The file path to the text file containing the list of files.
    :return: A list of file paths extracted from the list file.
    """
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
    """
    Main entry point for the CAST Compression Benchmark Tool.

    Parses command-line arguments, configures active competitors (LZMA, Zstd, Brotli),
    loads the files to test, and runs the compression/decompression benchmark loop.
    Prints results, timings, and verification status to standard output.

    :return: None
    """
    parser = argparse.ArgumentParser(description="CAST Compression Benchmark Tool")

    # Input: List or Single file
    input_group = parser.add_mutually_exclusive_group(required=True)
    input_group.add_argument(
        "--list", type=str, help="Path to text file containing list of files."
    )
    input_group.add_argument("--file", type=str, help="Path to a single file to test.")

    # Competitor Flags
    parser.add_argument(
        "--lzma", action="store_true", help="Enable LZMA2 (XZ) benchmark."
    )
    parser.add_argument(
        "--brotli", action="store_true", help="Enable Brotli benchmark."
    )
    parser.add_argument(
        "--zstd", action="store_true", help="Enable Zstandard benchmark."
    )
    parser.add_argument("--all", action="store_true", help="Enable ALL competitors.")

    args = parser.parse_args()

    # Competitor Configuration
    RUN_LZMA = args.all or args.lzma
    RUN_BROTLI = args.all or args.brotli
    RUN_ZSTD = args.all or args.zstd

    # Check libraries
    if RUN_BROTLI and not brotli:
        print("NOTE: Module 'brotli' missing. Skipped.")
        RUN_BROTLI = False
    if RUN_ZSTD and not zstd:
        print("NOTE: Module 'zstandard' missing. Skipped.")
        RUN_ZSTD = False

    # File loading
    files_to_test: List[str] = []
    if args.list:
        files_to_test = load_file_list(args.list)
    elif args.file:
        files_to_test = [args.file]

    if not files_to_test:
        print("[!] No files to test.")
        return

    print(f"\nSTARTING CAST TEST SUITE (Python Impl)")
    print(
        f"Active competitors: LZMA={'ON' if RUN_LZMA else 'OFF'}, BROTLI={'ON' if RUN_BROTLI else 'OFF'}, ZSTD={'ON' if RUN_ZSTD else 'OFF'}"
    )
    print("=" * 70)

    for file_path in files_to_test:
        file_path = os.path.abspath(file_path)

        # Clean visual separator
        print(f"\n{'=' * 70}")
        print(f"FILE: {os.path.basename(file_path)}")
        print(f"PATH: {file_path}")
        print(f"{'-' * 70}")

        if not os.path.exists(file_path):
            print(f"[!] File not found: {file_path}")
            continue

        try:
            with open(file_path, "rb") as f:
                original_data = f.read()
        except Exception as e:
            print(f"[!] File reading error: {e}")
            continue

        orig_len = len(original_data)
        if orig_len == 0:
            print("[!] Empty file. Skipped.")
            continue

        original_crc = zlib.crc32(original_data)
        print(f"Original : {orig_len:,} bytes | CRC32: {original_crc}")
        print("-" * 70)

        results = {}
        times = {}

        # --- 1. LZMA ---
        if RUN_LZMA:
            print("[1] LZMA (Extreme)... ", end="", flush=True)
            start = time.time()
            try:
                lzma_data = lzma.compress(
                    original_data, format=lzma.FORMAT_XZ, preset=9 | lzma.PRESET_EXTREME
                )
                elapsed = time.time() - start
                times["LZMA"] = elapsed
                results["LZMA"] = len(lzma_data)
                print(f"Done in {elapsed:.2f}s | Size: {len(lzma_data):,} bytes")
            except Exception as e:
                print(f"ERROR: {e}")
                results["LZMA"] = float("inf")

        # --- 2. ZSTD ---
        if RUN_ZSTD:
            print("[2] Zstd (Level 22)...  ", end="", flush=True)
            start = time.time()
            try:
                cctx = zstd.ZstdCompressor(level=22)
                zstd_data = cctx.compress(original_data)
                elapsed = time.time() - start
                times["Zstd"] = elapsed
                results["Zstd"] = len(zstd_data)
                print(f"Done in {elapsed:.2f}s | Size: {len(zstd_data):,} bytes")
            except Exception as e:
                print(f"ERROR: {e}")
                results["Zstd"] = float("inf")

        # --- 3. BROTLI ---
        if RUN_BROTLI:
            print("[3] Brotli (Level 11)...", end="", flush=True)
            start = time.time()
            try:
                brotli_data = brotli.compress(
                    original_data, mode=brotli.MODE_GENERIC, quality=11
                )
                elapsed = time.time() - start
                times["Brotli"] = elapsed
                results["Brotli"] = len(brotli_data)
                print(f"Done in {elapsed:.2f}s | Size: {len(brotli_data):,} bytes")
            except Exception as e:
                print(f"ERROR: {e}")
                results["Brotli"] = float("inf")

        # --- 4. CAST ---
        print("[4] CAST...            ", end="", flush=True)
        start = time.time()
        compressor = CASTCompressor()
        try:
            res = compressor.compress(original_data)

            # Return tuple handling (compatibility with different cast.py versions)
            if isinstance(res, tuple) and len(res) >= 4:
                c_reg, c_ids, c_vars, id_flag = res[:4]
            else:
                raise ValueError("Unexpected compressor output.")

            header = struct.pack(
                "<IIIIB", original_crc, len(c_reg), len(c_ids), len(c_vars), id_flag
            )
            full_blob = header + c_reg + c_ids + c_vars

            elapsed = time.time() - start
            times["CAST"] = elapsed
            results["CAST"] = len(full_blob)

            with open(file_path + ".cast", "wb") as f:
                f.write(full_blob)

            print(f"Done in {elapsed:.2f}s | Size: {len(full_blob):,} bytes")

        except Exception as e:
            print(f"\n[ERROR] CAST Failed: {e}")
            results["CAST"] = float("inf")

        # --- RANKING ---
        print("-" * 70)
        valid_results = {k: v for k, v in results.items() if v != float("inf")}

        if not valid_results:
            print("No algorithm completed compression.")
            continue

        sorted_results = sorted(valid_results.items(), key=lambda item: item[1])
        winner_name, winner_size = sorted_results[0]

        for rank, (name, size) in enumerate(sorted_results, 1):
            ratio = orig_len / size if size > 0 else 0
            diff_vs_winner = size - winner_size
            diff_str = (
                f"(+{diff_vs_winner:,} bytes)" if diff_vs_winner > 0 else "(WINNER)"
            )
            elapsed = times.get(name, 0)
            print(
                f"{rank}. {name:<6} : {size:>10,} bytes | Ratio: {ratio:.2f}x | Time: {elapsed:.2f}s | {diff_str}"
            )

        print("-" * 70)

        if "CAST" in valid_results:
            if winner_name == "CAST":
                if len(sorted_results) > 1:
                    runner_up = sorted_results[1][1]
                    delta = runner_up - winner_size
                    improvement = (delta / runner_up) * 100
                    print(
                        f"RESULT: CAST WINS! Savings: {delta:,} bytes (+{improvement:.2f}%)"
                    )
                else:
                    print("RESULT: CAST WINS! (Sole competitor)")
            else:
                delta = results["CAST"] - winner_size
                print(f"RESULT: {winner_name} wins. CAST loses by {delta:,} bytes.")

            # --- VERIFICATION (Lock Fix) ---
            print("\n[*] Waiting for file release...", end="", flush=True)
            time.sleep(1.0)  # Anti-lock pause for Windows Defender/FS
            print(" OK.")

            print("[*] Verifying Decompression...", end="", flush=True)
            try:
                # Open file
                with open(file_path + ".cast", "rb") as f:
                    read_blob = f.read()

                # Unpack
                read_crc, l_reg, l_ids, l_vars, r_flag = struct.unpack(
                    "<IIIIB", read_blob[:17]
                )

                decompressor = CASTDecompressor()
                restored = decompressor.decompress(
                    read_blob[17 : 17 + l_reg],
                    read_blob[17 + l_reg : 17 + l_reg + l_ids],
                    read_blob[17 + l_reg + l_ids : 17 + l_reg + l_ids + l_vars],
                    expected_crc=read_crc,
                    id_mode_flag=r_flag,
                )

                if original_data == restored:
                    print(" SUCCESS (Data identical)")
                    # Optional: Remove temporary file
                    # try: os.remove(file_path + ".cast")
                    # except: pass
                else:
                    print(" FAILED (Data mismatch!)")
            except Exception as e:
                print(f" ERROR ({e})")

        # Explicit memory cleanup for large files
        del original_data
        if "lzma_data" in locals():
            del lzma_data
        if "zstd_data" in locals():
            del zstd_data

    print("\n" + "=" * 70)
    print("TESTS COMPLETED.")


if __name__ == "__main__":
    main()
