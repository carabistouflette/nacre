use crate::ir::{self as t, NativeClass as C, NativeMethod as M, Type};
use super::support::{
    native_function, replace_method_ids, replace_placeholders, result_id, text_class, variant, variant_id,
};

pub(super) fn fields(program: &t::Program, class: C) -> String {
    match class {
        C::Path => format!("    ncr_obj_{} *text;\n", text_class(program).expect("Path requires Text")),
        C::File => "    int fd;\n".to_owned(),
        C::Directory => "    void *directory;\n".to_owned(),
        C::FileInfo => "    int64_t size;\n    bool is_file;\n    bool is_directory;\n    bool is_symlink;\n    int64_t modified_millis;\n    int64_t accessed_millis;\n    int64_t changed_millis;\n    int32_t mode;\n".to_owned(),
        C::Watcher => "    int fd;\n    unsigned char *events;\n    size_t events_length;\n    size_t events_offset;\n    bool invalidated;\n".to_owned(),
        C::WatchEvent => {
            let text = text_class(program).expect("WatchEvent requires Text");
            let kind = program.classes.iter().find(|class| class.name == "WatchKind")
                .expect("WatchEvent requires WatchKind").id;
            format!("    ncr_obj_{text} *name;\n    ncr_obj_{kind} kind;\n    uint32_t cookie;\n    bool is_directory;\n    bool overflow;\n")
        }
        C::Files => "    unsigned char ncr_static_class;\n".to_owned(),
        _ => unreachable!("filesystem storage requested for another native class"),
    }
}

pub(super) fn drop_fields(program: &t::Program, class: C) -> String {
    match class {
        C::Path => format!("    ncr_drop_{}(p->text);\n", text_class(program).expect("Path requires Text")),
        C::File => "    if (p->fd >= 0) { int descriptor = p->fd; p->fd = -1; (void)close(descriptor); }\n".to_owned(),
        C::Directory => "    if (p->directory != NULL) { void *directory = p->directory; p->directory = NULL; (void)closedir((DIR *)directory); }\n".to_owned(),
        C::Watcher => "    if (p->fd >= 0) { int descriptor = p->fd; p->fd = -1; (void)close(descriptor); }\n    if (p->events != NULL) { ncr_deallocate(p->events); p->events = NULL; }\n".to_owned(),
        C::WatchEvent => format!("    ncr_drop_{}(p->name);\n", text_class(program).expect("WatchEvent requires Text")),
        C::FileInfo | C::Files => String::new(),
        _ => unreachable!("filesystem drop requested for another native class"),
    }
}

fn class_id(program: &t::Program, is: impl Fn(C) -> bool) -> Option<usize> {
    program.classes.iter().find(|class| class.native.is_some_and(&is)).map(|class| class.id)
}

fn native_method(program: &t::Program, method: M) -> &t::Function {
    native_function(program, |candidate| candidate == method)
}
fn watch_kind_id(program: &t::Program) -> usize {
    program.classes.iter().find(|class| class.name == "WatchKind")
        .expect("Watcher and WatchEvent require WatchKind").id
}


fn parameter(function: &t::Function, index: usize) -> String {
    format!("ncr_v_{}", function.params[index].0)
}

