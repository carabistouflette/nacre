use nacre::{compile_to_c, native, validate};

fn execute_native(source: &str) -> (std::process::Output, usize) {
    let c = compile_to_c(source)
        .unwrap_or_else(|error| panic!("{}", error.render("map_traversal.ncr", source)));
    let output = native::compile_c_with_flags(&c, &["-DNACRE_MEMORY_STATS"])
        .unwrap().output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut fields = stderr.split_whitespace();
    assert_eq!(fields.next(), Some("NACRE_MEMORY"));
    let allocations = fields.next().unwrap().strip_prefix("allocations=").unwrap().parse::<usize>().unwrap();
    let frees = fields.next().unwrap().strip_prefix("frees=").unwrap().parse::<usize>().unwrap();
    assert_eq!(allocations, frees);
    assert_eq!(fields.next(), Some("live=0"));
    assert_eq!(fields.next(), None);
    (output, allocations)
}

fn assert_native(source: &str, stdout: &[u8]) {
    assert_eq!(execute_native(source).0.stdout, stdout);
}

fn allocation_count(source: &str) -> usize {
    execute_native(source).1
}

fn assert_rejected(source: &str) {
    validate(source).expect_err("invalid map traversal was accepted");
}

fn assert_rejected_with_code(source: &str, codes: &[&str]) {
    let error = validate(source).expect_err("invalid map traversal was accepted");
    assert!(
        codes.iter().any(|code| error.message.contains(code)),
        "expected one of {codes:?}: {}",
        error.render("map_traversal_invalid.ncr", source)
    );
}

#[test]
fn entry_scan_preserves_associations_and_exact_coverage_after_growth_and_deletion() {
    assert_native(
        r#"
void main() {
    Map<int, int> entries = new Map<int, int>();
    int index = 0;
    while (index < 1500) {
        entries.put(index, index * 3);
        index = index + 1;
    }
    entries.put(-2147483648, 7);
    entries.put(2147483647, 9);
    entries.put(-97119, 10);
    entries.put(-96755, 20);
    entries.put(-96323, 30);
    entries.put(-95065, 40);

    entries.tryRemove(0);
    entries.tryRemove(511);
    entries.tryRemove(1499);
    entries.tryRemove(-97119);
    entries.tryRemove(-96323);
    entries.tryRemove(-95065);
    entries.put(-97119, 101);
    entries.put(-96323, 303);
    entries.put(-95065, 505);

    Map<int, boolean> observed = new Map<int, boolean>();
    int visited = 0;
    boolean associations = true;
    for (int key, int value : entries) {
        if (key >= 0 && key < 1500) {
            if (key == 0 || key == 511 || key == 1499 || value != key * 3) {
                associations = false;
            }
        } else if (key == -2147483648) {
            if (value != 7) { associations = false; }
        } else if (key == 2147483647) {
            if (value != 9) { associations = false; }
        } else if (key == -97119) {
            if (value != 101) { associations = false; }
        } else if (key == -96755) {
            if (value != 20) { associations = false; }
        } else if (key == -96323) {
            if (value != 303) { associations = false; }
        } else if (key == -95065) {
            if (value != 505) { associations = false; }
        } else {
            associations = false;
        }
        observed.put(key, true);
        visited = visited + 1;
        if (key == 123) { key = 999999; }
    }

    index = 0;
    while (index < 1500) {
        if (index != 0 && index != 511 && index != 1499 && !observed.containsKey(index)) {
            associations = false;
        }
        index = index + 1;
    }
    if (!observed.containsKey(-2147483648) || !observed.containsKey(2147483647) ||
        !observed.containsKey(-97119) || !observed.containsKey(-96755) ||
        !observed.containsKey(-96323) || !observed.containsKey(-95065)) {
        associations = false;
    }
    println(associations);
    println(visited);
    println(observed.size());
    println(entries.containsKey(123));
    println(entries.containsKey(999999));

    Map<int, int> shifted = new Map<int, int>();
    shifted.put(-97119, 10);
    shifted.put(-96755, 20);
    shifted.put(-96323, 30);
    shifted.put(-95065, 40);
    shifted.tryRemove(-97119);
    shifted.tryRemove(-96323);
    shifted.tryRemove(-95065);
    shifted.put(-97119, 101);
    shifted.put(-96323, 303);
    shifted.put(-95065, 505);
    Map<int, boolean> shiftedSeen = new Map<int, boolean>();
    boolean shiftedAssociations = true;
    int shiftedVisits = 0;
    for (int key, int value : shifted) {
        if (key == -97119) {
            if (value != 101) { shiftedAssociations = false; }
        } else if (key == -96755) {
            if (value != 20) { shiftedAssociations = false; }
        } else if (key == -96323) {
            if (value != 303) { shiftedAssociations = false; }
        } else if (key == -95065) {
            if (value != 505) { shiftedAssociations = false; }
        } else {
            shiftedAssociations = false;
        }
        shiftedSeen.put(key, true);
        shiftedVisits = shiftedVisits + 1;
    }
    if (!shiftedSeen.containsKey(-97119) || !shiftedSeen.containsKey(-96755) ||
        !shiftedSeen.containsKey(-96323) || !shiftedSeen.containsKey(-95065)) {
        shiftedAssociations = false;
    }
    println(shiftedAssociations);
    println(shiftedVisits);
    println(shiftedSeen.size());
}
"#,
        b"true\n1503\n1503\ntrue\nfalse\ntrue\n4\n4\n",
    );
}

