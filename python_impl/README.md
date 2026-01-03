# CAST: Python Reference Implementation

> ⚠️ **Educational Purpose Only**
>
> This implementation is designed as a **Readable Reference** to understand the CAST algorithm's logic, data structures (`Skeleton` + `Variables`), and serialization format.
>
> **It is NOT intended for production or performance profiling.**
> * **Single-Threaded:** It runs on a single core.
> * **Memory Bound:** It loads the entire dataset into memory (Monolithic processing). It **does not support chunking**. Do not attempt to process files larger than your available physical RAM.
> * **Regex-Based:** It uses standard Python `re` module, which is significantly slower than the custom byte-level parser used in the Rust implementation.

## 📂 Project Structure

* **`cast.py`**: The core library containing the `CAST` class, the `Skeleton` logic, and the standard Regex patterns used for parsing.
* **`cli.py`**: A simple Command Line Interface to compress, decompress, and verify files.
* **`run_benchmarks.py`**: A script to validate the compression ratio against Python's native `lzma`, `zstd`, and `brotli` libraries.
* **`requirements.txt`**: List of dependencies (mainly for benchmarking).

---

## ⚙️ Setup

Ensure you have **Python 3.10+** installed.

1.  **Install Dependencies:**
    ```bash
    pip install -r requirements.txt
    ```

---

## 📦 Usage: CLI Tool

The `cli.py` script allows you to test the algorithm on single files manually.

### Syntax
```bash
python cli.py <mode> <input_file> <output_file> [options]
```

### Modes
* `-c` : **Compress**. Reads input, compresses to `.cast`, and optionally verifies.
* `-d` : **Decompress**. Restores the original file.
* `-v` : **Verify Only**. Checks integrity without writing to disk.

### Examples

**1. Compress a file (Standard):**
```bash
python cli.py -c data.csv archive.cast
```

**2. Compress with Immediate Verification (Recommended):**
```bash
python cli.py -c data.csv archive.cast -v
```

**3. Decompress and restore:**
```bash
python cli.py -d archive.cast restored.csv
```

**4. Check archive integrity (No extraction):**
```bash
python cli.py -v archive.cast
```

---

## 📊 Usage: Benchmarks

The `run_benchmarks.py` script is used to calculate the **Theoretical Maximum Compression Ratio** (Table 1 in the paper).
It compares CAST against standard libraries using their maximum compression settings (LZMA Extreme, Zstd 22, Brotli 11).

### Syntax
```bash
python run_benchmarks.py [--list LIST | --file FILE] [competitors]
```

### Input Arguments (Mutually Exclusive)
* `--file <path>`: Test a single specific file.
* `--list <path>`: Path to a text file containing a list of file paths (one per line).

### Competitor Flags
* `--lzma`: Compare against LZMA2 (XZ).
* `--zstd`: Compare against Zstandard.
* `--brotli`: Compare against Brotli.
* `--all`: Compare against ALL supported algorithms.

### Examples

**1. Benchmark a single file against everything:**
```bash
python run_benchmarks.py --file "C:\Data\dataset.csv" --all
```

**2. Benchmark a list of files against LZMA only:**
```bash
python run_benchmarks.py --list files.txt --lzma
```

**3. Benchmark against Zstandard and Brotli:**
```bash
python run_benchmarks.py --file logs.txt --zstd --brotli
```

### Example `files.txt` format
```text
C:\Datasets\data.csv
/home/user/logs/server.log
# Lines starting with # are ignored
```

---

## 🧠 Implementation Details

### Why is this slower than Rust?
1.  **Interpreter Overhead:** Python is an interpreted language.
2.  **Regex Engine:** This version uses `re.match()` for every line. While flexible and easy to read, it introduces significant CPU overhead compared to the Rust version's zero-allocation byte scanner.
3.  **Global Object Overhead:** Every `Skeleton` and `Variable` is a full Python object, incurring memory overhead.

### When to use this version?
* You want to read the code to understand *how* the Skeleton/Variable separation works.
* You want to debug a specific parsing edge case.
* You are verifying the bit-perfect reproducibility of the algorithm logic on small files.

**For all other use cases, please use the [Rust Implementation](../rust_impl).**
