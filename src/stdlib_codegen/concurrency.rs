use crate::ir::{self as t, NativeClass as C, NativeMethod as M, Type};
use super::support::{required_indexed_native_method, result_id, variant_id};
use crate::c_repr::ctype;

fn matching_class(program: &t::Program, predicate: impl Fn(C) -> Option<Type>, value: Type) -> usize {
    program.classes.iter().find_map(|class| {
        class.native.and_then(|native| predicate(native).filter(|element| *element == value).map(|_| class.id))
    }).expect("matching concurrency specialization is present")
}
fn mutex_class(program: &t::Program, value: Type) -> usize {
    matching_class(program, |native| if let C::Mutex(element) = native { Some(element) } else { None }, value)
}
fn channel_class(program: &t::Program, value: Type) -> usize {
    matching_class(program, |native| if let C::Channel(element) = native { Some(element) } else { None }, value)
}
fn is_concurrency(class: C) -> bool {
    matches!(class, C::Thread(_) | C::Mutex(_) | C::Guard(_) | C::Channel(_) | C::Sender(_) | C::Receiver(_) |
        C::Condition(_) | C::Future(_) | C::ThreadPool(_) | C::AtomicInt | C::AtomicLong |
        C::AtomicBoolean | C::Semaphore | C::CountDownLatch | C::Barrier | C::CancellationToken)
}
fn native_owner(program: &t::Program, function: &t::Function) -> usize {
    if let Some(receiver) = function.receiver { return receiver; }
    let result = result_id(function);
    let Type::Object(owner) = program.classes[result].result.expect("native factory returns Result").ok_type
        else { unreachable!("native factory result owns its handle") };
    owner
}
fn type_parameter(program: &t::Program, id: usize) -> Type {
    match program.classes[id].native.expect("concurrency native class") {
        C::Thread(value) | C::Mutex(value) | C::Guard(value) | C::Channel(value) |
        C::Sender(value) | C::Receiver(value) => value,
        _ => unreachable!("concurrency type parameter"),
    }
}
#[derive(Clone, Copy)]
struct ThreadRun {
    task: usize,
    run: usize,
    result: Type,
}
fn thread_run_index(program: &t::Program) -> Vec<Option<ThreadRun>> {
    if !program.classes.iter().any(|class| {
        matches!(class.native, Some(C::Thread(_)))
    }) {
        return Vec::new();
    }
    let mut index = vec![None; program.classes.len()];
    for spawn in &program.functions {
        if spawn.native != Some(M::ThreadSpawn) { continue; }
        let thread_id = native_owner(program, spawn);
        let Type::Object(task) = spawn.params[0].1 else {
            unreachable!("Task is an interface object");
        };
        let run = program.classes[task].interface_methods.iter().copied()
            .find(|id| program.functions[*id].name.ends_with(".run"))
            .expect("Task.run interface method is present");
        index[thread_id].get_or_insert(ThreadRun {
            task,
            run,
            result: program.functions[run].return_type,
        });
    }
    index
}
fn task_run(thread_runs: &[Option<ThreadRun>], thread_id: usize) -> ThreadRun {
    thread_runs[thread_id].expect("Thread.spawn method is present")
}
fn zero_value(program: &t::Program, ty: Type) -> String {
    match ty {
        Type::Object(id) if program.classes[id].is_interface => format!("(ncr_obj_{id}){{NULL, NULL}}"),
        Type::Object(id) if program.classes[id].variants.is_some() => format!("(ncr_obj_{id}){{.tag = -1}}"),
        Type::Object(_) | Type::Borrow { .. } => "NULL".to_owned(),
        Type::Byte | Type::Unit => "UINT8_C(0)".to_owned(),
        Type::Char => "UINT32_C(0)".to_owned(),
        Type::Int => "INT32_C(0)".to_owned(),
        Type::Long => "INT64_C(0)".to_owned(),
        Type::Double => "0.0".to_owned(),
        Type::Boolean => "false".to_owned(),
        Type::String => "(ncr_string){NULL, 0}".to_owned(),
        Type::Void => unreachable!("void cannot be stored in a concurrency control block"),
    }
}
fn drop_value(_program: &t::Program, ty: Type, value: &str) -> String {
    if let Type::Object(id) = ty { format!("ncr_drop_{id}({value});\n") } else { String::new() }
}
fn result_ok(program: &t::Program, function: &t::Function, value: &str) -> String {
    let id = result_id(function);
    let layout = program.classes[id].result.expect("trusted Result layout");
    format!("(ncr_obj_{id}){{ .tag = {}, .data.v{}.f0 = {value} }}",
        layout.ok_variant, layout.ok_variant)
}
fn result_error_literal(_program: &t::Program, function: &t::Function, message: &str) -> String {
    let id = result_id(function);
    format!("ncr_result_error_{id}(\"{message}\", sizeof(\"{message}\") - 1, ncr_line, ncr_column)")
}
fn result_error_code(_program: &t::Program, function: &t::Function, operation: &str, code: &str) -> String {
    let id = result_id(function);
    format!("char ncr_error_message[128];\nsize_t ncr_error_length = ncr_sync_error_text(ncr_error_message, sizeof(ncr_error_message), \"{operation}\", {code});\nreturn ncr_result_error_{id}(ncr_error_message, ncr_error_length, ncr_line, ncr_column);")
}
fn method_body(program: &t::Program, function: &t::Function, body: String) -> String {
    let mut params = Vec::new();
    if let Some(receiver) = function.receiver {
        params.push(format!("{} ncr_self", ctype(program, Type::Object(receiver))));
    }
    for (id, ty) in &function.params {
        params.push(format!("{} ncr_v_{id}", ctype(program, *ty)));
    }
    params.extend(["size_t ncr_line".to_owned(), "size_t ncr_column".to_owned()]);
    format!("static {} ncr_fn_{}({}) {{\n{}\n}}\n",
        ctype(program, function.return_type), function.id, params.join(", "), body)
}
fn option_type(program: &t::Program, function: &t::Function) -> usize {
    let result = result_id(function);
    let Type::Object(option) = program.classes[result].result.expect("Result layout").ok_type
        else { unreachable!("receive returns Option<T>") };
    option
}
fn option_none(program: &t::Program, option: usize) -> String {
    let tag = variant_id(program, option, "None");
    format!("(ncr_obj_{option}){{ .tag = {tag} }}")
}
fn option_some(program: &t::Program, option: usize, value: &str) -> String {
    let tag = variant_id(program, option, "Some");
    format!("(ncr_obj_{option}){{ .tag = {tag}, .data.v{tag}.f0 = {value} }}")
}
fn error_return(program: &t::Program, function: &t::Function, operation: &str, code: &str) -> String {
    format!("{{\n{}\n}}", result_error_code(program, function, operation, code))
}
fn error_literal_return(program: &t::Program, function: &t::Function, message: &str) -> String {
    format!("return {};", result_error_literal(program, function, message))
}

