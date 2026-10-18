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
fn bitset_preallocated_and_cloned_capacity_stays_logically_zero() {
    assert_native(r#"
void main() {
    BitSet reserved = new BitSet(1024L);
    BitSet copy = reserved.clone();
    copy.set(1000L);
    println(reserved.isEmpty() && !reserved.get(1000L));
    println(copy.cardinality() == 1L && copy.get(1000L) && !copy.get(500L));
    reserved.set(1L);
    println(reserved.get(1L) && !copy.get(1L));
}
"#, b"true\ntrue\ntrue\n");
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
fn container_drains_compose_and_keep_their_documented_iteration_order() {
    assert_native(r#"
class DoubleInt implements Mapper<int, int> {
    public int apply(int value) { return value * 2; }
}
class DivisibleByFour implements Predicate<int> {
    public readonly boolean test(int value) { return value % 4 == 0; }
}
class IntOrder implements Comparator<int> {
    public readonly int compare(int left, int right) {
        if (left < right) { return -1; }
        if (left > right) { return 1; }
        return 0;
    }
}
void main() {
    Deque<int> deque = new Deque<int>();
    deque.pushBack(1); deque.pushBack(2); deque.pushBack(3);
    deque.pushBack(4); deque.pushBack(5); deque.pushBack(6);
    MapIterator<int, int> mapped = new MapIterator<int, int>(deque.drain(), new DoubleInt());
    FilterIterator<int> filtered = new FilterIterator<int>(mapped, new DivisibleByFour());
    Iterator<int> skipped = Iterators<int>.skip(filtered, 1L);
    List<int> pipeline = List<int>.collect(Iterators<int>.take(skipped, 1L));
    println(pipeline.get(0));
    println(deque.size());
    Deque<int> directSource = new Deque<int>();
    directSource.pushBack(2); directSource.pushBack(3);
    MapIterator<int, int> concreteIterator =
        new MapIterator<int, int>(directSource.drain(), new DoubleInt());
    int directSum = 0;
    for (int value : concreteIterator) { directSum = directSum + value; }
    println(directSum);

    List<int> arrayInput = new List<int>();
    arrayInput.add(7); arrayInput.add(8);
    Array<int> array = new Array<int>(arrayInput);
    List<int> arrayItems = List<int>.collect(array.drain());
    println(arrayItems.get(0)); println(arrayItems.get(1));

    Set<int> set = new Set<int>();
    set.add(3); set.add(1); set.add(2);
    int setSum = 0; int setCount = 0;
    for (int value : set.drain()) { setSum = setSum + value; setCount = setCount + 1; }
    println(setSum); println(setCount);

    OrderedSet<int> orderedSet = new OrderedSet<int>(new IntOrder());
    orderedSet.add(3); orderedSet.add(1); orderedSet.add(2);
    List<int> orderedSetItems = List<int>.collect(orderedSet.drain());
    println(orderedSetItems.get(0)); println(orderedSetItems.get(1)); println(orderedSetItems.get(2));

    PriorityQueue<int> queue = new PriorityQueue<int>(new IntOrder());
    queue.add(5); queue.add(1); queue.add(4); queue.add(2); queue.add(3);
    List<int> queueItems = List<int>.collect(queue.drain());
    println(queueItems.get(0)); println(queueItems.get(1)); println(queueItems.get(2));
    println(queueItems.get(3)); println(queueItems.get(4));

    Map<int, int> keyMap = new Map<int, int>();
    keyMap.put(3, 30); keyMap.put(1, 10); keyMap.put(2, 20);
    int keySum = 0; int keyCount = 0;
    for (int key : keyMap.drainKeys()) { keySum = keySum + key; keyCount = keyCount + 1; }
    println(keySum); println(keyCount); println(keyMap.isEmpty());

    Map<int, int> valueMap = new Map<int, int>();
    valueMap.put(3, 30); valueMap.put(1, 10); valueMap.put(2, 20);
    int valueSum = 0; int valueCount = 0;
    for (int value : valueMap.drainValues()) { valueSum = valueSum + value; valueCount = valueCount + 1; }
    println(valueSum); println(valueCount); println(valueMap.isEmpty());

    OrderedMap<int, int> orderedKeys = new OrderedMap<int, int>(new IntOrder());
    orderedKeys.put(3, 30); orderedKeys.put(1, 10); orderedKeys.put(2, 20);
    List<int> orderedKeyItems = List<int>.collect(orderedKeys.drainKeys());
    println(orderedKeyItems.get(0)); println(orderedKeyItems.get(1)); println(orderedKeyItems.get(2));

    OrderedMap<int, int> orderedValues = new OrderedMap<int, int>(new IntOrder());
    orderedValues.put(3, 30); orderedValues.put(1, 10); orderedValues.put(2, 20);
    List<int> orderedValueItems = List<int>.collect(orderedValues.drainValues());
    println(orderedValueItems.get(0)); println(orderedValueItems.get(1)); println(orderedValueItems.get(2));
}
    "#, b"8\n0\n10\n7\n8\n6\n3\n1\n2\n3\n1\n2\n4\n5\n3\n6\n3\ntrue\n60\n3\ntrue\n1\n2\n3\n10\n20\n30\n");
}

#[test]
fn direct_ordered_traversal_borrows_entries_and_iterator_break_preserves_the_input() {
    assert_native(r#"
class Item {
    private int id;
    private int count;
    private Text payload;
    Item(int id) { this.id = id; this.count = 0; this.payload = new Text("owned"); }
    public readonly int idValue() { return this.id; }
    public readonly int countValue() { return this.count; }
    public void add(int amount) { this.count = this.count + amount; }
}
class ItemOrder implements Comparator<Item> {
    public readonly int compare(borrow Item left, borrow Item right) {
        if (left.idValue() < right.idValue()) { return -1; }
        if (left.idValue() > right.idValue()) { return 1; }
        return 0;
    }
}
void main() {
    OrderedMap<Item, Item> map = new OrderedMap<Item, Item>(new ItemOrder());
    map.put(new Item(3), new Item(30));
    map.put(new Item(1), new Item(10));
    map.put(new Item(2), new Item(20));
    for (borrow Item key, borrow mut Item value : map) {
        value.add(key.idValue());
        if (key.idValue() == 2) { continue; }
    }
    for (borrow Item key, borrow Item value : map) {
        println(key.idValue()); println(value.countValue());
    }
    map.put(new Item(4), new Item(40));
    println(map.size());

    OrderedSet<Item> set = new OrderedSet<Item>(new ItemOrder());
    set.add(new Item(3)); set.add(new Item(1)); set.add(new Item(2));
    for (borrow Item value : set) { println(value.idValue()); }

    PriorityQueue<Item> queue = new PriorityQueue<Item>(new ItemOrder());
    queue.add(new Item(3)); queue.add(new Item(1)); queue.add(new Item(2));
    int sum = 0;
    for (borrow Item value : queue) { sum = sum + value.idValue(); }
    println(sum);
    println(queue.size());

    List<Item> values = new List<Item>();
    values.add(new Item(7)); values.add(new Item(8)); values.add(new Item(9));
    Iterator<Item> input = values.drain();
    for (Item value : input) { println(value.idValue()); break; }
    match (input.next()) { Some(value) { println(value.idValue()); } None {} }
    for (borrow Item value : input) { println(value.idValue()); continue; }
    match (input.next()) { Some(value) { println(value.idValue()); } None { println("exhausted"); } }
}
"#, b"1\n1\n2\n2\n3\n3\n4\n1\n2\n3\n6\n3\n7\n8\n9\nexhausted\n");
}

#[test]
fn map_drains_release_unreturned_guards_before_sending_iterators() {
    assert_native(r#"
class IntOrder implements Comparator<int> {
    public readonly int compare(int left, int right) {
        if (left < right) { return -1; }
        if (left > right) { return 1; }
        return 0;
    }
}
class GuardKey {
    private int id;
    private Guard<int> guard;
    GuardKey(int id, Guard<int> guard) { this.id = id; this.guard = guard; }
    public readonly int idValue() { return this.id; }
}
class KeyHasher implements Hasher<GuardKey> {
    public readonly long hash(borrow GuardKey key) { return 1L; }
    public readonly boolean equals(borrow GuardKey left, borrow GuardKey right) {
        return left.idValue() == right.idValue();
    }
}
class KeyOrder implements Comparator<GuardKey> {
    public readonly int compare(borrow GuardKey left, borrow GuardKey right) {
        if (left.idValue() < right.idValue()) { return -1; }
        if (left.idValue() > right.idValue()) { return 1; }
        return 0;
    }
}
class IteratorTask implements Task<int> {
    private Iterator<int> input;
    IteratorTask(Iterator<int> input) { this.input = input; }
    public int run() {
        int sum = 0;
        for (int value : this.input) { sum = sum + value; }
        return sum;
    }
}
Result<Unit, Text> checkDrain(borrow mut Mutex<int> mutex, Iterator<int> input) {
    boolean released = false;
    match (mutex.lock()) {
        Ok(guard) { released = true; drop(guard); }
        Err(error) {}
    }
    println(released);
    // Keep the failing-before case on its owning thread rather than unlocking a foreign guard.
    if (!released) { drop(input); return new Result<Unit, Text>.Ok(()); }
    Thread<int> worker = Thread<int>.spawn(new IteratorTask(input))?;
    println(worker.join()?);
    return new Result<Unit, Text>.Ok(());
}
Result<Unit, Text> exercise() {
    Mutex<int> mutex = Mutex<int>.create(0)?;
    Map<int, Guard<int>> hashKeys = new Map<int, Guard<int>>();
    hashKeys.put(7, mutex.lock()?);
    checkDrain(mutex, hashKeys.drainKeys())?;
    OrderedMap<int, Guard<int>> orderedKeys = new OrderedMap<int, Guard<int>>(new IntOrder());
    orderedKeys.put(11, mutex.lock()?);
    checkDrain(mutex, orderedKeys.drainKeys())?;
    Map<GuardKey, int> hashValues = Map<GuardKey, int>.withHasher(new KeyHasher());
    hashValues.put(new GuardKey(1, mutex.lock()?), 13);
    checkDrain(mutex, hashValues.drainValues())?;
    OrderedMap<GuardKey, int> orderedValues = new OrderedMap<GuardKey, int>(new KeyOrder());
    orderedValues.put(new GuardKey(1, mutex.lock()?), 17);
    checkDrain(mutex, orderedValues.drainValues())?;
    return new Result<Unit, Text>.Ok(());
}
void main() { match (exercise()) { Ok(done) {} Err(error) { error.print(); } } }
"#, b"true\n7\ntrue\n11\ntrue\n13\ntrue\n17\n");
}

#[test]
fn affine_drains_drop_unreturned_map_halves_and_iterator_remainders() {
    assert_native(r#"
class Item {
    private int id;
    private Text payload;
    Item(int id) { this.id = id; this.payload = new Text("owned item"); }
    public readonly int idValue() { return this.id; }
}
class ItemOrder implements Comparator<Item> {
    public readonly int compare(borrow Item left, borrow Item right) {
        if (left.idValue() < right.idValue()) { return -1; }
        if (left.idValue() > right.idValue()) { return 1; }
        return 0;
    }
}
void returnAfterFirst(Iterator<Item> items) {
    for (Item item : items) { println(item.idValue()); return; }
}
void main() {
    Map<int, Item> keyDrainSource = new Map<int, Item>();
    keyDrainSource.put(1, new Item(10)); keyDrainSource.put(2, new Item(20));
    int keySum = 0;
    for (int key : keyDrainSource.drainKeys()) { keySum = keySum + key; }
    println(keySum); println(keyDrainSource.isEmpty());

    Map<Text, Item> valueDrainSource = new Map<Text, Item>();
    valueDrainSource.put(new Text("first"), new Item(1));
    valueDrainSource.put(new Text("second"), new Item(2));
    int mapValueSum = 0;
    for (Item item : valueDrainSource.drainValues()) { mapValueSum = mapValueSum + item.idValue(); }
    println(mapValueSum); println(valueDrainSource.isEmpty());

    OrderedMap<Item, Item> orderedKeySource = new OrderedMap<Item, Item>(new ItemOrder());
    orderedKeySource.put(new Item(2), new Item(20)); orderedKeySource.put(new Item(1), new Item(10));
    int orderedKeySequence = 0;
    for (Item item : orderedKeySource.drainKeys()) { orderedKeySequence = orderedKeySequence * 10 + item.idValue(); }
    println(orderedKeySequence);

    OrderedMap<Item, Item> orderedValueSource = new OrderedMap<Item, Item>(new ItemOrder());
    orderedValueSource.put(new Item(2), new Item(20)); orderedValueSource.put(new Item(1), new Item(10));
    int orderedValueSequence = 0;
    for (Item item : orderedValueSource.drainValues()) { orderedValueSequence = orderedValueSequence * 100 + item.idValue(); }
    println(orderedValueSequence);

    Set<Text> textSet = new Set<Text>();
    textSet.add(new Text("a")); textSet.add(new Text("bb"));
    int textBytes = 0;
    for (Text item : textSet.drain()) { textBytes = textBytes + item.byteLength(); }
    println(textBytes);

    Deque<Item> deque = new Deque<Item>();
    deque.pushBack(new Item(4)); deque.pushBack(new Item(5));
    int dequeSum = 0;
    for (Item item : deque.drain()) { dequeSum = dequeSum + item.idValue(); }
    println(dequeSum);

    List<Item> arrayInput = new List<Item>();
    arrayInput.add(new Item(6)); arrayInput.add(new Item(7));
    Array<Item> array = new Array<Item>(arrayInput);
    int arraySum = 0;
    for (Item item : array.drain()) { arraySum = arraySum + item.idValue(); }
    println(arraySum);

    OrderedSet<Item> orderedSet = new OrderedSet<Item>(new ItemOrder());
    orderedSet.add(new Item(3)); orderedSet.add(new Item(1)); orderedSet.add(new Item(2));
    int orderedSetSequence = 0;
    for (Item item : orderedSet.drain()) { orderedSetSequence = orderedSetSequence * 10 + item.idValue(); }
    println(orderedSetSequence);

    PriorityQueue<Item> queue = new PriorityQueue<Item>(new ItemOrder());
    queue.add(new Item(3)); queue.add(new Item(1)); queue.add(new Item(2));
    int queueSum = 0;
    for (Item item : queue.drain()) { queueSum = queueSum + item.idValue(); }
    println(queueSum);

    List<Item> items = new List<Item>();
    items.add(new Item(8)); items.add(new Item(9)); items.add(new Item(10));
    Iterator<Item> input = items.drain();
    for (Item item : input) { println(item.idValue()); break; }
    match (input.next()) {
        None { println("none"); }
        Some(item) { println(item.idValue()); drop(item); }
    }
    drop(input);

    List<Item> returned = new List<Item>();
    returned.add(new Item(11)); returned.add(new Item(12));
    returnAfterFirst(returned.drain());
}
"#, b"3\ntrue\n3\ntrue\n12\n1020\n3\n9\n13\n123\n6\n8\n9\n11\n");
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
