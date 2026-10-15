# Nacre vs Rust Performance & Memory Benchmarks

Comparative benchmark suite evaluating execution latency, throughput, and resident memory footprint between Nacre (v0.12.0) with native compiler optimizations (`-O3 --native --jemalloc`) and Rust (`rustc 1.99.0 -O -C target-cpu=native`).

## Suite Overview & Execution Times

Measurements taken across 5 runs on Linux x86_64:

| Benchmark Domain | Description | Nacre (Default) | Nacre (Jemalloc) | Native Rust | Verdict |
|---|---|---|---|---|---|
| **`mandelbrot`** | Mandelbrot (700x700, 200 iter) [Float Math] | 55 ms | 54 ms | 55 ms | **Exact Parity** |
| **`n_queens`** | N-Queens solver (N=14) [Bitwise & Recursion] | 130 ms | 134 ms | 145 ms | **Nacre is 1.1x faster** |
| **`quicksort`** | Quicksort (100k random ints) [Memory Swaps] | 5 ms | 5 ms | 4 ms | **Rust is 1.2x faster** |
| **`game_of_life`** | Game of Life (100x100 grid, 100 gen) [2D Modulo Stride] | 3 ms | 3 ms | < 1 ms | **Rust is faster (< 1 ms)** |
| **`text_map`** | Text key Map churn (50k ops) [Hash Table & Strings] | 7 ms | 7 ms | 5 ms | **Rust is 1.4x faster** |
| **`binary_tree`** | Binary Tree recursion (524k heap nodes) [Allocator] | 27 ms | 14 ms | 17 ms | **Nacre is 1.2x faster** |

## Memory Footprint (Peak RSS)

Peak resident set size measured via `wait4` supervisor (`RUSAGE_CHILDREN`):

| Kernel | Nacre Median Time | Nacre Peak RSS | Rust Median Time | Rust Peak RSS |
|---|---|---|---|---|
| **`list_fill_traverse`** | 1.22 ms | 2.18 MB | 1.50 ms | 2.38 MB |
| **`map_int_insert_query_remove`** | 1.24 ms | 2.37 MB | 1.55 ms | 2.36 MB |
| **`map_text_insert_query_replace`** | 2.44 ms | 2.37 MB | 1.98 ms | 2.54 MB |
| **`empty_map_large_text_miss`** | 0.73 ms | 2.37 MB | 0.71 ms | 2.52 MB |

## Key Insights
1. **Allocator Efficiency**: With `--jemalloc` and Small String Optimization (SSO), Nacre's heap node allocation speed matches and slightly outperforms Rust's standard allocator on recursive tree construction.
2. **Computational Loops**: Bitwise and recursive computation (`n_queens`) exhibits parity or faster throughput due to lightweight stack frames and direct C11 emission.
3. **Resident Memory**: Across all collection benchmarks, Nacre's peak RSS remains tightly bounded around 2.2 - 2.4 MB, with zero leaked objects (`live=0`).
