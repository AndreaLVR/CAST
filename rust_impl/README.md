# CAST: Rust Implementations

> ⚠️ **Disclaimer: Proof of Concept & Performance Focus**
>
> Please note that both implementations provided here are intended as **Proof of Concepts (PoC)**. Neither version is designed for critical production environments.
>
> **💡 Key Performance Metric:** Regardless of the implementation chosen (7z or Native), the critical metric to observe is the **Time-to-Compression-Ratio balance**.
> CAST aims for a unique "sweet spot": it often achieves **LZMA-like ratios in significantly less time**, or outperforms faster algorithms (like Zstd) in ratio while maintaining acceptable performance.
>
> **The goal is to demonstrate a superior trade-off compared to standard algorithms, rather than just winning on a single metric.**

This directory contains the high-performance Rust ports of the CAST (Columnar Agnostic Structural Transformation) algorithm.

To serve different deployment needs, the implementation is split into two distinct variants. Please choose the one that best fits your environment.

---

## 📂 Available Variants

### 1. [7z Backend Support](./7z_support) (Recommended)
> **Path:** `./7z_support/`

This version acts as a smart wrapper around the external **7-Zip executable**.
* **Pros:** Great compression performance (slightly worse than the native version), utilizes 7-Zip's highly optimized multi-threading, **extremely faster than the native version**.
* **Cons:** Requires 7-Zip to be installed on the host machine and accessible via PATH or environment variable (explained in 7z_support/README.md).
* **Best for:** Benchmarking, local heavy-duty compression, environments where installing 7z is allowed.

### 2. [Native Implementation](./native)
> **Path:** `./native/`

This version uses Rust crates (`xz2`, `lzma-rs`, etc.) to handle compression internally without calling external processes.
* **Pros:** Completely self-contained binary (no external dependencies required at runtime), cleaner distribution.
* **Cons:** Slightly better compression ratio but lower speed compared to the optimized 7z CLI; build process requires standard C build tools.
* **Best for:** Standalone tools, distribution to end-users, environments where external binaries cannot be called.

---

## ⚡ Quick Comparison

| Feature | 7z Support (`./7z_support`) | Native (`./native`) |
| :--- | :---: | :---: |
| **Runtime Dependency** | Requires `7z` executable | None (Standalone) |
| **Compressio Ratio** | ⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ |
| **Performance** | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐ |
| **Multi-threading** | Managed by 7z (Auto) | Managed by Rust |
| **Build Complexity** | Very Low | Low/Medium |

---

## 🚀 How to Start

Navigate to the folder of your choice to see specific build and usage instructions:

```bash
# For the 7z-based version
cd 7z_support

# For the Native version
cd native