#[test]
fn copied_values_and_shared_exclusive_map_value_representations_traverse() {
    assert_native(
        r#"
interface Numeric {
    readonly int read();
    void increment();
}
class Cell implements Numeric {
    int stored;
    Text marker;
    Cell(int stored) { this.stored = stored; this.marker = new Text("owned payload"); }
    public readonly int read() { return this.stored; }
    public void increment() { this.stored = this.stored + 1; }
}
enum Stored { Boxed(Cell); Empty; }
void main() {
    Map<int, int> empty = new Map<int, int>();
    int emptyVisits = 0;
    for (int key, int value : empty) { emptyVisits = emptyVisits + 1; }
    println(emptyVisits);
    Map<int, boolean> flags = new Map<int, boolean>();
    flags.put(-1, true);
    flags.put(1, false);
    Map<int, boolean> copiedFlags = new Map<int, boolean>();
    int trueFlags = 0;
    int flagCount = 0;
    for (int key, boolean flag : flags) {
        if (flag) { trueFlags = trueFlags + 1; }
        copiedFlags.put(key, flag);
        flagCount = flagCount + 1;
    }
    println(trueFlags);
    println(flagCount);
    match (copiedFlags.tryGet(-1)) { Some(flag) { println(flag); } None { println("missing"); } }
    match (copiedFlags.tryGet(1)) { Some(flag) { println(flag); } None { println("missing"); } }

    Map<int, String> labels = new Map<int, String>();
    labels.put(-3, "north");
    labels.put(4, "south");
    Map<int, String> copiedLabels = new Map<int, String>();
    int labelCount = 0;
    for (int key, String label : labels) {
        copiedLabels.put(key, label);
        labelCount = labelCount + 1;
    }
    println(labelCount);
    match (copiedLabels.tryGet(-3)) { Some(label) { println(label); } None { println("missing"); } }
    match (copiedLabels.tryGet(4)) { Some(label) { println(label); } None { println("missing"); } }

    Map<int, Cell> cells = new Map<int, Cell>();
    cells.put(1, new Cell(4));
    cells.put(2, new Cell(8));
    int cellTotal = 0;
    int cellCount = 0;
    for (int key, borrow Cell cell : cells) {
        cellTotal = cellTotal + key * cell.read();
        cellCount = cellCount + 1;
    }
    println(cellTotal);
    println(cellCount);
    for (int key, borrow mut Cell cell : cells) { cell.increment(); }
    cellTotal = 0;
    for (int key, borrow Cell cell : cells) { cellTotal = cellTotal + key * cell.read(); }
    println(cellTotal);

    Map<int, Stored> variants = new Map<int, Stored>();
    variants.put(1, new Stored.Boxed(new Cell(3)));
    variants.put(2, new Stored.Empty());
    int variantTotal = 0;
    for (int key, borrow Stored variant : variants) {
        match (variant) { Boxed(cell) { variantTotal = variantTotal + key * cell.read(); } Empty {} }
    }
    println(variantTotal);
    for (int key, borrow mut Stored variant : variants) {
        match (variant) { Boxed(cell) { cell.increment(); } Empty {} }
    }
    variantTotal = 0;
    for (int key, borrow Stored variant : variants) {
        match (variant) { Boxed(cell) { variantTotal = variantTotal + key * cell.read(); } Empty {} }
    }
    println(variantTotal);

    Map<int, Numeric> erased = new Map<int, Numeric>();
    erased.put(7, new Cell(6));
    erased.put(8, new Cell(10));
    int interfaceTotal = 0;
    for (int key, borrow Numeric value : erased) { interfaceTotal = interfaceTotal + key * value.read(); }
    println(interfaceTotal);
    for (int key, borrow mut Numeric value : erased) { value.increment(); }
    interfaceTotal = 0;
    for (int key, borrow Numeric value : erased) { interfaceTotal = interfaceTotal + key * value.read(); }
    println(interfaceTotal);

    Map<int, List<int>> groups = new Map<int, List<int>>();
    List<int> first = new List<int>();
    first.add(2);
    List<int> second = new List<int>();
    second.add(4);
    groups.put(1, first);
    groups.put(2, second);
    int nestedListTotal = 0;
    for (int key, borrow List<int> group : groups) { nestedListTotal = nestedListTotal + key * group.get(0); }
    println(nestedListTotal);
    for (int key, borrow mut List<int> group : groups) { group.add(2); }
    nestedListTotal = 0;
    for (int key, borrow List<int> group : groups) { nestedListTotal = nestedListTotal + key * group.get(1); }
    println(nestedListTotal);

    Map<int, Map<int, int>> tables = new Map<int, Map<int, int>>();
    Map<int, int> table = new Map<int, int>();
    table.put(4, 40);
    tables.put(3, table);
    int nestedMapTotal = 0;
    for (int key, borrow Map<int, int> inner : tables) {
        for (int innerKey, int value : inner) { nestedMapTotal = nestedMapTotal + value; }
    }
    println(nestedMapTotal);
    for (int key, borrow mut Map<int, int> inner : tables) { inner.put(5, 50); }
    nestedMapTotal = 0;
    for (int key, borrow Map<int, int> inner : tables) {
        for (int innerKey, int value : inner) { nestedMapTotal = nestedMapTotal + value; }
    }
    println(nestedMapTotal);
}
"#,
        b"0\n1\n2\ntrue\nfalse\n2\nnorth\nsouth\n20\n2\n23\n3\n4\n122\n137\n10\n6\n40\n90\n",
    );
}

