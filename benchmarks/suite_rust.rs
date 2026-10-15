use std::time::Instant;
use std::collections::HashMap;

// 1. Pure Floating-Point Compute: Mandelbrot escape time calculation
fn mandelbrot(width: usize, height: usize, max_iter: usize) -> usize {
    let mut total_iterations = 0;
    for y in 0..height {
        let ci = (y as f64) * 2.0 / (height as f64) - 1.0;
        for x in 0..width {
            let cr = (x as f64) * 3.0 / (width as f64) - 2.1;
            let mut zr = 0.0;
            let mut zi = 0.0;
            let mut iter = 0;
            while zr * zr + zi * zi <= 4.0 && iter < max_iter {
                let temp = zr * zr - zi * zi + cr;
                zi = 2.0 * zr * zi + ci;
                zr = temp;
                iter += 1;
            }
            total_iterations += iter;
        }
    }
    total_iterations
}

// 2. Discrete Math & Backtracking: N-Queens (N=14)
struct NQueens {
    count: usize,
}

impl NQueens {
    fn new() -> Self {
        Self { count: 0 }
    }

    fn solve(&mut self, row: i32, ld: i32, rd: i32, all_mask: i32) {
        if row == all_mask {
            self.count += 1;
            return;
        }
        let mut pos = all_mask & !(row | ld | rd);
        while pos != 0 {
            let p = pos & -pos;
            pos -= p;
            self.solve(row | p, (ld | p) << 1, (rd | p) >> 1, all_mask);
        }
    }
}

// 3. Sorting & Dynamic Memory Access: Quicksort on 100,000 integers
fn quicksort(slice: &mut [i32]) {
    if slice.len() <= 1 {
        return;
    }
    let pivot = slice[slice.len() - 1];
    let mut i = 0;
    for j in 0..slice.len() - 1 {
        if slice[j] <= pivot {
            slice.swap(i, j);
            i += 1;
        }
    }
    let last = slice.len() - 1;
    slice.swap(i, last);
    let (left, right) = slice.split_at_mut(i);
    quicksort(left);
    quicksort(&mut right[1..]);
}

// 4. Cellular Automaton Simulation: Game of Life (100x100 grid, 100 generations)
struct LifeSim {
    grid: Vec<i32>,
    next: Vec<i32>,
}

impl LifeSim {
    fn new() -> Self {
        let total = 100 * 100;
        let mut grid = Vec::with_capacity(total);
        let next = vec![0; total];
        for i in 0..total {
            let val = if (i * 17 + 11) % 7 == 0 || (i * 31 + 5) % 11 == 0 { 1 } else { 0 };
            grid.push(val);
        }
        Self { grid, next }
    }

    fn count_neighbors(&self, r: usize, c: usize) -> i32 {
        let mut count = 0;
        for dr in [-1, 0, 1] {
            for dc in [-1, 0, 1] {
                if dr != 0 || dc != 0 {
                    let nr = (r as isize + dr + 100) as usize % 100;
                    let nc = (c as isize + dc + 100) as usize % 100;
                    if self.grid[nr * 100 + nc] != 0 {
                        count += 1;
                    }
                }
            }
        }
        count
    }

    fn step(&mut self) {
        for r in 0..100 {
            for c in 0..100 {
                let idx = r * 100 + c;
                let current = self.grid[idx];
                let neighbors = self.count_neighbors(r, c);
                let next_val = if current != 0 {
                    if neighbors == 2 || neighbors == 3 { 1 } else { 0 }
                } else {
                    if neighbors == 3 { 1 } else { 0 }
                };
                self.next[idx] = next_val;
            }
        }
        self.grid.copy_from_slice(&self.next);
    }

    fn live_count(&self) -> i32 {
        self.grid.iter().filter(|&&v| v != 0).count() as i32
    }
}

// 5. Text & Hash Map churn (50,000 operations)
fn bench_text_map(count: usize) -> i32 {
    let mut map: HashMap<String, i32> = HashMap::new();
    for i in 0..count {
        let k = format!("key-prefix-segment-{}", i % 10000);
        map.insert(k, i as i32);
    }
    let mut sum = 0;
    for i in 0..count {
        let q = format!("key-prefix-segment-{}", i % 10000);
        if let Some(&v) = map.get(&q) {
            sum += v & 1;
        }
    }
    sum
}

// 6. Tree / Recursive Data Structures: Binary Tree sum
struct Node {
    value: i32,
    children: Vec<Node>,
}

impl Node {
    fn new(val: i32) -> Self {
        Self { value: val, children: Vec::new() }
    }

    fn build(depth: usize, val: i32) -> Self {
        let mut node = Node::new(val);
        if depth > 0 {
            node.children.push(Node::build(depth - 1, val * 2));
            node.children.push(Node::build(depth - 1, val * 2 + 1));
        }
        node
    }

    fn sum(&self) -> i32 {
        let mut total = self.value;
        for child in &self.children {
            total += child.sum();
        }
        total
    }
}

fn main() {
    println!("BENCHMARK_SUITE_START");

    // 1. Mandelbrot (700x700, 200 iter)
    let t0 = Instant::now();
    let mandel_res = mandelbrot(700, 700, 200);
    let mandel_ms = t0.elapsed().as_millis();
    println!("RESULT mandelbrot: {mandel_ms} ms (checksum: {mandel_res})");

    // 2. N-Queens (N=14)
    let t0 = Instant::now();
    let mut nq = NQueens::new();
    nq.solve(0, 0, 0, (1 << 14) - 1);
    let nq_ms = t0.elapsed().as_millis();
    println!("RESULT n_queens: {nq_ms} ms (solutions: {})", nq.count);

    // 3. Quicksort (100,000 pseudo-random numbers)
    let mut numbers = Vec::with_capacity(100_000);
    let mut seed: i64 = 1234567;
    for _ in 0..100_000 {
        seed = (seed * 1103515245 + 12345) & 2147483647;
        numbers.push((seed % 1000000) as i32);
    }
    let total_numbers = numbers.len();
    let t0 = Instant::now();
    quicksort(&mut numbers);
    let sort_ms = t0.elapsed().as_millis();
    println!("RESULT quicksort: {sort_ms} ms (min: {}, max: {})", numbers[0], numbers[total_numbers - 1]);

    // 4. Game of Life (100x100 grid, 100 generations)
    let mut sim = LifeSim::new();
    let t0 = Instant::now();
    for _ in 0..100 {
        sim.step();
    }
    let life_ms = t0.elapsed().as_millis();
    println!("RESULT game_of_life: {life_ms} ms (live cells: {})", sim.live_count());

    // 5. Text & Hash Map churn (50,000 operations)
    let t0 = Instant::now();
    let text_map_res = bench_text_map(50_000);
    let map_ms = t0.elapsed().as_millis();
    println!("RESULT text_map: {map_ms} ms (checksum: {text_map_res})");

    // 6. Binary Tree recursion (depth=18, 524,287 nodes)
    let t0 = Instant::now();
    let tree = Node::build(18, 1);
    let tree_sum = tree.sum();
    let tree_ms = t0.elapsed().as_millis();
    println!("RESULT binary_tree: {tree_ms} ms (tree sum: {tree_sum})");

    println!("BENCHMARK_SUITE_END");
}
