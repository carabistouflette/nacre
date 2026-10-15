use crate::c_repr::ctype;
use crate::ir::{self as t, NativeClass as C, NativeMethod as M, Type};
use super::support::{native_function, replace_placeholders, result_id, text_class, variant};

pub(super) fn fields() -> String {
    "    unsigned char bytes[16];\n".to_owned()
}

pub(super) fn drop_fields() -> String {
    String::new()
}

fn function(program: &t::Program, method: M) -> &t::Function {
    native_function(program, |candidate| candidate == method)
}

fn parameter(function: &t::Function, index: usize) -> String {
    format!("ncr_v_{}", function.params[index].0)
}

fn signature(program: &t::Program, method: M) -> String {
    let function = function(program, method);
    assert_eq!(function.native, Some(method), "Uuid signature has the expected native method");
    let mut parameters = Vec::with_capacity(function.params.len() + 3);
    if let Some(receiver) = function.receiver {
        parameters.push(format!("{} ncr_self", ctype(program, Type::Object(receiver))));
    }
    parameters.extend(function.params.iter()
        .map(|(id, ty)| format!("{} ncr_v_{id}", ctype(program, *ty))));
    parameters.extend(["size_t ncr_line".to_owned(), "size_t ncr_column".to_owned()]);
    format!("static {} ncr_fn_{}({})", ctype(program, function.return_type), function.id, parameters.join(", "))
}

fn assert_static(function: &t::Function) {
    assert!(function.receiver.is_none(), "Uuid factories are static");
}

fn assert_instance(function: &t::Function, class: usize) {
    assert_eq!(function.receiver, Some(class), "Uuid value methods have a Uuid receiver");
}

pub(super) const UUID_METHODS: &[M] = &[
    M::UuidParse, M::UuidFromBytes, M::UuidFromLongs, M::UuidRandom,
    M::UuidNameMd5, M::UuidNameSha1, M::UuidClone, M::UuidToText,
    M::UuidToBytes, M::UuidMostSignificantBits, M::UuidLeastSignificantBits,
    M::UuidVersion, M::UuidVariant, M::UuidEquals, M::UuidCompareTo,
];

pub(super) fn is_uuid_method(method: M) -> bool {
    UUID_METHODS.contains(&method)
}

pub(super) fn is_used(program: &t::Program) -> bool {
    super::native_usage::any(program, is_uuid_method)
}

fn assert_borrow(function: &t::Function, index: usize, expected: usize) {
    assert!(matches!(function.params.get(index),
        Some((_, Type::Borrow { class, mutable: false })) if *class == expected),
        "Uuid method takes the specified shared borrow");
}

fn result(
    program: &t::Program,
    function: &t::Function,
    ok_type: Type,
    text: usize,
) -> (usize, usize, usize) {
    let result = result_id(function);
    let (ok_tag, ok) = variant(program, result, "Ok");
    assert_eq!(ok.fields.as_slice(), &[ok_type], "Uuid Result has its specialized Ok payload");
    let (err_tag, error) = variant(program, result, "Err");
    assert!(matches!(error.fields.as_slice(), [Type::Object(class)] if *class == text),
        "Uuid Result owns an error Text");
    (result, ok_tag, err_tag)
}

