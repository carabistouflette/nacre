use nacre::{compile_llvm_direct_with_flags, compile_to_c, native, validate};
use std::process::Output;

fn run(source: &str) -> Output {
    run_with_allocations(source).0
}

fn run_with_allocations(source: &str) -> (Output, usize, usize) {
    let c = compile_to_c(source).unwrap();
    let c_output = native::compile_c_with_flags(&c, &["-DNACRE_MEMORY_STATS"])
        .unwrap().output().unwrap();
    assert!(c_output.status.success(), "C backend failed: {}", String::from_utf8_lossy(&c_output.stderr));
    let c_allocations = memory_allocations(&c_output);

    let native_program = compile_llvm_direct_with_flags(source, &["-DNACRE_MEMORY_STATS"])
        .unwrap_or_else(|error| panic!("{}", error));
    let output = native_program.output().unwrap();
    assert!(output.status.success(), "direct LLVM backend failed: {}", String::from_utf8_lossy(&output.stderr));
    let direct_allocations = memory_allocations(&output);
    assert_eq!(output.stdout, c_output.stdout);
    (output, c_allocations, direct_allocations)
}

fn memory_allocations(output: &Output) -> usize {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let line = stderr.lines().find(|line| line.starts_with("NACRE_MEMORY "))
        .unwrap_or_else(|| panic!("missing memory report: {stderr}"));
    let fields: Vec<_> = line.split_ascii_whitespace().collect();
    let allocations: usize = fields[1].strip_prefix("allocations=").unwrap().parse().unwrap();
    let frees: usize = fields[2].strip_prefix("frees=").unwrap().parse().unwrap();
    assert_eq!(allocations, frees, "{line}");
    assert_eq!(fields[3], "live=0", "{line}");
    allocations
}

