use crate::ast::MapKeyKind;
use crate::codegen::ctype;
use crate::ir::{self as t, NativeMethod as M, Type};
use super::support::{marker, result_id, text_class, variant};
pub(super) fn list_fields(program: &t::Program, element: Type) -> String {
    format!("    {} *items;\n    int32_t length;\n    int32_t capacity;\n", ctype(program, element))
}

pub(super) fn map_fields(class_id: usize) -> String {
    format!("    struct ncr_map_entry_{class_id} *entries;\n    int32_t length;\n    int32_t capacity;\n")
}
const MAP_HASH_FINALIZER: &str = concat!(
    "    value ^= value >> 16;\n",
    "    value *= UINT32_C(0x7feb352d);\n",
    "    value ^= value >> 15;\n",
    "    value *= UINT32_C(0x846ca68b);\n",
    "    value ^= value >> 16;\n",
    "    return value;\n",
);
fn map_key_type(program: &t::Program, key: MapKeyKind) -> Type {
    match key {
        MapKeyKind::Int => Type::Int,
        MapKeyKind::Text => Type::Object(text_class(program).expect("native Text class accompanies Map<Text, V>")),
    }
}
fn drop_map_key(program: &t::Program, key: MapKeyKind, expression: &str) -> String {
    match key {
        MapKeyKind::Int => String::new(),
        MapKeyKind::Text => format!(
            "ncr_drop_{}({expression});",
            text_class(program).expect("native Text class accompanies Map<Text, V>"),
        ),
    }
}
pub(super) fn map_entry_definition(program: &t::Program, class_id: usize, key: MapKeyKind, value: Type) -> String {
    let hash = if key == MapKeyKind::Text { "    uint32_t hash;\n" } else { "" };
    format!(
        "struct ncr_map_entry_{class_id} {{\n    {} key;\n{}    {} value;\n    bool occupied;\n}};\n",
        ctype(program, map_key_type(program, key)), hash, ctype(program, value),
    )
}
fn map_borrowed_value(program: &t::Program, borrow_type: Type, stored: &str) -> String {
    let Type::Borrow { class, .. } = borrow_type else {
        unreachable!("Map value traversal requires a borrowed value type");
    };
    // Inline enums are stored by value; other representations are already borrowed views.
    if program.classes[class].variants.is_some() { format!("&({stored})") }
    else { stored.to_owned() }
}
pub(super) fn map_capacity_expression(map: &str) -> String {
    format!("{map}->capacity")
}
pub(super) fn map_slot_expression(
    program: &t::Program, map: &str, index: &str, kind: t::MapSlotKind, value_type: Type,
) -> String {
    let entry = format!("{map}->entries[{index}]");
    match kind {
        t::MapSlotKind::Occupied => format!("{entry}.occupied"),
        t::MapSlotKind::Key => format!("{entry}.key"),
        t::MapSlotKind::Value if value_type.is_borrow() =>
            map_borrowed_value(program, value_type, &format!("{entry}.value")),
        t::MapSlotKind::Value => format!("{entry}.value"),
    }
}
fn drop_element(ty: Type, expression: &str) -> String {
    match ty {
        Type::Object(id) => format!("ncr_drop_{id}({expression});"),
        _ => "/* Copy value: no destructor. */".to_owned(),
    }
}
pub(super) fn drop_list_fields(element: Type) -> String {
    format!(
        "    while (p->length > 0) {{\n        --p->length;\n        {}\n    }}\n    ncr_deallocate(p->items);\n",
        drop_element(element, "p->items[p->length]"))
}

