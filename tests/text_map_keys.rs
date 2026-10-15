use nacre::validate;
mod support;

fn execute_native(source: &str) -> (std::process::Output, usize) {
    let output = support::compile(source, &["-DNACRE_MEMORY_STATS"])
        .unwrap()
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));

    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut fields = stderr.split_whitespace();
    assert_eq!(fields.next(), Some("NACRE_MEMORY"));
    let allocations = fields.next().unwrap().strip_prefix("allocations=").unwrap();
    let frees = fields.next().unwrap().strip_prefix("frees=").unwrap();
    assert_eq!(allocations, frees, "{stderr}");
    assert_eq!(fields.next(), Some("live=0"), "{stderr}");
    assert_eq!(fields.next(), None, "{stderr}");
    let allocations = allocations.parse::<usize>().unwrap();
    (output, allocations)
}

fn assert_native(source: &str, stdout: &[u8]) {
    assert_eq!(execute_native(source).0.stdout, stdout);
}

fn allocation_count(source: &str) -> usize {
    execute_native(source).1
}

fn assert_rejected(source: &str) {
    validate(source).expect_err("invalid Text map program was accepted");
}

#[test]
fn text_keys_compare_exact_utf8_bytes_from_independent_queries() {
    assert_native(
        r#"
void main() {
    Map<Text, int> values = new Map<Text, int>();
    values.put(new Text(""), 10);
    values.put(new Text("a\0b"), 20);
    values.put(new Text("é"), 30);
    values.put(new Text("é"), 40);
    values.put(new Text("Ω"), 50);

    Text emptyQuery = new Text("");
    Text nulQuery = new Text("a\0b");
    Text composedQuery = new Text("é");
    Text decomposedQuery = new Text("é");
    Text omegaQuery = new Text("Ω");

    println(values.size());
    println(values.containsKey(emptyQuery));
    match (values.tryGet(emptyQuery)) { None { println("missing empty"); } Some(value) { println(value); } }
    println(values.containsKey(nulQuery));
    println(nulQuery.byteLength());
    match (values.tryGet(nulQuery)) { None { println("missing NUL"); } Some(value) { println(value); } }
    match (values.tryGet(composedQuery)) { None { println("missing composed"); } Some(value) { println(value); } }
    match (values.tryGet(decomposedQuery)) { None { println("missing decomposed"); } Some(value) { println(value); } }
    match (values.tryGet(omegaQuery)) { None { println("missing omega"); } Some(value) { println(value); } }
    println(composedQuery.equals(decomposedQuery));

    match (values.put(new Text("é"), 31)) {
        None { println("missing replaced value"); }
        Some(previous) { println(previous); }
    }
    println(values.size());
    emptyQuery.append("x");
    println(emptyQuery.equalsString("x"));
    println(values.containsKey(emptyQuery));
    Text emptyAgain = new Text("");
    match (values.tryGet(emptyAgain)) { None { println("missing original empty key"); } Some(value) { println(value); } }
}
"#,
        b"5\ntrue\n10\ntrue\n3\n20\n30\n40\n50\nfalse\n30\n5\ntrue\nfalse\n10\n",
    );
}

#[test]
fn owned_text_values_replace_remove_and_clear_without_losing_queries() {
    assert_native(
        r#"
void main() {
    Map<Text, Text> entries = new Map<Text, Text>();
    entries.put(new Text("label"), new Text("old value"));
    Text query = new Text("label");
    match (entries.put(new Text("label"), new Text("new value"))) {
        None { println("missing previous value"); }
        Some(previous) { previous.print(); drop(previous); }
    }
    {
        borrow Text stored = entries.getRef(query);
        stored.print();
    }
    Text absent = new Text("absent");
    match (entries.tryRemove(absent)) {
        None { println("absent"); }
        Some(value) { value.print(); drop(value); }
    }
    println(entries.size());
    {
        borrow Text unchanged = entries.getRef(query);
        unchanged.print();
    }
    println(absent.equalsString("absent"));
    match (entries.tryRemove(query)) {
        None { println("missing removed value"); }
        Some(removed) { removed.print(); drop(removed); }
    }
    println(query.equalsString("label"));
    println(entries.containsKey(query));
    entries.put(new Text("temporary"), new Text("cleared value"));
    entries.clear();
    println(entries.size());
}
"#,
        b"old value\nnew value\nabsent\n1\nnew value\ntrue\nnew value\ntrue\nfalse\n0\n",
    );
}