pub(super) fn fields(program: &t::Program, class: C, class_id: usize) -> String {
    match class {
        C::Thread(_) => "    pthread_t thread;\n    void *context;\n    bool joined;\n".to_owned(),
        C::Mutex(_) => format!("    struct ncr_mutex_control_{class_id} *control;\n"),
        C::Guard(value) => {
            let mutex = mutex_class(program, value);
            format!("    struct ncr_mutex_control_{mutex} *control;\n    bool locked;\n")
        }
        C::Channel(_) => format!("    struct ncr_channel_control_{class_id} *control;\n    bool sender_taken;\n    bool receiver_taken;\n"),
        C::Sender(value) | C::Receiver(value) => {
            let channel = channel_class(program, value);
            format!("    struct ncr_channel_control_{channel} *control;\n    bool closed;\n")
        }
        C::Condition(_) | C::Future(_) | C::ThreadPool(_) |
        C::AtomicInt | C::AtomicLong | C::AtomicBoolean | C::Semaphore |
        C::CountDownLatch | C::Barrier | C::CancellationToken =>
            super::concurrency_extra::fields(program, class, class_id),
        _ => unreachable!("non-concurrency native fields"),
    }
}
pub(super) fn drop_fields(class_id: usize) -> String {
    format!("    ncr_concurrency_drop_{class_id}(p);\n")
}
pub(super) fn drop_helper_prototypes(program: &t::Program) -> String {
    let mut out = String::new();
    for class in &program.classes {
        if class.native.is_some_and(is_concurrency) {
            out.push_str(&format!("static void ncr_concurrency_drop_{}(ncr_obj_{} *p);\n", class.id, class.id));
        }
    }
    out
}

fn control_definitions(program: &t::Program, thread_runs: &[Option<ThreadRun>]) -> String {
    let mut out = String::new();
    for class in &program.classes {
        match class.native {
            Some(C::Thread(_)) => {
                let ThreadRun { task, result, .. } = task_run(thread_runs, class.id);
                out.push_str(&format!("struct ncr_thread_context_{} {{ ncr_obj_{task} task; {} result; }};\n",
                    class.id, ctype(program, result)));
            }
            Some(C::Mutex(value)) => out.push_str(&format!(
                "struct ncr_mutex_control_{} {{ _Atomic size_t references; pthread_mutex_t mutex; {} value; void *aux_payload; void (*aux_drop)(void *); struct ncr_condition_node *conditions; }};\n",
                class.id, ctype(program, value))),
            Some(C::Channel(value)) => out.push_str(&format!(
                "struct ncr_channel_control_{} {{ _Atomic size_t references; pthread_mutex_t mutex; pthread_cond_t not_empty; pthread_cond_t not_full; {} *slots; size_t capacity; size_t head; size_t count; size_t sender_count; bool receiver_closed; }};\n",
                class.id, ctype(program, value))),
            _ => {}
        }
    }
    out.push_str(&super::concurrency_extra::control_definitions(program));
    out
}

