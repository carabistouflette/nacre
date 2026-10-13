use nacre::{compile_to_c, native, validate};

fn assert_native(source: &str, stdout: &[u8]) {
    let c = compile_to_c(source)
        .unwrap_or_else(|error| panic!("{}", error.render("v9_maps.ncr", source)));
    let output = native::compile_c_with_flags(&c, &["-DNACRE_MEMORY_STATS"])
        .unwrap().output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, stdout);

    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut fields = stderr.split_whitespace();
    assert_eq!(fields.next(), Some("NACRE_MEMORY"));
    let allocations = fields.next().unwrap().strip_prefix("allocations=").unwrap();
    let frees = fields.next().unwrap().strip_prefix("frees=").unwrap();
    assert_eq!(allocations, frees);
    assert_eq!(fields.next(), Some("live=0"));
    assert_eq!(fields.next(), None);
}

fn assert_rejected(source: &str) {
    assert!(validate(source).is_err(), "expected compiler rejection for:\n{source}");
}

fn memory_allocation_count(source: &str) -> usize {
    let c = compile_to_c(source)
        .unwrap_or_else(|error| panic!("{}", error.render("v9_maps.ncr", source)));
    let output = native::compile_c_with_flags(&c, &["-DNACRE_MEMORY_STATS"])
        .unwrap().output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut fields = stderr.split_whitespace();
    assert_eq!(fields.next(), Some("NACRE_MEMORY"));
    let allocations = fields.next().unwrap().strip_prefix("allocations=").unwrap()
        .parse::<usize>().unwrap();
    let frees = fields.next().unwrap().strip_prefix("frees=").unwrap()
        .parse::<usize>().unwrap();
    assert_eq!(allocations, frees);
    assert_eq!(fields.next(), Some("live=0"));
    assert_eq!(fields.next(), None);
    allocations
}

#[test]
fn updates_lookups_removal_and_clear_reuse_add_no_allocations() {
    let baseline = r#"
    void main() {
        Map<int, int> values = new Map<int, int>();
        values.put(1, 1);
    }
    "#;
    let empty = r#"
    void main() {
        Map<int, int> values = new Map<int, int>();
    }
    "#;
    assert!(memory_allocation_count(empty) < memory_allocation_count(baseline));
    let exercised = r#"
    void main() {
        Map<int, int> values = new Map<int, int>();
        values.put(1, 1);
        values.put(1, 2);
        values.containsKey(1);
        values.size();
        values.isEmpty();
        values.tryGet(1);
        values.tryGet(99);
        values.tryRemove(99);
        values.tryRemove(1);
        values.put(2, 2);
        values.clear();
        values.put(3, 3);
    }
    "#;
    assert_eq!(memory_allocation_count(baseline), memory_allocation_count(exercised));

    // Sweep final map sizes to catch replacement allocating at a growth boundary,
    // without asserting the buffer's initial capacity or growth defaults.
    let growth_baseline = r#"
    void main() {
        int count = 1;
        while (count <= 1024) {
            Map<int, int> values = new Map<int, int>();
            int index = 0;
            while (index < count) {
                values.put(index, index);
                index = index + 1;
            }
            count = count + 1;
        }
    }
    "#;
    let growth_with_replacement = r#"
    void main() {
        int count = 1;
        while (count <= 1024) {
            Map<int, int> values = new Map<int, int>();
            int index = 0;
            while (index < count) {
                values.put(index, index);
                index = index + 1;
            }
            values.put(count - 1, count - 1);
            count = count + 1;
        }
    }
    "#;
    assert_eq!(memory_allocation_count(growth_baseline), memory_allocation_count(growth_with_replacement));
}


