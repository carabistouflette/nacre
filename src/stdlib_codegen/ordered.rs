use crate::c_repr::ctype;
use crate::ir::{self as t, NativeClass as C, NativeMethod as M, Type};
use super::{
    iterators, ordering,
    support::{drop_element, marker, option_tags, replace_method_ids, replace_placeholders, zero},
};

fn native_method(program: &t::Program, receiver: usize, method: M) -> Option<&t::Function> {
    program.functions.iter().find(|function| {
        function.receiver == Some(receiver) && function.native == Some(method)
    })
}

fn required_method(program: &t::Program, receiver: usize, method: M) -> &t::Function {
    native_method(program, receiver, method).expect("compiler-owned ordered collection method is present")
}

const COMPARATOR_FIELD: usize = 0;

pub(super) fn comparator_field(program: &t::Program, class_id: usize) -> Type {
    let Type::Object(interface) = *program.classes[class_id].fields.get(COMPARATOR_FIELD)
        .expect("ordered collection stores its Comparator<K> interface") else {
            unreachable!("ordered collection comparator field is an owning interface")
        };
    assert!(program.classes[interface].is_interface, "Comparator<K> is an interface value");
    Type::Object(interface)
}

pub(super) fn comparator_field_name() -> String {
    format!("ncr_f_{COMPARATOR_FIELD}")
}

fn comparator_info(program: &t::Program, class_id: usize, method: M) -> (usize, usize, Type) {
    ordering::comparator_info(program, required_method(program, class_id, method))
}

fn ordered_map_for_set(program: &t::Program, key: Type) -> &t::Class {
    program.classes.iter().find(|class| {
        matches!(class.native, Some(C::OrderedMap(candidate, Type::Unit)) if candidate == key)
    }).expect("OrderedSet specialization owns its shared OrderedMap<K, Unit> core")
}
fn value_at_mut_method(program: &t::Program, class_id: usize, value: Type) -> String {
    let Some(function) = native_method(program, class_id, M::OrderedMapValueAtMut) else {
        return String::new();
    };
    let index_type = function.params.first().expect("OrderedMap.valueAtMut index").1;
    assert_eq!(index_type, Type::Int, "OrderedMap.valueAtMut index is int32");
    let stored = ordering::borrow_storage(program, value, "p->table.items[index].value");
    format!(
        "static {} {}(ncr_obj_{class_id} *p, int32_t index, size_t line, size_t column) {{\n    if (index < 0 || index >= p->table.length) {{ ncr_fail(\"OrderedMap index out of bounds\", line, column); }}\n    return {};\n}}\n",
        ctype(program, function.return_type), marker(M::OrderedMapValueAtMut), stored,
    )
}

fn ordered_map_drain(
    program: &t::Program,
    class_id: usize,
    key: Type,
    value: Type,
    method: M,
    drain_keys: bool,
) -> String {
    let Some(drain) = native_method(program, class_id, method) else { return String::new(); };
    let element = if drain_keys { key } else { value };
    let (next_name, drop_name) = iterators::callback_names(drain.id);
    let entry = format!("struct ncr_ordered_entry_{class_id}");
    let (field, unused_field, unused) = if drain_keys {
        ("key", "value", value)
    } else {
        ("value", "key", key)
    };
    let next_body = format!(
        "    {entry} *entries = raw;\n    if (*cursor >= length) {{ *available = false; return; }}\n    int32_t index = (*cursor)++;\n    *value = entries[index].{field};\n    entries[index].{field} = {};\n    *available = true;",
        zero(program, element),
    );
    let drop_body = format!(
        "    {entry} *entries = raw;\n    for (int32_t index = cursor; index < length; ++index) {{\n        {}\n    }}\n    ncr_deallocate(entries);",
        drop_element(element, &format!("entries[index].{field}")),
    );
    // Erased entry halves must not carry thread-affine owners into a sendable iterator.
    let discard_unused = if matches!(unused, Type::Object(_)) {
        let slot = format!("p->table.items[index].{unused_field}");
        format!(
            "for (int32_t index = 0; index < p->table.length; ++index) {{\n    {}\n    {slot} = {};\n}}\n",
            drop_element(unused, &slot), zero(program, unused),
        )
    } else {
        String::new()
    };
    let callbacks = iterators::drain_callbacks(program, drain.id, element, &next_body, &drop_body);
    let setup = format!(
        "{discard_unused}iterator->state = p->table.items;\niterator->length = p->table.length;\niterator->ncr_next_fn = {next_name};\niterator->ncr_drop_fn = {drop_name};\np->table.items = NULL;\np->table.length = 0;\np->table.capacity = 0;"
    );
    format!("{callbacks}{}", iterators::drain_function(program, class_id, method, element, &setup))
}

