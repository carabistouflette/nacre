// Equivalent named workloads for the Option1 Nacre/Rust benchmark suite.
// Nacre has no JSON prelude parser; both sides parse the same fixed typed schema.
use std::collections::HashMap;
use std::hint::black_box;
use std::time::{Duration, Instant};

const JSON_INPUT: &str = " [\n { \"id\" : 7 , \"name\" : \"oak\\\"wood\" , \"active\" : true } ,\t{ \"id\" : 19 , \"name\" : \"pine\\n\" , \"active\" : false } ,\n { \"id\" : 42 , \"name\" : \"elm\\u0021\" , \"active\" : true } ] ";
const STRING_INPUT: &str = "nacre::memory::benchmarks::string::search::stream::nacre::compiler";
const SUDOKU_PUZZLE: &str = "530070000600195000098000060800060003400803001700020006060000280000419005000080079";
const WORKLOADS: [&str; 6] = ["json", "string", "binary", "hashmap", "sudoku", "game_of_life"];

struct JsonParser {
    input: String,
    position: usize,
    string_checksum: i64,
}

impl JsonParser {
    fn new(input: String) -> Self {
        Self { input, position: 0, string_checksum: 0 }
    }

    fn skip_whitespace(&mut self) {
        while self.input.as_bytes().get(self.position).is_some_and(
            |value| matches!(*value, b' ' | b'\t' | b'\n' | b'\r')
        ) {
            self.position += 1;
        }
    }

    fn punctuation(&mut self, expected: u8) -> bool {
        self.skip_whitespace();
        if self.input.as_bytes().get(self.position) != Some(&expected) {
            return false;
        }
        self.position += 1;
        true
    }

    fn key(&mut self, expected: &str) -> bool {
        self.skip_whitespace();
        if self.input.as_bytes().get(self.position) != Some(&b'"') {
            return false;
        }
        self.position += 1;
        if self.input.as_bytes().get(
            self.position..self.position.saturating_add(expected.len())
        ) != Some(expected.as_bytes()) {
            return false;
        }
        self.position += expected.len();
        if self.input.as_bytes().get(self.position) != Some(&b'"') {
            return false;
        }
        self.position += 1;
        true
    }

    fn literal(&mut self, expected: &str) -> bool {
        self.skip_whitespace();
        if self.input.as_bytes().get(
            self.position..self.position.saturating_add(expected.len())
        ) != Some(expected.as_bytes()) {
            return false;
        }
        self.position += expected.len();
        true
    }

    fn hex_unit(&mut self) -> Option<u32> {
        if self.position + 4 > self.input.len() {
            return None;
        }
        let mut value = 0_u32;
        for _ in 0..4 {
            let digit = match self.input.as_bytes()[self.position] {
                b'0'..=b'9' => u32::from(self.input.as_bytes()[self.position] - b'0'),
                b'A'..=b'F' => u32::from(self.input.as_bytes()[self.position] - b'A' + 10),
                b'a'..=b'f' => u32::from(self.input.as_bytes()[self.position] - b'a' + 10),
                _ => return None,
            };
            value = value * 16 + digit;
            self.position += 1;
        }
        Some(value)
    }
    fn checksum_byte(checksum: i64, value: u8, offset: usize) -> i64 {
        checksum + i64::from(value) * (offset as i64 + 1)
    }