#[test]
fn generic_map_header_and_borrowed_getter_evaluate_collection_once() {
    assert_native(
        r#"
class Cell {
    int stored;
    Cell(int stored) { this.stored = stored; }
    readonly int read() { return this.stored; }
}
class Bag<T> {
    Map<int, T> entries = new Map<int, T>();
    void put(int key, T value) { this.entries.put(key, value); }
    int keyTotal() {
        int total = 0;
        for (int key, borrow T value : this.entries) { total = total + key; }
        return total;
    }
}
class Shelf {
    Map<int, Cell> entries = new Map<int, Cell>();
    int calls = 0;
    borrow Map<int, Cell> entriesRef() {
        this.calls = this.calls + 1;
        return this.entries;
    }
    int total() {
        int result = 0;
        for (int key, borrow Cell value : this.entriesRef()) { result = result + value.read(); }
        return result;
    }
    readonly borrow Cell find(int target) {
        for (int key, borrow Cell value : this.entries) {
            if (key == target) { return value; }
        }
        return this.entries.getRef(target);
    }
    readonly int callCount() { return this.calls; }
}
void main() {
    Bag<Cell> bag = new Bag<Cell>();
    bag.put(-2, new Cell(1));
    bag.put(5, new Cell(2));
    println(bag.keyTotal());

    Shelf shelf = new Shelf();
    shelf.entries.put(7, new Cell(4));
    shelf.entries.put(9, new Cell(7));
    println(shelf.total());
    println(shelf.callCount());
    {
        borrow Cell selected = shelf.find(9);
        println(selected.read());
    }
    shelf.entries.put(11, new Cell(1));
    println(shelf.entries.size());
}
"#,
        b"3\n11\n1\n7\n3\n",
    );
}

