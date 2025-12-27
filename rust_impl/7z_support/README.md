# CAST: Columnar Agnostic Structural Transformation (Rust + 7z Backend)

A high-performance Rust implementation of the CAST (Columnar Agnostic Structural Transformation) algorithm.
This project uses a hybrid compression strategy (Template/Dictionary + LZMA2) relying on an **external 7-Zip executable** for the heavy lifting. This approach ensures maximum performance and compatibility without complex build dependencies.

It includes a comprehensive **Benchmark Suite** to compare performance against industry standards (LZMA2, Brotli, Zstd).

## 📂 Project Structure

* **`src/lib.rs`**: Library entry point.
* **`src/cast.rs`**: Core logic (CAST/GTF Algorithm + 7z wrapper).
* **`src/main.rs`**: CLI Entry point (Compress/Decompress/Verify).
* **`src/bin/run_benchmarks.rs`**: Advanced Benchmarking Suite.

---

## ⚙️ Configuration (Crucial)

Since this tool wraps the 7-Zip executable, **you must ensure the system can find it**.

### 1. Install 7-Zip
* **Windows:** Download and install from [7-zip.org](https://www.7-zip.org/).
* **Linux:** Install via terminal (e.g., `sudo apt install p7zip-full` or `7zip`).

### 2. Set the Environment Variable
You must tell CAST where the executable is located if it is not in your global system PATH (or if you want to use a specific version).

**Option A: Temporary (PowerShell - Current Session Only)**
```powershell
$env:SEVEN_ZIP_PATH = "C:\Program Files\7-Zip\7z.exe"
```

**Option B: Permanent (PowerShell - User Profile)**
```powershell
[System.Environment]::SetEnvironmentVariable("SEVEN_ZIP_PATH", "C:\Program Files\7-Zip\7z.exe", "User")
```

**On Linux:**
```bash
export SEVEN_ZIP_PATH="/usr/bin/7z"
```

*(Note: Restart your terminal after running this command).*

---

## 🚀 Build

```powershell
cargo build --release
```

The binary will be located at:
* **Windows:** `target/release/cast.exe`
* **Linux:** `target/release/cast`

---

## 📦 CLI Usage (User Tool)

The main tool (`src/main.rs`) allows you to compress, decompress, and verify single files.

### 1. Compression
**Syntax:**
```powershell
cargo run --release -- -c <input_file> <output_file> [options]
```

**Options:**
* `--chunk-size <SIZE>`: **RAM Saver.** Splits the input into chunks of the specified size (e.g., `100MB`, `1GB`, `500KB`). Critical for processing huge files larger than available RAM.
* `-v` or `--verify`: **Security Check.** Immediately verifies the created archive after compression. Recommended for backups.

**Examples:**
```powershell
# Standard Compression (Max Performance)
cargo run --release -- -c "data.csv" "archive.cast"

# Compression + Verification (Safest)
cargo run --release -- -c "data.csv" "archive.cast" -v

# Huge Files (> RAM) with Chunking (e.g., 500MB chunks)
cargo run --release -- -c "huge_dataset.csv" "archive.cast" --chunk-size 500MB
```

### 2. Decompression
Automatically detects the format, restores the file, and verifies CRC32 integrity. No chunk size needed (auto-detected).

```powershell
cargo run --release -- -d "archive.cast" "restored.csv"
```

### 3. Verification (Standalone)
Checks the integrity of an archive (CRC32 & Structure) without writing the decompressed file to disk. Useful for testing backups.

```powershell
cargo run --release -- -v "archive.cast"
```

---

## 📊 Benchmark Suite

The benchmarking tool (`src/bin/run_benchmarks.rs`) compares CAST against **LZMA2**, **Brotli**, and **Zstd**.
All algorithms are configured to run at **Maximum Performance** (utilizing all available threads where supported).

**Note:** The `--list` and `--compare-with` arguments are **mandatory**.

### Syntax
```powershell
cargo run --release --bin run_benchmarks -- --list <file_list.txt> --compare-with <algos> [options]
```

### Parameters
* `--list <path>`: Path to a text file containing the list of files to test (one path per line).
* `--compare-with <algos>`: Comma-separated list of algorithms to test against: `lzma2`, `brotli`, `zstd`, or `all`.
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

**1. Full Comparison (Global):**
Best for maximum compression ratio (uses all RAM).
```powershell
cargo run --release --bin run_benchmarks -- --list files.txt --compare-with all
```

**2. Chunked Comparison (e.g., 100MB blocks):**
Fair comparison for block-based compression or low-memory scenarios. Resets dictionary every 100MB for all competitors.
```powershell
cargo run --release --bin run_benchmarks -- --list files.txt --compare-with all --chunk-size 100MB
```

**3. Specific Competitor Comparison:**
```powershell
cargo run --release --bin run_benchmarks -- --list files.txt --compare-with zstd
```

**4. Multiple Specific Competitors Comparison:**
```powershell
cargo run --release --bin run_benchmarks -- --list files.txt --compare-with zstd, brotli
```



