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

#[test]
fn try_get_and_failed_removals_preserve_primitive_lists() {
    assert_native(r#"
    void main() {
    List<int> values = new List<int>();
    values.add(10); values.add(20); values.add(30);
    int capacity = values.capacity();
    match (values.tryGet(-1)) { None { println("none"); } Some(value) { println(value); } }
    match (values.tryGet(3)) { None { println("none"); } Some(value) { println(value); } }
    match (values.tryGet(2147483647)) { None { println("none"); } Some(value) { println(value); } }
    match (values.tryGet(1)) { None { println("none"); } Some(value) { println(value); } }
    match (values.tryRemove(-1)) { None { println("none"); } Some(value) { println(value); } }
    match (values.tryRemove(3)) { None { println("none"); } Some(value) { println(value); } }
    match (values.tryRemove(2147483647)) { None { println("none"); } Some(value) { println(value); } }
    println(values.size()); println(values.capacity() == capacity);
    println(values.get(0)); println(values.get(1)); println(values.get(2));
    match (values.tryRemove(1)) { None { println("none"); } Some(value) { println(value); } }
    println(values.get(0)); println(values.get(1));
    values.clear();
    match (values.tryPop()) { None { println("none"); } Some(value) { println(value); } }
    match (values.tryRemove(0)) { None { println("none"); } Some(value) { println(value); } }
    match (values.tryRemove(2147483647)) { None { println("none"); } Some(value) { println(value); } }
    println(values.size()); println(values.capacity() == capacity);
    
    List<boolean> flags = new List<boolean>();
    flags.add(true); flags.add(false);
    match (flags.tryGet(0)) { None { println("none"); } Some(value) { println(value); } }
    match (flags.tryGet(1)) { None { println("none"); } Some(value) { println(value); } }
    match (flags.tryGet(2)) { None { println("none"); } Some(value) { println(value); } }
    
    List<String> labels = new List<String>();
    labels.add("a\0b");
    match (labels.tryGet(0)) { None { println("none"); } Some(value) { println(value); } }
    }
    "#, b"none\nnone\nnone\n20\nnone\nnone\nnone\n3\ntrue\n10\n20\n30\n20\n10\n30\nnone\nnone\nnone\n0\ntrue\ntrue\nfalse\nnone\na\0b\n");
}

#[test]
fn try_removal_transfers_object_enum_and_interface_payloads_once() {
    assert_native(r#"
    interface HasValue { readonly int value(); }
    class Box implements HasValue {
    private int stored;
    private Text marker;
    Box(int stored) { this.stored = stored; this.marker = new Text("x"); }
    public readonly int value() { return this.stored; }
    }
    enum Stored { Boxed(Box); }
    void main() {
    int i = 0;
    List<Box> direct = new List<Box>();
    while (i < 5) { direct.add(new Box(i)); i = i + 1; }
    match (direct.tryRemove(-1)) { None { println("none"); } Some(value) { drop(value); } }
    println(direct.size());
    match (direct.tryRemove(0)) { None { println("none"); } Some(value) { println(value.value()); drop(value); } }
    match (direct.tryRemove(1)) { None { println("none"); } Some(value) { println(value.value()); drop(value); } }
    match (direct.tryRemove(2)) { None { println("none"); } Some(value) { println(value.value()); drop(value); } }
    match (direct.tryPop()) { None { println("none"); } Some(value) { println(value.value()); drop(value); } }
    match (direct.tryPop()) { None { println("none"); } Some(value) { println(value.value()); drop(value); } }
    direct.add(new Box(99));
    drop(direct.tryPop());
    
    i = 10;
    List<Stored> stored_items = new List<Stored>();
    while (i < 15) { stored_items.add(new Stored.Boxed(new Box(i))); i = i + 1; }
    match (stored_items.tryRemove(0)) { None { println("none"); } Some(value) { match (value) { Boxed(boxed) { println(boxed.value()); drop(boxed); } } } }
    match (stored_items.tryRemove(1)) { None { println("none"); } Some(value) { match (value) { Boxed(boxed) { println(boxed.value()); drop(boxed); } } } }
    match (stored_items.tryRemove(2)) { None { println("none"); } Some(value) { match (value) { Boxed(boxed) { println(boxed.value()); drop(boxed); } } } }
    match (stored_items.tryPop()) { None { println("none"); } Some(value) { match (value) { Boxed(boxed) { println(boxed.value()); drop(boxed); } } } }
    match (stored_items.tryPop()) { None { println("none"); } Some(value) { match (value) { Boxed(boxed) { println(boxed.value()); drop(boxed); } } } }
    stored_items.add(new Stored.Boxed(new Box(199)));
    drop(stored_items.tryPop());
    
    i = 20;
    List<HasValue> erased = new List<HasValue>();
    while (i < 25) { erased.add(new Box(i)); i = i + 1; }
    match (erased.tryRemove(0)) { None { println("none"); } Some(value) { println(value.value()); drop(value); } }
    match (erased.tryRemove(1)) { None { println("none"); } Some(value) { println(value.value()); drop(value); } }
    match (erased.tryRemove(2)) { None { println("none"); } Some(value) { println(value.value()); drop(value); } }
    match (erased.tryPop()) { None { println("none"); } Some(value) { println(value.value()); drop(value); } }
    match (erased.tryPop()) { None { println("none"); } Some(value) { println(value.value()); drop(value); } }
    erased.add(new Box(299));
    drop(erased.tryPop());
    }
    "#, b"none\n5\n0\n2\n4\n3\n1\n10\n12\n14\n13\n11\n20\n22\n24\n23\n21\n");
}
