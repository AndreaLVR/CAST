# CAST: Random Access Preview (Experimental WIP)

This directory contains a **wip experimental evolution** of the CAST algorithm that introduces **Random Access** capabilities via Row Groups and Indexing, similar to formats like Apache Parquet.

> **Note:** The stable implementation described in the current paper is located in the `../rust_impl` directory. Use this version only if you want to try granular access to data without full decompression.

## 🚀 Key Features

* **Indexed Stream:** The file is split into independent chunks (Row Groups).
* **Partial Decompression:** Extract specific rows (e.g., rows 25,000-26,000) instantly without processing the whole file.
* **Binary Guard:** Automatic handling of binary/mixed content (fallback to passthrough mode).

## 📊 Performance Trade-offs (Preliminary)

Compared to the standard "Solid" CAST implementation:

* **Compression Ratio:** Slight decrease (**~5% larger files**) due to independent dictionary resets for each chunk.
* **Compression Speed:** Identical.
* **Decompression Speed:** Slightly faster (**~5% faster**) on full files due to improved I/O streaming buffering.
* **Random Access:** **O(1) complexity**. Seeking and extracting a small range is instantaneous, regardless of file size.

## 🛠 Usage

Build the preview version:

```bash
cargo build --release
```

### Compress with Indexing
Use `--chunk-size` to define the granularity (e.g., 64MB blocks).

```bash
# Creates an index entry roughly every 64MB of input data
./target/release/cast_ra_preview -c data.log archive.cast --chunk-size 64MB
```

### Random Access (The Magic)
Extract specific rows using 1-based indexing (like typical text editors).

```bash
# Instantly extracts rows 25,000 to 26,000
./target/release/cast_ra_preview -d archive.cast extract.txt --rows 25000-26000
```

---
*Status: Work in Progress / Feature Preview.*
