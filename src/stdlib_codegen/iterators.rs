use crate::codegen::ctype;
use crate::ir::{self as t, NativeClass as C, NativeMethod as M, Type};
use super::{ordering, support::{drop_element, option_tags, zero}};

fn class_method(program: &t::Program, class_id: usize, method: M) -> Option<&t::Function> {
    let class_prefix = format!("{}.", program.classes[class_id].name);
    program.functions.iter().find(|function| {
        function.native == Some(method)
            && (function.receiver == Some(class_id) || function.name.starts_with(&class_prefix))
    })
}

fn required_method(program: &t::Program, class_id: usize, method: M) -> &t::Function {
    class_method(program, class_id, method).expect("compiler-owned iterator method is present")
}

fn method_slot(program: &t::Program, interface: usize, suffix: &str) -> usize {
    let methods: Vec<_> = program.classes[interface].interface_methods.iter()
        .map(|id| &program.functions[*id])
        .filter(|method| method.name.ends_with(suffix))
        .collect();
    assert_eq!(methods.len(), 1, "iterator callback interface declares one {suffix} method");
    methods[0].virtual_slot.expect("interface method has a vtable slot")
}

fn implemented_iterator(program: &t::Program, concrete: usize, element: Type) -> usize {
    let interfaces = program.classes[concrete].implementations.iter()
        .map(|(interface, _)| *interface).collect::<Vec<_>>();
    assert_eq!(interfaces.len(), 1, "iterator adapter implements one Iterator<T>");
    let interface = interfaces[0];
    assert!(program.classes[interface].is_interface, "Iterator<T> is an interface");
    let next = program.classes[interface].interface_methods.iter()
        .map(|id| &program.functions[*id])
        .find(|method| method.name.ends_with(".next"))
        .expect("Iterator<T> declares next");
    option_tags(program, next, element);
    interface
}

fn concrete_class(program: &t::Program, native: C) -> usize {
    program.classes.iter().find(|class| {
        matches!(
            (class.native, native),
            (Some(C::ListIterator(left)), C::ListIterator(right)) if left == right
        ) || matches!(
            (class.native, native),
            (Some(C::TakeIterator(left)), C::TakeIterator(right)) if left == right
        ) || matches!(
            (class.native, native),
            (Some(C::SkipIterator(left)), C::SkipIterator(right)) if left == right
        )
    }).map(|class| class.id).expect("specialized iterator adapter is registered")
}

fn vtable(program: &t::Program, concrete: usize, interface: usize) -> String {
    assert!(program.classes[concrete].implementations.iter()
        .any(|(id, _)| *id == interface), "iterator adapter implements its returned interface");
    format!("&ncr_vtable_{concrete}_{interface}")
}

pub(super) fn fields(program: &t::Program, class: C, _class_id: usize) -> String {
    match class {
        C::ListIterator(element) => format!(
            "    {} *items;\n    void *state;\n    int32_t length;\n    int32_t cursor;\n    int32_t head;\n    int32_t capacity;\n    void (*ncr_next_fn)(void *, int32_t *, int32_t, {} *, bool *);\n    void (*ncr_drop_fn)(void *, int32_t, int32_t);\n",
            ctype(program, element), ctype(program, element),
        ),
        C::MapIterator(_, _) | C::FilterIterator(_) => String::new(),
        C::TakeIterator(_) | C::SkipIterator(_) => "    int64_t remaining;\n".to_owned(),
        C::Iterators(_) => "    unsigned char ncr_empty;\n".to_owned(),
        _ => unreachable!("iterator fields called for another native class"),
    }
}

pub(super) fn drop_fields(class: C) -> String {
    match class {
        C::ListIterator(element) => format!(
            "    if (p->ncr_drop_fn != NULL) {{\n        p->ncr_drop_fn(p->state, p->cursor, p->length);\n    }} else {{\n        for (int32_t index = p->cursor; index < p->length; ++index) {{\n            int32_t physical = index;\n            if (p->capacity > 0) {{\n                int32_t until_end = p->capacity - p->head;\n                physical = index < until_end ? p->head + index : index - until_end;\n            }}\n            {}\n        }}\n        ncr_deallocate(p->items);\n    }}\n",
            drop_element(element, "p->items[physical]"),
        ),
        C::MapIterator(_, _) | C::FilterIterator(_) | C::TakeIterator(_) |
        C::SkipIterator(_) | C::Iterators(_) => String::new(),
        _ => unreachable!("iterator drops called for another native class"),
    }
}

