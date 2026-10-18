use std::borrow::Cow;
use crate::c_repr::ctype;
use crate::ir::{self as t, NativeMethod as M, Type};
use super::support::{native_method_if, replace_method_ids, replace_placeholders, result_id, variant_id, zero};


fn default_compare(class_id: usize, element: Type, element_c: &str) -> String {
    let body = match element {
        Type::Byte | Type::Char | Type::Int | Type::Long =>
            "    return left < right ? -1 : (left > right ? 1 : 0);\n".to_owned(),
        Type::Boolean =>
            "    return left == right ? 0 : (left ? 1 : -1);\n".to_owned(),
        Type::Double => concat!(
            "    uint64_t left_bits;\n",
            "    uint64_t right_bits;\n",
            "    memcpy(&left_bits, &left, sizeof(left_bits));\n",
            "    memcpy(&right_bits, &right, sizeof(right_bits));\n",
            "    uint64_t sign = UINT64_C(0x8000000000000000);\n",
            "    uint64_t left_order = (left_bits & sign) != 0 ? ~left_bits : (left_bits | sign);\n",
            "    uint64_t right_order = (right_bits & sign) != 0 ? ~right_bits : (right_bits | sign);\n",
            "    return left_order < right_order ? -1 : (left_order > right_order ? 1 : 0);\n",
        ).to_owned(),
        Type::String => concat!(
            "    size_t common = left.length < right.length ? left.length : right.length;\n",
            "    int compared = common == 0 ? 0 : memcmp(left.data, right.data, common);\n",
            "    if (compared < 0) { return -1; }\n",
            "    if (compared > 0) { return 1; }\n",
            "    return left.length < right.length ? -1 : (left.length > right.length ? 1 : 0);\n",
        ).to_owned(),
        _ => unreachable!("only the specialized default ordering types reach code generation"),
    };
    format!(
        "static int32_t ncr_list_compare_default_{class_id}({element_c} left, {element_c} right) {{\n{body}}}\n",
    )
}

pub(super) fn borrow_storage(program: &t::Program, element: Type, name: &str) -> String {
    let Type::Object(class) = element else { return name.to_owned(); };
    if program.classes[class].variants.is_some() {
        format!("&({name})")
    } else {
        name.to_owned()
    }
}

pub(super) fn comparator_info(program: &t::Program, function: &t::Function) -> (usize, usize, Type) {
    let Some(Type::Object(interface)) = function.params.first().map(|(_, ty)| *ty) else {
        unreachable!("sortBy and binarySearchBy take an owned Comparator<T> interface")
    };
    assert!(program.classes[interface].is_interface, "Comparator<T> is a narrow interface");
    let compare_id = *program.classes[interface].interface_methods.iter()
        .find(|id| program.functions[**id].name.ends_with(".compare"))
        .expect("Comparator<T> has one compare method");
    let compare = &program.functions[compare_id];
    assert_eq!(compare.return_type, Type::Int, "Comparator.compare returns int");
    assert_eq!(compare.params.len(), 2, "Comparator.compare compares exactly two values");
    (interface, compare.virtual_slot.expect("Comparator.compare has a vtable slot"), compare.params[0].1)
}

struct HeapComparison<'a> {
    element: &'a str,
    reference: &'a str,
    expression: &'a str,
}

fn heapsort_core(
    class_id: usize,
    element_c: &str,
    kind: &str,
    comparator_param: &str,
    comparator_pass: &str,
    comparison: HeapComparison<'_>,
    swap_body: &str,
) -> String {
    let swap_body = replace_placeholders(
        swap_body.to_owned(),
        &[("@ELEMENT@", Cow::Borrowed(element_c))],
    );
    let substitutions = [
        ("@SWAP_BODY@", Cow::Owned(swap_body)),
        ("@ID@", Cow::Owned(class_id.to_string())),
        ("@KIND@", Cow::Borrowed(kind)),
        ("@ELEMENT@", Cow::Borrowed(element_c)),
        ("@CONTEXT_PARAM@", Cow::Borrowed(comparator_param)),
        ("@CONTEXT_PASS@", Cow::Borrowed(comparator_pass)),
        ("@COMPARE_ELEMENT@", Cow::Borrowed(comparison.element)),
        ("@COMPARE_REFERENCE@", Cow::Borrowed(comparison.reference)),
        ("@COMPARE_EXPRESSION@", Cow::Borrowed(comparison.expression)),
    ];
    replace_placeholders(
        include_str!("../../stdlib/list_heapsort.c.in").to_owned(),
        &substitutions,
    )
}

