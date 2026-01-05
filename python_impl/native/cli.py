import sys
import os
import struct
import time
import zlib

try:
    from cast import CASTCompressor, CASTDecompressor
except ImportError:
    print("[ERROR] File 'cast.py' not found. Ensure it is in the same directory.")
    sys.exit(1)


def format_bytes(n):
    """Formats bytes with commas for readability (e.g. 1,024,000 bytes)."""
    if n is None: return "Default"
    return f"{n:,} bytes"


def parse_human_size(size_str):
    """Parses a human readable size string (e.g. '100MB', '1GB') into bytes."""
    if not size_str:
        return None

    s = size_str.strip().upper()
    try:
        if s.endswith("GB"):
            return int(float(s[:-2]) * 1024**3)
        elif s.endswith("MB"):
            return int(float(s[:-2]) * 1024**2)
        elif s.endswith("KB"):
            return int(float(s[:-2]) * 1024)
        elif s.endswith("B"):
            return int(s[:-1])
        else:
            return int(s)
    except ValueError:
        print(
            f"[!] Error: Invalid chunk size format '{size_str}'. Using default (Solid)."
        )
        return None


def print_usage():
    print("\nCAST Compressor CLI (Python Port)")
    print("---------------------")
    print("Usage:")
    print("  COMPRESS:   python cli.py -c <in> <out> [options]")
    print("    --chunk-size <SIZE>  : Split processing (e.g., '100MB', '1GB')")
    print("    --dict-size <SIZE>   : LZMA Dictionary Size (Default: 128MB)")
    print("    -v / --verify        : Post-creation integrity check")
    print("")
    print("  DECOMPRESS: python cli.py -d <in> <out>")
    print("  VERIFY:     python cli.py -v <in>")


# --- COMPRESSION ---
# CHANGED: Added dict_size parameter
def do_compress(input_path, output_path, chunk_size=None, dict_size=None, verify=False):
    start_total = time.time()

    mode_str = (
        f"CHUNKED ({format_bytes(chunk_size)})"
        if chunk_size
        else "SOLID (Single Block)"
    )
    dict_str = format_bytes(dict_size) if dict_size else "Default (128MB)"

    print(f"      Mode:       {mode_str}")
    print(f"      Dict Size:  {dict_str}")
    print("\n[*]    Starting Compression...")

    total_input_processed = 0
    total_output_written = 0
    chunk_idx = 0

    try:
        with open(input_path, "rb") as f_in, open(output_path, "wb") as f_out:
            while True:
                # Read logic: If chunk_size is defined, read specific amount.
                # If None (Solid mode), read(-1) reads the whole remaining file.
                read_amount = chunk_size if chunk_size else -1

                chunk_data = f_in.read(read_amount)

                if not chunk_data:
                    break  # End of file

                chunk_idx += 1
                chunk_len = len(chunk_data)
                total_input_processed += chunk_len

                # UI: Update line with carriage return
                print(
                    f"\r       Processing Chunk #{chunk_idx} ({format_bytes(chunk_len)})... ",
                    end="",
                    flush=True,
                )

                # 1. CRC Calculation for this chunk
                chunk_crc = zlib.crc32(chunk_data)

                # 2. Compression
                compressor = CASTCompressor()
                # CHANGED: Pass dict_size
                res = compressor.compress(chunk_data, dict_size=dict_size)

                if isinstance(res, tuple) and len(res) >= 4:
                    c_reg, c_ids, c_vars, id_flag = res[:4]
                else:
                    raise ValueError("Unexpected compressor output from cast.py")

                # 3. Write Output Header + Body
                # Header: CRC(4) | L_REG(4) | L_IDS(4) | L_VARS(4) | FLAG(1)
                header = struct.pack(
                    "<IIIIB", chunk_crc, len(c_reg), len(c_ids), len(c_vars), id_flag
                )

                f_out.write(header)
                f_out.write(c_reg)
                f_out.write(c_ids)
                f_out.write(c_vars)

                written_this_round = len(header) + len(c_reg) + len(c_ids) + len(c_vars)
                total_output_written += written_this_round

                # Explicit Memory Cleanup (Crucial for Chunking in Python)
                del chunk_data
                del c_reg
                del c_ids
                del c_vars
                # Force Python to release references immediately

        print(" Done.")

    except Exception as e:
        print(f"\n\n[!] Error during compression: {e}")
        return

    ratio = (
        total_input_processed / total_output_written
        if total_output_written > 0
        else 0.0
    )
    elapsed = time.time() - start_total

    print(f"\n[+]    Compression completed!")
    print(f"       Chunks:         {chunk_idx}")
    print(f"       Total Input:    {format_bytes(total_input_processed)}")
    print(f"       Total Output:   {format_bytes(total_output_written)}")
    print(f"       Ratio:          {ratio:.2f}x")
    print(f"       Time:           {elapsed:.2f}s")

    if verify:
        print("\n------------------------------------------------")
        print("[*]   Starting Post-Compression Verification...")
        # Technical pause to ensure OS releases the file handle
        time.sleep(0.5)
        do_verify_standalone(output_path)