pub(super) fn drain_function(
    program: &t::Program,
    source_class: usize,
    method: M,
    element: Type,
    setup: &str,
) -> String {
    let Some(drain) = class_method(program, source_class, method) else {
        return String::new();
    };
    let Type::Object(interface) = drain.return_type else {
        unreachable!("container drain returns Iterator<T>");
    };
    let iterator = concrete_class(program, C::ListIterator(element));
    assert_eq!(implemented_iterator(program, iterator, element), interface,
        "ListIterator<T> implements the drained Iterator<T>");
    format!(
        "static {} ncr_fn_{}(ncr_obj_{} *p, size_t line, size_t column) {{\n    ncr_obj_{} *iterator = ncr_allocate(sizeof(ncr_obj_{}), line, column);\n    iterator->items = NULL;\n    iterator->state = NULL;\n    iterator->length = 0;\n    iterator->cursor = 0;\n    iterator->head = 0;\n    iterator->capacity = 0;\n    iterator->ncr_next_fn = NULL;\n    iterator->ncr_drop_fn = NULL;\n{}\n    return ({}){{ .data = iterator, .vtable = {} }};\n}}\n",
        ctype(program, drain.return_type), drain.id, source_class,
        iterator, iterator, indent_setup(setup),
        ctype(program, drain.return_type), vtable(program, iterator, interface),
    )
}

fn indent_setup(setup: &str) -> String {
    setup.lines().map(|line| format!("    {line}\n")).collect()
}
pub(super) fn callback_names(method_id: usize) -> (String, String) {
    (format!("ncr_drain_next_{method_id}"), format!("ncr_drain_drop_{method_id}"))
}
pub(super) fn drain_callbacks(
    program: &t::Program,
    method_id: usize,
    element: Type,
    next_body: &str,
    drop_body: &str,
) -> String {
    let (next, drop) = callback_names(method_id);
    format!(
        "static void {next}(void *raw, int32_t *cursor, int32_t length, {} *value, bool *available) {{\n{next_body}\n}}\nstatic void {drop}(void *raw, int32_t cursor, int32_t length) {{\n{drop_body}\n}}\n",
        ctype(program, element),
    )
}

pub(super) fn list_implementation(program: &t::Program, class_id: usize, element: Type) -> String {
    let mut output = String::new();
    if class_method(program, class_id, M::ListDrain).is_some() {
        output.push_str(&drain_function(
            program,
            class_id,
            M::ListDrain,
            element,
            "iterator->items = p->items;\niterator->length = p->length;\np->items = NULL;\np->length = 0;\np->capacity = 0;",
        ));
    }

    if let Some(collect) = class_method(program, class_id, M::ListCollect) {
        let input_type = collect.params.first().expect("List.collect input iterator").1;
        let Type::Object(interface) = input_type else {
            unreachable!("List.collect consumes an Iterator<T> interface")
        };
        let next_slot = method_slot(program, interface, ".next");
        let next = program.classes[interface].interface_methods.iter()
            .map(|id| &program.functions[*id])
            .find(|method| method.name.ends_with(".next"))
            .expect("Iterator<T> declares next");
        let (none_tag, some_tag) = option_tags(program, next, element);
        let option = ctype(program, next.return_type);
        let list_new = required_method(program, class_id, M::ListNew);
        let list_add = required_method(program, class_id, M::ListAdd);
        output.push_str(&format!(
            "static {} ncr_fn_{}({} input, size_t line, size_t column) {{\n    ncr_obj_{} *result = ncr_allocate(sizeof(ncr_obj_{}), line, column);\n    ncr_fn_{}(result, line, column);\n    while (true) {{\n        {} item = input.vtable->m{}(input.data);\n        if (item.tag == {}) {{ break; }}\n        {} value = item.data.v{}.f0;\n        ncr_fn_{}(result, value, line, column);\n    }}\n    ncr_drop_{}(input);\n    return result;\n}}\n",
            ctype(program, collect.return_type), collect.id, ctype(program, input_type),
            class_id, class_id, list_new.id, option, next_slot, none_tag,
            ctype(program, element), some_tag, list_add.id, interface,
        ));
    }
    output
}

