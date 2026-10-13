use nacre::{emit_borrows, emit_flow, validate};

const CLASS: &str = "class C { int n = 1; readonly int get() { return n; } void inc() { n = n + 1; } } ";

#[test]
fn diagnostic_points_to_the_conflicting_loan() {
    let source = format!("{CLASS}void main() {{ C c = new C(); borrow C reader = c; c.inc(); }}");
    let error = validate(&source).unwrap_err();
    assert!(error.message.contains("E3001"));
    let rendered = error.render("borrow.ncr", &source);
    assert!(rendered.contains("note:"));
    assert!(rendered.contains("help:"));
    assert!(error.notes[0].0.start < error.span.start);
}
#[test]
fn destruction_has_a_distinct_diagnostic() {
    let source = format!("{CLASS}void main() {{ C c = new C(); borrow C r = c; drop(c); }}");
    assert!(validate(&source).unwrap_err().message.contains("E3002"));
}
#[test]
fn dump_is_deterministic_and_contains_real_loan_events() {
    let source = include_str!("../examples/reborrowing.ncr");
    let dump = emit_borrows(source).unwrap();
    assert_eq!(dump, emit_borrows(source).unwrap());
    for token in ["exclusive", "shared", "parent=Some", "end loan", "scope exit", "activate receiver"] {
        assert!(dump.contains(token), "missing {token}:\n{dump}");
    }
}
#[test]
fn both_analysis_commands_reject_invalid_borrows() {
    let source = include_str!("../examples/invalid/borrow_then_drop.ncr");
    assert!(emit_borrows(source).unwrap_err().message.contains("E3002"));
    assert!(emit_flow(source).unwrap_err().message.contains("E3002"));
}
#[test]
fn all_same_root_parameter_modes_obey_alias_rules() {
    // This small matrix covers consumption as well as shared/exclusive aliases.
    let qualifiers = ["", "borrow ", "borrow mut "];
    let expected = [
        [Some("E2001"), Some("E2001"), Some("E2001")],
        [Some("E3002"), None, Some("E3001")],
        [Some("E3002"), Some("E3001"), Some("E3001")],
    ];
    for (a, qa) in qualifiers.iter().enumerate() {
        for (b, qb) in qualifiers.iter().enumerate() {
            let source = format!("class C {{}} void f({qa}C a, {qb}C b) {{}} void main() {{ C c = new C(); f(c, c); }}");
            match expected[a][b] {
                None => validate(&source).unwrap(),
                Some(code) => assert!(validate(&source).unwrap_err().message.contains(code), "{source}"),
            }
        }
    }
}
#[test]
fn all_distinct_root_parameter_modes_are_valid() {
    for a in ["", "borrow ", "borrow mut "] {
        for b in ["", "borrow ", "borrow mut "] {
            let source = format!("class C {{}} void f({a}C a, {b}C b) {{}} void main() {{ C a = new C(); C b = new C(); f(a, b); }}");
            validate(&source).unwrap_or_else(|e| panic!("{}", e.render("matrix.ncr", &source)));
        }
    }
}
#[test]
fn inner_borrows_do_not_escape_both_branches_or_loop_edges() {
    let source = format!("{CLASS}void main() {{ C c = new C(); int n = 0; while (n < 2) {{ borrow mut C r = c; r.inc(); n = n + 1; if (n == 1) {{ continue; }} else {{ break; }} }} c.inc(); }}");
    validate(&source).unwrap();
}
#[test]
fn receiver_reservation_and_activation_have_correct_order() {
    let source = "class C { int n = 1; int inc() { n = n + 1; return n; } void use(int n) {} } void main() { C c = new C(); c.use(c.inc()); }";
    let trace = emit_borrows(source).unwrap();
    let main = trace.split(" main {").nth(1).unwrap();
    let first_reservation = main.find("reserve receiver").unwrap();
    let second_reservation = main[first_reservation + 1..].find("reserve receiver").unwrap() + first_reservation + 1;
    let activation = main.find("activate receiver").unwrap();
    assert!(first_reservation < second_reservation && second_reservation < activation);
}
