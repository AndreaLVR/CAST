# CAST: System Mode (7z Backend) Implementation

This directory contains the **Hybrid** implementation of the CAST algorithm.
Unlike the `native` variant, this version combines the logical flexibility of Python with the raw speed of the system's **7-Zip** executable.

## 🎯 Design Philosophy

1.  **Performance:** Bypasses Python's single-threaded `lzma` module constraints. By piping data to an external process, we unlock **Multithreading** and optimized C++ performance.
2.  **Rust Parity:** Uses the exact same compression parameters (LZMA2 Ultra, 128MB Dictionary) as the Rust implementation.
3.  **Seamless Integration:** If 7-Zip is available, it transparently replaces the backend compressor while keeping the Python API identical.

## ⚙️ Technical Characteristics

* **Parsing:** Uses Python's `re` module (compiled C Regex) for tokenization (same as Native).
* **Compression:** Instead of `import lzma`, this version uses `subprocess.Popen` to pipe streams:
    * **Mechanism:** `stdin` (Python) -> `7z process` -> `stdout` (Python).
    * **Arguments:** `-mx=9` (Ultra), `-m0=lzma2:d128m` (128MB Dict), `-mmt=on` (Multithreading).
* **Memory:**
    * **Solid Mode:** Loads the entire file into RAM, then pipes it to 7z.
    * **Chunked Mode:** Stream-processing available via CLI options (pipes one chunk at a time).

## 🛠️ Prerequisites

**You must have 7-Zip installed and configured via Environment Variable.**

### 1. Install 7-Zip
* **Windows:** Download and install from [7-zip.org](https://www.7-zip.org/).
* **Linux:** `sudo apt install p7zip-full` (Ubuntu/Debian) or equivalent.
* **macOS:** `brew install p7zip`

### 2. Set Environment Variable (Required)
You must set the `SEVEN_ZIP_PATH` variable pointing to the 7z executable.

* **Windows (PowerShell):**
  ```powershell
  $env:SEVEN_ZIP_PATH="C:\Program Files\7-Zip\7z.exe"
  ```
* **Linux / macOS:**
  ```bash
  export SEVEN_ZIP_PATH="/usr/bin/7z"
  ```

## 📂 Files

* **`cast.py`**: Modified implementation. Detects `7z` on init and delegates compression/decompression via IPC (Inter-Process Communication).
* **`cli.py`**: Command-line tool. It will report `[i] Active Strategy: ... (System 7z)` if the backend is active.
* **`run_benchmarks.py`**: Tool to compare CAST efficiency against LZMA2/Zstd/Brotli.

## 🚀 Usage

### 1. CLI Tool (Compression & Decompression)

**Compression (Recommended with -v):**
```bash
# Solid Compression (Max Ratio, High RAM) + Immediate Verification
python cli.py -c input.csv output.cast -v

# Chunked Compression (Low RAM, slightly lower Ratio) + Immediate Verification
python cli.py -c input.csv output.cast --chunk-size 300MB -v
```

**Decompression:**
```bash
# Decompression uses 7z automatically if available
python cli.py -d output.cast restored.csv
```

**Verification Only (Standalone):**
```bash
python cli.py -v output.cast
```

### 2. Benchmarks

To run benchmarks, you first need to install the competitor libraries:
```bash
pip install -r ../requirements.txt
```

Then run the suite:
```bash
# Compare against all competitors
python run_benchmarks.py --list ../files.txt --all

# Compare against LZMA only
python run_benchmarks.py --file data.csv --lzma

# Benchmark with Simulated Chunking
# (Note: Unlike the CLI, this loads the full file into RAM to ensure fair timing vs competitors)
python run_benchmarks.py --file data.csv --all --chunk-size 100MB
```

---
