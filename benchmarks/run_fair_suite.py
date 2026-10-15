#!/usr/bin/env python3
import subprocess
import re
import statistics
import os
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

def build_binaries():
    print("Building benchmark binaries with native optimizations...")
    nacre_bin = ROOT / "target" / "release" / "nacre"
    if not nacre_bin.exists():
        subprocess.run(["cargo", "build", "--release", "--locked"], cwd=ROOT, check=True)

    # 1. Nacre Default
    subprocess.run([
        str(nacre_bin), "build", "benchmarks/suite_nacre.ncr",
        "-o", "/tmp/suite_nacre", "-O3", "--native"
    ], cwd=ROOT, check=True)

    # 2. Nacre with Jemalloc
    subprocess.run([
        str(nacre_bin), "build", "benchmarks/suite_nacre.ncr",
        "-o", "/tmp/suite_nacre_jemalloc", "-O3", "--native", "--jemalloc"
    ], cwd=ROOT, check=True)

    # 3. Rust Native
    subprocess.run([
        "rustc", "-O", "-C", "target-cpu=native",
        "benchmarks/suite_rust.rs", "-o", "/tmp/suite_rust"
    ], cwd=ROOT, check=True)

def run_suite(binary, runs=5):
    results = {}
    for _ in range(runs):
        proc = subprocess.run([binary], capture_output=True, text=True, check=True)
        for line in proc.stdout.splitlines():
            m = re.match(r"^RESULT\s+(\w+):\s+(\d+)\s+ms", line)
            if m:
                name = m.group(1)
                ms = int(m.group(2))
                results.setdefault(name, []).append(ms)
    summary = {}
    for name, times in results.items():
        summary[name] = {
            "median": statistics.median(times),
            "min": min(times),
            "max": max(times),
            "samples": times,
        }
    return summary

def main():
    build_binaries()
    print("Running 5 iterations across all configurations...")
    nacre_def = run_suite("/tmp/suite_nacre", runs=5)
    nacre_jem = run_suite("/tmp/suite_nacre_jemalloc", runs=5)
    rust_stats = run_suite("/tmp/suite_rust", runs=5)

    print("\n| Benchmark Domain | Description | Nacre (Default) | Nacre (Jemalloc) | Native Rust | Nacre vs Rust Verdict |")
    print("|---|---|---|---|---|---|")

    descriptions = {
        "mandelbrot": "Mandelbrot (700x700, 200 iter) [Float Math]",
        "n_queens": "N-Queens solver (N=14) [Bitwise & Recursion]",
        "quicksort": "Quicksort (100k random ints) [Memory Swaps]",
        "game_of_life": "Game of Life (100x100 grid, 100 gen) [2D Modulo Stride]",
        "text_map": "Text key Map churn (50k ops) [Hash Table & Strings]",
        "binary_tree": "Binary Tree recursion (524k heap nodes) [Allocator]",
    }

    for name, desc in descriptions.items():
        n_def = nacre_def[name]["median"]
        n_jem = nacre_jem[name]["median"]
        r_med = rust_stats[name]["median"]

        best_nacre = min(n_def, n_jem)
        if r_med == 0:
            verdict = "Rust is faster (< 1 ms)"
        elif best_nacre == 0:
            verdict = "Nacre is faster (< 1 ms)"
        elif best_nacre == r_med or abs(best_nacre - r_med) / max(best_nacre, r_med) <= 0.05:
            verdict = "Exact Parity"
        elif best_nacre < r_med:
            verdict = f"Nacre is {r_med / best_nacre:.1f}x faster"
        else:
            verdict = f"Rust is {best_nacre / r_med:.1f}x faster"
        print(f"| **`{name}`** | {desc} | **{n_def} ms** | **{n_jem} ms** | **{r_med} ms** | **{verdict}** |")

if __name__ == "__main__":
    main()
