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
    return f"{n:,} bytes"


def print_usage():
    print("\nCAST Compressor CLI (Python Port)")
    print("---------------------")
    print("Usage:")
    print("  COMPRESS:   python main.py -c <in> <out> [-v]")
    print("    -v / --verify        : Post-creation integrity check")
    print("")
    print("  DECOMPRESS: python main.py -d <in> <out>")
    print("  VERIFY:     python main.py -v <in>")


# --- COMPRESSION ---
def do_compress(input_path, output_path, verify=False):
    start_total = time.time()

    # SOLID Mode: We treat the file as a single block.
    # Warning: This will load the entire file into RAM.

    print("\n[*]    Starting Solid Compression...")

    try:
        # 1. Full Read
        with open(input_path, "rb") as f_in:
            file_data = f_in.read()

        total_len = len(file_data)
        print(f"       Input Size:     {format_bytes(total_len)}")

        # 2. CRC Calculation
        file_crc = zlib.crc32(file_data)

        # 3. Compression
        print(f"       Compressing... ", end="", flush=True)
        compressor = CASTCompressor()

        # Adapt unpacking based on your cast.py version
        res = compressor.compress(file_data)

        if isinstance(res, tuple) and len(res) >= 4:
            c_reg, c_ids, c_vars, id_flag = res[:4]
        else:
            raise ValueError("Unexpected compressor output from cast.py")

        print(" Done.")

        # 4. Write Output
        # Header: CRC(4) | L_REG(4) | L_IDS(4) | L_VARS(4) | FLAG(1)
        header = struct.pack(
            "<IIIIB", file_crc, len(c_reg), len(c_ids), len(c_vars), id_flag
        )

        with open(output_path, "wb") as f_out:
            f_out.write(header)
            f_out.write(c_reg)
            f_out.write(c_ids)
            f_out.write(c_vars)

        total_written = len(header) + len(c_reg) + len(c_ids) + len(c_vars)

        # Immediate RAM cleanup
        del file_data
        del c_reg
        del c_ids
        del c_vars

    except Exception as e:
        print(f"\n[!] Error during compression: {e}")
        return

    ratio = total_len / total_written if total_written > 0 else 0.0
    elapsed = time.time() - start_total

    print(f"\n[+]    Compression completed!")
    print(f"       Total Output:   {format_bytes(total_written)}")
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
                    print("\n[!] Unexpected EOF reading header.")
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

    except Exception as e:
        print(f"\n[!] Error during decompression: {e}")
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

    # Remove unsupported flags if user inputs them by mistake
    clean_args = [arg for arg in args if arg not in ["--chunk-size", "--multithread"]]

    verify_flag = "-v" in clean_args or "--verify" in clean_args
    # Remove verify flag from list to identify input/output
    cmd_args = [arg for arg in clean_args if arg not in ["-v", "--verify"]]

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

        print("\n[*]   Starting Compression...")
        print(f"      Input:      {input_file}")
        print(f"      Output:     {output_file}")
        print(f"      Mode:       SOLID (Single Block)")

        do_compress(input_file, output_file, verify_flag)

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
