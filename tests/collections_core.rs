use nacre::validate;
mod support;

fn assert_native(source: &str, stdout: &[u8]) {
    let output = support::compile(source, &["-DNACRE_MEMORY_STATS"])
        .unwrap_or_else(|error| panic!("native compile failed: {error}"))
        .output().unwrap();
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
    let output = support::compile(source, &[])
        .unwrap_or_else(|error| panic!("native compile failed: {error}"))
        .output().unwrap();
    assert_eq!(output.status.code(), Some(70), "{}", String::from_utf8_lossy(&output.stderr));
}

fn assert_rejected_with_code(source: &str, code: &str) {
    let error = validate(source).expect_err("invalid collection program was accepted");
    assert!(error.message.contains(code), "{}", error.render("collections_alpha_invalid.ncr", source));
}

const AFFINE_ORDERING: &str = r#"
class Item {
    private int amount;
    private Text marker;
    Item(int amount) { this.amount = amount; this.marker = new Text("item"); }
    public readonly int value() { return this.amount; }
}
class ItemOrder implements Comparator<Item> {
    private Text marker;
    ItemOrder() { this.marker = new Text("comparator"); }
    public readonly int compare(borrow Item left, borrow Item right) {
        if (left.value() < right.value()) { return -1; }
        if (left.value() > right.value()) { return 1; }
        return 0;
    }
}
void main() {
    List<Item> values = new List<Item>();
    values.add(new Item(5)); values.add(new Item(1)); values.add(new Item(4));
    values.add(new Item(2)); values.add(new Item(3));
    values.sortBy(new ItemOrder());
    for (borrow Item value : values) { println(value.value()); }
    Item query = new Item(3);
    match (values.binarySearchBy(new ItemOrder(), query)) {
        None {} Some(index) { println(index); }
    }
    Item missing = new Item(6);
    match (values.binarySearchBy(new ItemOrder(), missing)) {
        None { println("none"); } Some(index) { println(index); }
    }
    List<Item> empty = new List<Item>();
    match (empty.binarySearchBy(new ItemOrder(), query)) {
        None { println("none"); } Some(index) { println(index); }
    }
}
"#;

