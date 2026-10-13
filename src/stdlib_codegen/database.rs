use crate::ir::{self as t, NativeClass as C, NativeMethod as M, Type};
use super::support::{native_function, replace_method_ids, result_id, variant, variant_id};

pub(super) fn database_fields() -> String {
    "    void *database;\n    void *lifetime;\n    void (*destroy)(void *);\n".to_owned()
}

pub(super) fn statement_fields() -> String {
    "    void *statement;\n    void *lifetime;\n    bool row_active;\n    bool statement_done;\n    bool statement_reset_required;\n    void (*destroy)(void *);\n".to_owned()
}

pub(super) fn drop_fields() -> String {
    "    if (p->destroy != NULL) { void (*destroy)(void *) = p->destroy; p->destroy = NULL; destroy(p); }\n".to_owned()
}

fn parameter(function: &t::Function, index: usize) -> String {
    format!("ncr_v_{}", function.params.get(index)
        .expect("compiler-owned SQL parameter is present").0)
}

fn result_class(function: &t::Function) -> usize {
    let class = result_id(function);
    assert_eq!(function.return_type, Type::Object(class));
    class
}

fn assert_parameters(function: &t::Function, expected: &[Type]) {
    assert_eq!(function.params.len(), expected.len(), "SQL native parameter count");
    for (index, (_, actual)) in function.params.iter().enumerate() {
        assert_eq!(*actual, expected[index], "SQL native parameter type");
    }
}

fn assert_result_payload(program: &t::Program, function: &t::Function,
                         payload: Type, error: Type) -> usize {
    let result = result_class(function);
    let (_, ok) = variant(program, result, "Ok");
    assert_eq!(ok.fields.as_slice(), &[payload], "SQL Result<T, Text> success payload");
    let (_, err) = variant(program, result, "Err");
    assert_eq!(err.fields.as_slice(), &[error], "SQL Result<T, Text> error payload");
    result
}

fn option_info(program: &t::Program, method: M, expected_payload: Type) -> (usize, usize, usize, usize) {
    let function = native_function(program, |candidate| candidate == method);
    let result = result_class(function);
    let (_, ok) = variant(program, result, "Ok");
    let [Type::Object(option)] = ok.fields.as_slice() else {
        unreachable!("SQL optional column result returns Option<T>");
    };
    let (_, some) = variant(program, *option, "Some");
    assert_eq!(some.fields.as_slice(), &[expected_payload], "SQL column Option<T> payload type");
    let none = variant_id(program, *option, "None");
    let some = variant_id(program, *option, "Some");
    (result, *option, none, some)
}

