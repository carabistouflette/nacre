use nacre::validate;
mod support;

fn run(source: &str) -> std::process::Output {
    support::compile(source, &["-DNACRE_MEMORY_STATS"])
        .unwrap_or_else(|error| panic!("{error}"))
        .output()
        .unwrap()
}

#[test]
fn by_value_sums_transfer_payloads_through_returns_matches_and_loops() {
    let source = r#"
enum Choice<T> { Empty; Item(T); }
interface NumberLike { readonly int value(); }
class Box implements NumberLike {
    private int stored;
    public Box(int stored) { this.stored = stored; }
    public readonly int value() { return this.stored; }
}
Choice<int> number() { return new Choice<int>.Item(42); }
Choice<int> identity(Choice<int> value) { return value; }
Choice<NumberLike> boxed() { return new Choice<NumberLike>.Item(new Box(9)); }
Choice<Option<int>> nested() {
    return new Choice<Option<int>>.Item(new Option<int>.Some(11));
}
int readNumber(Choice<int> value) {
    int result = 0;
    match (value) {
        Empty { }
        Item(number) { result = number; }
    }
    return result;
}
int readInterface(Choice<NumberLike> value) {
    match (value) {
        Empty { return 0; }
        Item(box) { return box.value(); }
    }
}
int readNested(Choice<Option<int>> value) {
    int result = 0;
    match (value) {
        Empty { }
        Item(inner) {
            match (inner) {
                None { result = 0; }
                Some(number) { result = number; }
            }
        }
    }
    return result;
}
void loop() {
    int index = 0;
    while (index < 3) {
        Choice<Box> item = new Choice<Box>.Item(new Box(index));
        match (item) {
            Empty { }
            Item(box) {
                println(box.value());
                if (index == 1) { index = index + 1; continue; }
            }
        }
        index = index + 1;
    }
}
void main() {
    println(readInterface(boxed()));
    println(readNested(nested()));
    println(readNumber(identity(number())));
    loop();
}
"#;
    let result = run(source);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    assert_eq!(result.stdout, "9\n11\n42\n0\n1\n2\n".as_bytes());
    assert_eq!(result.stderr, "NACRE_MEMORY allocations=4 frees=4 live=0\n".as_bytes());
}

#[test]
fn enum_payload_and_inline_layout_cycles_are_distinguished() {
    let valid = r#"
enum Link { End; Next(Node); }
class Node {
    private Link next;
    public Node(Link next) { this.next = next; }
}
void main() { Node node = new Node(new Link.End()); }
"#;
    validate(valid).unwrap_or_else(|error| panic!("{}", error.render("sums.ncr", valid)));

    let inline_cycle = "enum Bad { Wrap(Bad); } void main() {}";
    assert!(validate(inline_cycle).is_err());

    let mandatory_cycle = "class Left { Right right; Left(Right right) { this.right = right; } } class Right { Left left; Right(Left left) { this.left = left; } } void main() {}";
    assert!(validate(mandatory_cycle).is_err());
    let interface_cycle = "interface Link {} class Node implements Link { Link next; public Node(Link next) { this.next = next; } } void main() {}";
    assert!(validate(interface_cycle).is_err());

    let interface_with_base = "interface Link {} class Node implements Link { Link next; public Node(Link next) { this.next = next; } } class Empty implements Link {} void main() {}";
    validate(interface_with_base).unwrap_or_else(|error| panic!("{}", error.render("sums.ncr", interface_with_base)));
}

#[test]
fn match_rejects_non_exhaustive_duplicate_unknown_and_wrong_payload_arms() {
    let cases = [
        "enum E { None; Some(int); } void main() { E value = new E.None(); match (value) { None {} } }",
        "enum E { None; Some(int); } void main() { E value = new E.None(); match (value) { None {} None {} Some(x) {} } }",
        "enum E { None; Some(int); } void main() { E value = new E.None(); match (value) { None {} Missing {} Some(x) {} } }",
        "enum E { None; Some(int); } void main() { E value = new E.None(); match (value) { None {} Some() {} } }",
        "enum E { None; Some(int); } void main() { E value = new E.None(); match (value) { None {} Some(true) {} } }",
        "void main() { match (1) { None {} } }",
        "enum E { None; None; } void main() {}",
    ];
    for source in cases { assert!(validate(source).is_err(), "accepted: {source}"); }
}

#[test]
fn consuming_match_rejects_reuse_and_conflicting_borrows() {
    let use_after_match = "enum E { None; Some(int); } void main() { E value = new E.None(); match (value) { None {} Some(x) {} } match (value) { None {} Some(x) {} } }";
    assert!(validate(use_after_match).is_err());

    let active_borrow = "enum E { None; } void inspect(borrow E view) { match (view) { None {} } } void main() { E value = new E.None(); { borrow E view = value; match (value) { None {} } inspect(view); } }";
    assert!(validate(active_borrow).is_err());
}

#[test]
fn enum_payloads_reject_borrows_and_transitive_views() {
    let borrowed_payload = "class Box {} enum E { Ref(borrow Box); } void main() {}";
    assert!(validate(borrowed_payload).is_err());

    let view_payload = r#"
class Box { }
class View {
    borrow Box item;
    public View(borrow Box item) { this.item = item; }
}
enum E { Ref(View); }
void main() { }
"#;
    assert!(validate(view_payload).is_err());
}

#[test]
fn static_native_classes_have_no_constructible_values() {
    for class in ["Console", "Files", "Args", "Compression", "Checksums"] {
        let construction = format!("void main() {{ {class} value = new {class}(); }}");
        assert!(validate(&construction).is_err());
        let typed_as_value = format!("void consume({class} value) {{}} void main() {{}}");
        assert!(validate(&typed_as_value).is_err());
    }
}