    fn checksum_codepoint(mut checksum: i64, value: u32, offset: usize) -> i64 {
        if value <= 0x7f {
            return Self::checksum_byte(checksum, value as u8, offset);
        }
        if value <= 0x7ff {
            checksum = Self::checksum_byte(checksum, (0xc0 | (value >> 6)) as u8, offset);
            return Self::checksum_byte(checksum, (0x80 | (value & 0x3f)) as u8, offset + 1);
        }
        if value <= 0xffff {
            checksum = Self::checksum_byte(checksum, (0xe0 | (value >> 12)) as u8, offset);
            checksum = Self::checksum_byte(checksum, (0x80 | ((value >> 6) & 0x3f)) as u8, offset + 1);
            return Self::checksum_byte(checksum, (0x80 | (value & 0x3f)) as u8, offset + 2);
        }
        checksum = Self::checksum_byte(checksum, (0xf0 | (value >> 18)) as u8, offset);
        checksum = Self::checksum_byte(checksum, (0x80 | ((value >> 12) & 0x3f)) as u8, offset + 1);
        checksum = Self::checksum_byte(checksum, (0x80 | ((value >> 6) & 0x3f)) as u8, offset + 2);
        Self::checksum_byte(checksum, (0x80 | (value & 0x3f)) as u8, offset + 3)
    }

    fn codepoint_bytes(value: u32) -> usize {
        if value <= 0x7f { 1 } else if value <= 0x7ff { 2 } else if value <= 0xffff { 3 } else { 4 }
    }

    fn string_length(&mut self) -> Option<usize> {
        if !self.punctuation(b'"') {
            return None;
        }
        let mut decoded_bytes = 0;
        let mut checksum = 0_i64;
        loop {
            let value = *self.input.as_bytes().get(self.position)?;
            self.position += 1;
            if value == b'"' {
                self.string_checksum = checksum;
                return Some(decoded_bytes);
            }
            if value < 0x20 {
                return None;
            }
            if value != b'\\' {
                checksum = Self::checksum_byte(checksum, value, decoded_bytes);
                decoded_bytes += 1;
                continue;
            }
            let escape = *self.input.as_bytes().get(self.position)?;
            self.position += 1;
            let decoded = match escape {
                b'"' => Some(b'"'),
                b'\\' => Some(b'\\'),
                b'/' => Some(b'/'),
                b'b' => Some(8),
                b'f' => Some(12),
                b'n' => Some(b'\n'),
                b'r' => Some(b'\r'),
                b't' => Some(b'\t'),
                _ => None,
            };
            if let Some(decoded) = decoded {
                checksum = Self::checksum_byte(checksum, decoded, decoded_bytes);
                decoded_bytes += 1;
            } else if escape == b'u' {
                let first = self.hex_unit()?;
                let mut codepoint = first;
                if (0xd800..=0xdbff).contains(&first) {
                    if self.input.as_bytes().get(self.position..self.position + 2) != Some(b"\\u") {
                        return None;
                    }
                    self.position += 2;
                    let second = self.hex_unit()?;
                    if !(0xdc00..=0xdfff).contains(&second) {
                        return None;
                    }
                    codepoint = 0x10000 + ((first - 0xd800) << 10) + (second - 0xdc00);
                } else if (0xdc00..=0xdfff).contains(&first) {
                    return None;
                }
                checksum = Self::checksum_codepoint(checksum, codepoint, decoded_bytes);
                decoded_bytes += Self::codepoint_bytes(codepoint);
            } else {
                return None;
            }
        }
    }

    fn integer_value(&mut self) -> Option<i32> {
        self.skip_whitespace();
        let negative = if self.input.as_bytes().get(self.position) == Some(&b'-') {
            self.position += 1;
            true
        } else {
            false
        };
        let first = *self.input.as_bytes().get(self.position)?;
        if !first.is_ascii_digit() {
            return None;
        }
        let mut magnitude = 0_i64;
        if first == b'0' {
            self.position += 1;
            if self.input.as_bytes().get(self.position).is_some_and(u8::is_ascii_digit) {
                return None;
            }
        } else {
            while let Some(digit) = self.input.as_bytes().get(self.position).copied() {
                if !digit.is_ascii_digit() {
                    break;
                }
                magnitude = magnitude * 10 + i64::from(digit - b'0');
                if magnitude > 2_147_483_648 {
                    return None;
                }
                self.position += 1;
            }
        }
        if negative {
            if magnitude > 2_147_483_648 {
                return None;
            }
            Some((-magnitude) as i32)
        } else if magnitude > i64::from(i32::MAX) {
            None
        } else {
            Some(magnitude as i32)
        }
    }

