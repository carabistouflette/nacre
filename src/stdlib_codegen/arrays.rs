use crate::codegen::ctype;
use crate::ir::{self as t, NativeMethod as M, Type};
use super::{iterators, support::{marker, result_id, variant}};

pub(super) fn array_fields(program: &t::Program, element: Type) -> String {
    format!("    {} *items;\n    int32_t length;\n", ctype(program, element))
}

pub(super) fn utility_fields() -> String {
    "    unsigned char ncr_static_class;\n".to_owned()
}

fn drop_element(ty: Type, expression: &str) -> String {
    match ty {
        Type::Object(id) => format!("ncr_drop_{id}({expression});"),
        _ => String::new(),
    }
}

pub(super) fn drop_array_fields(element: Type) -> String {
    format!(
        "    while (p->length > 0) {{\n        --p->length;\n        {}\n    }}\n    ncr_deallocate(p->items);\n",
        drop_element(element, "p->items[p->length]")
    )
}

pub(super) fn drop_utility_fields() -> String {
    String::new()
}

fn array_function(program: &t::Program, class_id: usize, wanted: M) -> Option<&t::Function> {
    program.functions.iter().find(|function| {
        function.receiver == Some(class_id) && function.native == Some(wanted)
    })
}

fn borrowed_methods(program: &t::Program, class_id: usize, element: Type) -> String {
    let Type::Object(element_class) = element else { return String::new(); };
    let mut out = String::new();
    for function in program.functions.iter().filter(|function| {
        function.receiver == Some(class_id)
            && matches!(function.native, Some(M::ArrayGetRef | M::ArrayGetMut))
    }) {
        let Type::Borrow { class, .. } = function.return_type else {
            unreachable!("native Array borrowed accessor must return a typed borrow");
        };
        assert_eq!(class, element_class, "Array borrow result must match its element type");
        let index_type = function.params.first().expect("Array accessor index parameter").1;
        let borrowed_element = if program.classes[element_class].is_interface {
            "p->items[index]"
        } else if program.classes[element_class].variants.is_some() {
            "&(p->items[index])"
        } else {
            "p->items[index]"
        };
        out.push_str(
            &include_str!("../../stdlib/array_get_borrowed.c.in")
                .replace("@RETURN_TYPE@", &ctype(program, function.return_type))
                .replace("@INDEX_TYPE@", &ctype(program, index_type))
                .replace("@ARRAY_METHOD@", marker(function.native.expect("Array accessor method")))
                .replace("@BORROWED_ELEMENT@", borrowed_element),
        );
    }
    out
}

fn try_get_method(program: &t::Program, class_id: usize, element: Type) -> String {
    let Some(function) = array_function(program, class_id, M::ArrayTryGet) else {
        return String::new();
    };
    assert!(element.is_copy(), "Array.tryGet must be pruned for affine elements");
    let option = result_id(function);
    let (none_tag, none) = variant(program, option, "None");
    let (some_tag, some) = variant(program, option, "Some");
    assert!(none.fields.is_empty(), "Option None must have no payload");
    assert!(matches!(some.fields.as_slice(), [field] if *field == element),
        "Option Some payload must match its Array element type");
    let index_type = function.params.first().expect("Array.tryGet index parameter").1;
    include_str!("../../stdlib/array_try_get.c.in")
        .replace("@OPTION_TYPE@", &ctype(program, function.return_type))
        .replace("@INDEX_TYPE@", &ctype(program, index_type))
        .replace("@ARRAY_TRY_GET@", marker(M::ArrayTryGet))
        .replace("@NONE_TAG@", &none_tag.to_string())
        .replace("@SOME_TAG@", &some_tag.to_string())
        .replace("@ELEMENT@", &ctype(program, element))
}

pub(super) fn array_implementation(program: &t::Program, class_id: usize, element: Type) -> String {
    let constructor = array_function(program, class_id, M::ArrayNew)
        .expect("compiler-owned Array constructor is present");
    let Type::Object(list_class) = constructor.params.first().expect("Array constructor List parameter").1 else {
        unreachable!("Array constructor consumes a List owner");
    };
    assert!(matches!(program.classes[list_class].native, Some(t::NativeClass::List(list_element)) if list_element == element),
        "Array constructor List element must match the Array element");

    let get = array_function(program, class_id, M::ArrayGet).map_or_else(String::new, |_| {
        include_str!("../../stdlib/array_get.c.in")
            .replace("@ELEMENT@", &ctype(program, element))
            .replace("@ARRAY_GET@", marker(M::ArrayGet))
    });
    let set = array_function(program, class_id, M::ArraySet)
        .expect("compiler-owned Array.set method is present");
    let set_index = set.params.first().expect("Array.set index parameter").1;
    let set_value = set.params.get(1).expect("Array.set value parameter").1;
    assert_eq!(set_index, Type::Int, "Array.set index must use int32");
    assert_eq!(set_value, element, "Array.set value must match its element type");

    let mut body = include_str!("../../stdlib/array.c.in")
        .replace("@ARRAY_TYPE@", &ctype(program, Type::Object(class_id)))
        .replace("@LIST_TYPE@", &ctype(program, Type::Object(list_class)))
        .replace("@ELEMENT@", &ctype(program, element))
        .replace("@ARRAY_NEW@", marker(M::ArrayNew))
        .replace("@ARRAY_SIZE@", marker(M::ArraySize))
        .replace("@ARRAY_GET_METHOD@", &get)
        .replace("@BORROWED_GETTERS@", &borrowed_methods(program, class_id, element))
        .replace("@TRY_GET_METHOD@", &try_get_method(program, class_id, element))
        .replace("@ARRAY_SET@", marker(M::ArraySet))
        .replace("@DROP_AT_INDEX@", &drop_element(element, "p->items[index]"));
    body.push_str(&iterators::drain_function(
        program,
        class_id,
        M::CollectionDrain,
        element,
        "iterator->items = p->items;\niterator->length = p->length;\np->items = NULL;\np->length = 0;",
    ));
    body
}

pub(super) fn zeroed_implementations(program: &t::Program) -> String {
    let factories = [
        M::ArraysBytes,
        M::ArraysChars,
        M::ArraysInts,
        M::ArraysLongs,
        M::ArraysDoubles,
        M::ArraysBooleans,
        M::ArraysUnits,
    ];
    let mut out = String::new();
    for method in factories {
        let Some(function) = program.functions.iter().find(|function| function.native == Some(method)) else {
            continue;
        };
        let Type::Object(array_class) = function.return_type else {
            unreachable!("Arrays zeroed factory returns an owned Array");
        };
        let Some(t::NativeClass::Array(element)) = program.classes[array_class].native else {
            unreachable!("Arrays factory specializes a native Array");
        };
        assert!(element.is_copy(), "zeroed Array factory cannot initialize affine elements");
        let (length_id, length_type) = function.params.first().expect("Arrays length parameter");
        assert_eq!(*length_type, Type::Int, "ordinary Array length is int32");
        out.push_str(
            &include_str!("../../stdlib/array_zeroed.c.in")
                .replace("@ARRAY_TYPE@", &ctype(program, function.return_type))
                .replace("@ELEMENT@", &ctype(program, element))
                .replace("@LENGTH_ID@", &length_id.to_string())
                .replace("@ARRAYS_FACTORY@", marker(method))
        );
        out.push('\n');
    }
    out
}