# --- DECOMPRESSION ---
def do_decompress(input_path, output_path):
    start = time.time()
    chunk_idx = 0

    print("\n[*]    Extracting stream...")

    try:
        with open(input_path, "rb") as f_in, open(output_path, "wb") as f_out:
            while True:
                # Read Header 17 bytes
                header_data = f_in.read(17)
                if not header_data:
                    break
                if len(header_data) < 17:
                    # Only warn if we read partial header bytes, implies corruption
                    if len(header_data) > 0:
                        print("\n[!] Unexpected EOF reading header (file truncated?).")
                    break

                chunk_idx += 1
                expected_crc, l_reg, l_ids, l_vars, id_flag = struct.unpack(
                    "<IIIIB", header_data
                )

                # Read Body
                body_len = l_reg + l_ids + l_vars
                body_data = f_in.read(body_len)

                if len(body_data) != body_len:
                    print(f"\n[!] Truncated file in body at chunk {chunk_idx}.")
                    break

                print(f"\r       Extracting Chunk #{chunk_idx}...", end="", flush=True)

                # Slice buffer
                c_reg = body_data[0:l_reg]
                c_ids = body_data[l_reg : l_reg + l_ids]
                c_vars = body_data[l_reg + l_ids : l_reg + l_ids + l_vars]

                decompressor = CASTDecompressor()
                restored = decompressor.decompress(
                    c_reg,
                    c_ids,
                    c_vars,
                    expected_crc=expected_crc,
                    id_mode_flag=id_flag,
                )

                f_out.write(restored)

                # Cleanup
                del body_data, c_reg, c_ids, c_vars, restored

    except Exception as e:
        print(f"\n\n[!] Error during decompression: {e}")
        return

    elapsed = time.time() - start
    print(f"\n\n[+]    Decompression done in {elapsed:.2f}s")


# --- VERIFICATION ---
def do_verify_standalone(input_path):
    start = time.time()
    chunk_idx = 0

    print("\n[*]    Verifying Stream Integrity...")

    try:
        with open(input_path, "rb") as f_in:
            while True:
                header_data = f_in.read(17)
                if not header_data:
                    break
                if len(header_data) < 17:
                    if len(header_data) > 0:
                        print(f"\n[!] Partial header at end of file.")
                    break

                chunk_idx += 1
                expected_crc, l_reg, l_ids, l_vars, id_flag = struct.unpack(
                    "<IIIIB", header_data
                )

                body_len = l_reg + l_ids + l_vars
                body_data = f_in.read(body_len)

                if len(body_data) != body_len:
                    print(f"\n[!] Truncated file at chunk {chunk_idx}.")
                    sys.exit(1)

                # UI: Feedback before calculation
                print(f"\r       Verifying Chunk #{chunk_idx}... ", end="", flush=True)

                c_reg = body_data[0:l_reg]
                c_ids = body_data[l_reg : l_reg + l_ids]
                c_vars = body_data[l_reg + l_ids : l_reg + l_ids + l_vars]

                try:
                    decompressor = CASTDecompressor()
                    restored = decompressor.decompress(
                        c_reg,
                        c_ids,
                        c_vars,
                        expected_crc=expected_crc,
                        id_mode_flag=id_flag,
                    )

                    # Manual CRC check for safety
                    calc_crc = zlib.crc32(restored)
                    if calc_crc != expected_crc:
                        print(f"\n[!]    FAILURE: CRC Mismatch at Chunk {chunk_idx}!")
                        sys.exit(1)

                    # Cleanup
                    del body_data, c_reg, c_ids, c_vars, restored

                except Exception as e:
                    print(
                        f"\n[!]    CRASH: Decompression error at Chunk {chunk_idx}! ({e})"
                    )
                    sys.exit(1)

    except Exception as e:
        print(f"\n[!] Verification error: {e}")
        return

    elapsed = time.time() - start
    print(
        f"\n\n[+]    FILE INTEGRITY VERIFIED. Chunks: {chunk_idx}. Time: {elapsed:.2f}s"
    )


