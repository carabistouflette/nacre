use crate::codegen::ctype;
use crate::ir::{self as t, NativeClass as C, NativeMethod as M, Type};
use super::{
    collections, iterators, ordering, ordered,
    support::{marker, replace_method_ids, replace_placeholders, result_id, variant},
};

pub(super) fn fields(program: &t::Program, class: C, _class_id: usize) -> String {
    let C::PriorityQueue(element) = class else {
        unreachable!("PriorityQueue fields called for another native class")
    };
    format!(
        "    {} *items;\n    int32_t length;\n    int32_t capacity;\n",
        ctype(program, element),
    )
}

pub(super) fn drop_fields(class: C) -> String {
    let C::PriorityQueue(element) = class else {
        unreachable!("PriorityQueue drops called for another native class")
    };
    format!(
        "    while (p->length > 0) {{\n        --p->length;\n        {}\n    }}\n    ncr_deallocate(p->items);\n",
        collections::drop_element(element, "p->items[p->length]"),
    )
}

fn native_method(program: &t::Program, receiver: usize, method: M) -> Option<&t::Function> {
    program.functions.iter().find(|function| {
        function.receiver == Some(receiver) && function.native == Some(method)
    })
}

fn required_method(program: &t::Program, receiver: usize, method: M) -> &t::Function {
    native_method(program, receiver, method).expect("compiler-owned PriorityQueue method is present")
}

fn option_tags(program: &t::Program, function: &t::Function, element: Type) -> (usize, usize) {
    let option = result_id(function);
    let (none_tag, none) = variant(program, option, "None");
    let (some_tag, some) = variant(program, option, "Some");
    assert!(none.fields.is_empty(), "Option None has no payload");
    assert!(matches!(some.fields.as_slice(), [field] if *field == element),
        "Option Some payload matches PriorityQueue<T>");
    (none_tag, some_tag)
}


fn peek_method(program: &t::Program, class_id: usize, element: Type) -> String {
    let Some(function) = native_method(program, class_id, M::PriorityQueuePeek) else {
        return String::new();
    };
    assert!(element.is_copy(), "PriorityQueue.peek is specialized for copied values");
    format!(
        "static {} {}(ncr_obj_{} *p, size_t line, size_t column) {{\n    if (p->length == 0) {{ ncr_fail(\"peek from empty PriorityQueue\", line, column); }}\n    return p->items[0];\n}}\n",
        ctype(program, function.return_type), marker(M::PriorityQueuePeek), class_id,
    )
}

fn borrowed_getter(program: &t::Program, class_id: usize, element: Type) -> String {
    let Some(function) = native_method(program, class_id, M::PriorityQueueGetRef) else {
        return String::new();
    };
    let Type::Borrow { class, .. } = function.return_type else {
        unreachable!("PriorityQueue.getRef returns a borrowed element");
    };
    assert_eq!(class, element.object_class().expect("affine queue element class"));
    let value = ordering::borrow_storage(program, element, "p->items[0]");
    format!(
        "static {} {}(ncr_obj_{} *p, size_t line, size_t column) {{\n    if (p->length == 0) {{ ncr_fail(\"peek from empty PriorityQueue\", line, column); }}\n    return {};\n}}\n",
        ctype(program, function.return_type), marker(M::PriorityQueueGetRef), class_id, value,
    )
}

fn try_peek_method(program: &t::Program, class_id: usize, element: Type) -> String {
    let Some(function) = native_method(program, class_id, M::PriorityQueueTryPeek) else {
        return String::new();
    };
    assert!(element.is_copy(), "PriorityQueue.tryPeek is specialized for copied values");
    let (none_tag, some_tag) = option_tags(program, function, element);
    let result = ctype(program, function.return_type);
    format!(
        "static {result} {}(ncr_obj_{class_id} *p, size_t line, size_t column) {{\n    (void)line; (void)column;\n    if (p->length == 0) {{ return ({result}){{ .tag = {none_tag} }}; }}\n    return ({result}){{ .tag = {some_tag}, .data.v{some_tag}.f0 = p->items[0] }};\n}}\n",
        marker(M::PriorityQueueTryPeek),
    )
}
fn value_at_method(program: &t::Program, class_id: usize, element: Type) -> String {
    let Some(function) = native_method(program, class_id, M::PriorityQueueValueAt) else {
        return String::new();
    };
    let index = function.params.first().expect("PriorityQueue.valueAt index parameter").1;
    assert_eq!(index, Type::Int, "PriorityQueue.valueAt index is int32");
    let value = ordering::borrow_storage(program, element, "p->items[index]");
    format!(
        "static {} {}(ncr_obj_{class_id} *p, int32_t index, size_t line, size_t column) {{\n    if (index < 0 || index >= p->length) {{ ncr_fail(\"PriorityQueue index out of bounds\", line, column); }}\n    return {};\n}}\n",
        ctype(program, function.return_type), marker(M::PriorityQueueValueAt), value,
    )
}