pub(super) fn implementation(program: &t::Program, class_id: usize) -> String {
    assert!(matches!(program.classes[class_id].native, Some(C::Database)),
        "Database implementation belongs to its native class");
    let statement_class = program.classes.iter()
        .find(|class| matches!(class.native, Some(C::SqlStatement)))
        .expect("Database requires SqlStatement").id;
    let text = program.classes.iter().find(|class| matches!(class.native, Some(C::Text)))
        .expect("Database requires Text").id;
    let buffer = program.classes.iter().find(|class| matches!(class.native, Some(C::ByteBuffer)))
        .expect("Database requires ByteBuffer").id;

    let open = native_function(program, |method| method == M::DatabaseOpen);
    let prepare = native_function(program, |method| method == M::DatabasePrepare);
    let changes = native_function(program, |method| method == M::DatabaseChanges);
    let rowid = native_function(program, |method| method == M::DatabaseLastInsertRowId);
    let execute = native_function(program, |method| method == M::DatabaseExecute);
    let begin = native_function(program, |method| method == M::DatabaseBegin);
    let commit = native_function(program, |method| method == M::DatabaseCommit);
    let rollback = native_function(program, |method| method == M::DatabaseRollback);
    let timeout = native_function(program, |method| method == M::DatabaseSetBusyTimeoutMillis);
    let database_close = native_function(program, |method| method == M::DatabaseClose);
    let bind_null = native_function(program, |method| method == M::SqlStatementBindNull);
    let bind_long = native_function(program, |method| method == M::SqlStatementBindLong);
    let bind_double = native_function(program, |method| method == M::SqlStatementBindDouble);
    let bind_text = native_function(program, |method| method == M::SqlStatementBindText);
    let bind_blob = native_function(program, |method| method == M::SqlStatementBindBlob);
    let step = native_function(program, |method| method == M::SqlStatementStep);
    let reset = native_function(program, |method| method == M::SqlStatementReset);
    let clear = native_function(program, |method| method == M::SqlStatementClearBindings);
    let column_count = native_function(program, |method| method == M::SqlStatementColumnCount);
    let column_name = native_function(program, |method| method == M::SqlStatementColumnName);
    let column_type = native_function(program, |method| method == M::SqlStatementColumnType);
    let column_long = native_function(program, |method| method == M::SqlStatementColumnLong);
    let column_double = native_function(program, |method| method == M::SqlStatementColumnDouble);
    let column_text = native_function(program, |method| method == M::SqlStatementColumnText);
    let column_blob = native_function(program, |method| method == M::SqlStatementColumnBlob);
    let statement_close = native_function(program, |method| method == M::SqlStatementClose);

    let borrowed_text = Type::Borrow { class: text, mutable: false };
    let borrowed_buffer = Type::Borrow { class: buffer, mutable: false };
    assert_parameters(open, &[borrowed_text]);
    assert_parameters(prepare, &[borrowed_text]);
    assert_parameters(bind_null, &[Type::Int]);
    assert_parameters(bind_long, &[Type::Int, Type::Long]);
    assert_parameters(bind_double, &[Type::Int, Type::Double]);
    assert_parameters(bind_text, &[Type::Int, borrowed_text]);
    assert_parameters(bind_blob, &[Type::Int, borrowed_buffer]);
    assert_parameters(step, &[]);
    assert_parameters(reset, &[]);
    assert_parameters(clear, &[]);
    assert_parameters(column_count, &[]);
    for function in [column_name, column_type, column_long, column_double, column_text, column_blob] {
        assert_parameters(function, &[Type::Int]);
    }
    assert_parameters(changes, &[]);
    assert_parameters(rowid, &[]);
    assert_parameters(execute, &[borrowed_text]);
    assert_parameters(begin, &[]);
    assert_parameters(commit, &[]);
    assert_parameters(rollback, &[]);
    assert_parameters(timeout, &[Type::Long]);
    assert_parameters(database_close, &[]);
    assert_parameters(statement_close, &[]);

    let text_type = Type::Object(text);
    let database_unit_result = assert_result_payload(program, begin, Type::Unit, text_type);
    for function in [commit, rollback, timeout, database_close] {
        let result = assert_result_payload(program, function, Type::Unit, text_type);
        assert_eq!(result, database_unit_result, "Database command methods share Result<Unit, Text>");
    }
    let statement_unit_result = assert_result_payload(program, bind_null, Type::Unit, text_type);
    for function in [bind_long, bind_double, bind_text, bind_blob, reset, clear, statement_close] {
        let result = assert_result_payload(program, function, Type::Unit, text_type);
        assert_eq!(result, statement_unit_result, "SqlStatement command methods share Result<Unit, Text>");
    }
    assert_result_payload(program, open, Type::Object(class_id), text_type);
    assert_result_payload(program, prepare, Type::Object(statement_class), text_type);
    assert_result_payload(program, step, Type::Boolean, text_type);
    assert_result_payload(program, column_count, Type::Int, text_type);
    assert_result_payload(program, column_name, text_type, text_type);
    assert_result_payload(program, column_type, Type::Int, text_type);
    assert_result_payload(program, changes, Type::Long, text_type);
    assert_result_payload(program, rowid, Type::Long, text_type);
    assert_result_payload(program, execute, Type::Long, text_type);

    let (long_result, long_option, long_none, long_some) =
        option_info(program, M::SqlStatementColumnLong, Type::Long);
    let (double_result, double_option, double_none, double_some) =
        option_info(program, M::SqlStatementColumnDouble, Type::Double);
    let (text_result, text_option, text_none, text_some) =
        option_info(program, M::SqlStatementColumnText, text_type);
    let (blob_result, blob_option, blob_none, blob_some) =
        option_info(program, M::SqlStatementColumnBlob, Type::Object(buffer));
    assert_result_payload(program, column_long, Type::Object(long_option), text_type);
    assert_result_payload(program, column_double, Type::Object(double_option), text_type);
    assert_result_payload(program, column_text, Type::Object(text_option), text_type);
    assert_result_payload(program, column_blob, Type::Object(blob_option), text_type);

    let mut replacements = vec![
        ("@DATABASE_CLASS_ID@", class_id.to_string()),
        ("@STATEMENT_CLASS_ID@", statement_class.to_string()),
        ("@TEXT_CLASS_ID@", text.to_string()),
        ("@BUFFER_CLASS_ID@", buffer.to_string()),
        ("@DB_OPEN_RESULT@", result_class(open).to_string()),
        ("@DB_OPEN_OK_TAG@", variant_id(program, result_class(open), "Ok").to_string()),
        ("@DB_PREPARE_RESULT@", result_class(prepare).to_string()),
        ("@DB_PREPARE_OK_TAG@", variant_id(program, result_class(prepare), "Ok").to_string()),
        ("@DB_UNIT_RESULT@", database_unit_result.to_string()),
        ("@DB_UNIT_OK_TAG@", variant_id(program, database_unit_result, "Ok").to_string()),
        ("@STATEMENT_UNIT_RESULT@", statement_unit_result.to_string()),
        ("@STATEMENT_UNIT_OK_TAG@", variant_id(program, statement_unit_result, "Ok").to_string()),
        ("@DB_STEP_RESULT@", result_class(step).to_string()),
        ("@DB_STEP_OK_TAG@", variant_id(program, result_class(step), "Ok").to_string()),
        ("@STATEMENT_COLUMN_COUNT_RESULT@", result_class(column_count).to_string()),
        ("@STATEMENT_COLUMN_COUNT_OK_TAG@", variant_id(program, result_class(column_count), "Ok").to_string()),
        ("@STATEMENT_COLUMN_NAME_RESULT@", result_class(column_name).to_string()),
        ("@STATEMENT_COLUMN_NAME_OK_TAG@", variant_id(program, result_class(column_name), "Ok").to_string()),
        ("@STATEMENT_COLUMN_TYPE_RESULT@", result_class(column_type).to_string()),
        ("@STATEMENT_COLUMN_TYPE_OK_TAG@", variant_id(program, result_class(column_type), "Ok").to_string()),
        ("@STATEMENT_COLUMN_LONG_RESULT@", long_result.to_string()),
        ("@STATEMENT_COLUMN_LONG_OK_TAG@", variant_id(program, long_result, "Ok").to_string()),
        ("@STATEMENT_COLUMN_LONG_OPTION@", long_option.to_string()),
        ("@STATEMENT_COLUMN_LONG_NONE_TAG@", long_none.to_string()),
        ("@STATEMENT_COLUMN_LONG_SOME_TAG@", long_some.to_string()),
        ("@STATEMENT_COLUMN_DOUBLE_RESULT@", double_result.to_string()),
        ("@STATEMENT_COLUMN_DOUBLE_OK_TAG@", variant_id(program, double_result, "Ok").to_string()),
        ("@STATEMENT_COLUMN_DOUBLE_OPTION@", double_option.to_string()),
        ("@STATEMENT_COLUMN_DOUBLE_NONE_TAG@", double_none.to_string()),
        ("@STATEMENT_COLUMN_DOUBLE_SOME_TAG@", double_some.to_string()),
        ("@STATEMENT_COLUMN_TEXT_RESULT@", text_result.to_string()),
        ("@STATEMENT_COLUMN_TEXT_OK_TAG@", variant_id(program, text_result, "Ok").to_string()),
        ("@STATEMENT_COLUMN_TEXT_OPTION@", text_option.to_string()),
        ("@STATEMENT_COLUMN_TEXT_NONE_TAG@", text_none.to_string()),
        ("@STATEMENT_COLUMN_TEXT_SOME_TAG@", text_some.to_string()),
        ("@STATEMENT_COLUMN_BLOB_RESULT@", blob_result.to_string()),
        ("@STATEMENT_COLUMN_BLOB_OK_TAG@", variant_id(program, blob_result, "Ok").to_string()),
        ("@STATEMENT_COLUMN_BLOB_OPTION@", blob_option.to_string()),
        ("@STATEMENT_COLUMN_BLOB_NONE_TAG@", blob_none.to_string()),
        ("@STATEMENT_COLUMN_BLOB_SOME_TAG@", blob_some.to_string()),
        ("@DB_CHANGES_RESULT@", result_class(changes).to_string()),
        ("@DB_CHANGES_OK_TAG@", variant_id(program, result_class(changes), "Ok").to_string()),
        ("@DB_ROWID_RESULT@", result_class(rowid).to_string()),
        ("@DB_ROWID_OK_TAG@", variant_id(program, result_class(rowid), "Ok").to_string()),
        ("@DB_EXECUTE_RESULT@", result_class(execute).to_string()),
        ("@DB_EXECUTE_OK_TAG@", variant_id(program, result_class(execute), "Ok").to_string()),
    ];
    for (placeholder, method, index) in [
        ("@DB_OPEN_PATH_ID@", M::DatabaseOpen, 0),
        ("@DB_PREPARE_SQL_ID@", M::DatabasePrepare, 0),
        ("@DB_EXECUTE_SQL_ID@", M::DatabaseExecute, 0),
        ("@DB_TIMEOUT_MILLIS_ID@", M::DatabaseSetBusyTimeoutMillis, 0),
        ("@STMT_BIND_NULL_INDEX_ID@", M::SqlStatementBindNull, 0),
        ("@STMT_BIND_LONG_INDEX_ID@", M::SqlStatementBindLong, 0),
        ("@STMT_BIND_LONG_VALUE_ID@", M::SqlStatementBindLong, 1),
        ("@STMT_BIND_DOUBLE_INDEX_ID@", M::SqlStatementBindDouble, 0),
        ("@STMT_BIND_DOUBLE_VALUE_ID@", M::SqlStatementBindDouble, 1),
        ("@STMT_BIND_TEXT_INDEX_ID@", M::SqlStatementBindText, 0),
        ("@STMT_BIND_TEXT_VALUE_ID@", M::SqlStatementBindText, 1),
        ("@STMT_BIND_BLOB_INDEX_ID@", M::SqlStatementBindBlob, 0),
        ("@STMT_BIND_BLOB_VALUE_ID@", M::SqlStatementBindBlob, 1),
        ("@STMT_COLUMN_NAME_INDEX_ID@", M::SqlStatementColumnName, 0),
        ("@STMT_COLUMN_TYPE_INDEX_ID@", M::SqlStatementColumnType, 0),
        ("@STMT_COLUMN_LONG_INDEX_ID@", M::SqlStatementColumnLong, 0),
        ("@STMT_COLUMN_DOUBLE_INDEX_ID@", M::SqlStatementColumnDouble, 0),
        ("@STMT_COLUMN_TEXT_INDEX_ID@", M::SqlStatementColumnText, 0),
        ("@STMT_COLUMN_BLOB_INDEX_ID@", M::SqlStatementColumnBlob, 0),
    ] {
        let function = native_function(program, |candidate| candidate == method);
        replacements.push((placeholder, parameter(function, index)));
    }
    let template = include_str!("../../stdlib/database.c.in");
    let body = super::support::replace_placeholders(template.to_owned(), &replacements);
    let body = replace_method_ids(body, program, None);
    debug_assert!(!super::support::has_placeholder(&body), "unexpanded SQL placeholder");
    body
}