#[test]
fn map_filter_take_fold_is_lazy_and_honors_take_zero() {
    let output = run(r#"
class Emit implements Mapper<int, long> {
    public long apply(int value) { println(value); return value * 2L; }
}
class AtLeastFour implements Predicate<long> {
    public readonly boolean test(long value) { println(value); return value >= 4L; }
}
class AddLong implements Folder<long, long> {
    public long apply(long total, long value) { return total + value; }
}
void main() {
    List<int> values = new List<int>();
    values.add(1); values.add(2); values.add(3); values.add(4); values.add(5); values.add(6);

    long total = values.iter().map(new Emit()).filter(new AtLeastFour()).take(2L).fold(0L, new AddLong());
    println(total);
    long empty = values.iter().map(new Emit()).filter(new AtLeastFour()).take(0L).fold(9L, new AddLong());
    println(empty);
}
"#);
    assert_eq!(output.stdout, b"1\n2\n2\n4\n3\n6\n10\n9\n");
}

#[test]
fn exact_and_filtered_size_hints_report_sound_lower_and_upper_bounds() {
    let output = run(r#"
class KeepPositive implements Predicate<int> {
    public readonly boolean test(int value) { println(value); return value > 0; }
}
class Box {
    private int stored;
    public Box(int value) { this.stored = value; }
}
void expectHint(Pair<long, Option<long>> hint, long expected) {
    match (hint) {
        (lower, upper) {
            assert(lower == expected);
            match (upper) { Some(bound) { assert(bound == expected); } None { assert(false); } }
        }
    }
}
void main() {
    List<int> values = new List<int>();
    values.add(1); values.add(2); values.add(3); values.add(4); values.add(5);

    expectHint(values.iter().sizeHint(), 5L);
    Pair<long, Option<long>> exact = values.iter().sizeHint();
    assert(exact.first == 5L);
    expectHint(values.iter().take(3L).sizeHint(), 3L);
    expectHint(values.iter().enumerate().sizeHint(), 5L);
    long runtimeCount = 3L;
    expectHint(values.iter().take(runtimeCount).sizeHint(), 3L);

    List<int> shortInput = new List<int>();
    shortInput.add(10); shortInput.add(20);
    Array<int> array = new Array<int>(shortInput);
    expectHint(array.iter().sizeHint(), 2L);

    Map<int, int> map = new Map<int, int>();
    map.put(1, 10); map.put(2, 20);
    expectHint(map.iter().sizeHint(), 2L);
    Map<int, Box> objectMap = new Map<int, Box>();
    objectMap.put(1, new Box(10));
    expectHint(objectMap.iter().sizeHint(), 1L);
    expectHint(values.iter().zip(array.iter()).sizeHint(), 2L);

    Pair<long, Option<long>> bounded = values.iter().filter(new KeepPositive()).take(2L).sizeHint();
    match (bounded) {
        (lower, upper) {
            assert(lower == 0L);
            match (upper) { Some(bound) { assert(bound == 2L); } None { assert(false); } }
        }
    }
}
"#);
    assert!(output.stdout.is_empty());
}


#[test]
fn postfix_try_iterator_source_infers_enumerated_item_type() {
    let output = run(r#"
Result<List<int>, Text> makeValues() {
    List<int> values = new List<int>();
    values.add(7);
    return new Result<List<int>, Text>.Ok(values);
}

Result<Pair<long, Option<long>>, Text> hint() {
    Pair<long, Option<long>> size =
        makeValues()?.iter().enumerate().sizeHint();
    return new Result<Pair<long, Option<long>>, Text>.Ok(size);
}

void main() {
    match (hint()) {
        Err(error) {}
        Ok(size) { println(size.first); }
    }
}
"#);
    assert_eq!(output.stdout, b"1\n");
}

#[test]
fn filter_and_take_preserve_pipeline_order() {
    let output = run(r#"
class VisitOdd implements Predicate<int> {
    public readonly boolean test(int value) { println(value); return value % 2 == 1; }
}
class AddInt implements Folder<int, int> {
    public int apply(int total, int value) { return total + value; }
}
void main() {
    List<int> values = new List<int>();
    values.add(1); values.add(2); values.add(3); values.add(4);

    int firstTwo = values.iter().take(2L).filter(new VisitOdd()).fold(0, new AddInt());
    println(firstTwo);
    int firstTwoOdds = values.iter().filter(new VisitOdd()).take(2L).fold(0, new AddInt());
    println(firstTwoOdds);
}
"#);
    assert_eq!(output.stdout, b"1\n2\n1\n1\n2\n3\n4\n");
}

#[test]
fn sibling_iterator_scopes_restore_each_local_type() {
    let output = run(r#"
class DoubleInt implements Mapper<int, int> {
    public int apply(int value) { return value * 2; }
}
class SumInt implements Folder<int, int> {
    public int apply(int total, int value) { return total + value; }
}
class DoubleLong implements Mapper<long, long> {
    public long apply(long value) { return value * 2L; }
}
class SumLong implements Folder<long, long> {
    public long apply(long total, long value) { return total + value; }
}
void main() {
    if (true) {
        List<int> values = new List<int>();
        values.add(3);
        println(values.iter().map(new DoubleInt()).fold(0, new SumInt()));
    } else {
        List<long> values = new List<long>();
        values.add(4L);
        println(values.iter().map(new DoubleLong()).fold(0L, new SumLong()));
    }
}
"#);
    assert_eq!(output.stdout, b"6\n");
}

#[test]
fn iterator_inference_resolves_callback_fields_and_methods() {
    let output = run(r#"
class Doubler implements Mapper<int, int> {
    public int apply(int value) { return value * 2; }
}
class Sum implements Folder<int, int> {
    public int apply(int total, int value) { return total + value; }
}
class Pipeline {
    private Doubler callback;
    public Pipeline() { this.callback = new Doubler(); }
    public borrow mut Doubler mapper() { return callback; }
    public int sumByField() {
        List<int> values = new List<int>();
        values.add(2); values.add(3);
        return values.iter().map(borrow mut this.callback).fold(0, new Sum());
    }
    public int sumByMethod() {
        List<int> values = new List<int>();
        values.add(2); values.add(3);
        return values.iter().map(this.mapper()).fold(0, new Sum());
    }
}
void main() {
    Pipeline pipeline = new Pipeline();
    println(pipeline.sumByField());
    println(pipeline.sumByMethod());
}
"#);
    assert_eq!(output.stdout, b"10\n10\n");
}
#[test]
fn zip_enumerate_and_map_sources_use_the_shared_pair_representation() {
    let output = run(r#"
class Box {
    private int stored;
    public Box(int value) { this.stored = value; }
    public readonly int value() { return this.stored; }
}
class Key {
    private int stored;
    public Key(int value) { this.stored = value; }
    public readonly int value() { return this.stored; }
}
class KeyHasher implements Hasher<Key> {
    public readonly long hash(borrow Key key) { return 1L; }
    public readonly boolean equals(borrow Key left, borrow Key right) {
        return left.value() == right.value();
    }
}
class AddPair implements Folder<int, Pair<int, int>> {
    public int apply(int total, borrow Pair<int, int> pair) {
        return total + pair.first + pair.second;
    }
}
class AddKeyBoxPair implements Folder<int, Pair<Key, Box>> {
    public int apply(int total, borrow Pair<Key, Box> pair) {
        return total + pair.first.value() + pair.second.value();
    }
}
class AddIndexed implements Folder<long, Pair<long, int>> {
    public long apply(long total, borrow Pair<long, int> pair) {
        return total + pair.first + pair.second;
    }
}
class AddInt implements Folder<int, int> {
    public int apply(int total, int value) { return total + value; }
}
void main() {
    List<int> left = new List<int>();
    left.add(1); left.add(2); left.add(3);
    List<int> right = new List<int>();
    right.add(10); right.add(20);
    int zipped = left.iter().zip(right.iter()).fold(0, new AddPair());
    long indexed = left.iter().enumerate().fold(0L, new AddIndexed());

    Map<int, int> values = new Map<int, int>();
    values.put(4, 40); values.put(5, 50);
    Map<Key, Box> objectValues = Map<Key, Box>.withHasher(new KeyHasher());
    objectValues.put(new Key(4), new Box(40)); objectValues.put(new Key(5), new Box(50));
    int objectEntries = objectValues.iter().fold(0, new AddKeyBoxPair());
    int entries = values.iter().fold(0, new AddPair());

    List<int> arrayInput = new List<int>();
    arrayInput.add(7); arrayInput.add(8);
    Array<int> array = new Array<int>(arrayInput);
    int arrayTotal = array.iter().fold(0, new AddInt());
    println(zipped);
    println(indexed);
    println(entries);
    println(objectEntries);
    println(arrayTotal);
}
"#);
    assert_eq!(output.stdout, b"33\n9\n99\n99\n15\n");
}
#[test]
fn zip_does_not_pull_right_after_left_pipeline_exhausts() {
    let output = run(r#"
class TraceOdd implements Predicate<int> {
    public readonly boolean test(int value) { println(value); return value % 2 == 1; }
}
class TraceRight implements Mapper<int, int> {
    public int apply(int value) { println(value); return value; }
}
class AddPair implements Folder<int, Pair<int, int>> {
    public int apply(int total, borrow Pair<int, int> pair) {
        return total + pair.first + pair.second;
    }
}
void main() {
    List<int> left = new List<int>();
    left.add(1); left.add(2); left.add(3); left.add(4);
    List<int> right = new List<int>();
    right.add(10); right.add(20); right.add(30);

    int total = left.iter()
        .filter(new TraceOdd())
        .zip(right.iter().map(new TraceRight()))
        .fold(0, new AddPair());
    println(total);
}
"#);
    assert_eq!(output.stdout, b"1\n10\n2\n3\n20\n4\n34\n");
}

#[test]
fn affine_items_are_borrowed_and_remain_in_the_collection_after_fold() {
    let output = run(r#"
class Box {
    private int stored;
    public Box(int value) { this.stored = value; }
    public readonly int value() { return this.stored; }
}
class Positive implements Predicate<Box> {
    public readonly boolean test(borrow Box value) { return value.value() > 0; }
}
class ReadValue implements RefMapper<Box, int> {
    public int apply(borrow Box value) { return value.value(); }
}
class AddInt implements Folder<int, int> {
    public int apply(int total, int value) { return total + value; }
}
void main() {
    List<Box> values = new List<Box>();
    values.add(new Box(3)); values.add(new Box(-1)); values.add(new Box(4));
    int total = values.iter().filter(new Positive()).map(new ReadValue()).fold(0, new AddInt());
    assert(total == 7);
    {
        borrow Box first = values.getRef(0);
        assert(first.value() == 3);
    }
    values.add(new Box(5));
    assert(values.size() == 4);
    List<Box> arrayInput = new List<Box>();
    arrayInput.add(new Box(8)); arrayInput.add(new Box(-2));
    Array<Box> array = new Array<Box>(arrayInput);
    int arrayTotal = array.iter().filter(new Positive()).map(new ReadValue()).fold(0, new AddInt());
    assert(arrayTotal == 8);
    println(total);
}
"#);
    assert_eq!(output.stdout, b"7\n");
}


#[test]
fn owned_enum_map_filter_fold_borrows_inline_results() {
    let output = run(r#"
enum Reading { Value(int); Empty; }
class MakeReading implements Mapper<int, Reading> {
    public Reading apply(int value) {
        if (value > 0) { return new Reading.Value(value); }
        return new Reading.Empty();
    }
}
class KeepPositiveReading implements Predicate<Reading> {
    public readonly boolean test(borrow Reading reading) {
        match (reading) {
            Value(value) { return value > 0; }
            Empty { return false; }
        }
    }
}
class SumReading implements Folder<int, Reading> {
    public int apply(int total, borrow Reading reading) {
        match (reading) {
            Value(value) { return total + value; }
            Empty { return total; }
        }
    }
}
void main() {
    List<int> values = new List<int>();
    values.add(1); values.add(-2); values.add(3);
    int total = values.iter()
        .map(new MakeReading())
        .filter(new KeepPositiveReading())
        .fold(0, new SumReading());
    println(total);
}
"#);
    assert_eq!(output.stdout, b"4\n");
}
#[test]
fn iterator_pipelines_cannot_escape_and_cannot_mutate_the_borrowed_source() {
    let escaping = r#"
void main() {
    List<int> values = new List<int>();
    Iterator<int> cursor = values.iter();
}
"#;
    assert!(validate(escaping).is_err(), "a borrowed compiler pipeline must not escape into Iterator<T>");

    let mutation = r#"
long appendWhileBorrowed(borrow mut List<int> values) {
    values.add(99);
    return 1L;
}
void main() {
    List<int> values = new List<int>();
    values.add(1);
    int total = values.iter().take(appendWhileBorrowed(values)).fold(0, new Sum());
}
class Sum implements Folder<int, int> {
    public int apply(int total, int value) { return total + value; }
}
"#;
    assert!(validate(mutation).is_err(), "mutation must conflict with the terminal source borrow");

    let item_mutation = r#"
class Box {
    public void change() {}
}
class Mutate implements RefMapper<Box, int> {
    public int apply(borrow Box value) { value.change(); return 0; }
}
void main() {
    List<Box> values = new List<Box>();
    int result = values.iter().map(new Mutate()).fold(0, new Sum());
}
class Sum implements Folder<int, int> {
    public int apply(int total, int value) { return total + value; }
}
"#;
    assert!(validate(item_mutation).is_err(), "shared iteration cannot mutate affine source items");
}

#[test]
fn fused_pipeline_adds_no_adapter_allocations_over_direct_iteration() {
    let definitions = r#"
class Twice implements Mapper<int, int> {
    public int apply(int value) { return value * 2; }
}
class AboveFive implements Predicate<int> {
    public readonly boolean test(int value) { return value > 5; }
}
class Add implements Folder<int, int> {
    public int apply(int total, int value) { return total + value; }
}
"#;
    let fused = format!(r#"{definitions}
void main() {{
    List<int> values = new List<int>();
    values.add(1); values.add(2); values.add(3); values.add(4); values.add(5); values.add(6);
    Mapper<int, int> mapper = new Twice();
    Predicate<int> predicate = new AboveFive();
    Folder<int, int> reducer = new Add();
    int result = values.iter().map(mapper).filter(predicate).take(2L).fold(0, reducer);
    println(result);
}}
"#);
    let direct = format!(r#"{definitions}
void main() {{
    List<int> values = new List<int>();
    values.add(1); values.add(2); values.add(3); values.add(4); values.add(5); values.add(6);
    Mapper<int, int> mapper = new Twice();
    Predicate<int> predicate = new AboveFive();
    Folder<int, int> reducer = new Add();
    int result = 0;
    int taken = 0;
    for (int value : values) {{
        if (taken == 2) {{ break; }}
        int mapped = mapper.apply(value);
        if (predicate.test(mapped)) {{ result = reducer.apply(result, mapped); taken = taken + 1; }}
    }}
    println(result);
}}
"#);
    let (fused_output, fused_c_allocations, fused_llvm_allocations) = run_with_allocations(&fused);
    let (direct_output, direct_c_allocations, direct_llvm_allocations) = run_with_allocations(&direct);
    assert_eq!(fused_output.stdout, b"14\n");
    assert_eq!(direct_output.stdout, fused_output.stdout);
    assert_eq!(fused_c_allocations, direct_c_allocations);
    assert_eq!(fused_llvm_allocations, direct_llvm_allocations);
}
