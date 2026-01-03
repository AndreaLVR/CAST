# CAST: Rust Implementations

> ⚠️ **Disclaimer: Proof of Concept & Performance Focus**
>
> Please note that both implementations provided here are intended as **Scientific Proof of Concepts (PoC)**. Neither version is fully hardened for critical production environments.
>
> **💡 Key Performance Metric:** Regardless of the implementation chosen (System or Native), the critical metric to observe is the **Simultaneous Enhancement**.
>
> On structured datasets CAST often breaks the traditional compression trade-off by delivering a **Dual Advantage**:
> 1.  **Superior Density:** It often produces smaller files than standard LZMA2.
> 2.  **Faster Encoding:** It significantly reduces processing time by simplifying the data stream *before* the backend encoder sees it.
>
> **The goal is to demonstrate that structural pre-processing can improve both speed and ratio simultaneously, rather than sacrificing one for the other.**

This directory contains the high-performance Rust ports of the CAST algorithm.

To serve different deployment needs, the implementation is split into two distinct variants. Please choose the one that best fits your environment.

---

## 📂 Available Variants

### 1. [System Mode (7-Zip Backend)](./7z_support)
> **Path:** `./7z_support/`
> **Recommended for:** Benchmarking, High Throughput, Heavy Workloads.

This version acts as a smart wrapper (pipe) around the external **7-Zip executable**.
* **Pros:** **Significantly faster** than the native version. It leverages the highly optimized, multi-threaded C++ engine of 7-Zip/LZMA2.
* **Cons:** Slightly higher overhead for very small files due to process spawning; requires `7z` to be installed on the host machine.
* **Trade-off:** Prioritizes **speed** over absolute minimal file size (due to 7z container framing).

### 2. [Native Mode (Standalone)](./native)
> **Path:** `./native/`
> **Recommended for:** Distribution, Maximum Compression Density.

This version uses Rust crates (`xz2`, `lzma-rs`, etc.) to handle compression internally without calling external processes.
* **Pros:** **Completely self-contained binary**. No external dependencies required at runtime; cleaner distribution.
* **Cons:** Slower than the external 7-Zip engine; build process requires standard C build tools (links against `liblzma`).
* **Trade-off:** Prioritizes **maximum compression ratio** (Algorithmic Efficiency) over raw throughput.

---

## ⚡ Quick Comparison

| Feature | System Mode (`./7z_support`) | Native Mode (`./native`) |
| :--- | :---: | :---: |
| **Runtime Dependency** | Requires `7-Zip` executable | None (Standalone) |
| **Compression Ratio** | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ |
| **Throughput (Speed)** | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐ |
| **Multi-threading** | Managed by 7-Zip (Optimized) | Managed by Rust |
| **Build Complexity** | Very Low | Low/Medium |

---

## 🚀 How to Start

Navigate to the folder of your choice to see specific build and usage instructions:

```bash
# For the High-Throughput version
cd 7z_support

# For the Standalone version
cd native
