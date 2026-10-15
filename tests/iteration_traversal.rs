use nacre::{compile_to_c, native, validate};

fn assert_native(source: &str, stdout: &[u8]) {
    let c = compile_to_c(source).unwrap_or_else(|error| panic!("{}", error.render("v8-traversal.ncr", source)));
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

fn assert_output(source: &str, stdout: &[u8]) {
    let c = compile_to_c(source).unwrap_or_else(|error| panic!("{}", error.render("v8-traversal.ncr", source)));
    let output = native::compile_c_with_flags(&c, &[]).unwrap().output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, stdout);
}

fn assert_rejected(source: &str) {
    assert!(validate(source).is_err(), "invalid list traversal was accepted");
}
fn assert_rejected_with_code(source: &str, code: &str) {
    let error = validate(source).expect_err("invalid list traversal was accepted");
    assert!(error.message.contains(code), "{}", error.render("v8-traversal-invalid.ncr", source));
}

#[test]
fn copied_primitives_and_empty_lists_traverse_without_iterator_allocations() {
    assert_native(r#"
    void main() {
    List<int> numbers = new List<int>();
    numbers.add(2);
    numbers.add(5);
    List<boolean> flags = new List<boolean>();
    flags.add(true);
    flags.add(false);
    List<String> words = new List<String>();
    words.add("red");
    words.add("blue");
    List<int> empty = new List<int>();
    for (int number : numbers) { println(number); }
    for (boolean flag : flags) { println(flag); }
    for (String word : words) { println(word); }
    for (int number : empty) { println(number); }
    }
    "#, b"2\n5\ntrue\nfalse\nred\nblue\n");
}

#[test]
fn shared_and_exclusive_object_interface_and_enum_elements_are_borrowed_per_iteration() {
    assert_output(r#"
interface CounterLike {
    readonly int value();
    void increment();
}
class Counter implements CounterLike {
    int stored;
    Counter(int stored) { this.stored = stored; }
    readonly int value() { return this.stored; }
    void increment() { this.stored = this.stored + 1; }
}
enum Cell { Item(Counter); Empty; }
void main() {
    List<CounterLike> interfaces = new List<CounterLike>();
    interfaces.add(new Counter(3));
    interfaces.add(new Counter(8));
    for (borrow CounterLike item : interfaces) { println(item.value()); }
    for (borrow mut CounterLike item : interfaces) { item.increment(); }
    for (borrow CounterLike item : interfaces) { println(item.value()); }

    List<Counter> objects = new List<Counter>();
    objects.add(new Counter(6));
    for (borrow Counter item : objects) { println(item.value()); }
    for (borrow mut Counter item : objects) { item.increment(); }
    for (borrow Counter item : objects) { println(item.value()); }
    List<Counter> emptyObjects = new List<Counter>();
    for (borrow Counter item : emptyObjects) { println(item.value()); }

    List<Cell> cells = new List<Cell>();
    cells.add(new Cell.Item(new Counter(4)));
    cells.add(new Cell.Empty());
    for (borrow Cell cell : cells) {
        match (cell) { Item(counter) { println(counter.value()); } Empty { println(0); } }
    }
    for (borrow mut Cell cell : cells) {
        match (cell) { Item(counter) { counter.increment(); } Empty { } }
    }
    for (borrow Cell cell : cells) {
        match (cell) { Item(counter) { println(counter.value()); } Empty { println(0); } }
    }
}
"#, b"3\n8\n4\n9\n6\n7\n4\n0\n5\n0\n");
}

#[test]
fn nested_loops_break_continue_and_return_release_iteration_loans() {
    assert_output(r#"
int first(borrow List<int> values) {
    for (int value : values) { return value; }
    return -1;
}
void main() {
    List<int> values = new List<int>();
    values.add(1);
    values.add(2);
    values.add(3);
    int product = 0;
    for (int outer : values) {
        for (int inner : values) { product = product + outer * inner; }
    }
    println(product);
    int sum = 0;
    for (int value : values) {
        if (value == 1) { continue; }
        sum = sum + value;
        if (value == 2) { break; }
    }
    println(sum);
    println(first(values));
    for (int value : values) { if (value == 3) { break; } }
    values.add(4);
    println(values.size());
}
"#, b"36\n2\n1\n4\n");
}

#[test]
fn generic_for_each_headers_specialize_with_their_list_element_type() {
    assert_output(r#"
class Bag<T> {
    List<T> items = new List<T>();
    void add(T item) { this.items.add(item); }
    int sum() {
        int result = 0;
        for (T item : this.items) { result = result + item; }
        return result;
    }
}
void main() {
    Bag<int> bag = new Bag<int>();
    bag.add(3);
    bag.add(4);
    println(bag.sum());
}
"#, b"7\n");
}

#[test]
fn borrow_returning_collection_is_evaluated_once_and_shared_receiver_reads_remain_legal() {
    assert_output(r#"
class Entry {
    int stored;
    Entry(int stored) { this.stored = stored; }
    readonly int valueAt(int position) { return this.stored + position; }
    readonly boolean matches(int target) { return this.stored == target; }
}
class Book {
    List<Entry> entries = new List<Entry>();
    int calls = 0;
    borrow List<Entry> list() {
        this.calls = this.calls + 1;
        return this.entries;
    }
    readonly int displayValue(borrow Entry entry, int position) { return entry.valueAt(position); }
    readonly boolean matchesName(borrow Entry entry, int target) { return entry.matches(target); }
    readonly int callCount() { return this.calls; }
    int total() {
        int result = 0;
        int position = 0;
        for (borrow Entry entry : this.list()) {
            result = result + this.displayValue(entry, position);
            if (this.matchesName(entry, 2)) { result = result + 10; }
            position = position + 1;
        }
        return result;
    }
}
void main() {
    Book book = new Book();
    book.entries.add(new Entry(1));
    book.entries.add(new Entry(2));
    println(book.total());
    println(book.callCount());
}
"#, b"14\n1\n");
}

#[test]
fn traversal_rejects_structural_mutation_and_alias_mutation() {
    assert_rejected_with_code("void main() { List<int> items = new List<int>(); items.add(1); for (int item : items) { items.add(2); } }", "E3001");
    assert_rejected_with_code("void main() { List<int> items = new List<int>(); items.add(1); borrow mut List<int> alias = items; for (int item : alias) { alias.clear(); } }", "E3001");
    assert_rejected_with_code("void main() { List<int> items = new List<int>(); items.add(1); for (int item : items) { drop(items); break; } }", "E3002");
    assert_rejected_with_code("class Entry { } class Book { List<Entry> entries = new List<Entry>(); readonly int displayValue(borrow Entry entry) { return 1; } int sum() { int total = 0; for (borrow mut Entry entry : this.entries) { total = total + this.displayValue(entry); } return total; } } void main() { Book book = new Book(); book.entries.add(new Entry()); println(book.sum()); }", "E3001");
}

#[test]
fn traversal_rejects_temporary_collections_and_header_representation_mismatches() {
    assert_rejected("void main() { for (int item : new List<int>()) { } }");
    assert_rejected("void main() { List<int> items = new List<int>(); for (boolean item : items) { } }");
    assert_rejected_with_code("class Item { } void main() { List<Item> items = new List<Item>(); for (Item item : items) { } }", "E5004");
    assert_rejected("class Item { } class Other { } void main() { List<Item> items = new List<Item>(); for (borrow Other item : items) { } }");
    assert_rejected_with_code("class Item { } void visit(borrow List<Item> items) { for (borrow mut Item item : items) { } } void main() { }", "E3003");
}
