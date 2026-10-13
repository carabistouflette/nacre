use nacre::{compile_to_c, native};

fn assert_native(source: &str, stdout: &[u8]) {
    let c = compile_to_c(source)
        .unwrap_or_else(|error| panic!("{}", error.render("collections_ordered.ncr", source)));
    let output = native::compile_c_with_flags(&c, &["-DNACRE_MEMORY_STATS"])
        .unwrap().output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, stdout);
    let mut fields = std::str::from_utf8(&output.stderr).unwrap().split_whitespace();
    assert_eq!(fields.next(), Some("NACRE_MEMORY"));
    let allocations = fields.next().unwrap().strip_prefix("allocations=").unwrap();
    let frees = fields.next().unwrap().strip_prefix("frees=").unwrap();
    assert_eq!(allocations, frees);
    assert_eq!(fields.next(), Some("live=0"));
    assert_eq!(fields.next(), None);
}

fn assert_native_failure(source: &str) {
    let c = compile_to_c(source)
        .unwrap_or_else(|error| panic!("{}", error.render("collections_ordered_failure.ncr", source)));
    let output = native::compile_c_with_flags(&c, &[]).unwrap().output().unwrap();
    assert_eq!(output.status.code(), Some(70), "{}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn ordered_collections_and_priority_queue_use_owned_comparators() {
    assert_native(r#"
class Entry {
    private int rank;
    private int identity;
    private Text payload;
    Entry(int rank, int identity) {
        this.rank = rank;
        this.identity = identity;
        this.payload = new Text("entry");
    }
    public readonly int rankValue() { return this.rank; }
    public readonly int identityValue() { return this.identity; }
}
class EntryOrder implements Comparator<Entry> {
    private Text payload;
    EntryOrder() { this.payload = new Text("order"); }
    public readonly int compare(borrow Entry left, borrow Entry right) {
        if (left.rankValue() < right.rankValue()) { return -1; }
        if (left.rankValue() > right.rankValue()) { return 1; }
        return 0;
    }
}
class IntOrder implements Comparator<int> {
    public readonly int compare(int left, int right) {
        if (left < right) { return -1; }
        if (left > right) { return 1; }
        return 0;
    }
}
void main() {
    OrderedMap<Entry, int> map = new OrderedMap<Entry, int>(new EntryOrder());
    map.put(new Entry(3, 30), 300);
    map.put(new Entry(1, 10), 100);
    map.put(new Entry(2, 20), 200);
    map.put(new Entry(2, 99), 999);
    println(map.size());
    Entry query = new Entry(2, 0);
    println(map.contains(query));
    match (map.tryGet(query)) { None { println("none"); } Some(value) { println(value); } }
    {
        borrow Entry first = map.keyAt(0);
        println(first.rankValue());
    }
    {
        borrow Entry second = map.keyAt(1);
        println(second.identityValue());
    }
    match (map.floorIndex(query)) { None { println("none"); } Some(index) { println(index); } }
    match (map.ceilingIndex(query)) { None { println("none"); } Some(index) { println(index); } }
    match (map.lowerIndex(query)) { None { println("none"); } Some(index) { println(index); } }
    match (map.higherIndex(query)) { None { println("none"); } Some(index) { println(index); } }
    match (map.tryRemove(query)) { None { println("none"); } Some(value) { println(value); } }
    println(map.size());
    match (map.tryGet(query)) { None { println("none"); } Some(value) { println(value); } }
    map.clear();
    println(map.size());
    OrderedMap<int, int> scalarMap = new OrderedMap<int, int>(new IntOrder());
    scalarMap.put(5, 50); scalarMap.put(1, 10); scalarMap.put(3, 30);
    println(scalarMap.size());
    println(scalarMap.keyAt(0));
    println(scalarMap.valueAt(1));
    match (scalarMap.floorIndex(2)) { None { println("none"); } Some(index) { println(index); } }
    match (scalarMap.ceilingIndex(2)) { None { println("none"); } Some(index) { println(index); } }
    match (scalarMap.lowerIndex(2)) { None { println("none"); } Some(index) { println(index); } }
    match (scalarMap.higherIndex(3)) { None { println("none"); } Some(index) { println(index); } }
    println(scalarMap.contains(4));
    match (scalarMap.tryGet(3)) { None { println("none"); } Some(value) { println(value); } }
    match (scalarMap.put(3, 33)) { None { println("none"); } Some(value) { println(value); } }
    match (scalarMap.tryRemove(3)) { None { println("none"); } Some(value) { println(value); } }
    println(scalarMap.size());
    scalarMap.clear();

    OrderedSet<Entry> set = new OrderedSet<Entry>(new EntryOrder());
    set.add(new Entry(7, 70));
    set.add(new Entry(2, 20));
    set.add(new Entry(7, 99));
    println(set.size());
    Entry setQuery = new Entry(7, 0);
    println(set.contains(setQuery));
    println(set.remove(setQuery));
    println(set.remove(setQuery));
    println(set.size());
    set.clear();
    println(set.isEmpty());
    OrderedSet<int> scalarSet = new OrderedSet<int>(new IntOrder());
    scalarSet.add(7); scalarSet.add(2); scalarSet.add(7);
    println(scalarSet.size());
    println(scalarSet.contains(7));
    println(scalarSet.remove(2));
    println(scalarSet.size());
    scalarSet.clear();
    println(scalarSet.isEmpty());

    PriorityQueue<Entry> queue = new PriorityQueue<Entry>(new EntryOrder());
    queue.add(new Entry(5, 5));
    queue.add(new Entry(1, 1));
    queue.add(new Entry(4, 4));
    queue.add(new Entry(2, 2));
    queue.add(new Entry(3, 3));
    {
        borrow Entry root = queue.getRef();
        println(root.rankValue());
    }
    while (!queue.isEmpty()) {
        Entry value = queue.pop();
        println(value.rankValue());
    }
    match (queue.tryPop()) { None { println("empty"); } Some(value) { println(value.rankValue()); } }
    queue.add(new Entry(9, 9));
    queue.add(new Entry(8, 8));
    queue.clear();
    println(queue.isEmpty());
    PriorityQueue<int> scalarQueue = new PriorityQueue<int>(new IntOrder());
    scalarQueue.add(8); scalarQueue.add(2); scalarQueue.add(5);
    println(scalarQueue.size());
    println(scalarQueue.peek());
    match (scalarQueue.tryPeek()) { None { println("none"); } Some(value) { println(value); } }
    match (scalarQueue.tryPop()) { None { println("none"); } Some(value) { println(value); } }
    println(scalarQueue.peek());
    scalarQueue.clear();
    match (scalarQueue.tryPeek()) { None { println("none"); } Some(value) { println(value); } }
}
"#, b"3\ntrue\n999\n1\n20\n1\n1\n0\n2\n999\n2\nnone\n0\n3\n1\n30\n0\n1\n0\n2\nfalse\n30\n30\n33\n2\n2\ntrue\ntrue\nfalse\n1\ntrue\n2\ntrue\ntrue\n1\ntrue\n1\n1\n2\n3\n4\n5\nempty\ntrue\n3\n2\n2\n2\n5\nnone\n");
}

#[test]
fn bitset_handles_word_boundaries_ranges_and_independent_logical_copies() {
    assert_native(r#"
void main() {
    BitSet bits = new BitSet(65L);
    println(bits.isEmpty());
    bits.setRange(62L, 66L);
    println(bits.cardinality());
    println(bits.length());
    println(bits.get(63L));
    match (bits.nextSetBit(63L)) { None { println("none"); } Some(index) { println(index); } }
    bits.flip(63L);
    bits.clearRange(64L, 65L);
    println(bits.cardinality());
    println(bits.length());
    match (bits.nextSetBit(64L)) { None { println("none"); } Some(index) { println(index); } }
    match (bits.nextSetBit(66L)) { None { println("none"); } Some(index) { println(index); } }

    BitSet copy = bits.clone();
    copy.flipRange(0L, 64L);
    BitSet mask = new BitSet(0L);
    mask.setRange(60L, 67L);
    copy.and(mask);
    println(copy.cardinality());
    mask.andNot(bits);
    println(mask.cardinality());
    println(bits.cardinality());
    BitSet empty = new BitSet(0L);
    empty.xor(mask);
    println(empty.cardinality());
    empty.andNot(mask);
    println(empty.isEmpty());
    empty.or(bits);
    empty.or(mask);
    println(empty.cardinality() == 7L && empty.length() == 67L && empty.get(62L) && empty.get(66L));
    bits.set(128L);
    empty.or(bits);
    println(empty.cardinality() == 8L && empty.length() == 129L && empty.get(128L) && !mask.get(128L));
}
"#, b"true\n4\n66\ntrue\n63\n2\n66\n65\nnone\n4\n5\n2\n5\ntrue\ntrue\ntrue\n");
}

#[test]
fn owned_iterators_transfer_drain_map_filter_take_skip_and_drop_remainders() {
    assert_native(r#"
class TimesTwo implements Mapper<int, int> {
    public int apply(int value) { return value * 2; }
}
class Even implements Predicate<int> {
    public readonly boolean test(int value) { return value % 2 == 0; }
}
class Owned {
    private int value;
    private Text payload;
    Owned(int value) { this.value = value; this.payload = new Text("owned"); }
    public readonly int getValue() { return this.value; }
}
void main() {
    List<int> input = new List<int>();
    input.add(1); input.add(2); input.add(3); input.add(4); input.add(5);
    Iterator<int> drained = input.drain();
    println(input.size());
    MapIterator<int, int> mapped = new MapIterator<int, int>(drained, new TimesTwo());
    FilterIterator<int> filtered = new FilterIterator<int>(mapped, new Even());
    Iterator<int> firstTwo = Iterators<int>.take(filtered, 2L);
    List<int> result = List<int>.collect(firstTwo);
    println(result.size());
    println(result.get(0));
    println(result.get(1));

    List<int> values = new List<int>();
    values.add(10); values.add(20); values.add(30); values.add(40);
    List<int> tail = List<int>.collect(Iterators<int>.skip(values.drain(), 2L));
    println(tail.get(0));
    println(tail.get(1));

    List<Owned> owners = new List<Owned>();
    owners.add(new Owned(7)); owners.add(new Owned(8)); owners.add(new Owned(9));
    Iterator<Owned> pending = owners.drain();
    match (pending.next()) {
        None { println("none"); }
        Some(item) { println(item.getValue()); }
    }
    drop(pending);
    println(owners.size());
}
"#, b"0\n2\n2\n4\n30\n40\n7\n0\n");
}

#[test]
fn bitset_and_iterator_count_preconditions_fail_at_runtime() {
    assert_native_failure("void main() { BitSet bits = new BitSet(-1L); }");
    assert_native_failure("void main() { BitSet bits = new BitSet(0L); bits.setRange(3L, 2L); }");
    assert_native_failure("void main() { List<int> values = new List<int>(); Iterator<int> input = values.drain(); drop(Iterators<int>.take(input, -1L)); }");
}

#[test]
fn hidden_owned_collection_and_iterator_state_must_be_sendable() {
    let sources = [
        "void main() { Channel<OrderedMap<int, Guard<int>>>.bounded(1L); }",
        "void main() { Channel<PriorityQueue<Guard<int>>>.bounded(1L); }",
        "void main() { Channel<ListIterator<Guard<int>>>.bounded(1L); }",
        r#"
class CapturingComparator implements Comparator<int> {
    private Guard<int> guard;
    public CapturingComparator(Guard<int> guard) { this.guard = guard; }
    public readonly int compare(int left, int right) { return left - right; }
}
void main() { Channel<OrderedMap<int, int>>.bounded(1L); }
"#,
        r#"
class CapturingMapper implements Mapper<int, int> {
    private Guard<int> guard;
    public CapturingMapper(Guard<int> guard) { this.guard = guard; }
    public int apply(int value) { return value; }
}
void main() { Channel<MapIterator<int, int>>.bounded(1L); }
"#,
        r#"
class CapturingPredicate implements Predicate<int> {
    private Guard<int> guard;
    public CapturingPredicate(Guard<int> guard) { this.guard = guard; }
    public readonly boolean test(int value) { return value > 0; }
}
void main() { Channel<FilterIterator<int>>.bounded(1L); }
"#,
    ];
    for source in sources {
        let error = nacre::validate(source).expect_err("non-sendable hidden payload accepted");
        assert!(error.message.contains("[E7101]"), "{}", error.render("hidden_payload.ncr", source));
    }
}
