use nacre::{emit_borrows, emit_flow, validate};

const CLASS: &str = "class C { int n = 1; readonly int get() { return n; } void inc() { n = n + 1; } } ";

#[test]
fn diagnostic_points_to_the_conflicting_loan() {
    let source = format!("{CLASS}void main() {{ C c = new C(); borrow C reader = c; c.inc(); println(reader.get()); }}");
    let error = validate(&source).unwrap_err();
    assert!(error.message.contains("E3001"));
    let rendered = error.render("borrow.ncr", &source);
    assert!(rendered.contains("note:"));
    assert!(rendered.contains("help:"));
    assert!(error.notes[0].0.start < error.span.start);
}
#[test]
fn destruction_has_a_distinct_diagnostic() {
    let source = format!("{CLASS}void main() {{ C c = new C(); borrow C r = c; drop(c); println(r.get()); }}");
    assert!(validate(&source).unwrap_err().message.contains("E3002"));
}
#[test]
fn unused_borrow_does_not_block_owner_access() {
    let source = format!("{CLASS}void main() {{ C c = new C(); borrow C unused = c; c.inc(); drop(c); }}");
    validate(&source).unwrap_or_else(|e| panic!("{}", e.render("nll.ncr", &source)));
}
#[test]
fn dump_is_deterministic_and_contains_real_loan_events() {
    let source = include_str!("../examples/ownership/reborrowing.ncr");
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
fn local_borrow_releases_after_its_last_use() {
    let source = format!("{CLASS}void main() {{ C c = new C(); borrow C reader = c; println(reader.get()); c.inc(); }}");
    validate(&source).unwrap_or_else(|e| panic!("{}", e.render("nll.ncr", &source)));
}

#[test]
fn branch_last_uses_are_path_sensitive() {
    let source = format!("{CLASS}void main() {{ C c = new C(); borrow C reader = c; boolean choose = true; if (choose) {{ c.inc(); }} else {{ println(reader.get()); }} }}");
    validate(&source).unwrap_or_else(|e| panic!("{}", e.render("nll.ncr", &source)));
}

#[test]
fn a_borrow_stays_live_for_a_later_use_in_the_same_branch() {
    let source = format!("{CLASS}void main() {{ C c = new C(); borrow C reader = c; boolean choose = true; if (choose) {{ c.inc(); println(reader.get()); }} }}");
    let error = validate(&source).unwrap_err();
    assert!(error.message.contains("E3001"), "{}", error.render("nll.ncr", &source));
}

#[test]
fn loop_carried_borrow_releases_after_loop_exit() {
    let source = format!("{CLASS}void main() {{ C c = new C(); borrow C reader = c; int i = 0; while (i < 2) {{ println(reader.get()); i = i + 1; }} c.inc(); }}");
    validate(&source).unwrap_or_else(|e| panic!("{}", e.render("nll.ncr", &source)));
}

#[test]
fn loop_back_edge_keeps_borrow_live_for_next_iteration() {
    let source = format!("{CLASS}void main() {{ C c = new C(); borrow C reader = c; int i = 0; while (i < 2) {{ c.inc(); println(reader.get()); i = i + 1; }} }}");
    let error = validate(&source).unwrap_err();
    assert!(error.message.contains("E3001"), "{}", error.render("nll.ncr", &source));
}

#[test]
fn derived_borrow_and_local_view_release_after_last_use() {
    let source = format!("{CLASS}class View {{ borrow C target; View(borrow C target) {{ this.target = target; }} readonly int get() {{ return this.target.get(); }} }} void main() {{ C c = new C(); borrow C parent = c; borrow C child = parent; println(child.get()); c.inc(); View view = new View(c); println(view.get()); c.inc(); }}");
    validate(&source).unwrap_or_else(|e| panic!("{}", e.render("nll.ncr", &source)));
}

#[test]
fn a_view_keeps_its_loan_while_a_later_use_remains() {
    let source = format!("{CLASS}class View {{ borrow C target; View(borrow C target) {{ this.target = target; }} readonly int get() {{ return this.target.get(); }} }} void main() {{ C c = new C(); View view = new View(c); c.inc(); println(view.get()); }}");
    let error = validate(&source).unwrap_err();
    assert!(error.message.contains("E3001"), "{}", error.render("nll.ncr", &source));
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

#[test]
fn transitive_view_propagation_rejects_outer_fields_before_inner_fields() {
    let source = "class Outer {\n    Middle middle;\n}\nclass Middle {\n    View view;\n}\nclass View {\n    borrow Target target;\n}\nclass Target {}\nvoid main() {}";
    let error = validate(source).expect_err("a transitive local view was stored by value");
    assert!(
        error.message.contains("[E3007]"),
        "{}",
        error.render("case.ncr", source)
    );
    assert_eq!(error.span.start, source.find("Middle middle").unwrap());
}
