# CAST: Official Standard Rust Implementation 

> ⚠️ **Disclaimer: Proof of Concept & Performance Focus**
>
> This is an optimized reference implementation designed to demonstrate the full potential of the CAST algorithm. While it delivers production-grade performance and stability (especially in 7-Zip mode), it is a novel technology. Unlike legacy tools (gzip, xz) that have undergone decades of global field-testing, this tool is strictly for experimental use. Use it for archiving and benchmarking, but maintain backups for critical data.
> 
> **💡 Key Performance Metric:** The critical metric to observe is the **Simultaneous Enhancement**.
>
> On structured datasets CAST often breaks the traditional compression trade-off by delivering a **Dual Advantage**:
> 1. **Superior Density:** It often produces smaller files than standard LZMA2.
> 2. **Faster Encoding:** It significantly reduces processing time by simplifying the data stream *before* the backend encoder sees it.
> 3. **Memory Safety:** The **Block-Based Streaming Architecture** keeps memory usage bounded by the compressed block size, preventing crashes even on low-RAM devices (provided **Stream Chunking** was used during compression).
>
> **The goal is to demonstrate that structural pre-processing can improve both speed and ratio simultaneously.**

This directory contains the high-performance Rust port of the CAST algorithm.

This unified implementation supports two distinct operating modes (backends) within a single executable:
1.  **Native Mode (Standalone):** Uses embedded Rust libraries (`xz2`) for maximum portability, low latency, and zero runtime dependencies.
2.  **7-Zip Mode (High Performance):** Acts as a smart wrapper around an external **7z** executable, piping data **entirely in-memory** to leverage its optimized multi-threading engine without disk I/O overhead.

---

## 📥 Quick Start (Download & Run)

