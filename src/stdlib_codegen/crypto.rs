use crate::ir::{self as t, NativeClass as C, NativeMethod as M, Type};
use super::support::{
    native_function, replace_method_ids, replace_placeholders, result_id, template_section,
    text_class, variant_id,
};

fn method(program: &t::Program, native: M) -> &t::Function {
    let function = native_function(program, |candidate| candidate == native);
    assert!(function.receiver.is_none(), "Crypto methods are static");
    function
}

fn parameter(function: &t::Function, index: usize) -> String {
    format!("ncr_v_{}", function.params.get(index)
        .expect("compiler-owned Crypto parameter is present").0)
}

fn assert_parameters(function: &t::Function, expected: &[Type]) {
    assert_eq!(function.params.len(), expected.len(), "Crypto native parameter count");
    for (index, (_, actual)) in function.params.iter().enumerate() {
        assert_eq!(*actual, expected[index], "Crypto native parameter type");
    }
}

fn is_crypto_method(method: M) -> bool {
    matches!(method,
        M::CryptoRandomBytes | M::CryptoDigest | M::CryptoHmac | M::CryptoPbkdf2 |
        M::CryptoHkdf | M::CryptoAesGcmEncrypt | M::CryptoAesGcmDecrypt |
        M::CryptoConstantTimeEquals)
}


fn expression_uses_method(
    program: &t::Program,
    expression: &t::Expr,
    matches_method: fn(M) -> bool,
) -> bool {
    use t::ExprKind as E;
    match &expression.kind {
        E::New { args, .. } | E::Variant { args, .. } =>
            args.iter().any(|argument| expression_uses_method(program, argument, matches_method)),
        E::Call { target, args, .. } => {
            if let t::CallTarget::Function(id) = target
                && program.functions.get(*id).and_then(|function| function.native)
                    .is_some_and(matches_method) {
                    return true;
                }
            args.iter().any(|argument| expression_uses_method(program, argument, matches_method))
        }
        E::MapSlot { index, .. } => expression_uses_method(program, index, matches_method),
        E::Drop(value) | E::Upcast { value, .. } | E::Unary { expr: value, .. }
        | E::Try { value, .. } | E::Cast { value } =>
            expression_uses_method(program, value, matches_method),
        E::Binary { left, right, .. } =>
            expression_uses_method(program, left, matches_method) ||
                expression_uses_method(program, right, matches_method),
        E::Char(_) | E::Int(_) | E::Long(_) | E::Double(_) | E::Unit | E::Boolean(_)
        | E::String(_) | E::Read(_) | E::Move(_) | E::Borrow { .. } | E::MapCapacity(_) => false,
    }
}

fn block_uses_method(program: &t::Program, block: &t::Block, matches_method: fn(M) -> bool) -> bool {
    block.iter().any(|statement| {
        use t::Stmt as S;
        match statement {
            S::Let { value, .. } => value.as_ref()
                .is_some_and(|value| expression_uses_method(program, value, matches_method)),
            S::Assign { value, .. } | S::Expr(value) =>
                expression_uses_method(program, value, matches_method),
            S::If { condition, then_block, else_block } =>
                expression_uses_method(program, condition, matches_method)
                    || block_uses_method(program, then_block, matches_method)
                    || else_block.as_ref().is_some_and(|block| block_uses_method(program, block, matches_method)),
            S::While { condition, body } =>
                expression_uses_method(program, condition, matches_method)
                    || block_uses_method(program, body, matches_method),
            S::Match { value, arms } => expression_uses_method(program, value, matches_method)
                || arms.iter().any(|arm| block_uses_method(program, &arm.body, matches_method)),
            S::Return(value) => value.as_ref()
                .is_some_and(|value| expression_uses_method(program, value, matches_method)),
            S::Block(block) => block_uses_method(program, block, matches_method),
            S::Break | S::Continue => false,
        }
    })
}

fn is_used(program: &t::Program) -> bool {
    program.functions.iter().any(|function| {
        function.native.is_none() && block_uses_method(program, &function.body, is_crypto_method)
    })
}


pub(crate) fn digest_selection_helper(program: &t::Program) -> String {
    let has_key_digest_user = program.classes.iter().any(|class| matches!(
        class.native, Some(C::CryptoKey | C::CryptoDigest | C::CryptoMac)
    ));
    if !is_used(program) && !has_key_digest_user { return String::new(); }
    const START: &str = "/* CRYPTO_GROUP DIGEST_SELECTION */";
    const END: &str = "/* CRYPTO_GROUP_END DIGEST_SELECTION */";
    let template = include_str!("../../stdlib/crypto.c.in");
    let selection = template_section(template, START, END).to_owned();
    replace_placeholders(selection, &[(
        "@TEXT_CLASS@",
        text_class(program).expect("digest selection requires Text").to_string(),
    )])
}

fn without_digest_selection(source: &str) -> String {
    let (before, remaining) = source.split_once("/* CRYPTO_GROUP DIGEST_SELECTION */")
        .expect("Crypto template digest selection group");
    let (_, after) = remaining.split_once("/* CRYPTO_GROUP_END DIGEST_SELECTION */")
        .expect("Crypto template digest selection end");
    let mut body = String::with_capacity(source.len());
    body.push_str(before);
    body.push_str(after);
    body
}

