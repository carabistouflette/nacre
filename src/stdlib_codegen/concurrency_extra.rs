use crate::codegen::ctype;
use crate::ir::{self as t, NativeClass as C, NativeMethod as M, Type};
use super::support::{result_id, variant_id};

fn unit_mutex(program: &t::Program) -> usize {
    program.classes.iter().find_map(|class| match class.native {
        Some(C::Mutex(Type::Unit)) => Some(class.id),
        _ => None,
    }).expect("internal Mutex<Unit> specialization is present for concurrency builtins")
}

fn matching_class(program: &t::Program, predicate: impl Fn(C) -> Option<Type>, value: Type) -> usize {
    program.classes.iter().find_map(|class| {
        class.native.and_then(|native| predicate(native).filter(|element| *element == value).map(|_| class.id))
    }).expect("matching concurrency specialization is present")
}


fn native_owner(program: &t::Program, function: &t::Function) -> usize {
    if let Some(receiver) = function.receiver { return receiver; }
    let result = result_id(function);
    let Type::Object(owner) = program.classes[result].result.expect("native factory returns Result").ok_type
        else { unreachable!("native factory result owns its handle") };
    owner
}

fn mutex_class(program: &t::Program, value: Type) -> usize {
    matching_class(program, |native| if let C::Mutex(element) = native { Some(element) } else { None }, value)
}

fn future_class(program: &t::Program, value: Type) -> usize {
    matching_class(program, |native| if let C::Future(element) = native { Some(element) } else { None }, value)
}


fn text_class(program: &t::Program) -> usize {
    program.classes.iter().find_map(|class|
        class.native.filter(|native| matches!(native, C::Text)).map(|_| class.id)
    ).expect("Text builtin is present")
}
fn token_class(program: &t::Program) -> usize {
    program.classes.iter().find_map(|class|
        class.native.filter(|native| matches!(native, C::CancellationToken)).map(|_| class.id)
    ).expect("CancellationToken builtin is present")
}

fn task_info(program: &t::Program, value: Type) -> (usize, usize, usize) {
    let text = Type::Object(text_class(program));
    for class in &program.classes {
        if !class.is_interface || !class.name.starts_with("CancelableTask<") { continue; }
        for method in &class.interface_methods {
            let function = &program.functions[*method];
            if !function.name.ends_with(".run") { continue; }
            let Type::Object(result) = function.return_type else { continue };
            if program.classes[result].result.is_some_and(|layout|
                layout.ok_type == value && layout.err_type == text) {
                return (class.id, *method, result);
            }
        }
    }
    unreachable!("CancelableTask<T>.run specialization is present")
}

fn pool_error_shape(program: &t::Program, function: &t::Function) -> (usize, usize, usize, usize, usize) {
    let result = result_id(function);
    let layout = program.classes[result].result.expect("ThreadPool submission Result layout");
    let Type::Object(submit_error) = layout.err_type else { unreachable!("SubmitError<T> is the submission error") };
    let closed = variant_id(program, submit_error, "Closed");
    let full = variant_id(program, submit_error, "Full");
    let system = variant_id(program, submit_error, "System");
    (result, submit_error, closed, full, system)
}

fn option_class(program: &t::Program, function: &t::Function) -> usize {
    let result = result_id(function);
    let Type::Object(option) = program.classes[result].result.expect("Future.await Result").ok_type
        else { unreachable!("Future.await returns Option<T>") };
    option
}

fn result_ok(program: &t::Program, function: &t::Function, value: &str) -> String {
    let id = result_id(function);
    let layout = program.classes[id].result.expect("trusted Result layout");
    format!("(ncr_obj_{id}){{ .tag = {}, .data.v{}.f0 = {value} }}", layout.ok_variant, layout.ok_variant)
}

fn result_error_literal(_program: &t::Program, function: &t::Function, message: &str) -> String {
    let id = result_id(function);
    format!("return ncr_result_error_{id}(\"{message}\", sizeof(\"{message}\") - 1, ncr_line, ncr_column);")
}

fn result_error_code(_program: &t::Program, function: &t::Function, operation: &str, code: &str) -> String {
    let id = result_id(function);
    format!("char ncr_error_message[128];\nsize_t ncr_error_length = ncr_sync_error_text(ncr_error_message, sizeof(ncr_error_message), \"{operation}\", {code});\nreturn ncr_result_error_{id}(ncr_error_message, ncr_error_length, ncr_line, ncr_column);")
}

fn result_error_value(program: &t::Program, function: &t::Function, value: &str) -> String {
    let id = result_id(function);
    let error = program.classes[id].result.expect("Result layout").err_variant;
    format!("(ncr_obj_{id}){{ .tag = {error}, .data.v{error}.f0 = {value} }}")
}

fn method_body(program: &t::Program, function: &t::Function, body: String) -> String {
    let mut params = Vec::new();
    if let Some(receiver) = function.receiver {
        params.push(format!("{} ncr_self", ctype(program, Type::Object(receiver))));
    }
    for (id, ty) in &function.params { params.push(format!("{} ncr_v_{id}", ctype(program, *ty))); }
    params.extend(["size_t ncr_line".to_owned(), "size_t ncr_column".to_owned()]);
    format!("static {} ncr_fn_{}({}) {{\n{}\n}}\n", ctype(program, function.return_type), function.id, params.join(", "), body)
}


fn is_extra(class: C) -> bool {
    matches!(class, C::Condition(_) | C::Future(_) | C::ThreadPool(_) |
        C::AtomicInt | C::AtomicLong | C::AtomicBoolean | C::Semaphore |
        C::CountDownLatch | C::Barrier | C::CancellationToken)
}

pub(super) fn fields(program: &t::Program, class: C, _class_id: usize) -> String {
    match class {
        C::AtomicInt | C::AtomicLong | C::AtomicBoolean | C::CancellationToken => {
            let unit = unit_mutex(program);
            format!("    struct ncr_mutex_control_{unit} *control;\n")
        }
        C::Semaphore | C::CountDownLatch | C::Barrier => {
            let unit = unit_mutex(program);
            format!("    struct ncr_mutex_control_{unit} *control;\n    struct ncr_condition_node *condition;\n")
        }
        C::Condition(value) => {
            let mutex = mutex_class(program, value);
            format!("    struct ncr_mutex_control_{mutex} *control;\n    struct ncr_condition_node *condition;\n")
        }
        C::Future(_) => {
            let unit = unit_mutex(program);
            format!(
                "    struct ncr_mutex_control_{unit} *state_control;\n    struct ncr_condition_node *state_condition;\n    struct ncr_mutex_control_{unit} *token_control;\n    struct ncr_mutex_control_{unit} *pool_control;\n    struct ncr_condition_node *pool_ready;\n    pthread_t thread;\n    bool standalone;\n    bool joined;\n"
            )
        }
        C::ThreadPool(_) => {
            let unit = unit_mutex(program);
            format!("    struct ncr_mutex_control_{unit} *control;\n")
        }
        _ => unreachable!("non-extra concurrency native fields"),
    }
}

pub(super) fn control_definitions(program: &t::Program) -> String {
    let mut out = String::new();
    for class in &program.classes {
        match class.native {
            Some(C::AtomicInt) => out.push_str(&format!("struct ncr_atomic_state_{} {{ _Atomic int32_t value; }};\n", class.id)),
            Some(C::AtomicLong) => out.push_str(&format!("struct ncr_atomic_state_{} {{ _Atomic int64_t value; }};\n", class.id)),
            Some(C::AtomicBoolean) => out.push_str(&format!("struct ncr_atomic_state_{} {{ _Atomic bool value; }};\n", class.id)),
            Some(C::Semaphore) => out.push_str(&format!("struct ncr_semaphore_state_{} {{ uint64_t permits; }};\n", class.id)),
            Some(C::CountDownLatch) => out.push_str(&format!("struct ncr_latch_state_{} {{ uint64_t count; }};\n", class.id)),
            Some(C::Barrier) => out.push_str(&format!("struct ncr_barrier_state_{} {{ int32_t parties; int32_t arrived; uint64_t generation; uint64_t waiters; bool broken; }};\n", class.id)),
            Some(C::Future(value)) => {
                let (_, _, outcome) = task_info(program, value);
                out.push_str(&format!(
                    "struct ncr_future_state_{} {{ bool done; bool consumed; ncr_obj_{outcome} outcome; }};\n",
                    class.id));
                let (task, _, _) = task_info(program, value);
                out.push_str(&format!(
                    "struct ncr_future_context_{} {{ ncr_obj_{task} task; struct ncr_mutex_control_{} *state_control; struct ncr_condition_node *state_condition; struct ncr_mutex_control_{} *token_control; }};\n",
                    class.id, unit_mutex(program), unit_mutex(program)));
            }
            Some(C::ThreadPool(value)) => {
                let (task, _, _) = task_info(program, value);
                let unit = unit_mutex(program);
                out.push_str(&format!(
                    "struct ncr_pool_work_{} {{ ncr_obj_{task} task; struct ncr_mutex_control_{unit} *state_control; struct ncr_condition_node *state_condition; struct ncr_mutex_control_{unit} *token_control; int64_t due_ns; int64_t interval_ns; bool recurring; bool fixed_rate; }};\n",
                    class.id));
                out.push_str(&format!(
                    "struct ncr_pool_state_{} {{ struct ncr_pool_work_{0} *queue; pthread_t *workers; bool *joined; size_t queue_capacity; size_t queue_count; size_t active_recurring; size_t worker_capacity; size_t worker_count; bool shutting_down; bool drain; struct ncr_condition_node *ready; }};\n",
                    class.id));
            }
            _ => {}
        }
    }
    if native_present(program, |native| matches!(native, C::CancellationToken | C::Future(_) | C::ThreadPool(_))) {
        out.push_str("struct ncr_cancel_state { bool cancelled; };\n");
    }
    out
}
fn native_present(program: &t::Program, predicate: impl Fn(C) -> bool) -> bool {
    program.classes.iter().any(|class| class.native.is_some_and(&predicate))
}