    fn boolean_value(&mut self, true_token: &str, false_token: &str) -> Option<bool> {
        if self.literal(true_token) {
            Some(true)
        } else if self.literal(false_token) {
            Some(false)
        } else {
            None
        }
    }

    fn parse(&mut self) -> Option<i64> {
        let id_key = String::from("id");
        let name_key = String::from("name");
        let active_key = String::from("active");
        let true_token = String::from("true");
        let false_token = String::from("false");
        if !self.punctuation(b'[') {
            return None;
        }
        let mut records = 0;
        let mut checksum = 0_i64;
        loop {
            self.skip_whitespace();
            if self.input.as_bytes().get(self.position) == Some(&b']') {
                self.position += 1;
                break;
            }
            if !self.punctuation(b'{')
                || !self.key(&id_key)
                || !self.punctuation(b':')
            {
                return None;
            }
            let identifier = self.integer_value()?;
            if identifier < 0
                || !self.punctuation(b',')
                || !self.key(&name_key)
                || !self.punctuation(b':')
            {
                return None;
            }
            let name_bytes = self.string_length()?;
            let name_checksum = self.string_checksum;
            if !self.punctuation(b',')
                || !self.key(&active_key)
                || !self.punctuation(b':')
            {
                return None;
            }
            let active = self.boolean_value(&true_token, &false_token)? as i64;
            if !self.punctuation(b'}') {
                return None;
            }
            checksum += i64::from(identifier) * 31
                + name_bytes as i64 * 17 + active + name_checksum * 13;
            records += 1;
            self.skip_whitespace();
            match self.input.as_bytes().get(self.position) {
                Some(b',') => {
                    self.position += 1;
                    self.skip_whitespace();
                    if self.input.as_bytes().get(self.position) == Some(&b']') {
                        return None;
                    }
                }
                Some(b']') => {
                    self.position += 1;
                    break;
                }
                _ => return None,
            }
        }
        self.skip_whitespace();
        (records == 3 && self.position == self.input.len()).then_some(checksum)
    }
}

fn json_kernel() -> i64 {
    JsonParser::new(String::from(JSON_INPUT)).parse().unwrap_or(-1)
}

fn string_kernel() -> i64 {
    let input = String::from(STRING_INPUT);
    let separator = String::from("::");
    let replacement = String::from("|");
    let delimiter = String::from("|");
    let needle = String::from("nacre");
    let rewritten = input.replace(separator.as_str(), replacement.as_str());
    let fields: Vec<String> = rewritten
        .split(delimiter.as_str())
        .map(str::to_owned)
        .collect();
    let mut checksum = 0_i64;
    let mut matches = 0_i64;
    for field in &fields {
        let found = field.find(needle.as_str()).map_or(-1_i64, |index| index as i64);
        if found >= 0 {
            matches += 1;
        }
        checksum += field.len() as i64 * 31 + found;
    }
    checksum * 100 + matches
}

fn byte_checksum(bytes: &[u8]) -> i64 {
    bytes.iter().enumerate().map(|(index, value)| i64::from(*value) * (index as i64 % 17 + 1)).sum()
}

struct MemorySource<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl MemorySource<'_> {
    fn read(&mut self, destination: &mut Vec<u8>, max_bytes: usize) -> usize {
        let amount = max_bytes.min(self.bytes.len().saturating_sub(self.position));
        let end = self.position + amount;
        while self.position < end {
            destination.push(self.bytes[self.position]);
            self.position += 1;
        }
        amount
    }
}

struct MemorySink {
    bytes: Vec<u8>,
}

impl MemorySink {
    fn write(&mut self, source: &[u8], offset: usize, max_bytes: usize) -> usize {
        if offset > source.len() {
            return 0;
        }
        let amount = max_bytes.min(source.len() - offset);
        let mut index = 0;
        while index < amount {
            self.bytes.push(source[offset + index]);
            index += 1;
        }
        amount
    }
}

