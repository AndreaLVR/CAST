# CAST: Python Implementation

> ⚠️ **Disclaimer: Readable Reference & High Performance PoC**
>
> This implementation serves primarily as a **Readable Reference** to understand the CAST algorithm's logic (`Skeleton` + `Variables`) and serialization format.
>
> While **heavily optimized** (via regex tokenization), Python is inherently limited by the GIL (Global Interpreter Lock).
> * **For Production/Speed:** Use the [Rust Implementation](../rust).
> * **For Study/Prototyping:** This version is fully feature-complete and supports the same compression logic.
>
> **💡 Key Performance Metric:** Regardless of the language, CAST aims to demonstrate that structural pre-processing can improve both **Density** and **Encoding Speed** simultaneously compared to standard LZMA2.

This directory contains the Python reference implementation of the CAST algorithm.

To balance readability with performance, this tool supports two operating modes (backends):
1.  **Native Mode (Pure Python):** Uses the standard library `lzma` module. Zero external dependencies, highly portable, but single-threaded.
2.  **7-Zip Mode (High Performance):** Delegates raw compression to an external `7z` executable via pipes. Unlocks **multi-threading** and higher throughput.

---

## 📂 Project Structure

* **`cast.py`**: Core Algorithm logic (Agnostic). Separation of Skeletons and Variables.
* **`cast_lzma.py`**: Backend implementations (Native `lzma` vs External `subprocess` for 7-Zip).
* **`cli.py`**: Command-line interface for compression, decompression, and verification.
* **`run_benchmarks.py`**: Automated benchmarking suite against LZMA, Brotli, and Zstd.

---

## 🛠️ Prerequisites

* **Python 3.10+**: No special compilation required.
* **Dependencies:** Only required for running benchmarks (to compare against Zstd/Brotli).
  ```bash
  pip install -r requirements.txt
  ```

### (Optional) 7-Zip
To use the high-performance **7-Zip Mode** (`--mode 7zip`), you must ensure `7z` is installed:
* **Windows:** [7-zip.org](https://www.7-zip.org/)
* **Linux:** `sudo apt install p7zip-full`
* **macOS:** `brew install sevenzip`

---

## 📦 CLI Usage (User Tool)

The tool (`cli.py`) allows you to compress, decompress, and verify single files.

### 1. Compression
**Syntax:**
```bash
python cli.py -c <input_file> <output_file> [options]
```

**Options:**
* `--mode <native|7zip>`: Selects the compression backend.
    * `auto` (Default): Tries to find `7z`. If found, uses it (faster). If not, falls back to `native`.
    * `7zip`: Forces usage of external 7-Zip. Fails if not found.
    * `native`: Forces usage of internal library (single-threaded).
* `--chunk-size <SIZE>`: **RAM Saver.** Splits input into chunks (e.g., `100MB`, `1GB`). Enables streaming processing for files larger than RAM.
* `--dict-size <SIZE>`: Sets LZMA Dictionary Size (Default: 128MB).
* `-v` or `--verify`: **Security Check.** Immediately verifies the archive after creation.

**Examples:**

```bash
# Auto-detect best mode (Recommended)
python cli.py -c data.csv archive.cast -v

# Force High-Performance 7-Zip mode (Multithreaded)
python cli.py -c data.csv archive.cast --mode 7zip -v

# Force Pure Python mode (Single-threaded, Max Portability)
python cli.py -c data.csv archive.cast --mode native -v

# Low RAM Environment (Chunked processing) and Custom Dictionary size
python cli.py -c huge.csv archive.cast --chunk-size 500MB --dict-size 64MB -v
```

### 2. Decompression
Automatically detects the format and uses the best available backend.

```bash
python cli.py -d archive.cast restored.csv
```

### 3. Verification (Standalone)
Checks integrity (CRC32 & Structure) without writing to disk.

```bash
python cli.py -v archive.cast
```

---

## ⚙️ Configuration (7-Zip Path)

The tool automatically searches for `7z` (or `7zz`) in standard system paths.
If your executable is in a custom location, set the environment variable:

* **Windows (PowerShell):** `$env:SEVEN_ZIP_PATH = "C:\MyTools\7z.exe"`
* **Bash:** `export SEVEN_ZIP_PATH="/usr/local/bin/7zz"`

---

## 📊 Benchmark Suite

The benchmarking tool compares CAST against industry standards (**LZMA2**, **Brotli**, **Zstd**).

**Important:** The `--mode` flag determines not only how CAST runs, but also how the **LZMA2 competitor** runs, ensuring a fair comparison.

**Syntax:**
```bash
python run_benchmarks.py --list <file_list.txt> --compare-with <algos> [options]
```

**Parameters:**
* `--list <path>`: Text file with list of files to test (one per line).
* `--compare-with <algos>`: `lzma2`, `brotli`, `zstd`, or `all`.
* `--mode <native|7zip>`: Backend selection (Default: auto).
* `--dict-size <SIZE>`: Sets LZMA Dictionary Size (e.g., 64MB, 256MB). Default: 128MB.
* `--chunk-size <SIZE>`: Forces chunked processing for all algorithms.

**Examples:**

```bash
# Full Performance Comparison (using 7-Zip if available)
python run_benchmarks.py --list files.txt --compare-with all --mode 7zip

# Native Pure Python Comparison with larger dictionary
python run_benchmarks.py --list files.txt --compare-with zstd,brotli --mode native --dict-size 256MB
```

---

## 🧠 Implementation Details

### Parsing Strategy: Regex vs. Manual
Unlike the Rust implementation, which uses a custom zero-copy byte parser for maximum speed, this Python version uses the standard `re` (Regex) module.

* **Reasoning:** In Python, a manual character-by-character loop is prohibitively slow due to interpreter overhead. The `re` module is implemented in C, making it significantly faster than any manual Python loop while keeping the code simple and readable.