fn thread_worker(
    program: &t::Program,
    thread_id: usize,
    thread_runs: &[Option<ThreadRun>],
) -> String {
    let ThreadRun { task, run, .. } = task_run(thread_runs, thread_id);
    let slot = program.functions[run].virtual_slot.expect("Task.run vtable slot");
    format!(
        "static void *ncr_thread_main_{thread_id}(void *raw) {{\n    struct ncr_thread_context_{thread_id} *context = raw;\n    context->result = context->task.vtable->m{slot}(context->task.data);\n    ncr_drop_{task}(context->task);\n    return NULL;\n}}\n",
    )
}
fn mutex_control_helpers(program: &t::Program, id: usize, value: Type) -> String {
    let drop = drop_value(program, value, "control->value");
    let mut out = format!(
        "static void ncr_mutex_release_{id}(struct ncr_mutex_control_{id} *control) {{\n    if (!ncr_sync_ref_release(&control->references)) {{ return; }}\n    struct ncr_condition_node *condition = control->conditions;\n    while (condition != NULL) {{\n        struct ncr_condition_node *next = condition->next;\n        int condition_error = pthread_cond_destroy(&condition->condition);\n        if (condition_error != 0) {{ ncr_sync_fatal(\"pthread_cond_destroy(control release)\", condition_error); }}\n        ncr_deallocate(condition);\n        condition = next;\n    }}\n    int error = pthread_mutex_destroy(&control->mutex);\n    if (error != 0) {{ ncr_sync_fatal(\"pthread_mutex_destroy\", error); }}\n{drop}    if (control->aux_drop != NULL) {{ control->aux_drop(control->aux_payload); }}\n    ncr_deallocate(control);\n}}\n"
    );
    out.push_str(&format!(
        "static struct ncr_condition_node *ncr_mutex_condition_init_{id}(struct ncr_condition_node *node, int *failure) {{\n    node->next = NULL;\n    pthread_condattr_t attributes;\n    int error = pthread_condattr_init(&attributes);\n    if (error != 0) {{ ncr_deallocate(node); *failure = error; return NULL; }}\n    error = pthread_condattr_setclock(&attributes, CLOCK_MONOTONIC);\n    if (error != 0) {{\n        int cleanup_error = pthread_condattr_destroy(&attributes);\n        if (cleanup_error != 0) {{ ncr_sync_fatal(\"pthread_condattr_destroy(cleanup)\", cleanup_error); }}\n        ncr_deallocate(node); *failure = error; return NULL;\n    }}\n    error = pthread_cond_init(&node->condition, &attributes);\n    int attr_error = pthread_condattr_destroy(&attributes);\n    if (error != 0) {{\n        if (attr_error != 0) {{ ncr_sync_fatal(\"pthread_condattr_destroy(cleanup)\", attr_error); }}\n        ncr_deallocate(node); *failure = error; return NULL;\n    }}\n    if (attr_error != 0) {{\n        int cleanup_error = pthread_cond_destroy(&node->condition);\n        if (cleanup_error != 0) {{ ncr_sync_fatal(\"pthread_cond_destroy(cleanup)\", cleanup_error); }}\n        ncr_deallocate(node); *failure = attr_error; return NULL;\n    }}\n    *failure = 0;\n    return node;\n}}\nstatic struct ncr_condition_node *ncr_mutex_condition_new_{id}(struct ncr_mutex_control_{id} *control, int *failure) {{\n    struct ncr_condition_node *node = ncr_allocate(sizeof(*node), 0, 0);\n    node = ncr_mutex_condition_init_{id}(node, failure);\n    if (node == NULL) {{ return NULL; }}\n    int error = pthread_mutex_lock(&control->mutex);\n    if (error != 0) {{\n        int cleanup_error = pthread_cond_destroy(&node->condition);\n        if (cleanup_error != 0) {{ ncr_sync_fatal(\"pthread_cond_destroy(cleanup)\", cleanup_error); }}\n        ncr_deallocate(node); *failure = error; return NULL;\n    }}\n    node->next = control->conditions;\n    control->conditions = node;\n    error = pthread_mutex_unlock(&control->mutex);\n    if (error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(condition create)\", error); }}\n    *failure = 0;\n    return node;\n}}\nstatic struct ncr_condition_node *ncr_mutex_public_condition_new_{id}(int *failure) {{\n    struct ncr_condition_handle_node *handle = ncr_allocate(sizeof(*handle), 0, 0);\n    atomic_init(&handle->references, 1);\n    return ncr_mutex_condition_init_{id}(&handle->node, failure);\n}}\n"
    ));
    if value == Type::Unit {
        out.push_str(&format!(
            "static struct ncr_mutex_control_{id} *ncr_sync_mutex_control_new_{id}(int *failure) {{\n    struct ncr_mutex_control_{id} *control = ncr_allocate(sizeof(*control), 0, 0);\n    atomic_init(&control->references, 1);\n    control->value = UINT8_C(0);\n    control->aux_payload = NULL; control->aux_drop = NULL; control->conditions = NULL;\n    int error = pthread_mutex_init(&control->mutex, NULL);\n    if (error != 0) {{ ncr_deallocate(control); *failure = error; return NULL; }}\n    *failure = 0;\n    return control;\n}}\n"
        ));
    }
    out
}
fn channel_control_helpers(program: &t::Program, id: usize, value: Type) -> String {
    let drop = drop_value(program, value, "control->slots[index]");
    let zero = zero_value(program, value);
    let mut out = format!(
        "static size_t ncr_channel_index_{id}(size_t start, size_t offset, size_t capacity) {{\n    size_t remaining = capacity - start;\n    return offset < remaining ? start + offset : offset - remaining;\n}}\n"
    );
    out.push_str(&format!(
        "static void ncr_channel_drop_items_{id}(struct ncr_channel_control_{id} *control, size_t start, size_t count) {{\n    for (size_t i = 0; i < count; ++i) {{\n        size_t index = ncr_channel_index_{id}(start, i, control->capacity);\n{drop}        control->slots[index] = {zero};\n    }}\n}}\n"
    ));
    out.push_str(&format!(
        "static void ncr_channel_release_{id}(struct ncr_channel_control_{id} *control) {{\n    if (!ncr_sync_ref_release(&control->references)) {{ return; }}\n    if (control->count != 0) {{\n        size_t start = control->head;\n        size_t count = control->count;\n        control->head = 0; control->count = 0;\n        ncr_channel_drop_items_{id}(control, start, count);\n    }}\n    if (control->sender_count != 0 || !control->receiver_closed) {{ ncr_sync_fatal(\"channel released with open endpoints\", EINVAL); }}\n    int error = pthread_cond_destroy(&control->not_empty);\n    if (error != 0) {{ ncr_sync_fatal(\"pthread_cond_destroy(not_empty)\", error); }}\n    error = pthread_cond_destroy(&control->not_full);\n    if (error != 0) {{ ncr_sync_fatal(\"pthread_cond_destroy(not_full)\", error); }}\n    error = pthread_mutex_destroy(&control->mutex);\n    if (error != 0) {{ ncr_sync_fatal(\"pthread_mutex_destroy(channel)\", error); }}\n    ncr_deallocate(control->slots);\n    ncr_deallocate(control);\n}}\n"
    ));
    out.push_str(&format!(
        "static int ncr_channel_sender_close_{id}(struct ncr_channel_control_{id} *control, bool *closed) {{\n    if (*closed) {{ return EINVAL; }}\n    int error = pthread_mutex_lock(&control->mutex);\n    if (error != 0) {{ return error; }}\n    if (control->sender_count == 0) {{ ncr_sync_fatal(\"channel sender count underflow\", EINVAL); }}\n    *closed = true;\n    --control->sender_count;\n    error = pthread_cond_broadcast(&control->not_empty);\n    int unlock_error = pthread_mutex_unlock(&control->mutex);\n    if (unlock_error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(channel sender close)\", unlock_error); }}\n    return error;\n}}\n"
    ));
    out.push_str(&format!(
        "static int ncr_channel_receiver_close_{id}(struct ncr_channel_control_{id} *control, bool *closed) {{\n    if (*closed) {{ return EINVAL; }}\n    int error = pthread_mutex_lock(&control->mutex);\n    if (error != 0) {{ return error; }}\n    size_t start = control->head;\n    size_t count = control->count;\n    control->head = 0; control->count = 0;\n    control->receiver_closed = true;\n    *closed = true;\n    int wake_empty_error = pthread_cond_broadcast(&control->not_empty);\n    int wake_full_error = pthread_cond_broadcast(&control->not_full);\n    int unlock_error = pthread_mutex_unlock(&control->mutex);\n    if (unlock_error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(channel receiver close)\", unlock_error); }}\n    ncr_channel_drop_items_{id}(control, start, count);\n    return wake_empty_error != 0 ? wake_empty_error : wake_full_error;\n}}\n"
    ));
    out
}
fn channel_pair_drop(_program: &t::Program, id: usize) -> String {
    format!(
        r#"static void ncr_channel_pair_drop_{id}(ncr_obj_{id} *p) {{
    struct ncr_channel_control_{id} *control = p->control;
    p->control = NULL;
    if (control == NULL) {{ return; }}
    if (!p->sender_taken || !p->receiver_taken) {{
        int error = pthread_mutex_lock(&control->mutex);
        if (error != 0) {{ ncr_sync_fatal("pthread_mutex_lock(channel pair drop)", error); }}
        size_t start = control->head;
        size_t count = 0;
        if (!p->sender_taken) {{
            if (control->sender_count == 0) {{ ncr_sync_fatal("channel sender count underflow", EINVAL); }}
            --control->sender_count;
        }}
        if (!p->receiver_taken && !control->receiver_closed) {{
            control->receiver_closed = true;
            start = control->head;
            count = control->count;
            control->head = 0;
            control->count = 0;
        }}
        int wake_empty_error = pthread_cond_broadcast(&control->not_empty);
        int wake_full_error = pthread_cond_broadcast(&control->not_full);
        int unlock_error = pthread_mutex_unlock(&control->mutex);
        if (unlock_error != 0) {{ ncr_sync_fatal("pthread_mutex_unlock(channel pair drop)", unlock_error); }}
        ncr_channel_drop_items_{id}(control, start, count);
        if (wake_empty_error != 0) {{ ncr_sync_fatal("pthread_cond_broadcast(channel pair drop)", wake_empty_error); }}
        if (wake_full_error != 0) {{ ncr_sync_fatal("pthread_cond_broadcast(channel pair drop)", wake_full_error); }}
    }}
    ncr_channel_release_{id}(control);
}}
"#
    )
}
fn channel_sender_drop(_program: &t::Program, id: usize, channel: usize) -> String {
    format!(
        "static void ncr_sender_drop_{id}(ncr_obj_{id} *p) {{\n    struct ncr_channel_control_{channel} *control = p->control;\n    p->control = NULL;\n    if (control == NULL) {{ return; }}\n    if (!p->closed) {{\n        int error = ncr_channel_sender_close_{channel}(control, &p->closed);\n        if (error != 0) {{ ncr_sync_fatal(\"channel sender drop\", error); }}\n    }}\n    ncr_channel_release_{channel}(control);\n}}\n"
    )
}
fn channel_receiver_drop(_program: &t::Program, id: usize, channel: usize) -> String {
    format!(
        "static void ncr_receiver_drop_{id}(ncr_obj_{id} *p) {{\n    struct ncr_channel_control_{channel} *control = p->control;\n    p->control = NULL;\n    if (control == NULL) {{ return; }}\n    if (!p->closed) {{\n        int error = ncr_channel_receiver_close_{channel}(control, &p->closed);\n        if (error != 0) {{ ncr_sync_fatal(\"channel receiver drop\", error); }}\n    }}\n    ncr_channel_release_{channel}(control);\n}}\n"
    )
}

