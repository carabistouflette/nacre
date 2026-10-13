use crate::ir::{self as t, NativeClass as C, NativeMethod as M, Type};
pub(super) fn marker(method: M) -> &'static str {
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
pub(super) fn native_function_if(program: &t::Program, is: impl Fn(M) -> bool) -> Option<&t::Function> {
    program.functions.iter().find(|function| {
        function.native.is_some_and(&is)
    })
}
pub(super) fn native_function(program: &t::Program, is: impl Fn(M) -> bool) -> &t::Function {
    native_function_if(program, is).expect("compiler-owned native function is present")
}
pub(super) fn result_id(function: &t::Function) -> usize {
    match function.return_type {
        Type::Object(id) => id,
        _ => unreachable!("native result is a concrete sum object"),
    }
}
pub(super) fn variant<'a>(program: &'a t::Program, class: usize, name: &str) -> (usize, &'a t::Variant) {
    program.classes[class].variants.as_ref().expect("sum class variants")
        .iter().enumerate().find(|(_, variant)| variant.name == name)
        .expect("compiler-owned native enum variant")
}
pub(super) fn variant_id(program: &t::Program, class: usize, name: &str) -> usize {
    variant(program, class, name).0
}
pub(super) fn text_class(program: &t::Program) -> Option<usize> {
    program.classes.iter().find(|class| matches!(class.native, Some(C::Text))).map(|class| class.id)
}
pub(super) fn text_helpers(program: &t::Program, text: usize) -> String {
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
pub(super) fn replace_method_ids(
    mut body: String,
    program: &t::Program,
    receiver: Option<usize>,
) -> String {
    for function in &program.functions {
        if receiver.is_some_and(|class| function.receiver != Some(class)) {
            continue;
        }
        if let Some(method) = function.native {
            body = body.replace(marker(method), &format!("ncr_fn_{}", function.id));
        }
    }
    body
}

pub(super) fn replace_placeholders(mut body: String, substitutions: &[(&str, String)]) -> String {
    for (placeholder, replacement) in substitutions {
        body = body.replace(*placeholder, replacement.as_str());
    }
    body
}
