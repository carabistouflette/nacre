mod support;

fn assert_memory_balanced(stderr: &[u8]) {
    let report = String::from_utf8_lossy(stderr);
    let mut fields = report.split_whitespace();
    assert_eq!(fields.next(), Some("NACRE_MEMORY"), "{report}");
    let allocations = fields.next().unwrap().strip_prefix("allocations=").unwrap();
    let frees = fields.next().unwrap().strip_prefix("frees=").unwrap();
    assert_eq!(allocations, frees, "{report}");
    assert_eq!(fields.next(), Some("live=0"), "{report}");
    assert_eq!(fields.next(), None, "{report}");
}

#[test]
fn deep_linear_enum_owned_chain_drops_without_recursion() {
    let source = r#"
enum Link { End; Next(Node); }
class Node {
    private Link next;
    public Node(Link next) { this.next = next; }
}
void main() {
    Node node = new Node(new Link.End());
    int i = 0;
    while (i < 200000) {
        node = new Node(new Link.Next(move(node)));
        i = i + 1;
    }
    Node root = new Node(new Link.Next(move(node)));
}
"#;
    let programs = [
        support::compile(source, &["-DNACRE_MEMORY_STATS"]).unwrap(),
        support::compile_optimized(source, "-O0", &["-DNACRE_MEMORY_STATS"]).unwrap(),
    ];
    for program in programs {
        let result = program.output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(result.stdout.is_empty());
        assert_memory_balanced(&result.stderr);
    }
}