fn ordered_set_key_at(program: &t::Program, class_id: usize, key: Type) -> String {
    let Some(function) = native_method(program, class_id, M::OrderedSetKeyAt) else { return String::new(); };
    let index_type = function.params.first().expect("OrderedSet.keyAt index").1;
    assert_eq!(index_type, Type::Int, "OrderedSet.keyAt index is int32");
    let stored = ordering::borrow_storage(program, key, "p->table.items[index].key");
    format!(
        "static {} {}(ncr_obj_{class_id} *p, int32_t index, size_t line, size_t column) {{\n    if (index < 0 || index >= p->table.length) {{ ncr_fail(\"OrderedSet index out of bounds\", line, column); }}\n    return {};\n}}\n",
        ctype(program, function.return_type), marker(M::OrderedSetKeyAt), stored,
    )
}

fn ordered_set_drain(program: &t::Program, class_id: usize, key: Type) -> String {
    let Some(drain) = native_method(program, class_id, M::OrderedSetDrain) else { return String::new(); };
    let map = ordered_map_for_set(program, key);
    let (next_name, drop_name) = iterators::callback_names(drain.id);
    let entry = format!("struct ncr_ordered_entry_{}", map.id);
    let next_body = format!(
        "    {entry} *entries = raw;\n    if (*cursor >= length) {{ *available = false; return; }}\n    int32_t index = (*cursor)++;\n    *value = entries[index].key;\n    entries[index].key = {};\n    entries[index].value = UINT8_C(0);\n    *available = true;",
        zero(program, key),
    );
    let drop_body = format!(
        "    {entry} *entries = raw;\n    for (int32_t index = cursor; index < length; ++index) {{\n        {}\n    }}\n    ncr_deallocate(entries);",
        drop_element(key, "entries[index].key"),
    );
    let callbacks = iterators::drain_callbacks(program, drain.id, key, &next_body, &drop_body);
    let setup = format!(
        "iterator->state = p->table.items;\niterator->length = p->table.length;\niterator->ncr_next_fn = {next_name};\niterator->ncr_drop_fn = {drop_name};\np->table.items = NULL;\np->table.length = 0;\np->table.capacity = 0;"
    );
    format!("{callbacks}{}", iterators::drain_function(program, class_id, M::OrderedSetDrain, key, &setup))
}


pub(super) fn storage_definition(program: &t::Program, class_id: usize) -> String {
    let comparator = ctype(program, comparator_field(program, class_id));
    format!(
        "struct ncr_ordered_storage_{class_id} {{\n    struct ncr_ordered_entry_{class_id} *items;\n    int32_t length;\n    int32_t capacity;\n    const {comparator} *comparator;\n}};\n"
    )
}

pub(super) fn entry_definition(program: &t::Program, class_id: usize, key: Type, value: Type) -> String {
    format!(
        "struct ncr_ordered_entry_{class_id} {{\n    {} key;\n    {} value;\n}};\n",
        ctype(program, key), ctype(program, value),
    )
}

pub(super) fn map_fields(class_id: usize) -> String {
    format!("    struct ncr_ordered_storage_{class_id} table;\n")
}

pub(super) fn set_fields(program: &t::Program, key: Type) -> String {
    let map = ordered_map_for_set(program, key);
    format!("    struct ncr_ordered_storage_{} table;\n", map.id)
}

pub(super) fn fields(program: &t::Program, class: C, class_id: usize) -> String {
    match class {
        C::OrderedMap(_, _) => map_fields(class_id),
        C::OrderedSet(key) => set_fields(program, key),
        _ => unreachable!("ordered codegen fields called for another native class"),
    }
}

pub(super) fn drop_fields(class: C) -> String {
    match class {
        C::OrderedMap(key, value) => ordered_storage_drop(key, value),
        C::OrderedSet(key) => ordered_storage_drop(key, Type::Unit),
        _ => unreachable!("ordered codegen drops called for another native class"),
    }
}

