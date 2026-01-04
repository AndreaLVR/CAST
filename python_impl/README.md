# CAST: Python Reference Implementations

> ⚠️ **Educational Purpose Only**
>
> These implementations are designed as **Readable References** to understand the CAST algorithm's logic, data structures (`Skeleton` + `Variables`), and serialization format.
>
> While less performant than the [Rust Implementation](../rust_impl), the Python version is **feature-complete**:
> * **Chunking Support:**
>   * **CLI Tool:** Implements **true streaming** (low memory footprint), enabling processing of files larger than physical RAM.
>   * **Benchmark Tool:** Uses **simulated chunking** (loads full file) to measure pure algorithmic efficiency while maintaining a fair comparison environment against memory-bound competitors.
> * **Dual Mode:** Offers both a pure-Python implementation and a high-performance wrapper around 7-Zip.

## 📂 Project Structure

To mirror the Rust architecture, the Python implementation is split into two variants:

### 1. [System Mode (7z Backend)](./7z_support)
> **Path:** `./7z_support/`

This version implements the CAST logic in Python but delegates the heavy compression work to an external **7-Zip executable** via pipes.
* **Pros:** Significantly faster (multithreaded backend), better compression ratios (uses LZMA2 Ultra).
* **Cons:** Requires `7z` installed on the system.
* **Best for:** Rapid prototyping, testing on large files where pure Python is too slow.

### 2. [Native Mode (Pure Python)](./native)
> **Path:** `./native/`

The strict reference implementation using only Python's standard library modules (`lzma`, `re`, `struct`).
* **Pros:** No external dependencies (works out-of-the-box).
* **Cons:** Slower (Single-threaded), limited by the Python Global Interpreter Lock (GIL).
* **Best for:** Studying the algorithm, debugging, ensuring bit-perfect reproducibility without external factors.

---

## ⚡ Quick Comparison

| Feature | System Mode (`./7z_support`) | Native Mode (`./native`) |
| :--- | :---: | :---: |
| **Backend Engine** | External `7z` (C++) | Python `lzma` module |
| **Multithreading** | ✅ Yes (via backend) | ❌ No (Single Core) |
| **Chunking Support** | ✅ Yes (True Streaming) | ✅ Yes (True Streaming) |
| **Compression Speed** | ⭐⭐⭐ | ⭐ |
| **Dependencies** | Python 3 + `7z` | Python 3 Only |

---

## 🚀 How to Start

Navigate to the variant of your choice to run the CLI or Benchmark tools.

### 1. Install Dependencies (Global)
Both versions require the same dependencies for benchmarking (Zstd/Brotli):
```bash
pip install -r requirements.txt
```

### 2. Run the Tool
The syntax is identical for both variants.

#### CLI Usage Examples (Real-World Use)
```bash
# Standard Compression (Solid Mode): Loads full file into RAM for max Ratio. Includes verification (-v).
python cli.py -c "large_dataset.csv" "archive.cast" -v

# Chunked Compression (Stream Mode): Processes 100MB at a time. Ideal for files > RAM.
python cli.py -c "large_dataset.csv" "archive.cast" --chunk-size 100MB -v

# Decompression: Automatically detects mode.
python cli.py -d "archive.cast" "restored.csv"

# Integrity Check (Standalone): Verifies structure and CRC32 without extracting.
python cli.py -v "archive.cast"
```

#### Benchmark Usage Examples (Research)
```bash
# Benchmark All: Compares CAST vs LZMA/Zstd/Brotli on a file list.
python run_benchmarks.py --list files.txt --all

# Benchmark Specific: Compares only against Brotli and Zstd.
python run_benchmarks.py --list files.txt --brotli --zstd
```

---

## 🧠 Implementation Details

### Why use Python?
* **Readability:** The distinction between `Skeleton` (structure) and `Variables` (data) is explicit and easy to inspect in the `CASTCompressor` class.
* **Prototyping:** It serves as the "Gold Standard" logic verification for the faster Rust port.

### Parsing Strategy: Regex vs. Manual
Unlike the Rust implementation, which uses a custom zero-copy byte parser for maximum speed, this Python version uses the standard `re` (Regex) module.
* **Reasoning:** In Python, a manual character-by-character loop is prohibitively slow due to interpreter overhead. The `re` module is implemented in C, making it significantly faster than any manual Python loop while keeping the code simple and readable.
