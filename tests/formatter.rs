mod support;

#[test]
fn formatter_preserves_shift_operators_and_nested_generic_closers() {
    let source = r#"
void main() {
    List<List<int>> nested = new List<List<int>>();
    int left = 1 << 3;
    int right = 16 >> 2;
    println(left);
    println(right);
}
"#;

    let formatted = nacre::fmt::format_source(source).expect("valid source formats");
    let program = support::compile(&formatted, &[]).expect("formatted source compiles");
    let output = program.output().expect("formatted program runs");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"8\n4\n");
}