#[test]
fn nested_shared_loops_and_early_exits_release_owned_payloads() {
    assert_native(
        r#"
class Payload {
    int id;
    Text marker;
    Payload(int id) { this.id = id; this.marker = new Text("iteration owner"); }
    readonly int read() { return this.id; }
}
int findUnique(borrow Map<int, Payload> payloads) {
    for (int key, borrow Payload payload : payloads) {
        Text returnPayload = new Text("return payload");
        if (payload.read() == 22) { return key; }
    }
    return -1;
}
void main() {
    Map<int, int> values = new Map<int, int>();
    values.put(1, 1);
    values.put(2, 2);
    values.put(3, 3);
    int combined = 0;
    for (int key, int outer : values) {
        Text continuePayload = new Text("continue payload");
        if (key == 1) { continue; }
        for (int innerKey, int inner : values) { combined = combined + outer + inner; }
    }
    println(combined);

    int stoppedAfterOne = 0;
    for (int key, int value : values) {
        Text breakPayload = new Text("break payload");
        if (value > 0) { stoppedAfterOne = stoppedAfterOne + 1; break; }
    }
    println(stoppedAfterOne);
    values.put(4, 4);
    println(values.size());

    Map<int, Payload> payloads = new Map<int, Payload>();
    payloads.put(10, new Payload(11));
    payloads.put(20, new Payload(22));
    payloads.put(30, new Payload(33));
    println(findUnique(payloads));
}
"#,
        b"27\n1\n4\n20\n",
    );
}

