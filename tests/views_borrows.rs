use nacre::validate;
mod support;

fn assert_native(source: &str, stdout: &[u8]) {
    let output = support::compile(source, &["-DNACRE_MEMORY_STATS"])
        .unwrap()
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut fields = stderr.split_whitespace();
    assert_eq!(fields.next(), Some("NACRE_MEMORY"), "{stderr}");
    let allocations = fields.next().unwrap().strip_prefix("allocations=").unwrap();
    let frees = fields.next().unwrap().strip_prefix("frees=").unwrap();
    assert_eq!(allocations, frees, "{stderr}");
    assert_eq!(fields.next(), Some("live=0"), "{stderr}");
    assert_eq!(fields.next(), None, "{stderr}");
}

#[test]
fn returned_reborrows_preserve_nested_receiver_origin_and_interface_upcasts() {
    assert_native(r#"
interface Named { readonly borrow Text name(); }
class Entry implements Named {
    Text stored;
    Entry(Text stored) { this.stored = stored; }
    readonly borrow Text name() { return this.stored; }
}
class Holder {
    Entry entry = new Entry(new Text("Ada"));
    readonly borrow Entry entryRef() { return this.entry; }
    readonly borrow Text name() {
        borrow Entry entry = this.entryRef();
        return entry.name();
    }
    void replace() { this.entry = new Entry(new Text("Grace")); }
}
void main() {
    Holder holder = new Holder();
    {
        borrow Named named = holder.entryRef();
        borrow Text text = named.name();
        println(text.byteLength());
    }
    holder.replace();
    {
        borrow Text text = holder.name();
        println(text.byteLength());
    }
}
    "#, b"3\n5\n");
}

#[test]
fn list_enum_borrows_address_inline_payloads_without_consumption() {
    assert_native(r#"
void main() {
    List<Option<Text>> items = new List<Option<Text>>();
    items.add(new Option<Text>.Some(new Text("A")));
    {
        borrow Option<Text> item = items.getRef(0);
        match (item) { None { } Some(text) { println(text.byteLength()); } }
    }
    {
        borrow mut Option<Text> item = items.getMut(0);
        match (item) { None { } Some(text) { text.append("B"); } }
    }
    match (items.pop()) { None { } Some(text) { println(text.byteLength()); } }
    println(items.size());
}
 "#, b"1\n2\n0\n");
}

#[test]
fn list_interface_borrows_preserve_fat_pointer_dispatch_and_mutability() {
    assert_native(r#"
interface CounterLike { readonly int value(); void increment(); }
class Counter implements CounterLike {
    int stored = 7;
    readonly int value() { return this.stored; }
    void increment() { this.stored = this.stored + 1; }
}
void main() {
    List<CounterLike> items = new List<CounterLike>();
    items.add(new Counter());
    { borrow CounterLike item = items.getRef(0); println(item.value()); }
    { borrow mut CounterLike item = items.getMut(0); item.increment(); }
    { borrow CounterLike item = items.getRef(0); println(item.value()); }
    items.clear();
    println(items.size());
}
 "#, b"7\n8\n0\n");
}

#[test]
fn borrowed_match_retains_owner_through_multiple_payloads_and_loop_exits() {
    assert_native(r#"
class Counter {
    int stored;
    Counter(int stored) { this.stored = stored; }
    readonly int value() { return this.stored; }
    void increment() { this.stored = this.stored + 1; }
}
enum CounterPair { Both(Counter, Counter); }
int read(borrow CounterPair pair) {
    match (pair) { Both(first, second) { return first.value() + second.value(); } }
}
void main() {
    CounterPair pair = new CounterPair.Both(new Counter(1), new Counter(2));
    match (borrow mut pair) { Both(first, second) { first.increment(); second.increment(); } }
    println(read(pair));
    int index = 0;
    while (index < 4) {
        Option<Counter> item = new Option<Counter>.Some(new Counter(index));
        match (borrow item) {
            None { }
            Some(counter) {
                index = index + 1;
                if (index == 1) { continue; }
                println(counter.value());
                if (index == 3) { break; }
            }
        }
    }
    println(read(pair));
}
 "#, b"5\n1\n2\n5\n");
}

#[test]
fn owned_enum_field_replacement_destroys_only_the_old_active_payload() {
    assert_native(r#"
class Holder {
    Option<Text> item = new Option<Text>.Some(new Text("old"));
    void replace(Option<Text> item) { this.item = item; }
    readonly borrow Option<Text> view() { return this.item; }
}
void main() {
    Holder holder = new Holder();
    holder.replace(new Option<Text>.None());
    holder.replace(new Option<Text>.Some(new Text("new")));
    {
        borrow Option<Text> item = holder.view();
        match (item) { None { println(0); } Some(text) { println(text.byteLength()); } }
    }
}
 "#, b"3\n");
}

const HOLDER: &str = r#"
class Holder {
    Text value = new Text("value");
    readonly borrow Text view() { return this.value; }
    void replace() { this.value = new Text("replacement"); }
}
"#;

fn assert_rejected(source: &str, code: &str) {
    let error = validate(source).expect_err("unsafe reference accepted");
    assert!(error.message.contains(code), "{}", error.render("v7-invalid.ncr", source));
}

#[test]
fn returned_local_owner_cannot_escape() {
    assert_rejected("class Holder { readonly borrow Text view() { Text local = new Text(\"bad\"); return local; } } void main() {}", "E3004");
}

#[test]
fn borrowed_parameter_cannot_be_misrepresented_as_receiver_origin() {
    assert_rejected("class Holder { readonly borrow Text view(borrow Text parameter) { return parameter; } } void main() {}", "E3004");
}

#[test]
fn readonly_receiver_cannot_return_exclusive_access() {
    assert_rejected("class Holder { Text value = new Text(\"bad\"); readonly borrow mut Text view() { return this.value; } } void main() {}", "E3003");
}

#[test]
fn field_replacement_is_blocked_while_a_returned_loan_is_live() {
    assert_rejected(&format!("{HOLDER} void main() {{ Holder holder = new Holder(); borrow Text text = holder.view(); holder.replace(); println(text.byteLength()); }}"), "E3001");
}

#[test]
fn list_relocation_is_blocked_while_an_element_loan_is_live() {
    assert_rejected("void main() { List<Text> items = new List<Text>(); items.add(new Text(\"x\")); borrow Text item = items.getRef(0); items.reserve(100); println(item.byteLength()); }", "E3001");
}

#[test]
fn list_destruction_is_blocked_while_an_element_loan_is_live() {
    assert_rejected("void main() { List<Text> items = new List<Text>(); items.add(new Text(\"x\")); borrow Text item = items.getRef(0); drop(items); println(item.byteLength()); }", "E3002");
}

#[test]
fn exclusive_element_loan_blocks_independent_list_reads() {
    assert_rejected("void main() { List<Text> items = new List<Text>(); items.add(new Text(\"x\")); borrow mut Text item = items.getMut(0); println(items.size()); println(item.byteLength()); }", "E3001");
}

#[test]
fn shared_enum_payload_cannot_be_mutated() {
    assert_rejected("void main() { Option<Text> item = new Option<Text>.Some(new Text(\"x\")); match (borrow item) { None { } Some(text) { text.clear(); } } }", "E3003");
}

#[test]
fn returned_borrow_of_a_view_external_field_cannot_escape() {
    assert_rejected("class View { borrow Text source; View(borrow Text source) { this.source = source; } readonly borrow Text get() { return this.source; } } void main() {}", "E3004");
}

const PROJECTED_RECEIVER: &str = r#"
class Child {
    int value = 1;
    int assign(int value) { this.value = value; return this.value; }
}
class Parent {
    Child child = new Child();
    int replaceChild() { this.child = new Child(); return 42; }
}
"#;

#[test]
fn argument_cannot_invalidate_a_stabilized_projected_receiver() {
    assert_rejected(&format!("{PROJECTED_RECEIVER} void main() {{ Parent parent = new Parent(); println(parent.child.assign(parent.replaceChild())); }}"), "E3001");
}

#[test]
fn rhs_cannot_invalidate_a_stabilized_projected_field_target() {
    assert_rejected(&format!("{PROJECTED_RECEIVER} void main() {{ Parent parent = new Parent(); parent.child.value = parent.replaceChild(); }}"), "E3001");
}

#[test]
fn mutable_argument_cannot_alias_a_reserved_projected_receiver() {
    assert_rejected(&format!("{PROJECTED_RECEIVER} int replace(borrow mut Parent parent) {{ parent.child = new Child(); return 42; }} void main() {{ Parent parent = new Parent(); println(parent.child.assign(replace(parent))); }}"), "E3001");
}

#[test]
fn evaluating_mutation_before_field_selection_preserves_the_live_receiver() {
    assert_native(&format!("{PROJECTED_RECEIVER} void main() {{ Parent parent = new Parent(); int value = parent.replaceChild(); println(parent.child.assign(value)); parent.child.value = 43; println(parent.child.value); }}"), b"42\n43\n");
}