# --- MAIN ENTRY POINT ---
if __name__ == "__main__":
    args = sys.argv[1:]

    # 1. Parse Chunk Size
    chunk_size_bytes = None
    if "--chunk-size" in args:
        try:
            idx = args.index("--chunk-size")
            # Ensure there is a value after the flag
            if idx + 1 < len(args):
                size_str = args[idx + 1]
                chunk_size_bytes = parse_human_size(size_str)
                # Remove flag and value from args so they don't interfere later
                del args[idx : idx + 2]
            else:
                print("[!] Error: --chunk-size requires a value (e.g. 100MB)")
                sys.exit(1)
        except ValueError:
            pass

    # CHANGED: 2. Parse Dict Size
    dict_size_bytes = None
    if "--dict-size" in args:
        try:
            idx = args.index("--dict-size")
            if idx + 1 < len(args):
                size_str = args[idx + 1]
                dict_size_bytes = parse_human_size(size_str)
                # Remove
                del args[idx : idx + 2]
            else:
                print("[!] Error: --dict-size requires a value (e.g. 128MB)")
                sys.exit(1)
        except ValueError:
            pass

    # 3. Parse Multithread (Still unsupported, but remove to prevent errors if passed)
    if "--multithread" in args:
        print(
            "[*] Note: Multithreading is NOT supported in Python implementation. Running single-threaded."
        )
        # Only remove if it exists, to avoid errors
        try:
            args.remove("--multithread")
        except ValueError:
            pass

    # 4. Parse Verify Flag
    verify_flag = "-v" in args or "--verify" in args
    # Remove verify flag from list to identify input/output paths cleanly
    cmd_args = [arg for arg in args if arg not in ["-v", "--verify"]]

    if len(cmd_args) < 1:
        print_usage()
        sys.exit(0)

    mode = cmd_args[0]

    print("\n\n|--    CAST: Columnar Agnostic Structural Transformation    --|\n")

    if mode == "-c":
        if len(cmd_args) < 3:
            print("[!] Missing output path.")
            print_usage()
            sys.exit(1)

        input_file = cmd_args[1]
        output_file = cmd_args[2]

        if not os.path.exists(input_file):
            print(f"[!] Error: Input file '{input_file}' not found.")
            sys.exit(1)

        print(f"      Input:      {input_file}")
        print(f"      Output:     {output_file}")

        # CHANGED: Pass dict_size
        do_compress(
            input_file,
            output_file,
            chunk_size=chunk_size_bytes,
            dict_size=dict_size_bytes,
            verify=verify_flag
        )

    elif mode == "-d":
        if len(cmd_args) < 3:
            print("[!] Missing output path.")
            sys.exit(1)

        do_decompress(cmd_args[1], cmd_args[2])

    else:
        # Fallback: Verification or direct file (assumes verification)
        target_file = mode

        # If user passed only "-v file", cmd_args will only have "file" because we removed -v above.
        # If user passed only "file", we assume verification.

        if verify_flag or os.path.exists(target_file):
            if not os.path.exists(target_file):
                print(f"[!] Error: File '{target_file}' not found.")
                sys.exit(1)

            do_verify_standalone(target_file)
        else:
            print(f"[!] Unknown command: {mode}")
            print_usage()