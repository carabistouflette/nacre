use crate::ir::{self as t, CallTarget, Expr, ExprKind as E, NativeMethod as M, Stmt, Type};
use super::support::{native_function, replace_placeholders, result_id, variant_id};

const ICU_METHODS: &[M] = &[
    M::UnicodeToUpperCase,
    M::UnicodeToLowerCase,
    M::UnicodeCaseFold,
    M::UnicodeNormalize,
    M::UnicodeGraphemeCount,
    M::UnicodeGraphemeAt,
    M::UnicodeCompareLocale,
    M::CharacterIsLetter,
    M::CharacterIsDigit,
    M::CharacterIsWhitespace,
    M::CharacterIsUpperCase,
    M::CharacterIsLowerCase,
    M::CharacterToUpperCase,
    M::CharacterToLowerCase,
    M::CharacterCategory,
    M::CharacterNumericValue,
    M::CharsetEncode,
    M::CharsetDecode,
    M::LocaleAvailable,
    M::LocaleFormatNumber,
    M::LocaleFormatDecimal,
    M::LocaleParseNumber,
    M::LocaleFormatCurrency,
];

fn expr_uses_icu(program: &t::Program, expr: &Expr) -> bool {
    match &expr.kind {
        E::Call { target, args, .. } => {
            matches!(target, CallTarget::Function(id)
                if program.functions[*id].native.is_some_and(|method| ICU_METHODS.contains(&method)))
                || args.iter().any(|arg| expr_uses_icu(program, arg))
        }
        E::New { args, .. } | E::Variant { args, .. } =>
            args.iter().any(|arg| expr_uses_icu(program, arg)),
        E::Drop(value) => expr_uses_icu(program, value),
        E::Upcast { value, .. } => expr_uses_icu(program, value),
        E::Unary { expr, .. } => expr_uses_icu(program, expr),
        E::Try { value, .. } | E::Cast { value } => expr_uses_icu(program, value),
        E::MapSlot { index, .. } => expr_uses_icu(program, index),
        E::Binary { left, right, .. } =>
            expr_uses_icu(program, left) || expr_uses_icu(program, right),
        E::Char(_) | E::Int(_) | E::Long(_) | E::Double(_) | E::Unit | E::Boolean(_) |
        E::String(_) | E::Read(_) | E::Move(_) | E::Borrow { .. } | E::MapCapacity(_) => false,
    }
}

fn block_uses_icu(program: &t::Program, block: &[Stmt]) -> bool {
    block.iter().any(|stmt| match stmt {
        Stmt::Let { value, .. } => value.as_ref().is_some_and(|value| expr_uses_icu(program, value)),
        Stmt::Assign { value, .. } | Stmt::Expr(value) | Stmt::Return(Some(value)) =>
            expr_uses_icu(program, value),
        Stmt::If { condition, then_block, else_block } => {
            expr_uses_icu(program, condition)
                || block_uses_icu(program, then_block)
                || else_block.as_ref().is_some_and(|block| block_uses_icu(program, block))
        }
        Stmt::While { condition, body } =>
            expr_uses_icu(program, condition) || block_uses_icu(program, body),
        Stmt::Match { value, arms } => {
            expr_uses_icu(program, value)
                || arms.iter().any(|arm| block_uses_icu(program, &arm.body))
        }
        Stmt::Block(block) => block_uses_icu(program, block),
        Stmt::Return(None) | Stmt::Break | Stmt::Continue => false,
    })
}

fn parameter(function: &t::Function, index: usize) -> String {
    function.params[index].0.to_string()
}

fn class_id(program: &t::Program, is: impl Fn(t::NativeClass) -> bool) -> usize {
    program.classes.iter().find(|class| class.native.is_some_and(&is))
        .expect("Unicode native prelude class is present").id
}

