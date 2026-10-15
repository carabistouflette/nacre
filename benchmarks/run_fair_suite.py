#!/usr/bin/env python3
import subprocess
import re
import statistics

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
    print("Running 5 iterations of Nacre vs Rust suite...")
    nacre_stats = run_suite("/tmp/suite_nacre", runs=5)
    rust_stats = run_suite("/tmp/suite_rust", runs=5)

    print("\n| Benchmark Domain | Problem Description | Nacre (Median) | Rust (Median) | Ratio (Rust / Nacre) | Verdict |")
    print("|---|---|---|---|---|---|")

    descriptions = {
        "mandelbrot": "Mandelbrot fractal (700x700, 200 iter) [Floating Point]",
        "n_queens": "N-Queens backtracking solver (N=14) [Bitwise & Recursion]",
        "quicksort": "Quicksort (100k random numbers) [Cache Locality & Swaps]",
        "game_of_life": "Game of Life (100x100 grid, 100 gen) [2D Spatial Stride]",
        "text_map": "Text key Map churn (50,000 ops) [Hash Table & Strings]",
        "binary_tree": "Binary Tree recursive sum (524k nodes) [Pointer Chasing]",
    }

    for name, desc in descriptions.items():
        n_med = nacre_stats[name]["median"]
        r_med = rust_stats[name]["median"]
        if n_med == 0 and r_med == 0:
            ratio = "1.00x"
            verdict = "Exact Parity"
        elif n_med == 0:
            ratio = "< 1.00x"
            verdict = "Nacre is faster"
        elif r_med == 0:
            ratio = "> 1.00x"
            verdict = "Rust is faster"
        else:
            ratio_val = r_med / n_med
            ratio = f"{ratio_val:.2f}x"
            if abs(ratio_val - 1.0) <= 0.05:
                verdict = "Exact Parity"
            elif ratio_val > 1.0:
                verdict = f"Nacre is {ratio_val:.1f}x faster"
            else:
                verdict = f"Rust is {1.0/ratio_val:.1f}x faster"

        print(f"| **`{name}`** | {desc} | **{n_med} ms** (min: {nacre_stats[name]['min']}) | **{r_med} ms** (min: {rust_stats[name]['min']}) | {ratio} | **{verdict}** |")

if __name__ == "__main__":
    main()