pub(super) fn drop_map_fields(program: &t::Program, key: MapKeyKind, value: Type) -> String {
    format!(
        "    for (int32_t index = 0; index < p->capacity; ++index) {{\n        if (p->entries[index].occupied) {{\n            {}\n            {}\n        }}\n    }}\n    ncr_deallocate(p->entries);\n",
        drop_map_key(program, key, "p->entries[index].key"),
        drop_element(value, "p->entries[index].value"))
}
fn zero(program: &t::Program, ty: Type) -> String {
    match ty {
        Type::Int => "INT32_C(0)".to_owned(),
        Type::Boolean => "false".to_owned(),
        Type::String => "(ncr_string){NULL, 0}".to_owned(),
        Type::Object(id) => crate::codegen::object_zero(program, id),
        _ => unreachable!("checked native collection value"),
    }
}
fn borrowed_list_method(program: &t::Program, function: &t::Function, element_class: usize) -> String {
    let Type::Borrow { class, .. } = function.return_type else {
        unreachable!("native List borrowed accessor must return a typed borrow")
    };
    assert_eq!(class, element_class, "List borrow result must match its element type");
    let index_type = function.params.first().expect("List accessor index parameter").1;
    let borrowed_element = if program.classes[element_class].is_interface {
        "p->items[index]"
    } else if program.classes[element_class].variants.is_some() {
        "&p->items[index]"
    } else {
        "p->items[index]"
    };
    let method = function.native.expect("native List accessor");
    include_str!("../../stdlib/list_get_borrowed.c.in")
        .replace("@RETURN_TYPE@", &ctype(program, function.return_type))
        .replace("@INDEX_TYPE@", &ctype(program, index_type))
        .replace("@LIST_METHOD@", marker(method))
        .replace("@BORROWED_ELEMENT@", borrowed_element)
}

fn borrowed_list_methods(program: &t::Program, list_class: usize, element: Type) -> String {
    let Type::Object(element_class) = element else { return String::new(); };
    let mut methods = String::new();
    for function in program.functions.iter().filter(|function| {
        function.receiver == Some(list_class)
            && matches!(function.native, Some(M::ListGetRef | M::ListGetMut))
    }) {
        methods.push_str(&borrowed_list_method(program, function, element_class));
    }
    methods
}
fn list_native_function(
    program: &t::Program,
    list_class: usize,
    is: impl Fn(M) -> bool,
) -> Option<&t::Function> {
    program.functions.iter().find(|function| {
        function.receiver == Some(list_class)
            && function.native.is_some_and(&is)
    })
}

fn option_tags(program: &t::Program, function: &t::Function, element: Type) -> (usize, usize) {
    let option = result_id(function);
    let (none_tag, none) = variant(program, option, "None");
    let (some_tag, some) = variant(program, option, "Some");
    assert!(none.fields.is_empty(), "Option None must have no payload");
    assert!(matches!(some.fields.as_slice(), [field] if *field == element),
        "Option Some payload must match its collection value type");
    (none_tag, some_tag)
}

fn try_get_list_method(program: &t::Program, list_class: usize, element: Type) -> String {
    let Some(function) = list_native_function(program, list_class, |method| matches!(method, M::ListTryGet))
    else { return String::new(); };
    assert!(matches!(element, Type::Int | Type::Boolean | Type::String),
        "List tryGet must be pruned for non-copy elements");
    let (none_tag, some_tag) = option_tags(program, function, element);
    let index_type = function.params.first().expect("List tryGet index parameter").1;
    include_str!("../../stdlib/list_try_get.c.in")
        .replace("@OPTION_TYPE@", &ctype(program, function.return_type))
        .replace("@INDEX_TYPE@", &ctype(program, index_type))
        .replace("@NONE_TAG@", &none_tag.to_string())
        .replace("@SOME_TAG@", &some_tag.to_string())
        .replace("@ELEMENT@", &ctype(program, element))
}

