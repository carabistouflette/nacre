use crate::ir::{self as t, NativeClass as C, NativeMethod as M, Type};
use super::support::{
    native_function, replace_method_ids, replace_placeholders, result_id, text_class, variant,
    variant_id,
};

pub(super) fn fields(program: &t::Program, class: C) -> String {
    match class {
        C::ZipReader => "    void *archive;\n    void (*release)(void *);\n    unsigned char *memory;\n    bool entry_active;\n    bool entry_read;\n    bool eof;\n".to_owned(),
        C::ZipWriter => "    void *archive;\n    void (*release)(void *);\n    bool failed;\n".to_owned(),
        C::ZipEntry => {
            let text = text_class(program).expect("ZipEntry requires Text");
            format!("    ncr_obj_{text} *name;\n    int64_t size;\n    bool is_directory;\n")
        }
        _ => unreachable!("ZIP fields requested for another native class"),
    }
}

pub(super) fn drop_fields(program: &t::Program, class: C) -> String {
    match class {
        C::ZipReader => "    if (p->archive != NULL && p->release != NULL) { p->release(p->archive); }\n    p->archive = NULL;\n    p->release = NULL;\n    free(p->memory);\n    p->memory = NULL;\n".to_owned(),
        C::ZipWriter => "    if (p->archive != NULL && p->release != NULL) { p->release(p->archive); }\n    p->archive = NULL;\n    p->release = NULL;\n".to_owned(),
        C::ZipEntry => {
            let text = text_class(program).expect("ZipEntry requires Text");
            format!("    ncr_drop_{text}(p->name);\n")
        }
        _ => unreachable!("ZIP drop requested for another native class"),
    }
}

pub(super) fn entry_implementation(program: &t::Program) -> String {
    let entry = class_id(program, |native| matches!(native, C::ZipEntry));
    let text = text_class(program).expect("ZipEntry requires Text");
    let body = replace_placeholders(include_str!("../../stdlib/zip_entry.c.in").to_owned(), &[
        ("@ZIP_ENTRY_ID@", entry.to_string()),
        ("@TEXT_ID@", text.to_string()),
    ]);
    let body = replace_method_ids(body, program, None);
    debug_assert!(!body.contains('@'), "unexpanded ZIP entry placeholder");
    body
}

const ARCHIVE_METHODS: &[M] = &[
    M::ZipReaderOpen,
    M::ZipReaderFromBytes,
    M::ZipReaderNext,
    M::ZipReaderReadEntry,
    M::ZipReaderSkipEntry,
    M::ZipReaderClose,
    M::ZipWriterCreate,
    M::ZipWriterAdd,
    M::ZipWriterAddDirectory,
    M::ZipWriterClose,
];

pub(super) fn implementation(program: &t::Program) -> Option<String> {
    if !uses_archive_methods(program) {
        return None;
    }
    let reader = class_id(program, |native| matches!(native, C::ZipReader));
    let writer = class_id(program, |native| matches!(native, C::ZipWriter));
    let entry = class_id(program, |native| matches!(native, C::ZipEntry));
    let path = class_id(program, |native| matches!(native, C::Path));
    let text = text_class(program).expect("ZIP APIs require Text");
    let buffer = class_id(program, |native| matches!(native, C::ByteBuffer));

    let next = native_function(program, |native| native == M::ZipReaderNext);
    let next_result = result_id(next);
    let (_, next_ok) = variant(program, next_result, "Ok");
    let [Type::Object(option)] = next_ok.fields.as_slice() else {
        unreachable!("ZipReader.next returns Result<Option<ZipEntry>, Text>");
    };
    let option = *option;
    let (_, some) = variant(program, option, "Some");
    assert!(matches!(some.fields.as_slice(), [Type::Object(class)] if *class == entry));

    let mut replacements = vec![
        ("@ZIP_READER_ID@", reader.to_string()),
        ("@ZIP_WRITER_ID@", writer.to_string()),
        ("@ZIP_ENTRY_ID@", entry.to_string()),
        ("@PATH_ID@", path.to_string()),
        ("@TEXT_ID@", text.to_string()),
        ("@BUFFER_ID@", buffer.to_string()),
        ("@TEXT_FROM_BYTES@", format!("ncr_text_from_bytes_{text}")),
        ("@OPTION_ID@", option.to_string()),
        ("@OPTION_NONE_TAG@", variant_id(program, option, "None").to_string()),
        ("@OPTION_SOME_TAG@", variant_id(program, option, "Some").to_string()),
    ];

    for (placeholder, method) in [
        ("@OPEN_RESULT_ID@", M::ZipReaderOpen),
        ("@FROM_BYTES_RESULT_ID@", M::ZipReaderFromBytes),
        ("@NEXT_RESULT_ID@", M::ZipReaderNext),
        ("@READ_RESULT_ID@", M::ZipReaderReadEntry),
        ("@SKIP_RESULT_ID@", M::ZipReaderSkipEntry),
        ("@READER_CLOSE_RESULT_ID@", M::ZipReaderClose),
        ("@CREATE_RESULT_ID@", M::ZipWriterCreate),
        ("@ADD_RESULT_ID@", M::ZipWriterAdd),
        ("@ADD_DIRECTORY_RESULT_ID@", M::ZipWriterAddDirectory),
        ("@WRITER_CLOSE_RESULT_ID@", M::ZipWriterClose),
    ] {
        replacements.push((placeholder, result_id(native_function(program, |native| native == method)).to_string()));
    }
    for (placeholder, method) in [
        ("@OPEN_OK_TAG@", M::ZipReaderOpen),
        ("@FROM_BYTES_OK_TAG@", M::ZipReaderFromBytes),
        ("@NEXT_OK_TAG@", M::ZipReaderNext),
        ("@READ_OK_TAG@", M::ZipReaderReadEntry),
        ("@SKIP_OK_TAG@", M::ZipReaderSkipEntry),
        ("@READER_CLOSE_OK_TAG@", M::ZipReaderClose),
        ("@CREATE_OK_TAG@", M::ZipWriterCreate),
        ("@ADD_OK_TAG@", M::ZipWriterAdd),
        ("@ADD_DIRECTORY_OK_TAG@", M::ZipWriterAddDirectory),
        ("@WRITER_CLOSE_OK_TAG@", M::ZipWriterClose),
    ] {
        let result = result_id(native_function(program, |native| native == method));
        replacements.push((placeholder, variant_id(program, result, "Ok").to_string()));
    }
    for (placeholder, method, index) in [
        ("@OPEN_PATH_ID@", M::ZipReaderOpen, 0),
        ("@FROM_BYTES_BUFFER_ID@", M::ZipReaderFromBytes, 0),
        ("@READ_MAX_BYTES_ID@", M::ZipReaderReadEntry, 0),
        ("@CREATE_PATH_ID@", M::ZipWriterCreate, 0),
        ("@CREATE_LEVEL_ID@", M::ZipWriterCreate, 1),
        ("@ADD_NAME_ID@", M::ZipWriterAdd, 0),
        ("@ADD_BYTES_ID@", M::ZipWriterAdd, 1),
        ("@ADD_DIRECTORY_NAME_ID@", M::ZipWriterAddDirectory, 0),
    ] {
        let function = native_function(program, |native| native == method);
        replacements.push((placeholder, function.params[index].0.to_string()));
    }

    let body = replace_placeholders(include_str!("../../stdlib/zip.c.in").to_owned(), &replacements);
    let body = replace_method_ids(body, program, None);
    debug_assert!(!body.contains('@'), "unexpanded ZIP placeholder");
    Some(body)
}