fn cancel_helpers(program: &t::Program) -> String {
    if !native_present(program, |native| matches!(native, C::CancellationToken | C::Future(_) | C::ThreadPool(_))) { return String::new(); }
    let unit = unit_mutex(program);
    format!(r#"
static void ncr_cancel_state_drop(void *raw) {{ ncr_deallocate(raw); }}
static struct ncr_mutex_control_{unit} *ncr_cancel_control_new(int *failure) {{
    struct ncr_mutex_control_{unit} *control = ncr_sync_mutex_control_new_{unit}(failure);
    if (control == NULL) {{ return NULL; }}
    struct ncr_cancel_state *state = ncr_allocate(sizeof(*state), 0, 0);
    state->cancelled = false;
    control->aux_payload = state;
    control->aux_drop = ncr_cancel_state_drop;
    return control;
}}
static int ncr_cancel_request_control(struct ncr_mutex_control_{unit} *control) {{
    int error = pthread_mutex_lock(&control->mutex);
    if (error != 0) {{ return error; }}
    ((struct ncr_cancel_state *)control->aux_payload)->cancelled = true;
    return pthread_mutex_unlock(&control->mutex);
}}
static int ncr_cancel_read_control(struct ncr_mutex_control_{unit} *control, bool *cancelled) {{
    int error = pthread_mutex_lock(&control->mutex);
    if (error != 0) {{ return error; }}
    *cancelled = ((struct ncr_cancel_state *)control->aux_payload)->cancelled;
    return pthread_mutex_unlock(&control->mutex);
}}
static bool ncr_cancel_requested_internal(struct ncr_mutex_control_{unit} *control) {{
    bool cancelled = false;
    int error = ncr_cancel_read_control(control, &cancelled);
    if (error != 0) {{ ncr_sync_fatal("CancellationToken read in worker", error); }}
    return cancelled;
}}
"#)
}

fn state_helpers(program: &t::Program) -> String {
    let needs_unit = program.classes.iter().any(|class|
        class.native.is_some() && crate::stdlib::uses_unit_mutex(&class.name));
    if !needs_unit { return String::new(); }
    let mut out = String::new();
    let unit = unit_mutex(program);
    for class in &program.classes {
        match class.native {
            Some(C::AtomicInt | C::AtomicLong | C::AtomicBoolean) =>
                out.push_str(&format!("static void ncr_atomic_state_drop_{}(void *raw) {{ ncr_deallocate(raw); }}\n", class.id)),
            Some(C::Semaphore) =>
                out.push_str(&format!("static void ncr_semaphore_state_drop_{}(void *raw) {{ ncr_deallocate(raw); }}\n", class.id)),
            Some(C::CountDownLatch) =>
                out.push_str(&format!("static void ncr_latch_state_drop_{}(void *raw) {{ ncr_deallocate(raw); }}\n", class.id)),
            Some(C::Barrier) =>
                out.push_str(&format!("static void ncr_barrier_state_drop_{}(void *raw) {{ ncr_deallocate(raw); }}\n", class.id)),
            _ => {}
        }
    }
    out.push_str(&cancel_helpers(program));
    for class in &program.classes {
        if let Some(C::Future(value)) = class.native {
            let (_, _, result) = task_info(program, value);
            out.push_str(&format!(
                "static void ncr_future_state_drop_{}(void *raw) {{ struct ncr_future_state_{} *state = raw; if (state->outcome.tag >= 0) {{ ncr_drop_{}(state->outcome); }} ncr_deallocate(state); }}\n",
                class.id, class.id, result));
            out.push_str(&future_state_helpers(program, class.id, value, unit));
        }
    }
    for class in &program.classes {
        if let Some(C::ThreadPool(value)) = class.native {
            out.push_str(&pool_helpers(program, class.id, value, unit));
        }
    }
    out
}

fn future_state_helpers(program: &t::Program, id: usize, value: Type, unit: usize) -> String {
    let (task, run, result) = task_info(program, value);
    let slot = program.functions[run].virtual_slot.expect("CancelableTask.run vtable slot");
    let layout = program.classes[result].result.expect("CancelableTask returns Result<T,Text>");
    let text = text_class(program);
    let token = token_class(program);
    let err_tag = layout.err_variant;
    format!(r#"
static int ncr_future_handles_new_{id}(struct ncr_mutex_control_{unit} **state_control,
        struct ncr_condition_node **condition, struct ncr_mutex_control_{unit} **token_control) {{
    int error = 0;
    *state_control = ncr_sync_mutex_control_new_{unit}(&error);
    if (*state_control == NULL) {{ return error; }}
    struct ncr_future_state_{id} *state = ncr_allocate(sizeof(*state), 0, 0);
    state->done = false;
    state->consumed = false;
    state->outcome.tag = -1;
    (*state_control)->aux_payload = state;
    (*state_control)->aux_drop = ncr_future_state_drop_{id};
    *condition = ncr_mutex_condition_new_{unit}(*state_control, &error);
    if (*condition == NULL) {{
        ncr_mutex_release_{unit}(*state_control);
        *state_control = NULL;
        return error;
    }}
    *token_control = ncr_cancel_control_new(&error);
    if (*token_control == NULL) {{
        ncr_mutex_release_{unit}(*state_control);
        *state_control = NULL;
        *condition = NULL;
        return error;
    }}
    return 0;
}}

static void ncr_future_publish_{id}(struct ncr_mutex_control_{unit} *control,
        struct ncr_condition_node *condition, ncr_obj_{result} outcome) {{
    int error = pthread_mutex_lock(&control->mutex);
    if (error != 0) {{ ncr_sync_fatal("pthread_mutex_lock(Future publish)", error); }}
    struct ncr_future_state_{id} *state = control->aux_payload;
    if (state->done) {{ ncr_sync_fatal("Future result published twice", EINVAL); }}
    state->outcome = outcome;
    state->done = true;
    error = pthread_cond_broadcast(&condition->condition);
    if (error != 0) {{ ncr_sync_fatal("pthread_cond_broadcast(Future publish)", error); }}
    error = pthread_mutex_unlock(&control->mutex);
    if (error != 0) {{ ncr_sync_fatal("pthread_mutex_unlock(Future publish)", error); }}
}}

static ncr_obj_{result} ncr_future_cancelled_result_{id}(size_t line, size_t column) {{
    ncr_obj_{text} *message = ncr_text_from_bytes_{text}(
        (const unsigned char *)"task cancelled before it started",
        sizeof("task cancelled before it started") - 1, line, column);
    return (ncr_obj_{result}){{ .tag = {err_tag}, .data.v{err_tag}.f0 = message }};
}}

static void *ncr_future_worker_{id}(void *raw) {{
    struct ncr_future_context_{id} *context = raw;
    ncr_obj_{token} token = {{ .control = context->token_control }};
    ncr_obj_{result} outcome = context->task.vtable->m{slot}(context->task.data, &token);
    ncr_drop_{task}(context->task);
    ncr_future_publish_{id}(context->state_control, context->state_condition, outcome);
    ncr_mutex_release_{unit}(context->state_control);
    ncr_mutex_release_{unit}(context->token_control);
    ncr_deallocate(context);
    return NULL;
}}
"#)
}
fn pool_helpers(program: &t::Program, id: usize, value: Type, unit: usize) -> String {
    let (task, run, future_result) = task_info(program, value);
    let slot = program.functions[run].virtual_slot.expect("CancelableTask.run vtable slot");
    let future = future_class(program, value);
    let token = token_class(program);
    let mut out = pool_error_helpers(program, id, value);
    out.push_str(&pool_recurring_helpers(program, id, value, unit));
    out.push_str(&format!(r#"
static void ncr_pool_state_drop_{id}(void *raw) {{
    struct ncr_pool_state_{id} *state = raw;
    if (state->queue_count != 0 || state->active_recurring != 0) {{ ncr_sync_fatal("ThreadPool released with pending work", EINVAL); }}
    for (size_t i = 0; i < state->worker_count; ++i) {{
        if (!state->joined[i]) {{ ncr_sync_fatal("ThreadPool released with a live worker", EINVAL); }}
    }}
    ncr_deallocate(state->queue);
    ncr_deallocate(state->joined);
    ncr_deallocate(state->workers);
    ncr_deallocate(state);
}}

static void ncr_pool_take_{id}(struct ncr_pool_state_{id} *state, size_t index,
        struct ncr_pool_work_{id} *work) {{
    *work = state->queue[index];
    for (size_t i = index + 1; i < state->queue_count; ++i) {{
        state->queue[i - 1] = state->queue[i];
    }}
    --state->queue_count;
    memset(&state->queue[state->queue_count], 0, sizeof(state->queue[state->queue_count]));
}}

static void ncr_pool_finish_{id}(struct ncr_pool_work_{id} *work, bool force_cancel) {{
    ncr_obj_{token} token = {{ .control = work->token_control }};
    ncr_obj_{future_result} outcome;
    if (force_cancel) {{
        int error = ncr_cancel_request_control(work->token_control);
        if (error != 0) {{ ncr_sync_fatal("CancellationToken request during pool shutdown", error); }}
    }}
    if (force_cancel || ncr_cancel_requested_internal(work->token_control)) {{
        ncr_drop_{task}(work->task);
        if (work->recurring) {{
            outcome = ncr_pool_recurring_cancelled_{id}();
        }} else {{
            outcome = ncr_future_cancelled_result_{future}(0, 0);
        }}
    }} else {{
        outcome = work->task.vtable->m{slot}(work->task.data, &token);
        ncr_drop_{task}(work->task);
    }}
    ncr_future_publish_{future}(work->state_control, work->state_condition, outcome);
    ncr_mutex_release_{unit}(work->state_control);
    ncr_mutex_release_{unit}(work->token_control);
}}

static void *ncr_pool_worker_{id}(void *raw) {{
    struct ncr_mutex_control_{unit} *control = raw;
    struct ncr_pool_state_{id} *state = control->aux_payload;
    for (;;) {{
        int error = pthread_mutex_lock(&control->mutex);
        if (error != 0) {{ ncr_sync_fatal("pthread_mutex_lock(ThreadPool worker)", error); }}
        if (state->shutting_down && !state->drain) {{
            error = pthread_mutex_unlock(&control->mutex);
            if (error != 0) {{ ncr_sync_fatal("pthread_mutex_unlock(ThreadPool worker exit)", error); }}
            ncr_mutex_release_{unit}(control);
            return NULL;
        }}
        if (state->queue_count == 0) {{
            if (state->shutting_down) {{
                error = pthread_mutex_unlock(&control->mutex);
                if (error != 0) {{ ncr_sync_fatal("pthread_mutex_unlock(ThreadPool drained)", error); }}
                ncr_mutex_release_{unit}(control);
                return NULL;
            }}
            error = pthread_cond_wait(&state->ready->condition, &control->mutex);
            if (error != 0) {{ ncr_sync_fatal("pthread_cond_wait(ThreadPool worker)", error); }}
            error = pthread_mutex_unlock(&control->mutex);
            if (error != 0) {{ ncr_sync_fatal("pthread_mutex_unlock(ThreadPool worker wait)", error); }}
            continue;
        }}
        size_t selected = SIZE_MAX;
        for (size_t i = 0; i < state->queue_count; ++i) {{
            if (ncr_cancel_requested_internal(state->queue[i].token_control) ||
                    (state->shutting_down && state->drain && state->queue[i].recurring)) {{
                selected = i;
                break;
            }}
        }}
        if (selected == SIZE_MAX) {{
            selected = 0;
            for (size_t i = 1; i < state->queue_count; ++i) {{
                if (state->queue[i].due_ns < state->queue[selected].due_ns) {{ selected = i; }}
            }}
            int64_t now;
            error = ncr_sync_monotonic_now(&now);
            if (error != 0) {{ ncr_sync_fatal("clock_gettime(ThreadPool worker)", error); }}
            if (state->queue[selected].due_ns > now) {{
                struct timespec deadline;
                error = ncr_sync_timespec_from_monotonic_ns(state->queue[selected].due_ns, &deadline);
                if (error != 0) {{ ncr_sync_fatal("ThreadPool delayed deadline", error); }}
                error = pthread_cond_timedwait(&state->ready->condition, &control->mutex, &deadline);
                if (error != 0 && error != ETIMEDOUT) {{ ncr_sync_fatal("pthread_cond_timedwait(ThreadPool worker)", error); }}
                error = pthread_mutex_unlock(&control->mutex);
                if (error != 0) {{ ncr_sync_fatal("pthread_mutex_unlock(ThreadPool delayed wait)", error); }}
                continue;
            }}
        }}
        struct ncr_pool_work_{id} work;
        ncr_pool_take_{id}(state, selected, &work);
        bool stop_recurring = work.recurring && state->shutting_down;
        if (work.recurring && !stop_recurring) {{ ++state->active_recurring; }}
        error = pthread_mutex_unlock(&control->mutex);
        if (error != 0) {{ ncr_sync_fatal("pthread_mutex_unlock(ThreadPool take)", error); }}
        if (work.recurring && !stop_recurring) {{
            ncr_pool_recurring_run_{id}(&work, control);
        }} else {{
            ncr_pool_finish_{id}(&work, stop_recurring);
        }}
    }}
}}

static int ncr_pool_shutdown_{id}(struct ncr_mutex_control_{unit} *control, bool drain) {{
    struct ncr_pool_state_{id} *state = control->aux_payload;
    int error = pthread_mutex_lock(&control->mutex);
    if (error != 0) {{ return error; }}
    if (!state->shutting_down) {{
        state->shutting_down = true;
        state->drain = drain;
    }}
    if (!state->drain) {{
        while (state->queue_count != 0) {{
            struct ncr_pool_work_{id} work;
            ncr_pool_take_{id}(state, 0, &work);
            error = pthread_mutex_unlock(&control->mutex);
            if (error != 0) {{ ncr_sync_fatal("pthread_mutex_unlock(ThreadPool cancel queued)", error); }}
            ncr_pool_finish_{id}(&work, true);
            error = pthread_mutex_lock(&control->mutex);
            if (error != 0) {{ return error; }}
        }}
    }}
    error = pthread_cond_broadcast(&state->ready->condition);
    int unlock_error = pthread_mutex_unlock(&control->mutex);
    if (unlock_error != 0) {{ ncr_sync_fatal("pthread_mutex_unlock(ThreadPool shutdown)", unlock_error); }}
    if (error != 0) {{ return error; }}
    for (size_t i = 0; i < state->worker_count; ++i) {{
        if (state->joined[i]) {{ continue; }}
        if (pthread_equal(pthread_self(), state->workers[i])) {{ return EDEADLK; }}
        error = pthread_join(state->workers[i], NULL);
        if (error != 0) {{ return error; }}
        state->joined[i] = true;
    }}
    return 0;
}}
 "#));
    out
}

fn atomic_create(program: &t::Program, function: &t::Function) -> String {
    let id = native_owner(program, function);
    let unit = unit_mutex(program);
    let arg = function.params[0].0;
    let init = match program.classes[id].native.unwrap() {
        C::AtomicInt | C::AtomicLong | C::AtomicBoolean => "atomic_init(&state->value, ncr_v_ARG);",
        _ => unreachable!("atomic constructor"),
    };
    let state_name = format!("ncr_atomic_state_{id}");
    let state_init = init.replace("ARG", &arg.to_string());
    let body = format!(
        "    int error = 0;\n    struct ncr_mutex_control_{unit} *control = ncr_sync_mutex_control_new_{unit}(&error);\n    if (control == NULL) {{\n        {}\n    }}\n    struct {state_name} *state = ncr_allocate(sizeof(*state), ncr_line, ncr_column);\n    {state_init}\n    control->aux_payload = state;\n    control->aux_drop = ncr_atomic_state_drop_{id};\n    ncr_obj_{id} *handle = ncr_allocate(sizeof(*handle), ncr_line, ncr_column);\n    handle->control = control;\n    return {};",
        result_error_code(program, function, "pthread_mutex_init(atomic control)", "error"),
        result_ok(program, function, "handle"));
    method_body(program, function, body)
}

fn atomic_method(program: &t::Program, function: &t::Function) -> String {
    let id = function.receiver.expect("atomic instance method receiver");
    let state_type = format!("struct ncr_atomic_state_{id}");
    let native = program.classes[id].native.expect("atomic specialization");
    let value_type = match native {
        C::AtomicInt => "int32_t",
        C::AtomicLong => "int64_t",
        C::AtomicBoolean => "bool",
        _ => unreachable!("atomic method"),
    };
    let state = format!("{state_type} *state = ncr_self->control->aux_payload;\n");
    let body = match function.native.unwrap() {
        M::AtomicCloneHandle => format!(
            "    struct ncr_mutex_control_{} *control = ncr_self->control;\n    if (!ncr_sync_ref_acquire(&control->references)) {{ {} }}\n    ncr_obj_{id} *handle = ncr_allocate(sizeof(*handle), ncr_line, ncr_column);\n    handle->control = control;\n    return {};",
            unit_mutex(program),
            result_error_literal(program, function, "Atomic handle reference count overflow"),
            result_ok(program, function, "handle")),
        M::AtomicLoad => format!(
            "    {state}    return atomic_load_explicit(&state->value, memory_order_seq_cst);"),
        M::AtomicStore => {
            let arg = function.params[0].0;
            format!(
                "    {state}    atomic_store_explicit(&state->value, ncr_v_{arg}, memory_order_seq_cst);\n    return UINT8_C(0);")
        }
        M::AtomicExchange => {
            let arg = function.params[0].0;
            format!(
                "    {state}    return atomic_exchange_explicit(&state->value, ncr_v_{arg}, memory_order_seq_cst);")
        }
        M::AtomicCompareExchange => {
            let expected = function.params[0].0;
            let desired = function.params[1].0;
            format!(
                "    {state}    {value_type} observed = ncr_v_{expected};\n    (void)atomic_compare_exchange_strong_explicit(&state->value, &observed, ncr_v_{desired}, memory_order_seq_cst, memory_order_seq_cst);\n    return observed;")
        }
        M::AtomicFetchAdd | M::AtomicFetchSubtract => {
            let arg = function.params[0].0;
            let operation = if function.native == Some(M::AtomicFetchAdd) { "atomic_fetch_add_explicit" } else { "atomic_fetch_sub_explicit" };
            format!(
                "    {state}    return {operation}(&state->value, ncr_v_{arg}, memory_order_seq_cst);")
        }
        _ => unreachable!("atomic method dispatch"),
    };
    method_body(program, function, body)
}

fn synchronizer_clone(program: &t::Program, function: &t::Function) -> String {
    let id = function.receiver.expect("synchronizer receiver");
    let control_type = match program.classes[id].native.unwrap() {
        C::Condition(value) => mutex_class(program, value),
        _ => unit_mutex(program),
    };
    let body = format!(
        "    struct ncr_mutex_control_{control_type} *control = ncr_self->control;\n    if (!ncr_sync_ref_acquire(&control->references)) {{ {} }}\n    ncr_obj_{id} *handle = ncr_allocate(sizeof(*handle), ncr_line, ncr_column);\n    handle->control = control;\n    handle->condition = ncr_self->condition;\n    return {};",
        result_error_literal(program, function, "synchronizer handle reference count overflow"),
        result_ok(program, function, "handle"));
    method_body(program, function, body)
}

fn semaphore_create(program: &t::Program, function: &t::Function) -> String {
    let id = native_owner(program, function);
    let unit = unit_mutex(program);
    let arg = function.params[0].0;
    let body = format!(
        "    if (ncr_v_{arg} < 0) {{ {} }}\n    int error = 0;\n    struct ncr_mutex_control_{unit} *control = ncr_sync_mutex_control_new_{unit}(&error);\n    if (control == NULL) {{ {} }}\n    struct ncr_semaphore_state_{id} *state = ncr_allocate(sizeof(*state), ncr_line, ncr_column);\n    state->permits = (uint64_t)ncr_v_{arg};\n    control->aux_payload = state;\n    control->aux_drop = ncr_semaphore_state_drop_{id};\n    struct ncr_condition_node *condition = ncr_mutex_condition_new_{unit}(control, &error);\n    if (condition == NULL) {{ ncr_mutex_release_{unit}(control); {} }}\n    ncr_obj_{id} *handle = ncr_allocate(sizeof(*handle), ncr_line, ncr_column);\n    handle->control = control;\n    handle->condition = condition;\n    return {};",
        result_error_literal(program, function, "Semaphore permits must be nonnegative"),
        result_error_code(program, function, "pthread_mutex_init(Semaphore)", "error"),
        result_error_code(program, function, "pthread_cond_init(Semaphore)", "error"),
        result_ok(program, function, "handle"));
    method_body(program, function, body)
}

fn semaphore_acquire(program: &t::Program, function: &t::Function) -> String {
    let id = function.receiver.unwrap();
    let timeout = function.params[0].0;
    let body = format!(
        "    if (ncr_v_{timeout} < 0) {{ {} }}\n    struct timespec deadline;\n    int error = 0;\n    if (ncr_v_{timeout} > 0) {{\n        error = ncr_sync_deadline_after_millis(ncr_v_{timeout}, &deadline);\n        if (error != 0) {{ {} }}\n    }}\n    struct ncr_mutex_control_{} *control = ncr_self->control;\n    error = pthread_mutex_lock(&control->mutex);\n    if (error != 0) {{ {} }}\n    struct ncr_semaphore_state_{id} *state = control->aux_payload;\n    while (state->permits == 0) {{\n        if (ncr_v_{timeout} == 0) {{\n            error = pthread_mutex_unlock(&control->mutex);\n            if (error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(Semaphore try acquire)\", error); }}\n            return {};\n        }}\n        error = pthread_cond_timedwait(&ncr_self->condition->condition, &control->mutex, &deadline);\n        if (error == ETIMEDOUT && state->permits == 0) {{\n            error = pthread_mutex_unlock(&control->mutex);\n            if (error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(Semaphore timeout)\", error); }}\n            return {};\n        }}\n        if (error != 0 && error != ETIMEDOUT) {{\n            int unlock_error = pthread_mutex_unlock(&control->mutex);\n            if (unlock_error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(Semaphore wait error)\", unlock_error); }}\n            {}\n        }}\n    }}\n    --state->permits;\n    error = pthread_mutex_unlock(&control->mutex);\n    if (error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(Semaphore acquire)\", error); }}\n    return {};",
        result_error_literal(program, function, "timeout must be nonnegative"),
        result_error_code(program, function, "monotonic deadline", "error"),
        unit_mutex(program),
        result_error_code(program, function, "pthread_mutex_lock(Semaphore acquire)", "error"),
        result_ok(program, function, "false"),
        result_ok(program, function, "false"),
        result_error_code(program, function, "pthread_cond_timedwait(Semaphore acquire)", "error"),
        result_ok(program, function, "true"));
    method_body(program, function, body)
}

fn semaphore_release(program: &t::Program, function: &t::Function) -> String {
    let id = function.receiver.unwrap();
    let count = function.params[0].0;
    let body = format!(
        "    if (ncr_v_{count} < 0) {{ {} }}\n    struct ncr_mutex_control_{} *control = ncr_self->control;\n    int error = pthread_mutex_lock(&control->mutex);\n    if (error != 0) {{ {} }}\n    struct ncr_semaphore_state_{id} *state = control->aux_payload;\n    if ((uint64_t)ncr_v_{count} > (uint64_t)INT64_MAX - state->permits) {{\n        int unlock_error = pthread_mutex_unlock(&control->mutex);\n        if (unlock_error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(Semaphore overflow)\", unlock_error); }}\n        {}\n    }}\n    state->permits += (uint64_t)ncr_v_{count};\n    if (ncr_v_{count} > 0) {{\n        error = pthread_cond_broadcast(&ncr_self->condition->condition);\n        if (error != 0) {{\n            state->permits -= (uint64_t)ncr_v_{count};\n            int unlock_error = pthread_mutex_unlock(&control->mutex);\n            if (unlock_error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(Semaphore release error)\", unlock_error); }}\n            {}\n        }}\n    }}\n    error = pthread_mutex_unlock(&control->mutex);\n    if (error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(Semaphore release)\", error); }}\n    return {};",
        result_error_literal(program, function, "Semaphore release count must be nonnegative"),
        unit_mutex(program),
        result_error_code(program, function, "pthread_mutex_lock(Semaphore release)", "error"),
        result_error_literal(program, function, "Semaphore permit count overflow"),
        result_error_code(program, function, "pthread_cond_broadcast(Semaphore release)", "error"),
        result_ok(program, function, "UINT8_C(0)"));
    method_body(program, function, body)
}

fn latch_create(program: &t::Program, function: &t::Function) -> String {
    let id = native_owner(program, function);
    let unit = unit_mutex(program);
    let arg = function.params[0].0;
    let body = format!(
        "    if (ncr_v_{arg} < 0) {{ {} }}\n    int error = 0;\n    struct ncr_mutex_control_{unit} *control = ncr_sync_mutex_control_new_{unit}(&error);\n    if (control == NULL) {{ {} }}\n    struct ncr_latch_state_{id} *state = ncr_allocate(sizeof(*state), ncr_line, ncr_column);\n    state->count = (uint64_t)ncr_v_{arg};\n    control->aux_payload = state;\n    control->aux_drop = ncr_latch_state_drop_{id};\n    struct ncr_condition_node *condition = ncr_mutex_condition_new_{unit}(control, &error);\n    if (condition == NULL) {{ ncr_mutex_release_{unit}(control); {} }}\n    ncr_obj_{id} *handle = ncr_allocate(sizeof(*handle), ncr_line, ncr_column);\n    handle->control = control;\n    handle->condition = condition;\n    return {};",
        result_error_literal(program, function, "CountDownLatch count must be nonnegative"),
        result_error_code(program, function, "pthread_mutex_init(CountDownLatch)", "error"),
        result_error_code(program, function, "pthread_cond_init(CountDownLatch)", "error"),
        result_ok(program, function, "handle"));
    method_body(program, function, body)
}

fn latch_count_down(program: &t::Program, function: &t::Function) -> String {
    let id = function.receiver.unwrap();
    let unit = unit_mutex(program);
    let body = format!(
        "    struct ncr_mutex_control_{unit} *control = ncr_self->control;\n    int error = pthread_mutex_lock(&control->mutex);\n    if (error != 0) {{ {} }}\n    struct ncr_latch_state_{id} *state = control->aux_payload;\n    if (state->count > 0) {{\n        --state->count;\n        if (state->count == 0) {{\n            error = pthread_cond_broadcast(&ncr_self->condition->condition);\n            if (error != 0) {{\n                ++state->count;\n                int unlock_error = pthread_mutex_unlock(&control->mutex);\n                if (unlock_error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(CountDownLatch broadcast)\", unlock_error); }}\n                {}\n            }}\n        }}\n    }}\n    error = pthread_mutex_unlock(&control->mutex);\n    if (error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(CountDownLatch countDown)\", error); }}\n    return {};",
        result_error_code(program, function, "pthread_mutex_lock(CountDownLatch countDown)", "error"),
        result_error_code(program, function, "pthread_cond_broadcast(CountDownLatch)", "error"),
        result_ok(program, function, "UINT8_C(0)"));
    method_body(program, function, body)
}

fn latch_wait(program: &t::Program, function: &t::Function) -> String {
    let id = function.receiver.unwrap();
    let timeout = function.params[0].0;
    let body = format!(
        "    if (ncr_v_{timeout} < 0) {{ {} }}\n    struct timespec deadline;\n    int error = 0;\n    if (ncr_v_{timeout} > 0) {{\n        error = ncr_sync_deadline_after_millis(ncr_v_{timeout}, &deadline);\n        if (error != 0) {{ {} }}\n    }}\n    struct ncr_mutex_control_{} *control = ncr_self->control;\n    error = pthread_mutex_lock(&control->mutex);\n    if (error != 0) {{ {} }}\n    struct ncr_latch_state_{id} *state = control->aux_payload;\n    while (state->count != 0) {{\n        if (ncr_v_{timeout} == 0) {{\n            error = pthread_mutex_unlock(&control->mutex);\n            if (error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(CountDownLatch try wait)\", error); }}\n            return {};\n        }}\n        error = pthread_cond_timedwait(&ncr_self->condition->condition, &control->mutex, &deadline);\n        if (error == ETIMEDOUT && state->count != 0) {{\n            error = pthread_mutex_unlock(&control->mutex);\n            if (error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(CountDownLatch timeout)\", error); }}\n            return {};\n        }}\n        if (error != 0 && error != ETIMEDOUT) {{\n            int unlock_error = pthread_mutex_unlock(&control->mutex);\n            if (unlock_error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(CountDownLatch wait error)\", unlock_error); }}\n            {}\n        }}\n    }}\n    error = pthread_mutex_unlock(&control->mutex);\n    if (error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(CountDownLatch wait)\", error); }}\n    return {};",
        result_error_literal(program, function, "timeout must be nonnegative"),
        result_error_code(program, function, "monotonic deadline", "error"),
        unit_mutex(program),
        result_error_code(program, function, "pthread_mutex_lock(CountDownLatch wait)", "error"),
        result_ok(program, function, "false"),
        result_ok(program, function, "false"),
        result_error_code(program, function, "pthread_cond_timedwait(CountDownLatch wait)", "error"),
        result_ok(program, function, "true"));
    method_body(program, function, body)
}

fn latch_count(program: &t::Program, function: &t::Function) -> String {
    let id = function.receiver.unwrap();
    let unit = unit_mutex(program);
    method_body(program, function, format!(
        "    struct ncr_mutex_control_{unit} *control = ncr_self->control;\n    int error = pthread_mutex_lock(&control->mutex);\n    if (error != 0) {{ ncr_sync_fatal(\"pthread_mutex_lock(CountDownLatch count)\", error); }}\n    int64_t count = (int64_t)((struct ncr_latch_state_{id} *)control->aux_payload)->count;\n    error = pthread_mutex_unlock(&control->mutex);\n    if (error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(CountDownLatch count)\", error); }}\n    return count;"))
}

fn barrier_create(program: &t::Program, function: &t::Function) -> String {
    let id = native_owner(program, function);
    let unit = unit_mutex(program);
    let arg = function.params[0].0;
    let body = format!(
        "    if (ncr_v_{arg} < 1) {{ {} }}\n    int error = 0;\n    struct ncr_mutex_control_{unit} *control = ncr_sync_mutex_control_new_{unit}(&error);\n    if (control == NULL) {{ {} }}\n    struct ncr_barrier_state_{id} *state = ncr_allocate(sizeof(*state), ncr_line, ncr_column);\n    state->parties = ncr_v_{arg};\n    state->arrived = 0;\n    state->generation = 0;\n    state->waiters = 0;\n    state->broken = false;\n    control->aux_payload = state;\n    control->aux_drop = ncr_barrier_state_drop_{id};\n    struct ncr_condition_node *condition = ncr_mutex_condition_new_{unit}(control, &error);\n    if (condition == NULL) {{ ncr_mutex_release_{unit}(control); {} }}\n    ncr_obj_{id} *handle = ncr_allocate(sizeof(*handle), ncr_line, ncr_column);\n    handle->control = control;\n    handle->condition = condition;\n    return {};",
        result_error_literal(program, function, "Barrier parties must be positive"),
        result_error_code(program, function, "pthread_mutex_init(Barrier)", "error"),
        result_error_code(program, function, "pthread_cond_init(Barrier)", "error"),
        result_ok(program, function, "handle"));
    method_body(program, function, body)
}

fn barrier_wait(program: &t::Program, function: &t::Function) -> String {
    let id = function.receiver.unwrap();
    let unit = unit_mutex(program);
    let timeout = function.params[0].0;
    let body = format!(r#"    if (ncr_v_{timeout} < 0) {{ {negative_timeout} }}
    struct timespec deadline;
    int error = 0;
    if (ncr_v_{timeout} > 0) {{
        error = ncr_sync_deadline_after_millis(ncr_v_{timeout}, &deadline);
        if (error != 0) {{ {deadline_error} }}
    }}
    struct ncr_mutex_control_{unit} *control = ncr_self->control;
    error = pthread_mutex_lock(&control->mutex);
    if (error != 0) {{ {lock_error} }}
    struct ncr_barrier_state_{id} *state = control->aux_payload;
    if (state->broken) {{
        error = pthread_mutex_unlock(&control->mutex);
        if (error != 0) {{ ncr_sync_fatal("pthread_mutex_unlock(Barrier broken)", error); }}
        {already_broken}
    }}
    int32_t arrival_index = state->parties - state->arrived - 1;
    ++state->arrived;
    if (state->arrived == state->parties) {{
        state->arrived = 0;
        if (state->generation == UINT64_MAX) {{
            state->broken = true;
            error = pthread_cond_broadcast(&ncr_self->condition->condition);
            if (error != 0) {{ ncr_sync_fatal("pthread_cond_broadcast(Barrier overflow)", error); }}
            error = pthread_mutex_unlock(&control->mutex);
            if (error != 0) {{ ncr_sync_fatal("pthread_mutex_unlock(Barrier overflow)", error); }}
            {generation_overflow}
        }}
        ++state->generation;
        error = pthread_cond_broadcast(&ncr_self->condition->condition);
        if (error != 0) {{ ncr_sync_fatal("pthread_cond_broadcast(Barrier release)", error); }}
        error = pthread_mutex_unlock(&control->mutex);
        if (error != 0) {{ ncr_sync_fatal("pthread_mutex_unlock(Barrier release)", error); }}
        return {last_arrival};
    }}
    uint64_t generation = state->generation;
    ++state->waiters;
    if (ncr_v_{timeout} == 0) {{
        state->broken = true;
        state->arrived = 0;
        --state->waiters;
        error = pthread_cond_broadcast(&ncr_self->condition->condition);
        if (error != 0) {{ ncr_sync_fatal("pthread_cond_broadcast(Barrier try timeout)", error); }}
        error = pthread_mutex_unlock(&control->mutex);
        if (error != 0) {{ ncr_sync_fatal("pthread_mutex_unlock(Barrier try timeout)", error); }}
        {wait_timeout}
    }}
    while (!state->broken && state->generation == generation) {{
        error = pthread_cond_timedwait(&ncr_self->condition->condition, &control->mutex, &deadline);
        if (error == ETIMEDOUT && state->generation == generation && !state->broken) {{
            state->broken = true;
            state->arrived = 0;
            --state->waiters;
            error = pthread_cond_broadcast(&ncr_self->condition->condition);
            if (error != 0) {{ ncr_sync_fatal("pthread_cond_broadcast(Barrier timeout)", error); }}
            error = pthread_mutex_unlock(&control->mutex);
            if (error != 0) {{ ncr_sync_fatal("pthread_mutex_unlock(Barrier timeout)", error); }}
            {wait_timeout}
        }}
        if (error != 0 && error != ETIMEDOUT) {{
            int wait_error = error;
            state->broken = true;
            state->arrived = 0;
            --state->waiters;
            error = pthread_cond_broadcast(&ncr_self->condition->condition);
            if (error != 0) {{ ncr_sync_fatal("pthread_cond_broadcast(Barrier wait error)", error); }}
            error = pthread_mutex_unlock(&control->mutex);
            if (error != 0) {{ ncr_sync_fatal("pthread_mutex_unlock(Barrier wait error)", error); }}
            error = wait_error;
            {wait_error}
        }}
    }}
    --state->waiters;
    bool broken = state->generation == generation && state->broken;
    error = pthread_mutex_unlock(&control->mutex);
    if (error != 0) {{ ncr_sync_fatal("pthread_mutex_unlock(Barrier wait)", error); }}
    if (broken) {{ {already_broken} }}
    return {normal_arrival};"#,
        negative_timeout = result_error_literal(program, function, "timeout must be nonnegative"),
        deadline_error = result_error_code(program, function, "monotonic deadline", "error"),
        lock_error = result_error_code(program, function, "pthread_mutex_lock(Barrier wait)", "error"),
        already_broken = result_error_literal(program, function, "Barrier generation is broken"),
        generation_overflow = result_error_literal(program, function, "Barrier generation overflow"),
        last_arrival = result_ok(program, function, "arrival_index"),
        wait_timeout = result_error_literal(program, function, "Barrier generation timed out"),
        wait_error = result_error_code(program, function, "pthread_cond_timedwait(Barrier wait)", "error"),
        normal_arrival = result_ok(program, function, "arrival_index"));
    method_body(program, function, body)
}

fn barrier_reset(program: &t::Program, function: &t::Function) -> String {
    let id = function.receiver.unwrap();
    let unit = unit_mutex(program);
    let body = format!(
        "    struct ncr_mutex_control_{unit} *control = ncr_self->control;\n    int error = pthread_mutex_lock(&control->mutex);\n    if (error != 0) {{ {} }}\n    struct ncr_barrier_state_{id} *state = control->aux_payload;\n    if (!state->broken) {{\n        error = pthread_mutex_unlock(&control->mutex);\n        if (error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(Barrier reset not broken)\", error); }}\n        {}\n    }}\n    if (state->waiters != 0) {{\n        error = pthread_mutex_unlock(&control->mutex);\n        if (error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(Barrier reset waiters)\", error); }}\n        {}\n    }}\n    state->broken = false;\n    state->arrived = 0;\n    state->generation = 0;\n    error = pthread_mutex_unlock(&control->mutex);\n    if (error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(Barrier reset)\", error); }}\n    return {};",
        result_error_code(program, function, "pthread_mutex_lock(Barrier reset)", "error"),
        result_error_literal(program, function, "Barrier is not broken"),
        result_error_literal(program, function, "Barrier waiters are still leaving the broken generation"),
        result_ok(program, function, "UINT8_C(0)"));
    method_body(program, function, body)
}

fn condition_create(program: &t::Program, function: &t::Function) -> String {
    let id = native_owner(program, function);
    let value = match program.classes[id].native.unwrap() {
        C::Condition(value) => value,
        _ => unreachable!("Condition.create"),
    };
    let mutex = mutex_class(program, value);
    let associated = function.params[0].0;
    let body = format!(
        "    struct ncr_mutex_control_{mutex} *control = ncr_v_{associated}->control;\n    if (!ncr_sync_ref_acquire(&control->references)) {{ {} }}\n    int error = 0;\n    struct ncr_condition_node *condition = ncr_mutex_condition_new_{mutex}(control, &error);\n    if (condition == NULL) {{\n        ncr_mutex_release_{mutex}(control);\n        {}\n    }}\n    ncr_obj_{id} *handle = ncr_allocate(sizeof(*handle), ncr_line, ncr_column);\n    handle->control = control;\n    handle->condition = condition;\n    return {};",
        result_error_literal(program, function, "Mutex control reference count overflow"),
        result_error_code(program, function, "pthread_cond_init(Condition)", "error"),
        result_ok(program, function, "handle"));
    method_body(program, function, body)
}

fn condition_wait(program: &t::Program, function: &t::Function) -> String {
    let id = function.receiver.unwrap();
    let value = match program.classes[id].native.unwrap() {
        C::Condition(value) => value,
        _ => unreachable!("Condition.wait"),
    };
    let mutex = mutex_class(program, value);
    let Type::Borrow { class: guard, .. } = function.params[0].1 else {
        unreachable!("Condition.wait takes a borrowed Guard")
    };
    let guard_value = match program.classes[guard].native {
        Some(C::Guard(element)) => element,
        _ => unreachable!("Condition.wait borrowed value is a Guard"),
    };
    assert_eq!(guard_value, value, "Condition.wait Guard must protect its associated Mutex");
    let guard_arg = function.params[0].0;
    let timeout = function.params[1].0;
    let body = format!(
        "    if (ncr_v_{timeout} < 0) {{ {} }}\n    if (!ncr_v_{guard_arg}->locked || ncr_v_{guard_arg}->control != ncr_self->control) {{ {} }}\n    struct timespec deadline;\n    int error = ncr_sync_deadline_after_millis(ncr_v_{timeout}, &deadline);\n    if (error != 0) {{ {} }}\n    struct ncr_mutex_control_{mutex} *control = ncr_self->control;\n    error = pthread_cond_timedwait(&ncr_self->condition->condition, &control->mutex, &deadline);\n    if (error == ETIMEDOUT) {{ return {}; }}\n    if (error != 0) {{ {} }}\n    return {};",
        result_error_literal(program, function, "timeout must be nonnegative"),
        result_error_literal(program, function, "Guard does not belong to this Condition"),
        result_error_code(program, function, "monotonic deadline", "error"),
        result_ok(program, function, "false"),
        result_error_code(program, function, "pthread_cond_timedwait(Condition.wait)", "error"),
        result_ok(program, function, "true"));
    method_body(program, function, body)
}

fn condition_signal(program: &t::Program, function: &t::Function, broadcast: bool) -> String {
    let operation = if broadcast { "pthread_cond_broadcast" } else { "pthread_cond_signal" };
    let body = format!(
        "    int error = {operation}(&ncr_self->condition->condition);\n    if (error != 0) {{ {} }}\n    return {};",
        result_error_code(program, function, operation, "error"),
        result_ok(program, function, "UINT8_C(0)"));
    method_body(program, function, body)
}

fn cancellation_create(program: &t::Program, function: &t::Function) -> String {
    let id = native_owner(program, function);
    let unit = unit_mutex(program);
    let body = format!(
        "    int error = 0;\n    struct ncr_mutex_control_{unit} *control = ncr_cancel_control_new(&error);\n    if (control == NULL) {{ {} }}\n    ncr_obj_{id} *handle = ncr_allocate(sizeof(*handle), ncr_line, ncr_column);\n    handle->control = control;\n    return {};",
        result_error_code(program, function, "pthread_mutex_init(CancellationToken)", "error"),
        result_ok(program, function, "handle"));
    method_body(program, function, body)
}

fn cancellation_method(program: &t::Program, function: &t::Function) -> String {
    let id = function.receiver.unwrap();
    let unit = unit_mutex(program);
    let body = match function.native.unwrap() {
        M::CancellationTokenCloneHandle => format!(
            "    struct ncr_mutex_control_{unit} *control = ncr_self->control;\n    if (!ncr_sync_ref_acquire(&control->references)) {{ {} }}\n    ncr_obj_{id} *handle = ncr_allocate(sizeof(*handle), ncr_line, ncr_column);\n    handle->control = control;\n    return {};",
            result_error_literal(program, function, "CancellationToken handle reference count overflow"),
            result_ok(program, function, "handle")),
        M::CancellationTokenRequest => format!(
            "    int error = ncr_cancel_request_control(ncr_self->control);\n    if (error != 0) {{ {} }}\n    return {};",
            result_error_code(program, function, "CancellationToken request", "error"),
            result_ok(program, function, "UINT8_C(0)")),
        M::CancellationTokenIsCancelled => format!(
            "    bool cancelled = false;\n    int error = ncr_cancel_read_control(ncr_self->control, &cancelled);\n    if (error != 0) {{ {} }}\n    return {};",
            result_error_code(program, function, "CancellationToken isCancelled", "error"),
            result_ok(program, function, "cancelled")),
        _ => unreachable!("CancellationToken method"),
    };
    method_body(program, function, body)
}
fn future_spawn(program: &t::Program, function: &t::Function) -> String {
    let id = native_owner(program, function);
    let value = match program.classes[id].native.unwrap() {
        C::Future(value) => value,
        _ => unreachable!("Future.spawn"),
    };
    let (task, _, _) = task_info(program, value);
    let unit = unit_mutex(program);
    let arg = function.params[0].0;
    let body = format!(
        "    ncr_obj_{id} *future = ncr_allocate(sizeof(*future), ncr_line, ncr_column);\n    int error = ncr_future_handles_new_{id}(&future->state_control, &future->state_condition, &future->token_control);\n    if (error != 0) {{\n        ncr_drop_{task}(ncr_v_{arg});\n        ncr_deallocate(future);\n        {}\n    }}\n    future->pool_control = NULL;\n    future->pool_ready = NULL;\n    future->standalone = true;\n    future->joined = false;\n    struct ncr_future_context_{id} *context = ncr_allocate(sizeof(*context), ncr_line, ncr_column);\n    context->task = ncr_v_{arg};\n    context->state_control = future->state_control;\n    context->state_condition = future->state_condition;\n    context->token_control = future->token_control;\n    if (!ncr_sync_ref_acquire(&context->state_control->references)) {{\n        ncr_drop_{task}(context->task);\n        ncr_deallocate(context);\n        ncr_mutex_release_{unit}(future->state_control);\n        ncr_mutex_release_{unit}(future->token_control);\n        ncr_deallocate(future);\n        {}\n    }}\n    if (!ncr_sync_ref_acquire(&context->token_control->references)) {{\n        ncr_mutex_release_{unit}(context->state_control);\n        ncr_drop_{task}(context->task);\n        ncr_deallocate(context);\n        ncr_mutex_release_{unit}(future->state_control);\n        ncr_mutex_release_{unit}(future->token_control);\n        ncr_deallocate(future);\n        {}\n    }}\n    error = ncr_sync_pthread_create(&future->thread, ncr_future_worker_{id}, context);\n    if (error != 0) {{\n        ncr_mutex_release_{unit}(context->state_control);\n        ncr_mutex_release_{unit}(context->token_control);\n        ncr_drop_{task}(context->task);\n        ncr_deallocate(context);\n        ncr_mutex_release_{unit}(future->state_control);\n        ncr_mutex_release_{unit}(future->token_control);\n        ncr_deallocate(future);\n        {}\n    }}\n    return {};",
        result_error_code(program, function, "Future synchronization state creation", "error"),
        result_error_literal(program, function, "Future state reference count overflow"),
        result_error_literal(program, function, "Future cancellation reference count overflow"),
        result_error_code(program, function, "pthread_create(Future)", "error"),
        result_ok(program, function, "future"));
    method_body(program, function, body)
}

fn future_await(program: &t::Program, function: &t::Function) -> String {
    let id = function.receiver.unwrap();
    let value = match program.classes[id].native.unwrap() {
        C::Future(value) => value,
        _ => unreachable!("Future.await"),
    };
    let (_, _, task_result) = task_info(program, value);
    let task_layout = program.classes[task_result].result.expect("CancelableTask result layout");
    let outer = result_id(function);
    let outer_layout = program.classes[outer].result.expect("Future.await result layout");
    let option = option_class(program, function);
    let option_none = variant_id(program, option, "None");
    let option_some = variant_id(program, option, "Some");
    let unit = unit_mutex(program);
    let timeout = function.params[0].0;
    let task_error = task_layout.err_variant;
    let outer_error = outer_layout.err_variant;
    let task_ok = task_layout.ok_variant;
    let text = text_class(program);
    let body = format!(
        "    if (ncr_v_{timeout} < 0) {{ {} }}\n    struct timespec deadline;\n    int error = 0;\n    if (ncr_v_{timeout} > 0) {{\n        error = ncr_sync_deadline_after_millis(ncr_v_{timeout}, &deadline);\n        if (error != 0) {{ {} }}\n    }}\n    struct ncr_mutex_control_{unit} *control = ncr_self->state_control;\n    error = pthread_mutex_lock(&control->mutex);\n    if (error != 0) {{ {} }}\n    struct ncr_future_state_{id} *state = control->aux_payload;\n    while (!state->done) {{\n        if (ncr_v_{timeout} == 0) {{\n            error = pthread_mutex_unlock(&control->mutex);\n            if (error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(Future try await)\", error); }}\n            return {};\n        }}\n        error = pthread_cond_timedwait(&ncr_self->state_condition->condition, &control->mutex, &deadline);\n        if (error == ETIMEDOUT && !state->done) {{\n            error = pthread_mutex_unlock(&control->mutex);\n            if (error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(Future timeout)\", error); }}\n            return {};\n        }}\n        if (error != 0 && error != ETIMEDOUT) {{\n            int unlock_error = pthread_mutex_unlock(&control->mutex);\n            if (unlock_error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(Future wait error)\", unlock_error); }}\n            {}\n        }}\n    }}\n    if (state->consumed) {{\n        error = pthread_mutex_unlock(&control->mutex);\n        if (error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(Future already consumed)\", error); }}\n        {}\n    }}\n    ncr_obj_{task_result} outcome = state->outcome;\n    state->outcome.tag = -1;\n    state->consumed = true;\n    error = pthread_mutex_unlock(&control->mutex);\n    if (error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(Future await)\", error); }}\n    if (outcome.tag == {task_error}) {{\n        ncr_obj_{text} *message = outcome.data.v{task_error}.f0;\n        outcome.tag = -1;\n        return (ncr_obj_{outer}){{ .tag = {outer_error}, .data.v{outer_error}.f0 = message }};\n    }}\n    {} result_value = outcome.data.v{task_ok}.f0;\n    outcome.tag = -1;\n    ncr_obj_{option} option_value = {{ .tag = {option_some}, .data.v{option_some}.f0 = result_value }};\n    return {};",
        result_error_literal(program, function, "timeout must be nonnegative"),
        result_error_code(program, function, "monotonic deadline", "error"),
        result_error_code(program, function, "pthread_mutex_lock(Future await)", "error"),
        result_ok(program, function, &format!("(ncr_obj_{option}){{ .tag = {option_none} }}")),
        result_ok(program, function, &format!("(ncr_obj_{option}){{ .tag = {option_none} }}")),
        result_error_code(program, function, "pthread_cond_timedwait(Future await)", "error"),
        result_error_literal(program, function, "Future result has already been taken"),
        ctype(program, value),
        result_ok(program, function, "option_value"));
    method_body(program, function, body)
}

fn future_method(program: &t::Program, function: &t::Function) -> String {
    let id = function.receiver.unwrap();
    let unit = unit_mutex(program);
    match function.native.unwrap() {
        M::FutureAwait => future_await(program, function),
        M::FutureCancel => {
            let body = format!(
                "    int error = ncr_cancel_request_control(ncr_self->token_control);\n    if (error != 0) {{ {} }}\n    if (ncr_self->pool_control != NULL) {{\n        struct ncr_mutex_control_{unit} *pool_control = ncr_self->pool_control;\n        error = pthread_mutex_lock(&pool_control->mutex);\n        if (error != 0) {{ {} }}\n        error = pthread_cond_broadcast(&ncr_self->pool_ready->condition);\n        int unlock_error = pthread_mutex_unlock(&pool_control->mutex);\n        if (unlock_error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(Future cancel wake)\", unlock_error); }}\n        if (error != 0) {{ {} }}\n    }}\n    return {};",
                result_error_code(program, function, "CancellationToken request(Future.cancel)", "error"),
                result_error_code(program, function, "pthread_mutex_lock(ThreadPool cancel wake)", "error"),
                result_error_code(program, function, "pthread_cond_broadcast(ThreadPool cancel wake)", "error"),
                result_ok(program, function, "UINT8_C(0)"));
            method_body(program, function, body)
        }
        M::FutureIsDone => {
            let body = format!(
                "    struct ncr_mutex_control_{unit} *control = ncr_self->state_control;\n    int error = pthread_mutex_lock(&control->mutex);\n    if (error != 0) {{ ncr_sync_fatal(\"pthread_mutex_lock(Future isDone)\", error); }}\n    bool done = ((struct ncr_future_state_{id} *)control->aux_payload)->done;\n    error = pthread_mutex_unlock(&control->mutex);\n    if (error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(Future isDone)\", error); }}\n    return done;");
            method_body(program, function, body)
        }
        _ => unreachable!("Future method"),
    }
}

fn pool_error_helpers(program: &t::Program, id: usize, value: Type) -> String {
    let task = task_info(program, value).0;
    let submit = program.functions.iter().find(|function|
        function.receiver == Some(id) && function.native == Some(M::ThreadPoolSubmit))
        .expect("ThreadPool.submit exists");
    let (result, error_class, closed, full, system) = pool_error_shape(program, submit);
    let text = text_class(program);
    let closed_return = result_error_value(program, submit, "submit_error");
    let full_return = result_error_value(program, submit, "submit_error");
    let system_return = result_error_value(program, submit, "submit_error");
    let closed_value = format!(
        "static ncr_obj_{result} ncr_pool_closed_{id}(ncr_obj_{task} task) {{\n    ncr_obj_{error_class} submit_error = {{ .tag = {closed}, .data.v{closed}.f0 = task }};\n    return {closed_return};\n}}\n");
    let full_value = format!(
        "static ncr_obj_{result} ncr_pool_full_{id}(ncr_obj_{task} task) {{\n    ncr_obj_{error_class} submit_error = {{ .tag = {full}, .data.v{full}.f0 = task }};\n    return {full_return};\n}}\n");
    let system_value = format!(
        "static ncr_obj_{result} ncr_pool_system_{id}(ncr_obj_{task} task, const char *operation, int error, size_t line, size_t column) {{\n    char message[128];\n    size_t length = ncr_sync_error_text(message, sizeof(message), operation, error);\n    ncr_obj_{text} *text = ncr_text_from_bytes_{text}((const unsigned char *)message, length, line, column);\n    ncr_obj_{error_class} submit_error = {{ .tag = {system}, .data.v{system}.f0 = text, .data.v{system}.f1 = task }};\n    return {system_return};\n}}\n");
    format!("{closed_value}{full_value}{system_value}")
}

fn pool_recurring_helpers(program: &t::Program, id: usize, value: Type, unit: usize) -> String {
    let (task, run, result) = task_info(program, value);
    let slot = program.functions[run].virtual_slot.expect("CancelableTask.run vtable slot");
    let future = future_class(program, value);
    let token = token_class(program);
    let text = text_class(program);
    let layout = program.classes[result].result.expect("CancelableTask result layout");
    let error_tag = layout.err_variant;
    format!(r#"
static ncr_obj_{result} ncr_pool_recurring_error_{id}(const char *message, size_t length) {{
    ncr_obj_{text} *detail = ncr_text_from_bytes_{text}((const unsigned char *)message, length, 0, 0);
    return (ncr_obj_{result}){{ .tag = {error_tag}, .data.v{error_tag}.f0 = detail }};
}}

static ncr_obj_{result} ncr_pool_recurring_system_{id}(const char *operation, int failure) {{
    char message[128];
    size_t length = ncr_sync_error_text(message, sizeof(message), operation, failure);
    return ncr_pool_recurring_error_{id}(message, length);
}}
static ncr_obj_{result} ncr_pool_recurring_cancelled_{id}(void) {{
    static const char message[] = "recurring task cancelled";
    return ncr_pool_recurring_error_{id}(message, sizeof(message) - 1);
}}

static void ncr_pool_recurring_end_{id}(struct ncr_pool_work_{id} *work,
        struct ncr_mutex_control_{unit} *control, ncr_obj_{result} outcome) {{
    struct ncr_pool_state_{id} *state = control->aux_payload;
    ncr_drop_{task}(work->task);
    int error = pthread_mutex_lock(&control->mutex);
    if (error != 0) {{ ncr_sync_fatal("pthread_mutex_lock(ThreadPool recurring finish)", error); }}
    if (state->active_recurring == 0) {{ ncr_sync_fatal("ThreadPool recurring reservation underflow", EINVAL); }}
    --state->active_recurring;
    error = pthread_cond_broadcast(&state->ready->condition);
    if (error != 0) {{ ncr_sync_fatal("pthread_cond_broadcast(ThreadPool recurring finish)", error); }}
    error = pthread_mutex_unlock(&control->mutex);
    if (error != 0) {{ ncr_sync_fatal("pthread_mutex_unlock(ThreadPool recurring finish)", error); }}
    ncr_future_publish_{future}(work->state_control, work->state_condition, outcome);
    ncr_mutex_release_{unit}(work->state_control);
    ncr_mutex_release_{unit}(work->token_control);
}}

static void ncr_pool_recurring_run_{id}(struct ncr_pool_work_{id} *work,
        struct ncr_mutex_control_{unit} *control) {{
    ncr_obj_{token} token = {{ .control = work->token_control }};
    if (ncr_cancel_requested_internal(work->token_control)) {{
        ncr_pool_recurring_end_{id}(work, control, ncr_pool_recurring_cancelled_{id}());
        return;
    }}
    ncr_obj_{result} outcome = work->task.vtable->m{slot}(work->task.data, &token);
    if (outcome.tag == {error_tag}) {{
        ncr_pool_recurring_end_{id}(work, control, outcome);
        return;
    }}
    int64_t now_ns = 0;
    int error = work->fixed_rate ? 0 : ncr_sync_monotonic_now(&now_ns);
    ncr_drop_{result}(outcome);
    if (ncr_cancel_requested_internal(work->token_control)) {{
        ncr_pool_recurring_end_{id}(work, control, ncr_pool_recurring_cancelled_{id}());
        return;
    }}
    if (error != 0) {{
        ncr_pool_recurring_end_{id}(work, control,
            ncr_pool_recurring_system_{id}("clock_gettime(ThreadPool recurring)", error));
        return;
    }}
    int64_t next_due_ns = 0;
    if (work->fixed_rate) {{
        if (work->interval_ns > INT64_MAX - work->due_ns) {{
            static const char message[] = "ThreadPool recurring deadline overflow";
            ncr_pool_recurring_end_{id}(work, control,
                ncr_pool_recurring_error_{id}(message, sizeof(message) - 1));
            return;
        }}
        next_due_ns = work->due_ns + work->interval_ns;
    }} else {{
        if (work->interval_ns > INT64_MAX - now_ns) {{
            static const char message[] = "ThreadPool recurring deadline overflow";
            ncr_pool_recurring_end_{id}(work, control,
                ncr_pool_recurring_error_{id}(message, sizeof(message) - 1));
            return;
        }}
        next_due_ns = now_ns + work->interval_ns;
    }}
    error = pthread_mutex_lock(&control->mutex);
    if (error != 0) {{ ncr_sync_fatal("pthread_mutex_lock(ThreadPool recurring reschedule)", error); }}
    struct ncr_pool_state_{id} *state = control->aux_payload;
    if (state->shutting_down) {{
        error = pthread_mutex_unlock(&control->mutex);
        if (error != 0) {{ ncr_sync_fatal("pthread_mutex_unlock(ThreadPool recurring shutdown)", error); }}
        ncr_pool_recurring_end_{id}(work, control, ncr_pool_recurring_cancelled_{id}());
        return;
    }}
    if (state->active_recurring == 0 || state->queue_count >= state->queue_capacity) {{
        ncr_sync_fatal("ThreadPool recurring queue reservation lost", EINVAL);
    }}
    --state->active_recurring;
    work->due_ns = next_due_ns;
    state->queue[state->queue_count++] = *work;
    error = pthread_cond_broadcast(&state->ready->condition);
    if (error != 0) {{ ncr_sync_fatal("pthread_cond_broadcast(ThreadPool recurring reschedule)", error); }}
    error = pthread_mutex_unlock(&control->mutex);
    if (error != 0) {{ ncr_sync_fatal("pthread_mutex_unlock(ThreadPool recurring reschedule)", error); }}
}}
"#)
}

fn pool_create(program: &t::Program, function: &t::Function) -> String {
    let id = native_owner(program, function);
    let unit = unit_mutex(program);
    let workers = function.params[0].0;
    let capacity = function.params[1].0;
    let body = format!(
        "    if (ncr_v_{workers} < 1 || ncr_v_{capacity} < 1 || (uint64_t)ncr_v_{capacity} > (uint64_t)(SIZE_MAX / sizeof(struct ncr_pool_work_{id})) || (uint64_t)ncr_v_{workers} > (uint64_t)(SIZE_MAX / sizeof(pthread_t))) {{ {} }}\n    int error = 0;\n    struct ncr_mutex_control_{unit} *control = ncr_sync_mutex_control_new_{unit}(&error);\n    if (control == NULL) {{ {} }}\n    struct ncr_pool_state_{id} *state = ncr_allocate(sizeof(*state), ncr_line, ncr_column);\n    state->queue_capacity = (size_t)ncr_v_{capacity};\n    state->queue_count = 0;\n    state->active_recurring = 0;\n    state->worker_capacity = (size_t)ncr_v_{workers};\n    state->worker_count = 0;\n    state->shutting_down = false;\n    state->drain = false;\n    state->queue = ncr_allocate(state->queue_capacity * sizeof(*state->queue), ncr_line, ncr_column);\n    state->workers = ncr_allocate(state->worker_capacity * sizeof(*state->workers), ncr_line, ncr_column);\n    state->joined = ncr_allocate(state->worker_capacity * sizeof(*state->joined), ncr_line, ncr_column);\n    control->aux_payload = state;\n    control->aux_drop = ncr_pool_state_drop_{id};\n    state->ready = ncr_mutex_condition_new_{unit}(control, &error);\n    if (state->ready == NULL) {{ ncr_mutex_release_{unit}(control); {} }}\n    while (state->worker_count < state->worker_capacity) {{\n        if (!ncr_sync_ref_acquire(&control->references)) {{ error = EOVERFLOW; break; }}\n        error = ncr_sync_pthread_create(&state->workers[state->worker_count], ncr_pool_worker_{id}, control);\n        if (error != 0) {{ ncr_mutex_release_{unit}(control); break; }}\n        ++state->worker_count;\n    }}\n    if (error != 0) {{\n        int shutdown_error = pthread_mutex_lock(&control->mutex);\n        if (shutdown_error != 0) {{ ncr_sync_fatal(\"pthread_mutex_lock(ThreadPool create cleanup)\", shutdown_error); }}\n        state->shutting_down = true;\n        state->drain = false;\n        int wake_error = pthread_cond_broadcast(&state->ready->condition);\n        if (wake_error != 0) {{ ncr_sync_fatal(\"pthread_cond_broadcast(ThreadPool create cleanup)\", wake_error); }}\n        shutdown_error = pthread_mutex_unlock(&control->mutex);\n        if (shutdown_error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(ThreadPool create cleanup)\", shutdown_error); }}\n        for (size_t i = 0; i < state->worker_count; ++i) {{\n            int join_error = pthread_join(state->workers[i], NULL);\n            if (join_error != 0) {{ ncr_sync_fatal(\"pthread_join(ThreadPool create cleanup)\", join_error); }}\n            state->joined[i] = true;\n        }}\n        ncr_mutex_release_{unit}(control);\n        {}\n    }}\n    ncr_obj_{id} *handle = ncr_allocate(sizeof(*handle), ncr_line, ncr_column);\n    handle->control = control;\n    return {};",
        result_error_literal(program, function, "ThreadPool workers and queue capacity must be positive and representable"),
        result_error_code(program, function, "pthread_mutex_init(ThreadPool)", "error"),
        result_error_code(program, function, "pthread_cond_init(ThreadPool)", "error"),
        result_error_code(program, function, "pthread_create(ThreadPool worker)", "error"),
        result_ok(program, function, "handle"));
    method_body(program, function, body)
}

fn pool_submit(program: &t::Program, function: &t::Function) -> String {
    let id = function.receiver.expect("ThreadPool submission receiver");
    let value = match program.classes[id].native.unwrap() {
        C::ThreadPool(value) => value,
        _ => unreachable!("ThreadPool submission"),
    };
    let future = future_class(program, value);
    let unit = unit_mutex(program);
    let task_arg = function.params[0].0;
    let native = function.native.expect("ThreadPool submission method");
    let policy = crate::stdlib::pool_submission_policy(native).expect("ThreadPool submission policy");
    let scheduled = matches!(policy, crate::stdlib::PoolSubmissionPolicy::Delayed);
    let recurring = matches!(policy, crate::stdlib::PoolSubmissionPolicy::Recurring { .. });
    let fixed_rate = matches!(policy, crate::stdlib::PoolSubmissionPolicy::Recurring { fixed_rate: true });
    let deadline = if scheduled {
        let delay = function.params[1].0;
        format!(
            "    if (ncr_v_{delay} < 0) {{ return ncr_pool_system_{id}(ncr_v_{task_arg}, \"ThreadPool schedule delay\", EINVAL, ncr_line, ncr_column); }}\n    int64_t now_ns = 0;\n    int64_t due_ns = 0;\n    int error = ncr_sync_monotonic_now(&now_ns);\n    if (error != 0) {{ return ncr_pool_system_{id}(ncr_v_{task_arg}, \"clock_gettime(ThreadPool schedule)\", error, ncr_line, ncr_column); }}\n    if (!ncr_time_deadline_after_millis(now_ns, ncr_v_{delay}, &due_ns)) {{ return ncr_pool_system_{id}(ncr_v_{task_arg}, \"ThreadPool schedule deadline\", EOVERFLOW, ncr_line, ncr_column); }}"
        )
    } else if recurring {
        let initial = function.params[1].0;
        let interval = function.params[2].0;
        format!(
            "    if (ncr_v_{initial} < 0) {{ return ncr_pool_system_{id}(ncr_v_{task_arg}, \"ThreadPool recurring initial delay\", EINVAL, ncr_line, ncr_column); }}\n    if (ncr_v_{interval} <= 0) {{ return ncr_pool_system_{id}(ncr_v_{task_arg}, \"ThreadPool recurring interval\", EINVAL, ncr_line, ncr_column); }}\n    if (ncr_v_{interval} > INT64_MAX / INT64_C(1000000)) {{ return ncr_pool_system_{id}(ncr_v_{task_arg}, \"ThreadPool recurring interval\", EOVERFLOW, ncr_line, ncr_column); }}\n    int64_t now_ns = 0;\n    int64_t due_ns = 0;\n    int error = ncr_sync_monotonic_now(&now_ns);\n    if (error != 0) {{ return ncr_pool_system_{id}(ncr_v_{task_arg}, \"clock_gettime(ThreadPool recurring schedule)\", error, ncr_line, ncr_column); }}\n    if (!ncr_time_deadline_after_millis(now_ns, ncr_v_{initial}, &due_ns)) {{ return ncr_pool_system_{id}(ncr_v_{task_arg}, \"ThreadPool recurring initial deadline\", EOVERFLOW, ncr_line, ncr_column); }}\n    int64_t interval_ns = ncr_v_{interval} * INT64_C(1000000);"
        )
    } else {
        "    int64_t due_ns = 0;\n    int64_t interval_ns = 0;\n    int error = 0;".to_owned()
    };
    let interval_ns = if recurring { "interval_ns" } else { "INT64_C(0)" };
    let recurring_value = if recurring { "true" } else { "false" };
    let fixed_rate_value = if fixed_rate { "true" } else { "false" };
    let body = format!(r#"
{deadline}
    struct ncr_mutex_control_{unit} *control = ncr_self->control;
    error = pthread_mutex_lock(&control->mutex);
    if (error != 0) {{ return ncr_pool_system_{id}(ncr_v_{task_arg}, "pthread_mutex_lock(ThreadPool submit)", error, ncr_line, ncr_column); }}
    struct ncr_pool_state_{id} *state = control->aux_payload;
    if (state->shutting_down) {{
        error = pthread_mutex_unlock(&control->mutex);
        if (error != 0) {{ return ncr_pool_system_{id}(ncr_v_{task_arg}, "pthread_mutex_unlock(ThreadPool closed)", error, ncr_line, ncr_column); }}
        return ncr_pool_closed_{id}(ncr_v_{task_arg});
    }}
    /* Running recurring jobs keep a slot reserved so their next run always fits. */
    if (state->queue_count + state->active_recurring >= state->queue_capacity) {{
        error = pthread_mutex_unlock(&control->mutex);
        if (error != 0) {{ return ncr_pool_system_{id}(ncr_v_{task_arg}, "pthread_mutex_unlock(ThreadPool full)", error, ncr_line, ncr_column); }}
        return ncr_pool_full_{id}(ncr_v_{task_arg});
    }}
    error = pthread_cond_broadcast(&state->ready->condition);
    if (error != 0) {{
        int failure = error;
        error = pthread_mutex_unlock(&control->mutex);
        if (error != 0) {{ ncr_sync_fatal("pthread_mutex_unlock(ThreadPool submit wake error)", error); }}
        return ncr_pool_system_{id}(ncr_v_{task_arg}, "pthread_cond_broadcast(ThreadPool submit)", failure, ncr_line, ncr_column);
    }}
    ncr_obj_{future} *future = ncr_allocate(sizeof(*future), ncr_line, ncr_column);
    error = ncr_future_handles_new_{future}(&future->state_control, &future->state_condition, &future->token_control);
    if (error != 0) {{
        ncr_deallocate(future);
        int failure = error;
        error = pthread_mutex_unlock(&control->mutex);
        if (error != 0) {{ return ncr_pool_system_{id}(ncr_v_{task_arg}, "pthread_mutex_unlock(ThreadPool create future)", error, ncr_line, ncr_column); }}
        return ncr_pool_system_{id}(ncr_v_{task_arg}, "ThreadPool future synchronization state", failure, ncr_line, ncr_column);
    }}
    if (!ncr_sync_ref_acquire(&control->references)) {{
        ncr_mutex_release_{unit}(future->state_control);
        ncr_mutex_release_{unit}(future->token_control);
        ncr_deallocate(future);
        error = pthread_mutex_unlock(&control->mutex);
        if (error != 0) {{ return ncr_pool_system_{id}(ncr_v_{task_arg}, "pthread_mutex_unlock(ThreadPool reference error)", error, ncr_line, ncr_column); }}
        return ncr_pool_system_{id}(ncr_v_{task_arg}, "ThreadPool control reference overflow", EOVERFLOW, ncr_line, ncr_column);
    }}
    future->pool_control = control;
    future->pool_ready = state->ready;
    future->standalone = false;
    future->joined = true;
    if (!ncr_sync_ref_acquire(&future->state_control->references)) {{
        ncr_mutex_release_{unit}(future->pool_control);
        ncr_mutex_release_{unit}(future->state_control);
        ncr_mutex_release_{unit}(future->token_control);
        ncr_deallocate(future);
        error = pthread_mutex_unlock(&control->mutex);
        if (error != 0) {{ return ncr_pool_system_{id}(ncr_v_{task_arg}, "pthread_mutex_unlock(ThreadPool future reference error)", error, ncr_line, ncr_column); }}
        return ncr_pool_system_{id}(ncr_v_{task_arg}, "Future state reference overflow", EOVERFLOW, ncr_line, ncr_column);
    }}
    if (!ncr_sync_ref_acquire(&future->token_control->references)) {{
        ncr_mutex_release_{unit}(future->state_control);
        ncr_mutex_release_{unit}(future->pool_control);
        ncr_mutex_release_{unit}(future->state_control);
        ncr_mutex_release_{unit}(future->token_control);
        ncr_deallocate(future);
        error = pthread_mutex_unlock(&control->mutex);
        if (error != 0) {{ return ncr_pool_system_{id}(ncr_v_{task_arg}, "pthread_mutex_unlock(ThreadPool token reference error)", error, ncr_line, ncr_column); }}
        return ncr_pool_system_{id}(ncr_v_{task_arg}, "Future token reference overflow", EOVERFLOW, ncr_line, ncr_column);
    }}
    struct ncr_pool_work_{id} work;
    work.task = ncr_v_{task_arg};
    work.state_control = future->state_control;
    work.state_condition = future->state_condition;
    work.token_control = future->token_control;
    work.due_ns = due_ns;
    work.interval_ns = {interval_ns};
    work.recurring = {recurring_value};
    work.fixed_rate = {fixed_rate_value};
    state->queue[state->queue_count++] = work;
    error = pthread_mutex_unlock(&control->mutex);
    if (error != 0) {{ ncr_sync_fatal("pthread_mutex_unlock(ThreadPool submit)", error); }}
    return {};
"#,
        result_ok(program, function, "future"));
    method_body(program, function, body)
}

fn pool_shutdown(program: &t::Program, function: &t::Function) -> String {
    let id = function.receiver.expect("ThreadPool.shutdown receiver");
    let drain = function.params[0].0;
    let body = format!(
        "    int error = ncr_pool_shutdown_{id}(ncr_self->control, ncr_v_{drain});\n    if (error != 0) {{ {} }}\n    return {};",
        result_error_code(program, function, "ThreadPool shutdown", "error"),
        result_ok(program, function, "UINT8_C(0)"));
    method_body(program, function, body)
}

pub(super) fn helpers(program: &t::Program) -> String {
    state_helpers(program)
}

pub(super) fn drop_helpers(program: &t::Program) -> String {
    let mut out = String::new();
    for class in &program.classes {
        let Some(native) = class.native else { continue };
        if !is_extra(native) { continue; }
        match native {
            C::AtomicInt | C::AtomicLong | C::AtomicBoolean | C::Semaphore |
            C::CountDownLatch | C::Barrier | C::CancellationToken => {
                let unit = unit_mutex(program);
                out.push_str(&format!(
                    "static void ncr_concurrency_drop_{0}(ncr_obj_{0} *p) {{ struct ncr_mutex_control_{1} *control = p->control; p->control = NULL; if (control != NULL) {{ ncr_mutex_release_{1}(control); }} }}\n",
                    class.id, unit));
            }
            C::Condition(value) => {
                let mutex = mutex_class(program, value);
                out.push_str(&format!(
                    "static void ncr_concurrency_drop_{0}(ncr_obj_{0} *p) {{ struct ncr_mutex_control_{1} *control = p->control; p->control = NULL; p->condition = NULL; if (control != NULL) {{ ncr_mutex_release_{1}(control); }} }}\n",
                    class.id, mutex));
            }
            C::Future(_) => {
                let unit = unit_mutex(program);
                out.push_str(&format!(
                    "static void ncr_concurrency_drop_{0}(ncr_obj_{0} *p) {{\n    int error = ncr_cancel_request_control(p->token_control);\n    if (error != 0) {{ ncr_sync_fatal(\"CancellationToken request during Future drop\", error); }}\n    if (p->pool_ready != NULL) {{\n        struct ncr_mutex_control_{1} *pool_control = p->pool_control;\n        error = pthread_mutex_lock(&pool_control->mutex);\n        if (error != 0) {{ ncr_sync_fatal(\"pthread_mutex_lock(Future drop wake)\", error); }}\n        error = pthread_cond_broadcast(&p->pool_ready->condition);\n        int unlock_error = pthread_mutex_unlock(&pool_control->mutex);\n        if (unlock_error != 0) {{ ncr_sync_fatal(\"pthread_mutex_unlock(Future drop wake)\", unlock_error); }}\n        if (error != 0) {{ ncr_sync_fatal(\"pthread_cond_broadcast(Future drop)\", error); }}\n    }}\n    if (p->standalone && !p->joined) {{\n        if (pthread_equal(pthread_self(), p->thread)) {{ ncr_sync_fatal(\"Future dropped by its own worker\", EDEADLK); }}\n        error = pthread_join(p->thread, NULL);\n        if (error != 0) {{ ncr_sync_fatal(\"pthread_join during Future drop\", error); }}\n        p->joined = true;\n    }}\n    struct ncr_mutex_control_{1} *state_control = p->state_control;\n    struct ncr_mutex_control_{1} *token_control = p->token_control;\n    struct ncr_mutex_control_{1} *pool_control = p->pool_control;\n    p->state_control = NULL; p->token_control = NULL; p->pool_control = NULL; p->pool_ready = NULL;\n    if (state_control != NULL) {{ ncr_mutex_release_{1}(state_control); }}\n    if (token_control != NULL) {{ ncr_mutex_release_{1}(token_control); }}\n    if (pool_control != NULL) {{ ncr_mutex_release_{1}(pool_control); }}\n}}\n",
                    class.id, unit));
            }
            C::ThreadPool(_) => {
                let unit = unit_mutex(program);
                out.push_str(&format!(
                    "static void ncr_concurrency_drop_{0}(ncr_obj_{0} *p) {{\n    struct ncr_mutex_control_{1} *control = p->control;\n    p->control = NULL;\n    if (control == NULL) {{ return; }}\n    int error = ncr_pool_shutdown_{0}(control, false);\n    if (error != 0) {{ ncr_sync_fatal(\"ThreadPool shutdown during drop\", error); }}\n    ncr_mutex_release_{1}(control);\n}}\n",
                    class.id, unit));
            }
            _ => unreachable!("non-extra concurrency native class"),
        }
    }
    out
}

pub(super) fn implementations(program: &t::Program) -> String {
    let mut out = String::new();
    for function in &program.functions {
        let body = match function.native {
            Some(M::AtomicCreate) => Some(atomic_create(program, function)),
            Some(M::AtomicCloneHandle | M::AtomicLoad | M::AtomicStore |
                M::AtomicExchange | M::AtomicCompareExchange | M::AtomicFetchAdd |
                M::AtomicFetchSubtract) => Some(atomic_method(program, function)),
            Some(M::SemaphoreCreate) => Some(semaphore_create(program, function)),
            Some(M::SemaphoreCloneHandle) => Some(synchronizer_clone(program, function)),
            Some(M::SemaphoreAcquire) => Some(semaphore_acquire(program, function)),
            Some(M::SemaphoreRelease) => Some(semaphore_release(program, function)),
            Some(M::LatchCreate) => Some(latch_create(program, function)),
            Some(M::LatchCloneHandle) => Some(synchronizer_clone(program, function)),
            Some(M::LatchCountDown) => Some(latch_count_down(program, function)),
            Some(M::LatchWait) => Some(latch_wait(program, function)),
            Some(M::LatchCount) => Some(latch_count(program, function)),
            Some(M::BarrierCreate) => Some(barrier_create(program, function)),
            Some(M::BarrierCloneHandle) => Some(synchronizer_clone(program, function)),
            Some(M::BarrierWait) => Some(barrier_wait(program, function)),
            Some(M::BarrierReset) => Some(barrier_reset(program, function)),
            Some(M::ConditionCreate) => Some(condition_create(program, function)),
            Some(M::ConditionCloneHandle) => Some(synchronizer_clone(program, function)),
            Some(M::ConditionWait) => Some(condition_wait(program, function)),
            Some(M::ConditionSignal) => Some(condition_signal(program, function, false)),
            Some(M::ConditionBroadcast) => Some(condition_signal(program, function, true)),
            Some(M::CancellationTokenCreate) => Some(cancellation_create(program, function)),
            Some(M::CancellationTokenCloneHandle | M::CancellationTokenRequest |
                M::CancellationTokenIsCancelled) => Some(cancellation_method(program, function)),
            Some(M::FutureSpawn) => Some(future_spawn(program, function)),
            Some(M::FutureAwait | M::FutureCancel | M::FutureIsDone) => Some(future_method(program, function)),
            Some(M::ThreadPoolCreate) => Some(pool_create(program, function)),
            Some(method) if crate::stdlib::pool_submission_policy(method).is_some() =>
                Some(pool_submit(program, function)),
            Some(M::ThreadPoolShutdown) => Some(pool_shutdown(program, function)),
            _ => None,
        };
        if let Some(body) = body { out.push_str(&body); }
    }
    out
}
