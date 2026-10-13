use nacre::{compile_to_c, native, validate};
use std::process::Output;

const ARRAY_EXAMPLE: &str = include_str!("../examples/collections/arrays.ncr");
const BUFFER_EXAMPLE: &str = include_str!("../examples/io/byte_buffer.ncr");
const ARRAY_OUTPUT: &[u8] = b"5\n9\nnone\n0\n0\n0\n0\n0\nfalse\n()\n4\n20\n21\n30\n0\n0\n0\n";
const BUFFER_OUTPUT: &[u8] = b"negative capacity rejected\n0\n0\n40\n64\n0\n0\n39\n()\n255\n0\n65\n41\n255\ninvalid truncate\n0\ntrue\n";

fn execute(source: &str, flags: &[&str]) -> Output {
    let c = compile_to_c(source)
        .unwrap_or_else(|error| panic!("{}", error.render("arrays_buffers.ncr", source)));
    native::compile_c_with_flags(&c, flags)
        .unwrap_or_else(|error| panic!("native compile failed: {error}"))
        .output()
        .unwrap_or_else(|error| panic!("native execution failed: {error}"))
}

fn assert_memory_balanced(output: &Output) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let line = stderr.lines().find(|line| line.starts_with("NACRE_MEMORY "))
        .unwrap_or_else(|| panic!("missing memory report:\n{stderr}"));
    let mut fields = line.split_whitespace();
    assert_eq!(fields.next(), Some("NACRE_MEMORY"));
    let allocations = fields.next().unwrap().strip_prefix("allocations=").unwrap();
    let frees = fields.next().unwrap().strip_prefix("frees=").unwrap();
    assert_eq!(allocations, frees, "{line}");
    assert_eq!(fields.next(), Some("live=0"), "{line}");
    assert_eq!(fields.next(), None, "{line}");
}

#[test]
fn arrays_example_covers_copy_affine_inline_sum_interface_and_typed_factories() {
    let output = execute(ARRAY_EXAMPLE, &["-DNACRE_MEMORY_STATS"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, ARRAY_OUTPUT);
    assert_memory_balanced(&output);
}

#[test]
fn list_backing_storage_transfers_without_copy_and_fixed_array_drops_it() {
    let output = execute(
        r#"
void main() {
    List<int> values = new List<int>();
    values.reserve(128);
    values.add(41);
    Array<int> fixed = new Array<int>(values);
    println(fixed.size());
    println(fixed.get(0));
}
"#,
        &["-DNACRE_MEMORY_STATS"],
    );
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"1\n41\n");
    assert_eq!(output.stderr, b"NACRE_MEMORY allocations=3 frees=3 live=0\n");
}

#[test]
fn byte_buffer_example_covers_empty_growth_embedded_nul_copy_truncate_and_clear() {
    let output = execute(BUFFER_EXAMPLE, &["-DNACRE_MEMORY_STATS"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, BUFFER_OUTPUT);
    assert_memory_balanced(&output);
}

#[test]
fn byte_buffer_growth_allocates_geometrically_not_per_append() {
    let output = execute(
        r#"
void main() {
    match (ByteBuffer.withCapacity(0L)) {
        Err(error) { println("unexpected factory error"); }
        Ok(buffer) {
            int index = 0;
            while (index < 40) {
                buffer.append((byte)index);
                index = index + 1;
            }
            println(buffer.length());
            println(buffer.capacity());
        }
    }
}
"#,
        &["-DNACRE_MEMORY_STATS"],
    );
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"40\n64\n");
    assert_eq!(output.stderr, b"NACRE_MEMORY allocations=4 frees=4 live=0\n");
}

#[test]
fn array_and_buffer_bounds_fail_at_runtime() {
    for (source, expected) in [
        (
            "void main() { Array<int> values = Arrays.zeroedInts(1); println(values.get(-1)); }",
            "Array index out of bounds",
        ),
        (
            "void main() { match (ByteBuffer.withCapacity(0L)) { Err(error) { println(0); } Ok(buffer) { println(buffer.get(0L)); } } }",
            "ByteBuffer index out of bounds",
        ),
    ] {
        let output = execute(source, &[]);
        assert_eq!(output.status.code(), Some(70), "{}", String::from_utf8_lossy(&output.stderr));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(expected), "{stderr}");
        assert!(stderr.contains("[1:"), "{stderr}");
    }
}

#[test]
fn array_accessors_traversal_and_opaque_buffer_construction_obey_ownership_rules() {
    for source in [
        "void main() { Array<int> values = Arrays.zeroedInts(1); values.getRef(0); }",
        "void main() { Array<int> values = Arrays.zeroedInts(1); values.getMut(0); }",
        "class Box {} void main() { List<Box> source = new List<Box>(); source.add(new Box()); Array<Box> values = new Array<Box>(source); values.get(0); }",
        "class Box {} void main() { List<Box> source = new List<Box>(); source.add(new Box()); Array<Box> values = new Array<Box>(source); values.tryGet(0); }",
        "class Box {} void main() { List<Box> source = new List<Box>(); Array<Box> values = new Array<Box>(source); for (Box value : values) { } }",
        "class Box {} void main() { List<Box> source = new List<Box>(); Array<Box> values = new Array<Box>(source); for (borrow Box value : values) { values.set(0, new Box()); } }",
        "class Box {} void main() { List<Box> source = new List<Box>(); Array<Box> values = new Array<Box>(source); for (borrow Box value : values) { drop(values); break; } }",
        "void main() { ByteBuffer buffer = new ByteBuffer(); }",
    ] {
        assert!(validate(source).is_err(), "accepted invalid program: {source}");
    }
}

#[test]
fn array_and_buffer_examples_pass_address_and_undefined_behavior_sanitizers() {
    for (source, expected) in [(ARRAY_EXAMPLE, ARRAY_OUTPUT), (BUFFER_EXAMPLE, BUFFER_OUTPUT)] {
        let output = execute(source, &["-fsanitize=address,undefined", "-fno-omit-frame-pointer"]);
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(output.stdout, expected);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!stderr.contains("AddressSanitizer"), "{stderr}");
        assert!(!stderr.contains("runtime error:"), "{stderr}");
    }
}