#[test]
fn growth_and_collision_chain_deletion_preserve_text_key_associations() {
    assert_native(
        r#"
void main() {
    Map<Text, int> chain = new Map<Text, int>();
    // Selected with the Text FNV-1a hash and 32-bit finalizer; no capacity, bucket, or order is pinned.
    chain.put(new Text("collision-98"), 10);
    chain.put(new Text("collision-138"), 20);
    chain.put(new Text("collision-258"), 30);
    chain.put(new Text("collision-447"), 40);
    println(chain.size());

    Text firstCollision = new Text("collision-98");
    Text middleCollision = new Text("collision-258");
    Text lastCollision = new Text("collision-447");
    Text survivor = new Text("collision-138");
    match (chain.tryRemove(firstCollision)) { None { println("missing first"); } Some(value) { println(value); } }
    match (chain.tryRemove(middleCollision)) { None { println("missing middle"); } Some(value) { println(value); } }
    match (chain.tryRemove(lastCollision)) { None { println("missing last"); } Some(value) { println(value); } }
    match (chain.tryGet(survivor)) { None { println("lost survivor"); } Some(value) { println(value); } }
    println(chain.containsKey(firstCollision));
    println(chain.containsKey(middleCollision));
    println(chain.containsKey(lastCollision));

    chain.put(new Text("collision-98"), 101);
    chain.put(new Text("collision-258"), 303);
    chain.put(new Text("collision-447"), 404);
    Text reinsertedFirst = new Text("collision-98");
    Text reinsertedMiddle = new Text("collision-258");
    Text reinsertedLast = new Text("collision-447");
    println(chain.size());
    match (chain.tryGet(reinsertedFirst)) { None { println("missing reinserted first"); } Some(value) { println(value); } }
    match (chain.tryGet(survivor)) { None { println("missing survivor"); } Some(value) { println(value); } }
    match (chain.tryGet(reinsertedMiddle)) { None { println("missing reinserted middle"); } Some(value) { println(value); } }
    match (chain.tryGet(reinsertedLast)) { None { println("missing reinserted last"); } Some(value) { println(value); } }

    Map<Text, int> grown = new Map<Text, int>();
    int index = 0;
    while (index < 700) {
        Text key = new Text("item-");
        key.appendInt(index);
        grown.put(key, index);
        index = index + 1;
    }
    Text ordinaryQuery = new Text("item-");
    ordinaryQuery.appendInt(349);
    println(grown.size());
    match (grown.tryGet(ordinaryQuery)) { None { println("missing after growth"); } Some(value) { println(value); } }

    Text removedFirst = new Text("item-0");
    Text removedMiddle = new Text("item-255");
    Text removedLast = new Text("item-699");
    grown.tryRemove(removedFirst);
    grown.tryRemove(removedMiddle);
    grown.tryRemove(removedLast);
    grown.put(new Text("item-255"), 2550);
    println(grown.size());
    println(grown.containsKey(removedFirst));
    println(grown.containsKey(removedMiddle));
    match (grown.tryGet(removedMiddle)) { None { println("missing reinserted item"); } Some(value) { println(value); } }
    println(grown.containsKey(removedLast));
    boolean associations = true;
    int verifyIndex = 1;
    while (verifyIndex < 699) {
        Text key = new Text("item-");
        key.appendInt(verifyIndex);
        match (grown.tryGet(key)) {
            None { associations = false; }
            Some(value) {
                if (verifyIndex == 255) {
                    if (value != 2550) { associations = false; }
                } else if (value != verifyIndex) {
                    associations = false;
                }
            }
        }
        verifyIndex = verifyIndex + 1;
    }
    println(associations);
    int visits = 0;
    for (borrow Text key, int value : grown) { visits = visits + 1; }
    println(visits);
}
"#,
        b"4\n10\n30\n40\n20\nfalse\nfalse\nfalse\n4\n101\n20\n303\n404\n700\n349\n698\nfalse\ntrue\n2550\nfalse\ntrue\n698\n",
    );
}

