//! The native standard-library templates are part of the trusted C backend.
//! All source calls still pass normal type, ownership and borrowing checks.
use crate::ast::MapKeyKind;
use crate::codegen::ctype;
use crate::ir::{self as t, NativeClass as C, NativeMethod as M, Type};

const MAP_HASH_FINALIZER: &str = concat!(
    "    value ^= value >> 16;\n",
    "    value *= UINT32_C(0x7feb352d);\n",
    "    value ^= value >> 15;\n",
    "    value *= UINT32_C(0x846ca68b);\n",
    "    value ^= value >> 16;\n",
    "    return value;\n",
);

pub fn fields(program: &t::Program, class: C, class_id: usize) -> String {
    match class {
        C::List(element) => format!("    {} *items;\n    int32_t length;\n    int32_t capacity;\n", ctype(program, element)),
        C::Map(_, _) => format!("    struct ncr_map_entry_{class_id} *entries;\n    int32_t length;\n    int32_t capacity;\n"),
        C::Text => "    unsigned char *bytes;\n    int32_t length;\n    int32_t capacity;\n".to_owned(),
        C::Console | C::Files | C::Args => "    unsigned char ncr_static_class;\n".to_owned(),
    }
}
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
pub fn map_entry_definition(program: &t::Program, class_id: usize, key: MapKeyKind, value: Type) -> String {
    let hash = if key == MapKeyKind::Text { "    uint32_t hash;\n" } else { "" };
    format!(
        "struct ncr_map_entry_{class_id} {{\n    {} key;\n{}    {} value;\n    bool occupied;\n}};\n",
        ctype(program, map_key_type(program, key)), hash, ctype(program, value),
    )
}
pub(crate) fn map_borrowed_value(program: &t::Program, borrow_type: Type, stored: &str) -> String {
    let Type::Borrow { class, .. } = borrow_type else {
        unreachable!("Map value traversal requires a borrowed value type");
    };
    // Inline enums are stored by value; other representations are already borrowed views.
    if program.classes[class].variants.is_some() { format!("&({stored})") }
    else { stored.to_owned() }
}
pub(crate) fn map_capacity_expression(map: &str) -> String {
    format!("{map}->capacity")
}
pub(crate) fn map_slot_expression(
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
pub fn drop_fields(program: &t::Program, class: C) -> String {
    match class {
        C::List(element) => format!(
            "    while (p->length > 0) {{\n        --p->length;\n        {}\n    }}\n    ncr_deallocate(p->items);\n",
            drop_element(element, "p->items[p->length]")),
        C::Map(key, value) => format!(
            "    for (int32_t index = 0; index < p->capacity; ++index) {{\n        if (p->entries[index].occupied) {{\n            {}\n            {}\n        }}\n    }}\n    ncr_deallocate(p->entries);\n",
            drop_map_key(program, key, "p->entries[index].key"),
            drop_element(value, "p->entries[index].value")),
        C::Text => "    ncr_deallocate(p->bytes);\n".to_owned(),
        C::Console | C::Files | C::Args => String::new(),
    }
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
fn marker(method: M) -> &'static str {
    match method {
        M::ListNew => "@LIST_NEW@", M::ListAdd => "@LIST_ADD@",
        M::ListSize => "@LIST_SIZE@", M::ListCapacity => "@LIST_CAPACITY@",
        M::ListIsEmpty => "@LIST_IS_EMPTY@", M::ListGet => "@LIST_GET@",
        M::ListGetRef => "@LIST_GET_REF@", M::ListGetMut => "@LIST_GET_MUT@",
        M::ListSet => "@LIST_SET@", M::ListPop => "@LIST_POP@",
        M::ListRemove => "@LIST_REMOVE@", M::ListTryGet => "@LIST_TRY_GET@",
        M::ListTryPop => "@LIST_TRY_POP@", M::ListTryRemove => "@LIST_TRY_REMOVE@",
        M::ListClear => "@LIST_CLEAR@", M::ListReserve => "@LIST_RESERVE@",
        M::MapNew => "@MAP_NEW@", M::MapContainsKey => "@MAP_CONTAINS_KEY@",
        M::MapSize => "@MAP_SIZE@", M::MapIsEmpty => "@MAP_IS_EMPTY@",
        M::MapPut => "@MAP_PUT@", M::MapTryRemove => "@MAP_TRY_REMOVE@",
        M::MapTryGet => "@MAP_TRY_GET@", M::MapGetRef => "@MAP_GET_REF@",
        M::MapGetMut => "@MAP_GET_MUT@", M::MapClear => "@MAP_CLEAR@",
        M::TextNew => "@TEXT_NEW@",
        M::TextAppend => "@TEXT_APPEND@", M::TextAppendInt => "@TEXT_APPEND_INT@",
        M::TextAppendBoolean => "@TEXT_APPEND_BOOLEAN@", M::TextAppendText => "@TEXT_APPEND_TEXT@",
        M::TextByteLength => "@TEXT_BYTE_LENGTH@", M::TextByteAt => "@TEXT_BYTE_AT@",
        M::TextIsEmpty => "@TEXT_IS_EMPTY@", M::TextClear => "@TEXT_CLEAR@",
        M::TextPrint => "@TEXT_PRINT@", M::TextClone => "@TEXT_CLONE@",
        M::TextEquals => "@TEXT_EQUALS@", M::TextEqualsString => "@TEXT_EQUALS_STRING@",
        M::TextParseInt => "@TEXT_PARSE_INT@", M::TextSliceBytes => "@TEXT_SLICE_BYTES@",
        M::ConsoleReadLine => "@CONSOLE_READ_LINE@", M::ConsolePrint => "@CONSOLE_PRINT@",
        M::ConsolePrintText => "@CONSOLE_PRINT_TEXT@", M::FilesReadText => "@FILES_READ_TEXT@",
        M::FilesWriteText => "@FILES_WRITE_TEXT@", M::FilesWriteTextAtomic => "@FILES_WRITE_TEXT_ATOMIC@",
        M::ArgsCount => "@ARGS_COUNT@", M::ArgsGet => "@ARGS_GET@",
    }
}
fn native_function_if(program: &t::Program, is: impl Fn(M) -> bool) -> Option<&t::Function> {
    program.functions.iter().find(|function| {
        function.native.is_some_and(&is)
    })
}
fn native_function(program: &t::Program, is: impl Fn(M) -> bool) -> &t::Function {
    native_function_if(program, is).expect("compiler-owned native function is present")
}
fn result_id(function: &t::Function) -> usize {
    match function.return_type {
        Type::Object(id) => id,
        _ => unreachable!("native result is a concrete sum object"),
    }
}
fn variant<'a>(program: &'a t::Program, class: usize, name: &str) -> (usize, &'a t::Variant) {
    program.classes[class].variants.as_ref().expect("sum class variants")
        .iter().enumerate().find(|(_, variant)| variant.name == name)
        .expect("compiler-owned native enum variant")
}
fn variant_id(program: &t::Program, class: usize, name: &str) -> usize {
    variant(program, class, name).0
}
fn text_class(program: &t::Program) -> Option<usize> {
    program.classes.iter().find(|class| matches!(class.native, Some(C::Text))).map(|class| class.id)
}
fn text_helpers(program: &t::Program, text: usize) -> String {
    let text_new = native_function(program, |method| matches!(method, M::TextNew)).id;
    let mut out = format!(
        "static ncr_obj_{text} *ncr_io_text_from_bytes_{text}(const unsigned char *bytes, size_t length, size_t line, size_t column) {{\n    ncr_obj_{text} *text = ncr_allocate(sizeof(ncr_obj_{text}), line, column);\n    ncr_fn_{text_new}(text, (ncr_string){{bytes, length}}, line, column);\n    return text;\n}}\n"
    );
    out.push_str(&format!(
        "static ncr_obj_{text} *ncr_io_text_take_{text}(unsigned char *bytes, size_t length, size_t capacity, size_t line, size_t column) {{\n    ncr_obj_{text} *text = ncr_allocate(sizeof(ncr_obj_{text}), line, column);\n    text->bytes = bytes;\n    text->length = (int32_t)length;\n    text->capacity = (int32_t)capacity;\n    return text;\n}}\n"
    ));
    let mut result_ids = Vec::new();
    for function in &program.functions {
        if !matches!(function.native,
            Some(M::ConsoleReadLine | M::FilesReadText | M::FilesWriteText | M::FilesWriteTextAtomic
                | M::ArgsGet | M::TextParseInt | M::TextSliceBytes)) {
            continue;
        }
        let class = result_id(function);
        if !result_ids.contains(&class) { result_ids.push(class); }
    }
    for class in result_ids {
        let (err, err_variant) = variant(program, class, "Err");
        assert!(matches!(err_variant.fields.as_slice(), [Type::Object(id)] if *id == text),
            "native Result error payload must own Text");
        out.push_str(&format!(
            "static ncr_obj_{class} ncr_io_error_{class}(const char *message, size_t length, size_t line, size_t column) {{\n    ncr_obj_{text} *error = ncr_io_text_from_bytes_{text}((const unsigned char *)message, length, line, column);\n    return (ncr_obj_{class}){{ .tag = {err}, .data.v{err}.f0 = error }};\n}}\n"
        ));
    }
    out
}
fn parameter(function: &t::Function, index: usize) -> String {
    format!("ncr_v_{}", function.params[index].0)
}
fn text_method_placeholders(mut body: String, program: &t::Program, text: usize) -> String {
    let parse = native_function(program, |method| matches!(method, M::TextParseInt));
    let slice = native_function(program, |method| matches!(method, M::TextSliceBytes));
    let parse_id = result_id(parse);
    let slice_id = result_id(slice);
    let substitutions = [
        ("@PARSE_RESULT@", parse_id.to_string()),
        ("@PARSE_OK_TAG@", variant_id(program, parse_id, "Ok").to_string()),
        ("@ERROR_PARSE_RESULT@", format!("ncr_io_error_{parse_id}")),
        ("@SLICE_RESULT@", slice_id.to_string()),
        ("@SLICE_OK_TAG@", variant_id(program, slice_id, "Ok").to_string()),
        ("@ERROR_SLICE_RESULT@", format!("ncr_io_error_{slice_id}")),
        ("@TEXT_MAKE@", format!("ncr_io_text_from_bytes_{text}")),
    ];
    for (placeholder, replacement) in substitutions {
        body = body.replace(placeholder, &replacement);
    }
    body
}
fn replace_method_ids(mut body: String, program: &t::Program) -> String {
    for function in &program.functions {
        if let Some(method) = function.native {
            body = body.replace(marker(method), &format!("ncr_fn_{}", function.id));
        }
    }
    body
}
fn replace_io_values(mut body: String, substitutions: &[(&str, String)]) -> String {
    for (placeholder, replacement) in substitutions {
        body = body.replace(*placeholder, replacement.as_str());
    }
    body
}
fn io_implementations(program: &t::Program, text: usize) -> String {
    let console = native_function_if(program, |method| matches!(method, M::ConsoleReadLine));
    let files = native_function_if(program, |method| matches!(method, M::FilesReadText));
    let files_write_atomic = native_function_if(program, |method| matches!(method, M::FilesWriteTextAtomic));
    let args = native_function_if(program, |method| matches!(method, M::ArgsCount));
    if console.is_none() && files.is_none() && files_write_atomic.is_none() && args.is_none() { return String::new(); }

    let mut out = String::new();
    if console.is_some() || files.is_some() {
        out.push_str(&include_str!("../stdlib/io_buffer.c.in").replace("@IO_RESERVE@", "ncr_io_reserve"));
        out.push('\n');
    }
    if let Some(read_line) = console {
        let result = result_id(read_line);
        let (ok, ok_variant) = variant(program, result, "Ok");
        let option = match ok_variant.fields.as_slice() {
            [Type::Object(class)] => *class,
            _ => unreachable!("readLine Ok payload is Option<Text>"),
        };
        let (_, some_variant) = variant(program, option, "Some");
        assert!(matches!(some_variant.fields.as_slice(), [Type::Object(id)] if *id == text),
            "Option Some payload must own Text");
        let template = include_str!("../stdlib/console.c.in");
        let body = replace_io_values(template.to_owned(), &[
            ("@TEXT_ID@", text.to_string()),
            ("@TEXT_TAKE@", format!("ncr_io_text_take_{text}")),
            ("@IO_RESERVE@", "ncr_io_reserve".to_owned()),
            ("@RESULT_READLINE@", result.to_string()),
            ("@READLINE_OK_TAG@", ok.to_string()),
            ("@ERROR_READLINE@", format!("ncr_io_error_{result}")),
            ("@OPTION_TEXT@", option.to_string()),
            ("@OPTION_NONE_TAG@", variant_id(program, option, "None").to_string()),
            ("@OPTION_SOME_TAG@", variant_id(program, option, "Some").to_string()),
        ]);
        let body = replace_method_ids(body, program);
        debug_assert!(!body.contains('@'), "unexpanded Console placeholder");
        out.push_str(&body);
        out.push('\n');
    }
    if let Some(files_read) = files {
        let files_write = native_function(program, |method| matches!(method, M::FilesWriteText));
        let text_result = result_id(files_read);
        let bool_result = result_id(files_write);
        let template = include_str!("../stdlib/files.c.in");
        let body = replace_io_values(template.to_owned(), &[
            ("@TEXT_ID@", text.to_string()),
            ("@TEXT_TAKE@", format!("ncr_io_text_take_{text}")),
            ("@IO_RESERVE@", "ncr_io_reserve".to_owned()),
            ("@RESULT_TEXT@", text_result.to_string()),
            ("@TEXT_RESULT_OK_TAG@", variant_id(program, text_result, "Ok").to_string()),
            ("@ERROR_TEXT_RESULT@", format!("ncr_io_error_{text_result}")),
            ("@RESULT_BOOL@", bool_result.to_string()),
            ("@BOOL_RESULT_OK_TAG@", variant_id(program, bool_result, "Ok").to_string()),
            ("@ERROR_BOOL_RESULT@", format!("ncr_io_error_{bool_result}")),
            ("@FILE_READ_PATH@", parameter(files_read, 0)),
            ("@FILE_WRITE_PATH@", parameter(files_write, 0)),
            ("@FILE_WRITE_CONTENT@", parameter(files_write, 1)),
        ]);
        let body = replace_method_ids(body, program);
        debug_assert!(!body.contains('@'), "unexpanded Files placeholder");
        out.push_str(&body);
        out.push('\n');
        if let Some(files_write_atomic) = files_write_atomic {
            let result = result_id(files_write_atomic);
            let template = include_str!("../stdlib/files_atomic.c.in");
            let body = replace_io_values(template.to_owned(), &[
                ("@TEXT_ID@", text.to_string()),
                ("@RESULT_BOOL_ATOMIC@", result.to_string()),
                ("@BOOL_ATOMIC_OK_TAG@", variant_id(program, result, "Ok").to_string()),
                ("@ERROR_BOOL_ATOMIC@", format!("ncr_io_error_{result}")),
            ]);
            let body = replace_method_ids(body, program);
            debug_assert!(!body.contains('@'), "unexpanded atomic Files placeholder");
            out.push_str(&body);
            out.push('\n');
        }
    }
    if args.is_some() {
        let args_get = native_function(program, |method| matches!(method, M::ArgsGet));
        let text_result = result_id(args_get);
        let template = include_str!("../stdlib/args.c.in");
        let body = replace_io_values(template.to_owned(), &[
            ("@TEXT_ID@", text.to_string()),
            ("@TEXT_MAKE@", format!("ncr_io_text_from_bytes_{text}")),
            ("@RESULT_TEXT@", text_result.to_string()),
            ("@TEXT_RESULT_OK_TAG@", variant_id(program, text_result, "Ok").to_string()),
            ("@ERROR_TEXT_RESULT@", format!("ncr_io_error_{text_result}")),
            ("@ARGS_GET_INDEX@", parameter(args_get, 0)),
        ]);
        let body = replace_method_ids(body, program);
        debug_assert!(!body.contains('@'), "unexpanded Args placeholder");
        out.push_str(&body);
    }
    out
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
    include_str!("../stdlib/list_get_borrowed.c.in")
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
    include_str!("../stdlib/list_try_get.c.in")
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
    include_str!("../stdlib/list_try_owned.c.in")
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
    include_str!("../stdlib/map_get_borrowed.c.in")
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
    include_str!("../stdlib/map_try_get.c.in")
        .replace("@OPTION_TYPE@", &ctype(program, function.return_type))
        .replace("@QUERY_TYPE@", &ctype(program, function.params.first().expect("Map.tryGet key parameter").1))
        .replace("@NONE_TAG@", &none_tag.to_string())
        .replace("@SOME_TAG@", &some_tag.to_string())
        .replace("@ELEMENT@", &ctype(program, value))
}

