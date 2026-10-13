use crate::ir::{self as t, NativeClass as C, NativeMethod as M, Type};
use super::support::{
    native_function, replace_method_ids, replace_placeholders, result_id, template_section,
    text_class, variant, variant_id,
};

fn class(program: &t::Program, name: &str) -> usize {
    program.classes.iter().find(|class| class.name == name)
        .unwrap_or_else(|| panic!("compiler-owned {name} class is present")).id
}

fn native_class(program: &t::Program, name: &str, is: impl Fn(C) -> bool) -> usize {
    program.classes.iter().find(|class| class.name == name && class.native.is_some_and(&is))
        .unwrap_or_else(|| panic!("compiler-owned native {name} class is present")).id
}

fn result_payload(program: &t::Program, function: &t::Function) -> usize {
    let (_, ok) = variant(program, result_id(function), "Ok");
    match ok.fields.as_slice() {
        [Type::Object(id)] => *id,
        _ => unreachable!("process API Result has an object Ok payload"),
    }
}

fn result_error(_program: &t::Program, function: &t::Function) -> usize {
    result_id(function)
}

fn parameter(function: &t::Function, index: usize) -> String {
    format!("ncr_v_{}", function.params[index].0)
}

fn process_function(program: &t::Program, method: M) -> &t::Function {
    native_function(program, |native| native == method)
}

pub(super) fn fields(program: &t::Program, native: C) -> String {
    let text = text_class(program).expect("process APIs require Text");
    match native {
        C::Command => {
            let path = native_class(program, "Path", |class| matches!(class, C::Path));
            let file = native_class(program, "File", |class| matches!(class, C::File));
            format!(
                "    ncr_obj_{path} *executable;\n    ncr_obj_{text} **arguments;\n    size_t argument_count;\n    size_t argument_capacity;\n    ncr_obj_{text} **environment_names;\n    ncr_obj_{text} **environment_values;\n    size_t environment_count;\n    size_t environment_capacity;\n    ncr_obj_{file} *redirects[3];\n    bool clear_environment;\n"
            )
        }
        C::Process => "    pid_t pid;\n    bool reaped;\n".to_owned(),
        C::ProcessOutput => {
            let buffer = native_class(program, "ByteBuffer", |class| matches!(class, C::ByteBuffer));
            let status = class(program, "ExitStatus");
            format!(
                "    ncr_obj_{buffer} *stdout_buffer;\n    ncr_obj_{buffer} *stderr_buffer;\n    ncr_obj_{status} status;\n    bool stdout_truncated;\n    bool stderr_truncated;\n    bool timed_out;\n"
            )
        }
        _ => unreachable!("process layout belongs to a process native class"),
    }
}

pub(super) fn drop_fields(program: &t::Program, native: C) -> String {
    match native {
        C::Command => {
            let path = native_class(program, "Path", |class| matches!(class, C::Path));
            let file = native_class(program, "File", |class| matches!(class, C::File));
            let text = text_class(program).expect("process APIs require Text");
            format!(
                "    if (p->executable != NULL) {{ ncr_drop_{path}(p->executable); }}\n    for (size_t i = 0; i < p->argument_count; ++i) {{ ncr_drop_{text}(p->arguments[i]); }}\n    for (size_t i = 0; i < p->environment_count; ++i) {{ ncr_drop_{text}(p->environment_names[i]); ncr_drop_{text}(p->environment_values[i]); }}\n    for (size_t i = 0; i < 3; ++i) {{ if (p->redirects[i] != NULL) {{ ncr_drop_{file}(p->redirects[i]); }} }}\n    ncr_deallocate(p->arguments);\n    ncr_deallocate(p->environment_names);\n    ncr_deallocate(p->environment_values);\n"
            )
        }
        C::Process => {
            "    if (p->pid > 0 && !p->reaped) {\n        if (kill(p->pid, SIGKILL) != 0 && errno != ESRCH) { ncr_fail(\"Process drop: SIGKILL failed\", 0, 0); }\n        int status;\n        pid_t waited;\n        do { waited = waitpid(p->pid, &status, 0); } while (waited < 0 && errno == EINTR);\n        if (waited != p->pid) { ncr_fail(\"Process drop: waitpid failed\", 0, 0); }\n        p->reaped = true;\n    }\n".to_owned()
        }
        C::ProcessOutput => {
            let buffer = native_class(program, "ByteBuffer", |class| matches!(class, C::ByteBuffer));
            let status = class(program, "ExitStatus");
            format!(
                "    if (p->stdout_buffer != NULL) {{ ncr_drop_{buffer}(p->stdout_buffer); }}\n    if (p->stderr_buffer != NULL) {{ ncr_drop_{buffer}(p->stderr_buffer); }}\n    ncr_drop_{status}(p->status);\n"
            )
        }
        _ => unreachable!("process destructor belongs to a process native class"),
    }
}