fn thread_spawn(
    program: &t::Program,
    function: &t::Function,
    thread_runs: &[Option<ThreadRun>],
) -> String {
    let thread = native_owner(program, function);
    let ThreadRun { task, .. } = task_run(thread_runs, thread);
    let context = format!("struct ncr_thread_context_{thread}");
    let task_arg = function.params[0].0;
    let task_drop = format!("ncr_drop_{task}(context->task);");
    let body = format!(
        "    {context} *context = ncr_allocate(sizeof(*context), ncr_line, ncr_column);\n    context->task = ncr_v_{task_arg};\n    ncr_obj_{thread} *thread = ncr_allocate(sizeof(*thread), ncr_line, ncr_column);\n    int error = ncr_sync_pthread_create(&thread->thread, ncr_thread_main_{thread}, context);\n    if (error != 0) {{\n        {task_drop}\n        ncr_deallocate(context);\n        ncr_deallocate(thread);\n        {error}\n    }}\n    thread->context = context;\n    thread->joined = false;\n    return {};",
        result_ok(program, function, "thread"),
        error = result_error_code(program, function, "pthread_create", "error"),
    );
    method_body(program, function, body)
}
fn thread_join(program: &t::Program, function: &t::Function) -> String {
    let thread = function.receiver.expect("Thread.join receiver");
    let result = type_parameter(program, thread);
    let body = format!(
        "    if (ncr_self->joined || ncr_self->context == NULL) {{ {} }}\n    int error = pthread_join(ncr_self->thread, NULL);\n    if (error != 0) {{ {} }}\n    struct ncr_thread_context_{thread} *context = ncr_self->context;\n    {} result = context->result;\n    context->result = {};\n    ncr_self->context = NULL;\n    ncr_self->joined = true;\n    ncr_deallocate(context);\n    return {};",
        error_literal_return(program, function, "Thread has already been joined"),
        error_return(program, function, "pthread_join", "error"),
        ctype(program, result), zero_value(program, result), result_ok(program, function, "result"),
    );
    method_body(program, function, body)
}
fn mutex_create(program: &t::Program, function: &t::Function) -> String {
    let mutex = native_owner(program, function);
    let value = type_parameter(program, mutex);
    let arg = function.params[0].0;
    let drop_initial = drop_value(program, value, "control->value");
    let destroy_attr = "int cleanup_error = pthread_mutexattr_destroy(&attributes);\n        if (cleanup_error != 0) { ncr_sync_fatal(\"pthread_mutexattr_destroy(cleanup)\", cleanup_error); }";
    let body = format!(
        "    struct ncr_mutex_control_{mutex} *control = ncr_allocate(sizeof(*control), ncr_line, ncr_column);\n    atomic_init(&control->references, 1);\n    control->value = ncr_v_{arg};\n    pthread_mutexattr_t attributes;\n    int error = pthread_mutexattr_init(&attributes);\n    if (error != 0) {{\n        {drop_initial}        ncr_deallocate(control);\n        {attr_init_error}\n    }}\n    error = pthread_mutexattr_settype(&attributes, PTHREAD_MUTEX_ERRORCHECK);\n    if (error != 0) {{\n        {destroy_attr}\n        {drop_initial}        ncr_deallocate(control);\n        {attr_type_error}\n    }}\n    error = pthread_mutex_init(&control->mutex, &attributes);\n    int attr_error = pthread_mutexattr_destroy(&attributes);\n    if (error != 0) {{\n        if (attr_error != 0) {{ ncr_sync_fatal(\"pthread_mutexattr_destroy(cleanup)\", attr_error); }}\n        {drop_initial}        ncr_deallocate(control);\n        {mutex_init_error}\n    }}\n    if (attr_error != 0) {{\n        int cleanup_error = pthread_mutex_destroy(&control->mutex);\n        if (cleanup_error != 0) {{ ncr_sync_fatal(\"pthread_mutex_destroy(cleanup)\", cleanup_error); }}\n        {drop_initial}        ncr_deallocate(control);\n        {attr_destroy_error}\n    }}\n    ncr_obj_{mutex} *handle = ncr_allocate(sizeof(*handle), ncr_line, ncr_column);\n    handle->control = control;\n    return {};",
        result_ok(program, function, "handle"),
        attr_init_error = result_error_code(program, function, "pthread_mutexattr_init", "error"),
        attr_type_error = result_error_code(program, function, "pthread_mutexattr_settype", "error"),
        mutex_init_error = result_error_code(program, function, "pthread_mutex_init", "error"),
        attr_destroy_error = result_error_code(program, function, "pthread_mutexattr_destroy", "attr_error"),
    );
    method_body(program, function, body)
}
fn mutex_clone(program: &t::Program, function: &t::Function) -> String {
    let mutex = function.receiver.unwrap();
    let body = format!(
        "    struct ncr_mutex_control_{mutex} *control = ncr_self->control;\n    if (!ncr_sync_ref_acquire(&control->references)) {{ {} }}\n    ncr_obj_{mutex} *handle = ncr_allocate(sizeof(*handle), ncr_line, ncr_column);\n    handle->control = control;\n    return {};",
        error_literal_return(program, function, "Mutex handle reference count overflow"),
        result_ok(program, function, "handle"),
    );
    method_body(program, function, body)
}
fn mutex_lock(program: &t::Program, function: &t::Function) -> String {
    let mutex = function.receiver.unwrap();
    let guard = match program.classes[result_id(function)].result.expect("Result layout").ok_type {
        Type::Object(id) => id, _ => unreachable!("lock returns Guard"),
    };
    let body = format!(
        "    struct ncr_mutex_control_{mutex} *control = ncr_self->control;\n    if (!ncr_sync_ref_acquire(&control->references)) {{ {} }}\n    int error = pthread_mutex_lock(&control->mutex);\n    if (error != 0) {{\n        ncr_mutex_release_{mutex}(control);\n        {}\n    }}\n    ncr_obj_{guard} *guard = ncr_allocate(sizeof(*guard), ncr_line, ncr_column);\n    guard->control = control; guard->locked = true;\n    return {};",
        error_literal_return(program, function, "Mutex guard reference count overflow"),
        result_error_code(program, function, "pthread_mutex_lock", "error"),
        result_ok(program, function, "guard"),
    );
    method_body(program, function, body)
}
fn mutex_try_lock(program: &t::Program, function: &t::Function) -> String {
    let mutex = function.receiver.unwrap();
    let option = option_type(program, function);
    let Type::Object(guard) = program.classes[option].variants.as_ref().unwrap()
        .iter().find(|variant| variant.name == "Some").unwrap().fields[0] else {
        unreachable!("Some carries Guard")
    };
    let none = result_ok(program, function, &option_none(program, option));
    let some = option_some(program, option, "guard");
    let body = format!(
        "    struct ncr_mutex_control_{mutex} *control = ncr_self->control;\n    if (!ncr_sync_ref_acquire(&control->references)) {{ {} }}\n    int error = pthread_mutex_trylock(&control->mutex);\n    if (error == EBUSY) {{\n        ncr_mutex_release_{mutex}(control);\n        return {none};\n    }}\n    if (error != 0) {{\n        ncr_mutex_release_{mutex}(control);\n        {}\n    }}\n    ncr_obj_{guard} *guard = ncr_allocate(sizeof(*guard), ncr_line, ncr_column);\n    guard->control = control; guard->locked = true;\n    return {};",
        error_literal_return(program, function, "Mutex guard reference count overflow"),
        result_error_code(program, function, "pthread_mutex_trylock", "error"),
        result_ok(program, function, &some),
    );
    method_body(program, function, body)
}
fn guard_access(program: &t::Program, function: &t::Function, _mutable: bool) -> String {
    let guard = function.receiver.unwrap();
    let value = type_parameter(program, guard);
    let Some(id) = value.object_class() else { unreachable!("Guard accessors specialize to objects") };
    let access = if program.classes[id].variants.is_some() {
        "&ncr_self->control->value"
    } else {
        "ncr_self->control->value"
    };
    method_body(program, function, format!("    return {access};"))
}
fn guard_read(program: &t::Program, function: &t::Function) -> String {
    method_body(program, function, "    return ncr_self->control->value;".to_owned())
}
fn guard_write(program: &t::Program, function: &t::Function) -> String {
    let arg = function.params[0].0;
    let body = format!("    ncr_self->control->value = ncr_v_{arg};\n    return {};",
        result_ok(program, function, "UINT8_C(0)"));
    method_body(program, function, body)
}