pub(super) fn implementation(program: &t::Program, class: usize) -> String {
    if !is_used(program) { return String::new(); }
    assert!(matches!(program.classes[class].native, Some(C::Uuid)),
        "Uuid implementation belongs to its native class");
    let text = text_class(program).expect("Uuid requires Text");
    let buffer = program.classes.iter()
        .find(|candidate| matches!(candidate.native, Some(C::ByteBuffer)))
        .map(|candidate| candidate.id)
        .expect("Uuid requires ByteBuffer");

    let parse = function(program, M::UuidParse);
    let from_bytes = function(program, M::UuidFromBytes);
    let from_longs = function(program, M::UuidFromLongs);
    let random = function(program, M::UuidRandom);
    let name_md5 = function(program, M::UuidNameMd5);
    let name_sha1 = function(program, M::UuidNameSha1);
    let clone = function(program, M::UuidClone);
    let to_text = function(program, M::UuidToText);
    let to_bytes = function(program, M::UuidToBytes);
    let most_bits = function(program, M::UuidMostSignificantBits);
    let least_bits = function(program, M::UuidLeastSignificantBits);
    let version = function(program, M::UuidVersion);
    let variant_method = function(program, M::UuidVariant);
    let equals = function(program, M::UuidEquals);
    let compare_to = function(program, M::UuidCompareTo);

    for method in [parse, from_bytes, from_longs, random, name_md5, name_sha1] {
        assert_static(method);
    }
    for method in [clone, to_text, to_bytes, most_bits, least_bits, version, variant_method, equals, compare_to] {
        assert_instance(method, class);
    }
    assert_borrow(parse, 0, text);
    assert_borrow(from_bytes, 0, buffer);
    assert_eq!(from_longs.params.iter().map(|(_, ty)| *ty).collect::<Vec<_>>(),
        vec![Type::Long, Type::Long], "Uuid.fromLongs accepts two bit-preserving longs");
    assert!(random.params.is_empty(), "Uuid.random takes no parameters");
    assert_borrow(name_md5, 0, class);
    assert_borrow(name_md5, 1, buffer);
    assert_borrow(name_sha1, 0, class);
    assert_borrow(name_sha1, 1, buffer);
    assert_eq!(from_longs.return_type, Type::Object(class), "Uuid.fromLongs returns an owned Uuid");
    assert_eq!(clone.return_type, Type::Object(class), "Uuid.clone returns an owned Uuid");
    assert_eq!(to_text.return_type, Type::Object(text), "Uuid.toText returns an owned Text");
    assert_eq!(to_bytes.return_type, Type::Object(buffer), "Uuid.toBytes returns an owned ByteBuffer");
    assert_eq!(most_bits.return_type, Type::Long, "Uuid.mostSignificantBits returns long");
    assert_eq!(least_bits.return_type, Type::Long, "Uuid.leastSignificantBits returns long");
    assert_eq!(version.return_type, Type::Int, "Uuid.version returns int");
    assert_eq!(variant_method.return_type, Type::Int, "Uuid.variant returns int");
    assert_eq!(equals.return_type, Type::Boolean, "Uuid.equals returns boolean");
    assert_borrow(equals, 0, class);
    assert_eq!(compare_to.return_type, Type::Int, "Uuid.compareTo returns int");
    assert_borrow(compare_to, 0, class);

    let (uuid_result, uuid_ok, uuid_err) = result(program, parse, Type::Object(class), text);
    for method in [from_bytes, random, name_md5, name_sha1] {
        assert_eq!(result_id(method), uuid_result, "Uuid fallible factories share Result<Uuid, Text>");
        let _ = result(program, method, Type::Object(class), text);
    }

    let capacity = native_function(program, |method| method == M::ByteBufferWithCapacity);
    assert_static(capacity);
    let (buffer_result, buffer_ok, buffer_err) = result(program, capacity, Type::Object(buffer), text);
    assert_eq!(capacity.params.iter().map(|(_, ty)| *ty).collect::<Vec<_>>(), vec![Type::Long],
        "ByteBuffer.withCapacity accepts long");

    let uses_crypto = super::native_usage::any(program, |method| {
        matches!(method, M::UuidRandom | M::UuidNameMd5 | M::UuidNameSha1)
    });
    let (crypto_headers, crypto_digest_helper, random_body, name_md5_body, name_sha1_body) = if uses_crypto {
        (
            "/* nacre-link: crypto */\n#include <openssl/evp.h>\n#include <openssl/rand.h>\n".to_owned(),
            format!(
                "static bool ncr_uuid_digest_{class}(\n    const EVP_MD *algorithm, size_t expected_length,\n    const ncr_obj_{class} *namespace_uuid, const ncr_obj_{buffer} *name,\n    unsigned char digest[EVP_MAX_MD_SIZE]\n) {{\n    EVP_MD_CTX *context = EVP_MD_CTX_new();\n    if (context == NULL) {{ return false; }}\n    const int algorithm_length = EVP_MD_get_size(algorithm);\n    bool success = algorithm_length > 0\n        && (size_t)algorithm_length == expected_length\n        && expected_length <= (size_t)EVP_MAX_MD_SIZE;\n    if (success) {{ success = EVP_DigestInit_ex(context, algorithm, NULL) == 1; }}\n    if (success) {{ success = EVP_DigestUpdate(context, namespace_uuid->bytes, 16) == 1; }}\n    size_t offset = 0;\n    while (success && offset < name->length) {{\n        const size_t remaining = name->length - offset;\n        const size_t chunk = remaining < (size_t)65536 ? remaining : (size_t)65536;\n        success = EVP_DigestUpdate(context, name->bytes + offset, chunk) == 1;\n        if (success) {{ offset += chunk; }}\n    }}\n    unsigned int digest_length = 0;\n    if (success) {{ success = EVP_DigestFinal_ex(context, digest, &digest_length) == 1; }}\n    EVP_MD_CTX_free(context);\n    return success && (size_t)digest_length == expected_length;\n}}\n"
            ),
            format!(
                "    unsigned char bytes[16];\n    if (RAND_bytes(bytes, (int)sizeof(bytes)) != 1) {{\n        static const char message[] = \"Uuid.random: OpenSSL secure random generation failed\";\n        return ncr_result_error_{uuid_result}(message, sizeof(message) - 1, ncr_line, ncr_column);\n    }}\n    ncr_uuid_version_variant_{class}(bytes, 4);\n    ncr_obj_{class} *uuid = ncr_uuid_new_{class}(bytes, ncr_line, ncr_column);\n    return (ncr_obj_{uuid_result}){{ .tag = {uuid_ok}, .data.v{uuid_ok}.f0 = uuid }};\n"
            ),
            format!(
                "    unsigned char digest[EVP_MAX_MD_SIZE];\n    if (!ncr_uuid_digest_{class}(EVP_md5(), 16, {ns_md5}, {name_md5_p}, digest)) {{\n        static const char message[] = \"Uuid.nameMd5: OpenSSL MD5 operation failed\";\n        return ncr_result_error_{uuid_result}(message, sizeof(message) - 1, ncr_line, ncr_column);\n    }}\n    ncr_uuid_version_variant_{class}(digest, 3);\n    ncr_obj_{class} *uuid = ncr_uuid_new_{class}(digest, ncr_line, ncr_column);\n    return (ncr_obj_{uuid_result}){{ .tag = {uuid_ok}, .data.v{uuid_ok}.f0 = uuid }};\n",
                ns_md5 = parameter(name_md5, 0),
                name_md5_p = parameter(name_md5, 1),
            ),
            format!(
                "    unsigned char digest[EVP_MAX_MD_SIZE];\n    if (!ncr_uuid_digest_{class}(EVP_sha1(), 20, {ns_sha1}, {name_sha1_p}, digest)) {{\n        static const char message[] = \"Uuid.nameSha1: OpenSSL SHA-1 operation failed\";\n        return ncr_result_error_{uuid_result}(message, sizeof(message) - 1, ncr_line, ncr_column);\n    }}\n    ncr_uuid_version_variant_{class}(digest, 5);\n    ncr_obj_{class} *uuid = ncr_uuid_new_{class}(digest, ncr_line, ncr_column);\n    return (ncr_obj_{uuid_result}){{ .tag = {uuid_ok}, .data.v{uuid_ok}.f0 = uuid }};\n",
                ns_sha1 = parameter(name_sha1, 0),
                name_sha1_p = parameter(name_sha1, 1),
            ),
        )
    } else {
        (
            String::new(),
            String::new(),
            "    (void)ncr_line; (void)ncr_column;\n    ncr_fail(\"Uuid.random requires OpenSSL\", ncr_line, ncr_column);\n".to_owned(),
            format!(
                "    (void){ns}; (void){name}; (void)ncr_line; (void)ncr_column;\n    ncr_fail(\"Uuid.nameMd5 requires OpenSSL\", ncr_line, ncr_column);\n",
                ns = parameter(name_md5, 0),
                name = parameter(name_md5, 1),
            ),
            format!(
                "    (void){ns}; (void){name}; (void)ncr_line; (void)ncr_column;\n    ncr_fail(\"Uuid.nameSha1 requires OpenSSL\", ncr_line, ncr_column);\n",
                ns = parameter(name_sha1, 0),
                name = parameter(name_sha1, 1),
            ),
        )
    };

    let template = include_str!("../../stdlib/uuid.c.in");
    replace_placeholders(template.to_owned(), &[
        ("@UUID_CRYPTO_HEADERS@", crypto_headers),
        ("@UUID_CRYPTO_DIGEST_HELPER@", crypto_digest_helper),
        ("@UUID_RANDOM_BODY@", random_body),
        ("@UUID_NAME_MD5_BODY@", name_md5_body),
        ("@UUID_NAME_SHA1_BODY@", name_sha1_body),
        ("@UUID_PARSE_SIGNATURE@", signature(program, M::UuidParse)),
        ("@UUID_FROM_BYTES_SIGNATURE@", signature(program, M::UuidFromBytes)),
        ("@UUID_FROM_LONGS_SIGNATURE@", signature(program, M::UuidFromLongs)),
        ("@UUID_RANDOM_SIGNATURE@", signature(program, M::UuidRandom)),
        ("@UUID_NAME_MD5_SIGNATURE@", signature(program, M::UuidNameMd5)),
        ("@UUID_NAME_SHA1_SIGNATURE@", signature(program, M::UuidNameSha1)),
        ("@UUID_CLONE_SIGNATURE@", signature(program, M::UuidClone)),
        ("@UUID_TO_TEXT_SIGNATURE@", signature(program, M::UuidToText)),
        ("@UUID_TO_BYTES_SIGNATURE@", signature(program, M::UuidToBytes)),
        ("@UUID_MOST_SIGNIFICANT_BITS_SIGNATURE@", signature(program, M::UuidMostSignificantBits)),
        ("@UUID_LEAST_SIGNIFICANT_BITS_SIGNATURE@", signature(program, M::UuidLeastSignificantBits)),
        ("@UUID_VERSION_SIGNATURE@", signature(program, M::UuidVersion)),
        ("@UUID_VARIANT_SIGNATURE@", signature(program, M::UuidVariant)),
        ("@UUID_EQUALS_SIGNATURE@", signature(program, M::UuidEquals)),
        ("@UUID_COMPARE_TO_SIGNATURE@", signature(program, M::UuidCompareTo)),
        ("@UUID_PARSE_TEXT@", parameter(parse, 0)),
        ("@UUID_FROM_BYTES_BUFFER@", parameter(from_bytes, 0)),
        ("@UUID_FROM_LONGS_MOST@", parameter(from_longs, 0)),
        ("@UUID_FROM_LONGS_LEAST@", parameter(from_longs, 1)),
        ("@UUID_NAME_MD5_NAMESPACE@", parameter(name_md5, 0)),
        ("@UUID_NAME_MD5_NAME@", parameter(name_md5, 1)),
        ("@UUID_NAME_SHA1_NAMESPACE@", parameter(name_sha1, 0)),
        ("@UUID_NAME_SHA1_NAME@", parameter(name_sha1, 1)),
        ("@UUID_EQUALS_OTHER@", parameter(equals, 0)),
        ("@UUID_COMPARE_TO_OTHER@", parameter(compare_to, 0)),
        ("@UUID_RESULT_ID@", uuid_result.to_string()),
        ("@UUID_OK_TAG@", uuid_ok.to_string()),
        ("@UUID_ERR_TAG@", uuid_err.to_string()),
        ("@BUFFER_ID@", buffer.to_string()),
        ("@BUFFER_FACTORY_FN@", format!("ncr_fn_{}", capacity.id)),
        ("@BUFFER_RESULT_ID@", buffer_result.to_string()),
        ("@BUFFER_OK_TAG@", buffer_ok.to_string()),
        ("@BUFFER_ERR_TAG@", buffer_err.to_string()),
        ("@BUFFER_DROP_TEXT@", format!("ncr_drop_{text}")),
        ("@TEXT_ID@", text.to_string()),
        ("@TEXT_FROM_BYTES@", format!("ncr_text_from_bytes_{text}")),
        ("@UUID_RESULT_ERROR@", format!("ncr_result_error_{uuid_result}")),
    ])
}