fn group<'a>(source: &'a str, name: &str) -> &'a str {
    let start = format!("/* PROCESS_GROUP {name} */");
    let end = format!("/* PROCESS_GROUP_END {name} */");
    template_section(source, &start, &end)
}

fn has_any(program: &t::Program, methods: &[M]) -> bool {
    program.functions.iter().any(|function| function.native.is_some_and(|method| methods.contains(&method)))
}

fn environment_substitutions(program: &t::Program) -> Vec<(&'static str, String)> {
    let env_get = process_function(program, M::EnvGet);
    let env_set = process_function(program, M::EnvSet);
    let env_unset = process_function(program, M::EnvUnset);
    let env_get_result = result_id(env_get);
    let env_get_option = result_payload(program, env_get);
    let (_, env_get_ok) = variant(program, env_get_result, "Ok");
    assert!(matches!(env_get_ok.fields.as_slice(), [Type::Object(id)] if *id == env_get_option));
    let (_, env_get_some) = variant(program, env_get_option, "Some");
    let text = text_class(program).expect("Env APIs require Text");
    assert!(matches!(env_get_some.fields.as_slice(), [Type::Object(id)] if *id == text));
    vec![
        ("@ENV_GET_RESULT@", env_get_result.to_string()),
        ("@ENV_GET_OK_TAG@", variant_id(program, env_get_result, "Ok").to_string()),
        ("@ENV_GET_OPTION@", env_get_option.to_string()),
        ("@ENV_GET_NONE_TAG@", variant_id(program, env_get_option, "None").to_string()),
        ("@ENV_GET_SOME_TAG@", variant_id(program, env_get_option, "Some").to_string()),
        ("@ENV_GET_ERROR@", format!("ncr_result_error_{}", result_error(program, env_get))),
        ("@ENV_SET_RESULT@", result_id(env_set).to_string()),
        ("@ENV_SET_OK_TAG@", variant_id(program, result_id(env_set), "Ok").to_string()),
        ("@ENV_SET_ERROR@", format!("ncr_result_error_{}", result_error(program, env_set))),
        ("@ENV_UNSET_RESULT@", result_id(env_unset).to_string()),
        ("@ENV_UNSET_OK_TAG@", variant_id(program, result_id(env_unset), "Ok").to_string()),
        ("@ENV_UNSET_ERROR@", format!("ncr_result_error_{}", result_error(program, env_unset))),
        ("@ENV_GET_NAME@", parameter(env_get, 0)),
        ("@ENV_SET_NAME@", parameter(env_set, 0)),
        ("@ENV_SET_VALUE@", parameter(env_set, 1)),
        ("@ENV_UNSET_NAME@", parameter(env_unset, 0)),
    ]
}

