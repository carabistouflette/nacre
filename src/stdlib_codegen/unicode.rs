use crate::ir::{self as t, CallTarget, Expr, ExprKind as E, NativeMethod as M, Stmt, Type};
use super::support::{
    native_function, replace_placeholders, result_id, template_section, variant_id,
};

const UNICODE_METHODS: &[M] = &[
    M::UnicodeToUpperCase,
    M::UnicodeToLowerCase,
    M::UnicodeCaseFold,
    M::UnicodeNormalize,
    M::UnicodeGraphemeCount,
    M::UnicodeGraphemeAt,
    M::UnicodeCompareLocale,
];
const CHARACTER_METHODS: &[M] = &[
    M::CharacterIsLetter,
    M::CharacterIsDigit,
    M::CharacterIsWhitespace,
    M::CharacterIsUpperCase,
    M::CharacterIsLowerCase,
    M::CharacterToUpperCase,
    M::CharacterToLowerCase,
    M::CharacterCategory,
    M::CharacterNumericValue,
];
const CHARSET_METHODS: &[M] = &[M::CharsetAvailable, M::CharsetEncode, M::CharsetDecode];
const LOCALE_METHODS: &[M] = &[
    M::LocaleAvailable,
    M::LocaleFormatNumber,
    M::LocaleFormatDecimal,
    M::LocaleParseNumber,
    M::LocaleParseDecimal,
    M::LocaleFormatCurrency,
    M::LocaleFormatCurrencyDecimal,
];
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
    M::CharsetAvailable,
    M::CharsetEncode,
    M::CharsetDecode,
    M::LocaleAvailable,
    M::LocaleFormatNumber,
    M::LocaleFormatDecimal,
    M::LocaleParseNumber,
    M::LocaleParseDecimal,
    M::LocaleFormatCurrency,
    M::LocaleFormatCurrencyDecimal,
];

type IcuUsage = [bool; ICU_METHODS.len()];

fn method_index(method: M) -> Option<usize> {
    ICU_METHODS.iter().position(|candidate| *candidate == method)
}

fn uses(usage: &IcuUsage, method: M) -> bool {
    method_index(method).is_some_and(|index| usage[index])
}

fn any_used(usage: &IcuUsage, methods: &[M]) -> bool {
    methods.iter().any(|method| uses(usage, *method))
}

fn mark_expr_icu(program: &t::Program, expr: &Expr, usage: &mut IcuUsage) {
    match &expr.kind {
        E::Call { target, args, .. } => {
            if let CallTarget::Function(id) = target {
                if let Some(method) = program.functions[*id].native {
                    if let Some(index) = method_index(method) { usage[index] = true; }
                }
            }
            for arg in args { mark_expr_icu(program, arg, usage); }
        }
        E::New { args, .. } | E::Variant { args, .. } =>
            for arg in args { mark_expr_icu(program, arg, usage); },
        E::Drop(value) | E::Upcast { value, .. } | E::Unary { expr: value, .. } |
        E::Try { value, .. } | E::Cast { value } => mark_expr_icu(program, value, usage),
        E::MapSlot { index, .. } => mark_expr_icu(program, index, usage),
        E::Binary { left, right, .. } => {
            mark_expr_icu(program, left, usage);
            mark_expr_icu(program, right, usage);
        }
        E::Char(_) | E::Int(_) | E::Long(_) | E::Double(_) | E::Unit | E::Boolean(_) |
        E::String(_) | E::Read(_) | E::Move(_) | E::Borrow { .. } | E::MapCapacity(_) => {}
    }
}

fn mark_block_icu(program: &t::Program, block: &[Stmt], usage: &mut IcuUsage) {
    for stmt in block {
        match stmt {
            Stmt::Let { value, .. } => {
                if let Some(value) = value { mark_expr_icu(program, value, usage); }
            }
            Stmt::Assign { value, .. } | Stmt::Expr(value) | Stmt::Return(Some(value)) =>
                mark_expr_icu(program, value, usage),
            Stmt::If { condition, then_block, else_block } => {
                mark_expr_icu(program, condition, usage);
                mark_block_icu(program, then_block, usage);
                if let Some(block) = else_block { mark_block_icu(program, block, usage); }
            }
            Stmt::While { condition, body } => {
                mark_expr_icu(program, condition, usage);
                mark_block_icu(program, body, usage);
            }
            Stmt::Match { value, arms } => {
                mark_expr_icu(program, value, usage);
                for arm in arms { mark_block_icu(program, &arm.body, usage); }
            }
            Stmt::Block(block) => mark_block_icu(program, block, usage),
            Stmt::Return(None) | Stmt::Break | Stmt::Continue => {}
        }
    }
}

