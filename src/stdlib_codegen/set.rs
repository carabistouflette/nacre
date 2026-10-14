use crate::c_repr::ctype;
use crate::ir::{self as t, NativeClass as C, NativeMethod as M, Type};
use super::{collections, iterators, support::{drop_element, result_id, variant_id, zero}};

fn map_for_set(program: &t::Program, key: Type, hasher: Option<Type>) -> &t::Class {
    program.classes.iter().find(|class| {
        matches!(
            class.native,
            Some(C::Map { key: candidate, value: Type::Unit, hasher: policy })
                if candidate == key && policy == hasher
        )
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

fn initializer_methods(
    program: &t::Program, class_id: usize, table_id: usize, hasher: Option<Type>,
) -> String {
    let set_type = ctype(program, Type::Object(class_id));
    let mut methods = String::new();
    if native_method(program, class_id, |method| matches!(method, M::SetNew)).is_some() {
        methods.push_str(&format!(
            "static void @SET_NEW@({set_type} p, size_t line, size_t column) {{\n    (void)line;\n    (void)column;\n    ncr_map_init_{table_id}(&p->table);\n}}\n"
        ));
    }
    if let Some(policy) = hasher {
        let factory_name = format!("{}.withHasher", program.classes[class_id].name);
        assert!(program.functions.iter().any(|function| {
            function.name == factory_name && function.native == Some(M::SetWithHasher)
        }), "custom Set specialization must retain Set.withHasher");
        methods.push_str(&format!(
            "static {set_type} @SET_WITH_HASHER@({} hasher, size_t line, size_t column) {{\n    {set_type} p = ncr_allocate(sizeof(*p), line, column);\n    ncr_map_init_{table_id}(&p->table);\n    p->table.hasher = hasher;\n    return p;\n}}\n",
            ctype(program, policy),
        ));
    }
    methods
}

pub(super) fn fields(program: &t::Program, key: Type, hasher: Option<Type>) -> String {
    let map_class = map_for_set(program, key, hasher);
    format!("    struct ncr_hash_table_{} table;\n", map_class.id)
}

pub(super) fn drop_fields(key: Type, hasher: Option<Type>) -> String {
    collections::drop_hash_table_fields(key, Type::Unit, hasher, "p->table")
}

pub(super) fn implementation(
    program: &t::Program, class_id: usize, key: Type, hasher: Option<Type>,
) -> String {
    let map_class = map_for_set(program, key, hasher);
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
    let initializer_methods = initializer_methods(program, class_id, map_class.id, hasher);
    let mut body = include_str!("../../stdlib/set.c.in")
        .replace("@TABLE_ID@", &map_class.id.to_string())
        .replace("@KEY_TYPE@", &ctype(program, add.params[0].1))
        .replace("@QUERY_TYPE@", &ctype(program, contains.params[0].1))
        .replace("@OPTION_TYPE@", &ctype(program, option_type))
        .replace("@SOME_TAG@", &some_tag.to_string())
        .replace("@SET_INITIALIZER_METHODS@", &initializer_methods);
    if let Some(drain) = native_method(program, class_id, |method| matches!(method, M::CollectionDrain)) {
        let (next_name, drop_name) = iterators::callback_names(drain.id);
        let entry = format!("struct ncr_map_entry_{}", map_class.id);
        let next_body = format!(
            "    {entry} *entries = raw;\n    while (*cursor < length) {{\n        int32_t index = (*cursor)++;\n        if (!entries[index].occupied) {{ continue; }}\n        *value = entries[index].key;\n        entries[index].key = {};\n        entries[index].value = UINT8_C(0);\n        entries[index].occupied = false;\n        *available = true;\n        return;\n    }}\n    *available = false;",
            zero(program, key),
        );
        let drop_body = format!(
            "    {entry} *entries = raw;\n    for (int32_t index = cursor; index < length; ++index) {{\n        if (entries[index].occupied) {{\n            {}\n        }}\n    }}\n    ncr_deallocate(entries);",
            drop_element(key, "entries[index].key"),
        );
        body.push_str(&iterators::drain_callbacks(program, drain.id, key, &next_body, &drop_body));
        body.push_str(&iterators::drain_function(
            program,
            class_id,
            M::CollectionDrain,
            key,
            &format!(
                "iterator->state = p->table.entries;\niterator->length = p->table.capacity;\niterator->ncr_next_fn = {next_name};\niterator->ncr_drop_fn = {drop_name};\np->table.entries = NULL;\np->table.length = 0;\np->table.capacity = 0;"
            ),
        ));
    }
    body
}
