# HACC 1.0

**Big files. Short waits.** HACC is a Windows archiver with a custom NVIDIA GPU compression codec, a desktop interface, and integrity checks for every compressed block. In our 2 GiB benchmark, it produced a **1,041.66 MiB archive in 6.27 seconds**: about **6.3× faster than ZIP**, with an archive only **1.4% larger**.

Select files or a folder in File Explorer and choose **Pack with HACC…**. Right-click a `.hacc` archive and choose **Extract with HACC…**. The desktop app also lets you select multiple inputs, choose a destination, and switch between GPU and CPU compression. Empty folders are preserved.

## Why HACC?

- **Fast GPU compression.** A parallel LZ codec processes 4 KiB chunks on an NVIDIA GPU and represents repeated sequences with compact references.
- **Substantial size savings.** The GPU compressed a 256 MiB mixed-data file to **130.21 MiB** and 256 MiB of zeros to **0.56 MiB** in our tests.
- **Verified extraction.** Every block carries a CRC32 checksum; our round-trip benchmarks also verified the extracted files with SHA-256.
- **Built for large inputs.** Compression and extraction stream data in blocks instead of loading an entire file into memory.
- **CPU option.** CPU compression works without an NVIDIA GPU. Extracting a GPU-compressed archive requires a compatible NVIDIA GPU.

## Install on Windows

Run **`release/HACC-Setup-1.0.0.exe`**. The installer works per user without administrator privileges and includes both the desktop app and command-line tool. It adds HACC to the Start menu, associates `.hacc` files, and adds **Pack with HACC…** and **Extract with HACC…** to File Explorer. On Windows 11, these entries may appear under **Show more options**. Uninstall through Windows Settings.

GPU mode needs a compatible NVIDIA card and driver. The CUDA Toolkit is needed to build HACC from source, but not to run the installer.

## Measured performance

All measurements were made on the same PC with an NVIDIA GeForce RTX 3080 and a local disk. HACC GPU uses `-g`, HACC CPU uses `-l3`, ZIP is Deflate `-mx=5` from 7-Zip 26.03, and RAR uses `-m3` from RAR 6.11. Times measure **archive creation** in seconds. The `mixed.bin` HACC times are the average of two runs; other times are from one run.

| Input and metric | HACC GPU | HACC CPU | ZIP | RAR |
|---|---:|---:|---:|---:|
| `mixed.bin`, 256 MiB — archive size | **130.21 MiB** | 130.51 MiB | 128.42 MiB | 128.28 MiB |
| `mixed.bin` — time | 1.10 s | **0.86 s** | 4.98 s | 5.20 s |
| `real2g.bin`, 2,048 MiB — archive size | **1,041.66 MiB** | 1,044.06 MiB | 1,027.37 MiB | 1,026.22 MiB |
| `real2g.bin` — time | **6.27 s** | 6.68 s | 39.43 s | 41.59 s |
| 256 MiB of zeros — archive size | 0.56 MiB | 5.01 MiB | 0.30 MiB | **0.016 MiB** |
| 256 MiB of zeros — time | 0.87 s | **0.61 s** | 4.51 s | 1.40 s |

ZIP and RAR produced smaller archives in these tests. HACC GPU delivered close compression on the 2 GiB input much faster. Results depend on the data: the first 256 MiB of `real2g.bin` is the same data as `mixed.bin`, so these are not independent content samples. Every extracted benchmark file passed SHA-256 verification. Exact figures are in [`benchmark_results.csv`](benchmark_results.csv); run `python benchmark.py` to reproduce the comparison.

## Command line

```powershell
hacc a -g archive.hacc file.bin folder
hacc x archive.hacc extracted
hacc a -l3 archive-cpu.hacc file.bin
```

CPU levels are `-l1`, `-l3`, and `-l7`. `-g` requires a GPU-enabled build and a compatible NVIDIA GPU. Symbolic links are unsupported. New archives use format version 2; HACC 1.0 can also read version 1 archives and older HGP1/HGP2 GPU blocks.

## Build from source

On Windows, install Rust, Visual Studio C++ Build Tools, and the NVIDIA CUDA Toolkit (`nvcc`). Then run:

```powershell
./scripts/build-release.ps1
```

The script runs GPU tests and builds the CLI, desktop app, and standalone installer at `release/HACC-Setup-1.0.0.exe`. A CPU-only CLI can be built without CUDA using `cargo build --release`.