#[test]
fn ordered_maps_sets_and_priority_queues_traverse_safely() {
    assert_native(
        r#"
class Key {
    private int rank;
    private Text payload;
    Key(int rank) { this.rank = rank; this.payload = new Text("ordered key"); }
    public readonly int rankValue() { return this.rank; }
}
class Value {
    private int amount;
    private Text payload;
    Value(int amount) { this.amount = amount; this.payload = new Text("ordered value"); }
    public readonly int amountValue() { return this.amount; }
    public void increment() { this.amount = this.amount + 1; }
}
class KeyOrder implements Comparator<Key> {
    public readonly int compare(borrow Key left, borrow Key right) {
        if (left.rankValue() < right.rankValue()) { return -1; }
        if (left.rankValue() > right.rankValue()) { return 1; }
        return 0;
    }
}
class ValueOrder implements Comparator<Value> {
    public readonly int compare(borrow Value left, borrow Value right) {
        if (left.amountValue() < right.amountValue()) { return -1; }
        if (left.amountValue() > right.amountValue()) { return 1; }
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
    OrderedMap<Key, Value> entries = new OrderedMap<Key, Value>(new KeyOrder());
    entries.put(new Key(3), new Value(30));
    entries.put(new Key(1), new Value(10));
    entries.put(new Key(2), new Value(20));
    { borrow mut Value first = entries.valueAtMut(0); first.increment(); }
    for (borrow Key key, borrow mut Value value : entries) {
        println(key.rankValue());
        value.increment();
    }
    for (borrow Key key, borrow Value value : entries) { println(value.amountValue()); }
    OrderedMap<int, int> copiedEntries = new OrderedMap<int, int>(new IntOrder());
    copiedEntries.put(3, 30); copiedEntries.put(1, 10); copiedEntries.put(2, 20);
    int copiedTotal = 0;
    for (int key, int value : copiedEntries) { copiedTotal = copiedTotal + key + value; }
    println(copiedTotal);

    OrderedSet<Key> keys = new OrderedSet<Key>(new KeyOrder());
    keys.add(new Key(3)); keys.add(new Key(1)); keys.add(new Key(2));
    { borrow Key first = keys.keyAt(0); println(first.rankValue()); }
    for (borrow Key key : keys) { println(key.rankValue()); }

    PriorityQueue<Value> queue = new PriorityQueue<Value>(new ValueOrder());
    queue.add(new Value(5)); queue.add(new Value(1)); queue.add(new Value(4));
    queue.add(new Value(2)); queue.add(new Value(3));
    { borrow Value root = queue.valueAt(0); println(root.amountValue()); }
    int heapOrder = 0;
    for (borrow Value value : queue) { heapOrder = heapOrder * 10 + value.amountValue(); }
    println(heapOrder);

    PriorityQueue<int> copiedQueue = new PriorityQueue<int>(new IntOrder());
    copiedQueue.add(5); copiedQueue.add(1); copiedQueue.add(4);
    copiedQueue.add(2); copiedQueue.add(3);
    int copiedHeapOrder = 0;
    for (int value : copiedQueue) { copiedHeapOrder = copiedHeapOrder * 10 + value; }
    println(copiedHeapOrder);
}
"#,
        b"1\n2\n3\n12\n21\n31\n66\n1\n1\n2\n3\n1\n12453\n12453\n",
    );
}

#[test]
fn ordered_and_iterator_traversal_loans_reject_conflicting_access() {
    assert_rejected_with_code(
        r#"
class Key { int rank; Key(int rank) { this.rank = rank; } readonly int rankValue() { return this.rank; } }
class KeyOrder implements Comparator<Key> {
    public readonly int compare(borrow Key left, borrow Key right) { return left.rankValue() - right.rankValue(); }
}
void main() {
    OrderedMap<Key, int> entries = new OrderedMap<Key, int>(new KeyOrder());
    entries.put(new Key(1), 10);
    for (borrow Key key, int value : entries) { entries.clear(); }
}
"#,
        &["E3001"],
    );
    assert_rejected(
        r#"
class Key { int rank; Key(int rank) { this.rank = rank; } readonly int rankValue() { return this.rank; } }
class KeyOrder implements Comparator<Key> {
    public readonly int compare(borrow Key left, borrow Key right) { return left.rankValue() - right.rankValue(); }
}
void main() {
    OrderedMap<Key, int> entries = new OrderedMap<Key, int>(new KeyOrder());
    entries.put(new Key(1), 10);
    for (borrow mut Key key, int value : entries) { }
}
"#,
    );
    assert_rejected_with_code(
        r#"
class Item { int value; Item(int value) { this.value = value; } readonly int read() { return this.value; } }
class ItemOrder implements Comparator<Item> {
    public readonly int compare(borrow Item left, borrow Item right) { return left.read() - right.read(); }
}
void main() {
    PriorityQueue<Item> items = new PriorityQueue<Item>(new ItemOrder());
    items.add(new Item(1));
    for (borrow mut Item item : items) { }
}
"#,
        &["E3003"],
    );
    assert_rejected_with_code(
        r#"
class Item { int value; Item(int value) { this.value = value; } readonly int read() { return this.value; } }
class ItemOrder implements Comparator<Item> {
    public readonly int compare(borrow Item left, borrow Item right) { return left.read() - right.read(); }
}
void main() {
    OrderedSet<Item> items = new OrderedSet<Item>(new ItemOrder());
    items.add(new Item(1));
    for (borrow Item item : items) { items.clear(); }
}
"#,
        &["E3001"],
    );
    assert_rejected_with_code(
        r#"
class Item { int value; Item(int value) { this.value = value; } readonly int read() { return this.value; } }
class ItemOrder implements Comparator<Item> {
    public readonly int compare(borrow Item left, borrow Item right) { return left.read() - right.read(); }
}
void main() {
    PriorityQueue<Item> items = new PriorityQueue<Item>(new ItemOrder());
    items.add(new Item(1));
    for (borrow Item item : items) { items.pop(); }
}
"#,
        &["E3001"],
    );
    assert_rejected_with_code(
        "void main() { List<int> values = new List<int>(); values.add(1); Iterator<int> input = values.drain(); for (int item : input) { input.next(); } }",
        &["E3001"],
    );
    assert_rejected_with_code(
        "void main() { List<int> values = new List<int>(); values.add(1); Iterator<int> input = values.drain(); for (int item : input) { drop(input); } }",
        &["E3002"],
    );
}

#[test]
fn traversal_adds_no_heap_allocations_beyond_map_construction() {
    let baseline = r#"
void main() {
    Map<int, int> entries = new Map<int, int>();
    entries.put(1, 10);
    entries.put(2, 20);
    entries.put(3, 30);
    println(0);
}
"#;
    let traversed = r#"
void main() {
    Map<int, int> entries = new Map<int, int>();
    entries.put(1, 10);
    entries.put(2, 20);
    entries.put(3, 30);
    int total = 0;
    for (int key, int value : entries) { total = total + key + value; }
    println(total);
}
"#;
    assert_eq!(allocation_count(baseline), allocation_count(traversed));
}

#[test]
fn ordered_collection_traversal_adds_no_heap_allocations() {
    let baseline = r#"
class IntOrder implements Comparator<int> {
    public readonly int compare(int left, int right) {
        if (left < right) { return -1; }
        if (left > right) { return 1; }
        return 0;
    }
}
void main() {
    OrderedMap<int, int> entries = new OrderedMap<int, int>(new IntOrder());
    entries.put(1, 10); entries.put(2, 20); entries.put(3, 30);
    PriorityQueue<int> queue = new PriorityQueue<int>(new IntOrder());
    queue.add(1); queue.add(2); queue.add(3);
    println(0);
}
"#;
    let traversed = r#"
class IntOrder implements Comparator<int> {
    public readonly int compare(int left, int right) {
        if (left < right) { return -1; }
        if (left > right) { return 1; }
        return 0;
    }
}
void main() {
    OrderedMap<int, int> entries = new OrderedMap<int, int>(new IntOrder());
    entries.put(1, 10); entries.put(2, 20); entries.put(3, 30);
    PriorityQueue<int> queue = new PriorityQueue<int>(new IntOrder());
    queue.add(1); queue.add(2); queue.add(3);
    int total = 0;
    for (int key, int value : entries) { total = total + key + value; }
    for (int value : queue) { total = total + value; }
    println(total);
}
"#;
    assert_eq!(allocation_count(baseline), allocation_count(traversed));
}

#[test]
fn invalid_headers_and_value_representations_are_rejected() {
    for source in [
        include_str!("../examples/invalid/map_traversal_header_arity.ncr"),
        include_str!("../examples/invalid/map_traversal_key_type.ncr"),
        include_str!("../examples/invalid/map_traversal_value_type.ncr"),
        include_str!("../examples/invalid/map_traversal_map_key_type.ncr"),
        include_str!("../examples/invalid/map_traversal_header_missing_key.ncr"),
        include_str!("../examples/invalid/map_traversal_borrow_value_type.ncr"),
    ] {
        assert_rejected(source);
    }
    assert_rejected_with_code(
        include_str!("../examples/invalid/map_traversal_affine_copy.ncr"),
        &["E5004"],
    );
    assert_rejected_with_code(
        include_str!("../examples/invalid/map_traversal_borrowed_owner_extraction.ncr"),
        &["E5004"],
    );
}

#[test]
fn invalid_owners_aliases_and_structural_mutations_are_rejected() {
    assert_rejected(include_str!("../examples/invalid/map_traversal_temporary_owner.ncr"));
    for source in [
        include_str!("../examples/invalid/map_traversal_insert.ncr"),
        include_str!("../examples/invalid/map_traversal_replace.ncr"),
        include_str!("../examples/invalid/map_traversal_remove_absent.ncr"),
        include_str!("../examples/invalid/map_traversal_clear.ncr"),
    ] {
        assert_rejected_with_code(source, &["E3001"]);
    }
    for source in [
        include_str!("../examples/invalid/map_traversal_move.ncr"),
        include_str!("../examples/invalid/map_traversal_drop.ncr"),
    ] {
        assert_rejected_with_code(source, &["E3002"]);
    }
    assert_rejected_with_code(
        include_str!("../examples/invalid/map_traversal_ancestor_replacement.ncr"),
        &["E3001", "E3002"],
    );
}

#[test]
fn invalid_exclusive_reads_nested_conflicts_and_escapes_are_rejected() {
    assert_rejected_with_code(
        include_str!("../examples/invalid/map_traversal_exclusive_read.ncr"),
        &["E3001"],
    );
    assert_rejected_with_code(
        include_str!("../examples/invalid/map_traversal_exclusive_from_shared_owner.ncr"),
        &["E3003"],
    );
    assert_rejected_with_code(
        include_str!("../examples/invalid/map_traversal_conflicting_nested.ncr"),
        &["E3001", "E3003"],
    );
    assert_rejected_with_code(
        include_str!("../examples/invalid/map_traversal_borrow_escape.ncr"),
        &["E3004"],
    );
}