fn try_owned_list_methods(program: &t::Program, list_class: usize, element: Type) -> String {
    let try_pop = list_native_function(program, list_class, |method| matches!(method, M::ListTryPop))
        .expect("compiler-owned List.tryPop method is present");
    let try_remove = list_native_function(program, list_class, |method| matches!(method, M::ListTryRemove))
        .expect("compiler-owned List.tryRemove method is present");
    assert_eq!(try_pop.return_type, try_remove.return_type,
        "List.tryPop and tryRemove must return the same Option specialization");
    let pop = list_native_function(program, list_class, |method| matches!(method, M::ListPop))
        .expect("compiler-owned List.pop method is present");
    let remove = list_native_function(program, list_class, |method| matches!(method, M::ListRemove))
        .expect("compiler-owned List.remove method is present");
    let (none_tag, some_tag) = option_tags(program, try_pop, element);
    let index_type = try_remove.params.first().expect("List.tryRemove index parameter").1;
    include_str!("../../stdlib/list_try_owned.c.in")
        .replace("@OPTION_TYPE@", &ctype(program, try_pop.return_type))
        .replace("@INDEX_TYPE@", &ctype(program, index_type))
        .replace("@NONE_TAG@", &none_tag.to_string())
        .replace("@SOME_TAG@", &some_tag.to_string())
        .replace("@ELEMENT@", &ctype(program, element))
        .replace("@STRICT_POP_CALL@", &format!("ncr_fn_{}", pop.id))
        .replace("@STRICT_REMOVE_CALL@", &format!("ncr_fn_{}", remove.id))
}
fn map_native_function(
    program: &t::Program,
    map_class: usize,
    is: impl Fn(M) -> bool,
) -> Option<&t::Function> {
    program.functions.iter().find(|function| {
        function.receiver == Some(map_class) && function.native.is_some_and(&is)
    })
}

fn borrowed_map_method(program: &t::Program, function: &t::Function, value_class: usize) -> String {
    let Type::Borrow { class, .. } = function.return_type else {
        unreachable!("native Map borrowed accessor must return a typed borrow")
    };
    assert_eq!(class, value_class, "Map borrow result must match its value type");
    let borrowed_value = map_borrowed_value(program, function.return_type, "p->entries[index].value");
    include_str!("../../stdlib/map_get_borrowed.c.in")
        .replace("@RETURN_TYPE@", &ctype(program, function.return_type))
        .replace("@MAP_METHOD@", marker(function.native.expect("native Map accessor")))
        .replace("@QUERY_TYPE@", &ctype(program, function.params.first().expect("Map accessor key parameter").1))
        .replace("@BORROWED_VALUE@", &borrowed_value)
}

fn borrowed_map_methods(program: &t::Program, map_class: usize, value: Type) -> String {
    let Type::Object(value_class) = value else { return String::new(); };
    let mut methods = String::new();
    for function in program.functions.iter().filter(|function| {
        function.receiver == Some(map_class)
            && matches!(function.native, Some(M::MapGetRef | M::MapGetMut))
    }) {
        methods.push_str(&borrowed_map_method(program, function, value_class));
    }
    methods
}

fn try_get_map_method(program: &t::Program, map_class: usize, value: Type) -> String {
    let Some(function) = map_native_function(program, map_class, |method| matches!(method, M::MapTryGet))
    else { return String::new(); };
    assert!(matches!(value, Type::Int | Type::Boolean | Type::String),
        "Map tryGet must be pruned for non-copy values");
    let (none_tag, some_tag) = option_tags(program, function, value);
    include_str!("../../stdlib/map_try_get.c.in")
        .replace("@OPTION_TYPE@", &ctype(program, function.return_type))
        .replace("@QUERY_TYPE@", &ctype(program, function.params.first().expect("Map.tryGet key parameter").1))
        .replace("@NONE_TAG@", &none_tag.to_string())
        .replace("@SOME_TAG@", &some_tag.to_string())
        .replace("@ELEMENT@", &ctype(program, value))
}