#[test]
fn text_keys_support_copied_owned_enum_interface_and_nested_values() {
    assert_native(
        r#"
interface Counted {
    readonly int count();
}
class Parcel implements Counted {
    private int amount;
    private Text description;
    Parcel(int amount, Text description) { this.amount = amount; this.description = description; }
    public readonly int count() { return this.amount; }
    public readonly borrow Text name() { return this.description; }
    public void add(int amount) { this.amount = this.amount + amount; }
}
enum StockState { Available(int); Empty; }
void main() {
    Map<Text, boolean> flags = new Map<Text, boolean>();
    flags.put(new Text("primary"), true);
    Text flagQuery = new Text("primary");
    match (flags.tryGet(flagQuery)) { None { println("missing flag"); } Some(value) { println(value); } }

    Map<Text, String> copiedLabels = new Map<Text, String>();
    copiedLabels.put(new Text("title"), "copied value");
    Text titleQuery = new Text("title");
    match (copiedLabels.tryGet(titleQuery)) { None { println("missing title"); } Some(value) { println(value); } }

    Map<Text, Text> ownedLabels = new Map<Text, Text>();
    ownedLabels.put(new Text("name"), new Text("owned text"));
    Text nameQuery = new Text("name");
    {
        borrow Text label = ownedLabels.getRef(nameQuery);
        label.print();
    }

    Map<Text, StockState> states = new Map<Text, StockState>();
    states.put(new Text("shelf"), new StockState.Available(6));
    Text stateQuery = new Text("shelf");
    {
        borrow StockState state = states.getRef(stateQuery);
        match (state) { Available(amount) { println(amount); } Empty { println("empty"); } }
    }

    Map<Text, Counted> interfaces = new Map<Text, Counted>();
    interfaces.put(new Text("parcel"), new Parcel(7, new Text("interface value")));
    Text parcelQuery = new Text("parcel");
    {
        borrow Counted value = interfaces.getRef(parcelQuery);
        println(value.count());
    }

    Map<Text, Parcel> parcels = new Map<Text, Parcel>();
    parcels.put(new Text("crate"), new Parcel(4, new Text("owned record")));
    Text crateQuery = new Text("crate");
    {
        borrow mut Parcel value = parcels.getMut(crateQuery);
        value.add(5);
    }
    match (parcels.tryRemove(crateQuery)) {
        None { println("missing parcel"); }
        Some(value) { println(value.count()); drop(value); }
    }

    Map<Text, List<int>> shelves = new Map<Text, List<int>>();
    List<int> units = new List<int>();
    units.add(9);
    shelves.put(new Text("tools"), units);
    Text toolsQuery = new Text("tools");
    {
        borrow List<int> found = shelves.getRef(toolsQuery);
        println(found.get(0));
    }

    Map<Text, Map<Text, int>> departments = new Map<Text, Map<Text, int>>();
    Map<Text, int> counts = new Map<Text, int>();
    counts.put(new Text("small"), 17);
    departments.put(new Text("north"), counts);
    Text northQuery = new Text("north");
    Text smallQuery = new Text("small");
    {
        borrow Map<Text, int> north = departments.getRef(northQuery);
        match (north.tryGet(smallQuery)) { None { println("missing nested count"); } Some(value) { println(value); } }
    }

    flags.clear();
    copiedLabels.clear();
    ownedLabels.clear();
    states.clear();
    interfaces.clear();
    parcels.clear();
    shelves.clear();
    departments.clear();
    println(parcels.size());
}
"#,
        b"true\ncopied value\nowned text\n6\n7\n9\n9\n17\n0\n",
    );
}

