use crate::ir::{self as t, NativeMethod as M, Type};
use super::support::{
    native_function, replace_method_ids, replace_placeholders, result_id, text_class, variant,
};

fn input_id(function: &t::Function, buffer: usize) -> usize {
    match function.params.first() {
        Some((id, Type::Borrow { class, mutable: false })) if *class == buffer => *id,
        _ => unreachable!("compression APIs take a shared ByteBuffer borrow"),
    }
}

fn second_id(function: &t::Function, expected: Type) -> usize {
    match function.params.get(1) {
        Some((id, ty)) if *ty == expected => *id,
        _ => unreachable!("compression API parameter type is compiler-owned"),
    }
}

fn expression_calls(expr: &t::Expr, methods: &[usize]) -> bool {
    use t::ExprKind as E;
    match &expr.kind {
        E::MapSlot { index, .. } => expression_calls(index, methods),
        E::Drop(value) | E::Upcast { value, .. } | E::Unary { expr: value, .. } |
        E::Try { value, .. } | E::Cast { value } => expression_calls(value, methods),
        E::New { args, .. } | E::Variant { args, .. } =>
            args.iter().any(|arg| expression_calls(arg, methods)),
        E::Call { target, args, .. } => {
            matches!(target, t::CallTarget::Function(id) if methods.contains(id))
                || args.iter().any(|arg| expression_calls(arg, methods))
        }
        E::Binary { left, right, .. } =>
            expression_calls(left, methods) || expression_calls(right, methods),
        E::Char(_) | E::Int(_) | E::Long(_) | E::Double(_) | E::Unit | E::Boolean(_) |
        E::String(_) | E::Read(_) | E::Move(_) | E::Borrow { .. } | E::MapCapacity(_) => false,
    }
}

fn block_calls(block: &t::Block, methods: &[usize]) -> bool {
    use t::Stmt as S;
    block.iter().any(|stmt| match stmt {
        S::Let { value, .. } => value.as_ref().is_some_and(|value| expression_calls(value, methods)),
        S::Assign { value, .. } | S::Expr(value) => expression_calls(value, methods),
        S::If { condition, then_block, else_block } =>
            expression_calls(condition, methods)
                || block_calls(then_block, methods)
                || else_block.as_ref().is_some_and(|block| block_calls(block, methods)),
        S::While { condition, body } =>
            expression_calls(condition, methods) || block_calls(body, methods),
        S::Match { value, arms } =>
            expression_calls(value, methods) || arms.iter().any(|arm| block_calls(&arm.body, methods)),
        S::Return(value) =>
            value.as_ref().is_some_and(|value| expression_calls(value, methods)),
        S::Block(body) => block_calls(body, methods),
        S::Break | S::Continue => false,
    })
}

pub(super) fn implementation(program: &t::Program) -> String {
    let methods = [
        M::CompressionDeflate, M::CompressionInflate,
        M::CompressionGzip, M::CompressionGunzip,
        M::ChecksumsCrc32, M::ChecksumsAdler32,
    ];
    let method_ids: Vec<_> = program.functions.iter().filter_map(|function| {
        function.native.filter(|method| methods.contains(method)).map(|_| function.id)
    }).collect();
    if method_ids.is_empty() || !program.functions.iter()
        .filter(|function| function.native.is_none())
        .any(|function| block_calls(&function.body, &method_ids)) {
        return String::new();
    }


    let buffer = program.classes.iter()
        .find(|class| matches!(class.native, Some(t::NativeClass::ByteBuffer)))
        .expect("Compression requires the native ByteBuffer class").id;
    let deflate = native_function(program, |method| matches!(method, M::CompressionDeflate));
    let inflate = native_function(program, |method| matches!(method, M::CompressionInflate));
    let gzip = native_function(program, |method| matches!(method, M::CompressionGzip));
    let gunzip = native_function(program, |method| matches!(method, M::CompressionGunzip));
    let crc32 = native_function(program, |method| matches!(method, M::ChecksumsCrc32));
    let adler32 = native_function(program, |method| matches!(method, M::ChecksumsAdler32));

    assert_eq!(deflate.return_type, inflate.return_type);
    assert_eq!(deflate.return_type, gzip.return_type);
    assert_eq!(deflate.return_type, gunzip.return_type);
    let result = result_id(deflate);
    for function in [inflate, gzip, gunzip] {
        assert_eq!(result_id(function), result, "compression methods share Result<ByteBuffer, Text>");
    }
    let (ok_tag, ok) = variant(program, result, "Ok");
    assert!(matches!(ok.fields.as_slice(), [Type::Object(class)] if *class == buffer));
    let (_, error) = variant(program, result, "Err");
    assert!(matches!(error.fields.as_slice(), [Type::Object(class)] if Some(*class) == text_class(program)));

    assert_eq!(deflate.params.len(), 2);
    assert_eq!(gzip.params.len(), 2);
    assert_eq!(inflate.params.len(), 2);
    assert_eq!(gunzip.params.len(), 2);
    assert_eq!(crc32.params.len(), 1);
    assert_eq!(adler32.params.len(), 1);
    assert_eq!(crc32.return_type, Type::Long);
    assert_eq!(adler32.return_type, Type::Long);
    let deflate_input = input_id(deflate, buffer);
    let deflate_level = second_id(deflate, Type::Int);
    let inflate_input = input_id(inflate, buffer);
    let inflate_max = second_id(inflate, Type::Long);
    let gzip_input = input_id(gzip, buffer);
    let gzip_level = second_id(gzip, Type::Int);
    let gunzip_input = input_id(gunzip, buffer);
    let gunzip_max = second_id(gunzip, Type::Long);
    let crc32_input = input_id(crc32, buffer);
    let adler32_input = input_id(adler32, buffer);

    let mut body = replace_placeholders(include_str!("../../stdlib/compression.c.in").to_owned(), &[
        ("@BUFFER_TYPE@", crate::codegen::ctype(program, Type::Object(buffer))),
        ("@RESULT_TYPE@", crate::codegen::ctype(program, deflate.return_type)),
        ("@RESULT_CLASS@", result.to_string()),
        ("@RESULT_OK_TAG@", ok_tag.to_string()),
        ("@BUFFER_CLASS_ID@", buffer.to_string()),
        ("@DEFLATE_INPUT_ID@", deflate_input.to_string()),
        ("@DEFLATE_LEVEL_ID@", deflate_level.to_string()),
        ("@INFLATE_INPUT_ID@", inflate_input.to_string()),
        ("@INFLATE_MAX_ID@", inflate_max.to_string()),
        ("@GZIP_INPUT_ID@", gzip_input.to_string()),
        ("@GZIP_LEVEL_ID@", gzip_level.to_string()),
        ("@GUNZIP_INPUT_ID@", gunzip_input.to_string()),
        ("@GUNZIP_MAX_ID@", gunzip_max.to_string()),
        ("@CRC32_INPUT_ID@", crc32_input.to_string()),
        ("@ADLER32_INPUT_ID@", adler32_input.to_string()),
    ]);
    body = replace_method_ids(body, program, None);
    debug_assert!(!body.contains('@'), "unexpanded Compression placeholder");
    body
}