pub(super) fn map_implementation(program: &t::Program, map_class: usize, key: MapKeyKind, value: Type) -> String {
    let key_type = map_key_type(program, key);
    let put = map_native_function(program, map_class, |method| matches!(method, M::MapPut))
        .expect("compiler-owned Map.put method is present");
    assert_eq!(put.params.first().expect("Map.put key parameter").1, key_type,
        "Map.put must take an owned key");
    let contains = map_native_function(program, map_class, |method| matches!(method, M::MapContainsKey))
        .expect("compiler-owned Map.containsKey method is present");
    let query_type = contains.params.first().expect("Map.containsKey key parameter").1;
    let remove = map_native_function(program, map_class, |method| matches!(method, M::MapTryRemove))
        .expect("compiler-owned Map.tryRemove method is present");
    assert_eq!(remove.params.first().expect("Map.tryRemove key parameter").1, query_type,
        "Map query methods must share the specialized key ABI");
    assert_eq!(put.return_type, remove.return_type,
        "Map.put and tryRemove must return the same Option specialization");
    let (none_tag, some_tag) = option_tags(program, put, value);
    let borrowed = borrowed_map_methods(program, map_class, value);
    let try_get = try_get_map_method(program, map_class, value);
    let (hash_query_prefix, key_equal, old_hash, scan_hash, store_hash) = match key {
        MapKeyKind::Int => (
            "    uint32_t value = (uint32_t)key;\n",
            "entry->key == key",
            "ncr_map_hash_@ID@(old->key)",
            "ncr_map_hash_@ID@(entry->key)",
            "",
        ),
        MapKeyKind::Text => (
            concat!(
                "    uint32_t value = UINT32_C(2166136261);\n",
                "    for (int32_t index = 0; index < key->length; ++index) {\n",
                "        value ^= (uint32_t)key->bytes[index];\n",
                "        value *= UINT32_C(16777619);\n",
                "    }\n",
                "    value ^= (uint32_t)key->length;\n",
            ),
            concat!(
                "entry->hash == hash && ",
                "entry->key->length == key->length && ",
                "(key->length == 0 || memcmp(entry->key->bytes, key->bytes, (size_t)key->length) == 0)",
            ),
            "old->hash",
            "entry->hash",
            "p->entries[target].hash = hash;",
        ),
    };
    include_str!("../../stdlib/map.c.in")
        .replace("@ENTRY_TYPE@", &format!("struct ncr_map_entry_{map_class}"))
        .replace("@KEY_TYPE@", &ctype(program, key_type))
        .replace("@QUERY_TYPE@", &ctype(program, query_type))
        .replace("@HASH_QUERY_PREFIX@", hash_query_prefix)
        .replace("@HASH_FINALIZER@", MAP_HASH_FINALIZER)
        .replace("@KEY_EQUAL@", key_equal)
        .replace("@OLD_HASH@", old_hash)
        .replace("@SCAN_HASH@", scan_hash)
        .replace("@STORE_HASH@", store_hash)
        .replace("@DROP_DUPLICATE_KEY@", &drop_map_key(program, key, "key"))
        .replace("@DROP_KEY_AT_INDEX@", &drop_map_key(program, key, "p->entries[index].key"))
        .replace("@ELEMENT@", &ctype(program, value))
        .replace("@DROP_VALUE@", &drop_element(value, "p->entries[index].value"))
        .replace("@OPTION_TYPE@", &ctype(program, put.return_type))
        .replace("@NONE_TAG@", &none_tag.to_string())
        .replace("@SOME_TAG@", &some_tag.to_string())
        .replace("@TRY_GET_METHOD@", &try_get)
        .replace("@BORROWED_GETTERS@", &borrowed)
}

pub(super) fn list_implementation(program: &t::Program, class_id: usize, element: Type) -> String {
    let getter = if element.is_object() { "" } else { include_str!("../../stdlib/list_get.c.in") };
    let borrowed_getters = borrowed_list_methods(program, class_id, element);
    let try_get = try_get_list_method(program, class_id, element);
    let try_owned = try_owned_list_methods(program, class_id, element);
    include_str!("../../stdlib/list.c.in")
        .replace("@GET_METHOD@", getter)
        .replace("@BORROWED_GETTERS@", &borrowed_getters)
        .replace("@TRY_GET_METHOD@", &try_get)
        .replace("@TRY_OWNED_METHODS@", &try_owned)
        .replace("@ELEMENT@", &ctype(program, element))
        .replace("@ZERO@", &zero(program, element))
        .replace("@DROP_AT_INDEX@", &drop_element(element, "p->items[index]"))
}