pub(super) fn implementation(program: &t::Program, class_id: usize, element: Type) -> String {
    let constructor = required_method(program, class_id, M::PriorityQueueNew);
    let (comparator, compare_slot, compare_type) = ordering::comparator_info(program, constructor);
    assert_eq!(program.classes[class_id].fields.first(), Some(&Type::Object(comparator)),
        "PriorityQueue stores its owned Comparator<T> interface");
    let expected_compare_type = match element {
        Type::Object(class) => Type::Borrow { class, mutable: false },
        other => other,
    };
    assert_eq!(compare_type, expected_compare_type,
        "Comparator<T> borrows affine values and copies scalar values");
    let add = required_method(program, class_id, M::PriorityQueueAdd);
    assert_eq!(add.params.first().map(|(_, ty)| *ty), Some(element));
    let pop = required_method(program, class_id, M::PriorityQueuePop);
    assert_eq!(pop.return_type, element, "PriorityQueue.pop transfers an element");
    let try_pop = required_method(program, class_id, M::PriorityQueueTryPop);
    let (none_tag, some_tag) = option_tags(program, try_pop, element);
    let item = ordering::borrow_storage(program, element, "p->items[index]");
    let parent = ordering::borrow_storage(program, element, "p->items[parent]");
    let right = ordering::borrow_storage(program, element, "p->items[child + 1]");
    let left = ordering::borrow_storage(program, element, "p->items[child]");
    let child = ordering::borrow_storage(program, element, "p->items[child]");
    let parent_at_index = ordering::borrow_storage(program, element, "p->items[index]");
    let mut body = include_str!("../../stdlib/priority_queue.c.in").to_owned();
    let substitutions = [
        ("@COMPARE_TYPE@", ctype(program, compare_type)),
        ("@COMPARE_SLOT@", compare_slot.to_string()),
        ("@COMPARATOR_ID@", comparator.to_string()),
        ("@COMPARATOR_FIELD@", ordered::comparator_field_name()),
        ("@ELEMENT@", ctype(program, element)),
        ("@ZERO@", collections::zero(program, element)),
        ("@RIGHT_CHILD_KEY@", right),
        ("@LEFT_CHILD_KEY@", left),
        ("@CHILD_KEY@", child),
        ("@PARENT_KEY@", parent_at_index),
        ("@ITEM_KEY@", item),
        ("@PARENT_KEY_FOR_ADD@", parent),
        ("@OPTION_TYPE@", ctype(program, try_pop.return_type)),
        ("@NONE_TAG@", none_tag.to_string()),
        ("@SOME_TAG@", some_tag.to_string()),
        ("@PEEK_METHOD@", &peek_method(program, class_id, element)),
        ("@GET_REF_METHOD@", &borrowed_getter(program, class_id, element)),
        ("@TRY_PEEK_METHOD@", &try_peek_method(program, class_id, element)),
        ("@VALUE_AT_METHOD@", &value_at_method(program, class_id, element)),
        ("@DROP_AT_INDEX@", collections::drop_element(element, "p->items[index]")),
    ];
    body = replace_placeholders(body, &substitutions);
    body = replace_method_ids(body, program, Some(class_id));
    body.push_str(&iterators::drain_function(
        program,
        class_id,
        M::PriorityQueueDrain,
        element,
        "iterator->items = p->items;\niterator->length = p->length;\np->items = NULL;\np->length = 0;\np->capacity = 0;",
    ));
    body
}