**You do NOT need to install Rust or compile anything.**
Simply download the pre-compiled executables for your operating system from the **[Releases Page](https://github.com/AndreaLVR/CAST/releases/)**.

The release package includes two executables:
1.  `cast` (The main compression tool)
2.  `run_benchmarks` (The testing suite)

### How to run
Open your terminal (Command Prompt, PowerShell, or Bash) in the folder where you downloaded the files.

* **Windows:** Use `cast.exe`
* **Linux/macOS:** Use `./cast` (Ensure you give execution permissions: `chmod +x cast`)

---

## 📦 CLI Usage (User Tool)

The main tool allows you to compress, decompress, and verify single files.

### 1. Compression
**Syntax:**
```bash
# Windows
cast -c <input_file> <output_file> [options]

# Linux/macOS
./cast -c <input_file> <output_file> [options]
```

**Options:**
* `--mode <native|7zip>`: Selects the compression backend.
    * `auto` (Default): **Smart Hybrid Strategy.** Tries to find `7z`. If found, uses it for **Compression** (High Throughput). If not, falls back to `native`.
    * `7zip`: Forces usage of external 7-Zip. Fails if not found.
    * `native`: Forces usage of internal library (single-threaded by default).
* `--multithread`: Enables multi-threading for the **Native** backend. (7-Zip mode is multi-threaded by default).
* `--chunk-size <SIZE>`: **Memory Guard**. Splits input into independent blocks (e.g., `64MB`, `256MB`) to strictly bound RAM usage `(O(ChunkSize))`. Recommended for files larger than available system memory.
* `--dict-size <SIZE>`: Sets LZMA Dictionary Size (Default: 128MB).
* `-v` or `--verify`: **Security Check.** Immediately verifies the archive after creation.

**Examples:**

```bash
# Auto-detect best mode (Recommended)
cast -c data.csv archive.cast -v

# Force High-Performance 7-Zip mode
cast -c data.csv archive.cast --mode 7zip -v

# Force Standalone Native mode (Multi-threaded)
cast -c data.csv archive.cast --mode native --multithread

# Low RAM Environment (Chunked processing)
cast -c huge.csv archive.cast --chunk-size 500MB

# Low RAM Environment and Custom Dictionary size
cast -c huge.csv archive.cast --chunk-size 500MB --dict-size 64MB
```

### 2. Decompression
Automatically detects the format. You can use `--mode` to force a specific backend, though the default is usually optimal.

> **ℹ️ Note on Defaults:** When using `auto` (default), the engine prefers **Native Mode** for decompression as benchmarks show it provides lower latency and superior throughput for most datasets.

```bash
# Auto-detect (Recommended - Defaults to Native)
cast -d archive.cast restored.csv

# Force 7-Zip backend (Alternative)
cast -d archive.cast restored.csv --mode 7zip
```

### 3. Verification (Standalone)
Validates archive integrity (CRC32 & Structure) via a full in-memory streaming check, ensuring the data is recoverable without extracting files to disk.

```bash
# Auto-detect
cast -v archive.cast

# Force 7-Zip backend
cast -v archive.cast --mode 7zip
```

> **🛡️ Safe Streaming Restoration:**
> The decompressor utilizes **Buffered Streaming I/O**. This means memory usage remains bounded by the **Chunk Size** used during compression. If the file was compressed with chunks (e.g., `--chunk-size 100MB`), you can restore multi-gigabyte archives on low-RAM machines without crashing.

---

## ⚙️ Configuration (7-Zip Path)

CAST automatically searches for the 7-Zip executable in standard system locations (e.g., `C:\Program Files\7-Zip`, `/usr/bin`, `/opt/homebrew/bin`).

However, if you have installed 7-Zip in a non-standard location or want to force a specific version, you can set the `SEVEN_ZIP_PATH` environment variable:

* **Windows (PowerShell):**
  ```powershell
  $env:SEVEN_ZIP_PATH = "C:\MyTools\7z.exe"
  ```
* **Windows (CMD):**
  ```cmd
  set SEVEN_ZIP_PATH="C:\MyTools\7z.exe"
  ```
* **Linux/macOS (Bash/Zsh):**
  ```bash
  export SEVEN_ZIP_PATH="/usr/local/bin/7zz"
  ```

---

## 📊 Benchmark Suite

The benchmarking tool compares CAST against industry standards (**LZMA2**, **Brotli**, **Zstd**).

**Important:** The `--mode` flag determines not only how CAST runs, but also how the **LZMA2 competitor** runs, ensuring a fair comparison.

**Syntax:**
```bash
run_benchmarks --list <file_list.txt> --compare-with <algos> [options]
```

**Parameters:**
* `--list <path>`: Text file with list of files to test (one per line).
* `--compare-with <algos>`: `lzma2`, `brotli`, `zstd`, or `all`.
* `--mode <native|7zip>`: Backend selection (Default: auto).
* `--dict-size <SIZE>`: Sets LZMA Dictionary Size (e.g., 64MB, 256MB). Default: 128MB.
* `--multithread`: Enables threading for CAST (Native) and competitors.
* `--chunk-size <SIZE>`: Forces chunked processing for all algorithms.

**Examples:**

```bash
# Full Performance Comparison (using 7-Zip if available)
run_benchmarks --list files.txt --compare-with all --mode 7zip

# Native Standalone Comparison with larger dictionary
run_benchmarks --list files.txt --compare-with zstd --mode native --multithread --dict-size 256MB
```

---

## 🛠️ Build from Source (Developers Only)

If you want to modify the code or compile it yourself, follow these steps.

### Prerequisites
* **Rust & Cargo:** Install via [rustup.rs](https://rustup.rs/).
* **Linux Dependencies:** `sudo apt install build-essential liblzma-dev pkg-config`

### Compilation Command
This command generates the optimized executables in the `target/release/` folder.

```bash
cargo build --release
```

*To reproduce the static builds distributed in Releases, specific targets (like `x86_64-unknown-linux-musl` or `crt-static` on Windows) are used.*
