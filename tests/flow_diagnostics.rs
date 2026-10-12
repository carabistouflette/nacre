use nacre::{compile_to_c, emit_flow, validate};

#[test]
fn move_diagnostic_contains_the_transfer_origin() {
    let source = "class Box { int n = 1; } void main() { Box a = new Box(); drop(a); println(a.n); }";
    let error = validate(source).unwrap_err();
    assert!(error.message.contains("E2001"));
    assert!(error.notes.iter().any(|(_, n)| n.contains("transfert de propriete")));
    let rendered = error.render("case.ncr", source);
    assert!(rendered.contains("note:"));
    assert!(rendered.contains("aide:"));
}
#[test]
fn uninitialized_diagnostic_is_distinct_from_move() {
    let error = validate("void main() { int n; println(n); }").unwrap_err();
    assert!(error.message.contains("E2002"));
    assert!(!error.message.contains("E2001"));
}
#[test]
fn graph_contains_loop_moves_and_conditional_cleanup() {
    let flow = emit_flow(include_str!("../examples/ownership_loop.ncr")).unwrap();
    for expected in ["loop-header", "loop-exit", "move %", "drop-if-present", "initialize/replace", "in=["] {
        assert!(flow.contains(expected), "missing {expected}:\n{flow}");
    }
}
#[test]
fn graph_is_deterministic() {
    let source = include_str!("../examples/ownership_continue.ncr");
    assert_eq!(emit_flow(source).unwrap(), emit_flow(source).unwrap());
}
#[test]
fn graph_rejects_invalid_programs() {
    assert!(emit_flow(include_str!("../examples/invalid/loop_use_after_move.ncr")).is_err());
}
#[test]
fn uninitialized_object_storage_has_a_null_drop_flag() {
    let c = compile_to_c("class Box {} void main() { Box a; }").unwrap();
    assert!(c.contains("ncr_v_0 = NULL;"));
    assert!(c.contains("ncr_drop_0(ncr_v_0)"));
}
#[test]
fn explicit_drop_follows_the_move_in_generated_c() {
    let c = compile_to_c("class Box {} void main() { Box a = new Box(); drop(a); }").unwrap();
    let moved = c.find("ownership transferred").unwrap();
    assert!(c[moved..].contains("ncr_drop_0(ncr_t_"));
}
#[test]
fn source_errors_are_reported_before_invoking_a_backend() {
    let source = "class Box {} void main() { Box a = new Box(); drop(a); drop(a); }";
    assert!(compile_to_c(source).unwrap_err().message.contains("E2001"));
}
