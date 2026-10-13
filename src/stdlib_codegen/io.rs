use crate::ir::{self as t, NativeMethod as M, Type};
use super::support::{
    native_function, native_function_if, replace_method_ids, replace_placeholders, result_id,
    text_class, variant, variant_id,
};
pub(super) fn fields() -> String {
    "    unsigned char ncr_static_class;\n".to_owned()
}

pub(super) fn drop_fields() -> String {
    String::new()
}

fn parameter(function: &t::Function, index: usize) -> String {
    format!("ncr_v_{}", function.params[index].0)
}
pub(super) fn implementations(program: &t::Program) -> Option<String> {
    let has_io = program.functions.iter().any(|function| matches!(function.native,
        Some(M::ConsoleReadLine | M::ConsolePrint | M::ConsolePrintText
            | M::FilesReadText | M::FilesWriteText | M::FilesWriteTextAtomic
            | M::ArgsCount | M::ArgsGet)));
    if !has_io { return None; }
    let text = text_class(program).expect("native IO APIs require concrete Text specialization");
    let console = native_function_if(program, |method| matches!(method, M::ConsoleReadLine));
    let files = native_function_if(program, |method| matches!(method, M::FilesReadText));
    let files_write_atomic = native_function_if(program, |method| matches!(method, M::FilesWriteTextAtomic));
    let args = native_function_if(program, |method| matches!(method, M::ArgsCount));
    if console.is_none() && files.is_none() && files_write_atomic.is_none() && args.is_none() {
        return Some(String::new());
    }
    let mut out = String::new();
    if console.is_some() || files.is_some() {
        out.push_str(&include_str!("../../stdlib/io_buffer.c.in").replace("@IO_RESERVE@", "ncr_io_reserve"));
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
        let template = include_str!("../../stdlib/console.c.in");
        let body = replace_placeholders(template.to_owned(), &[
            ("@TEXT_ID@", text.to_string()),
            ("@TEXT_TAKE@", format!("ncr_text_take_bytes_{text}")),
            ("@IO_RESERVE@", "ncr_io_reserve".to_owned()),
            ("@RESULT_READLINE@", result.to_string()),
            ("@READLINE_OK_TAG@", ok.to_string()),
            ("@ERROR_READLINE@", format!("ncr_result_error_{result}")),
            ("@OPTION_TEXT@", option.to_string()),
            ("@OPTION_NONE_TAG@", variant_id(program, option, "None").to_string()),
            ("@OPTION_SOME_TAG@", variant_id(program, option, "Some").to_string()),
        ]);
        let body = replace_method_ids(body, program, None);
        debug_assert!(!body.contains('@'), "unexpanded Console placeholder");
        out.push_str(&body);
        out.push('\n');
    }
    if let Some(files_read) = files {
        let files_write = native_function(program, |method| matches!(method, M::FilesWriteText));
        let text_result = result_id(files_read);
        let bool_result = result_id(files_write);
        let template = include_str!("../../stdlib/files.c.in");
        let body = replace_placeholders(template.to_owned(), &[
            ("@TEXT_ID@", text.to_string()),
            ("@TEXT_TAKE@", format!("ncr_text_take_bytes_{text}")),
            ("@IO_RESERVE@", "ncr_io_reserve".to_owned()),
            ("@RESULT_TEXT@", text_result.to_string()),
            ("@TEXT_RESULT_OK_TAG@", variant_id(program, text_result, "Ok").to_string()),
            ("@ERROR_TEXT_RESULT@", format!("ncr_result_error_{text_result}")),
            ("@RESULT_BOOL@", bool_result.to_string()),
            ("@BOOL_RESULT_OK_TAG@", variant_id(program, bool_result, "Ok").to_string()),
            ("@ERROR_BOOL_RESULT@", format!("ncr_result_error_{bool_result}")),
            ("@FILE_READ_PATH@", parameter(files_read, 0)),
            ("@FILE_WRITE_PATH@", parameter(files_write, 0)),
            ("@FILE_WRITE_CONTENT@", parameter(files_write, 1)),
        ]);
        let body = replace_method_ids(body, program, None);
        debug_assert!(!body.contains('@'), "unexpanded Files placeholder");
        out.push_str(&body);
        out.push('\n');
        if let Some(files_write_atomic) = files_write_atomic {
            let result = result_id(files_write_atomic);
            let template = include_str!("../../stdlib/files_atomic.c.in");
            let body = replace_placeholders(template.to_owned(), &[
                ("@TEXT_ID@", text.to_string()),
                ("@RESULT_BOOL_ATOMIC@", result.to_string()),
                ("@BOOL_ATOMIC_OK_TAG@", variant_id(program, result, "Ok").to_string()),
                ("@ERROR_BOOL_ATOMIC@", format!("ncr_result_error_{result}")),
            ]);
            let body = replace_method_ids(body, program, None);
            debug_assert!(!body.contains('@'), "unexpanded atomic Files placeholder");
            out.push_str(&body);
            out.push('\n');
        }
    }
    if args.is_some() {
        let args_get = native_function(program, |method| matches!(method, M::ArgsGet));
        let text_result = result_id(args_get);
        let template = include_str!("../../stdlib/args.c.in");
        let body = replace_placeholders(template.to_owned(), &[
            ("@TEXT_ID@", text.to_string()),
            ("@TEXT_MAKE@", format!("ncr_text_from_bytes_{text}")),
            ("@RESULT_TEXT@", text_result.to_string()),
            ("@TEXT_RESULT_OK_TAG@", variant_id(program, text_result, "Ok").to_string()),
            ("@ERROR_TEXT_RESULT@", format!("ncr_result_error_{text_result}")),
            ("@ARGS_GET_INDEX@", parameter(args_get, 0)),
        ]);
        let body = replace_method_ids(body, program, None);
        debug_assert!(!body.contains('@'), "unexpanded Args placeholder");
        out.push_str(&body);
    }
    Some(out)
}