#[test]
fn paired_text_key_traversal_supports_shared_mutable_nested_and_early_exit_loans() {
    assert_native(
        r#"
class Counter {
    private int amount;
    Counter(int amount) { this.amount = amount; }
    public readonly int read() { return this.amount; }
    public void increment() { this.amount = this.amount + 1; }
}
class Rack {
    private Map<Text, Counter> entries = new Map<Text, Counter>();
    private Text fallback = new Text("missing");
    public void add(Text key, int amount) { this.entries.put(key, new Counter(amount)); }
    public readonly borrow Text keyFor(int amount) {
        for (borrow Text key, borrow Counter value : this.entries) {
            if (value.read() == amount) { return key; }
        }
        return this.fallback;
    }
    public readonly int equalPairs() {
        int pairs = 0;
        for (borrow Text key, borrow Counter left : this.entries) {
            for (borrow Text otherKey, borrow Counter right : this.entries) {
                if (key.equals(otherKey) && left.read() == right.read()) { pairs = pairs + 1; }
            }
        }
        return pairs;
    }
    public void incrementKnown() {
        for (borrow Text key, borrow mut Counter value : this.entries) {
            if (key.equalsString("oak") || key.equalsString("pine")) { value.increment(); }
        }
    }
    public readonly int amountFor(borrow Text key) {
        borrow Counter value = this.entries.getRef(key);
        return value.read();
    }
}
void main() {
    Rack rack = new Rack();
    rack.add(new Text("oak"), 2);
    rack.add(new Text("pine"), 7);
    {
        borrow Text selected = rack.keyFor(7);
        println(selected.equalsString("oak") || selected.equalsString("pine"));
    }
    println(rack.equalPairs());
    rack.incrementKnown();
    Text oakQuery = new Text("oak");
    Text pineQuery = new Text("pine");
    println(rack.amountFor(oakQuery));
    println(rack.amountFor(pineQuery));

    Map<Text, int> exits = new Map<Text, int>();
    exits.put(new Text("only"), 5);
    int visits = 0;
    for (borrow Text key, int value : exits) {
        Text continued = new Text("continue body");
        if (continued.byteLength() > 0) { visits = visits + 1; continue; }
    }
    println(visits);
    for (borrow Text key, int value : exits) {
        Text broken = new Text("break body");
        if (broken.byteLength() > 0) { visits = visits + 1; break; }
    }
    println(visits);
}
"#,
        b"true\n2\n3\n8\n1\n2\n",
    );
}


#[test]
fn named_inventory_example_uses_borrowed_queries_and_unordered_traversal() {
    assert_native(
        include_str!("../examples/collections/text_map_inventory.ncr"),
        b"true\ntrue\n12\n20\n2\n",
    );
}

#[test]
fn invalid_text_key_bindings_hash_mutation_and_root_aliases_are_rejected() {
    for source in [
        include_str!("../examples/invalid/text_map_owned_key_binding.ncr"),
        include_str!("../examples/invalid/text_map_mutable_key_binding.ncr"),
        include_str!("../examples/invalid/text_map_move_borrowed_key.ncr"),
        include_str!("../examples/invalid/text_map_mutate_key.ncr"),
        include_str!("../examples/invalid/text_map_root_write.ncr"),
        include_str!("../examples/invalid/text_map_root_absent_remove.ncr"),
        include_str!("../examples/invalid/text_map_root_write_during_traversal.ncr"),
        include_str!("../examples/invalid/text_map_exclusive_independent_read.ncr"),
        include_str!("../examples/invalid/text_map_exclusive_traversal_read.ncr"),
    ] {
        assert_rejected(source);
    }
}

