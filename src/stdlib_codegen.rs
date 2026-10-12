//! The native standard-library templates are part of the trusted C backend.
//! All source calls still pass normal type, ownership and borrowing checks.
use crate::codegen::ctype;
use crate::ir::{self as t, NativeClass as C, NativeMethod as M, Type};

pub fn fields(program: &t::Program, class: C) -> String {
    match class {
        C::List(element) => format!("    {} *items;\n    int32_t length;\n    int32_t capacity;\n", ctype(program, element)),
        C::Text => "    unsigned char *bytes;\n    int32_t length;\n    int32_t capacity;\n".to_owned(),
        C::Console | C::Files | C::Args => "    unsigned char ncr_static_class;\n".to_owned(),
    }
}
fn drop_element(ty: Type, expression: &str) -> String {
    match ty {
        Type::Object(id) => format!("ncr_drop_{id}({expression});"),
        _ => "/* Copy value: no destructor. */".to_owned(),
    }
}
pub fn drop_fields(class: C) -> String {
    match class {
        C::List(element) => format!(
            "    while (p->length > 0) {{\n        --p->length;\n        {}\n    }}\n    ncr_deallocate(p->items);\n",
            drop_element(element, "p->items[p->length]")),
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
        _ => unreachable!("checked list element"),
    }
}
fn marker(method: M) -> &'static str {
    match method {
        M::ListNew => "@LIST_NEW@", M::ListAdd => "@LIST_ADD@",
        M::ListSize => "@LIST_SIZE@", M::ListCapacity => "@LIST_CAPACITY@",
        M::ListIsEmpty => "@LIST_IS_EMPTY@", M::ListGet => "@LIST_GET@",
        M::ListSet => "@LIST_SET@", M::ListPop => "@LIST_POP@",
        M::ListRemove => "@LIST_REMOVE@", M::ListClear => "@LIST_CLEAR@",
        M::ListReserve => "@LIST_RESERVE@", M::TextNew => "@TEXT_NEW@",
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

pub fn implementations(program: &t::Program) -> String {
    let text = text_class(program);
    let mut out = String::new();
    if let Some(text) = text { out.push_str(&text_helpers(program, text)); }
    for class in &program.classes {
        let Some(native) = class.native else { continue; };
        let mut body = match native {
            C::List(element) => {
                let getter = if element.is_object() { "" } else { include_str!("../stdlib/list_get.c.in") };
                include_str!("../stdlib/list.c.in")
                    .replace("@GET_METHOD@", getter)
                    .replace("@ELEMENT@", &ctype(program, element))
                    .replace("@ZERO@", &zero(program, element))
                    .replace("@DROP_AT_INDEX@", &drop_element(element, "p->items[index]"))
            }
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