fn binary_kernel() -> i64 {
    let mut input = Vec::with_capacity(4096);
    for index in 0..4096 {
        input.push(((index * 37 + 19) % 127) as u8);
    }
    let mut source = MemorySource { bytes: &input, position: 0 };
    let mut sink = MemorySink { bytes: Vec::new() };
    let mut buffer = Vec::with_capacity(257);
    let mut copied = 0_usize;
    loop {
        buffer.clear();
        let count = source.read(&mut buffer, 257);
        if count == 0 {
            break;
        }
        let mut offset = 0;
        while offset < count {
            let written = sink.write(&buffer, offset, count - offset);
            if written == 0 {
                return -1;
            }
            offset += written;
        }
        copied += count;
    }
    if copied != 4096 || sink.bytes != input {
        return -1;
    }
    let encoded = hex_encode(&sink.bytes);
    let decoded = match hex_decode(&encoded) {
        Some(bytes) => bytes,
        None => return -1,
    };
    if decoded.len() != 4096 {
        return -1;
    }
    byte_checksum(&decoded) + copied as i64 * 31 + encoded.len() as i64
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 15) as usize] as char);
    }
    output
}

fn hex_decode(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    fn nibble(value: u8) -> Option<u8> {
        match value {
            b'0'..=b'9' => Some(value - b'0'),
            b'a'..=b'f' => Some(value - b'a' + 10),
            b'A'..=b'F' => Some(value - b'A' + 10),
            _ => None,
        }
    }
    let mut output = Vec::with_capacity(text.len() / 2);
    for pair in text.as_bytes().chunks_exact(2) {
        output.push((nibble(pair[0])? << 4) | nibble(pair[1])?);
    }
    Some(output)
}

fn hashmap_kernel() -> i64 {
    let count = 2048_i32;
    let mut values = HashMap::<i32, i32>::new();
    for index in 0..count {
        values.insert(index, index * 3 + 7);
    }
    let mut queried = 0_i64;
    for index in 0..count {
        let Some(value) = values.get(&index) else { return -1 };
        queried += i64::from(*value);
    }
    let mut replaced = 0_i64;
    for index in 0..count {
        let Some(previous) = values.insert(index, index * 3 + 11) else { return -1 };
        replaced += i64::from(previous);
    }
    let mut final_sum = 0_i64;
    for index in 0..count {
        let Some(value) = values.get(&index) else { return -1 };
        final_sum += i64::from(*value);
    }
    let mut removed = 0_i64;
    for index in 0..count {
        let Some(value) = values.remove(&index) else { return -1 };
        removed += i64::from(value);
    }
    queried + replaced * 3 + final_sum * 5 + removed * 7 + values.len() as i64
}

struct SudokuSolver {
    cells: Vec<i32>,
}

impl SudokuSolver {
    fn new(puzzle: &str) -> Self {
        Self { cells: puzzle.bytes().map(|byte| i32::from(byte - b'0')).collect() }
    }

    fn can_place(&self, index: usize, value: i32) -> bool {
        let row = index / 9;
        let column = index % 9;
        for offset in 0..9 {
            let row_cell = row * 9 + offset;
            let column_cell = offset * 9 + column;
            let box_cell = (row / 3 * 3 + offset / 3) * 9 + column / 3 * 3 + offset % 3;
            if row_cell != index && self.cells[row_cell] == value {
                return false;
            }
            if column_cell != index && self.cells[column_cell] == value {
                return false;
            }
            if box_cell != index && self.cells[box_cell] == value {
                return false;
            }
        }
        true
    }

    fn search(&mut self, start: usize) -> bool {
        for index in start..81 {
            if self.cells[index] == 0 {
                for value in 1..=9 {
                    if self.can_place(index, value) {
                        self.cells[index] = value;
                        if self.search(index + 1) {
                            return true;
                        }
                        self.cells[index] = 0;
                    }
                }
                return false;
            }
        }
        true
    }

    fn solve(&mut self) -> bool {
        self.search(0)
    }

