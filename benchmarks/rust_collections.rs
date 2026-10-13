use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hash, Hasher};

const FNV_OFFSET: u32 = 2_166_136_261;
const FNV_PRIME: u32 = 16_777_619;

fn nacre_mix32(mut value: u32) -> u32 {
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846c_a68b);
    value ^= value >> 16;
    value
}

// These are the exact 32-bit hashes used by Nacre's native Map; the u64
// Hasher result repeats those bits to preserve them for Rust HashMap.
struct NacreHasher {
    state: u32,
}

impl Default for NacreHasher {
    fn default() -> Self {
        Self { state: FNV_OFFSET }
    }
}

impl NacreHasher {
    fn nacre_hash64(&self) -> u64 {
        let hash = u64::from(self.state);
        hash | (hash << 32)
    }
}

impl Hasher for NacreHasher {
    fn finish(&self) -> u64 {
        self.nacre_hash64()
    }

    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.state = (self.state ^ u32::from(byte)).wrapping_mul(FNV_PRIME);
        }
    }

    fn write_i32(&mut self, value: i32) {
        self.state = nacre_mix32(value as u32);
    }

    fn write_u32(&mut self, value: u32) {
        self.state = nacre_mix32(self.state ^ value);
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct NacreInt(i32);

impl Hash for NacreInt {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_i32(self.0);
    }
}

#[derive(Eq, PartialEq)]
struct TextKey(String);

impl Hash for TextKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(self.0.as_bytes());
        state.write_u32(self.0.len() as u32);
    }
}

type NacreMap<K, V> = HashMap<K, V, BuildHasherDefault<NacreHasher>>;
fn nacre_map<K, V>() -> NacreMap<K, V> {
    NacreMap::with_hasher(BuildHasherDefault::default())
}

fn run_list(count: i32) {
    let mut values = Vec::new();
    for index in 0..count {
        values.push(index);
    }

    let checksum = values
        .iter()
        .fold(0_i32, |sum, value| sum.wrapping_add(*value));
    println!("{checksum}");
    println!("{}", values.len());
}

fn run_integer_map(count: i32) {
    let mut values = nacre_map::<NacreInt, i32>();
    for index in 0..count {
        values.insert(NacreInt(index), index.wrapping_add(17));
    }

    let mut queried = 0_i32;
    for index in 0..count {
        let value = values
            .get(&NacreInt(index))
            .expect("inserted integer key is missing");
        queried = queried.wrapping_add(*value);
    }

    let mut removed = 0_i32;
    for index in 0..count {
        let value = values
            .remove(&NacreInt(index))
            .expect("inserted integer key could not be removed");
        removed = removed.wrapping_add(value);
    }
    println!("{queried}");
    println!("{removed}");
    println!("{}", values.len());
}

fn text_key(index: i32) -> TextKey {
    TextKey(format!("bench-key-{index}"))
}

fn run_text_map(count: i32) {
    let mut values = nacre_map::<TextKey, i32>();
    for index in 0..count {
        values.insert(text_key(index), index.wrapping_add(1));
    }

    let mut queried = 0_i32;
    for index in 0..count {
        let value = values
            .get(&text_key(index))
            .expect("inserted text key is missing");
        queried = queried.wrapping_add(*value);
    }

    let mut replaced = 0_i32;
    for index in 0..count {
        let previous = values
            .insert(text_key(index), index.wrapping_add(2))
            .expect("text replacement did not return the old value");
        replaced = replaced.wrapping_add(previous);
    }

    let mut final_values = 0_i32;
    for index in 0..count {
        let value = values
            .get(&text_key(index))
            .expect("replaced text key is missing");
        final_values = final_values.wrapping_add(*value);
    }
    println!("{queried}");
    println!("{replaced}");
    println!("{final_values}");
    println!("{}", values.len());
}

fn run_empty_text_miss(query_count: i32, query: String) {
    let values = nacre_map::<TextKey, i32>();
    let query = TextKey(query);
    let mut found = 0_i32;
    for _ in 0..query_count {
        if values.contains_key(&query) {
            found = found.wrapping_add(1);
        }
    }
    println!("{found}");
}

fn run_benchmark() {
    let mut arguments = std::env::args().skip(1);
    let mode = arguments.next().expect("missing kernel name");
    let count = arguments
        .next()
        .expect("missing collection size")
        .parse::<i32>()
        .expect("invalid collection size");
    let query_count = arguments
        .next()
        .expect("missing query count")
        .parse::<i32>()
        .expect("invalid query count");

    match mode.as_str() {
        "list" => run_list(count),
        "int-map" => run_integer_map(count),
        "text-map" => run_text_map(count),
        "empty-text-miss" => {
            let query = arguments.next().expect("missing large text query");
            run_empty_text_miss(query_count, query);
        }
        _ => panic!("unknown kernel"),
    }
}

fn main() {
    run_benchmark();
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