fn default_sort_method(program: &t::Program, class_id: usize) -> String {
    let Some(function) = native_method_if(program, class_id, |method| matches!(method, M::ListSort)) else {
        return String::new();
    };
    format!(
        "static void ncr_fn_{}(ncr_obj_{} *p, size_t line, size_t column) {{\n    (void)line; (void)column;\n    ncr_list_sort_default_{}(p);\n}}\n",
        function.id, class_id, class_id,
    )
}

fn sort_by_method(program: &t::Program, class_id: usize) -> String {
    let Some(function) = native_method_if(program, class_id, |method| matches!(method, M::ListSortBy)) else {
        return String::new();
    };
    let (comparator, _, _) = comparator_info(program, function);
    format!(
        "static void ncr_fn_{}(ncr_obj_{} *p, ncr_obj_{} comparator, size_t line, size_t column) {{\n    (void)line; (void)column;\n    ncr_list_sort_by_{}(p, &comparator);\n    ncr_drop_{}(comparator);\n}}\n",
        function.id, class_id, comparator, class_id, comparator,
    )
}

fn default_search(program: &t::Program, class_id: usize, element: Type) -> String {
    let Some(function) = native_method_if(program, class_id, |method| matches!(method, M::ListBinarySearch)) else {
        return String::new();
    };
    assert_eq!(function.params.first().expect("binarySearch query").1, element);
    let query = ctype(program, element);
    let option = ctype(program, function.return_type);
    let option_class = result_id(function);
    let none = variant_id(program, option_class, "None");
    let some = variant_id(program, option_class, "Some");
    format!(
        "static {option} ncr_fn_{function_id}(ncr_obj_{class_id} *p, {query} value, size_t line, size_t column) {{\n    (void)line; (void)column;\n    int32_t low = 0;\n    int32_t high = p->length;\n    while (low < high) {{\n        int32_t middle = low + (high - low) / 2;\n        int32_t order = ncr_list_compare_default_{class_id}(p->items[middle], value);\n        if (order < 0) {{ low = middle + 1; }} else {{ high = middle; }}\n    }}\n    if (low < p->length && ncr_list_compare_default_{class_id}(p->items[low], value) == 0) {{\n        return ({option}){{ .tag = {some}, .data.v{some}.f0 = low }};\n    }}\n    return ({option}){{ .tag = {none} }};\n}}\n",
        function_id = function.id,
    )
}

fn binary_search_by(program: &t::Program, class_id: usize, element: Type) -> String {
    let Some(function) = native_method_if(program, class_id, |method| matches!(method, M::ListBinarySearchBy)) else {
        return String::new();
    };
    let (comparator, slot, comparison_type) = comparator_info(program, function);
    let query_type = function.params.get(1).expect("binarySearchBy query").1;
    let query_c = ctype(program, query_type);
    let element_c = ctype(program, comparison_type);
    let option_c = ctype(program, function.return_type);
    let option_class = result_id(function);
    let none = variant_id(program, option_class, "None");
    let some = variant_id(program, option_class, "Some");
    let left = "left";
    let compare_call = format!(
        "comparator->vtable->m{slot}(comparator->data, {left}, right)",
    );
    let helper = format!(
        "static int32_t ncr_list_compare_search_{class_id}(const ncr_obj_{comparator} *comparator, {element_c} left, {query_c} right) {{\n    return {compare_call};\n}}\n",
    );
    let stored = borrow_storage(program, element, "p->items[middle]");
    let candidate = borrow_storage(program, element, "p->items[low]");
    let definition = format!(
        "static {option_c} ncr_fn_{function_id}(ncr_obj_{class_id} *p, ncr_obj_{comparator} comparator, {query_c} value, size_t line, size_t column) {{\n    (void)line; (void)column;\n    int32_t low = 0;\n    int32_t high = p->length;\n    while (low < high) {{\n        int32_t middle = low + (high - low) / 2;\n        int32_t order = ncr_list_compare_search_{class_id}(&comparator, {stored}, value);\n        if (order < 0) {{ low = middle + 1; }} else {{ high = middle; }}\n    }}\n    {option_c} result;\n    if (low < p->length && ncr_list_compare_search_{class_id}(&comparator, {candidate}, value) == 0) {{\n        result = ({option_c}){{ .tag = {some}, .data.v{some}.f0 = low }};\n    }} else {{\n        result = ({option_c}){{ .tag = {none} }};\n    }}\n    ncr_drop_{comparator}(comparator);\n    return result;\n}}\n",
        function_id = function.id,
    );
    helper + &definition
}