fn command_substitutions(program: &t::Program) -> Vec<(&'static str, String)> {
    let path = native_class(program, "Path", |class| matches!(class, C::Path));
    let file = native_class(program, "File", |class| matches!(class, C::File));
    let command = native_class(program, "Command", |class| matches!(class, C::Command));
    let buffer = native_class(program, "ByteBuffer", |class| matches!(class, C::ByteBuffer));
    let process = native_class(program, "Process", |class| matches!(class, C::Process));
    let output = native_class(program, "ProcessOutput", |class| matches!(class, C::ProcessOutput));
    let reserve = format!("ncr_byte_buffer_reserve_{buffer}");

    let command_create = process_function(program, M::CommandCreate);
    let command_add_arg = process_function(program, M::CommandAddArg);
    let command_set_env = process_function(program, M::CommandSetEnv);
    let redirect_stdin = process_function(program, M::CommandRedirectStdin);
    let redirect_stdout = process_function(program, M::CommandRedirectStdout);
    let redirect_stderr = process_function(program, M::CommandRedirectStderr);
    let clear_env = process_function(program, M::CommandClearEnv);
    let spawn = process_function(program, M::CommandSpawn);
    let capture = process_function(program, M::CommandOutput);

    let command_result = result_id(command_add_arg);
    let spawn_result = result_id(spawn);
    let capture_result = result_id(capture);
    let redirect_stdin_result = result_id(redirect_stdin);
    let redirect_stdout_result = result_id(redirect_stdout);
    let redirect_stderr_result = result_id(redirect_stderr);
    let clear_env_result = result_id(clear_env);

    assert_eq!(command_create.return_type, Type::Object(command), "Command.create returns Command");
    assert_eq!(result_payload(program, spawn), process, "Command.spawn returns Process");
    assert_eq!(result_payload(program, capture), output, "Command.output returns ProcessOutput");

    vec![
        ("@PATH_CLASS@", path.to_string()),
        ("@FILE_CLASS@", file.to_string()),
        ("@COMMAND_CLASS@", command.to_string()),
        ("@BUFFER_RESERVE@", reserve),
        ("@COMMAND_CREATE@", format!("ncr_fn_{}", command_create.id)),
        ("@COMMAND_UNIT_RESULT@", command_result.to_string()),
        ("@COMMAND_UNIT_OK_TAG@", variant_id(program, command_result, "Ok").to_string()),
        ("@COMMAND_ADD_ARG_ERROR@", format!("ncr_result_error_{}", result_error(program, command_add_arg))),
        ("@COMMAND_SET_ENV_ERROR@", format!("ncr_result_error_{}", result_error(program, command_set_env))),
        ("@REDIRECT_STDIN_RESULT@", redirect_stdin_result.to_string()),
        ("@REDIRECT_STDIN_OK_TAG@", variant_id(program, redirect_stdin_result, "Ok").to_string()),
        ("@REDIRECT_STDIN_ERROR@", format!("ncr_result_error_{}", result_error(program, redirect_stdin))),
        ("@REDIRECT_STDOUT_RESULT@", redirect_stdout_result.to_string()),
        ("@REDIRECT_STDOUT_OK_TAG@", variant_id(program, redirect_stdout_result, "Ok").to_string()),
        ("@REDIRECT_STDOUT_ERROR@", format!("ncr_result_error_{}", result_error(program, redirect_stdout))),
        ("@REDIRECT_STDERR_RESULT@", redirect_stderr_result.to_string()),
        ("@REDIRECT_STDERR_OK_TAG@", variant_id(program, redirect_stderr_result, "Ok").to_string()),
        ("@REDIRECT_STDERR_ERROR@", format!("ncr_result_error_{}", result_error(program, redirect_stderr))),
        ("@CLEAR_ENV_RESULT@", clear_env_result.to_string()),
        ("@CLEAR_ENV_OK_TAG@", variant_id(program, clear_env_result, "Ok").to_string()),
        ("@CLEAR_ENV_ERROR@", format!("ncr_result_error_{}", result_error(program, clear_env))),
        ("@COMMAND_SPAWN_RESULT@", spawn_result.to_string()),
        ("@COMMAND_SPAWN_OK_TAG@", variant_id(program, spawn_result, "Ok").to_string()),
        ("@COMMAND_SPAWN_ERROR@", format!("ncr_result_error_{}", result_error(program, spawn))),
        ("@COMMAND_OUTPUT_RESULT@", capture_result.to_string()),
        ("@COMMAND_OUTPUT_OK_TAG@", variant_id(program, capture_result, "Ok").to_string()),
        ("@COMMAND_OUTPUT_ERROR@", format!("ncr_result_error_{}", result_error(program, capture))),
        ("@COMMAND_CREATE_PATH@", parameter(command_create, 0)),
        ("@COMMAND_ADD_ARG_VALUE@", parameter(command_add_arg, 0)),
        ("@COMMAND_SET_ENV_NAME@", parameter(command_set_env, 0)),
        ("@COMMAND_SET_ENV_VALUE@", parameter(command_set_env, 1)),
        ("@REDIRECT_STDIN_FILE@", parameter(redirect_stdin, 0)),
        ("@REDIRECT_STDOUT_FILE@", parameter(redirect_stdout, 0)),
        ("@REDIRECT_STDERR_FILE@", parameter(redirect_stderr, 0)),
        ("@COMMAND_OUTPUT_MAX@", parameter(capture, 0)),
        ("@COMMAND_OUTPUT_TIMEOUT@", parameter(capture, 1)),
    ]
}

fn status_substitutions(program: &t::Program) -> Vec<(&'static str, String)> {
    let status = class(program, "ExitStatus");
    vec![
        ("@EXIT_STATUS_CLASS@", status.to_string()),
        ("@EXITED_TAG@", variant_id(program, status, "Exited").to_string()),
        ("@SIGNALED_TAG@", variant_id(program, status, "Signaled").to_string()),
    ]
}