fn list_iterator_next(program: &t::Program, class_id: usize, element: Type) -> String {
    let function = required_method(program, class_id, M::ListIteratorNext);
    let (none_tag, some_tag) = option_tags(program, function, element);
    format!(
        "static {} ncr_fn_{}(ncr_obj_{} *p, size_t line, size_t column) {{\n    (void)line; (void)column;\n    if (p->ncr_next_fn != NULL) {{\n        {} value = {};\n        bool available = false;\n        p->ncr_next_fn(p->state, &p->cursor, p->length, &value, &available);\n        if (!available) {{ return ({}){{ .tag = {} }}; }}\n        return ({}){{ .tag = {}, .data.v{}.f0 = value }};\n    }}\n    if (p->cursor >= p->length) {{ return ({}){{ .tag = {} }}; }}\n    int32_t index = p->cursor++;\n    int32_t physical = index;\n    if (p->capacity > 0) {{\n        int32_t until_end = p->capacity - p->head;\n        physical = index < until_end ? p->head + index : index - until_end;\n    }}\n    {} value = p->items[physical];\n    p->items[physical] = {};\n    return ({}){{ .tag = {}, .data.v{}.f0 = value }};\n}}\n",
        ctype(program, function.return_type), function.id, class_id,
        ctype(program, element), zero(program, element),
        ctype(program, function.return_type), none_tag,
        ctype(program, function.return_type), some_tag, some_tag,
        ctype(program, function.return_type), none_tag,
        ctype(program, element), zero(program, element),
        ctype(program, function.return_type), some_tag, some_tag,
    )
}

fn map_iterator(program: &t::Program, class_id: usize, input: Type, output: Type) -> String {
    let constructor = required_method(program, class_id, M::MapIteratorNew);
    let input_interface = constructor.params[0].1.object_class().expect("MapIterator input interface");
    let mapper_interface = constructor.params[1].1.object_class().expect("MapIterator mapper interface");
    assert_eq!(program.classes[class_id].fields.first(), Some(&constructor.params[0].1),
        "MapIterator's first declared field owns its input");
    assert_eq!(program.classes[class_id].fields.get(1), Some(&constructor.params[1].1),
        "MapIterator's second declared field owns its mapper");
    let next = required_method(program, class_id, M::MapIteratorNext);
    let (none_tag, some_tag) = option_tags(program, next, output);
    let input_next = program.classes[input_interface].interface_methods.iter()
        .map(|id| &program.functions[*id]).find(|method| method.name.ends_with(".next"))
        .expect("Iterator<T> declares next");
    let (input_none, input_some) = option_tags(program, input_next, input);
    let apply_slot = method_slot(program, mapper_interface, ".apply");
    format!(
        "static void ncr_fn_{constructor_id}(ncr_obj_{class_id} *p, {input_c} input, {mapper_c} mapper, size_t line, size_t column) {{\n    (void)line; (void)column;\n    p->ncr_f_0 = input;\n    p->ncr_f_1 = mapper;\n}}\nstatic {next_option} ncr_fn_{next_id}(ncr_obj_{class_id} *p, size_t line, size_t column) {{\n    (void)line; (void)column;\n    {input_option} item = p->ncr_f_0.vtable->m{next_slot}(p->ncr_f_0.data);\n    if (item.tag == {input_none}) {{ return ({next_option}){{ .tag = {none_tag} }}; }}\n    {input_element_c} value = item.data.v{input_some}.f0;\n    {output_c} mapped = p->ncr_f_1.vtable->m{apply_slot}(p->ncr_f_1.data, value);\n    return ({next_option}){{ .tag = {some_tag}, .data.v{some_tag}.f0 = mapped }};\n}}\n",
        constructor_id = constructor.id,
        input_c = ctype(program, constructor.params[0].1),
        input_element_c = ctype(program, input),
        mapper_c = ctype(program, constructor.params[1].1),
        next_option = ctype(program, next.return_type),
        next_id = next.id,
        input_option = ctype(program, input_next.return_type),
        next_slot = method_slot(program, input_interface, ".next"),
        input_none = input_none,
        none_tag = none_tag,
        input_some = input_some,
        output_c = ctype(program, output),
        apply_slot = apply_slot,
        some_tag = some_tag,
    )
}

