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
* **`cli.py`**: A simple Command Line Interface to compress and decompress files.
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

The `cli.py` script allows you to test the algorithm on single files.

### Syntax
```bash
python cli.py <mode> <input_file> <output_file>