fn comparator_sort_helpers(program: &t::Program, class_id: usize, element: Type) -> String {
    let sort = native_method_if(program, class_id, |method| matches!(method, M::ListSortBy));
    let search = native_method_if(program, class_id, |method| matches!(method, M::ListBinarySearchBy));
    let source = sort.or(search).expect("comparator methods retained on List specialization");
    let (comparator, slot, comparison_type) = comparator_info(program, source);
    let element_c = ctype(program, element);
    let comparison_type = ctype(program, comparison_type);
    let reference = if matches!(element, Type::Object(id) if program.classes[id].variants.is_some()) { "&" } else { "" };
    let call = format!("comparator->vtable->m{slot}(comparator->data, left, right)");
    let helper = format!(
        "static int32_t ncr_list_compare_by_{class_id}(const ncr_obj_{comparator} *comparator, {comparison_type} left, {comparison_type} right) {{\n    return {call};\n}}\n",
    );
    let swap_body = if element.is_copy() {
        "    @ELEMENT@ value = *left;\n    *left = *right;\n    *right = value;\n".to_owned()
    } else {
        replace_placeholders(
            "    @ELEMENT@ value = *left;\n    *left = @ZERO@;\n    *left = *right;\n    *right = @ZERO@;\n    *right = value;\n    value = @ZERO@;\n".to_owned(),
            &[("@ZERO@", Cow::Owned(zero(program, element)))],
        )
    };
    let heap = heapsort_core(
        class_id,
        &element_c,
        "by",
        &format!(", const ncr_obj_{comparator} *comparator"),
        ", comparator",
        HeapComparison {
            element: &comparison_type,
            reference,
            expression: &format!("ncr_list_compare_by_{class_id}(comparator, left, right)"),
        },
        &swap_body,
    );
    helper + &heap
}

pub(super) fn list_implementation(program: &t::Program, class_id: usize, element: Type) -> String {
    let default_sort = native_method_if(program, class_id, |method| matches!(method, M::ListSort)).is_some();
    let has_default_search = native_method_if(program, class_id, |method| matches!(method, M::ListBinarySearch)).is_some();
    let sort_by = native_method_if(program, class_id, |method| matches!(method, M::ListSortBy)).is_some();
    let search_by = native_method_if(program, class_id, |method| matches!(method, M::ListBinarySearchBy)).is_some();
    let mut output = String::new();
    if default_sort || has_default_search {
        let element_c = ctype(program, element);
        output.push_str(&default_compare(class_id, element, &element_c));
        if default_sort {
            output.push_str(&heapsort_core(
                class_id,
                &element_c,
                "default",
                "",
                "",
                HeapComparison {
                    element: &element_c,
                    reference: "",
                    expression: &format!("ncr_list_compare_default_{class_id}(left, right)"),
                },
                "    @ELEMENT@ value = *left;\n    *left = *right;\n    *right = value;\n",
            ));
            output.push_str(&default_sort_method(program, class_id));
        }
        if has_default_search { output.push_str(&default_search(program, class_id, element)); }
    }
    if sort_by || search_by {
        output.push_str(&comparator_sort_helpers(program, class_id, element));
        if sort_by { output.push_str(&sort_by_method(program, class_id)); }
        if search_by { output.push_str(&binary_search_by(program, class_id, element)); }
    }
    output
}