fn filter_iterator(program: &t::Program, class_id: usize, element: Type) -> String {
    let constructor = required_method(program, class_id, M::FilterIteratorNew);
    let input_interface = constructor.params[0].1.object_class().expect("FilterIterator input interface");
    let predicate_interface = constructor.params[1].1.object_class().expect("FilterIterator predicate interface");
    assert_eq!(program.classes[class_id].fields.first(), Some(&constructor.params[0].1),
        "FilterIterator's first declared field owns its input");
    assert_eq!(program.classes[class_id].fields.get(1), Some(&constructor.params[1].1),
        "FilterIterator's second declared field owns its predicate");
    let next = required_method(program, class_id, M::FilterIteratorNext);
    let (none_tag, some_tag) = option_tags(program, next, element);
    let input_next = program.classes[input_interface].interface_methods.iter()
        .map(|id| &program.functions[*id]).find(|method| method.name.ends_with(".next"))
        .expect("Iterator<T> declares next");
    let (input_none, input_some) = option_tags(program, input_next, element);
    assert_eq!((none_tag, some_tag), (input_none, input_some),
        "FilterIterator input and output use the same Option<T> layout");
    let test_slot = method_slot(program, predicate_interface, ".test");
    let borrowed = ordering::borrow_storage(program, element, "value");
    format!(
        "static void ncr_fn_{constructor_id}(ncr_obj_{class_id} *p, {input_c} input, {predicate_c} predicate, size_t line, size_t column) {{\n    (void)line; (void)column;\n    p->ncr_f_0 = input;\n    p->ncr_f_1 = predicate;\n}}\nstatic {next_option} ncr_fn_{next_id}(ncr_obj_{class_id} *p, size_t line, size_t column) {{\n    (void)line; (void)column;\n    while (true) {{\n        {input_option} item = p->ncr_f_0.vtable->m{next_slot}(p->ncr_f_0.data);\n        if (item.tag == {input_none}) {{ return ({next_option}){{ .tag = {none_tag} }}; }}\n        {element_c} value = item.data.v{some_tag}.f0;\n        if (p->ncr_f_1.vtable->m{test_slot}(p->ncr_f_1.data, {borrowed})) {{\n            return ({next_option}){{ .tag = {some_tag}, .data.v{some_tag}.f0 = value }};\n        }}\n        {drop_value}\n    }}\n}}\n",
        constructor_id = constructor.id,
        class_id = class_id,
        input_c = ctype(program, constructor.params[0].1),
        predicate_c = ctype(program, constructor.params[1].1),
        next_option = ctype(program, next.return_type),
        next_id = next.id,
        input_option = ctype(program, input_next.return_type),
        next_slot = method_slot(program, input_interface, ".next"),
        input_none = input_none,
        none_tag = none_tag,
        element_c = ctype(program, element),
        some_tag = input_some,
        test_slot = test_slot,
        borrowed = borrowed,
        drop_value = drop_element(element, "value"),
    )
}

