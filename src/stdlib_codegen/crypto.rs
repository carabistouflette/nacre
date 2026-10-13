use crate::ir::{self as t, NativeClass as C, NativeMethod as M, Type};
use super::support::{native_function, replace_method_ids, replace_placeholders, result_id, text_class, variant_id};

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

fn expression_uses_crypto(program: &t::Program, expression: &t::Expr) -> bool {
    use t::ExprKind as E;
    match &expression.kind {
        E::New { args, .. } | E::Variant { args, .. } =>
            args.iter().any(|argument| expression_uses_crypto(program, argument)),
        E::Call { target, args, .. } => {
            if let t::CallTarget::Function(id) = target {
                if program.functions.get(*id).and_then(|function| function.native)
                    .is_some_and(is_crypto_method) {
                    return true;
                }
            }
            args.iter().any(|argument| expression_uses_crypto(program, argument))
        }
        E::MapSlot { index, .. } => expression_uses_crypto(program, index),
        E::Drop(value) | E::Upcast { value, .. } | E::Unary { expr: value, .. }
        | E::Try { value, .. } | E::Cast { value } =>
            expression_uses_crypto(program, value),
        E::Binary { left, right, .. } =>
            expression_uses_crypto(program, left) || expression_uses_crypto(program, right),
        E::Char(_) | E::Int(_) | E::Long(_) | E::Double(_) | E::Unit | E::Boolean(_)
        | E::String(_) | E::Read(_) | E::Move(_) | E::Borrow { .. } | E::MapCapacity(_) => false,
    }
}

fn block_uses_crypto(program: &t::Program, block: &t::Block) -> bool {
    block.iter().any(|statement| {
        use t::Stmt as S;
        match statement {
            S::Let { value, .. } => value.as_ref()
                .is_some_and(|value| expression_uses_crypto(program, value)),
            S::Assign { value, .. } | S::Expr(value) => expression_uses_crypto(program, value),
            S::If { condition, then_block, else_block } =>
                expression_uses_crypto(program, condition)
                    || block_uses_crypto(program, then_block)
                    || else_block.as_ref().is_some_and(|block| block_uses_crypto(program, block)),
            S::While { condition, body } =>
                expression_uses_crypto(program, condition) || block_uses_crypto(program, body),
            S::Match { value, arms } => expression_uses_crypto(program, value)
                || arms.iter().any(|arm| block_uses_crypto(program, &arm.body)),
            S::Return(value) => value.as_ref()
                .is_some_and(|value| expression_uses_crypto(program, value)),
            S::Block(block) => block_uses_crypto(program, block),
            S::Break | S::Continue => false,
        }
    })
}

fn is_used(program: &t::Program) -> bool {
    program.functions.iter().any(|function| {
        function.native.is_none() && block_uses_crypto(program, &function.body)
    })
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

    let body = replace_placeholders(include_str!("../../stdlib/crypto.c.in").to_owned(), &[
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
    debug_assert!(!body.contains('@'), "unexpanded Crypto placeholder");
    body
}
