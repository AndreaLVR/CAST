# CAST: Native Python Implementation

This directory contains the **Pure Python** reference implementation of the CAST algorithm.
Unlike the `7z_support` variant, this version **does not** require any external executables. It relies strictly on Python's standard library.

## 🎯 Design Philosophy

1.  **Portability:** Runs anywhere Python 3.10+ is installed (Windows, Linux, macOS) without configuration.
2.  **Zero External Dependencies:** Uses `import lzma` (liblzma binding) for the backend compression.
3.  **Algorithmic Baseline:** Serves as the strict logical reference for how CAST transforms data (Skeletons/Variables) before the final LZMA pass.

## ⚙️ Technical Characteristics

* **Parsing:** Uses Python's `re` module (compiled C Regex) for tokenization. This is chosen over manual loops because Python's interpreter overhead makes character-by-character parsing prohibitively slow.
* **Compression:** Uses the standard `lzma.compress()` function.
    * *Note:* Python's `lzma` module does not support multithreading for single-stream compression. As a result, this implementation is **single-threaded** and significantly slower than the Rust or 7z-based versions.
* **Memory:**
    * **Solid Mode:** Loads the entire file into RAM.
    * **Chunked Mode:** Stream-processing available via CLI options.

## 📂 Files

* **`cast.py`**: The pure Python implementation. It contains the core `CASTCompressor` class using native libraries.
* **`cli.py`**: Command-line tool for compressing/decompressing/verifying.
* **`run_benchmarks.py`**: Tool to compare CAST efficiency against LZMA2/Zstd/Brotli.

## 🚀 Usage

### 1. CLI Tool (Compression & Decompression)

```bash
# Solid Compression (Best Ratio, High RAM)
python cli.py -c input.csv output.cast -v

# Chunked Compression (Low RAM, slightly lower Ratio)
python cli.py -c input.csv output.cast --chunk-size 100MB -v

# Decompression
python cli.py -d output.cast restored.csv
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
```

---

## ⚠️ Performance Notice

This implementation is CPU-bound by the Python Global Interpreter Lock (GIL).
For large datasets (>1GB) or production environments, please use:
1.  The **Rust Implementation** (Recommended).
2.  The **7z_support** Python variant (if Rust is not an option).