#[test]
fn escaping_local_borrows_and_default_custom_keys_are_rejected() {
    for source in [
        include_str!("../examples/invalid/text_map_escape_key.ncr"),
        include_str!("../examples/invalid/text_map_escape_value.ncr"),
    ] {
        assert_rejected(source);
    }
    let custom = include_str!("../examples/invalid/text_map_custom_key.ncr");
    let error = validate(custom).expect_err("custom key accepted without Hasher");
    assert!(error.message.contains("E5004"), "{}", error.render("text_map_keys.ncr", custom));
}

#[test]
fn duplicate_text_key_replacement_needs_only_its_explicit_key_allocation() {
    let baseline = r#"
void main() {
    Map<Text, int> entries = new Map<Text, int>();
    entries.put(new Text("same"), 1);
    Text incoming = new Text("same");
    drop(incoming);
}
"#;
    let replacement = r#"
void main() {
    Map<Text, int> entries = new Map<Text, int>();
    entries.put(new Text("same"), 1);
    entries.put(new Text("same"), 2);
}
"#;
    assert_eq!(allocation_count(baseline), allocation_count(replacement));
}

#[test]
fn borrowed_text_queries_and_key_traversal_add_no_allocations() {
    let setup = r#"
class Counter {
    private int amount;
    private Text marker;
    Counter(int amount) { this.amount = amount; this.marker = new Text("record"); }
    readonly int read() { return this.amount; }
    void increment() { this.amount = this.amount + 1; }
}
void main() {
    Map<Text, int> counts = new Map<Text, int>();
    counts.put(new Text("copy"), 42);
    Map<Text, Counter> records = new Map<Text, Counter>();
    records.put(new Text("record"), new Counter(7));
    Text countQuery = new Text("copy");
    Text recordQuery = new Text("record");
    Text absentQuery = new Text("absent");
"#;
    let without_lookups = format!(
        "{setup}{}",
        r#"    println(countQuery.byteLength());
    println(recordQuery.byteLength());
    println(absentQuery.byteLength());
    countQuery.append("!");
    recordQuery.append("!");
}
"#
    );
    let with_lookups = format!(
        "{setup}{}",
        r#"    println(counts.containsKey(countQuery));
    match (counts.tryGet(countQuery)) { None { println("missing count"); } Some(value) { println(value); } }
    { borrow Counter shared = records.getRef(recordQuery); println(shared.read()); }
    { borrow mut Counter exclusive = records.getMut(recordQuery); exclusive.increment(); }
    counts.tryRemove(absentQuery);
    println(countQuery.byteLength());
    println(recordQuery.byteLength());
    println(absentQuery.byteLength());
    countQuery.append("!");
    recordQuery.append("!");
}
"#
    );
    assert_eq!(allocation_count(&without_lookups), allocation_count(&with_lookups));

    let traversal_setup = r#"
class Counter {
    private int amount;
    private Text marker;
    Counter(int amount) { this.amount = amount; this.marker = new Text("entry"); }
    readonly int read() { return this.amount; }
}
void main() {
    Map<Text, Counter> entries = new Map<Text, Counter>();
    entries.put(new Text("one"), new Counter(1));
    entries.put(new Text("two"), new Counter(2));
    entries.put(new Text("three"), new Counter(3));
    int total = 0;
"#;
    let without_traversal = format!(
        "{traversal_setup}{}",
        r#"    println(total);
}
"#
    );
    let with_traversal = format!(
        "{traversal_setup}{}",
        r#"    for (borrow Text key, borrow Counter value : entries) { total = total + value.read(); }
    println(total);
}
"#
    );
    assert_eq!(allocation_count(&without_traversal), allocation_count(&with_traversal));
}
