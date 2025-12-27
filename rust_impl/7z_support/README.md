# CAST: Columnar Agnostic Structural Transformation (Rust with 7z Port)

A high-performance Rust port of the CAST (Columnar Agnostic Structural Transformation) algorithm.
This project implements a hybrid compression strategy (Template/Dictionary + LZMA2) offering two distinct operating modes: **Solid** (Single thread, Maximum Compression) and **Multithread** (Maximum Speed).

It also includes a comprehensive **Benchmark Suite** to compare performance against industry standards (LZMA2, Brotli, Zstd).

## 📂 Project Structure

* **`src/lib.rs`**: Library entry point.
* **`src/cast.rs`**: Core logic (CAST/GTF Algorithm).
* **`src/main.rs`**: CLI Entry point (Compress/Decompress/Verify).
* **`src/bin/run_benchmarks.rs`**: Advanced Benchmarking Suite.

## 🛠️ Prerequisites

### Windows (Static Setup)
To build this project on Windows, you need **vcpkg** to obtain the static version of `liblzma`.

1.  **Download and Install vcpkg:**
    Open PowerShell (as Administrator) and run:
    ```powershell
    git clone [https://github.com/microsoft/vcpkg.git](https://github.com/microsoft/vcpkg.git)
    cd vcpkg
    .\bootstrap-vcpkg.bat
    ```

2.  **Install static liblzma:**
    ```powershell
    .\vcpkg install liblzma:x64-windows-static
    ```

3.  **Configure Environment:**
    Tell Cargo where vcpkg is located (replace path with yours).

    **PowerShell:**
    ```powershell
    $env:VCPKG_ROOT = "C:\path\to\your\vcpkg"
    ```

    **CMD (Command Prompt):**
    ```cmd
    set VCPKG_ROOT=C:\path\to\your\vcpkg
    ```

### Linux (Ubuntu/Debian)
Simply install the required development packages:
```bash
sudo apt update
sudo apt install build-essential liblzma-dev pkg-config
```

---

## 🚀 Build

To create the optimized executable (Release mode):

```powershell
cargo build --release
```

The binary will be located at:
* **Windows:** `target/release/CAST.exe`
* **Linux:** `target/release/CAST`

*(Note: The exact filename depends on the `name` field in your `Cargo.toml`).*

---

## 📦 CLI Usage (User Tool)

The main tool (`src/main.rs`) allows you to compress, decompress, and verify single files.

### 1. Compression
**Syntax:**
```powershell
cargo run --release -- -c <input_file> <output_file> [options]
```

**Options:**
* `--multithread`: Uses all CPU cores. Faster, but slightly lower compression ratio.
* `--chunk-size <SIZE>`: **RAM Saver.** Splits the input into chunks of the specified size (e.g., `100MB`, `1GB`, `500KB`). Critical for processing huge files larger than available RAM.
* `-v` or `--verify`: **Security Check.** Immediately verifies the created archive after compression. Recommended for backups.

**Examples:**
```powershell
# Standard Compression (Fastest)
cargo run --release -- -c "data.csv" "archive.gtf"

# Compression + Verification (Safest)
cargo run --release -- -c "data.csv" "archive.gtf" -v

# Huge Files (> RAM) with Chunking (e.g., 500MB chunks)
cargo run --release -- -c "huge_dataset.csv" "archive.gtf" --chunk-size 500MB

# Max Speed + Chunking + Verification
cargo run --release -- -c "huge.csv" "archive.gtf" --multithread --chunk-size "1 GB" -v
```

### 2. Decompression
Automatically detects the format, restores the file, and verifies CRC32 integrity. No chunk size needed (auto-detected).

```powershell
cargo run --release -- -d "archive.gtf" "restored.csv"
```

### 3. Verification (Standalone)
Checks the integrity of an archive (CRC32 & Structure) without writing the decompressed file to disk. Useful for testing backups.

```powershell
cargo run --release -- -v "archive.gtf"
```

---

## 📊 Benchmark Suite

The benchmarking tool (`src/bin/run_benchmarks.rs`) compares CAST against **LZMA2**, **Brotli**, and **Zstd** (all set to maximum compression settings).

**Note:** The `--list` and `--compare-with` arguments are **mandatory**.

### Syntax
```powershell
cargo run --release --bin run_benchmarks -- --list <file_list.txt> --compare-with <algos> [options]
```

### Parameters
* `--list <path>`: Path to a text file containing the list of files to test (one path per line).
* `--compare-with <algos>`: Comma-separated list of algorithms to test against: `lzma2`, `brotli`, `zstd`, or `all`.
* `--multithread`: Enables multithreading for CAST, LZMA2, and Zstd tests.
* `--chunk-size <SIZE>`: Forces a chunk-based compression for ALL algorithms (CAST and competitors) to simulate memory-constrained environments or block-based storage.

### How to prepare the file list
Create a text file (e.g., `files.txt`) with absolute or relative paths:
```text
C:\Data\dataset_1.json
D:\Logs\server_dump.log
# You can comment out lines with #
# ..\test\ignored_file.txt
```

### Examples

**1. Full Comparison (Global/Solid Mode):**
Best for maximum compression ratio (uses all RAM).
```powershell
cargo run --release --bin run_benchmarks -- --list files.txt --compare-with all
```

**2. Chunked Comparison (e.g., 100MB blocks):**
Fair comparison for block-based compression or low-memory scenarios. Resets dictionary every 100MB for all competitors.
```powershell
cargo run --release --bin run_benchmarks -- --list files.txt --compare-with all --chunk-size 100MB
```

**3. Multithreaded Comparison:**
```powershell
cargo run --release --bin run_benchmarks -- --list files.txt --compare-with zstd --multithread
```