fn channel_bounded(program: &t::Program, function: &t::Function) -> String {
    let channel = native_owner(program, function);
    let value = type_parameter(program, channel);
    let capacity = function.params[0].0;
    let invalid = error_literal_return(program, function,
        "Channel capacity must be positive and fit in the native allocation size");
    let attr_init_error = result_error_code(program, function, "pthread_mutexattr_init", "error");
    let attr_type_error = result_error_code(program, function, "pthread_mutexattr_settype", "error");
    let mutex_init_error = result_error_code(program, function, "pthread_mutex_init", "error");
    let attr_destroy_error = result_error_code(program, function, "pthread_mutexattr_destroy", "attr_error");
    let empty_init_error = result_error_code(program, function, "pthread_cond_init(not_empty)", "error");
    let full_init_error = result_error_code(program, function, "pthread_cond_init(not_full)", "error");
    let ok = result_ok(program, function, "pair");
    let body = format!(
        r#"    if (ncr_v_{capacity} < 1 || (uint64_t)ncr_v_{capacity} > (uint64_t)(SIZE_MAX / sizeof({value_type}))) {{ {invalid} }}
    size_t slot_bytes = (size_t)ncr_v_{capacity} * sizeof({value_type});
    struct ncr_channel_control_{channel} *control = ncr_allocate(sizeof(*control), ncr_line, ncr_column);
    atomic_init(&control->references, 1);
    control->capacity = (size_t)ncr_v_{capacity};
    control->head = 0;
    control->count = 0;
    control->sender_count = 1;
    control->receiver_closed = false;
    pthread_mutexattr_t attributes;
    int error = pthread_mutexattr_init(&attributes);
    if (error != 0) {{ ncr_deallocate(control); {attr_init_error} }}
    error = pthread_mutexattr_settype(&attributes, PTHREAD_MUTEX_ERRORCHECK);
    if (error != 0) {{
        int cleanup_error = pthread_mutexattr_destroy(&attributes);
        if (cleanup_error != 0) {{ ncr_sync_fatal("pthread_mutexattr_destroy(cleanup)", cleanup_error); }}
        ncr_deallocate(control);
        {attr_type_error}
    }}
    error = pthread_mutex_init(&control->mutex, &attributes);
    int attr_error = pthread_mutexattr_destroy(&attributes);
    if (error != 0) {{
        if (attr_error != 0) {{ ncr_sync_fatal("pthread_mutexattr_destroy(cleanup)", attr_error); }}
        ncr_deallocate(control);
        {mutex_init_error}
    }}
    if (attr_error != 0) {{
        int cleanup_error = pthread_mutex_destroy(&control->mutex);
        if (cleanup_error != 0) {{ ncr_sync_fatal("pthread_mutex_destroy(cleanup)", cleanup_error); }}
        ncr_deallocate(control);
        {attr_destroy_error}
    }}
    error = pthread_cond_init(&control->not_empty, NULL);
    if (error != 0) {{
        int cleanup_error = pthread_mutex_destroy(&control->mutex);
        if (cleanup_error != 0) {{ ncr_sync_fatal("pthread_mutex_destroy(cleanup)", cleanup_error); }}
        ncr_deallocate(control);
        {empty_init_error}
    }}
    error = pthread_cond_init(&control->not_full, NULL);
    if (error != 0) {{
        int cleanup_error = pthread_cond_destroy(&control->not_empty);
        if (cleanup_error != 0) {{ ncr_sync_fatal("pthread_cond_destroy(cleanup)", cleanup_error); }}
        cleanup_error = pthread_mutex_destroy(&control->mutex);
        if (cleanup_error != 0) {{ ncr_sync_fatal("pthread_mutex_destroy(cleanup)", cleanup_error); }}
        ncr_deallocate(control);
        {full_init_error}
    }}
    control->slots = ncr_allocate(slot_bytes, ncr_line, ncr_column);
    ncr_obj_{channel} *pair = ncr_allocate(sizeof(*pair), ncr_line, ncr_column);
    pair->control = control;
    pair->sender_taken = false;
    pair->receiver_taken = false;
    return {ok};"#,
        value_type = ctype(program, value),
    );
    method_body(program, function, body)
}
fn channel_take(program: &t::Program, function: &t::Function, sender: bool) -> String {
    let channel = function.receiver.unwrap();
    let value = type_parameter(program, channel);
    let control_id = channel_class(program, value);
    let endpoint = match program.classes[result_id(function)].result.unwrap().ok_type {
        Type::Object(id) => id, _ => unreachable!("take returns an endpoint"),
    };
    let (flag, message) = if sender {
        ("sender_taken", "Channel sender has already been taken")
    } else {
        ("receiver_taken", "Channel receiver has already been taken")
    };
    let closed_check = format!(
        "if (control->receiver_closed) {{\n        int unlock_error = pthread_mutex_unlock(&control->mutex);\n        if (unlock_error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(channel take)\", unlock_error); }}\n        {}\n    }}",
        error_literal_return(program, function, "Channel receiver is closed"),
    );
    let body = format!(
        "    if (ncr_self->{flag}) {{ {} }}\n    struct ncr_channel_control_{control_id} *control = ncr_self->control;\n    int error = pthread_mutex_lock(&control->mutex);\n    if (error != 0) {{ {} }}\n    {closed_check}\n    if (!ncr_sync_ref_acquire(&control->references)) {{\n        int unlock_error = pthread_mutex_unlock(&control->mutex);\n        if (unlock_error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(channel take)\", unlock_error); }}\n        {}\n    }}\n    ncr_obj_{endpoint} *endpoint = ncr_allocate(sizeof(*endpoint), ncr_line, ncr_column);\n    endpoint->control = control; endpoint->closed = false;\n    ncr_self->{flag} = true;\n    error = pthread_mutex_unlock(&control->mutex);\n    if (error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(channel take)\", error); }}\n    return {};",
        error_literal_return(program, function, message),
        result_error_code(program, function, "pthread_mutex_lock(channel take)", "error"),
        error_literal_return(program, function, "Channel endpoint reference count overflow"),
        result_ok(program, function, "endpoint"),
    );
    method_body(program, function, body)
}
fn sender_error_helpers(
    program: &t::Program,
    sender: usize,
    native_method_ids: &mut Option<t::NativeMethodIndex>,
) -> String {
    let method_ids = native_method_ids.get_or_insert_with(|| t::NativeMethodIndex::new(program));
    let function =
        required_indexed_native_method(program, method_ids, Some(sender), M::SenderSend);
    let result = result_id(function);
    let err_tag = program.classes[result].result.unwrap().err_variant;
    let Type::Object(send_error) = program.classes[result].result.unwrap().err_type
        else { unreachable!("Sender.send error is SendError<T>") };
    let closed_tag = variant_id(program, send_error, "Closed");
    let system_tag = variant_id(program, send_error, "System");
    let Type::Object(text) = program.classes[send_error].variants.as_ref().unwrap()[system_tag].fields[0]
        else { unreachable!("SendError.System begins with Text") };
    let value_type = type_parameter(program, sender);
    let closed = format!(
        "static ncr_obj_{result} ncr_sender_closed_error_{sender}({} value) {{\n    ncr_obj_{send_error} send_error = {{ .tag = {closed_tag}, .data.v{closed_tag}.f0 = value }};\n    return (ncr_obj_{result}){{ .tag = {err_tag}, .data.v{err_tag}.f0 = send_error }};\n}}\n",
        ctype(program, value_type));
    let system = format!(
        "static ncr_obj_{result} ncr_sender_system_error_{sender}({} value, const char *operation, int error, size_t line, size_t column) {{\n    char message[128];\n    size_t length = ncr_sync_error_text(message, sizeof(message), operation, error);\n    ncr_obj_{text} *text = ncr_text_from_bytes_{text}((const unsigned char *)message, length, line, column);\n    ncr_obj_{send_error} send_error = {{ .tag = {system_tag}, .data.v{system_tag}.f0 = text, .data.v{system_tag}.f1 = value }};\n    return (ncr_obj_{result}){{ .tag = {err_tag}, .data.v{err_tag}.f0 = send_error }};\n}}\n",
        ctype(program, value_type));
    format!("{closed}{system}")
}
fn sender_send(program: &t::Program, function: &t::Function) -> String {
    let sender = function.receiver.unwrap();
    let value = type_parameter(program, sender);
    let channel = channel_class(program, value);
    let value_arg = function.params[0].0;
    let result = result_id(function);
    let ok = program.classes[result].result.unwrap().ok_variant;
    let body = format!(
        "    if (ncr_self->closed) {{ return ncr_sender_closed_error_{sender}(ncr_v_{value_arg}); }}\n    struct ncr_channel_control_{channel} *control = ncr_self->control;\n    int error = pthread_mutex_lock(&control->mutex);\n    if (error != 0) {{ return ncr_sender_system_error_{sender}(ncr_v_{value_arg}, \"pthread_mutex_lock(channel send)\", error, ncr_line, ncr_column); }}\n    while (control->count == control->capacity && !control->receiver_closed) {{\n        error = pthread_cond_wait(&control->not_full, &control->mutex);\n        if (error != 0) {{\n            int unlock_error = pthread_mutex_unlock(&control->mutex);\n            if (unlock_error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(channel send error)\", unlock_error); }}\n            return ncr_sender_system_error_{sender}(ncr_v_{value_arg}, \"pthread_cond_wait(not_full)\", error, ncr_line, ncr_column);\n        }}\n    }}\n    if (control->receiver_closed) {{\n        int unlock_error = pthread_mutex_unlock(&control->mutex);\n        if (unlock_error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(channel closed send)\", unlock_error); }}\n        return ncr_sender_closed_error_{sender}(ncr_v_{value_arg});\n    }}\n    error = pthread_cond_broadcast(&control->not_empty);\n    if (error != 0) {{\n        int unlock_error = pthread_mutex_unlock(&control->mutex);\n        if (unlock_error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(channel send error)\", unlock_error); }}\n        return ncr_sender_system_error_{sender}(ncr_v_{value_arg}, \"pthread_cond_broadcast(not_empty)\", error, ncr_line, ncr_column);\n    }}\n    size_t tail = control->count < control->capacity - control->head\n        ? control->head + control->count : control->count - (control->capacity - control->head);\n    control->slots[tail] = ncr_v_{value_arg};\n    ++control->count;\n    error = pthread_mutex_unlock(&control->mutex);\n    if (error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(channel send)\", error); }}\n    return (ncr_obj_{result}){{ .tag = {ok}, .data.v{ok}.f0 = UINT8_C(0) }};",
    );
    method_body(program, function, body)
}
fn sender_clone(program: &t::Program, function: &t::Function) -> String {
    let sender = function.receiver.unwrap();
    let value = type_parameter(program, sender);
    let channel = channel_class(program, value);
    let body = format!(
        "    if (ncr_self->closed) {{ {} }}\n    struct ncr_channel_control_{channel} *control = ncr_self->control;\n    int error = pthread_mutex_lock(&control->mutex);\n    if (error != 0) {{ {} }}\n    if (control->receiver_closed) {{\n        int unlock_error = pthread_mutex_unlock(&control->mutex);\n        if (unlock_error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(cloneSender)\", unlock_error); }}\n        {}\n    }}\n    if (control->sender_count == SIZE_MAX || !ncr_sync_ref_acquire(&control->references)) {{\n        int unlock_error = pthread_mutex_unlock(&control->mutex);\n        if (unlock_error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(cloneSender)\", unlock_error); }}\n        {}\n    }}\n    ++control->sender_count;\n    ncr_obj_{sender} *copy = ncr_allocate(sizeof(*copy), ncr_line, ncr_column);\n    copy->control = control; copy->closed = false;\n    error = pthread_mutex_unlock(&control->mutex);\n    if (error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(cloneSender)\", error); }}\n    return {};",
        error_literal_return(program, function, "Sender is closed"),
        result_error_code(program, function, "pthread_mutex_lock(cloneSender)", "error"),
        error_literal_return(program, function, "Channel receiver is closed"),
        error_literal_return(program, function, "Channel sender or control reference count overflow"),
        result_ok(program, function, "copy"),
    );
    method_body(program, function, body)
}
fn sender_close(program: &t::Program, function: &t::Function) -> String {
    let sender = function.receiver.unwrap();
    let value = type_parameter(program, sender);
    let channel = channel_class(program, value);
    let body = format!(
        "    if (ncr_self->closed) {{ {} }}\n    int error = ncr_channel_sender_close_{channel}(ncr_self->control, &ncr_self->closed);\n    if (error != 0) {{ {} }}\n    return {};",
        error_literal_return(program, function, "Sender is already closed"),
        result_error_code(program, function, "channel sender close", "error"),
        result_ok(program, function, "UINT8_C(0)"),
    );
    method_body(program, function, body)
}
fn receiver_receive(program: &t::Program, function: &t::Function) -> String {
    let receiver = function.receiver.unwrap();
    let value = type_parameter(program, receiver);
    let channel = channel_class(program, value);
    let option = option_type(program, function);
    let body = format!(
        "    if (ncr_self->closed) {{ {} }}\n    struct ncr_channel_control_{channel} *control = ncr_self->control;\n    int error = pthread_mutex_lock(&control->mutex);\n    if (error != 0) {{ {} }}\n    while (control->count == 0 && control->sender_count != 0 && !control->receiver_closed) {{\n        error = pthread_cond_wait(&control->not_empty, &control->mutex);\n        if (error != 0) {{\n            int unlock_error = pthread_mutex_unlock(&control->mutex);\n            if (unlock_error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(channel receive error)\", unlock_error); }}\n            {}\n        }}\n    }}\n    if (control->receiver_closed) {{\n        int unlock_error = pthread_mutex_unlock(&control->mutex);\n        if (unlock_error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(closed receive)\", unlock_error); }}\n        {}\n    }}\n    if (control->count == 0) {{\n        error = pthread_mutex_unlock(&control->mutex);\n        if (error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(channel EOF)\", error); }}\n        return {};\n    }}\n    error = pthread_cond_broadcast(&control->not_full);\n    if (error != 0) {{\n        int unlock_error = pthread_mutex_unlock(&control->mutex);\n        if (unlock_error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(channel receive error)\", unlock_error); }}\n        {}\n    }}\n    {} value = control->slots[control->head];\n    control->slots[control->head] = {};\n    ++control->head; if (control->head == control->capacity) {{ control->head = 0; }}\n    --control->count;\n    error = pthread_mutex_unlock(&control->mutex);\n    if (error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(channel receive)\", error); }}\n    return {};",
        error_literal_return(program, function, "Receiver is closed"),
        result_error_code(program, function, "pthread_mutex_lock(channel receive)", "error"),
        result_error_code(program, function, "pthread_cond_wait(not_empty)", "error"),
        error_literal_return(program, function, "Receiver is closed"),
        result_ok(program, function, &option_none(program, option)),
        result_error_code(program, function, "pthread_cond_broadcast(not_full)", "error"),
        ctype(program, value), zero_value(program, value),
        result_ok(program, function, &option_some(program, option, "value")),
    );
    method_body(program, function, body)
}
fn receiver_close(program: &t::Program, function: &t::Function) -> String {
    let receiver = function.receiver.unwrap();
    let value = type_parameter(program, receiver);
    let channel = channel_class(program, value);
    let body = format!(
        "    if (ncr_self->closed) {{ {} }}\n    int error = ncr_channel_receiver_close_{channel}(ncr_self->control, &ncr_self->closed);\n    if (error != 0) {{ {} }}\n    return {};",
        error_literal_return(program, function, "Receiver is already closed"),
        result_error_code(program, function, "channel receiver close", "error"),
        result_ok(program, function, "UINT8_C(0)"),
    );
    method_body(program, function, body)
}