fn ordered_storage_drop(key: Type, value: Type) -> String {
    let items = "p->table.items[index]";
    format!(
        "    while (p->table.length > 0) {{\n        int32_t index = --p->table.length;\n        {}\n        {}\n    }}\n    ncr_deallocate(p->table.items);\n",
        drop_element(key, &format!("{items}.key")),
        drop_element(value, &format!("{items}.value")),
    )
}

fn try_get_method(program: &t::Program, class_id: usize, value: Type) -> String {
    let Some(function) = native_method(program, class_id, M::OrderedMapTryGet) else {
        return String::new();
    };
    assert!(value.is_copy(), "OrderedMap.tryGet is specialized for copied values");
    let query = function.params.first().expect("OrderedMap.tryGet key query").1;
    let (none_tag, some_tag) = option_tags(program, function, value);
    format!(
        "static {} {}(ncr_obj_{} *p, {} key, size_t line, size_t column) {{\n    (void)line; (void)column;\n    int32_t index = ncr_ordered_lookup_{}(&p->table, key);\n    if (index < 0) {{ return ({}){{ .tag = {} }}; }}\n    return ({}){{ .tag = {}, .data.v{}.f0 = p->table.items[index].value }};\n}}\n",
        ctype(program, function.return_type), marker(M::OrderedMapTryGet), class_id,
        ctype(program, query), class_id, ctype(program, function.return_type), none_tag,
        ctype(program, function.return_type), some_tag, some_tag,
    )
}

fn borrowed_getters(program: &t::Program, class_id: usize, value: Type) -> String {
    let mut output = String::new();
    for method in [M::OrderedMapGetRef, M::OrderedMapGetMut] {
        let Some(function) = native_method(program, class_id, method) else { continue; };
        let Type::Borrow { class: value_class, .. } = function.return_type else {
            unreachable!("OrderedMap borrowed getters return borrowed values");
        };
        assert_eq!(value_class, value.object_class().expect("affine value object class"));
        let query = function.params.first().expect("OrderedMap getter query").1;
        let stored = ordering::borrow_storage(program, value, "p->table.items[index].value");
        let name = marker(method);
        output.push_str(&format!(
            "static {} {}(ncr_obj_{} *p, {} key, size_t line, size_t column) {{\n    int32_t index = ncr_ordered_lookup_{}(&p->table, key);\n    if (index < 0) {{ ncr_fail(\"OrderedMap key not found\", line, column); }}\n    return {};\n}}\n",
            ctype(program, function.return_type), name, class_id, ctype(program, query), class_id, stored,
        ));
    }
    output
}

