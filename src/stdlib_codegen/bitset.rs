use crate::codegen::ctype;
use crate::ir::{self as t, NativeClass as C, NativeMethod as M, Type};
use super::{
    support::{marker, replace_method_ids, replace_placeholders, result_id, variant},
};

fn native_method(program: &t::Program, receiver: usize, method: M) -> Option<&t::Function> {
    program.functions.iter().find(|function| {
        function.receiver == Some(receiver) && function.native == Some(method)
    })
}

fn required_method(program: &t::Program, receiver: usize, method: M) -> &t::Function {
    native_method(program, receiver, method).expect("compiler-owned BitSet method is present")
}

pub(super) fn fields(_program: &t::Program, class: C, _class_id: usize) -> String {
    match class {
        C::BitSet => "    uint64_t *words;\n    int64_t word_count;\n    int64_t capacity;\n".to_owned(),
        _ => unreachable!("BitSet fields called for another native class"),
    }
}

pub(super) fn drop_fields(_program: &t::Program, class: C, _class_id: usize) -> String {
    match class {
        C::BitSet => "    ncr_deallocate(p->words);\n".to_owned(),
        _ => unreachable!("BitSet drops called for another native class"),
    }
}

fn option_tags(program: &t::Program, function: &t::Function) -> (usize, usize) {
    let option = result_id(function);
    let (none_tag, none) = variant(program, option, "None");
    let (some_tag, some) = variant(program, option, "Some");
    assert!(none.fields.is_empty(), "Option None has no payload");
    assert!(matches!(some.fields.as_slice(), [Type::Long]),
        "BitSet.nextSetBit returns Option<long>");
    (none_tag, some_tag)
}

pub(super) fn implementation(program: &t::Program, class_id: usize) -> String {
    let constructor = required_method(program, class_id, M::BitSetNew);
    assert_eq!(constructor.params.first().map(|(_, ty)| *ty), Some(Type::Long),
        "BitSet constructor takes the initial bit count");
    let next = required_method(program, class_id, M::BitSetNextSetBit);
    let (none_tag, some_tag) = option_tags(program, next);
    let clone = required_method(program, class_id, M::BitSetClone);
    assert_eq!(clone.return_type, Type::Object(class_id), "BitSet.clone returns a new BitSet");
    let and = required_method(program, class_id, M::BitSetAnd);
    let or = required_method(program, class_id, M::BitSetOr);
    let xor = required_method(program, class_id, M::BitSetXor);
    let and_not = required_method(program, class_id, M::BitSetAndNot);
    for function in [and, or, xor, and_not] {
        assert_eq!(function.params.first().map(|(_, ty)| *ty),
            Some(Type::Borrow { class: class_id, mutable: false }),
            "BitSet logical operations take a borrowed peer");
    }

    let mut body = include_str!("../../stdlib/bit_set.c.in").to_owned();
    let substitutions = [
        ("@ID@", class_id.to_string()),
        ("@BIT_SET_NEW@", marker(M::BitSetNew).to_owned()),
        ("@BIT_SET_GET@", marker(M::BitSetGet).to_owned()),
        ("@BIT_SET_SET@", marker(M::BitSetSet).to_owned()),
        ("@BIT_SET_CLEAR@", marker(M::BitSetClear).to_owned()),
        ("@BIT_SET_FLIP@", marker(M::BitSetFlip).to_owned()),
        ("@BIT_SET_SET_RANGE@", marker(M::BitSetSetRange).to_owned()),
        ("@BIT_SET_CLEAR_RANGE@", marker(M::BitSetClearRange).to_owned()),
        ("@BIT_SET_FLIP_RANGE@", marker(M::BitSetFlipRange).to_owned()),
        ("@BIT_SET_NEXT_SET_BIT@", marker(M::BitSetNextSetBit).to_owned()),
        ("@BIT_SET_CARDINALITY@", marker(M::BitSetCardinality).to_owned()),
        ("@BIT_SET_LENGTH@", marker(M::BitSetLength).to_owned()),
        ("@BIT_SET_IS_EMPTY@", marker(M::BitSetIsEmpty).to_owned()),
        ("@BIT_SET_CLONE@", marker(M::BitSetClone).to_owned()),
        ("@BIT_SET_AND@", marker(M::BitSetAnd).to_owned()),
        ("@BIT_SET_OR@", marker(M::BitSetOr).to_owned()),
        ("@BIT_SET_XOR@", marker(M::BitSetXor).to_owned()),
        ("@BIT_SET_AND_NOT@", marker(M::BitSetAndNot).to_owned()),
        ("@OPTION_LONG_TYPE@", ctype(program, next.return_type)),
        ("@NONE_TAG@", none_tag.to_string()),
        ("@SOME_TAG@", some_tag.to_string()),
    ];
    body = replace_placeholders(body, &substitutions);
    body = replace_method_ids(body, program, Some(class_id));
    debug_assert!(!body.contains('@'), "unexpanded BitSet template placeholder");
    body
}
