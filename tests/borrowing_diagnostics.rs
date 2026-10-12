use nacre::{compile_to_c, emit_borrows, emit_flow, validate};

const CLASS: &str = "class C { int n = 1; readonly int get() { return n; } void inc() { n = n + 1; } } ";

#[test]
fn diagnostic_points_to_the_conflicting_loan() {
    let source = format!("{CLASS}void main() {{ C c = new C(); borrow C reader = c; c.inc(); }}");
    let error = validate(&source).unwrap_err();
    assert!(error.message.contains("E3001"));
    assert!(error.notes.iter().any(|(_, n)| n.contains("reader")));
    let rendered = error.render("borrow.ncr", &source);
    assert!(rendered.contains("note:"));
    assert!(rendered.contains("aide:"));
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
fn borrowed_local_is_not_an_owner_in_flow_graph() {
    let source = format!("{CLASS}void main() {{ C c = new C(); borrow C reader = c; println(reader.n); }}");
    let graph = emit_flow(&source).unwrap();
    assert!(graph.contains("initialize/replace %1 reader"));
    assert!(!graph.contains("drop-if-present %1 reader"));
}
#[test]
fn borrowed_parameter_has_no_destructor_in_generated_function() {
    for qualifier in ["borrow", "borrow mut"] {
        let source = format!("class C {{}} void inspect({qualifier} C c) {{}} void main() {{ C c = new C(); inspect(c); }}");
        let c = compile_to_c(&source).unwrap();
        // Find the definition, not the forward declaration or a call.
        let start = c.rfind("static void ncr_fn_0(").unwrap();
        let body = c[start..].split("\n}\n").next().unwrap();
        assert!(!body.contains("ncr_drop_"), "borrowed parameter freed: {body}");
        assert!(body.contains("return;"));
    }
}
#[test]
fn borrowed_field_is_not_freed_but_owned_field_is() {
    let source = "class C {} class V { borrow C target; C child = new C(); V(borrow C c) { target = c; } } void main() { C c = new C(); V v = new V(c); }";
    let c = compile_to_c(source).unwrap();
    let start = c.find("static void ncr_drop_1(ncr_obj_1 * p) {").unwrap();
    let body = c[start..].split("\n}\n").next().unwrap();
    assert!(!body.contains("ncr_drop_0(p->ncr_f_0)"));
    assert!(body.contains("ncr_drop_0(p->ncr_f_1)"));
    assert!(body.contains("ncr_deallocate(p)"));
}
#[test]
fn borrowed_arguments_do_not_null_the_owner() {
    let source = "class C {} void f(borrow C c) {} void main() { C c = new C(); f(c); f(c); }";
    let c = compile_to_c(source).unwrap();
    assert!(!c.contains("ownership transferred"));
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
