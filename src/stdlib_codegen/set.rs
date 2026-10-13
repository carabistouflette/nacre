use crate::ast::MapKeyKind;
use crate::codegen::ctype;
use crate::ir::{self as t, NativeClass as C, NativeMethod as M, Type};
use super::{collections, support::{result_id, variant_id}};

fn map_for_set(program: &t::Program, key: MapKeyKind) -> &t::Class {
    program.classes.iter().find(|class| {
        matches!(class.native, Some(C::Map(candidate, Type::Unit)) if candidate == key)
    }).expect("Set specialization requests its shared Map<K, Unit> table")
}

fn native_method(program: &t::Program, receiver: usize, is: impl Fn(M) -> bool) -> Option<&t::Function> {
    program.functions.iter().find(|function| {
        function.receiver == Some(receiver) && function.native.is_some_and(&is)
    })
}

fn required_method(program: &t::Program, receiver: usize, is: impl Fn(M) -> bool) -> &t::Function {
    native_method(program, receiver, is).expect("compiler-owned collection method is present")
}

pub(super) fn fields(program: &t::Program, key: MapKeyKind) -> String {
    let map_class = map_for_set(program, key);
    format!("    struct ncr_hash_table_{} table;\n", map_class.id)
}

pub(super) fn drop_fields(program: &t::Program, key: MapKeyKind) -> String {
    collections::drop_hash_table_fields(program, key, Type::Unit, "p->table")
}

pub(super) fn implementation(program: &t::Program, class_id: usize, key: MapKeyKind) -> String {
    let map_class = map_for_set(program, key);
    let add = required_method(program, class_id, |method| matches!(method, M::SetAdd));
    let contains = required_method(program, class_id, |method| matches!(method, M::SetContains));
    let remove = required_method(program, class_id, |method| matches!(method, M::SetRemove));
    let map_put = required_method(program, map_class.id, |method| matches!(method, M::MapPut));
    let map_remove = required_method(program, map_class.id, |method| matches!(method, M::MapTryRemove));
    let map_contains = required_method(program, map_class.id, |method| matches!(method, M::MapContainsKey));
    assert_eq!(add.params.first().expect("Set.add key").1, map_put.params.first().expect("Map.put key").1);
    assert_eq!(contains.params.first().expect("Set.contains key").1, map_contains.params.first().expect("Map.containsKey key").1);
    assert_eq!(remove.params.first().expect("Set.remove key").1, map_remove.params.first().expect("Map.tryRemove key").1);
    assert_eq!(map_put.params.get(1).expect("Map.put Unit value").1, Type::Unit);
    assert_eq!(map_put.return_type, map_remove.return_type);
    let option_type = map_remove.return_type;
    let option_class = result_id(map_remove);
    let some_tag = variant_id(program, option_class, "Some");
    include_str!("../../stdlib/set.c.in")
        .replace("@TABLE_ID@", &map_class.id.to_string())
        .replace("@KEY_TYPE@", &ctype(program, add.params[0].1))
        .replace("@QUERY_TYPE@", &ctype(program, contains.params[0].1))
        .replace("@OPTION_TYPE@", &ctype(program, option_type))
        .replace("@SOME_TAG@", &some_tag.to_string())
}
