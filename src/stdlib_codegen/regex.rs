use crate::ir::{self as t, NativeClass as C, NativeMethod as M, Type};
use super::support::{
    has_placeholder, native_class_id_if, replace_method_ids, replace_placeholders,
    required_indexed_native_method, result_id, text_class, variant, variant_id,
};

fn method<'a>(
    program: &'a t::Program,
    native_method_ids: &mut Option<t::NativeMethodIndex>,
    receiver: Option<usize>,
    method: M,
) -> &'a t::Function {
    let method_ids = native_method_ids.get_or_insert_with(|| t::NativeMethodIndex::new(program));
    required_indexed_native_method(program, method_ids, receiver, method)
}

pub(super) fn preamble(program: &t::Program) -> String {
    if native_class_id_if(program, |class| matches!(class, C::Regex)).is_none() {
        return String::new();
    }
    "/* nacre-link: pcre2 */\n#define PCRE2_CODE_UNIT_WIDTH 8\n#include <pcre2.h>\n".to_owned()
}

pub(super) fn fields(_program: &t::Program, class: C) -> String {
    match class {
        C::Regex => "    pcre2_code *compiled;\n    pcre2_match_context *context;\n    int32_t group_count;\n".to_owned(),
        C::RegexMatch => "    unsigned char *capture_storage; /* Text pointers, then start and end byte offsets. */\n    int32_t group_count;\n".to_owned(),
        _ => unreachable!("regex field request for a non-regex native class"),
    }
}

pub(super) fn drop_fields(program: &t::Program, class: C) -> String {
    match class {
        C::Regex => "    pcre2_match_context_free(p->context);\n    pcre2_code_free(p->compiled);\n".to_owned(),
        C::RegexMatch => {
            let text = text_class(program).expect("RegexMatch captures own Text");
            format!(
                "    ncr_obj_{text} **groups = (ncr_obj_{text} **)p->capture_storage;\n    for (int32_t index = 0; index < p->group_count; ++index) {{\n        if (groups[index] != NULL) {{ ncr_drop_{text}(groups[index]); }}\n    }}\n    ncr_deallocate(p->capture_storage);\n"
            )
        }
        _ => unreachable!("regex drop request for a non-regex native class"),
    }
}

pub(super) fn implementation(program: &t::Program, native_method_ids: &mut Option<t::NativeMethodIndex>) -> String {
    let mut body = String::new();
    if let Some(regex_class) = program.classes.iter().find_map(|class| {
        matches!(class.native, Some(C::Regex)).then_some(class.id)
    }) {
        body.push_str(&regex_implementation(program, regex_class, native_method_ids));
    }
    if let Some(match_class) = program.classes.iter().find_map(|class| {
        matches!(class.native, Some(C::RegexMatch)).then_some(class.id)
    }) {
        body.push_str(&match_implementation(program, match_class, native_method_ids));
    }
    body
}

fn regex_implementation(program: &t::Program, regex_class: usize, native_method_ids: &mut Option<t::NativeMethodIndex>) -> String {
    let compile = method(program, native_method_ids, None, M::RegexCompile);
    let matches = method(program, native_method_ids, Some(regex_class), M::RegexMatches);
    let search = method(program, native_method_ids, Some(regex_class), M::RegexSearch);
    let compile_result = result_id(compile);
    let matches_result = result_id(matches);
    let search_result = result_id(search);
    let (_, search_ok) = variant(program, search_result, "Ok");
    let [Type::Object(option_class)] = search_ok.fields.as_slice() else {
        unreachable!("Regex.search returns Result<Option<RegexMatch>, Text>");
    };
    let (_, some) = variant(program, *option_class, "Some");
    let [Type::Object(match_class)] = some.fields.as_slice() else {
        unreachable!("Regex.search Option payload is RegexMatch");
    };
    assert!(matches!(program.classes[*match_class].native, Some(C::RegexMatch)));
    let text = text_class(program).expect("Regex requires Text");
    let mut body = replace_placeholders(include_str!("../../stdlib/regex.c.in").to_owned(), &[
        ("@REGEX_CLASS@", regex_class.to_string()),
        ("@REGEX_MATCH_CLASS@", match_class.to_string()),
        ("@TEXT_CLASS@", text.to_string()),
        ("@COMPILE_RESULT@", compile_result.to_string()),
        ("@COMPILE_OK_TAG@", variant_id(program, compile_result, "Ok").to_string()),
        ("@COMPILE_ERR@", format!("ncr_result_error_{compile_result}")),
        ("@MATCHES_RESULT@", matches_result.to_string()),
        ("@MATCHES_OK_TAG@", variant_id(program, matches_result, "Ok").to_string()),
        ("@MATCHES_ERR@", format!("ncr_result_error_{matches_result}")),
        ("@SEARCH_RESULT@", search_result.to_string()),
        ("@SEARCH_OK_TAG@", variant_id(program, search_result, "Ok").to_string()),
        ("@SEARCH_ERR@", format!("ncr_result_error_{search_result}")),
        ("@OPTION_TYPE@", crate::c_repr::ctype(program, Type::Object(*option_class)).into_owned()),
        ("@OPTION_NONE_TAG@", variant_id(program, *option_class, "None").to_string()),
        ("@OPTION_SOME_TAG@", variant_id(program, *option_class, "Some").to_string()),
        ("@TEXT_MAKE@", format!("ncr_text_from_bytes_{text}")),
    ]);
    body = replace_method_ids(body, program, None, native_method_ids);
    debug_assert!(!has_placeholder(&body), "unexpanded Regex placeholder");
    body
}

fn match_implementation(program: &t::Program, match_class: usize, native_method_ids: &mut Option<t::NativeMethodIndex>) -> String {
    let text = text_class(program).expect("RegexMatch group accessors require Text");
    let mut body = replace_placeholders(
        include_str!("../../stdlib/regex_match.c.in").to_owned(),
        &[
            ("@REGEX_MATCH_CLASS@", match_class.to_string()),
            ("@TEXT_CLASS@", text.to_string()),
        ],
    );
    body = replace_method_ids(body, program, Some(match_class), native_method_ids);
    debug_assert!(!has_placeholder(&body), "unexpanded RegexMatch placeholder");
    body
}