    fn checksum(&self) -> i64 {
        self.cells.iter().enumerate().map(|(index, value)| i64::from(*value) * (index as i64 + 1)).sum()
    }
}

fn sudoku_kernel() -> i64 {
    let mut solver = SudokuSolver::new(SUDOKU_PUZZLE);
    if !solver.solve() { -1 } else { solver.checksum() }
}

struct LifeSim {
    grid: Vec<i32>,
    next: Vec<i32>,
}

impl LifeSim {
    fn new() -> Self {
        let mut grid = Vec::with_capacity(10_000);
        let mut next = Vec::with_capacity(10_000);
        for index in 0..10_000 {
            let alive = if (index * 17 + 11) % 7 == 0 || (index * 31 + 5) % 11 == 0 { 1 } else { 0 };
            grid.push(alive);
            next.push(0);
        }
        Self { grid, next }
    }

    fn neighbors(&self, row: usize, column: usize) -> i32 {
        let up = (row + 99) % 100;
        let down = (row + 101) % 100;
        let left = (column + 99) % 100;
        let right = (column + 101) % 100;
        self.grid[up * 100 + left]
            + self.grid[up * 100 + column]
            + self.grid[up * 100 + right]
            + self.grid[row * 100 + left]
            + self.grid[row * 100 + right]
            + self.grid[down * 100 + left]
            + self.grid[down * 100 + column]
            + self.grid[down * 100 + right]
    }

    fn step(&mut self) {
        for row in 0..100 {
            for column in 0..100 {
                let index = row * 100 + column;
                let current = self.grid[index];
                let adjacent = self.neighbors(row, column);
                self.next[index] = if current != 0 {
                    if adjacent == 2 || adjacent == 3 { 1 } else { 0 }
                } else if adjacent == 3 {
                    1
                } else {
                    0
                };
            }
        }
        self.grid.copy_from_slice(&self.next);
    }

    fn live_count(&self) -> i32 {
        self.grid.iter().filter(|&&value| value != 0).count() as i32
    }
}

fn life_kernel() -> i64 {
    let mut simulation = LifeSim::new();
    for _ in 0..100 {
        simulation.step();
    }
    i64::from(simulation.live_count())
}

fn run_kernel(name: &str) -> i64 {
    match name {
        "json" => json_kernel(),
        "string" => string_kernel(),
        "binary" => binary_kernel(),
        "hashmap" => hashmap_kernel(),
        "sudoku" => sudoku_kernel(),
        "game_of_life" => life_kernel(),
        _ => -1,
    }
}

fn report_results(name: &str, repetitions: usize) {
    let checksum = (0..repetitions).fold(0_i64, |sum, _| sum.wrapping_add(run_kernel(name)));
    println!("RESULT {name} {checksum}");
}

fn benchmark_workload(name: &str) {
    for _ in 0..3 {
        black_box(run_kernel(name));
    }
    let mut iterations = 1_u64;
    loop {
        let start = Instant::now();
        for _ in 0..iterations {
            black_box(run_kernel(name));
        }
        if start.elapsed() >= Duration::from_millis(20) || iterations >= 100_000_000 {
            break;
        }
        iterations *= 10;
    }
    let mut samples = Vec::with_capacity(5);
    let mut checksum = 0_i64;
    for _ in 0..5 {
        let start = Instant::now();
        for _ in 0..iterations {
            checksum = checksum.wrapping_add(black_box(run_kernel(name)));
        }
        samples.push((start.elapsed().as_nanos() / u128::from(iterations)) as u64);
    }
    let mean_ns = samples.iter().sum::<u64>() / samples.len() as u64;
    let max_diff = samples.iter().map(|sample| sample.abs_diff(mean_ns)).max().unwrap_or(0);
    let ops_per_second = if mean_ns == 0 { 0 } else { 1_000_000_000_u64 / mean_ns };
    for sample in &samples {
        println!("BENCH_SAMPLE: {name} {sample}");
    }
    println!("BENCH_RESULT: {name} {mean_ns} {max_diff} {ops_per_second}");
    println!("BENCH_CHECKSUM: {name} {}", black_box(run_kernel(name)));
    black_box(checksum);
}