#[test]
fn copied_values_boundaries_replacement_and_absence() {
    assert_native(r#"
    void main() {
        Map<int, int> numbers = new Map<int, int>();
        println(numbers.size());
        println(numbers.isEmpty());
        println(numbers.containsKey(0));
        match (numbers.tryGet(0)) { None { println("none"); } Some(value) { println(value); } }
        match (numbers.tryRemove(0)) { None { println("none"); } Some(value) { println(value); } }
        println(numbers.size());

        numbers.put(-2147483648, -1);
        numbers.put(2147483647, 1);
        numbers.put(0, 10);
        numbers.put(-17, 17);
        match (numbers.put(0, 20)) { None { println("missing old value"); } Some(value) { println(value); } }
        match (numbers.tryGet(-2147483648)) { None { println("none"); } Some(value) { println(value); } }
        match (numbers.tryGet(2147483647)) { None { println("none"); } Some(value) { println(value); } }
        match (numbers.tryGet(0)) { None { println("none"); } Some(value) { println(value); } }
        match (numbers.tryRemove(-17)) { None { println("none"); } Some(value) { println(value); } }
        println(numbers.size());
        println(numbers.containsKey(-17));

        Map<int, boolean> flags = new Map<int, boolean>();
        flags.put(-3, true);
        flags.put(4, false);
        match (flags.tryGet(-3)) { None { println("none"); } Some(value) { println(value); } }
        match (flags.tryGet(4)) { None { println("none"); } Some(value) { println(value); } }
        match (flags.put(-3, false)) { None { println("missing old flag"); } Some(value) { println(value); } }
        match (flags.tryRemove(-3)) { None { println("missing flag"); } Some(value) { println(value); } }

        Map<int, String> labels = new Map<int, String>();
        labels.put(9, "a\0b");
        match (labels.tryGet(9)) { None { println("none"); } Some(value) { println(value); } }
        match (labels.put(9, "new")) { None { println("missing old value"); } Some(value) { println(value); } }
        match (labels.tryRemove(9)) { None { println("missing label"); } Some(value) { println(value); } }
        labels.clear();
        println(labels.isEmpty());
    }
    "#, b"0\ntrue\nfalse\nnone\nnone\n0\n10\n-1\n1\n20\n17\n3\nfalse\ntrue\nfalse\ntrue\nfalse\na\0b\na\0b\nnew\ntrue\n");
}

#[test]
fn colliding_probe_chains_shift_on_head_middle_and_tail_removal() {
    assert_native(r#"
    void main() {
        Map<int, int> values = new Map<int, int>();
        // These keys share the low ten hash bits in the native integer mixer.
        values.put(-97119, 10);
        values.put(-96755, 20);
        values.put(-96323, 30);
        values.put(-95065, 40);
        println(values.size());
        match (values.tryRemove(-97119)) { None { println("none"); } Some(value) { println(value); } }
        match (values.tryRemove(-96323)) { None { println("none"); } Some(value) { println(value); } }
        match (values.tryRemove(-95065)) { None { println("none"); } Some(value) { println(value); } }
        match (values.tryGet(-96755)) { None { println("none"); } Some(value) { println(value); } }
        println(values.containsKey(-97119));
        println(values.containsKey(-96323));
        println(values.containsKey(-95065));
        values.put(-97119, 11);
        values.put(-96323, 33);
        values.put(-95065, 44);
        match (values.tryGet(-97119)) { None { println("none"); } Some(value) { println(value); } }
        match (values.tryGet(-96323)) { None { println("none"); } Some(value) { println(value); } }
        match (values.tryGet(-95065)) { None { println("none"); } Some(value) { println(value); } }
    }
    "#, b"4\n10\n30\n40\n20\nfalse\nfalse\nfalse\n11\n33\n44\n");
}

#[test]
fn growth_preserves_entries_and_reuses_deleted_slots() {
    assert_native(r#"
    void main() {
        Map<int, int> values = new Map<int, int>();
        int index = 0;
        while (index < 1500) {
            values.put(index, index * 2);
            index = index + 1;
        }
        println(values.size());
        match (values.tryGet(0)) { None { println("none"); } Some(value) { println(value); } }
        match (values.tryGet(997)) { None { println("none"); } Some(value) { println(value); } }
        match (values.tryGet(1499)) { None { println("none"); } Some(value) { println(value); } }
        values.tryRemove(0);
        values.tryRemove(511);
        values.tryRemove(1499);
        values.put(511, 777);
        println(values.size());
        match (values.tryGet(511)) { None { println("none"); } Some(value) { println(value); } }
        println(values.containsKey(0));
        println(values.containsKey(1499));
        values.clear();
        println(values.size());
        println(values.isEmpty());
    }
    "#, b"1500\n0\n1994\n2998\n1498\n777\nfalse\nfalse\n0\ntrue\n");
}

#[test]
fn owned_enum_interface_transfers_and_borrowed_accessors() {
    assert_native(r#"
    interface HasValue {
        readonly int read();
        void increment();
    }
    class Box implements HasValue {
        private int value;
        private Text marker;
        Box(int value) { this.value = value; this.marker = new Text("owned"); }
        public readonly int read() { return this.value; }
        public void increment() { this.value = this.value + 1; }
    }
    enum Stored { Boxed(Box); Empty; }
    void main() {
        Map<int, Box> boxes = new Map<int, Box>();
        boxes.put(1, new Box(10));
        {
            borrow Box first = boxes.getRef(1);
            borrow Box second = boxes.getRef(1);
            println(first.read() + second.read());
        }
        {
            borrow mut Box editor = boxes.getMut(1);
            editor.increment();
        }
        match (boxes.put(1, new Box(20))) {
            None { println("missing old box"); }
            Some(previous) { println(previous.read()); drop(previous); }
        }
        match (boxes.tryRemove(1)) {
            None { println("missing box"); }
            Some(value) { println(value.read()); drop(value); }
        }
        match (boxes.tryRemove(1)) { None { println("none"); } Some(value) { drop(value); } }

        Map<int, Stored> variants = new Map<int, Stored>();
        variants.put(2, new Stored.Boxed(new Box(30)));
        match (variants.put(2, new Stored.Boxed(new Box(40)))) {
            None { println("missing old enum"); }
            Some(previous) { match (previous) { Boxed(value) { println(value.read()); drop(value); } Empty {} } }
        }
        match (variants.tryRemove(2)) {
            None { println("missing enum"); }
            Some(value) { match (value) { Boxed(boxed) { println(boxed.read()); drop(boxed); } Empty {} } }
        }
        variants.put(2, new Stored.Boxed(new Box(35)));
        {
            borrow Stored reader = variants.getRef(2);
            match (reader) {
                Boxed(value) { println(value.read()); }
                Empty {}
            }
        }
        {
            borrow mut Stored editor = variants.getMut(2);
            match (editor) {
                Boxed(value) { value.increment(); }
                Empty {}
            }
        }
        match (variants.tryRemove(2)) {
            None { println("missing enum"); }
            Some(value) { match (value) { Boxed(boxed) { println(boxed.read()); drop(boxed); } Empty {} } }
        }

        Map<int, HasValue> erased = new Map<int, HasValue>();
        erased.put(3, new Box(50));
        {
            borrow HasValue reader = erased.getRef(3);
            println(reader.read());
        }
        {
            borrow mut HasValue editor = erased.getMut(3);
            editor.increment();
        }
        match (erased.put(3, new Box(60))) {
            None { println("missing old interface"); }
            Some(previous) { println(previous.read()); drop(previous); }
        }
        match (erased.tryRemove(3)) {
            None { println("missing interface"); }
            Some(value) { println(value.read()); drop(value); }
        }
        boxes.put(4, new Box(70));
        variants.put(4, new Stored.Boxed(new Box(80)));
        erased.put(4, new Box(90));
        println(boxes.size() + variants.size() + erased.size());
        boxes.clear();
        variants.clear();
        erased.clear();
        println(boxes.isEmpty());
        println(variants.isEmpty());
        println(erased.isEmpty());

        Map<int, Box> growing = new Map<int, Box>();
        int index = 0;
        while (index < 20) {
            growing.put(index, new Box(index));
            index = index + 1;
        }
        println(growing.size());
        growing.clear();
        println(growing.isEmpty());
    }
    "#, b"20\n11\n20\nnone\n30\n40\n35\n36\n50\n51\n60\n3\ntrue\ntrue\ntrue\n20\ntrue\n");
}

#[test]
fn nested_collections_transfer_and_cleanup() {
    assert_native(r#"
    void main() {
        Map<int, List<int>> lists = new Map<int, List<int>>();
        List<int> initial = new List<int>();
        initial.add(10);
        lists.put(1, initial);
        {
            borrow List<int> reader = lists.getRef(1);
            println(reader.get(0));
        }
        List<int> replacement = new List<int>();
        replacement.add(20);
        match (lists.put(1, replacement)) {
            None { println("missing list"); }
            Some(previous) { println(previous.get(0)); drop(previous); }
        }
        match (lists.tryRemove(1)) {
            None { println("missing list"); }
            Some(value) { println(value.get(0)); drop(value); }
        }

        Map<int, Map<int, int>> maps = new Map<int, Map<int, int>>();
        Map<int, int> nested = new Map<int, int>();
        nested.put(9, 90);
        maps.put(2, nested);
        {
            borrow Map<int, int> reader = maps.getRef(2);
            match (reader.tryGet(9)) {
                None { println("missing nested value"); }
                Some(value) { println(value); }
            }
        }
        match (maps.tryRemove(2)) {
            None { println("missing map"); }
            Some(value) {
                match (value.tryRemove(9)) {
                    None { println("missing nested value"); }
                    Some(found) { println(found); }
                }
                drop(value);
            }
        }
        println(lists.size() + maps.size());
        lists.clear();
        maps.clear();
        println(lists.isEmpty());
        println(maps.isEmpty());
    }
    "#, b"10\n10\n20\n90\n90\n0\ntrue\ntrue\n");
}

#[test]
fn missing_borrowed_key_fails_with_call_location() {
    let source = r#"class Box {} void main() { Map<int, Box> values = new Map<int, Box>(); borrow Box missing = values.getRef(-9); }"#;
    let c = compile_to_c(source).unwrap_or_else(|error| panic!("{}", error.render("v9_maps.ncr", source)));
    let output = native::compile_c_with_flags(&c, &[]).unwrap().output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("[1:"), "{stderr}");
}

#[test]
fn map_types_and_borrow_domains_reject_invalid_programs() {
    for source in [
        "void main() { Map<int> invalid = new Map<int>(); }",
        "void main() { Map<int, int, int> invalid = new Map<int, int, int>(); }",
        "class Box {} void main() { Map<int, Box> values = new Map<int, Box>(); values.tryGet(0); }",
        "void main() { Map<int, int> values = new Map<int, int>(); values.getRef(0); }",
        "class Box {} void main() { Map<int, int> values = new Map<int, int>(); values.getMut(0); }",
        "class Box {} void main() { Map<int, Box> values = new Map<int, Box>(); Box owned = new Box(); borrow Box view = owned; values.put(1, view); }",
        "class Box {} class View { borrow Box target; View(borrow Box target) { this.target = target; } } class Holder { View nested; } void main() { Map<int, Holder> invalid = new Map<int, Holder>(); }",
    ] {
        assert_rejected(source);
    }
}

#[test]
fn map_borrows_reject_invalidation_moves_and_ancestor_replacement() {
    for source in [
        "class Box { readonly int read() { return 1; } } void main() { Map<int, Box> values = new Map<int, Box>(); values.put(1, new Box()); borrow Box shared = values.getRef(1); values.put(1, new Box()); println(shared.read()); }",
        "class Box { readonly int read() { return 1; } } void main() { Map<int, Box> values = new Map<int, Box>(); values.put(1, new Box()); borrow Box shared = values.getRef(1); values.tryRemove(1); println(shared.read()); }",
        "class Box { readonly int read() { return 1; } } void main() { Map<int, Box> values = new Map<int, Box>(); values.put(1, new Box()); borrow Box shared = values.getRef(1); values.clear(); println(shared.read()); }",
        "class Box { readonly int read() { return 1; } } void main() { Map<int, Box> values = new Map<int, Box>(); values.put(1, new Box()); borrow Box shared = values.getRef(1); values = new Map<int, Box>(); println(shared.read()); }",
        "class Box { void increment() {} } void main() { Map<int, Box> values = new Map<int, Box>(); values.put(1, new Box()); borrow mut Box exclusive = values.getMut(1); println(values.size()); exclusive.increment(); }",
        "class Box { readonly int read() { return 1; } } void main() { Map<int, Box> values = new Map<int, Box>(); values.put(1, new Box()); borrow Box shared = values.getRef(1); Map<int, Box> moved = move(values); println(shared.read()); }",
        "class Box { readonly int read() { return 1; } } class Shelf { Map<int, Box> values = new Map<int, Box>(); } void main() { Shelf shelf = new Shelf(); shelf.values.put(1, new Box()); borrow Box shared = shelf.values.getRef(1); shelf.values = new Map<int, Box>(); println(shared.read()); }",
    ] {
        assert_rejected(source);
    }
}

#[test]
fn affine_collision_deletion_preserves_survivors_and_transfers_cleanup_once() {
    assert_native(r#"
interface Named { readonly int read(); }
class Box implements Named {
    int value;
    Text marker;
    Box(int value) { this.value = value; this.marker = new Text("owned"); }
    readonly int read() { return this.value; }
}
enum Stored { Boxed(Box); }
void main() {
    Map<int,Box> objects = new Map<int,Box>();
    objects.put(-97119, new Box(10));
    objects.put(-96755, new Box(20));
    objects.put(-96323, new Box(30));
    objects.put(-95065, new Box(40));
    match (objects.tryRemove(-97119)) { None {} Some(value) { println(value.read()); } }
    match (objects.tryRemove(-96323)) { None {} Some(value) { println(value.read()); } }
    {
        borrow Box first = objects.getRef(-96755);
        borrow Box last = objects.getRef(-95065);
        println(first.read()); println(last.read());
    }
    objects.put(-97119, new Box(11));
    match (objects.put(-95065, new Box(44))) { None {} Some(value) { println(value.read()); } }
    objects.clear();
    println(objects.isEmpty());

    Map<int,Stored> enums = new Map<int,Stored>();
    enums.put(-97119, new Stored.Boxed(new Box(10)));
    enums.put(-96755, new Stored.Boxed(new Box(20)));
    enums.put(-96323, new Stored.Boxed(new Box(30)));
    enums.put(-95065, new Stored.Boxed(new Box(40)));
    match (enums.tryRemove(-97119)) { None {} Some(value) { match (value) { Boxed(boxed) { println(boxed.read()); } } } }
    match (enums.tryRemove(-96323)) { None {} Some(value) { match (value) { Boxed(boxed) { println(boxed.read()); } } } }
    {
        borrow Stored first = enums.getRef(-96755);
        borrow Stored last = enums.getRef(-95065);
        match (first) { Boxed(boxed) { println(boxed.read()); } }
        match (last) { Boxed(boxed) { println(boxed.read()); } }
    }
    enums.put(-97119, new Stored.Boxed(new Box(11)));
    match (enums.put(-95065, new Stored.Boxed(new Box(44)))) { None {} Some(value) { match (value) { Boxed(boxed) { println(boxed.read()); } } } }
    enums.clear();
    println(enums.isEmpty());

    Map<int,Named> interfaces = new Map<int,Named>();
    interfaces.put(-97119, new Box(10));
    interfaces.put(-96755, new Box(20));
    interfaces.put(-96323, new Box(30));
    interfaces.put(-95065, new Box(40));
    match (interfaces.tryRemove(-97119)) { None {} Some(value) { println(value.read()); } }
    match (interfaces.tryRemove(-96323)) { None {} Some(value) { println(value.read()); } }
    {
        borrow Named first = interfaces.getRef(-96755);
        borrow Named last = interfaces.getRef(-95065);
        println(first.read()); println(last.read());
    }
    interfaces.put(-97119, new Box(11));
    match (interfaces.put(-95065, new Box(44))) { None {} Some(value) { println(value.read()); } }
    interfaces.clear();
    println(interfaces.isEmpty());
}
"#, b"10\n30\n20\n40\n40\ntrue\n10\n30\n20\n40\n40\ntrue\n10\n30\n20\n40\n40\ntrue\n");
}

#[test]
fn builtin_map_set_key_families_keep_value_semantics() {
    assert_native(r#"
    void main() {
        Map<byte, int> bytes = new Map<byte, int>();
        bytes.put((byte)0, 10);
        bytes.put((byte)255, 20);
        println(bytes.size());
        match (bytes.tryGet((byte)0)) { None {} Some(value) { println(value); } }
        match (bytes.tryGet((byte)255)) { None {} Some(value) { println(value); } }

        Map<char, int> chars = new Map<char, int>();
        chars.put('A', 30);
        chars.put('Ω', 40);
        println(chars.size());
        match (chars.tryGet('Ω')) { None {} Some(value) { println(value); } }
        match (chars.tryGet('A')) { None {} Some(value) { println(value); } }

        Map<long, int> longs = new Map<long, int>();
        longs.put(1L, 1);
        longs.put(4294967297L, 2);
        longs.put(-9223372036854775808L, 3);
        println(longs.size());
        match (longs.tryGet(4294967297L)) { None {} Some(value) { println(value); } }
        match (longs.tryGet(-9223372036854775808L)) { None {} Some(value) { println(value); } }

        Map<double, int> doubles = new Map<double, int>();
        double firstNaN = 0.0 / 0.0;
        double secondNaN = -0.0 / 0.0;
        doubles.put(firstNaN, 50);
        doubles.put(-0.0, 60);
        match (doubles.put(0.0, 61)) { None {} Some(value) { println(value); } }
        match (doubles.put(secondNaN, 51)) { None {} Some(value) { println(value); } }
        println(doubles.size());
        match (doubles.tryGet(firstNaN)) { None {} Some(value) { println(value); } }
        match (doubles.tryGet(-0.0)) { None {} Some(value) { println(value); } }

        Map<boolean, int> flags = new Map<boolean, int>();
        flags.put(true, 1);
        flags.put(false, 2);
        println(flags.size());
        match (flags.tryGet(false)) { None {} Some(value) { println(value); } }

        Map<Unit, int> units = new Map<Unit, int>();
        units.put((), 3);
        match (units.put((), 4)) { None {} Some(value) { println(value); } }
        println(units.size());
        match (units.tryGet(())) { None {} Some(value) { println(value); } }

        Map<String, int> strings = new Map<String, int>();
        strings.put("a\0b", 1);
        strings.put("a\0c", 2);
        match (strings.put("a\0b", 3)) { None {} Some(value) { println(value); } }
        println(strings.size());
        match (strings.tryGet("a\0b")) { None {} Some(value) { println(value); } }
        match (strings.tryGet("a\0c")) { None {} Some(value) { println(value); } }

        Map<Text, int> texts = new Map<Text, int>();
        texts.put(new Text("é"), 70);
        texts.put(new Text("é\0"), 80);
        println(texts.size());
        Text composedQuery = new Text("é");
        Text nulQuery = new Text("é\0");
        match (texts.tryGet(composedQuery)) { None {} Some(value) { println(value); } }
        match (texts.tryGet(nulQuery)) { None {} Some(value) { println(value); } }

        Set<boolean> truth = new Set<boolean>();
        truth.add(true);
        truth.add(true);
        truth.add(false);
        println(truth.size());
        println(truth.contains(false));
        println(truth.remove(true));
        println(truth.size());
    }
    "#, b"2\n10\n20\n2\n40\n30\n3\n2\n3\n60\n50\n2\n51\n61\n2\n2\n3\n1\n4\n1\n2\n3\n2\n2\n70\n80\n2\ntrue\ntrue\n1\n");
}

#[test]
fn custom_class_interface_and_enum_keys_transfer_owned_hashers() {
    assert_native(r#"
    interface Named {
        readonly int id();
        readonly borrow Text label();
    }
    class CollisionKey implements Hasher<CollisionKey> {
        private int identifier;
        private Text labelText;
        CollisionKey(int identifier, Text labelText) {
            this.identifier = identifier;
            this.labelText = labelText;
        }
        public readonly int id() { return this.identifier; }
        public readonly borrow Text label() { return this.labelText; }
        public readonly long hash(borrow CollisionKey value) { return (long)(value.identifier % 2); }
        public readonly boolean equals(borrow CollisionKey left, borrow CollisionKey right) {
            return left.identifier == right.identifier;
        }
    }
    class Record implements Named {
        private int identifier;
        private Text labelText;
        Record(int identifier, Text labelText) {
            this.identifier = identifier;
            this.labelText = labelText;
        }
        public readonly int id() { return this.identifier; }
        public readonly borrow Text label() { return this.labelText; }
    }
    class NamedHasher implements Hasher<Named> {
        private Text marker;
        NamedHasher() { this.marker = new Text("owned policy"); }
        public readonly long hash(borrow Named value) { return (long)(value.id() % 2); }
        public readonly boolean equals(borrow Named left, borrow Named right) {
            return left.id() == right.id();
        }
    }
    enum Shade { Red; Green; Blue; }
    class ShadeHasher implements Hasher<Shade> {
        public readonly long hash(borrow Shade value) {
            match (value) {
                Red { return 1L; }
                Green { return 1L; }
                Blue { return 1L; }
            }
        }
        public readonly boolean equals(borrow Shade left, borrow Shade right) {
            boolean same = false;
            match (left) {
                Red { match (right) { Red { same = true; } Green {} Blue {} } }
                Green { match (right) { Red {} Green { same = true; } Blue {} } }
                Blue { match (right) { Red {} Green {} Blue { same = true; } } }
            }
            return same;
        }
    }
    void main() {
        Map<CollisionKey, int> classKeys =
            Map<CollisionKey, int>.withHasher(new CollisionKey(-1, new Text("class policy")));
        classKeys.put(new CollisionKey(1, new Text("stored")), 10);
        match (classKeys.put(new CollisionKey(1, new Text("incoming")), 20)) {
            None {} Some(value) { println(value); }
        }
        for (borrow CollisionKey key, int value : classKeys) {
            borrow Text label = key.label();
            println(label.equalsString("stored"));
            println(value);
        }
        int id = 2;
        while (id < 16) {
            classKeys.put(new CollisionKey(id, new Text("key")), id);
            id = id + 1;
        }
        println(classKeys.size());
        CollisionKey query = new CollisionKey(8, new Text("query"));
        println(classKeys.containsKey(query));
        match (classKeys.tryRemove(query)) { None {} Some(value) { println(value); } }
        println(classKeys.size());
        classKeys.put(new CollisionKey(8, new Text("replacement")), 88);
        match (classKeys.put(new CollisionKey(8, new Text("discarded")), 99)) {
            None {} Some(value) { println(value); }
        }
        for (borrow CollisionKey key, int value : classKeys) {
            if (key.id() == 8) {
                borrow Text label = key.label();
                println(label.equalsString("replacement"));
                println(value);
            }
        }
        classKeys.clear();
        println(classKeys.isEmpty());

        Map<Named, int> interfaceKeys =
            Map<Named, int>.withHasher(new NamedHasher());
        interfaceKeys.put(new Record(7, new Text("kept")), 70);
        match (interfaceKeys.put(new Record(7, new Text("incoming")), 71)) {
            None {} Some(value) { println(value); }
        }
        for (borrow Named key, int value : interfaceKeys) {
            borrow Text label = key.label();
            println(label.equalsString("kept"));
            println(value);
        }
        Named interfaceQuery = new Record(7, new Text("query"));
        match (interfaceKeys.tryRemove(interfaceQuery)) { None {} Some(value) { println(value); } }
        println(interfaceKeys.isEmpty());

        Set<Shade> shades = Set<Shade>.withHasher(new ShadeHasher());
        shades.add(new Shade.Red());
        shades.add(new Shade.Green());
        shades.add(new Shade.Red());
        println(shades.size());
        Shade green = new Shade.Green();
        println(shades.contains(green));
        Shade redQuery = new Shade.Red();
        println(shades.remove(redQuery));
        boolean sawGreen = false;
        for (borrow Shade shade : shades) {
            match (shade) { Green { sawGreen = true; } Red {} Blue {} }
        }
        println(sawGreen);
        Map<Shade, int> enumKeys = Map<Shade, int>.withHasher(new ShadeHasher());
        enumKeys.put(new Shade.Red(), 10);
        enumKeys.put(new Shade.Green(), 20);
        match (enumKeys.put(new Shade.Red(), 11)) {
            None {} Some(value) { println(value); }
        }
        boolean sawRed = false;
        boolean sawStoredGreen = false;
        for (borrow Shade shade, int value : enumKeys) {
            match (shade) {
                Red { sawRed = value == 11; }
                Green { sawStoredGreen = value == 20; }
                Blue {}
            }
        }
        println(sawRed && sawStoredGreen);
        println(enumKeys.size());
    }
    "#, b"10\ntrue\n20\n15\ntrue\n8\n14\n88\ntrue\n99\ntrue\n70\ntrue\n71\n71\ntrue\n2\ntrue\ntrue\ntrue\n10\ntrue\n2\n");
}


#[test]
fn custom_key_constructors_and_concrete_policy_sendability_are_checked() {
    for source in [
        "class Key {} void main() { Map<Key, int> values = new Map<Key, int>(); }",
        "class Key {} void main() { Set<Key> values = new Set<Key>(); }",
    ] {
        let error = validate(source).expect_err("custom key accepted without a hasher");
        assert!(error.message.contains("E5004"), "{}", error.render("map_key_policy.ncr", source));
    }

    let view_bearing_hasher = r#"
    class Key {}
    class BorrowingHasher implements Hasher<Key> {
        borrow Key captured;
        BorrowingHasher(borrow Key captured) { this.captured = captured; }
        public readonly long hash(borrow Key value) { return 1L; }
        public readonly boolean equals(borrow Key left, borrow Key right) { return true; }
    }
    void main() {}
    "#;
    let error = validate(view_bearing_hasher).expect_err("view-bearing Hasher was erased");
    assert!(error.message.contains("E4005"), "{}", error.render("map_key_policy.ncr", view_bearing_hasher));

    let non_sendable_concrete_policy = r#"
    class Key {}
    class GuardHasher implements Hasher<Key> {
        Guard<int> guard;
        GuardHasher(Guard<int> guard) { this.guard = guard; }
        public readonly long hash(borrow Key value) { return 1L; }
        public readonly boolean equals(borrow Key left, borrow Key right) { return true; }
    }
    class Worker implements Task<Unit> {
        Map<Key, int> values;
        Worker(Map<Key, int> values) { this.values = values; }
        public Unit run() { return (); }
    }
    Result<Unit, Text> sendMap() {
        Mutex<int> mutex = Mutex<int>.create(0)?;
        Guard<int> guard = mutex.lock()?;
        Map<Key, int> values = Map<Key, int>.withHasher(new GuardHasher(guard));
        Thread<Unit> thread = Thread<Unit>.spawn(new Worker(values))?;
        thread.join()?;
        return new Result<Unit, Text>.Ok(());
    }
    void main() { match (sendMap()) { Ok(unit) {} Err(error) { error.print(); } } }
    "#;
    let error = validate(non_sendable_concrete_policy).expect_err("Map hid a non-sendable concrete Hasher");
    assert!(error.message.contains("E7101"), "{}", error.render("map_key_policy.ncr", non_sendable_concrete_policy));
}
