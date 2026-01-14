# CAST: Random Access Preview (Experimental WIP)

This directory contains a **Work-In-Progress (WIP) experimental evolution** of the CAST algorithm. It introduces **Random Access** capabilities via Row Groups and Footer Indexing, moving CAST from a pure archival format (like `.tar.gz`) towards a query-ready storage format (like Apache Parquet).

> **⚠️ Note:** The stable implementation described in the current paper is located in the `../rust_impl` directory. Use this version only if you want to try granular access to data without full decompression.
>
> **Status:** Experimental. Internal structures and API might change. Comprehensive benchmarks and a formal paper update will follow once the format stabilizes.

---

## 🏗️ Architecture: How it Works

Unlike the standard CAST implementation which compresses data as a single continuous stream ("Solid Mode") to maximize compression ratio, this version adopts a **Block-Based Architecture**.

### 1. Smart Chunking Strategy
Instead of blindly cutting files at fixed byte offsets (which would corrupt row structures), CAST RA uses a **Sampling Heuristic**:
1.  Reads the first **1,000 rows** of the input file.
2.  Calculates the **Average Row Size** (in bytes).
3.  Computes the target number of rows to fit the user-requested chunk size (e.g., 64MB).
4.  The stream is then processed and "flushed" every $N$ rows, ensuring cleanly separated blocks.

### 2. Independent Row Groups
Each chunk (or **Row Group**) is a fully self-contained CAST archive:
* It has its own **Dictionary** (the compressor state is reset for each block).
* It contains its own locally optimized **Registry (Templates)** and **Variables**.
* **Trade-off:** This independence allows random access but slightly reduces compression efficiency (~5-10% depending on the case) because patterns cannot be referenced across block boundaries.

### 3. The Footer Index
At the end of the file, CAST appends a **Metadata Footer** containing:
* **Start Offset** (byte position) of each block.
* **Row Count** for each block.
* **Compressed Size** of each block.

When you request a specific row range (e.g., `--rows 25000-26000`), the decompressor reads the footer, calculates exactly which block contains those rows, seeks directly to that offset, and decompresses **only that block**.

---

## 🚀 Key Features

* **Indexed Stream:** The file is split into independent chunks (Row Groups).
* **Partial Decompression:** Extract specific rows (e.g., rows 25,000-26,000) instantly without processing the whole file.
* **Binary Guard:** Automatic handling of binary/mixed content (fallback to passthrough mode) per-chunk.

## 📊 Performance Trade-offs (Preliminary)

Compared to the standard "Solid" CAST implementation:

* **Compression Ratio:** Slight decrease (**~5-10% larger files**) due to independent dictionary resets for each chunk.
* **Compression Speed:** Identical. The overhead of flushing blocks is negligible.
* **Decompression Speed:** Slightly faster (**~10-20% faster**) on full files due to improved I/O streaming buffering.
* **Random Access:** **O(1) complexity**. Seeking and extracting a small range is instantaneous (milliseconds), regardless of total file size (GBs or TBs).

---

## 🛠 Usage

Build the preview version:

```bash
cargo build --release
```

### Compress with Indexing
Use `--chunk-size` to define the granularity. A size of **64MB** or **128MB** is recommended for a good balance between seek speed and compression ratio.

```bash
# Creates an index entry roughly every 64MB of input data
./target/release/cast_ra_preview -c data.log archive.cast --chunk-size 64MB
```

### Random Access (The Magic)
Extract specific rows using human-readable **1-based indexing** (like typical text editors). CAST handles the offset calculation internally.

```bash
# Instantly extracts rows 25,000 to 26,000
./target/release/cast_ra_preview -d archive.cast extract.txt --rows 25000-26000
```

---
*Status: Work in Progress / Feature Preview.*