fn drop_helpers(program: &t::Program, thread_runs: &[Option<ThreadRun>]) -> String {
    let mut out = String::new();
    for class in &program.classes {
        match class.native {
            Some(C::Thread(_)) => {
                let ThreadRun { result, .. } = task_run(thread_runs, class.id);
                out.push_str(&format!(
                    "static void ncr_concurrency_drop_{}(ncr_obj_{} *p) {{\n    if (p->context == NULL) {{ return; }}\n    int error = pthread_join(p->thread, NULL);\n    if (error != 0) {{ ncr_sync_fatal(\"pthread_join during Thread drop\", error); }}\n    struct ncr_thread_context_{} *context = p->context;\n{}    p->context = NULL; p->joined = true;\n    ncr_deallocate(context);\n}}\n",
                    class.id, class.id, class.id, drop_value(program, result, "context->result")));
            }
            Some(C::Mutex(_)) => out.push_str(&format!(
                "static void ncr_concurrency_drop_{}(ncr_obj_{} *p) {{\n    struct ncr_mutex_control_{} *control = p->control;\n    p->control = NULL;\n    if (control != NULL) {{ ncr_mutex_release_{}(control); }}\n}}\n",
                class.id, class.id, class.id, class.id)),
            Some(C::Guard(value)) => {
                let mutex = mutex_class(program, value);
                out.push_str(&format!(
                    "static void ncr_concurrency_drop_{}(ncr_obj_{} *p) {{\n    if (p->control == NULL) {{ return; }}\n    if (p->locked) {{\n        int error = pthread_mutex_unlock(&p->control->mutex);\n        if (error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock during Guard drop\", error); }}\n        p->locked = false;\n    }}\n    struct ncr_mutex_control_{} *control = p->control;\n    p->control = NULL;\n    ncr_mutex_release_{}(control);\n}}\n",
                    class.id, class.id, mutex, mutex));
            }
            Some(C::Channel(_)) => out.push_str(&format!(
                "static void ncr_concurrency_drop_{}(ncr_obj_{} *p) {{ ncr_channel_pair_drop_{}(p); }}\n",
                class.id, class.id, class.id)),
            Some(C::Sender(_)) => out.push_str(&format!(
                "static void ncr_concurrency_drop_{}(ncr_obj_{} *p) {{ ncr_sender_drop_{}(p); }}\n",
                class.id, class.id, class.id)),
            Some(C::Receiver(_)) => out.push_str(&format!(
                "static void ncr_concurrency_drop_{}(ncr_obj_{} *p) {{ ncr_receiver_drop_{}(p); }}\n",
                class.id, class.id, class.id)),
            _ => {}
        }
    }
    out
}

