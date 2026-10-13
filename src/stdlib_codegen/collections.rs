use crate::codegen::ctype;
use crate::ir::{self as t, NativeClass as C, NativeMethod as M, Type};
use super::{iterators, support::{marker, result_id, variant}};
pub(super) fn list_fields(program: &t::Program, element: Type) -> String {
    format!("    {} *items;\n    int32_t length;\n    int32_t capacity;\n", ctype(program, element))
}

pub(super) fn map_fields(class_id: usize) -> String {
    format!("    struct ncr_hash_table_{class_id} table;\n")
}
pub(super) fn hash_table_definition(program: &t::Program, class_id: usize, hasher: Option<Type>) -> String {
    let hasher_field = hasher.map(|ty| format!("    {} hasher;\n", ctype(program, ty))).unwrap_or_default();
    format!(
        "struct ncr_hash_table_{class_id} {{\n    struct ncr_map_entry_{class_id} *entries;\n    int32_t length;\n    int32_t capacity;\n{hasher_field}}};\n",
    )
}
const MAP_HASH_FINALIZER: &str = concat!(
    "    value ^= value >> 30;\n",
    "    value *= UINT64_C(0xbf58476d1ce4e5b9);\n",
    "    value ^= value >> 27;\n",
    "    value *= UINT64_C(0x94d049bb133111eb);\n",
    "    value ^= value >> 31;\n",
    "    return value;\n",
);
fn is_text_key(program: &t::Program, key: Type) -> bool {
    matches!(key, Type::Object(id) if matches!(program.classes[id].native, Some(C::Text)))
}
fn drop_map_key(key: Type, expression: &str) -> String {
    drop_element(key, expression)
}
pub(super) fn map_entry_definition(program: &t::Program, class_id: usize, key: Type, value: Type) -> String {
    format!(
        "struct ncr_map_entry_{class_id} {{\n    {} key;\n    uint64_t hash;\n    {} value;\n    bool occupied;\n}};\n",
        ctype(program, key), ctype(program, value),
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
    format!("{map}->table.capacity")
}
pub(super) fn map_slot_expression(
    program: &t::Program, map_class: usize, map: &str, index: &str,
    kind: t::MapSlotKind, value_type: Type,
) -> String {
    let stored = match (program.classes[map_class].native, kind) {
        (Some(t::NativeClass::Map { .. } | t::NativeClass::Set { .. }), t::MapSlotKind::Occupied) =>
            format!("{map}->table.entries[{index}].occupied"),
        (Some(t::NativeClass::Map { .. } | t::NativeClass::Set { .. }), t::MapSlotKind::Key) =>
            format!("{map}->table.entries[{index}].key"),
        (Some(t::NativeClass::Map { .. }), t::MapSlotKind::Value) =>
            format!("{map}->table.entries[{index}].value"),
        (Some(t::NativeClass::OrderedMap(..)), t::MapSlotKind::Key) =>
            format!("{map}->table.items[{index}].key"),
        (Some(t::NativeClass::OrderedMap(..)), t::MapSlotKind::Value) =>
            format!("{map}->table.items[{index}].value"),
        (Some(t::NativeClass::OrderedSet(_)), t::MapSlotKind::Key) =>
            format!("{map}->table.items[{index}].key"),
        (Some(t::NativeClass::PriorityQueue(_)), t::MapSlotKind::Value) =>
            format!("{map}->items[{index}]"),
        _ => unreachable!("slot kind matches its native collection storage"),
    };
    if value_type.is_borrow() {
        map_borrowed_value(program, value_type, &stored)
    } else {
        stored
    }
}
pub(super) fn drop_element(ty: Type, expression: &str) -> String {
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

pub(super) fn drop_hash_table_fields(key: Type, value: Type, hasher: Option<Type>, table: &str) -> String {
    let entries = format!("{table}.entries");
    let drop_policy = match hasher {
        Some(Type::Object(id)) => format!("ncr_drop_{id}({table}.hasher);"),
        Some(_) => unreachable!("Hasher policy must be an owned interface value"),
        None => String::new(),
    };
    format!(
        "    for (int32_t index = 0; index < {table}.capacity; ++index) {{\n        if ({entries}[index].occupied) {{\n            {}\n            {}\n        }}\n    }}\n    ncr_deallocate({entries});\n    {drop_policy}\n",
        drop_map_key(key, &format!("{entries}[index].key")),
        drop_element(value, &format!("{entries}[index].value")))
}

pub(super) fn drop_map_fields(key: Type, value: Type, hasher: Option<Type>) -> String {
    drop_hash_table_fields(key, value, hasher, "p->table")
}
pub(super) fn zero(program: &t::Program, ty: Type) -> String {
    match ty {
        Type::Byte | Type::Unit => "UINT8_C(0)".to_owned(),
        Type::Char => "UINT32_C(0)".to_owned(),
        Type::Long => "INT64_C(0)".to_owned(),
        Type::Double => "0.0".to_owned(),
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
    assert!(element.is_copy(),
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
fn map_query_view(program: &t::Program, key: Type, query_type: Type, expression: &str) -> String {
    if let Type::Borrow { class, .. } = query_type {
        if key == Type::Object(class) && program.classes[class].variants.is_some() {
            return format!("&({expression})");
        }
    }
    expression.to_owned()
}

fn hasher_slots(program: &t::Program, hasher: Type) -> (usize, usize) {
    let Type::Object(interface) = hasher else {
        unreachable!("Hasher policy must be an interface object");
    };
    assert!(program.classes[interface].is_interface, "Hasher policy must be an interface");
    let slot = |suffix: &str| {
        program.classes[interface].interface_methods.iter().position(|id| {
            program.functions[*id].name.ends_with(suffix)
        }).expect("Hasher interface method is present")
    };
    (slot(".hash"), slot(".equals"))
}

fn content_hash_body(map_class: usize, bytes: &str, length: &str) -> String {
    format!(
        "    (void)table;\n    const unsigned char *bytes = {bytes};\n    size_t length = {length};\n    uint64_t value = UINT64_C(14695981039346656037);\n    for (size_t index = 0; index < length; ++index) {{\n        value ^= (uint64_t)bytes[index];\n        value *= UINT64_C(1099511628211);\n    }}\n    value ^= (uint64_t)length;\n    value *= UINT64_C(1099511628211);\n    return ncr_map_finish_hash_{map_class}(value);\n"
    )
}

fn map_hash_body(program: &t::Program, map_class: usize, key: Type, hasher: Option<Type>) -> String {
    if let Some(hasher) = hasher {
        let (hash_slot, _) = hasher_slots(program, hasher);
        return format!(
            "    uint64_t value = (uint64_t)table->hasher.vtable->m{hash_slot}(table->hasher.data, key);\n    return ncr_map_finish_hash_{map_class}(value);\n"
        );
    }
    match key {
        Type::Byte | Type::Char | Type::Int | Type::Long | Type::Boolean => format!(
            "    (void)table;\n    uint64_t value = (uint64_t)key;\n    return ncr_map_finish_hash_{map_class}(value);\n"
        ),
        Type::Unit => format!(
            "    (void)table;\n    (void)key;\n    uint64_t value = UINT64_C(0);\n    return ncr_map_finish_hash_{map_class}(value);\n"
        ),
        Type::Double => format!(
            "    (void)table;\n    uint64_t value;\n    if (isnan(key)) {{ value = UINT64_C(0x7ff8000000000000); }}\n    else if (key == 0.0) {{ value = UINT64_C(0); }}\n    else {{ memcpy(&value, &key, sizeof(value)); }}\n    return ncr_map_finish_hash_{map_class}(value);\n"
        ),
        Type::String => content_hash_body(map_class, "key.data", "key.length"),
        Type::Object(_) if is_text_key(program, key) =>
            content_hash_body(map_class, "key->bytes", "(size_t)key->length"),
        _ => unreachable!("checker only instantiates builtin hashing for supported key types"),
    }
}

fn map_key_equal(
    program: &t::Program, key: Type, hasher: Option<Type>, query_type: Type,
) -> String {
    if let Some(hasher) = hasher {
        let (_, equals_slot) = hasher_slots(program, hasher);
        let stored = map_query_view(program, key, query_type, "entry->key");
        return format!(
            "table->hasher.vtable->m{equals_slot}(table->hasher.data, {stored}, key)"
        );
    }
    if is_text_key(program, key) {
        return "entry->key->length == key->length && (key->length == 0 || memcmp(entry->key->bytes, key->bytes, (size_t)key->length) == 0)".to_owned();
    }
    match key {
        Type::String => "entry->key.length == key.length && (key.length == 0 || memcmp(entry->key.data, key.data, key.length) == 0)".to_owned(),
        Type::Double => "entry->key == key || (isnan(entry->key) && isnan(key))".to_owned(),
        _ => "entry->key == key".to_owned(),
    }
}

fn map_hash_helpers(map_class: usize) -> String {
    format!(
        "static uint64_t ncr_map_finish_hash_{map_class}(uint64_t value) {{\n{MAP_HASH_FINALIZER}}}\nstatic uint32_t ncr_map_bucket_{map_class}(uint64_t hash) {{\n    return (uint32_t)(hash ^ (hash >> 32));\n}}\n"
    )
}

fn map_table_initializers(map_class: usize) -> String {
    format!(
        "static void ncr_map_init_{map_class}(struct ncr_hash_table_{map_class} *table) {{\n    table->entries = NULL;\n    table->length = 0;\n    table->capacity = 0;\n}}\n"
    )
}

fn map_initializer_methods(program: &t::Program, map_class: usize, hasher: Option<Type>) -> String {
    let map_type = ctype(program, Type::Object(map_class));
    let mut methods = String::new();
    if map_native_function(program, map_class, |method| matches!(method, M::MapNew)).is_some() {
        methods.push_str(&format!(
            "static void @MAP_NEW@({map_type} p, size_t line, size_t column) {{\n    (void)line;\n    (void)column;\n    ncr_map_init_{map_class}(&p->table);\n}}\n"
        ));
    }
    if let Some(policy) = hasher {
        let factory_name = format!("{}.withHasher", program.classes[map_class].name);
        assert!(program.functions.iter().any(|function| {
            function.name == factory_name && function.native == Some(M::MapWithHasher)
        }), "custom Map specialization must retain Map.withHasher");
        methods.push_str(&format!(
            "static {map_type} @MAP_WITH_HASHER@({} hasher, size_t line, size_t column) {{\n    {map_type} p = ncr_allocate(sizeof(*p), line, column);\n    ncr_map_init_{map_class}(&p->table);\n    p->table.hasher = hasher;\n    return p;\n}}\n",
            ctype(program, policy),
        ));
    }
    methods
}


fn borrowed_map_method(program: &t::Program, function: &t::Function, value_class: usize) -> String {
    let Type::Borrow { class, .. } = function.return_type else {
        unreachable!("native Map borrowed accessor must return a typed borrow")
    };
    assert_eq!(class, value_class, "Map borrow result must match its value type");
    let borrowed_value = map_borrowed_value(program, function.return_type, "p->table.entries[index].value");
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
    assert!(value.is_copy(), "Map tryGet must be pruned for non-copy values");
    let (none_tag, some_tag) = option_tags(program, function, value);
    include_str!("../../stdlib/map_try_get.c.in")
        .replace("@OPTION_TYPE@", &ctype(program, function.return_type))
        .replace("@QUERY_TYPE@", &ctype(program, function.params.first().expect("Map.tryGet key parameter").1))
        .replace("@NONE_TAG@", &none_tag.to_string())
        .replace("@SOME_TAG@", &some_tag.to_string())
        .replace("@ELEMENT@", &ctype(program, value))
}

fn map_drain_implementation(
    program: &t::Program,
    map_class: usize,
    key: Type,
    value: Type,
    method: M,
    drain_keys: bool,
) -> String {
    let Some(drain) = map_native_function(program, map_class, |candidate|
        if drain_keys { matches!(candidate, M::MapDrainKeys) } else { matches!(candidate, M::MapDrainValues) })
    else { return String::new(); };
    assert_eq!(drain.native, Some(method), "map drain method dispatch matches its source signature");
    let element = if drain_keys { key } else { value };
    let (next_name, drop_name) = iterators::callback_names(drain.id);
    let entry = format!("struct ncr_map_entry_{map_class}");
    let (field, unused_field, unused) = if drain_keys {
        ("key", "value", value)
    } else {
        ("value", "key", key)
    };
    let next_body = format!(
        "    {entry} *entries = raw;\n    while (*cursor < length) {{\n        int32_t index = (*cursor)++;\n        if (!entries[index].occupied) {{ continue; }}\n        *value = entries[index].{field};\n        entries[index].{field} = {};\n        entries[index].occupied = false;\n        *available = true;\n        return;\n    }}\n    *available = false;",
        zero(program, element),
    );
    let drop_body = format!(
        "    {entry} *entries = raw;\n    for (int32_t index = cursor; index < length; ++index) {{\n        if (entries[index].occupied) {{\n            {}\n        }}\n    }}\n    ncr_deallocate(entries);",
        drop_element(element, &format!("entries[index].{field}")),
    );
    // The iterator owns only its yielded type; discard hidden owners on the calling thread.
    let discard_unused = if matches!(unused, Type::Object(_)) {
        let slot = format!("p->table.entries[index].{unused_field}");
        format!(
            "for (int32_t index = 0; index < p->table.capacity; ++index) {{\n    if (p->table.entries[index].occupied) {{\n        {}\n        {slot} = {};\n    }}\n}}\n",
            drop_element(unused, &slot), zero(program, unused),
        )
    } else {
        String::new()
    };
    let callbacks = iterators::drain_callbacks(program, drain.id, element, &next_body, &drop_body);
    let setup = format!(
        "{discard_unused}iterator->state = p->table.entries;\niterator->length = p->table.capacity;\niterator->ncr_next_fn = {next_name};\niterator->ncr_drop_fn = {drop_name};\np->table.entries = NULL;\np->table.length = 0;\np->table.capacity = 0;"
    );
    format!("{callbacks}{}", iterators::drain_function(program, map_class, method, element, &setup))
}

pub(super) fn map_implementation(
    program: &t::Program, map_class: usize, key: Type, value: Type, hasher: Option<Type>,
) -> String {
    let put = map_native_function(program, map_class, |method| matches!(method, M::MapPut))
        .expect("compiler-owned Map.put method is present");
    assert_eq!(put.params.first().expect("Map.put key parameter").1, key,
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
    let hash_helpers = map_hash_helpers(map_class);
    let table_initializers = map_table_initializers(map_class);
    let initializer_methods = map_initializer_methods(program, map_class, hasher);
    let hash_body = map_hash_body(program, map_class, key, hasher);
    let key_equal = map_key_equal(program, key, hasher, query_type);
    let key_query = map_query_view(program, key, query_type, "key");
    let mut body = include_str!("../../stdlib/map.c.in")
        .replace("@ENTRY_TYPE@", &format!("struct ncr_map_entry_{map_class}"))
        .replace("@KEY_TYPE@", &ctype(program, key))
        .replace("@QUERY_TYPE@", &ctype(program, query_type))
        .replace("@HASH_FINALIZER@", &hash_helpers)
        .replace("@TABLE_INITIALIZERS@", &table_initializers)
        .replace("@MAP_INITIALIZER_METHODS@", &initializer_methods)
        .replace("@HASH_BODY@", &hash_body)
        .replace("@KEY_QUERY@", &key_query)
        .replace("@KEY_EQUAL@", &key_equal)
        .replace("@STORE_HASH@", "table->entries[target].hash = hash;")
        .replace("@DROP_DUPLICATE_KEY@", &drop_map_key(key, "key"))
        .replace("@DROP_KEY_AT_INDEX@", &drop_map_key(key, "table->entries[index].key"))
        .replace("@ELEMENT@", &ctype(program, value))
        .replace("@DROP_VALUE@", &drop_element(value, "table->entries[index].value"))
        .replace("@OPTION_TYPE@", &ctype(program, put.return_type))
        .replace("@NONE_TAG@", &none_tag.to_string())
        .replace("@SOME_TAG@", &some_tag.to_string())
        .replace("@TRY_GET_METHOD@", &try_get)
        .replace("@BORROWED_GETTERS@", &borrowed);
    body.push_str(&map_drain_implementation(program, map_class, key, value, M::MapDrainKeys, true));
    body.push_str(&map_drain_implementation(program, map_class, key, value, M::MapDrainValues, false));
    body
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
