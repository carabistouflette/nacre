use crate::c_repr::ctype;
use crate::ir::{self as t, NativeClass as C, NativeMethod as M, Type};
use super::{
    support::{marker, native_method, option_tags, replace_method_ids, replace_placeholders},
};


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

pub(super) fn implementation(program: &t::Program, class_id: usize) -> String {
    let constructor = native_method(program, class_id, |candidate| candidate == M::BitSetNew);
    assert_eq!(constructor.params.first().map(|(_, ty)| *ty), Some(Type::Long),
        "BitSet constructor takes the initial bit count");
    let next = native_method(program, class_id, |candidate| candidate == M::BitSetNextSetBit);
    let (none_tag, some_tag) = option_tags(program, next, Type::Long);
    let clone = native_method(program, class_id, |candidate| candidate == M::BitSetClone);
    assert_eq!(clone.return_type, Type::Object(class_id), "BitSet.clone returns a new BitSet");
    let and = native_method(program, class_id, |candidate| candidate == M::BitSetAnd);
    let or = native_method(program, class_id, |candidate| candidate == M::BitSetOr);
    let xor = native_method(program, class_id, |candidate| candidate == M::BitSetXor);
    let and_not = native_method(program, class_id, |candidate| candidate == M::BitSetAndNot);
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