pub(super) fn implementations(
    program: &t::Program,
    native_method_ids: &mut Option<t::NativeMethodIndex>,
) -> String {
    let thread_runs = thread_run_index(program);
    let mut out = include_str!("../../stdlib/concurrency.c.in").to_owned();
    out.push_str(&control_definitions(program, &thread_runs));
    for class in &program.classes {
        match class.native {
            Some(C::Thread(_)) => out.push_str(&thread_worker(program, class.id, &thread_runs)),
            Some(C::Mutex(value)) => out.push_str(&mutex_control_helpers(program, class.id, value)),
            Some(C::Channel(value)) => out.push_str(&channel_control_helpers(program, class.id, value)),
            _ => {}
        }
    }
    out.push_str(&super::concurrency_extra::helpers(program, native_method_ids));
    for class in &program.classes {
        match class.native {
            Some(C::Channel(_)) => out.push_str(&channel_pair_drop(program, class.id)),
            Some(C::Sender(value)) => {
                let channel = channel_class(program, value);
                out.push_str(&channel_sender_drop(program, class.id, channel));
                out.push_str(&sender_error_helpers(program, class.id, native_method_ids));
            }
            Some(C::Receiver(value)) => {
                let channel = channel_class(program, value);
                out.push_str(&channel_receiver_drop(program, class.id, channel));
            }
            _ => {}
        }
    }
    out.push_str(&super::concurrency_extra::drop_helpers(program));
    out.push_str(&drop_helpers(program, &thread_runs));
    for function in &program.functions {
        let body = match function.native {
            Some(M::ThreadSpawn) => Some(thread_spawn(program, function, &thread_runs)),
            Some(M::ThreadJoin) => Some(thread_join(program, function)),
            Some(M::MutexCreate) => Some(mutex_create(program, function)),
            Some(M::MutexCloneHandle) => Some(mutex_clone(program, function)),
            Some(M::MutexLock) => Some(mutex_lock(program, function)),
            Some(M::MutexTryLock) => Some(mutex_try_lock(program, function)),
            Some(M::GuardGetRef) => Some(guard_access(program, function, false)),
            Some(M::GuardGetMut) => Some(guard_access(program, function, true)),
            Some(M::GuardRead) => Some(guard_read(program, function)),
            Some(M::GuardWrite) => Some(guard_write(program, function)),
            Some(M::ChannelBounded) => Some(channel_bounded(program, function)),
            Some(M::ChannelTakeSender) => Some(channel_take(program, function, true)),
            Some(M::ChannelTakeReceiver) => Some(channel_take(program, function, false)),
            Some(M::SenderSend) => Some(sender_send(program, function)),
            Some(M::SenderCloneSender) => Some(sender_clone(program, function)),
            Some(M::SenderClose) => Some(sender_close(program, function)),
            Some(M::ReceiverReceive) => Some(receiver_receive(program, function)),
            Some(M::ReceiverClose) => Some(receiver_close(program, function)),
            _ => None,
        };
        if let Some(body) = body { out.push_str(&body); }
    }
    out.push_str(&super::concurrency_extra::implementations(program));
    out
}