pub(super) fn implementation(program: &t::Program) -> String {
    if !is_used(program) { return String::new(); }
    let text = text_class(program).expect("Crypto requires Text");
    let byte_buffer = program.classes.iter()
        .find(|class| matches!(class.native, Some(C::ByteBuffer)))
        .expect("Crypto requires ByteBuffer").id;
    let text_borrow = Type::Borrow { class: text, mutable: false };
    let buffer_borrow = Type::Borrow { class: byte_buffer, mutable: false };

    let random = method(program, M::CryptoRandomBytes);
    let digest = method(program, M::CryptoDigest);
    let hmac = method(program, M::CryptoHmac);
    let pbkdf2 = method(program, M::CryptoPbkdf2);
    let hkdf = method(program, M::CryptoHkdf);
    let encrypt = method(program, M::CryptoAesGcmEncrypt);
    let decrypt = method(program, M::CryptoAesGcmDecrypt);
    let compare = method(program, M::CryptoConstantTimeEquals);
    assert_parameters(random, &[Type::Long]);
    assert_parameters(digest, &[text_borrow, buffer_borrow]);
    assert_parameters(hmac, &[text_borrow, buffer_borrow, buffer_borrow]);
    assert_parameters(pbkdf2, &[text_borrow, buffer_borrow, buffer_borrow, Type::Int, Type::Long]);
    assert_parameters(hkdf, &[text_borrow, buffer_borrow, buffer_borrow, buffer_borrow, Type::Long]);
    assert_parameters(encrypt, &[buffer_borrow, buffer_borrow, buffer_borrow, buffer_borrow]);
    assert_parameters(decrypt, &[buffer_borrow, buffer_borrow, buffer_borrow, buffer_borrow]);
    assert_parameters(compare, &[buffer_borrow, buffer_borrow]);
    assert_eq!(compare.return_type, Type::Boolean, "Crypto.constantTimeEquals returns boolean");

    let result = result_id(random);
    for function in [digest, hmac, pbkdf2, hkdf, encrypt, decrypt] {
        assert_eq!(result_id(function), result, "Crypto byte-producing APIs share Result<ByteBuffer, Text>");
    }
    for function in [random, digest, hmac, pbkdf2, hkdf, encrypt, decrypt] {
        assert!(matches!(function.return_type, Type::Object(class) if class == result));
    }
    let (_, ok) = super::support::variant(program, result, "Ok");
    let (_, err) = super::support::variant(program, result, "Err");
    assert!(matches!(ok.fields.as_slice(), [Type::Object(class)] if *class == byte_buffer));
    assert!(matches!(err.fields.as_slice(), [Type::Object(class)] if *class == text));

    let template = include_str!("../../stdlib/crypto.c.in");
    let body = replace_placeholders(without_digest_selection(template), &[
        ("@RESULT@", result.to_string()),
        ("@RESULT_OK_TAG@", variant_id(program, result, "Ok").to_string()),
        ("@ERROR_RESULT_FN@", format!("ncr_result_error_{result}")),
        ("@BYTE_BUFFER_CLASS@", byte_buffer.to_string()),
        ("@TEXT_CLASS@", text.to_string()),
        ("@RANDOM_LENGTH@", parameter(random, 0)),
        ("@DIGEST_ALGORITHM@", parameter(digest, 0)),
        ("@DIGEST_DATA@", parameter(digest, 1)),
        ("@HMAC_ALGORITHM@", parameter(hmac, 0)),
        ("@HMAC_KEY@", parameter(hmac, 1)),
        ("@HMAC_DATA@", parameter(hmac, 2)),
        ("@PBKDF2_ALGORITHM@", parameter(pbkdf2, 0)),
        ("@PBKDF2_PASSWORD@", parameter(pbkdf2, 1)),
        ("@PBKDF2_SALT@", parameter(pbkdf2, 2)),
        ("@PBKDF2_ITERATIONS@", parameter(pbkdf2, 3)),
        ("@PBKDF2_OUTPUT_LENGTH@", parameter(pbkdf2, 4)),
        ("@HKDF_ALGORITHM@", parameter(hkdf, 0)),
        ("@HKDF_KEY@", parameter(hkdf, 1)),
        ("@HKDF_SALT@", parameter(hkdf, 2)),
        ("@HKDF_INFO@", parameter(hkdf, 3)),
        ("@HKDF_OUTPUT_LENGTH@", parameter(hkdf, 4)),
        ("@AES_ENCRYPT_KEY@", parameter(encrypt, 0)),
        ("@AES_ENCRYPT_NONCE@", parameter(encrypt, 1)),
        ("@AES_ENCRYPT_PLAINTEXT@", parameter(encrypt, 2)),
        ("@AES_ENCRYPT_AAD@", parameter(encrypt, 3)),
        ("@AES_DECRYPT_KEY@", parameter(decrypt, 0)),
        ("@AES_DECRYPT_NONCE@", parameter(decrypt, 1)),
        ("@AES_DECRYPT_CIPHERTEXT@", parameter(decrypt, 2)),
        ("@AES_DECRYPT_AAD@", parameter(decrypt, 3)),
        ("@COMPARE_LEFT@", parameter(compare, 0)),
        ("@COMPARE_RIGHT@", parameter(compare, 1)),
    ]);
    let body = replace_method_ids(body, program, None);
    debug_assert!(!super::support::has_placeholder(&body), "unexpanded Crypto placeholder");
    body
}