#[test]
fn int_and_text_sets_handle_duplicates_collisions_nul_and_reuse() {
    assert_native(r#"
void main() {
    Set<int> numbers = new Set<int>();
    println(numbers.size());
    println(numbers.isEmpty());
    println(numbers.contains(0));
    println(numbers.remove(0));
    numbers.add(-97119); numbers.add(-96755); numbers.add(-96323); numbers.add(-95065);
    numbers.add(-97119);
    println(numbers.size());
    println(numbers.contains(-97119));
    println(numbers.remove(-97119));
    println(numbers.remove(-96323));
    println(numbers.remove(-95065));
    println(numbers.contains(-96755));
    println(numbers.size());
    numbers.add(-97119);
    int sum = 0;
    for (int value : numbers) { sum = sum + value; }
    println(sum);
    numbers.clear();
    println(numbers.isEmpty());

    Set<Text> labels = new Set<Text>();
    labels.add(new Text("key0")); labels.add(new Text("key6"));
    labels.add(new Text("key11")); labels.add(new Text("key32"));
    labels.add(new Text("key46")); labels.add(new Text("key47"));
    labels.add(new Text("key6"));
    Text collisionQuery = new Text("key46");
    println(labels.size());
    println(labels.contains(collisionQuery));
    Text firstCollision = new Text("key0");
    println(labels.remove(firstCollision));
    Text lastCollision = new Text("key47");
    println(labels.contains(lastCollision));

    labels.add(new Text("a\0b"));
    labels.add(new Text("a\0c"));
    labels.add(new Text("a"));
    Text nulQuery = new Text("a\0b");
    println(labels.contains(nulQuery));
    println(labels.remove(nulQuery));
    println(labels.contains(nulQuery));
    int bytes = 0;
    for (borrow Text label : labels) { bytes = bytes + label.byteLength(); }
    println(bytes);
    labels.clear();
    println(labels.isEmpty());
}
    "#, b"0\ntrue\nfalse\nfalse\n4\ntrue\ntrue\ntrue\ntrue\ntrue\n1\n-193874\ntrue\n6\ntrue\ntrue\ntrue\ntrue\ntrue\nfalse\n28\ntrue\n");
}

#[test]
fn deque_checks_indices_wraps_grows_and_transfers_affine_values() {
    assert_native(r#"
class Box {
    private int amount;
    private Text marker;
    Box(int amount) { this.amount = amount; this.marker = new Text("owned"); }
    public readonly int value() { return this.amount; }
    public void increment() { this.amount = this.amount + 1; }
}
void main() {
    Deque<int> values = new Deque<int>();
    println(values.isEmpty());
    values.reserve(3);
    println(values.capacity() >= 3);
    match (values.tryPopFront()) { None { println("none"); } Some(value) { println(value); } }
    match (values.tryPopBack()) { None { println("none"); } Some(value) { println(value); } }
    match (values.tryGet(-1)) { None { println("none"); } Some(value) { println(value); } }
    values.pushBack(1); values.pushBack(2); values.pushBack(3); values.pushBack(4);
    println(values.popFront()); println(values.popFront());
    values.pushBack(5); values.pushBack(6);
    println(values.size());
    println(values.get(0)); println(values.get(1)); println(values.get(2)); println(values.get(3));
    values.pushFront(2);
    values.set(2, 40);
    match (values.tryGet(-1)) { None { println("none"); } Some(value) { println(value); } }
    match (values.tryGet(2)) { None { println("none"); } Some(value) { println(value); } }
    match (values.tryGet(5)) { None { println("none"); } Some(value) { println(value); } }
    println(values.popBack()); println(values.popFront());
    match (values.tryPopFront()) { None { println("none"); } Some(value) { println(value); } }
    match (values.tryPopBack()) { None { println("none"); } Some(value) { println(value); } }
    println(values.size()); println(values.get(0));
    values.clear();
    println(values.size()); println(values.capacity() >= 5);
    match (values.tryPopBack()) { None { println("none"); } Some(value) { println(value); } }
    values.pushFront(99);
    println(values.popBack());

    Deque<Box> boxes = new Deque<Box>();
    boxes.pushBack(new Box(1)); boxes.pushBack(new Box(2)); boxes.pushBack(new Box(3));
    boxes.pushBack(new Box(4)); boxes.pushBack(new Box(5));
    Box removed = boxes.popFront();
    println(removed.value()); drop(removed);
    {
        borrow Box first = boxes.getRef(0);
        println(first.value());
    }
    {
        borrow mut Box second = boxes.getMut(1);
        second.increment();
        println(second.value());
    }
    boxes.set(3, new Box(99));
    match (boxes.tryPopFront()) { None {} Some(value) { println(value.value()); drop(value); } }
    match (boxes.tryPopBack()) { None {} Some(value) { println(value.value()); drop(value); } }
    boxes.clear();
    println(boxes.size());
}
"#, b"true\ntrue\nnone\nnone\nnone\n1\n2\n4\n3\n4\n5\n6\nnone\n40\nnone\n6\n2\n3\n5\n1\n40\n0\ntrue\nnone\n99\n1\n2\n4\n2\n99\n0\n");
}

#[test]
fn default_list_ordering_is_total_and_binary_search_returns_lower_bound() {
    assert_native(r#"
void main() {
    List<int> numbers = new List<int>();
    numbers.add(5); numbers.add(1); numbers.add(4); numbers.add(1); numbers.add(3);
    numbers.sort();
    for (int value : numbers) { println(value); }
    match (numbers.binarySearch(1)) { None {} Some(index) { println(index); } }
    match (numbers.binarySearch(2)) { None { println("none"); } Some(index) { println(index); } }

    List<long> wide = new List<long>();
    wide.add(9223372036854775807L); wide.add(0L); wide.add(-9223372036854775808L);
    wide.sort();
    for (long value : wide) { println(value); }

    List<byte> bytes = new List<byte>();
    bytes.add((byte)255); bytes.add((byte)1); bytes.add((byte)0);
    bytes.sort();
    for (byte value : bytes) { println(value); }

    List<char> letters = new List<char>();
    letters.add('z'); letters.add('a'); letters.sort();
    for (char value : letters) { println(value); }

    List<boolean> flags = new List<boolean>();
    flags.add(true); flags.add(false); flags.sort();
    for (boolean value : flags) { println(value); }

    List<String> words = new List<String>();
    words.add("b"); words.add("a\0b"); words.add("a"); words.add("a\0a");
    words.sort();
    match (words.binarySearch("a")) { None {} Some(index) { println(index); } }
    match (words.binarySearch("a\0a")) { None {} Some(index) { println(index); } }
    match (words.binarySearch("a\0b")) { None {} Some(index) { println(index); } }
    match (words.binarySearch("b")) { None {} Some(index) { println(index); } }

    List<double> zeros = new List<double>();
    zeros.add(0.0); zeros.add(-0.0); zeros.sort();
    match (zeros.binarySearch(-0.0)) { None {} Some(index) { println(index); } }
    match (zeros.binarySearch(0.0)) { None {} Some(index) { println(index); } }
    double nan = 0.0 / 0.0;
    double negativeInfinity = -1.0 / 0.0;
    double positiveInfinity = 1.0 / 0.0;
    List<double> special = new List<double>();
    special.add(1.0); special.add(nan); special.add(negativeInfinity);
    special.add(positiveInfinity); special.add(-0.0); special.add(0.0); special.add(-1.0);
    special.sort();
    int nanIndex = -1; int negativeInfinityIndex = -1; int positiveInfinityIndex = -1;
    int negativeOneIndex = -1; int negativeZeroIndex = -1; int positiveZeroIndex = -1; int oneIndex = -1;
    match (special.binarySearch(negativeInfinity)) { None {} Some(index) { negativeInfinityIndex = index; } }
    match (special.binarySearch(-1.0)) { None {} Some(index) { negativeOneIndex = index; } }
    match (special.binarySearch(-0.0)) { None {} Some(index) { negativeZeroIndex = index; } }
    match (special.binarySearch(0.0)) { None {} Some(index) { positiveZeroIndex = index; } }
    match (special.binarySearch(1.0)) { None {} Some(index) { oneIndex = index; } }
    match (special.binarySearch(positiveInfinity)) { None {} Some(index) { positiveInfinityIndex = index; } }
    match (special.binarySearch(nan)) { None {} Some(index) { nanIndex = index; } }
    boolean negativeNaNFirst = nanIndex == 0 && negativeInfinityIndex == 1 && negativeOneIndex == 2
        && negativeZeroIndex == 3 && positiveZeroIndex == 4 && oneIndex == 5 && positiveInfinityIndex == 6;
    boolean positiveNaNLast = nanIndex == 6 && negativeInfinityIndex == 0 && negativeOneIndex == 1
        && negativeZeroIndex == 2 && positiveZeroIndex == 3 && oneIndex == 4 && positiveInfinityIndex == 5;
    println(negativeNaNFirst || positiveNaNLast);
    List<int> empty = new List<int>();
    match (empty.binarySearch(1)) { None { println("none"); } Some(index) { println(index); } }
}
    "#, b"1\n1\n3\n4\n5\n0\nnone\n-9223372036854775808\n0\n9223372036854775807\n0\n1\n255\na\nz\nfalse\ntrue\n0\n1\n2\n3\n0\n1\ntrue\nnone\n");
}

#[test]
fn affine_comparator_sort_and_binary_search_borrow_values_and_drop_state() {
    assert_native(AFFINE_ORDERING, b"1\n2\n3\n4\n5\n2\nnone\nnone\n");
}

#[test]
fn collection_borrows_and_unsupported_specializations_are_rejected() {
    assert_rejected_with_code(
        "void main() { Set<Text> values = new Set<Text>(); for (borrow mut Text key : values) { } }",
        "E3003",
    );
    assert_rejected_with_code(
        "void main() { Set<Text> values = new Set<Text>(); for (borrow Text key : values) { values.add(new Text(\"next\")); } }",
        "E3001",
    );
    assert_rejected_with_code(
        "void main() { Deque<int> values = new Deque<int>(); values.pushBack(1); for (int item : values) { values.pushBack(2); } }",
        "E3001",
    );
    assert_rejected_with_code(
        "class Box {} void main() { Deque<Box> values = new Deque<Box>(); values.get(0); }",
        "E5002",
    );
    validate("void main() { Set<boolean> values = new Set<boolean>(); }").unwrap();
    assert!(validate("class Box {} void main() { List<Box> values = new List<Box>(); values.sort(); }").is_err());
}



#[test]
fn deque_capacity_empty_pop_and_index_failures_are_checked() {
    assert_native_failure(
        "void main() { Deque<int> values = new Deque<int>(); values.reserve(-1); }",
    );
    assert_native_failure(
        "void main() { Deque<int> values = new Deque<int>(); values.popBack(); }",
    );
    assert_native_failure(
        "void main() { Deque<int> values = new Deque<int>(); values.pushBack(1); values.get(1); }",
    );
}

#[test]
fn collections_example_runs_as_a_native_consumer_program() {
    assert_native(
        include_str!("../examples/collections/set_deque_sort.ncr"),
        b"true\n11\ntest\nmerge\nreview\n3\n5\n8\n1\n",
    );
}

#[test]
fn native_collection_cleanup_passes_address_and_undefined_sanitizers() {
    let source = r#"
class Box {
    private int value;
    private Text marker;
    Box(int value) { this.value = value; this.marker = new Text("box"); }
    public readonly int read() { return this.value; }
}
class BoxOrder implements Comparator<Box> {
    private Text marker;
    BoxOrder() { this.marker = new Text("order"); }
    public readonly int compare(borrow Box left, borrow Box right) {
        if (left.read() < right.read()) { return -1; }
        if (left.read() > right.read()) { return 1; }
        return 0;
    }
}
void main() {
    Set<Text> names = new Set<Text>();
    names.add(new Text("a\0b")); names.add(new Text("a\0c"));
    Text key = new Text("a\0b"); names.remove(key); names.clear();
    Deque<Box> queue = new Deque<Box>();
    queue.pushFront(new Box(2)); queue.pushBack(new Box(1));
    Box first = queue.popFront(); drop(first);
    queue.pushBack(new Box(3)); queue.clear();
    List<Box> values = new List<Box>();
    values.add(new Box(3)); values.add(new Box(1)); values.add(new Box(2));
    values.sortBy(new BoxOrder());
    Box query = new Box(2); values.binarySearchBy(new BoxOrder(), query);
}
"#;
    let output = support::compile(
        source,
        &["-fsanitize=address,undefined", "-fno-omit-frame-pointer"],
    )
    .unwrap_or_else(|error| panic!("native compile failed: {error}"))
    .output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("AddressSanitizer"), "{stderr}");
    assert!(!stderr.contains("runtime error:"), "{stderr}");
}

#[test]
fn enum_comparator_preserves_owned_payloads_and_searches_duplicate_lower_bound() {
    assert_native(r#"
enum Entry { Value(int, Text); }
int rank(borrow Entry entry) {
    match (entry) { Value(number, label) { return number; } }
}
class EntryOrder implements Comparator<Entry> {
    public readonly int compare(borrow Entry left, borrow Entry right) {
        if (rank(left) < rank(right)) { return -1; }
        if (rank(left) > rank(right)) { return 1; }
        return 0;
    }
}
void main() {
    List<Entry> entries = new List<Entry>();
    entries.add(new Entry.Value(3, new Text("three")));
    entries.add(new Entry.Value(1, new Text("one")));
    entries.add(new Entry.Value(2, new Text("two")));
    entries.add(new Entry.Value(2, new Text("second two")));
    entries.sortBy(new EntryOrder());
    for (borrow Entry entry : entries) { println(rank(entry)); }
    Entry query = new Entry.Value(2, new Text("query"));
    match (entries.binarySearchBy(new EntryOrder(), query)) {
        None { println(-1); } Some(index) { println(index); }
    }
    println(rank(query));
    int payloadBytes = 0;
    for (borrow Entry entry : entries) {
        match (entry) { Value(number, label) { payloadBytes = payloadBytes + label.byteLength(); } }
    }
    println(payloadBytes);
}
    "#, b"1\n2\n2\n3\n1\n2\n21\n");
}

#[test]
fn affine_deque_wrapped_growth_and_mutable_traversal_preserve_logical_order() {
    assert_native(r#"
class Box {
    private int number;
    private Text marker;
    Box(int number) { this.number = number; this.marker = new Text("owned"); }
    public readonly int value() { return this.number; }
    public void increase() { this.number = this.number + 10; }
}
void main() {
    Deque<Box> queue = new Deque<Box>();
    queue.reserve(4);
    int originalCapacity = queue.capacity();
    int index = 0;
    while (index < originalCapacity) { queue.pushBack(new Box(index)); index = index + 1; }
    Box first = queue.popFront();
    Box second = queue.popFront();
    println(first.value()); println(second.value());
    queue.pushBack(new Box(originalCapacity));
    queue.pushBack(new Box(originalCapacity + 1));
    queue.pushBack(new Box(originalCapacity + 2));
    for (borrow mut Box item : queue) { item.increase(); }
    index = 2;
    boolean ordered = true;
    for (borrow Box item : queue) {
        ordered = ordered && item.value() == index + 10;
        index = index + 1;
    }
    println(ordered);
    println(index == originalCapacity + 3);
    queue.clear();
    println(queue.isEmpty());
}
    "#, b"0\n1\ntrue\ntrue\ntrue\n");
}