fn map_implementation(program: &t::Program, map_class: usize, key: MapKeyKind, value: Type) -> String {
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
    include_str!("../stdlib/map.c.in")
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

pub fn implementations(program: &t::Program) -> String {
    let text = text_class(program);
    let mut out = String::new();
    if let Some(text) = text { out.push_str(&text_helpers(program, text)); }
    for class in &program.classes {
        let Some(native) = class.native else { continue; };
        let mut body = match native {
            C::List(element) => {
                let getter = if element.is_object() { "" } else { include_str!("../stdlib/list_get.c.in") };
                let borrowed_getters = borrowed_list_methods(program, class.id, element);
                let try_get = try_get_list_method(program, class.id, element);
                let try_owned = try_owned_list_methods(program, class.id, element);
                include_str!("../stdlib/list.c.in")
                    .replace("@GET_METHOD@", getter)
                    .replace("@BORROWED_GETTERS@", &borrowed_getters)
                    .replace("@TRY_GET_METHOD@", &try_get)
                    .replace("@TRY_OWNED_METHODS@", &try_owned)
                    .replace("@ELEMENT@", &ctype(program, element))
                    .replace("@ZERO@", &zero(program, element))
                    .replace("@DROP_AT_INDEX@", &drop_element(element, "p->items[index]"))
            }
            C::Map(key, value) => map_implementation(program, class.id, key, value),
            C::Text => text_method_placeholders(
                include_str!("../stdlib/text.c.in").to_owned(), program, class.id),
            C::Console | C::Files | C::Args => continue,
        };
        body = body.replace("@ID@", &class.id.to_string());
        for function in &program.functions {
            if function.receiver != Some(class.id) { continue; }
            if let Some(method) = function.native {
                body = body.replace(marker(method), &format!("ncr_fn_{}", function.id));
            }
        }
        debug_assert!(!body.contains('@'), "unexpanded standard-library placeholder");
        out.push_str(&body);
        out.push('\n');
    }
    let has_io = program.functions.iter().any(|function| matches!(function.native,
        Some(M::ConsoleReadLine | M::ConsolePrint | M::ConsolePrintText
            | M::FilesReadText | M::FilesWriteText | M::FilesWriteTextAtomic
            | M::ArgsCount | M::ArgsGet)));
    if has_io {
        let text = text.expect("native IO APIs require concrete Text specialization");
        out.push_str(&io_implementations(program, text));
    }
    out
}

