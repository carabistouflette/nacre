use crate::ir::{self as t, NativeClass as C, NativeMethod as M, Type};
use super::support::{
    has_placeholder, native_function, replace_method_ids, replace_placeholders, result_id,
    template_section, text_class, variant, variant_id,
};

pub(super) fn fields(program: &t::Program, class: C) -> String {
    match class {
        C::ZipReader => "    void *archive;\n    void (*release)(void *, void *);\n    void *locale;\n    void *source_file;\n    unsigned char *source_buffer;\n    unsigned char *memory;\n    bool entry_active;\n    bool entry_read;\n    bool eof;\n".to_owned(),
        C::ZipWriter => "    void *archive;\n    void (*release)(void *, void *);\n    void *locale;\n    bool failed;\n".to_owned(),
        C::ZipEntry => {
            let text = text_class(program).expect("ZipEntry requires Text");
            format!("    ncr_obj_{text} *name;\n    int64_t size;\n    bool is_directory;\n")
        }
        _ => unreachable!("ZIP fields requested for another native class"),
    }
}

pub(super) fn drop_fields(program: &t::Program, class: C) -> String {
    match class {
        C::ZipReader => "    if (p->archive != NULL && p->release != NULL) { p->release(p->archive, p->locale); }\n    p->archive = NULL;\n    p->release = NULL;\n    p->locale = NULL;\n    if (p->source_file != NULL) { (void)fclose((FILE *)p->source_file); }\n    p->source_file = NULL;\n    ncr_deallocate(p->source_buffer);\n    p->source_buffer = NULL;\n    free(p->memory);\n    p->memory = NULL;\n".to_owned(),
        C::ZipWriter => "    if (p->archive != NULL && p->release != NULL) { p->release(p->archive, p->locale); }\n    p->archive = NULL;\n    p->release = NULL;\n    p->locale = NULL;\n".to_owned(),
        C::ZipEntry => {
            let text = text_class(program).expect("ZipEntry requires Text");
            format!("    ncr_drop_{text}(p->name);\n")
        }
        _ => unreachable!("ZIP drop requested for another native class"),
    }
}

pub(super) fn entry_implementation(program: &t::Program) -> String {
    let Some(entry) = class_id(program, |native| matches!(native, C::ZipEntry)) else {
        return String::new();
    };
    let text = text_class(program).expect("ZipEntry requires Text");
    let body = replace_placeholders(include_str!("../../stdlib/zip_entry.c.in").to_owned(), &[
        ("@ZIP_ENTRY_ID@", entry.to_string()),
        ("@TEXT_ID@", text.to_string()),
    ]);
    let body = replace_method_ids(body, program, None);
    debug_assert!(!has_placeholder(&body), "unexpanded ZIP entry placeholder");
    body
}

const TEMPLATE: &str = include_str!("../../stdlib/zip.c.in");

pub(super) fn implementation(program: &t::Program) -> Option<String> {
    let reader = class_id(program, |native| matches!(native, C::ZipReader));
    let writer = class_id(program, |native| matches!(native, C::ZipWriter));
    if reader.is_none() && writer.is_none() {
        return None;
    }

    let mut body = template_section(
        TEMPLATE,
        "/* ZIP_GROUP common */",
        "/* ZIP_GROUP_END common */",
    ).to_owned();
    if reader.is_some() {
        body.push_str(&reader_implementation(program));
    }
    if writer.is_some() {
        body.push_str(&writer_implementation(program));
    }
    debug_assert!(!has_placeholder(&body), "unexpanded ZIP placeholder");
    Some(body)
}