fn class_id(program: &t::Program, is: impl Fn(t::NativeClass) -> bool) -> usize {
    program.classes.iter().find(|class| class.native.is_some_and(&is))
        .expect("Unicode native prelude class is present").id
}

fn append_section(output: &mut String, source: &str, begin: &str, end: &str) {
    output.push_str(template_section(source, begin, end));
}

fn add_param(substitutions: &mut Vec<(&'static str, String)>, placeholder: &'static str,
             function: &t::Function, index: usize) {
    substitutions.push((placeholder, function.params[index].0.to_string()));
}

fn add_result(program: &t::Program, substitutions: &mut Vec<(&'static str, String)>,
              function: &t::Function, result_placeholder: &'static str, ok_placeholder: &'static str) {
    let result = result_id(function);
    substitutions.push((result_placeholder, result.to_string()));
    substitutions.push((ok_placeholder, variant_id(program, result, "Ok").to_string()));
}

pub(super) fn implementation(program: &t::Program) -> String {
    let mut usage = [false; ICU_METHODS.len()];
    for function in program.functions.iter().filter(|function| function.native.is_none()) {
        mark_block_icu(program, &function.body, &mut usage);
    }
    if !usage.iter().any(|used| *used) { return String::new(); }

    let has_unicode = any_used(&usage, UNICODE_METHODS);
    let has_character = any_used(&usage, CHARACTER_METHODS);
    let has_charset = any_used(&usage, CHARSET_METHODS);
    let has_charset_conversion = uses(&usage, M::CharsetEncode) || uses(&usage, M::CharsetDecode);
    let has_locale = any_used(&usage, LOCALE_METHODS);
    let has_locale_numbers = [
        M::LocaleFormatNumber, M::LocaleFormatDecimal, M::LocaleParseNumber,
        M::LocaleParseDecimal, M::LocaleFormatCurrency, M::LocaleFormatCurrencyDecimal,
    ].iter().any(|method| uses(&usage, *method));
    let has_text = has_unicode || has_charset || has_locale;
    let text = has_text.then(|| class_id(program, |native| matches!(native, t::NativeClass::Text)));
    let buffer = has_charset_conversion.then(|| class_id(program, |native| matches!(native, t::NativeClass::ByteBuffer)));

    let source = include_str!("../../stdlib/unicode.c.in");
    let locale_source = include_str!("../../stdlib/locale.c.in");
    let mut body = String::new();
    append_section(&mut body, source, "/* NACRE_UNICODE_BEGIN ICU_LINK */", "/* NACRE_UNICODE_END ICU_LINK */");
    append_section(&mut body, source, "/* NACRE_UNICODE_BEGIN ICU_HEADERS */", "/* NACRE_UNICODE_END ICU_HEADERS */");

    if has_character {
        if uses(&usage, M::CharacterIsLetter) {
            append_section(&mut body, source, "/* NACRE_UNICODE_BEGIN CHARACTER_IS_LETTER */", "/* NACRE_UNICODE_END CHARACTER_IS_LETTER */");
        }
        if uses(&usage, M::CharacterIsDigit) {
            append_section(&mut body, source, "/* NACRE_UNICODE_BEGIN CHARACTER_IS_DIGIT */", "/* NACRE_UNICODE_END CHARACTER_IS_DIGIT */");
        }
        if uses(&usage, M::CharacterIsWhitespace) {
            append_section(&mut body, source, "/* NACRE_UNICODE_BEGIN CHARACTER_IS_WHITESPACE */", "/* NACRE_UNICODE_END CHARACTER_IS_WHITESPACE */");
        }
        if uses(&usage, M::CharacterIsUpperCase) {
            append_section(&mut body, source, "/* NACRE_UNICODE_BEGIN CHARACTER_IS_UPPER_CASE */", "/* NACRE_UNICODE_END CHARACTER_IS_UPPER_CASE */");
        }
        if uses(&usage, M::CharacterIsLowerCase) {
            append_section(&mut body, source, "/* NACRE_UNICODE_BEGIN CHARACTER_IS_LOWER_CASE */", "/* NACRE_UNICODE_END CHARACTER_IS_LOWER_CASE */");
        }
        if uses(&usage, M::CharacterToUpperCase) {
            append_section(&mut body, source, "/* NACRE_UNICODE_BEGIN CHARACTER_TO_UPPER_CASE */", "/* NACRE_UNICODE_END CHARACTER_TO_UPPER_CASE */");
        }
        if uses(&usage, M::CharacterToLowerCase) {
            append_section(&mut body, source, "/* NACRE_UNICODE_BEGIN CHARACTER_TO_LOWER_CASE */", "/* NACRE_UNICODE_END CHARACTER_TO_LOWER_CASE */");
        }
        if uses(&usage, M::CharacterCategory) {
            append_section(&mut body, source, "/* NACRE_UNICODE_BEGIN CHARACTER_CATEGORY */", "/* NACRE_UNICODE_END CHARACTER_CATEGORY */");
        }
        if uses(&usage, M::CharacterNumericValue) {
            append_section(&mut body, source, "/* NACRE_UNICODE_BEGIN CHARACTER_NUMERIC_VALUE */", "/* NACRE_UNICODE_END CHARACTER_NUMERIC_VALUE */");
        }
    }

    let needs_preflight = has_unicode || has_charset_conversion || has_locale;
    let needs_text_to_utf16 = has_unicode || uses(&usage, M::CharsetEncode) || has_locale_numbers;
    let needs_utf16_to_utf8 = uses(&usage, M::UnicodeToUpperCase) || uses(&usage, M::UnicodeToLowerCase) ||
        uses(&usage, M::UnicodeCaseFold) || uses(&usage, M::UnicodeNormalize) ||
        uses(&usage, M::UnicodeGraphemeAt) || uses(&usage, M::CharsetDecode) ||
        uses(&usage, M::LocaleFormatNumber) || uses(&usage, M::LocaleFormatDecimal) ||
        uses(&usage, M::LocaleFormatCurrency) || uses(&usage, M::LocaleFormatCurrencyDecimal);
    let needs_make_text = uses(&usage, M::UnicodeToUpperCase) || uses(&usage, M::UnicodeToLowerCase) ||
        uses(&usage, M::UnicodeCaseFold) || uses(&usage, M::UnicodeNormalize) ||
        uses(&usage, M::LocaleFormatNumber) || uses(&usage, M::LocaleFormatDecimal) ||
        uses(&usage, M::LocaleFormatCurrency) || uses(&usage, M::LocaleFormatCurrencyDecimal);
    let needs_locale = uses(&usage, M::UnicodeToUpperCase) || uses(&usage, M::UnicodeToLowerCase) ||
        uses(&usage, M::UnicodeCompareLocale) || has_locale_numbers;

    if needs_preflight {
        append_section(&mut body, source, "/* NACRE_UNICODE_BEGIN PREFLIGHT */", "/* NACRE_UNICODE_END PREFLIGHT */");
    }
    if needs_text_to_utf16 {
        append_section(&mut body, source, "/* NACRE_UNICODE_BEGIN TEXT_TO_UTF16 */", "/* NACRE_UNICODE_END TEXT_TO_UTF16 */");
    }
    if needs_utf16_to_utf8 {
        append_section(&mut body, source, "/* NACRE_UNICODE_BEGIN UTF16_TO_UTF8 */", "/* NACRE_UNICODE_END UTF16_TO_UTF8 */");
    }
    if uses(&usage, M::CharsetEncode) {
        append_section(&mut body, source, "/* NACRE_UNICODE_BEGIN CHARSET_FROM_UCHARS */", "/* NACRE_UNICODE_END CHARSET_FROM_UCHARS */");
    }
    if uses(&usage, M::CharsetDecode) {
        append_section(&mut body, source, "/* NACRE_UNICODE_BEGIN CHARSET_TO_UCHARS */", "/* NACRE_UNICODE_END CHARSET_TO_UCHARS */");
    }
    if uses(&usage, M::UnicodeToUpperCase) || uses(&usage, M::UnicodeToLowerCase) {
        append_section(&mut body, source, "/* NACRE_UNICODE_BEGIN CASE_MAP_HELPER */", "/* NACRE_UNICODE_END CASE_MAP_HELPER */");
    }
    if uses(&usage, M::UnicodeCaseFold) {
        append_section(&mut body, source, "/* NACRE_UNICODE_BEGIN FOLD_HELPER */", "/* NACRE_UNICODE_END FOLD_HELPER */");
    }
    if uses(&usage, M::UnicodeNormalize) {
        append_section(&mut body, source, "/* NACRE_UNICODE_BEGIN NORMALIZE_HELPER */", "/* NACRE_UNICODE_END NORMALIZE_HELPER */");
    }
    if needs_make_text {
        append_section(&mut body, source, "/* NACRE_UNICODE_BEGIN MAKE_TEXT */", "/* NACRE_UNICODE_END MAKE_TEXT */");
    }
    if needs_locale {
        append_section(&mut body, source, "/* NACRE_UNICODE_BEGIN LOCALE */", "/* NACRE_UNICODE_END LOCALE */");
    }
    if has_charset_conversion {
        append_section(&mut body, source, "/* NACRE_UNICODE_BEGIN CHARSET_NAME */", "/* NACRE_UNICODE_END CHARSET_NAME */");
    }
    if uses(&usage, M::CharsetAvailable) {
        append_section(&mut body, source, "/* NACRE_UNICODE_BEGIN CHARSET_AVAILABLE */", "/* NACRE_UNICODE_END CHARSET_AVAILABLE */");
    }

    let mut substitutions = Vec::new();
    if let Some(text) = text { substitutions.push(("@TEXT_ID@", text.to_string())); }
    if let Some(buffer) = buffer { substitutions.push(("@BUFFER_ID@", buffer.to_string())); }

    if uses(&usage, M::CharacterIsLetter) {
        add_param(&mut substitutions, "@CHAR_LETTER@", native_function(program, |method| method == M::CharacterIsLetter), 0);
    }
    if uses(&usage, M::CharacterIsDigit) {
        add_param(&mut substitutions, "@CHAR_DIGIT@", native_function(program, |method| method == M::CharacterIsDigit), 0);
    }
    if uses(&usage, M::CharacterIsWhitespace) {
        add_param(&mut substitutions, "@CHAR_WHITESPACE@", native_function(program, |method| method == M::CharacterIsWhitespace), 0);
    }
    if uses(&usage, M::CharacterIsUpperCase) {
        add_param(&mut substitutions, "@CHAR_UPPER@", native_function(program, |method| method == M::CharacterIsUpperCase), 0);
    }
    if uses(&usage, M::CharacterIsLowerCase) {
        add_param(&mut substitutions, "@CHAR_LOWER@", native_function(program, |method| method == M::CharacterIsLowerCase), 0);
    }
    if uses(&usage, M::CharacterToUpperCase) {
        add_param(&mut substitutions, "@CHAR_TO_UPPER@", native_function(program, |method| method == M::CharacterToUpperCase), 0);
    }
    if uses(&usage, M::CharacterToLowerCase) {
        add_param(&mut substitutions, "@CHAR_TO_LOWER@", native_function(program, |method| method == M::CharacterToLowerCase), 0);
    }
    if uses(&usage, M::CharacterCategory) {
        add_param(&mut substitutions, "@CHAR_CATEGORY@", native_function(program, |method| method == M::CharacterCategory), 0);
    }
    if uses(&usage, M::CharacterNumericValue) {
        add_param(&mut substitutions, "@CHAR_NUMERIC@", native_function(program, |method| method == M::CharacterNumericValue), 0);
    }

    if uses(&usage, M::UnicodeToUpperCase) {
        append_section(&mut body, source, "/* NACRE_UNICODE_BEGIN UNICODE_TO_UPPER_CASE */", "/* NACRE_UNICODE_END UNICODE_TO_UPPER_CASE */");
        let function = native_function(program, |method| method == M::UnicodeToUpperCase);
        add_result(program, &mut substitutions, function, "@UPPER_RESULT@", "@UPPER_OK@");
        add_param(&mut substitutions, "@UPPER_TEXT@", function, 0);
        add_param(&mut substitutions, "@UPPER_LOCALE@", function, 1);
    }
    if uses(&usage, M::UnicodeToLowerCase) {
        append_section(&mut body, source, "/* NACRE_UNICODE_BEGIN UNICODE_TO_LOWER_CASE */", "/* NACRE_UNICODE_END UNICODE_TO_LOWER_CASE */");
        let function = native_function(program, |method| method == M::UnicodeToLowerCase);
        add_result(program, &mut substitutions, function, "@LOWER_RESULT@", "@LOWER_OK@");
        add_param(&mut substitutions, "@LOWER_TEXT@", function, 0);
        add_param(&mut substitutions, "@LOWER_LOCALE@", function, 1);
    }
    if uses(&usage, M::UnicodeCaseFold) {
        append_section(&mut body, source, "/* NACRE_UNICODE_BEGIN UNICODE_CASE_FOLD */", "/* NACRE_UNICODE_END UNICODE_CASE_FOLD */");
        let function = native_function(program, |method| method == M::UnicodeCaseFold);
        add_result(program, &mut substitutions, function, "@FOLD_RESULT@", "@FOLD_OK@");
        add_param(&mut substitutions, "@FOLD_TEXT@", function, 0);
    }
    if uses(&usage, M::UnicodeNormalize) {
        append_section(&mut body, source, "/* NACRE_UNICODE_BEGIN UNICODE_NORMALIZE */", "/* NACRE_UNICODE_END UNICODE_NORMALIZE */");
        let function = native_function(program, |method| method == M::UnicodeNormalize);
        add_result(program, &mut substitutions, function, "@NORMALIZE_RESULT@", "@NORMALIZE_OK@");
        add_param(&mut substitutions, "@NORMALIZE_TEXT@", function, 0);
        add_param(&mut substitutions, "@NORMALIZE_FORM@", function, 1);
    }
    if uses(&usage, M::UnicodeGraphemeCount) {
        append_section(&mut body, source, "/* NACRE_UNICODE_BEGIN UNICODE_GRAPHEME_COUNT */", "/* NACRE_UNICODE_END UNICODE_GRAPHEME_COUNT */");
        let function = native_function(program, |method| method == M::UnicodeGraphemeCount);
        add_result(program, &mut substitutions, function, "@COUNT_RESULT@", "@COUNT_OK@");
        add_param(&mut substitutions, "@COUNT_TEXT@", function, 0);
    }
    if uses(&usage, M::UnicodeGraphemeAt) {
        append_section(&mut body, source, "/* NACRE_UNICODE_BEGIN UNICODE_GRAPHEME_AT */", "/* NACRE_UNICODE_END UNICODE_GRAPHEME_AT */");
        let function = native_function(program, |method| method == M::UnicodeGraphemeAt);
        add_result(program, &mut substitutions, function, "@AT_RESULT@", "@AT_OK@");
        add_param(&mut substitutions, "@AT_TEXT@", function, 0);
        add_param(&mut substitutions, "@AT_INDEX@", function, 1);
    }
    if uses(&usage, M::UnicodeCompareLocale) {
        append_section(&mut body, source, "/* NACRE_UNICODE_BEGIN UNICODE_COMPARE_LOCALE */", "/* NACRE_UNICODE_END UNICODE_COMPARE_LOCALE */");
        let function = native_function(program, |method| method == M::UnicodeCompareLocale);
        add_result(program, &mut substitutions, function, "@COMPARE_RESULT@", "@COMPARE_OK@");
        add_param(&mut substitutions, "@COMPARE_LEFT@", function, 0);
        add_param(&mut substitutions, "@COMPARE_RIGHT@", function, 1);
        add_param(&mut substitutions, "@COMPARE_LOCALE@", function, 2);
    }

    if uses(&usage, M::CharsetEncode) {
        append_section(&mut body, source, "/* NACRE_UNICODE_BEGIN CHARSET_ENCODE */", "/* NACRE_UNICODE_END CHARSET_ENCODE */");
        let function = native_function(program, |method| method == M::CharsetEncode);
        add_result(program, &mut substitutions, function, "@ENCODE_RESULT@", "@ENCODE_OK@");
        add_param(&mut substitutions, "@ENCODE_TEXT@", function, 0);
        add_param(&mut substitutions, "@ENCODE_CHARSET@", function, 1);
    }
    if uses(&usage, M::CharsetDecode) {
        append_section(&mut body, source, "/* NACRE_UNICODE_BEGIN CHARSET_DECODE */", "/* NACRE_UNICODE_END CHARSET_DECODE */");
        let function = native_function(program, |method| method == M::CharsetDecode);
        add_result(program, &mut substitutions, function, "@DECODE_RESULT@", "@DECODE_OK@");
        add_param(&mut substitutions, "@DECODE_BUFFER@", function, 0);
        add_param(&mut substitutions, "@DECODE_CHARSET@", function, 1);
    }

    if uses(&usage, M::LocaleAvailable) {
        append_section(&mut body, locale_source, "/* NACRE_LOCALE_BEGIN AVAILABLE */", "/* NACRE_LOCALE_END AVAILABLE */");
    }
    if uses(&usage, M::CharsetAvailable) || uses(&usage, M::LocaleAvailable) {
        let function = native_function(program, |method|
            method == M::LocaleAvailable || method == M::CharsetAvailable);
        let Type::Object(list) = function.return_type else { unreachable!("Locale/Charset.available returns List<Text>") };
        let Some(t::NativeClass::List(Type::Object(text))) = program.classes[list].native else {
            unreachable!("Locale/Charset.available specializes List<Text>");
        };
        let list_new = program.functions.iter().find(|function| {
            function.receiver == Some(list) && function.native == Some(M::ListNew)
        }).expect("available List<Text> constructor").id;
        let list_add = program.functions.iter().find(|function| {
            function.receiver == Some(list) && function.native == Some(M::ListAdd)
        }).expect("available List<Text>.add").id;
        substitutions.push(("@LIST_TYPE@", crate::codegen::ctype(program, Type::Object(list))));
        substitutions.push(("@LIST_NEW@", format!("ncr_fn_{list_new}")));
        substitutions.push(("@LIST_ADD@", format!("ncr_fn_{list_add}")));
        substitutions.push(("@LIST_ID@", list.to_string()));
        substitutions.push(("@LIST_DROP@", format!("ncr_drop_{list}")));
        assert!(matches!(program.classes[text].native, Some(t::NativeClass::Text)),
            "Locale/Charset.available specializes List<Text>");
    }

    let uses_decimal_open = uses(&usage, M::LocaleFormatNumber) ||
        uses(&usage, M::LocaleFormatDecimal) || uses(&usage, M::LocaleParseNumber) ||
        uses(&usage, M::LocaleParseDecimal);
    if uses_decimal_open {
        append_section(&mut body, locale_source, "/* NACRE_LOCALE_BEGIN OPEN_DECIMAL */", "/* NACRE_LOCALE_END OPEN_DECIMAL */");
    }
    if uses(&usage, M::LocaleFormatCurrency) || uses(&usage, M::LocaleFormatCurrencyDecimal) {
        append_section(&mut body, locale_source, "/* NACRE_LOCALE_BEGIN CURRENCY */", "/* NACRE_LOCALE_END CURRENCY */");
    }
    if uses(&usage, M::LocaleFormatNumber) || uses(&usage, M::LocaleFormatCurrency) {
        append_section(&mut body, locale_source, "/* NACRE_LOCALE_BEGIN FORMAT_DOUBLE */", "/* NACRE_LOCALE_END FORMAT_DOUBLE */");
    }
    if uses(&usage, M::LocaleFormatDecimal) || uses(&usage, M::LocaleFormatCurrencyDecimal) {
        append_section(&mut body, locale_source, "/* NACRE_LOCALE_BEGIN FORMAT_DECIMAL */", "/* NACRE_LOCALE_END FORMAT_DECIMAL */");
        append_section(&mut body, locale_source, "/* NACRE_LOCALE_BEGIN EXACT_DECIMAL */", "/* NACRE_LOCALE_END EXACT_DECIMAL */");
    }
    if uses(&usage, M::LocaleParseDecimal) {
        append_section(&mut body, locale_source, "/* NACRE_LOCALE_BEGIN DECIMAL_SYNTAX */", "/* NACRE_LOCALE_END DECIMAL_SYNTAX */");
    }
    if uses(&usage, M::LocaleParseDecimal) {
        append_section(&mut body, locale_source, "/* NACRE_LOCALE_BEGIN PARSE_DECIMAL_API */", "/* NACRE_LOCALE_END PARSE_DECIMAL_API */");
        let function = native_function(program, |method| method == M::LocaleParseDecimal);
        add_result(program, &mut substitutions, function, "@PARSE_DECIMAL_RESULT@", "@PARSE_DECIMAL_OK@");
        add_param(&mut substitutions, "@PARSE_DECIMAL_VALUE@", function, 0);
        add_param(&mut substitutions, "@PARSE_DECIMAL_PATTERN@", function, 1);
        add_param(&mut substitutions, "@PARSE_DECIMAL_LOCALE@", function, 2);
    }
    if uses(&usage, M::LocaleFormatCurrencyDecimal) {
        append_section(&mut body, locale_source, "/* NACRE_LOCALE_BEGIN FORMAT_CURRENCY_DECIMAL */", "/* NACRE_LOCALE_END FORMAT_CURRENCY_DECIMAL */");
        let function = native_function(program, |method| method == M::LocaleFormatCurrencyDecimal);
        add_result(program, &mut substitutions, function, "@FORMAT_CURRENCY_DECIMAL_RESULT@", "@FORMAT_CURRENCY_DECIMAL_OK@");
        add_param(&mut substitutions, "@FORMAT_CURRENCY_DECIMAL_VALUE@", function, 0);
        add_param(&mut substitutions, "@FORMAT_CURRENCY_DECIMAL_CODE@", function, 1);
        add_param(&mut substitutions, "@FORMAT_CURRENCY_DECIMAL_LOCALE@", function, 2);
    }
    if uses(&usage, M::LocaleFormatNumber) {
        append_section(&mut body, locale_source, "/* NACRE_LOCALE_BEGIN FORMAT_NUMBER */", "/* NACRE_LOCALE_END FORMAT_NUMBER */");
        let function = native_function(program, |method| method == M::LocaleFormatNumber);
        add_result(program, &mut substitutions, function, "@FORMAT_NUMBER_RESULT@", "@FORMAT_NUMBER_OK@");
        add_param(&mut substitutions, "@FORMAT_NUMBER_VALUE@", function, 0);
        add_param(&mut substitutions, "@FORMAT_NUMBER_PATTERN@", function, 1);
        add_param(&mut substitutions, "@FORMAT_NUMBER_LOCALE@", function, 2);
    }
    if uses(&usage, M::LocaleFormatDecimal) {
        append_section(&mut body, locale_source, "/* NACRE_LOCALE_BEGIN FORMAT_DECIMAL_API */", "/* NACRE_LOCALE_END FORMAT_DECIMAL_API */");
        let function = native_function(program, |method| method == M::LocaleFormatDecimal);
        add_result(program, &mut substitutions, function, "@FORMAT_DECIMAL_RESULT@", "@FORMAT_DECIMAL_OK@");
        add_param(&mut substitutions, "@FORMAT_DECIMAL_VALUE@", function, 0);
        add_param(&mut substitutions, "@FORMAT_DECIMAL_PATTERN@", function, 1);
        add_param(&mut substitutions, "@FORMAT_DECIMAL_LOCALE@", function, 2);
    }
    if uses(&usage, M::LocaleParseNumber) {
        append_section(&mut body, locale_source, "/* NACRE_LOCALE_BEGIN PARSE_NUMBER */", "/* NACRE_LOCALE_END PARSE_NUMBER */");
        let function = native_function(program, |method| method == M::LocaleParseNumber);
        add_result(program, &mut substitutions, function, "@PARSE_NUMBER_RESULT@", "@PARSE_NUMBER_OK@");
        add_param(&mut substitutions, "@PARSE_NUMBER_VALUE@", function, 0);
        add_param(&mut substitutions, "@PARSE_NUMBER_PATTERN@", function, 1);
        add_param(&mut substitutions, "@PARSE_NUMBER_LOCALE@", function, 2);
    }
    if uses(&usage, M::LocaleFormatCurrency) {
        append_section(&mut body, locale_source, "/* NACRE_LOCALE_BEGIN FORMAT_CURRENCY */", "/* NACRE_LOCALE_END FORMAT_CURRENCY */");
        let function = native_function(program, |method| method == M::LocaleFormatCurrency);
        add_result(program, &mut substitutions, function, "@FORMAT_CURRENCY_RESULT@", "@FORMAT_CURRENCY_OK@");
        add_param(&mut substitutions, "@FORMAT_CURRENCY_VALUE@", function, 0);
        add_param(&mut substitutions, "@FORMAT_CURRENCY_CODE@", function, 1);
        add_param(&mut substitutions, "@FORMAT_CURRENCY_LOCALE@", function, 2);
    }

    replace_placeholders(body, &substitutions)
}