fn class_id(program: &t::Program, is: impl Fn(C) -> bool) -> usize {
    program.classes.iter().find(|class| class.native.is_some_and(&is))
        .expect("ZIP signature specializes a required native class").id
}

fn uses_archive_methods(program: &t::Program) -> bool {
    program.functions.iter().filter(|function| function.native.is_none())
        .any(|function| block_uses_archive_methods(&function.body, program))
}

fn block_uses_archive_methods(block: &t::Block, program: &t::Program) -> bool {
    block.iter().any(|statement| match statement {
        t::Stmt::Let { value, .. } => value.as_ref().is_some_and(|value| expr_uses_archive_methods(value, program)),
        t::Stmt::Assign { value, .. } | t::Stmt::Expr(value) => expr_uses_archive_methods(value, program),
        t::Stmt::If { condition, then_block, else_block } => {
            expr_uses_archive_methods(condition, program)
                || block_uses_archive_methods(then_block, program)
                || else_block.as_ref().is_some_and(|block| block_uses_archive_methods(block, program))
        }
        t::Stmt::While { condition, body } => {
            expr_uses_archive_methods(condition, program) || block_uses_archive_methods(body, program)
        }
        t::Stmt::ForEach { collection, body, .. } => {
            expr_uses_archive_methods(collection, program) || block_uses_archive_methods(body, program)
        }
        t::Stmt::Match { value, arms } => {
            expr_uses_archive_methods(value, program)
                || arms.iter().any(|arm| block_uses_archive_methods(&arm.body, program))
        }
        t::Stmt::Return(value) => value.as_ref().is_some_and(|value| expr_uses_archive_methods(value, program)),
        t::Stmt::Break | t::Stmt::Continue => false,
        t::Stmt::Block(block) => block_uses_archive_methods(block, program),
    })
}

fn expr_uses_archive_methods(expression: &t::Expr, program: &t::Program) -> bool {
    match &expression.kind {
        t::ExprKind::Call { target: t::CallTarget::Function(id), args, .. } => {
            program.functions[*id].native.is_some_and(|method| ARCHIVE_METHODS.contains(&method))
                || args.iter().any(|argument| expr_uses_archive_methods(argument, program))
        }
        t::ExprKind::Call { target: t::CallTarget::Println, args, .. }
        | t::ExprKind::New { args, .. }
        | t::ExprKind::Variant { args, .. } => {
            args.iter().any(|argument| expr_uses_archive_methods(argument, program))
        }
        t::ExprKind::MapSlot { index, .. } => expr_uses_archive_methods(index, program),
        t::ExprKind::Drop(value)
        | t::ExprKind::Unary { expr: value, .. }
        | t::ExprKind::Try { value, .. }
        | t::ExprKind::Cast { value, .. }
        | t::ExprKind::Upcast { value, .. } => expr_uses_archive_methods(value, program),
        t::ExprKind::Binary { left, right, .. } => {
            expr_uses_archive_methods(left, program) || expr_uses_archive_methods(right, program)
        }
        t::ExprKind::Char(_)
        | t::ExprKind::Int(_)
        | t::ExprKind::Long(_)
        | t::ExprKind::Double(_)
        | t::ExprKind::Unit
        | t::ExprKind::Boolean(_)
        | t::ExprKind::String(_)
        | t::ExprKind::Read(_)
        | t::ExprKind::Move(_)
        | t::ExprKind::Borrow { .. }
        | t::ExprKind::MapCapacity(_) => false,
    }
}
