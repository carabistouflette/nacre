use crate::c_repr::ctype;
use crate::ir::{self as t, NativeMethod as M, Type};
use super::{iterators, support::{drop_element, marker, native_method, native_method_if, option_tags, zero}};


pub(super) fn fields(program: &t::Program, element: Type) -> String {
    format!(
        "    {} *items;\n    int32_t head;\n    int32_t length;\n    int32_t capacity;\n",
        ctype(program, element),
    )
}

pub(super) fn drop_fields(element: Type) -> String {
    format!(
        "    for (int32_t logical = 0; logical < p->length; ++logical) {{\n        int32_t until_end = p->capacity - p->head;\n        int32_t physical = logical < until_end ? p->head + logical : logical - until_end;\n        {}\n    }}\n    ncr_deallocate(p->items);\n",
        drop_element(element, "p->items[physical]"),
    )
}

fn borrowed_getter(program: &t::Program, function: &t::Function, element_class: usize) -> String {
    let Type::Borrow { class, .. } = function.return_type else {
        unreachable!("Deque borrowed accessor must return a typed borrow")
    };
    assert_eq!(class, element_class, "Deque borrow must match its element type");
    let borrowed = if program.classes[element_class].is_interface {
        "p->items[physical]".to_owned()
    } else if program.classes[element_class].variants.is_some() {
        "&(p->items[physical])".to_owned()
    } else {
        "p->items[physical]".to_owned()
    };
    include_str!("../../stdlib/deque_get_borrowed.c.in")
        .replace("@RETURN_TYPE@", &ctype(program, function.return_type))
        .replace("@METHOD@", marker(function.native.expect("native Deque accessor")))
        .replace("@INDEX_TYPE@", &ctype(program, function.params[0].1))
        .replace("@BORROWED_ELEMENT@", &borrowed)
}

fn borrowed_getters(program: &t::Program, class_id: usize, element: Type) -> String {
    let Type::Object(element_class) = element else { return String::new(); };
    let mut output = String::new();
    for function in program.functions.iter().filter(|function| {
        function.receiver == Some(class_id)
            && matches!(function.native, Some(M::DequeGetRef | M::DequeGetMut))
    }) {
        output.push_str(&borrowed_getter(program, function, element_class));
    }
    output
}

fn get_method(program: &t::Program, class_id: usize, element: Type) -> String {
    let Some(function) = native_method_if(program, class_id, |method| matches!(method, M::DequeGet)) else {
        return String::new();
    };
    assert!(element.is_copy(), "Deque.get is pruned for affine elements");
    include_str!("../../stdlib/deque_get.c.in")
        .replace("@RETURN_TYPE@", &ctype(program, function.return_type))
        .replace("@METHOD@", marker(function.native.expect("native Deque.get")))
}

fn try_get_method(program: &t::Program, class_id: usize, element: Type) -> String {
    let Some(function) = native_method_if(program, class_id, |method| matches!(method, M::DequeTryGet)) else {
        return String::new();
    };
    assert!(element.is_copy(), "Deque.tryGet is pruned for affine elements");
    let (none_tag, some_tag) = option_tags(program, function, element);
    include_str!("../../stdlib/deque_try_get.c.in")
        .replace("@OPTION_TYPE@", &ctype(program, function.return_type))
        .replace("@DEQUE_TRY_GET@", marker(function.native.expect("native Deque.tryGet")))
        .replace("@INDEX_TYPE@", &ctype(program, function.params[0].1))
        .replace("@NONE_TAG@", &none_tag.to_string())
        .replace("@SOME_TAG@", &some_tag.to_string())
        .replace("@ELEMENT@", &ctype(program, element))
}

fn try_pop_method(program: &t::Program, class_id: usize, element: Type, front: bool) -> String {
    let is_try = |method| if front {
        matches!(method, M::DequeTryPopFront)
    } else {
        matches!(method, M::DequeTryPopBack)
    };
    let is_pop = |method| if front {
        matches!(method, M::DequePopFront)
    } else {
        matches!(method, M::DequePopBack)
    };
    let Some(function) = native_method_if(program, class_id, is_try) else { return String::new(); };
    let pop = native_method(program, class_id, is_pop);
    assert_eq!(pop.return_type, element, "Deque strict pop transfers one element");
    let (none_tag, some_tag) = option_tags(program, function, element);
    include_str!("../../stdlib/deque_try_pop.c.in")
        .replace("@OPTION_TYPE@", &ctype(program, function.return_type))
        .replace("@METHOD@", marker(function.native.expect("native Deque tryPop")))
        .replace("@STRICT_CALL@", &format!("ncr_fn_{}", pop.id))
        .replace("@NONE_TAG@", &none_tag.to_string())
        .replace("@SOME_TAG@", &some_tag.to_string())
}

fn try_pop_methods(program: &t::Program, class_id: usize, element: Type) -> String {
    let mut output = try_pop_method(program, class_id, element, true);
    output.push_str(&try_pop_method(program, class_id, element, false));
    output
}

pub(super) fn implementation(program: &t::Program, class_id: usize, element: Type) -> String {
    let borrowed = borrowed_getters(program, class_id, element);
    let get = get_method(program, class_id, element);
    let try_get = try_get_method(program, class_id, element);
    let try_pops = try_pop_methods(program, class_id, element);
    let mut body = include_str!("../../stdlib/deque.c.in")
        .replace("@ELEMENT@", &ctype(program, element))
        .replace("@ZERO@", &zero(program, element))
        .replace("@DROP_AT_INDEX@", &drop_element(element, "p->items[physical]"))
        .replace("@GET_METHOD@", &get)
        .replace("@BORROWED_GETTERS@", &borrowed)
        .replace("@TRY_GET_METHOD@", &try_get)
        .replace("@TRY_POP_METHODS@", &try_pops);
    body.push_str(&iterators::drain_function(
        program,
        class_id,
        M::CollectionDrain,
        element,
        "iterator->items = p->items;\niterator->head = p->head;\niterator->capacity = p->capacity;\niterator->length = p->length;\np->items = NULL;\np->head = 0;\np->length = 0;\np->capacity = 0;",
    ));
    body
}
