use nacre::{compile_to_c, native, validate};

fn assert_program(source: &str, expected_stdout: &[u8]) {
    let flags = ["-DNACRE_MEMORY_STATS"];
    let c = compile_to_c(source).unwrap_or_else(|error| panic!("{}", error.render("patterns.ncr", source)));
    let c_program = native::compile_c_with_flags(&c, &flags).unwrap();
    let direct_program = nacre::compile_llvm_direct_with_flags(source, &flags).unwrap();
    for program in [c_program, direct_program] {
        let output = program.output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(output.stdout, expected_stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let report = stderr.lines().find(|line| line.starts_with("NACRE_MEMORY ")).unwrap();
        let mut fields = report.split_ascii_whitespace().skip(1);
        let allocations = fields.next().unwrap().strip_prefix("allocations=").unwrap();
        let frees = fields.next().unwrap().strip_prefix("frees=").unwrap();
        assert_eq!(allocations, frees, "{stderr}");
        assert_eq!(fields.next(), Some("live=0"), "{stderr}");
    }
}

#[test]
fn nested_enum_record_patterns_and_guard_fallback_execute() {
    let source = r#"
class Leaf {
    int value;
    Leaf(int value) { this.value = value; }
}
enum Inner { Empty; Value(Leaf); }
enum Outer { Nested(Inner); Nothing; }

int inspect(Outer outer) {
    match (outer) {
        Nested(Value(Leaf { value: value })) if value > 0 { return value; }
        Nested(Value(Leaf { value: value })) { return -value; }
        Nested(Empty) { return 0; }
        Nothing { return -1; }
    }
}
enum Parcel { Empty; Item(Leaf); }
int inspectOwned(Parcel parcel) {
    match (parcel) {
        Item(leaf) if leaf.value < 0 { return leaf.value; }
        Item(Leaf { value: value }) { return value; }
        Empty { return 0; }
    }
}


class Branch {
    Leaf leaf;
    Branch(Leaf leaf) { this.leaf = leaf; }
}
class Tree {
    Branch root;
    Tree(Branch root) { this.root = root; }
}
int treeValue(Tree tree) {
    match (tree) {
        Tree { root: Branch { leaf: Leaf { value: value } } } { return value; }
    }
}

void main() {
    assert(inspect(new Outer.Nested(new Inner.Value(new Leaf(7)))) == 7);
    assert(inspect(new Outer.Nested(new Inner.Value(new Leaf(-4)))) == 4);
    assert(inspect(new Outer.Nested(new Inner.Empty())) == 0);
    assert(inspect(new Outer.Nothing()) == -1);
    assert(treeValue(new Tree(new Branch(new Leaf(23)))) == 23);
    assert(inspectOwned(new Parcel.Item(new Leaf(9))) == 9);
    assert(inspectOwned(new Parcel.Item(new Leaf(-9))) == -9);
    println("patterns-ok");
}
"#;
    assert_program(source, b"patterns-ok\n");
}

#[test]
fn record_pattern_fields_keep_declaration_indices() {
    assert_program(r#"
class Point {
    int x;
    int y;
    Point(int x, int y) { this.x = x; this.y = y; }
}
int readY(Point point) {
    match (point) {
        Point { y: value } { return value; }
    }
}
void main() {
    Point point = new Point(3, 5);
    println(point.y + readY(point));
}
"#, b"10\n");
}

#[test]
fn nested_pair_patterns_destructure_both_levels() {
    let source = r#"
int sum(Pair<int, Pair<int, int>> pair) {
    match (pair) {
        (first, (second, third)) { return first + second + third; }
    }
}
void main() {
    Pair<int, Pair<int, int>> pair = (1, (2, 3));
    assert(sum(pair) == 6);
    println("pairs-ok");
}
"#;
    assert_program(source, b"pairs-ok\n");
}

#[test]
fn nested_pattern_coverage_rejects_missing_cases_and_unreachable_arms() {
    let non_exhaustive = r#"
    enum Inner { Empty; Value(int); }
    enum Outer { Nested(Inner); Nothing; }
    void main() {
        Outer outer = new Outer.Nested(new Inner.Empty());
        match (outer) {
            Nested(Value(_)) {}
            Nothing {}
        }
    }
    "#;
    assert!(validate(non_exhaustive).unwrap_err().message.contains("not exhaustive"));

    let unreachable_nested = r#"
    enum Inner { Empty; Value(int); }
    enum Outer { Nested(Inner); Nothing; }
    void main() {
        Outer outer = new Outer.Nested(new Inner.Empty());
        match (outer) {
            Nested(_) {}
            Nested(Empty) {}
            Nothing {}
        }
    }
    "#;
    assert!(validate(unreachable_nested).unwrap_err().message.contains("unreachable match arm"));

    let unreachable_record = r#"
    class Point { int x; Point(int x) { this.x = x; } }
    void main() {
        Point point = new Point(1);
        match (point) {
            Point {} {}
            Point { x: _ } {}
        }
    }
    "#;
    assert!(validate(unreachable_record).unwrap_err().message.contains("unreachable match arm"));
}

#[test]
fn nested_record_object_fields_are_readonly_borrows() {
    let source = r#"
    class Item {
        int value;
        Item(int value) { this.value = value; }
        void increment() { this.value = this.value + 1; }
    }
    class Wrapper {
        Item item;
        Wrapper(Item item) { this.item = item; }
    }
    void main() {
        Wrapper wrapper = new Wrapper(new Item(1));
        match (wrapper) {
            Wrapper { item: item } { item.increment(); }
        }
    }
    "#;
    assert!(validate(source).is_err());
    let escaping = r#"
    class Item { int value; }
    class Wrapper {
        Item item;
        Wrapper(Item item) { this.item = item; }
    }
    Item escape(Wrapper wrapper) {
        match (wrapper) {
            Wrapper { item: item } { return item; }
        }
    }
    void main() {}
    "#;
    assert!(validate(escaping).is_err());
}

#[test]
fn qualified_nullary_nested_variants_resolve_in_modules() {
    assert_program(r#"
module app;
enum E { Empty; Value(int); }
enum Wrap { Wrapped(E); }
void main() {
    Wrap value = new Wrap.Wrapped(new E.Empty());
    match (value) {
        Wrapped(E.Empty) { println(1); }
        Wrapped(E.Value(number)) { println(number); }
    }
}
"#, b"1\n");
}

#[test]
fn temporary_local_views_cannot_be_destructured_into_dangling_borrows() {
    let source = r#"
class View { borrow Text source; View(borrow Text source) { this.source = source; } }
void main() {
    Text owner = new Text("kept alive");
    match (new View(owner)) {
        View { source: text } { drop(owner); println(text.byteLength()); }
    }
}
"#;
    assert!(validate(source).unwrap_err().message.contains("[E3007]"));
}

#[test]
fn pair_type_nesting_is_bounded_before_stack_exhaustion() {
    let source = format!("void sink({}int{} value) {{}}\nvoid main() {{}}\n",
        "(int,".repeat(5000), ")".repeat(5000));
    assert!(validate(&source).unwrap_err().message.contains("nesting is too deep"));
}

#[test]
fn loop_exits_drop_inner_match_subjects_but_preserve_outer_subjects() {
    assert_program(r#"
class Box { Text label; Box(Text label) { this.label = label; } }
void main() {
    int count = 0;
    while (count < 4) {
        count = count + 1;
        match (new Box(new Text("continue"))) { Box {} { continue; } }
    }
    while (true) { match (new Box(new Text("break"))) { Box {} { break; } } }
    println(count);
    match (new Box(new Text("outer"))) {
        Box { label: label } {
            while (true) { match (new Box(new Text("inner"))) { Box {} { break; } } }
            label.print();
        }
    }
}
"#, b"4\nouter\n");
}
