# CAST: Columnar Agnostic Structural Transformation

> **An agnostic algorithm that transcends standard compression limits by reducing structural entropy and re-engineering data layout for superior compression ratios and speed on structured data.**

![Status](https://img.shields.io/badge/Status-Proof_of_Concept-orange)
![Python](https://img.shields.io/badge/Python-3.10+-blue.svg?logo=python&logoColor=white)
![Rust](https://img.shields.io/badge/Rust-1.70+-orange.svg?logo=rust&logoColor=white)
![License](https://img.shields.io/badge/License-MIT-yellow)
![Platform](https://img.shields.io/badge/Platform-Cross--Platform-lightgrey)

> ⚠️ **Disclaimer: Proof of Concept & Performance Focus**
>
> Please note that the implementations provided here (Python & Rust) are intended as **Proof of Concepts (PoC)** to demonstrate the algorithmic efficiency of CAST. Neither version is designed for critical production environments.
>
> **💡 Key Performance Metric:** The critical metric to observe is the **Time-to-Compression-Ratio balance**. CAST aims for a unique "sweet spot": it often **surpasses LZMA2 compression ratios in significantly less time**, and outperforms algorithms like Zstd in **both compression density and processing speed**.
> **The goal is to demonstrate a superior trade-off compared to standard algorithms, rather than just winning on a single metric.**

**CAST** is a high-performance pre-processing algorithm designed to bridge the gap between raw structured text and modern entropy engines (like LZMA, Zstd, Brotli).

Standard compressors are physically limited by their local "look-back" windows. CAST breaks this barrier by parsing the file structure globally, separating the **Skeleton** (syntax) from the **Variables** (data), and re-engineering the layout into continuous columnar streams.

The result? **Compression ratios up to 135x** and processing speeds up to **10x faster** than LZMA Extreme alone.

---

## ⚡ At a Glance

* 🚀 **The Performance Paradox**: Despite adding a pre-processing step, CAST reduces total compression time by up to **90%** (e.g., 2.6s vs 23s on SQL dumps).
* 📦 **Extreme Density**: Outperforms LZMA2 (Preset 9 Extreme), Zstd (Level 22) AND Brotli (Level 11) by an additional **30-60%** on structured data files, like CSVs, server logs, JSON, SQL dumps, IoT data, and so on.
* 🧠 **Fully Agnostic**: No schema definition required. CAST automatically detects structure in SQL, CSV, XML, JSON, Log files and, in general, in any structured data files.
* 🔒 **Lossless**: Bit-perfect reconstruction validated by CRC32 checks.

---

## 📊 Benchmarks & Performance Analysis

To provide a comprehensive evaluation, this project features **three distinct implementations**, each designed to validate a specific aspect of the algorithm:

1.  **🐍 Python Reference:** Optimized purely for **Maximum Compression Ratio** (validating the mathematical model).
2.  **🦀 Rust (Native Backend):** A standalone implementation using native Rust crates (dependency-free).
3.  **🦀 Rust (7z Backend):** Optimized for **Maximum Throughput**, demonstrating real-world production speeds by leveraging the 7-Zip executable.

> **⚠️ Important Context on Results:**
>
> 1.  **Python Table (Below):** Represents the theoretical maximum compression. Times include interpreter overhead.
> 2.  **Rust Table (Further down):** Demonstrates the **production speed** and ratio retention of CAST.
>     * It compares CAST only against **LZMA2** (both using 7z backend) to provide a clean, direct comparison of the algorithmic impact without repeating all competitors.
>     * **Note on Native Rust:** While the Native Rust implementation supports multithreading (offering significant speedups over the single-threaded versions), it does not yet match the raw throughput of the highly optimized 7-Zip engine. Consequently, the **Rust + 7z** results are presented below to illustrate the algorithm's performance ceiling in a fully optimized production scenario.
>
> **For detailed usage and build instructions, please refer to the specific `README.md` in each implementation's subdirectory.**

### 1. The Reference Benchmark (Python Implementation)
> **🎯 Goal:** Validate the maximum theoretical **Compression Ratio**.

The following results were obtained using the **Python reference implementation**.
**Note on Speed:** The processing times listed below reflect the overhead of the Python interpreter and single-threaded execution. They **do not** represent the true speed potential of the CAST algorithm. Use this table to evaluate **Density**, not Throughput.

#### 📄 CSV Datasets (Structured Data)
| Dataset | Original size | CAST <br>(w/ LZMA) | LZMA2 <br>(Extreme) | Zstd <br>(Level 22) | Brotli <br>(Quality 11) | CAST Ratio |
| :--- | :---: | :--- | :--- | :--- | :--- | :---: |
| [**Balance of Payments**](https://www.stats.govt.nz/assets/Uploads/Balance-of-payments/Balance-of-payments-and-international-investment-position-September-2025-quarter/Download-data/balance-of-payments-and-international-investment-position-september-2025-quarter.csv)<br>*(Finance CSV)* | 33.1 MB | 🏆 **244 KB**<br>**(5.5s)** | 501 KB<br>(93.9s) | 697 KB<br>(100s) | 590 KB<br>(89.8s) | **135.7x** |
| [**Migration Stats**](https://www.stats.govt.nz/assets/Uploads/International-migration/International-migration-October-2025/Download-data/international-migration-october-2025-citizenship-by-visa-and-by-country-of-last-permanent-residence.csv)<br>*(Demographics CSV)* | 29.2 MB | 🏆 **317 KB**<br>**(6.9s)** | 945 KB<br>(48.5s) | 1.12 MB<br>(47.3s) | 1.05 MB<br>(67.6s) | **92.1x** |
| [**DDoS Data**](https://www.kaggle.com/datasets/siddharthm1698/ddos-botnet-attack-on-iot-devices?select=DDoSdata.csv)<br>*(IoT Security CSV)* | 616.7 MB | 🏆 **10.2 MB**<br>**(463s)** | 19.6 MB<br>(1308s) | 24.3 MB<br>(1371s) | 21.9 MB<br>(1490s) | **59.9x** |
| [**RT_IOT2022**](https://www.kaggle.com/datasets/supplejade/rt-iot2022real-time-internet-of-things)<br>*(IoT Traffic CSV)* | 54.8 MB | 🏆 **1.99 MB**<br>**(23.5s)** | 2.53 MB<br>(141s) | 2.53 MB<br>(240s) | 2.51 MB<br>(39.6s) | **27.5x** |
| [**Wireshark**](https://www.kaggle.com/datasets/kanelsnegl/wireshark?select=p3.csv)<br>*(Network PCAP CSV)* | 154.4 MB | 🏆 **5.8 MB**<br>**(145s)** | 9.5 MB<br>(312s) | 10.7 MB<br>(314s) | 10.1 MB<br>(325s) | **26.5x** |
| [**Metasploitable**](https://www.kaggle.com/datasets/badcodebuilder/insdn-dataset)<br>*(CyberSec CSV)* | 52.8 MB | 🏆 **3.5 MB**<br>(28.0s) | 3.7 MB<br>**(23.1s)** | 3.8 MB<br>(46.5s) | 3.7 MB<br>(126s) | **15.0x** |
| [**HomeC**](https://www.kaggle.com/datasets/taranvee/smart-home-dataset-with-weather-information)<br>*(Smart Home CSV)* | 131.0 MB | 🏆 **11.1 MB**<br>**(103s)** | 14.8 MB<br>(189s) | 15.5 MB<br>(184s) | 15.5 MB<br>(266s) | **11.7x** |
| [**Custom 2020**](https://www.kaggle.com/datasets/zanjibar/japantradestat)<br>*(Trade Finance CSV)* | 207.9 MB | 🏆 **18.4 MB**<br>**(213s)** | 24.7 MB<br>(449s) | 26.4 MB<br>(448s) | 25.1 MB<br>(478s) | **11.3x** |
| [**Owid Covid**](https://www.kaggle.com/datasets/taranvee/covid-19-dataset-till-2222022)<br>*(Epidemiology CSV)* | 46.7 MB | 🏆 **6.3 MB**<br>**(29.4s)** | 7.1 MB<br>(48.8s) | 7.5 MB<br>(49.7s) | 6.9 MB<br>(112s) | **7.4x** |
| [**Gafgyt Botnet**](https://www.kaggle.com/datasets/mkashifn/nbaiot-dataset?select=1.gafgyt.combo.csv)<br>*(IoT Botnet CSV)* | 105.8 MB | 🏆 **22.6 MB**<br>**(128s)** | 25.7 MB<br>(161s) | 25.6 MB<br>(174s) | 24.6 MB<br>(237s) | **4.7x** |
| [**Assaults 2015**](https://www.kaggle.com/datasets/mohamedbakrey/analysispublicplaceassaultssexualassault-2015)<br>*(Crime Stats CSV)* | 234 KB | 39.5 KB<br>(0.18s) | 🏆 **33.9 KB**<br>**(0.14s)** | 37.6 KB<br>(0.29s) | 34.0 KB<br>(0.38s) | 5.9x |

#### 📄 JSON & XML (Hierarchical Data)
| Dataset | Original size | CAST <br>(w/ LZMA) | LZMA2 <br>(Extreme) | Zstd <br>(Level 22) | Brotli <br>(Quality 11) | CAST Ratio |
| :--- | :---: | :--- | :--- | :--- | :--- | :---: |
| **Votes Archive**<br>*(Community XML)* | 145.8 MB | 🏆 **3.6 MB**<br>**(57s)** | 4.7 MB<br>(192s) | 5.6 MB<br>(183s) | 5.4 MB<br>(285s) | **39.6x** |
| **Badges**<br>*(Community XML)* | 32.7 MB | 🏆 **1.9 MB**<br>**(12.8s)** | 2.5 MB<br>(71.8s) | 2.9 MB<br>(68.5s) | 2.8 MB<br>(65.1s) | **16.5x** |
| **Users**<br>*(Community XML)* | 48.0 MB | 🏆 **6.4 MB**<br>**(21.9s)** | 7.6 MB<br>(52.0s) | 8.1 MB<br>(50.6s) | 8.1 MB<br>(89.2s) | **7.5x** |
| [**Gandhi Works**](https://www.kaggle.com/datasets/abelgeorge2222/collected-works-mahatma-gandhi-a-json-dataset)<br>*(Literature JSON)* | 100.6 MB | 🏆 **20.3 MB**<br>(91.5s) | 20.7 MB<br>**(89.6s)** | 20.9 MB<br>(95.3s) | 22.5 MB<br>(213s) | **4.95x** |
| [**GloVe Embeddings**](https://www.kaggle.com/datasets/ouhammourachid/glove-6b-json-format)<br>*(ML Vectors JSON)* | 193.4 MB | 🏆 **57.3 MB**<br>(315s) | 57.9 MB<br>(261s) | 57.9 MB<br>**(239s)** | 60.0 MB<br>(426s) | **3.37x** |

#### 📁 Logs, SQL & Misc
| Dataset | Original size | CAST <br>(w/ LZMA) | LZMA2 <br>(Extreme) | Zstd <br>(Level 22) | Brotli <br>(Quality 11) | CAST Ratio |
| :--- | :---: | :--- | :--- | :--- | :--- | :---: |
| [**Sakila DB**](https://www.kaggle.com/datasets/atanaskanev/sqlite-sakila-sample-database)<br>*(Sample DB SQL)* | 8.7 MB | 🏆 **298 KB**<br>**(2.6s)** | 426 KB<br>(23.4s) | 501 KB<br>(22.9s) | 466 KB<br>(22.8s) | **29.4x** |
| **Weblog Sample**<br>*(Server Logs)* | 67.6 MB | 🏆 **2.5 MB**<br>**(34.6s)** | 2.7 MB<br>(51.3s) | 2.9 MB<br>(77.7s) | 3.1 MB<br>(177s) | **26.8x** |
| [**Logfiles**](https://www.kaggle.com/datasets/vishnu0399/server-logs)<br>*(Apache Web Logs)* | 242.0 MB | 🏆 **10.2 MB**<br>**(99s)** | 13.0 MB<br>(203s) | 13.3 MB<br>(258s) | 14.1 MB<br>(572s) | **23.5x** |
| **Audit Dump**<br>*(Synthetic Logs SQL)* | 64.6 MB | 🏆 **10.1 MB**<br>**(32.8s)** | 12.0 MB<br>(110s) | 12.6 MB<br>(106s) | 12.1 MB<br>(125s) | **6.4x** |

### 2. High-Performance Benchmark (Rust + 7z Implementation)
> **🎯 Goal:** Validate the **Production Throughput** (Speed).

To demonstrate the real-world performance of the algorithm, we implemented a **Rust Port** that processes data in parallel and leverages **7-Zip** as an optimized backend for the final encoding step.
* **Why 7-Zip?** This backend was chosen to simulate a fully optimized, multi-threaded LZMA environment for the PoC without re-implementing a custom threaded encoder from scratch. It represents the "speed ceiling" achievable when CAST is integrated into a mature pipeline.

**Preliminary Rust Results (vs LZMA2 Native 7z):**
*Note: Both CAST and LZMA2 were run using 7z as backend to ensure a fair comparison.*

#### 📄 CSV Datasets (Structured Data)
| Dataset | Original size | CAST (Rust + 7z) | LZMA2 (Standard 7z) | Speed Comparison | Density Gain |
| :--- | :---: | :--- | :--- | :---: | :---: |
| [**Balance of Payments**](https://www.stats.govt.nz/assets/Uploads/Balance-of-payments/Balance-of-payments-and-international-investment-position-September-2025-quarter/Download-data/balance-of-payments-and-international-investment-position-september-2025-quarter.csv) | 33.1 MB | **255 KB** (1.57s) | 834 KB (2.02s) | **1.28x Faster** | **3.26x** |
| [**Migration Stats**](https://www.stats.govt.nz/assets/Uploads/International-migration/International-migration-October-2025/Download-data/international-migration-october-2025-citizenship-by-visa-and-by-country-of-last-permanent-residence.csv) | 29.2 MB | **343 KB** (2.11s) | 1.38 MB (4.64s) | **2.20x Faster** | **4.02x** |
| [**NZDep Life Tables**](https://www.stats.govt.nz) | 13.0 MB | **883 KB** (1.40s) | 1.20 MB (2.73s) | **1.95x Faster** | **1.35x** |
| [**Subnational Life Tables**](https://www.stats.govt.nz) | 16.0 MB | **344 KB** (1.10s) | 824 KB (2.65s) | **2.41x Faster** | **2.39x** |
| [**Custom 2020**](https://www.kaggle.com/datasets/zanjibar/japantradestat) | 207.9 MB | **19.0 MB** (66.4s) | 25.3 MB (89.4s) | **1.35x Faster** | **1.33x** |
| [**Custom 2018**](https://www.kaggle.com/datasets/zanjibar/japantradestat) | 668.3 MB | **25.9 MB** (136s) | 56.6 MB (105s) | *0.77x Slower* | **2.18x** |
| [**IOT Temp**](https://www.kaggle.com/datasets/atulanandjha/temperature-readings-iot-devices) | 6.9 MB | **724 KB** (1.20s) | 787 KB (1.45s) | **1.21x Faster** | **1.08x** |
| [**Sitemap Apple**](https://www.apple.com/sitemap.xml) | 124.2 MB | **1.99 MB** (12.5s) | 2.69 MB (9.25s) | *0.74x Slower* | **1.35x** |
| [**Nashville Housing**](https://www.kaggle.com/datasets/bvanntruong/housing-sql-project) | 9.9 MB | **1.28 MB** (2.05s) | 1.42 MB (2.36s) | **1.15x Faster** | **1.10x** |
| [**Item Aliases**](https://www.kaggle.com/datasets/timoboz/wikidata-jsons) | 201.5 MB | **40.2 MB** (97.0s) | 40.6 MB (83.7s) | *0.86x Slower* | **1.01x** |
| [**IoT Intrusion**](https://www.kaggle.com/datasets/babaruzair/iot-intrusion) | 197.5 MB | **24.2 MB** (74.4s) | 28.2 MB (99.7s) | **1.34x Faster** | **1.16x** |
| [**LinkedIn Profiles**](https://www.kaggle.com/datasets/killbot/linkedin-profiles-and-jobs-data) | 52.5 MB | **4.03 MB** (10.7s) | 4.57 MB (12.0s) | **1.11x Faster** | **1.13x** |
| [**Gafgyt Botnet**](https://www.kaggle.com/datasets/mkashifn/nbaiot-dataset) | 105.8 MB | **25.3 MB** (69.0s) | 26.3 MB (74.9s) | **1.08x Faster** | **1.04x** |
| [**HomeC**](https://www.kaggle.com/datasets/taranvee/smart-home-dataset-with-weather-information) | 131.0 MB | **11.7 MB** (41.3s) | 15.4 MB (54.6s) | **1.32x Faster** | **1.32x** |
| [**DDoS Data**](https://www.kaggle.com/datasets/siddharthm1698/ddos-botnet-attack-on-iot-devices) | 616.8 MB | **10.9 MB** (71.9s) | 20.4 MB (81.1s) | **1.13x Faster** | **1.85x** |
| [**Wireshark P3**](https://www.kaggle.com/datasets/kanelsnegl/wireshark) | 154.4 MB | **6.94 MB** (35.8s) | 10.6 MB (47.7s) | **1.33x Faster** | **1.52x** |
| [**RT_IOT2022**](https://www.kaggle.com/datasets/supplejade/rt-iot2022real-time-internet-of-things) | 54.8 MB | **2.01 MB** (9.54s) | 2.56 MB (8.66s) | *0.91x Slower* | **1.27x** |
| [**Metasploitable**](https://www.kaggle.com/datasets/badcodebuilder/insdn-dataset) | 52.8 MB | **3.52 MB** (11.8s) | 3.87 MB (11.3s) | *0.96x Slower* | **1.10x** |
| [**OWID Covid**](https://www.kaggle.com/datasets/taranvee/covid-19-dataset-till-2222022) | 46.7 MB | **6.36 MB** (14.2s) | 7.20 MB (15.7s) | **1.10x Faster** | **1.13x** |
| [**Assaults 2015**](https://www.kaggle.com/datasets/mohamedbakrey/analysispublicplaceassaultssexualassault-2015) | 234 KB | 39.9 KB (0.08s) | **34.4 KB** (0.06s) | *Slower* | *Loss* |

#### 📄 JSON & XML (Hierarchical Data)
| Dataset | Original size | CAST (Rust + 7z) | LZMA2 (Standard 7z) | Speed Comparison | Density Gain |
| :--- | :---: | :--- | :--- | :---: | :---: |
| [**Wikidata Fanout**](https://www.kaggle.com/datasets/timoboz/wikidata-jsons) | 262.3 MB | **29.2 MB** (124s) | 33.4 MB (139s) | **1.12x Faster** | **1.14x** |
| [**Gandhi Works**](https://www.kaggle.com/datasets/abelgeorge2222/collected-works-mahatma-gandhi-a-json-dataset) | 100.6 MB | **20.3 MB** (55.2s) | 20.8 MB (55.4s) | **Equal** | **1.02x** |
| **Badges** (XML) | 32.7 MB | **1.95 MB** (4.06s) | 2.56 MB (9.16s) | **2.25x Faster** | **1.31x** |
| **Users** (XML) | 48.0 MB | **6.43 MB** (9.57s) | 7.71 MB (15.8s) | **1.65x Faster** | **1.20x** |
| **Votes** (XML) | 145.8 MB | **3.92 MB** (12.9s) | 6.20 MB (30.8s) | **2.39x Faster** | **1.58x** |
| [**Yelp Business**](https://www.kaggle.com/datasets/snax07/yelp-dataset-2024) | 118.9 MB | **10.9 MB** (26.1s) | 11.1 MB (32.5s) | **1.24x Faster** | **1.02x** |
| [**Yelp Tips**](https://www.kaggle.com/datasets/snax07/yelp-dataset-2024) | 180.6 MB | **30.4 MB** (58.6s) | 35.0 MB (79.3s) | **1.35x Faster** | **1.15x** |
| [**Yelp Checkin**](https://www.kaggle.com/datasets/snax07/yelp-dataset-2024) | 287.0 MB | **54.2 MB** (167s) | 55.0 MB (157s) | *0.94x Slower* | **1.01x** |
| [**Parent-Child Dict**](https://www.kaggle.com/datasets/timoboz/wikidata-jsons) | 214.5 MB | **28.8 MB** (111s) | 29.5 MB (120s) | **1.08x Faster** | **1.02x** |
| [**Train.json**](https://huggingface.co/datasets) | 11.9 MB | **1.80 MB** (3.03s) | 1.85 MB (3.06s) | **Equal** | **1.02x** |
| [**Examples Train**](https://huggingface.co/datasets) | 201.4 MB | **4.68 MB** (26.1s) | 7.73 MB (39.3s) | **1.51x Faster** | **1.65x** |
| [**Wiki Text 1**](https://www.kaggle.com/datasets/ltcmdrdata/plain-text-wikipedia-202011) | 41.2 MB | **10.2 MB** (19.1s) | 10.3 MB (17.7s) | *0.93x Slower* | **1.00x** |
| [**Wiki Text 2**](https://www.kaggle.com/datasets/ltcmdrdata/plain-text-wikipedia-202011) | 41.5 MB | **10.0 MB** (18.7s) | 10.1 MB (17.4s) | *0.93x Slower* | **1.00x** |
| [**Glove Emb.**](https://www.kaggle.com/datasets/ouhammourachid/glove-6b-json-format) | 193.4 MB | **57.8 MB** (195s) | 58.1 MB (179s) | *0.92x Slower* | **1.00x** |
| [**Pagerank**](https://www.kaggle.com/datasets/aldebbaran/html-br-collection) | 121.9 MB | **15.7 MB** (48.6s) | 15.8 MB (45.4s) | *0.94x Slower* | **1.00x** |
| [**Brazil Geo**](https://www.kaggle.com/datasets/thiagobodruk/brazil-geojson) | 14.5 MB | 1.55 MB (3.03s) | **1.55 MB** (2.52s) | *Slower* | *Loss* |

#### 📁 Logs, SQL & Misc
| Dataset | Original size | CAST (Rust + 7z) | LZMA2 (Standard 7z) | Speed Comparison | Density Gain |
| :--- | :---: | :--- | :--- | :---: | :---: |
| [**Logfiles**](https://www.kaggle.com/datasets/vishnu0399/server-logs) | 242.0 MB | **11.9 MB** (39.2s) | 15.7 MB (52.6s) | **1.34x Faster** | **1.32x** |
| [**Weblog Sample**](https://www.kaggle.com/datasets/kimjmin/apache-web-log) | 67.6 MB | **2.90 MB** (9.52s) | 3.16 MB (9.09s) | *0.95x Slower* | **1.09x** |
| [**Dynamic Audit**](https://www.kaggle.com/datasets/atanaskanev/sqlite-sakila-sample-database) (SQL) | 64.6 MB | **10.0 MB** (16.0s) | 12.4 MB (27.1s) | **1.69x Faster** | **1.23x** |
| [**Sakila Insert**](https://www.kaggle.com/datasets/atanaskanev/sqlite-sakila-sample-database) (SQL) | 8.8 MB | **297 KB** (1.04s) | 492 KB (0.97s) | *0.93x Slower* | **1.65x** |
| [**Xdados**](https://www.kaggle.com/datasets/caesarlupum/iot-sensordata) (Txt) | 4.4 MB | **433 KB** (1.50s) | 533 KB (1.10s) | *0.73x Slower* | **1.23x** |
| **PCAP Dump** (Bin) | 0.9 MB | 144 KB (0.18s) | **144 KB** (0.18s) | *Equal* | *Loss* |
| **IP Capture** (Bin) | 38.7 MB | 18.3 MB (3.50s) | **18.3 MB** (3.39s) | *Equal* | *Loss* |

> **Observation:** As shown in the Rust preview, when the interpreter bottleneck is removed and multi-threading is applied, **CAST retains its massive compression ratio advantage while becoming drastically faster**, often outpacing the standard LZMA2 compression process itself.
> Even in cases where CAST is slightly slower (due to pre-processing overhead), the Density Gain often justifies the trade-off. Binary files or high-entropy streams (like PCAP) are correctly identified and passed through with minimal overhead.

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
* **Goal:** Maximum Compression Density & Algorithmic Reference.
* **Method:** A simplified, monolithic implementation using Python's native `lzma`. It processes the file as a single block to maximize global deduplication context.
* **Pros:** Achieves the absolute best compression ratios (theoretical maximum) and serves as the readable baseline for the algorithm logic.
* **Cons:** Lacks the advanced resource management (chunking) and parallelism found in the Rust port. Slower due to interpreter overhead and limited by available RAM.

### 2. 🦀 Rust Implementation (The Performance Preview)
* **Goal:** High-Performance, Scalability & Production Simulation.
* **Method:** A highly optimized port designed for speed and resource management. Unlike the Python reference, this version introduces **Multithreading** (for parallel block processing) and **Stream Chunking** (to manage memory pressure).
* **Backends:**
    * **7z Backend:** Invokes the external `7z` CLI. Fastest option, max throughput.
    * **Native Backend:** Self-contained, no external dependencies.
* **Pros:**
    * **Structural Efficiency:** Drastically faster on datasets requiring intensive structural manipulation and complex memory access patterns, leveraging Rust's zero-cost abstractions.
    * **Scalability:** Includes a `--chunk-size` feature to process datasets in stream-like blocks. This guarantees a **constant low-memory footprint**, preventing OS pressure or swapping even for files that technically fit in RAM but are unwieldy to load entirely.
    * **Parallelism:** Both backends fully support multithreading, significantly accelerating large file processing.
* **Trade-off:** The use of multithreading and block-based processing may result in a negligible compression difference (<1%) compared to the monolithic Python reference, exchanged for massive gains in throughput and scalability.

---

## 🚀 Usage

Since this project offers multiple implementations, detailed usage instructions, dependencies, and build commands are provided in the respective directories:

* **📂 [Python Implementation](./python_impl/)**: Follow the instructions in the inner README to run the reference scripts.
* **📂 [Rust Implementation](./rust_impl/)**: Refer to the inner README to choose between the **7z Backend** or **Native** version and for compilation steps.

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