fn add_parameters(
    program: &t::Program,
    replacements: &mut Vec<(&'static str, String)>,
    parameters: &[(&'static str, M, usize)],
) {
    for (placeholder, method, index) in parameters {
        replacements.push((*placeholder, parameter(native_method(program, *method), *index)));
    }
}

fn add_results(
    program: &t::Program,
    replacements: &mut Vec<(&'static str, String)>,
    results: &[(&'static str, &'static str, M)],
) {
    for (result_placeholder, ok_placeholder, method) in results {
        let result = result_id(native_method(program, *method));
        replacements.push((*result_placeholder, result.to_string()));
        replacements.push((*ok_placeholder, variant_id(program, result, "Ok").to_string()));
    }
}

fn add_self_parameters(
    replacements: &mut Vec<(&'static str, String)>,
    placeholders: &[&'static str],
) {
    replacements.extend(placeholders.iter().map(|placeholder| (*placeholder, "ncr_self".to_owned())));
}

fn expand_template(
    program: &t::Program,
    template: &str,
    replacements: &[(&str, String)],
) -> String {
    let body = replace_placeholders(template.to_owned(), replacements);
    let body = replace_method_ids(body, program, None);
    debug_assert!(!super::support::has_placeholder(&body), "unexpanded filesystem placeholder");
    body
}

fn path_implementation(program: &t::Program, text: usize, path: usize) -> String {
    let mut replacements = vec![
        ("@TEXT_ID@", text.to_string()),
        ("@PATH_ID@", path.to_string()),
        ("@TEXT_FROM_BYTES@", format!("ncr_text_from_bytes_{text}")),
        ("@TEXT_TAKE_BYTES@", format!("ncr_text_take_bytes_{text}")),
    ];
    add_parameters(program, &mut replacements, &[
        ("@PATH_FROM_TEXT_ARG@", M::PathFromText, 0),
        ("@PATH_JOIN_BASE@", M::PathJoin, 0),
        ("@PATH_JOIN_CHILD@", M::PathJoin, 1),
    ]);
    add_results(program, &mut replacements, &[
        ("@PATH_FROM_TEXT_RESULT@", "@PATH_FROM_TEXT_OK_TAG@", M::PathFromText),
        ("@PATH_JOIN_RESULT@", "@PATH_JOIN_OK_TAG@", M::PathJoin),
    ]);
    add_self_parameters(&mut replacements, &[
        "@PATH_CLONE_SELF@", "@PATH_IS_ABSOLUTE_SELF@", "@PATH_AS_TEXT_SELF@",
    ]);
    expand_template(program, include_str!("../../stdlib/path.c.in"), &replacements)
}

fn file_implementation(program: &t::Program, text: usize, path: usize, file: usize, buffer: usize) -> String {
    let mut replacements = vec![
        ("@TEXT_ID@", text.to_string()),
        ("@PATH_ID@", path.to_string()),
        ("@FILE_ID@", file.to_string()),
        ("@BUFFER_ID@", buffer.to_string()),
        ("@BUFFER_RESERVE@", format!("ncr_byte_buffer_reserve_{buffer}")),
    ];
    add_parameters(program, &mut replacements, &[
        ("@FILE_OPEN_READ_PATH@", M::FileOpenRead, 0),
        ("@FILE_CREATE_PATH@", M::FileCreate, 0),
        ("@FILE_CREATE_MODE@", M::FileCreate, 1),
        ("@FILE_OPEN_APPEND_PATH@", M::FileOpenAppend, 0),
        ("@FILE_OPEN_APPEND_MODE@", M::FileOpenAppend, 1),
        ("@FILE_OPEN_READ_WRITE_PATH@", M::FileOpenReadWrite, 0),
        ("@FILE_OPEN_READ_WRITE_MODE@", M::FileOpenReadWrite, 1),
        ("@FILE_OPEN_READ_WRITE_CREATE@", M::FileOpenReadWrite, 2),
        ("@FILE_READ_DESTINATION@", M::FileReadSome, 0),
        ("@FILE_READ_MAX@", M::FileReadSome, 1),
        ("@FILE_WRITE_SOURCE@", M::FileWriteSome, 0),
        ("@FILE_WRITE_OFFSET@", M::FileWriteSome, 1),
        ("@FILE_WRITE_MAX@", M::FileWriteSome, 2),
        ("@FILE_READ_INTERFACE_DESTINATION@", M::FileRead, 0),
        ("@FILE_READ_INTERFACE_MAX@", M::FileRead, 1),
        ("@FILE_WRITE_INTERFACE_SOURCE@", M::FileWrite, 0),
        ("@FILE_WRITE_INTERFACE_OFFSET@", M::FileWrite, 1),
        ("@FILE_WRITE_INTERFACE_MAX@", M::FileWrite, 2),
        ("@FILE_SEEK_OFFSET@", M::FileSeek, 0),
        ("@FILE_SEEK_ORIGIN@", M::FileSeek, 1),
        ("@FILE_TRUNCATE_LENGTH@", M::FileTruncate, 0),
        ("@FILE_READ_AT_DESTINATION@", M::FileReadAt, 0),
        ("@FILE_READ_AT_FILE_OFFSET@", M::FileReadAt, 1),
        ("@FILE_READ_AT_MAX@", M::FileReadAt, 2),
        ("@FILE_WRITE_AT_SOURCE@", M::FileWriteAt, 0),
        ("@FILE_WRITE_AT_BUFFER_OFFSET@", M::FileWriteAt, 1),
        ("@FILE_WRITE_AT_FILE_OFFSET@", M::FileWriteAt, 2),
        ("@FILE_WRITE_AT_MAX@", M::FileWriteAt, 3),
        ("@FILE_WRITE_ALL_SOURCE@", M::FileWriteAll, 0),
    ]);
    add_results(program, &mut replacements, &[
        ("@FILE_OPEN_READ_RESULT@", "@FILE_OPEN_READ_OK_TAG@", M::FileOpenRead),
        ("@FILE_CREATE_RESULT@", "@FILE_CREATE_OK_TAG@", M::FileCreate),
        ("@FILE_OPEN_APPEND_RESULT@", "@FILE_OPEN_APPEND_OK_TAG@", M::FileOpenAppend),
        ("@FILE_OPEN_READ_WRITE_RESULT@", "@FILE_OPEN_READ_WRITE_OK_TAG@", M::FileOpenReadWrite),
        ("@FILE_READ_RESULT@", "@FILE_READ_OK_TAG@", M::FileReadSome),
        ("@FILE_WRITE_RESULT@", "@FILE_WRITE_OK_TAG@", M::FileWriteSome),
        ("@FILE_POSITION_RESULT@", "@FILE_POSITION_OK_TAG@", M::FilePosition),
        ("@FILE_SEEK_RESULT@", "@FILE_SEEK_OK_TAG@", M::FileSeek),
        ("@FILE_TRUNCATE_RESULT@", "@FILE_TRUNCATE_OK_TAG@", M::FileTruncate),
        ("@FILE_READ_AT_RESULT@", "@FILE_READ_AT_OK_TAG@", M::FileReadAt),
        ("@FILE_WRITE_AT_RESULT@", "@FILE_WRITE_AT_OK_TAG@", M::FileWriteAt),
        ("@FILE_WRITE_ALL_RESULT@", "@FILE_WRITE_ALL_OK_TAG@", M::FileWriteAll),
        ("@FILE_FLUSH_RESULT@", "@FILE_FLUSH_OK_TAG@", M::FileFlush),
        ("@FILE_CLOSE_RESULT@", "@FILE_CLOSE_OK_TAG@", M::FileClose),
    ]);
    add_self_parameters(&mut replacements, &[
        "@FILE_READ_SELF@", "@FILE_WRITE_SELF@", "@FILE_READ_INTERFACE_SELF@",
        "@FILE_WRITE_INTERFACE_SELF@", "@FILE_POSITION_SELF@", "@FILE_SEEK_SELF@",
        "@FILE_TRUNCATE_SELF@", "@FILE_READ_AT_SELF@", "@FILE_WRITE_AT_SELF@",
        "@FILE_WRITE_ALL_SELF@", "@FILE_FLUSH_SELF@", "@FILE_CLOSE_SELF@",
    ]);
    let mut body = expand_template(program, include_str!("../../stdlib/file.c.in"), &replacements);
    body.push_str(&expand_template(program, include_str!("../../stdlib/file_random_access.c.in"), &replacements));
    body
}

fn directory_implementation(program: &t::Program, text: usize, path: usize, directory: usize) -> String {
    let mut replacements = vec![
        ("@TEXT_ID@", text.to_string()),
        ("@PATH_ID@", path.to_string()),
        ("@DIRECTORY_ID@", directory.to_string()),
        ("@TEXT_FROM_BYTES@", format!("ncr_text_from_bytes_{text}")),
    ];
    add_parameters(program, &mut replacements, &[
        ("@DIRECTORY_OPEN_PATH@", M::DirectoryOpen, 0),
    ]);
    add_results(program, &mut replacements, &[
        ("@DIRECTORY_OPEN_RESULT@", "@DIRECTORY_OPEN_OK_TAG@", M::DirectoryOpen),
        ("@DIRECTORY_NEXT_RESULT@", "@DIRECTORY_NEXT_OK_TAG@", M::DirectoryNextName),
        ("@DIRECTORY_CLOSE_RESULT@", "@DIRECTORY_CLOSE_OK_TAG@", M::DirectoryClose),
    ]);
    add_self_parameters(&mut replacements, &["@DIRECTORY_NEXT_SELF@", "@DIRECTORY_CLOSE_SELF@"]);
    let next_result = result_id(native_method(program, M::DirectoryNextName));
    let (_, ok_variant) = variant(program, next_result, "Ok");
    let [Type::Object(option)] = ok_variant.fields.as_slice() else {
        unreachable!("Directory.nextName returns Result<Option<Text>, Text>");
    };
    let option = *option;
    replacements.push(("@DIRECTORY_OPTION_ID@", option.to_string()));
    replacements.push(("@DIRECTORY_OPTION_NONE_TAG@", variant_id(program, option, "None").to_string()));
    replacements.push(("@DIRECTORY_OPTION_SOME_TAG@", variant_id(program, option, "Some").to_string()));
    expand_template(program, include_str!("../../stdlib/directory.c.in"), &replacements)
}

fn file_info_implementation(program: &t::Program, file_info: usize) -> String {
    let replacements = vec![("@FILE_INFO_ID@", file_info.to_string())];
    expand_template(program, include_str!("../../stdlib/file_info.c.in"), &replacements)
}
fn watcher_implementation(
    program: &t::Program,
    text: usize,
    path: usize,
    watcher: usize,
    watch_event: usize,
) -> String {
    let mut replacements = vec![
        ("@TEXT_ID@", text.to_string()),
        ("@PATH_ID@", path.to_string()),
        ("@WATCHER_ID@", watcher.to_string()),
        ("@WATCH_EVENT_ID@", watch_event.to_string()),
    ];
    add_parameters(program, &mut replacements, &[
        ("@WATCHER_OPEN_PATH@", M::WatcherOpen, 0),
        ("@WATCHER_NEXT_TIMEOUT@", M::WatcherNext, 0),
    ]);
    add_results(program, &mut replacements, &[
        ("@WATCHER_OPEN_RESULT@", "@WATCHER_OPEN_OK_TAG@", M::WatcherOpen),
        ("@WATCHER_NEXT_RESULT@", "@WATCHER_NEXT_OK_TAG@", M::WatcherNext),
        ("@WATCHER_CLOSE_RESULT@", "@WATCHER_CLOSE_OK_TAG@", M::WatcherClose),
    ]);
    add_self_parameters(&mut replacements, &[
        "@WATCHER_NEXT_SELF@", "@WATCHER_CLOSE_SELF@",
    ]);
    let next_result = result_id(native_method(program, M::WatcherNext));
    let (_, next_ok) = variant(program, next_result, "Ok");
    let [Type::Object(option)] = next_ok.fields.as_slice() else {
        unreachable!("Watcher.next returns Result<Option<WatchEvent>, Text>");
    };
    let option = *option;
    let (_, option_some) = variant(program, option, "Some");
    let [Type::Object(event)] = option_some.fields.as_slice() else {
        unreachable!("Watcher.next Some variant contains WatchEvent");
    };
    assert_eq!(*event, watch_event, "Watcher.next returns WatchEvent");
    let kind = watch_kind_id(program);
    replacements.extend([
        ("@WATCHER_OPTION_ID@", option.to_string()),
        ("@WATCHER_OPTION_NONE_TAG@", variant_id(program, option, "None").to_string()),
        ("@WATCHER_OPTION_SOME_TAG@", variant_id(program, option, "Some").to_string()),
        ("@WATCH_KIND_ID@", kind.to_string()),
        ("@WATCH_KIND_CREATED_TAG@", variant_id(program, kind, "Created").to_string()),
        ("@WATCH_KIND_MODIFIED_TAG@", variant_id(program, kind, "Modified").to_string()),
        ("@WATCH_KIND_ATTRIBUTES_CHANGED_TAG@", variant_id(program, kind, "AttributesChanged").to_string()),
        ("@WATCH_KIND_CLOSED_FOR_WRITE_TAG@", variant_id(program, kind, "ClosedForWrite").to_string()),
        ("@WATCH_KIND_DELETED_TAG@", variant_id(program, kind, "Deleted").to_string()),
        ("@WATCH_KIND_MOVED_FROM_TAG@", variant_id(program, kind, "MovedFrom").to_string()),
        ("@WATCH_KIND_MOVED_TO_TAG@", variant_id(program, kind, "MovedTo").to_string()),
        ("@WATCH_KIND_MOVED_SELF_TAG@", variant_id(program, kind, "MovedSelf").to_string()),
        ("@WATCH_KIND_DELETED_SELF_TAG@", variant_id(program, kind, "DeletedSelf").to_string()),
        ("@WATCH_KIND_IGNORED_TAG@", variant_id(program, kind, "Ignored").to_string()),
        ("@WATCH_KIND_OVERFLOW_TAG@", variant_id(program, kind, "Overflow").to_string()),
    ]);
    expand_template(program, include_str!("../../stdlib/watcher.c.in"), &replacements)
}
fn watch_event_implementation(program: &t::Program, text: usize, watch_event: usize) -> String {
    let kind = watch_kind_id(program);
    let replacements = [
        ("@TEXT_ID@", text.to_string()),
        ("@WATCH_EVENT_ID@", watch_event.to_string()),
        ("@WATCH_KIND_ID@", kind.to_string()),
    ];
    expand_template(program, include_str!("../../stdlib/watch_event.c.in"), &replacements)
}



fn files_implementation(
    program: &t::Program,
    text: usize,
    path: usize,
    file_info: usize,
    buffer: usize,
) -> String {
    let list_method = native_method(program, M::FilesList);
    let list_result = result_id(list_method);
    let (_, list_ok) = variant(program, list_result, "Ok");
    let [Type::Object(list_class)] = list_ok.fields.as_slice() else {
        unreachable!("Files.list returns Result<List<Text>, Text>");
    };
    let Some(C::List(Type::Object(list_text))) = program.classes[*list_class].native else {
        unreachable!("Files.list specializes native List<Text>");
    };
    assert_eq!(list_text, text, "Files.list returns List<Text>");
    let list_new = program.functions.iter().find(|function| {
        function.receiver == Some(*list_class) && matches!(function.native, Some(M::ListNew))
    }).expect("Files.list List<Text> constructor");
    let list_add = program.functions.iter().find(|function| {
        function.receiver == Some(*list_class) && matches!(function.native, Some(M::ListAdd))
    }).expect("Files.list List<Text>.add");
    let path_from_text = native_method(program, M::PathFromText);
    let mut replacements = vec![
        ("@TEXT_ID@", text.to_string()),
        ("@PATH_ID@", path.to_string()),
        ("@FILE_INFO_ID@", file_info.to_string()),
        ("@BUFFER_ID@", buffer.to_string()),
        ("@TEXT_TAKE_BYTES@", format!("ncr_text_take_bytes_{text}")),
        ("@PATH_FROM_TEXT@", format!("ncr_fn_{}", path_from_text.id)),
        ("@LIST_ID@", list_class.to_string()),
        ("@LIST_TYPE@", crate::c_repr::ctype(program, Type::Object(*list_class)).into_owned()),
        ("@LIST_NEW@", format!("ncr_fn_{}", list_new.id)),
        ("@LIST_ADD@", format!("ncr_fn_{}", list_add.id)),
    ];
    add_parameters(program, &mut replacements, &[
        ("@FILES_METADATA_PATH@", M::FilesMetadata, 0),
        ("@FILES_CREATE_DIRECTORY_PATH@", M::FilesCreateDirectory, 0),
        ("@FILES_CREATE_DIRECTORY_MODE@", M::FilesCreateDirectory, 1),
        ("@FILES_REMOVE_FILE_PATH@", M::FilesRemoveFile, 0),
        ("@FILES_REMOVE_DIRECTORY_PATH@", M::FilesRemoveDirectory, 0),
        ("@FILES_RENAME_FROM@", M::FilesRename, 0),
        ("@FILES_RENAME_TO@", M::FilesRename, 1),
        ("@FILES_READ_TEXT_PATH@", M::FilesReadText, 0),
        ("@FILES_WRITE_TEXT_PATH@", M::FilesWriteText, 0),
        ("@FILES_WRITE_TEXT_CONTENT@", M::FilesWriteText, 1),
        ("@FILES_COPY_SOURCE@", M::FilesCopy, 0),
        ("@FILES_COPY_TARGET@", M::FilesCopy, 1),
        ("@FILES_COPY_REPLACE@", M::FilesCopy, 2),
        ("@FILES_CREATE_DIRECTORIES_PATH@", M::FilesCreateDirectories, 0),
        ("@FILES_CREATE_DIRECTORIES_MODE@", M::FilesCreateDirectories, 1),
        ("@FILES_READ_LINK_PATH@", M::FilesReadLink, 0),
        ("@FILES_SYMLINK_TARGET@", M::FilesSymlink, 0),
        ("@FILES_SYMLINK_LINK@", M::FilesSymlink, 1),
        ("@FILES_CHMOD_PATH@", M::FilesChmod, 0),
        ("@FILES_CHMOD_MODE@", M::FilesChmod, 1),
        ("@FILES_LIST_PATH@", M::FilesList, 0),
        ("@FILES_CANONICAL_PATH@", M::FilesCanonical, 0),
    ]);
    add_results(program, &mut replacements, &[
        ("@FILES_METADATA_RESULT@", "@FILES_METADATA_OK_TAG@", M::FilesMetadata),
        ("@FILES_CREATE_DIRECTORY_RESULT@", "@FILES_CREATE_DIRECTORY_OK_TAG@", M::FilesCreateDirectory),
        ("@FILES_REMOVE_FILE_RESULT@", "@FILES_REMOVE_FILE_OK_TAG@", M::FilesRemoveFile),
        ("@FILES_REMOVE_DIRECTORY_RESULT@", "@FILES_REMOVE_DIRECTORY_OK_TAG@", M::FilesRemoveDirectory),
        ("@FILES_RENAME_RESULT@", "@FILES_RENAME_OK_TAG@", M::FilesRename),
        ("@FILES_READ_TEXT_RESULT@", "@FILES_READ_TEXT_OK_TAG@", M::FilesReadText),
        ("@FILES_WRITE_TEXT_RESULT@", "@FILES_WRITE_TEXT_OK_TAG@", M::FilesWriteText),
        ("@FILES_COPY_RESULT@", "@FILES_COPY_OK_TAG@", M::FilesCopy),
        ("@FILES_CREATE_DIRECTORIES_RESULT@", "@FILES_CREATE_DIRECTORIES_OK_TAG@", M::FilesCreateDirectories),
        ("@FILES_READ_LINK_RESULT@", "@FILES_READ_LINK_OK_TAG@", M::FilesReadLink),
        ("@FILES_SYMLINK_RESULT@", "@FILES_SYMLINK_OK_TAG@", M::FilesSymlink),
        ("@FILES_CHMOD_RESULT@", "@FILES_CHMOD_OK_TAG@", M::FilesChmod),
        ("@FILES_LIST_RESULT@", "@FILES_LIST_OK_TAG@", M::FilesList),
        ("@FILES_CANONICAL_RESULT@", "@FILES_CANONICAL_OK_TAG@", M::FilesCanonical),
        ("@FILES_WRITE_ATOMIC_RESULT@", "@FILES_WRITE_ATOMIC_OK_TAG@", M::FilesWriteTextAtomic),
    ]);
    let mut body = expand_template(program, include_str!("../../stdlib/files.c.in"), &replacements);
    let atomic_result = result_id(native_method(program, M::FilesWriteTextAtomic));
    let atomic = replace_placeholders(include_str!("../../stdlib/files_atomic.c.in").to_owned(), &[
        ("@TEXT_ID@", text.to_string()),
        ("@RESULT_UNIT_ATOMIC@", atomic_result.to_string()),
        ("@UNIT_ATOMIC_OK_TAG@", variant_id(program, atomic_result, "Ok").to_string()),
        ("@ERROR_UNIT_ATOMIC@", format!("ncr_result_error_{atomic_result}")),
    ]);
    let atomic = replace_method_ids(atomic, program, None);
    debug_assert!(!super::support::has_placeholder(&atomic), "unexpanded atomic Files placeholder");
    body.push_str(&atomic);
    body.push_str(&expand_template(program, include_str!("../../stdlib/files_extended.c.in"), &replacements));
    body
}

pub(super) fn implementations(program: &t::Program) -> Option<String> {
    let path = class_id(program, |class| matches!(class, C::Path));
    let file = class_id(program, |class| matches!(class, C::File));
    let directory = class_id(program, |class| matches!(class, C::Directory));
    let file_info = class_id(program, |class| matches!(class, C::FileInfo));
    let files = class_id(program, |class| matches!(class, C::Files));
    let watcher = class_id(program, |class| matches!(class, C::Watcher));
    let watch_event = class_id(program, |class| matches!(class, C::WatchEvent));
    if path.is_none() && file.is_none() && directory.is_none() && file_info.is_none()
        && files.is_none() && watcher.is_none() && watch_event.is_none() {
        return None;
    }

    let mut output = String::new();
    let needs_paths = path.is_some() || file.is_some() || directory.is_some() || files.is_some() || watcher.is_some();
    let text = if needs_paths {
        let text = text_class(program).expect("filesystem APIs require Text");
        let common = replace_placeholders(include_str!("../../stdlib/files_common.c.in").to_owned(), &[
            ("@TEXT_ID@", text.to_string()),
        ]);
        debug_assert!(!super::support::has_placeholder(&common), "unexpanded common filesystem placeholder");
        output.push_str(&common);
        Some(text)
    } else {
        None
    };

    let needs_streams = file.is_some() || files.is_some();
    let buffer = if needs_streams {
        let buffer = class_id(program, |class| matches!(class, C::ByteBuffer))
            .expect("File and Files implementations require ByteBuffer");
        let streams = replace_placeholders(include_str!("../../stdlib/files_stream.c.in").to_owned(), &[
            ("@BUFFER_ID@", buffer.to_string()),
            ("@BUFFER_RESERVE@", format!("ncr_byte_buffer_reserve_{buffer}")),
        ]);
        debug_assert!(!super::support::has_placeholder(&streams), "unexpanded filesystem stream placeholder");
        output.push_str(&streams);
        Some(buffer)
    } else {
        None
    };

    if let Some(path) = path {
        output.push_str(&path_implementation(program, text.expect("Path requires Text"), path));
    }
    if let Some(file) = file {
        output.push_str(&file_implementation(
            program,
            text.expect("File requires Text"),
            path.expect("File requires Path"),
            file,
            buffer.expect("File requires ByteBuffer"),
        ));
    }
    if let Some(directory) = directory {
        output.push_str(&directory_implementation(
            program,
            text.expect("Directory requires Text"),
            path.expect("Directory requires Path"),
            directory,
        ));
    }
    if let Some(file_info) = file_info {
        output.push_str(&file_info_implementation(program, file_info));
    }
    if files.is_some() {
        output.push_str(&files_implementation(
            program,
            text.expect("Files requires Text"),
            path.expect("Files requires Path"),
            file_info.expect("Files.metadata requires FileInfo"),
            buffer.expect("Files text APIs require ByteBuffer"),
        ));
    }
    if let Some(watcher) = watcher {
        output.push_str(&watcher_implementation(
            program,
            text.expect("Watcher requires Text"),
            path.expect("Watcher requires Path"),
            watcher,
            watch_event.expect("Watcher requires WatchEvent"),
        ));
    }
    if let Some(watch_event) = watch_event {
        output.push_str(&watch_event_implementation(
            program,
            text_class(program).expect("WatchEvent requires Text"),
            watch_event,
        ));
    }
    Some(output)
}
