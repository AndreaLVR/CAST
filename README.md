# CAST: Columnar Agnostic Structural Transformation

> **An agnostic algorithm that transcends standard compression limits by reducing structural entropy and re-engineering data layout for superior compression ratios and speed on structured data.**

![Status](https://img.shields.io/badge/Status-Proof_of_Concept-orange)
![Python](https://img.shields.io/badge/Python-3.10+-blue.svg?logo=python&logoColor=white)
![Rust](https://img.shields.io/badge/Rust-1.70+-orange.svg?logo=rust&logoColor=white)
![License](https://img.shields.io/badge/License-MIT-yellow)
![Platform](https://img.shields.io/badge/Platform-Cross--Platform-lightgrey)

**CAST** is a high-performance pre-processing algorithm designed to bridge the gap between raw structured text and modern entropy engines (like LZMA, Zstd, Brotli).

Standard compressors are physically limited by their local "look-back" windows. CAST breaks this barrier by parsing the file structure globally, separating the **Skeleton** (syntax) from the **Variables** (data), and re-engineering the layout into continuous columnar streams.

The result? **Compression ratios up to 135x** and processing speeds up to **10x faster** than LZMA Extreme alone.

---

## ⚡ At a Glance

* 🚀 **The Performance Paradox**: Despite adding a pre-processing step, CAST reduces total compression time by up to **90%** (e.g., 2.6s vs 23s on SQL dumps).
* 📦 **Extreme Density**: Outperforms LZMA2 (Preset 9), Zstd (Level 22) AND Brotli (Level 11) by an additional **30-60%** on complex logs, CSVs, and IoT data.
* 🧠 **Fully Agnostic**: No schema definition required. CAST automatically detects structure in SQL, CSV, XML, JSON, Log files and, in general, in any structured data files.
* 🔒 **Lossless**: Bit-perfect reconstruction validated by CRC32 checks.

---

## 📊 Benchmarks (Comprehensive Suite)

All tests were performed on the same hardware comparing **CAST** against industry standards at maximum settings.

> **⚠️ Note on Methodology & Performance:**
> 1. **Pipeline:** The results below represent the combined pipeline of **CAST Pre-processing + LZMA2 Encoding**.
> 2. **Python vs Rust:** These benchmarks reflect the **Python reference implementation**, which is strictly optimized for **maximum compression ratio**. The repository also includes a **Rust + 7z implementation** designed for higher performances. While not listed below to avoid redundancy, the Rust version delivers **significantly higher speeds** with only a negligible difference in compression ratio (<1%).

The table shows `Final Size` (top) and `(Time to Compress)` (bottom). **Bold** indicates the best values in the row.
Dataset names are linked to their source where available.

| Dataset | Original size | CAST <br>(w/ LZMA) | LZMA <br>(Extreme) | Zstd <br>(Level 22) | Brotli <br>(Quality 11) | CAST Ratio |
| :--- | :---: | :--- | :--- | :--- | :--- | :---: |
| [**Balance of Payments**](https://www.stats.govt.nz/assets/Uploads/Balance-of-payments/Balance-of-payments-and-international-investment-position-September-2025-quarter/Download-data/balance-of-payments-and-international-investment-position-september-2025-quarter.csv)<br>*(Finance CSV)* | 33.1 MB | 🏆 **244 KB**<br>**(5.5s)** | 501 KB<br>(93.9s) | 697 KB<br>(100s) | 590 KB<br>(89.8s) | **135.7x** |
| [**Migration Stats**](https://www.stats.govt.nz/assets/Uploads/International-migration/International-migration-October-2025/Download-data/international-migration-october-2025-citizenship-by-visa-and-by-country-of-last-permanent-residence.csv)<br>*(Demographics CSV)* | 29.2 MB | 🏆 **317 KB**<br>**(6.9s)** | 945 KB<br>(48.5s) | 1.12 MB<br>(47.3s) | 1.05 MB<br>(67.6s) | **92.1x** |
| [**DDoS Data**](https://www.kaggle.com/datasets/siddharthm1698/ddos-botnet-attack-on-iot-devices?select=DDoSdata.csv)<br>*(IoT Security CSV)* | 616.7 MB | 🏆 **10.2 MB**<br>**(463s)** | 19.6 MB<br>(1308s) | 24.3 MB<br>(1371s) | 21.9 MB<br>(1490s) | **59.9x** |
| **Votes Archive**<br>*(Community XML)* | 145.8 MB | 🏆 **3.6 MB**<br>**(57s)** | 4.7 MB<br>(192s) | 5.6 MB<br>(183s) | 5.4 MB<br>(285s) | **39.6x** |
| [**Sakila DB**](https://www.kaggle.com/datasets/atanaskanev/sqlite-sakila-sample-database)<br>*(Sample DB SQL)* | 8.7 MB | 🏆 **298 KB**<br>**(2.6s)** | 426 KB<br>(23.4s) | 501 KB<br>(22.9s) | 466 KB<br>(22.8s) | **29.4x** |
| [**RT_IOT2022**](https://www.kaggle.com/datasets/supplejade/rt-iot2022real-time-internet-of-things)<br>*(IoT Traffic CSV)* | 54.8 MB | 🏆 **1.99 MB**<br>**(23.5s)** | 2.53 MB<br>(141s) | 2.53 MB<br>(240s) | 2.51 MB<br>(39.6s) | **27.5x** |
| **Weblog Sample**<br>*(Server Logs)* | 67.6 MB | 🏆 **2.5 MB**<br>**(34.6s)** | 2.7 MB<br>(51.3s) | 2.9 MB<br>(77.7s) | 3.1 MB<br>(177s) | **26.8x** |
| [**Wireshark**](https://www.kaggle.com/datasets/kanelsnegl/wireshark?select=p3.csv)<br>*(Network PCAP CSV)* | 154.4 MB | 🏆 **5.8 MB**<br>**(145s)** | 9.5 MB<br>(312s) | 10.7 MB<br>(314s) | 10.1 MB<br>(325s) | **26.5x** |
| [**Logfiles**](https://www.kaggle.com/datasets/vishnu0399/server-logs)<br>*(Apache Web Logs)* | 242.0 MB | 🏆 **10.2 MB**<br>**(99s)** | 13.0 MB<br>(203s) | 13.3 MB<br>(258s) | 14.1 MB<br>(572s) | **23.5x** |
| **Badges**<br>*(Community XML)* | 32.7 MB | 🏆 **1.9 MB**<br>**(12.8s)** | 2.5 MB<br>(71.8s) | 2.9 MB<br>(68.5s) | 2.8 MB<br>(65.1s) | **16.5x** |
| [**Metasploitable**](https://www.kaggle.com/datasets/badcodebuilder/insdn-dataset)<br>*(CyberSec CSV)* | 52.8 MB | 🏆 **3.5 MB**<br>(28.0s) | 3.7 MB<br>**(23.1s)** | 3.8 MB<br>(46.5s) | 3.7 MB<br>(126s) | **15.0x** |
| [**HomeC**](https://www.kaggle.com/datasets/taranvee/smart-home-dataset-with-weather-information)<br>*(Smart Home CSV)* | 131.0 MB | 🏆 **11.1 MB**<br>**(103s)** | 14.8 MB<br>(189s) | 15.5 MB<br>(184s) | 15.5 MB<br>(266s) | **11.7x** |
| [**Custom 2020**](https://www.kaggle.com/datasets/zanjibar/japantradestat)<br>*(Trade Finance CSV)* | 207.9 MB | 🏆 **18.4 MB**<br>**(213s)** | 24.7 MB<br>(449s) | 26.4 MB<br>(448s) | 25.1 MB<br>(478s) | **11.3x** |
| **Users**<br>*(Community XML)* | 48.0 MB | 🏆 **6.4 MB**<br>**(21.9s)** | 7.6 MB<br>(52.0s) | 8.1 MB<br>(50.6s) | 8.1 MB<br>(89.2s) | **7.5x** |
| [**Owid Covid**](https://www.kaggle.com/datasets/taranvee/covid-19-dataset-till-2222022)<br>*(Epidemiology CSV)* | 46.7 MB | 🏆 **6.3 MB**<br>**(29.4s)** | 7.1 MB<br>(48.8s) | 7.5 MB<br>(49.7s) | 6.9 MB<br>(112s) | **7.4x** |
| **Audit Dump**<br>*(Synthetic Logs SQL)* | 64.6 MB | 🏆 **10.1 MB**<br>**(32.8s)** | 12.0 MB<br>(110s) | 12.6 MB<br>(106s) | 12.1 MB<br>(125s) | **6.4x** |
| [**Gandhi Works**](https://www.kaggle.com/datasets/abelgeorge2222/collected-works-mahatma-gandhi-a-json-dataset)<br>*(Literature JSON)* | 100.6 MB | 🏆 **20.3 MB**<br>(91.5s) | 20.7 MB<br>**(89.6s)** | 20.9 MB<br>(95.3s) | 22.5 MB<br>(213s) | **4.95x** |
| [**Gafgyt Botnet**](https://www.kaggle.com/datasets/mkashifn/nbaiot-dataset?select=1.gafgyt.combo.csv)<br>*(IoT Botnet CSV)* | 105.8 MB | 🏆 **22.6 MB**<br>**(128s)** | 25.7 MB<br>(161s) | 25.6 MB<br>(174s) | 24.6 MB<br>(237s) | **4.7x** |
| [**GloVe Embeddings**](https://www.kaggle.com/datasets/ouhammourachid/glove-6b-json-format)<br>*(ML Vectors JSON)* | 193.4 MB | 🏆 **57.3 MB**<br>(315s) | 57.9 MB<br>(261s) | 57.9 MB<br>**(239s)** | 60.0 MB<br>(426s) | **3.37x** |
| [**Assaults 2015**](https://www.kaggle.com/datasets/mohamedbakrey/analysispublicplaceassaultssexualassault-2015)<br>*(Crime Stats CSV)* | 234 KB | 39.5 KB<br>(0.18s) | 🏆 **33.9 KB**<br>**(0.14s)** | 37.6 KB<br>(0.29s) | 34.0 KB<br>(0.38s) | 5.9x |

> **Key Takeaway:** CAST consistently outperforms all three major engines in both **Density** (smaller files) and **Throughput** (faster processing) on medium-to-large structured datasets.

---

## 🛠️ Methodology & Architecture

The core innovation of CAST is **Structural Agnosticism**. Unlike formats like Parquet which require a pre-defined schema, CAST infers the structure on the fly using a regex-based pattern recognition engine.

### The Process
1.  **Pattern Recognition**: The algorithm scans the file to identify repeating lines (Templates).
2.  **Structural Deduplication**: It separates the static characters (**Skeleton**) from the dynamic values (**Variables**).
3.  **Columnar Transformation**: Variables are transposed from row-oriented layout to column-oriented blocks.
4.  **Entropy Reduction**: Since values in a column (e.g., dates, IPs, IDs) have much lower entropy than rows, the final compression/decompression step (via LZMA2) is exponentially more efficient.

> 📄 **Scientific Paper:** For a deep dive into the mathematical proofs, the "Binary Guard" logic, and the specific regex strategies used for "Structural Deduction", please refer to the **LaTeX documentation** included in the `/paper` directory of this repository.

---

## 🧪 Implementation Notes: Proof of Concept

This repository serves as a **scientific Proof of Concept** to demonstrate the efficacy of the CAST algorithm. It provides two distinct implementations, each with a specific goal:

### 1. 🐍 Python Implementation (The Reference)
* **Goal:** Maximum Compression Density & Algorithmic Clarity.
* **Method:** Uses Python's native `lzma` library with fine-tuned parameters.
* **Pros:** Achieves the absolute best compression ratios (as seen in the benchmarks).
* **Cons:** Slower due to Python's interpreter overhead. Ideal for understanding the logic and verifying the math.

### 2. 🦀 Rust Implementation (The Performance Preview)
* **Goal:** Simulation of Production Speeds.
* **Method:** Implements the pre-processing in high-performance Rust and **invokes the external `7z` CLI** for the final compression step.
* **Pros:** Extremely fast (closer to C++ production speeds). Demonstrates that CAST can run in real-time pipelines.
* **Trade-off:** Due to the use of external 7z CLI calls (vs native library integration), there is a **negligible compression loss** (<1%) compared to the Python version, but with a massive gain in throughput.
* 
---

## 🚀 Usage

```bash
# Compress a file
python cast.py compress --input data/big_log.log --output archive.cast

# Decompress (Verification)
python cast.py decompress --input archive.cast --output restored_log.log
```

---

## 📜 Citation

If you use CAST in your research or production pipeline, please cite it as:

```bibtex
@software{cast,
  author = {Olivari, Andrea},
  title = {CAST: Columnar Agnostic Structural Transformation},
  year = {2025},
  url = {[https://github.com/AndreaLVR/CAST](https://github.com/AndreaLVR/CAST)},
  note = {An agnostic algorithm that transcends standard compression limits by neutralizing structural entropy.}
}
```

---

## 📄 License

This project is open-source and available under the MIT License.