pub(super) fn map_implementation(program: &t::Program, class_id: usize, key: Type, value: Type) -> String {
    let constructor = required_method(program, class_id, M::OrderedMapNew);
    let (comparator, compare_slot, compare_type) = ordering::comparator_info(program, constructor);
    assert_eq!(comparator_field(program, class_id), Type::Object(comparator));
    let contains = required_method(program, class_id, M::OrderedMapContains);
    let query_type = contains.params.first().expect("OrderedMap key query").1;
    assert_eq!(query_type, compare_type, "borrowed key query matches Comparator<K> representation");
    let put = required_method(program, class_id, M::OrderedMapPut);
    let (none_tag, some_tag) = option_tags(program, put, value);
    let floor = required_method(program, class_id, M::OrderedMapFloorIndex);
    let (index_none_tag, index_some_tag) = option_tags(program, floor, Type::Int);
    let try_get = try_get_method(program, class_id, value);
    let borrowed = borrowed_getters(program, class_id, value);
    let entry_middle = ordering::borrow_storage(program, key, "table->items[middle].key");
    let entry_lower = ordering::borrow_storage(program, key, "table->items[low].key");
    let key_query = ordering::borrow_storage(program, key, "key");
    let drop_key_field = drop_element(key, "table->items[index].key");
    let drop_value_field = drop_element(value, "table->items[index].value");
    let drop_key_at = drop_element(key, "p->table.items[index].key");
    let zero_key = zero(program, key);
    let zero_value = zero(program, value);
    let key_at = required_method(program, class_id, M::OrderedMapKeyAt);
    let value_at = required_method(program, class_id, M::OrderedMapValueAt);
    let key_at_expr = ordering::borrow_storage(program, key, "p->table.items[index].key");
    let value_at_expr = ordering::borrow_storage(program, value, "p->table.items[index].value");
    let query_type_c = ctype(program, query_type);
    let value_type_c = ctype(program, value);
    let mut body = replace_placeholders(include_str!("../../stdlib/ordered_map.c.in").to_owned(), &[
        ("@COMPARE_TYPE@", ctype(program, compare_type)),
        ("@COMPARE_SLOT@", compare_slot.to_string()),
        ("@COMPARE_STORED_KEY@", entry_middle),
        ("@COMPARE_LOWER_KEY@", entry_lower),
        ("@KEY_TYPE@", ctype(program, key)),
        ("@VALUE_TYPE@", value_type_c),
        ("@DROP_KEY_FIELD@", drop_key_field),
        ("@DROP_VALUE_FIELD@", drop_value_field),
        ("@ZERO_KEY@", zero_key),
        ("@ZERO_VALUE@", zero_value),
        ("@KEY_QUERY_EXPR@", key_query),
        ("@COMPARATOR_TYPE@", ctype(program, Type::Object(comparator))),
        ("@COMPARATOR_FIELD@", comparator_field_name()),
        ("@OPTION_TYPE@", ctype(program, put.return_type)),
        ("@NONE_TAG@", none_tag.to_string()),
        ("@SOME_TAG@", some_tag.to_string()),
        ("@DROP_DUPLICATE_KEY@", drop_element(key, "key")),
        ("@DROP_KEY_AT_INDEX@", drop_key_at),
        ("@QUERY_TYPE@", query_type_c),
        ("@TRY_GET_METHOD@", try_get),
        ("@BORROWED_GETTERS@", borrowed),
        ("@KEY_AT_TYPE@", ctype(program, key_at.return_type)),
        ("@KEY_AT_EXPR@", key_at_expr),
        ("@VALUE_AT_TYPE@", ctype(program, value_at.return_type)),
        ("@VALUE_AT_EXPR@", value_at_expr),
        ("@VALUE_AT_MUT_METHOD@", value_at_mut_method(program, class_id, value)),
        ("@OPTION_INDEX_TYPE@", ctype(program, floor.return_type)),
        ("@INDEX_NONE_TAG@", index_none_tag.to_string()),
        ("@INDEX_SOME_TAG@", index_some_tag.to_string()),
    ]);
    body = replace_method_ids(body, program, Some(class_id));
    body.push_str(&ordered_map_drain(program, class_id, key, value, M::OrderedMapDrainKeys, true));
    body.push_str(&ordered_map_drain(program, class_id, key, value, M::OrderedMapDrainValues, false));
    body
}

pub(super) fn set_implementation(program: &t::Program, class_id: usize, key: Type) -> String {
    let map = ordered_map_for_set(program, key);
    let (_, _, compare_type) = comparator_info(program, class_id, M::OrderedSetNew);
    let contains = required_method(program, class_id, M::OrderedSetContains);
    let query = contains.params.first().expect("OrderedSet key query").1;
    assert_eq!(query, compare_type, "borrowed key query matches Comparator<T> representation");
    let key_query = ordering::borrow_storage(program, key, "key");
    let mut body = replace_placeholders(include_str!("../../stdlib/ordered_set.c.in").to_owned(), &[
        ("@ID@", class_id.to_string()),
        ("@MAP_ID@", map.id.to_string()),
        ("@COMPARATOR_TYPE@", ctype(program, comparator_field(program, class_id))),
        ("@COMPARATOR_FIELD@", comparator_field_name()),
        ("@KEY_TYPE@", ctype(program, key)),
        ("@QUERY_TYPE@", ctype(program, query)),
        ("@KEY_QUERY_EXPR@", key_query),
        ("@DROP_DUPLICATE_KEY@", drop_element(key, "key")),
        ("@ORDERED_SET_KEY_AT@", ordered_set_key_at(program, class_id, key)),
    ]);
    body = replace_method_ids(body, program, Some(class_id));
    debug_assert!(!body.contains('@'), "unexpanded OrderedSet template placeholder");
    body.push_str(&ordered_set_drain(program, class_id, key));
    body
}