fn take_or_skip_iterator(program: &t::Program, class_id: usize, element: Type, take: bool) -> String {
    let (constructor_method, next_method) = if take {
        (M::TakeIteratorNew, M::TakeIteratorNext)
    } else {
        (M::SkipIteratorNew, M::SkipIteratorNext)
    };
    let constructor = required_method(program, class_id, constructor_method);
    let input_type = constructor.params[0].1;
    let input_interface = input_type.object_class().expect("bounded iterator input interface");
    assert_eq!(program.classes[class_id].fields.first(), Some(&input_type),
        "bounded iterator's declared input field owns the iterator");
    assert_eq!(constructor.params.get(1).map(|(_, ty)| *ty), Some(Type::Long),
        "bounded iterator constructor takes a nonnegative long count");
    let next = required_method(program, class_id, next_method);
    let (none_tag, some_tag) = option_tags(program, next, element);
    let input_next = program.classes[input_interface].interface_methods.iter()
        .map(|id| &program.functions[*id]).find(|method| method.name.ends_with(".next"))
        .expect("Iterator<T> declares next");
    let (input_none, input_some) = option_tags(program, input_next, element);
    assert_eq!((none_tag, some_tag), (input_none, input_some),
        "bounded iterator input and output use the same Option<T> layout");
    let next_slot = method_slot(program, input_interface, ".next");
    let next_body = if take {
        format!(
            "    if (p->remaining == 0) {{ return ({option}){{ .tag = {none} }}; }}\n    {option} item = p->ncr_f_0.vtable->m{slot}(p->ncr_f_0.data);\n    if (item.tag == {input_none}) {{ p->remaining = 0; return item; }}\n    --p->remaining;\n    return item;\n",
            option = ctype(program, next.return_type),
            none = none_tag,
            slot = next_slot,
            input_none = input_none,
        )
    } else {
        format!(
            "    while (p->remaining > 0) {{\n        {option} skipped = p->ncr_f_0.vtable->m{slot}(p->ncr_f_0.data);\n        if (skipped.tag == {input_none}) {{ p->remaining = 0; return skipped; }}\n        {element} value = skipped.data.v{input_some}.f0;\n        {drop_value}\n        --p->remaining;\n    }}\n    return p->ncr_f_0.vtable->m{slot}(p->ncr_f_0.data);\n",
            option = ctype(program, next.return_type),
            slot = next_slot,
            input_none = input_none,
            element = ctype(program, element),
            input_some = input_some,
            drop_value = drop_element(element, "value"),
        )
    };
    format!(
        "static void ncr_fn_{constructor_id}(ncr_obj_{class_id} *p, {input_c} input, int64_t count, size_t line, size_t column) {{\n    if (count < 0) {{ ncr_fail(\"negative iterator bound\", line, column); }}\n    p->ncr_f_0 = input;\n    p->remaining = count;\n}}\nstatic {next_option} ncr_fn_{next_id}(ncr_obj_{class_id} *p, size_t line, size_t column) {{\n    (void)line; (void)column;\n{next_body}}}\n",
        constructor_id = constructor.id,
        class_id = class_id,
        input_c = ctype(program, input_type),
        next_option = ctype(program, next.return_type),
        next_id = next.id,
        next_body = next_body,
    )
}

fn iterators_factory(program: &t::Program, class_id: usize, take: bool) -> String {
    let factory_method = if take { M::IteratorsTake } else { M::IteratorsSkip };
    let factory = required_method(program, class_id, factory_method);
    let input_type = factory.params[0].1;
    let Type::Object(interface) = factory.return_type else {
        unreachable!("Iterators factory returns Iterator<T>")
    };
    assert_eq!(input_type, factory.return_type, "Iterator factory transfers its input interface");
    assert_eq!(factory.params.get(1).map(|(_, ty)| *ty), Some(Type::Long),
        "Iterator factory takes a nonnegative long count");
    let element = match program.classes[class_id].native {
        Some(C::Iterators(element)) => element,
        _ => unreachable!("Iterators factory belongs to Iterators<T>"),
    };
    let adapter_native = if take { C::TakeIterator(element) } else { C::SkipIterator(element) };
    let adapter = concrete_class(program, adapter_native);
    assert_eq!(implemented_iterator(program, adapter, element), interface,
        "bounded adapter implements its returned Iterator<T>");
    let constructor_method = if take { M::TakeIteratorNew } else { M::SkipIteratorNew };
    let constructor = class_method(program, adapter, constructor_method)
        .expect("bounded iterator constructor is present");
    format!(
        "static {} ncr_fn_{}({} input, int64_t count, size_t line, size_t column) {{\n    ncr_obj_{} *adapter = ncr_allocate(sizeof(ncr_obj_{}), line, column);\n    ncr_fn_{}(adapter, input, count, line, column);\n    return ({}){{ .data = adapter, .vtable = {} }};\n}}\n",
        ctype(program, factory.return_type), factory.id, ctype(program, input_type),
        adapter, adapter, constructor.id, ctype(program, factory.return_type),
        vtable(program, adapter, interface),
    )
}

pub(super) fn implementation(program: &t::Program, class: C, class_id: usize) -> String {
    match class {
        C::ListIterator(element) => list_iterator_next(program, class_id, element),
        C::MapIterator(input, output) => map_iterator(program, class_id, input, output),
        C::FilterIterator(element) => filter_iterator(program, class_id, element),
        C::TakeIterator(element) => take_or_skip_iterator(program, class_id, element, true),
        C::SkipIterator(element) => take_or_skip_iterator(program, class_id, element, false),
        C::Iterators(_) => format!(
            "{}{}",
            iterators_factory(program, class_id, true),
            iterators_factory(program, class_id, false),
        ),
        _ => unreachable!("iterator implementation called for another native class"),
    }
}