fn reader_implementation(program: &t::Program) -> String {
    let reader = class_id(program, |native| matches!(native, C::ZipReader))
        .expect("ZipReader implementation requires ZipReader");
    let entry = class_id(program, |native| matches!(native, C::ZipEntry))
        .expect("ZipReader implementation requires ZipEntry");
    let path = class_id(program, |native| matches!(native, C::Path))
        .expect("ZipReader APIs require Path");
    let text = text_class(program).expect("ZipReader APIs require Text");
    let buffer = class_id(program, |native| matches!(native, C::ByteBuffer))
        .expect("ZipReader APIs require ByteBuffer");

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
    ] {
        let result = result_id(native_function(program, |native| native == method));
        replacements.push((placeholder, variant_id(program, result, "Ok").to_string()));
    }
    for (placeholder, method, index) in [
        ("@OPEN_PATH_ID@", M::ZipReaderOpen, 0),
        ("@FROM_BYTES_BUFFER_ID@", M::ZipReaderFromBytes, 0),
        ("@READ_MAX_BYTES_ID@", M::ZipReaderReadEntry, 0),
    ] {
        replacements.push((placeholder, native_function(program, |native| native == method).params[index].0.to_string()));
    }

    let body = template_section(
        TEMPLATE,
        "/* ZIP_GROUP reader */",
        "/* ZIP_GROUP_END reader */",
    ).to_owned();
    let body = replace_placeholders(body, &replacements);
    replace_method_ids(body, program, None)
}

fn writer_implementation(program: &t::Program) -> String {
    let writer = class_id(program, |native| matches!(native, C::ZipWriter))
        .expect("ZipWriter implementation requires ZipWriter");
    let path = class_id(program, |native| matches!(native, C::Path))
        .expect("ZipWriter APIs require Path");
    let text = text_class(program).expect("ZipWriter APIs require Text");
    let buffer = class_id(program, |native| matches!(native, C::ByteBuffer))
        .expect("ZipWriter APIs require ByteBuffer");

    let mut replacements = vec![
        ("@ZIP_WRITER_ID@", writer.to_string()),
        ("@PATH_ID@", path.to_string()),
        ("@TEXT_ID@", text.to_string()),
        ("@BUFFER_ID@", buffer.to_string()),
    ];
    for (placeholder, method) in [
        ("@CREATE_RESULT_ID@", M::ZipWriterCreate),
        ("@ADD_RESULT_ID@", M::ZipWriterAdd),
        ("@ADD_DIRECTORY_RESULT_ID@", M::ZipWriterAddDirectory),
        ("@WRITER_CLOSE_RESULT_ID@", M::ZipWriterClose),
    ] {
        replacements.push((placeholder, result_id(native_function(program, |native| native == method)).to_string()));
    }
    for (placeholder, method) in [
        ("@CREATE_OK_TAG@", M::ZipWriterCreate),
        ("@ADD_OK_TAG@", M::ZipWriterAdd),
        ("@ADD_DIRECTORY_OK_TAG@", M::ZipWriterAddDirectory),
        ("@WRITER_CLOSE_OK_TAG@", M::ZipWriterClose),
    ] {
        let result = result_id(native_function(program, |native| native == method));
        replacements.push((placeholder, variant_id(program, result, "Ok").to_string()));
    }
    for (placeholder, method, index) in [
        ("@CREATE_PATH_ID@", M::ZipWriterCreate, 0),
        ("@CREATE_LEVEL_ID@", M::ZipWriterCreate, 1),
        ("@ADD_NAME_ID@", M::ZipWriterAdd, 0),
        ("@ADD_BYTES_ID@", M::ZipWriterAdd, 1),
        ("@ADD_DIRECTORY_NAME_ID@", M::ZipWriterAddDirectory, 0),
    ] {
        replacements.push((placeholder, native_function(program, |native| native == method).params[index].0.to_string()));
    }

    let body = template_section(
        TEMPLATE,
        "/* ZIP_GROUP writer */",
        "/* ZIP_GROUP_END writer */",
    ).to_owned();
    let body = replace_placeholders(body, &replacements);
    replace_method_ids(body, program, None)
}

fn class_id(program: &t::Program, is: impl Fn(C) -> bool) -> Option<usize> {
    program.classes.iter().find(|class| class.native.is_some_and(&is)).map(|class| class.id)
}