fn process_substitutions(program: &t::Program) -> Vec<(&'static str, String)> {
    let process = native_class(program, "Process", |class| matches!(class, C::Process));
    let wait = process_function(program, M::ProcessWait);
    let wait_for = process_function(program, M::ProcessWaitFor);
    let terminate = process_function(program, M::ProcessTerminate);
    let kill = process_function(program, M::ProcessKill);
    let wait_result = result_id(wait);
    let wait_for_result = result_id(wait_for);
    let option = result_payload(program, wait_for);
    vec![
        ("@PROCESS_CLASS@", process.to_string()),
        ("@PROCESS_WAIT_RESULT@", wait_result.to_string()),
        ("@PROCESS_WAIT_OK_TAG@", variant_id(program, wait_result, "Ok").to_string()),
        ("@PROCESS_WAIT_ERROR@", format!("ncr_result_error_{wait_result}")),
        ("@PROCESS_WAIT_FOR_RESULT@", wait_for_result.to_string()),
        ("@PROCESS_WAIT_FOR_OK_TAG@", variant_id(program, wait_for_result, "Ok").to_string()),
        ("@PROCESS_WAIT_FOR_ERROR@", format!("ncr_result_error_{wait_for_result}")),
        ("@PROCESS_WAIT_FOR_OPTION@", option.to_string()),
        ("@PROCESS_WAIT_FOR_NONE_TAG@", variant_id(program, option, "None").to_string()),
        ("@PROCESS_WAIT_FOR_SOME_TAG@", variant_id(program, option, "Some").to_string()),
        ("@PROCESS_WAIT_FOR_TIMEOUT@", parameter(wait_for, 0)),
        ("@PROCESS_TERMINATE_RESULT@", result_id(terminate).to_string()),
        ("@PROCESS_TERMINATE_OK_TAG@", variant_id(program, result_id(terminate), "Ok").to_string()),
        ("@PROCESS_TERMINATE_ERROR@", format!("ncr_result_error_{}", result_id(terminate))),
        ("@PROCESS_KILL_RESULT@", result_id(kill).to_string()),
        ("@PROCESS_KILL_OK_TAG@", variant_id(program, result_id(kill), "Ok").to_string()),
        ("@PROCESS_KILL_ERROR@", format!("ncr_result_error_{}", result_id(kill))),
    ]
}

fn output_substitutions(program: &t::Program) -> Vec<(&'static str, String)> {
    let output = native_class(program, "ProcessOutput", |class| matches!(class, C::ProcessOutput));
    let buffer = native_class(program, "ByteBuffer", |class| matches!(class, C::ByteBuffer));
    vec![
        ("@PROCESS_OUTPUT_CLASS@", output.to_string()),
        ("@BUFFER_CLASS@", buffer.to_string()),
    ]
}

pub(super) fn implementations(program: &t::Program) -> Option<String> {
    const ENV_METHODS: &[M] = &[M::EnvGet, M::EnvSet, M::EnvUnset];
    const COMMAND_METHODS: &[M] = &[
        M::CommandCreate, M::CommandAddArg, M::CommandSetEnv,
        M::CommandRedirectStdin, M::CommandRedirectStdout, M::CommandRedirectStderr,
        M::CommandClearEnv, M::CommandSpawn, M::CommandOutput,
    ];
    const PROCESS_METHODS: &[M] = &[M::ProcessWait, M::ProcessWaitFor, M::ProcessTerminate, M::ProcessKill];
    const OUTPUT_METHODS: &[M] = &[
        M::ProcessOutputStdout, M::ProcessOutputStderr, M::ProcessOutputStatus,
        M::ProcessOutputStdoutTruncated, M::ProcessOutputStderrTruncated, M::ProcessOutputTimedOut,
    ];
    let has_env = has_any(program, ENV_METHODS);
    let has_command = has_any(program, COMMAND_METHODS);
    let has_process = has_any(program, PROCESS_METHODS);
    let has_output = has_any(program, OUTPUT_METHODS);
    if !has_env && !has_command && !has_process && !has_output { return None; }

    let template = include_str!("../../stdlib/process.c.in");
    let mut body = template.split_once("/* PROCESS_GROUP common */")
        .expect("process common template group").0.to_owned();
    if has_env || has_command || has_process { body.push_str(group(template, "common")); }
    if has_env { body.push_str(group(template, "env")); }
    if has_command { body.push_str(group(template, "command")); }
    if has_command || has_process { body.push_str(group(template, "wait_helpers")); }
    if has_process { body.push_str(group(template, "process")); }
    if has_command { body.push_str(group(template, "capture")); }
    if has_output { body.push_str(group(template, "output")); }

    let text = text_class(program).expect("process APIs require Text");
    let mut substitutions = vec![
        ("@TEXT_CLASS@", text.to_string()),
        ("@TEXT_TAKE@", format!("ncr_text_take_bytes_{text}")),
    ];
    if has_env { substitutions.extend(environment_substitutions(program)); }
    if has_command { substitutions.extend(command_substitutions(program)); }
    if has_process { substitutions.extend(process_substitutions(program)); }
    if has_output { substitutions.extend(output_substitutions(program)); }
    if has_process || has_output || has_command { substitutions.extend(status_substitutions(program)); }
    body = replace_placeholders(body, &substitutions);
    body = replace_method_ids(body, program, None);
    debug_assert!(!super::support::has_placeholder(&body), "unexpanded Process placeholder");
    Some(body)
}