pub(super) fn implementation(program: &t::Program) -> String {
    if !program.functions.iter().any(|function| {
        function.native.is_none() && block_uses_icu(program, &function.body)
    }) {
        return String::new();
    }

    let text = class_id(program, |native| matches!(native, t::NativeClass::Text));
    let buffer = class_id(program, |native| matches!(native, t::NativeClass::ByteBuffer));
    let upper = native_function(program, |method| method == M::UnicodeToUpperCase);
    let lower = native_function(program, |method| method == M::UnicodeToLowerCase);
    let fold = native_function(program, |method| method == M::UnicodeCaseFold);
    let normalize = native_function(program, |method| method == M::UnicodeNormalize);
    let grapheme_count = native_function(program, |method| method == M::UnicodeGraphemeCount);
    let grapheme_at = native_function(program, |method| method == M::UnicodeGraphemeAt);
    let compare = native_function(program, |method| method == M::UnicodeCompareLocale);
    let encode = native_function(program, |method| method == M::CharsetEncode);
    let decode = native_function(program, |method| method == M::CharsetDecode);
    let available = native_function(program, |method| method == M::LocaleAvailable);
    let format_number = native_function(program, |method| method == M::LocaleFormatNumber);
    let format_decimal = native_function(program, |method| method == M::LocaleFormatDecimal);
    let parse_number = native_function(program, |method| method == M::LocaleParseNumber);
    let format_currency = native_function(program, |method| method == M::LocaleFormatCurrency);
    let Type::Object(list) = available.return_type else { unreachable!("Locale.available returns List<Text>") };
    assert!(matches!(program.classes[list].native, Some(t::NativeClass::List(Type::Object(element))) if element == text),
        "Locale.available specializes List<Text>");
    let list_new = program.functions.iter().find(|function| {
        function.receiver == Some(list) && function.native == Some(M::ListNew)
    }).expect("Locale.available List<Text> constructor").id;
    let list_add = program.functions.iter().find(|function| {
        function.receiver == Some(list) && function.native == Some(M::ListAdd)
    }).expect("Locale.available List<Text>.add").id;

    let upper_result = result_id(upper);
    let lower_result = result_id(lower);
    let fold_result = result_id(fold);
    let normalize_result = result_id(normalize);
    let count_result = result_id(grapheme_count);
    let at_result = result_id(grapheme_at);
    let compare_result = result_id(compare);
    let encode_result = result_id(encode);
    let decode_result = result_id(decode);
    let format_number_result = result_id(format_number);
    let format_decimal_result = result_id(format_decimal);
    let parse_number_result = result_id(parse_number);
    let format_currency_result = result_id(format_currency);

    let upper_ok = variant_id(program, upper_result, "Ok");
    let lower_ok = variant_id(program, lower_result, "Ok");
    let fold_ok = variant_id(program, fold_result, "Ok");
    let normalize_ok = variant_id(program, normalize_result, "Ok");
    let count_ok = variant_id(program, count_result, "Ok");
    let at_ok = variant_id(program, at_result, "Ok");
    let compare_ok = variant_id(program, compare_result, "Ok");
    let encode_ok = variant_id(program, encode_result, "Ok");
    let decode_ok = variant_id(program, decode_result, "Ok");
    let format_number_ok = variant_id(program, format_number_result, "Ok");
    let format_decimal_ok = variant_id(program, format_decimal_result, "Ok");
    let parse_number_ok = variant_id(program, parse_number_result, "Ok");
    let format_currency_ok = variant_id(program, format_currency_result, "Ok");

    let substitutions = [
        ("@TEXT_ID@", text.to_string()),
        ("@BUFFER_ID@", buffer.to_string()),
        ("@UPPER_RESULT@", upper_result.to_string()),
        ("@UPPER_OK@", upper_ok.to_string()),
        ("@UPPER_TEXT@", parameter(upper, 0)),
        ("@UPPER_LOCALE@", parameter(upper, 1)),
        ("@LOWER_RESULT@", lower_result.to_string()),
        ("@LOWER_OK@", lower_ok.to_string()),
        ("@LOWER_TEXT@", parameter(lower, 0)),
        ("@LOWER_LOCALE@", parameter(lower, 1)),
        ("@FOLD_RESULT@", fold_result.to_string()),
        ("@FOLD_OK@", fold_ok.to_string()),
        ("@FOLD_TEXT@", parameter(fold, 0)),
        ("@NORMALIZE_RESULT@", normalize_result.to_string()),
        ("@NORMALIZE_OK@", normalize_ok.to_string()),
        ("@NORMALIZE_TEXT@", parameter(normalize, 0)),
        ("@NORMALIZE_FORM@", parameter(normalize, 1)),
        ("@COUNT_RESULT@", count_result.to_string()),
        ("@COUNT_OK@", count_ok.to_string()),
        ("@COUNT_TEXT@", parameter(grapheme_count, 0)),
        ("@AT_RESULT@", at_result.to_string()),
        ("@AT_OK@", at_ok.to_string()),
        ("@AT_TEXT@", parameter(grapheme_at, 0)),
        ("@AT_INDEX@", parameter(grapheme_at, 1)),
        ("@COMPARE_RESULT@", compare_result.to_string()),
        ("@COMPARE_OK@", compare_ok.to_string()),
        ("@COMPARE_LEFT@", parameter(compare, 0)),
        ("@COMPARE_RIGHT@", parameter(compare, 1)),
        ("@COMPARE_LOCALE@", parameter(compare, 2)),
        ("@ENCODE_RESULT@", encode_result.to_string()),
        ("@ENCODE_OK@", encode_ok.to_string()),
        ("@ENCODE_TEXT@", parameter(encode, 0)),
        ("@ENCODE_CHARSET@", parameter(encode, 1)),
        ("@DECODE_RESULT@", decode_result.to_string()),
        ("@DECODE_OK@", decode_ok.to_string()),
        ("@DECODE_BUFFER@", parameter(decode, 0)),
        ("@DECODE_CHARSET@", parameter(decode, 1)),
        ("@LIST_TYPE@", crate::codegen::ctype(program, Type::Object(list))),
        ("@LIST_NEW@", format!("ncr_fn_{list_new}")),
        ("@LIST_ADD@", format!("ncr_fn_{list_add}")),
        ("@LIST_ID@", list.to_string()),
        ("@LIST_DROP@", format!("ncr_drop_{list}")),
        ("@FORMAT_NUMBER_RESULT@", format_number_result.to_string()),
        ("@FORMAT_NUMBER_OK@", format_number_ok.to_string()),
        ("@FORMAT_NUMBER_VALUE@", parameter(format_number, 0)),
        ("@FORMAT_NUMBER_PATTERN@", parameter(format_number, 1)),
        ("@FORMAT_NUMBER_LOCALE@", parameter(format_number, 2)),
        ("@FORMAT_DECIMAL_RESULT@", format_decimal_result.to_string()),
        ("@FORMAT_DECIMAL_OK@", format_decimal_ok.to_string()),
        ("@FORMAT_DECIMAL_VALUE@", parameter(format_decimal, 0)),
        ("@FORMAT_DECIMAL_PATTERN@", parameter(format_decimal, 1)),
        ("@FORMAT_DECIMAL_LOCALE@", parameter(format_decimal, 2)),
        ("@PARSE_NUMBER_RESULT@", parse_number_result.to_string()),
        ("@PARSE_NUMBER_OK@", parse_number_ok.to_string()),
        ("@PARSE_NUMBER_VALUE@", parameter(parse_number, 0)),
        ("@PARSE_NUMBER_PATTERN@", parameter(parse_number, 1)),
        ("@PARSE_NUMBER_LOCALE@", parameter(parse_number, 2)),
        ("@FORMAT_CURRENCY_RESULT@", format_currency_result.to_string()),
        ("@FORMAT_CURRENCY_OK@", format_currency_ok.to_string()),
        ("@FORMAT_CURRENCY_VALUE@", parameter(format_currency, 0)),
        ("@FORMAT_CURRENCY_CODE@", parameter(format_currency, 1)),
        ("@FORMAT_CURRENCY_LOCALE@", parameter(format_currency, 2)),
        ("@CHAR_LETTER@", parameter(native_function(program, |m| m == M::CharacterIsLetter), 0)),
        ("@CHAR_DIGIT@", parameter(native_function(program, |m| m == M::CharacterIsDigit), 0)),
        ("@CHAR_WHITESPACE@", parameter(native_function(program, |m| m == M::CharacterIsWhitespace), 0)),
        ("@CHAR_UPPER@", parameter(native_function(program, |m| m == M::CharacterIsUpperCase), 0)),
        ("@CHAR_LOWER@", parameter(native_function(program, |m| m == M::CharacterIsLowerCase), 0)),
        ("@CHAR_TO_UPPER@", parameter(native_function(program, |m| m == M::CharacterToUpperCase), 0)),
        ("@CHAR_TO_LOWER@", parameter(native_function(program, |m| m == M::CharacterToLowerCase), 0)),
        ("@CHAR_CATEGORY@", parameter(native_function(program, |m| m == M::CharacterCategory), 0)),
        ("@CHAR_NUMERIC@", parameter(native_function(program, |m| m == M::CharacterNumericValue), 0)),
    ];
    let mut body = include_str!("../../stdlib/unicode.c.in").to_owned();
    body.push('\n');
    body.push_str(include_str!("../../stdlib/locale.c.in"));
    replace_placeholders(body, &substitutions)
}