fn main() {
    let mut args = std::env::args().skip(1);
    if let Some(first) = args.next() {
        if first == "bench" {
            for name in WORKLOADS {
                benchmark_workload(name);
            }
        } else {
            let repetitions = args.next().map(|value| value.parse::<usize>().expect("invalid repetition count")).unwrap_or(1);
            assert!((1..=2_000_000).contains(&repetitions));
            report_results(&first, repetitions);
        }
    } else {
        for name in WORKLOADS {
            report_results(name, 1);
        }
    }
    #[cfg(allocation_stats)]
    allocation_stats::report();
}

#[cfg(allocation_stats)]
mod allocation_stats {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::sync::atomic::{AtomicUsize, Ordering};

    static ALLOC_CALLS: AtomicUsize = AtomicUsize::new(0);
    static ZEROED_CALLS: AtomicUsize = AtomicUsize::new(0);
    static REALLOC_CALLS: AtomicUsize = AtomicUsize::new(0);
    static DEALLOC_CALLS: AtomicUsize = AtomicUsize::new(0);
    static LIVE_BLOCKS: AtomicUsize = AtomicUsize::new(0);
    static ALLOC_BYTES: AtomicUsize = AtomicUsize::new(0);
    static REALLOC_BYTES: AtomicUsize = AtomicUsize::new(0);
    static DEALLOC_BYTES: AtomicUsize = AtomicUsize::new(0);

    struct CountingSystem;

    unsafe impl GlobalAlloc for CountingSystem {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let pointer = unsafe { System.alloc(layout) };
            if !pointer.is_null() {
                ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
                LIVE_BLOCKS.fetch_add(1, Ordering::Relaxed);
                ALLOC_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
            }
            pointer
        }

        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            let pointer = unsafe { System.alloc_zeroed(layout) };
            if !pointer.is_null() {
                ZEROED_CALLS.fetch_add(1, Ordering::Relaxed);
                LIVE_BLOCKS.fetch_add(1, Ordering::Relaxed);
                ALLOC_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
            }
            pointer
        }

        unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
            DEALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
            LIVE_BLOCKS.fetch_sub(1, Ordering::Relaxed);
            DEALLOC_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
            unsafe { System.dealloc(pointer, layout) };
        }

        unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            let result = unsafe { System.realloc(pointer, layout, new_size) };
            if !result.is_null() {
                REALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
                REALLOC_BYTES.fetch_add(new_size, Ordering::Relaxed);
            }
            result
        }
    }

    #[global_allocator]
    static ALLOCATOR: CountingSystem = CountingSystem;

    pub fn report() {
        let alloc_calls = ALLOC_CALLS.load(Ordering::Relaxed);
        let zeroed_calls = ZEROED_CALLS.load(Ordering::Relaxed);
        let realloc_calls = REALLOC_CALLS.load(Ordering::Relaxed);
        let dealloc_calls = DEALLOC_CALLS.load(Ordering::Relaxed);
        let allocation_events = alloc_calls + zeroed_calls + realloc_calls;
        let live_blocks = LIVE_BLOCKS.load(Ordering::Relaxed);
        let alloc_bytes = ALLOC_BYTES.load(Ordering::Relaxed);
        let realloc_bytes = REALLOC_BYTES.load(Ordering::Relaxed);
        let dealloc_bytes = DEALLOC_BYTES.load(Ordering::Relaxed);
        eprintln!(
            "RUST_BENCH_MEMORY allocation_events={allocation_events} alloc={alloc_calls} zeroed={zeroed_calls} realloc={realloc_calls} dealloc={dealloc_calls} live={live_blocks} alloc_bytes={alloc_bytes} realloc_bytes={realloc_bytes} dealloc_bytes={dealloc_bytes}"
        );
    }
}
