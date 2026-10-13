use crate::ir::{self as t, NativeClass as C, NativeMethod as M, Type};
use super::support::{native_function, replace_method_ids, replace_placeholders, result_id, variant, variant_id};

pub(super) fn fields() -> String {
    "    uint64_t state;\n".to_owned()
}

pub(super) fn drop_fields() -> String { String::new() }

pub(super) fn implementation(program: &t::Program) -> String {
    let random_class = program.classes.iter()
        .find(|class| matches!(class.native, Some(C::Random)))
        .expect("Random native class is present").id;
    let from_system = native_function(program, |method| matches!(method, M::RandomFromSystem));
    let next_int = native_function(program, |method| matches!(method, M::RandomNextInt));
    let system_result = result_id(from_system);
    let int_result = result_id(next_int);
    let (ok_tag, ok_variant) = variant(program, system_result, "Ok");
    assert!(matches!(ok_variant.fields.as_slice(), [Type::Object(id)] if *id == random_class),
        "Random.fromSystem must return an owning Random");

    let mut body = include_str!("../../stdlib/random.c.in").to_owned();
    body = replace_placeholders(body, &[
        ("@RANDOM_CLASS@", random_class.to_string()),
        ("@RANDOM_SYSTEM_RESULT@", system_result.to_string()),
        ("@RANDOM_SYSTEM_OK_TAG@", ok_tag.to_string()),
        ("@RANDOM_INT_RESULT@", int_result.to_string()),
        ("@RANDOM_INT_OK_TAG@", variant_id(program, int_result, "Ok").to_string()),
    ]);
    let body = replace_method_ids(body, program, None);
    debug_assert!(!super::support::has_placeholder(&body), "unexpanded Random placeholder");
    body
}
