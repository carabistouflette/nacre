use crate::c_repr::ctype;
use crate::ir::{self as t, NativeClass as C, NativeMethod as M, Type};
use super::support::{
    native_function, replace_method_ids, replace_placeholders, result_id,
    template_section, text_class, variant_id,
};

pub(super) fn fields(class: C) -> String {
    match class {
        C::CryptoKey => "    void *pkey;\n    bool has_private;\n".to_owned(),
        C::CryptoDigest => "    void *context;\n    bool finished;\n".to_owned(),
        C::CryptoMac => "    void *algorithm;\n    void *context;\n    bool finished;\n".to_owned(),
        C::Certificate => "    void *certificate;\n    void *chain;\n".to_owned(),
        _ => unreachable!("crypto key storage requested for another native class"),
    }
}

pub(super) fn drop_fields(class: C, _class_id: usize) -> String {
    match class {
        C::CryptoKey => "    ncr_cryptokeys_key_destroy(p->pkey);\n".to_owned(),
        C::CryptoDigest => "    ncr_cryptokeys_digest_destroy(p->context);\n".to_owned(),
        C::CryptoMac => "    ncr_cryptokeys_mac_destroy(p->algorithm, p->context);\n".to_owned(),
        C::Certificate => "    ncr_cryptokeys_certificate_destroy(p->certificate, p->chain);\n".to_owned(),
        _ => unreachable!("crypto key drop requested for another native class"),
    }
}

pub(crate) fn drop_helper_prototypes(program: &t::Program) -> String {
    let mut out = String::new();
    for class in &program.classes {
        match class.native {
            Some(C::CryptoKey) => out.push_str("static void ncr_cryptokeys_key_destroy(void *);\n"),
            Some(C::CryptoDigest) => out.push_str("static void ncr_cryptokeys_digest_destroy(void *);\n"),
            Some(C::CryptoMac) => out.push_str("static void ncr_cryptokeys_mac_destroy(void *, void *);\n"),
            Some(C::Certificate) => out.push_str("static void ncr_cryptokeys_certificate_destroy(void *, void *);\n"),
            _ => {}
        }
    }
    out
}

fn class_id(program: &t::Program, is: impl Fn(C) -> bool) -> Option<usize> {
    program.classes.iter().find(|class| class.native.is_some_and(&is)).map(|class| class.id)
}

fn method(program: &t::Program, native: M) -> &t::Function {
    native_function(program, |candidate| candidate == native)
}


fn add_method_parameter(
    program: &t::Program,
    replacements: &mut Vec<(&'static str, String)>,
    placeholder: &'static str,
    native: M,
    index: usize,
) {
    replacements.push((placeholder, format!("ncr_v_{}", method_parameter_id(method(program, native), index))));
}

fn method_parameter_id(function: &t::Function, index: usize) -> usize {
    function.params[index].0
}

fn add_result(
    program: &t::Program,
    replacements: &mut Vec<(&'static str, String)>,
    native: M,
    result_placeholder: &'static str,
    ok_tag_placeholder: &'static str,
    error_result_placeholder: &'static str,
) {
    let function = method(program, native);
    let result = result_id(function);
    replacements.push((result_placeholder, ctype(program, function.return_type).into_owned()));
    replacements.push((ok_tag_placeholder, variant_id(program, result, "Ok").to_string()));
    replacements.push((error_result_placeholder, result.to_string()));
}

fn group<'a>(source: &'a str, name: &str) -> &'a str {
    let start = format!("/* CRYPTOKEYS_GROUP {name} */");
    let end = format!("/* CRYPTOKEYS_GROUP_END {name} */");
    template_section(source, &start, &end)
}

fn add_method_placeholders(
    program: &t::Program,
    replacements: &mut Vec<(&'static str, String)>,
    has_key: bool,
    has_digest: bool,
    has_mac: bool,
    has_certificate: bool,
) {
    if has_key {
        add_method_parameter(program, replacements, "@KEY_GENERATE_ALGORITHM_ID@", M::CryptoKeyGenerate, 0);
        add_method_parameter(program, replacements, "@KEY_GENERATE_BITS_ID@", M::CryptoKeyGenerate, 1);
        add_result(program, replacements, M::CryptoKeyGenerate, "@KEY_GENERATE_RESULT@", "@KEY_GENERATE_OK_TAG@", "@KEY_GENERATE_ERROR_RESULT@");
        add_method_parameter(program, replacements, "@KEY_PARSE_PRIVATE_PEM_ID@", M::CryptoKeyParsePrivatePem, 0);
        add_result(program, replacements, M::CryptoKeyParsePrivatePem, "@KEY_PARSE_PRIVATE_RESULT@", "@KEY_PARSE_PRIVATE_OK_TAG@", "@KEY_PARSE_PRIVATE_PEM_ERROR_RESULT@");
        add_method_parameter(program, replacements, "@KEY_PARSE_PUBLIC_PEM_ID@", M::CryptoKeyParsePublicPem, 0);
        add_result(program, replacements, M::CryptoKeyParsePublicPem, "@KEY_PARSE_PUBLIC_RESULT@", "@KEY_PARSE_PUBLIC_OK_TAG@", "@KEY_PARSE_PUBLIC_PEM_ERROR_RESULT@");
        add_result(program, replacements, M::CryptoKeyPublicPem, "@KEY_PUBLIC_PEM_RESULT@", "@KEY_PUBLIC_PEM_OK_TAG@", "@KEY_PUBLIC_PEM_ERROR_RESULT@");
        add_result(program, replacements, M::CryptoKeyPrivatePem, "@KEY_PRIVATE_PEM_RESULT@", "@KEY_PRIVATE_PEM_OK_TAG@", "@KEY_PRIVATE_PEM_ERROR_RESULT@");
        add_method_parameter(program, replacements, "@KEY_SIGN_DIGEST_ID@", M::CryptoKeySign, 0);
        add_method_parameter(program, replacements, "@KEY_SIGN_DATA_ID@", M::CryptoKeySign, 1);
        add_result(program, replacements, M::CryptoKeySign, "@KEY_SIGN_RESULT@", "@KEY_SIGN_OK_TAG@", "@KEY_SIGN_ERROR_RESULT@");
        add_method_parameter(program, replacements, "@KEY_VERIFY_DIGEST_ID@", M::CryptoKeyVerify, 0);
        add_method_parameter(program, replacements, "@KEY_VERIFY_DATA_ID@", M::CryptoKeyVerify, 1);
        add_method_parameter(program, replacements, "@KEY_VERIFY_SIGNATURE_ID@", M::CryptoKeyVerify, 2);
        add_result(program, replacements, M::CryptoKeyVerify, "@KEY_VERIFY_RESULT@", "@KEY_VERIFY_OK_TAG@", "@KEY_VERIFY_ERROR_RESULT@");
        add_method_parameter(program, replacements, "@KEY_AGREE_PEER_ID@", M::CryptoKeyAgree, 0);
        add_result(program, replacements, M::CryptoKeyAgree, "@KEY_AGREE_RESULT@", "@KEY_AGREE_OK_TAG@", "@KEY_AGREE_ERROR_RESULT@");
    }
    if has_digest {
        add_method_parameter(program, replacements, "@DIGEST_CREATE_ALGORITHM_ID@", M::CryptoDigestCreate, 0);
        add_result(program, replacements, M::CryptoDigestCreate, "@DIGEST_CREATE_RESULT@", "@DIGEST_CREATE_OK_TAG@", "@DIGEST_CREATE_ERROR_RESULT@");
        add_method_parameter(program, replacements, "@DIGEST_UPDATE_BYTES_ID@", M::CryptoDigestUpdate, 0);
        add_result(program, replacements, M::CryptoDigestUpdate, "@DIGEST_UPDATE_RESULT@", "@DIGEST_UPDATE_OK_TAG@", "@DIGEST_UPDATE_ERROR_RESULT@");
        add_result(program, replacements, M::CryptoDigestFinish, "@DIGEST_FINISH_RESULT@", "@DIGEST_FINISH_OK_TAG@", "@DIGEST_FINISH_ERROR_RESULT@");
    }
    if has_mac {
        add_method_parameter(program, replacements, "@MAC_CREATE_ALGORITHM_ID@", M::CryptoMacCreate, 0);
        add_method_parameter(program, replacements, "@MAC_CREATE_KEY_ID@", M::CryptoMacCreate, 1);
        add_result(program, replacements, M::CryptoMacCreate, "@MAC_CREATE_RESULT@", "@MAC_CREATE_OK_TAG@", "@MAC_CREATE_ERROR_RESULT@");
        add_method_parameter(program, replacements, "@MAC_UPDATE_BYTES_ID@", M::CryptoMacUpdate, 0);
        add_result(program, replacements, M::CryptoMacUpdate, "@MAC_UPDATE_RESULT@", "@MAC_UPDATE_OK_TAG@", "@MAC_UPDATE_ERROR_RESULT@");
        add_result(program, replacements, M::CryptoMacFinish, "@MAC_FINISH_RESULT@", "@MAC_FINISH_OK_TAG@", "@MAC_FINISH_ERROR_RESULT@");
    }
    if has_certificate {
        add_method_parameter(program, replacements, "@CERTIFICATE_PARSE_PEM_ID@", M::CertificateParsePem, 0);
        add_result(program, replacements, M::CertificateParsePem, "@CERTIFICATE_PARSE_RESULT@", "@CERTIFICATE_PARSE_OK_TAG@", "@CERTIFICATE_PARSE_ERROR_RESULT@");
        add_method_parameter(program, replacements, "@CERTIFICATE_PARSE_PEM_CHAIN_ID@", M::CertificateParsePemChain, 0);
        add_result(program, replacements, M::CertificateParsePemChain, "@CERTIFICATE_PARSE_CHAIN_RESULT@", "@CERTIFICATE_PARSE_CHAIN_OK_TAG@", "@CERTIFICATE_PARSE_CHAIN_ERROR_RESULT@");
        add_result(program, replacements, M::CertificateToPem, "@CERTIFICATE_PEM_RESULT@", "@CERTIFICATE_PEM_OK_TAG@", "@CERTIFICATE_PEM_ERROR_RESULT@");
        add_result(program, replacements, M::CertificateToPemChain, "@CERTIFICATE_PEM_CHAIN_RESULT@", "@CERTIFICATE_PEM_CHAIN_OK_TAG@", "@CERTIFICATE_PEM_CHAIN_ERROR_RESULT@");
        add_result(program, replacements, M::CertificateSubject, "@CERTIFICATE_SUBJECT_RESULT@", "@CERTIFICATE_SUBJECT_OK_TAG@", "@CERTIFICATE_SUBJECT_ERROR_RESULT@");
        add_result(program, replacements, M::CertificateIssuer, "@CERTIFICATE_ISSUER_RESULT@", "@CERTIFICATE_ISSUER_OK_TAG@", "@CERTIFICATE_ISSUER_ERROR_RESULT@");
        add_result(program, replacements, M::CertificateSerialText, "@CERTIFICATE_SERIAL_RESULT@", "@CERTIFICATE_SERIAL_OK_TAG@", "@CERTIFICATE_SERIAL_ERROR_RESULT@");
        add_result(program, replacements, M::CertificateNotBeforeUnixSeconds, "@CERTIFICATE_NOT_BEFORE_RESULT@", "@CERTIFICATE_NOT_BEFORE_OK_TAG@", "@CERTIFICATE_NOT_BEFORE_ERROR_RESULT@");
        add_result(program, replacements, M::CertificateNotAfterUnixSeconds, "@CERTIFICATE_NOT_AFTER_RESULT@", "@CERTIFICATE_NOT_AFTER_OK_TAG@", "@CERTIFICATE_NOT_AFTER_ERROR_RESULT@");
        add_method_parameter(program, replacements, "@CERTIFICATE_MATCHES_HOST_ID@", M::CertificateMatchesHost, 0);
        add_result(program, replacements, M::CertificateMatchesHost, "@CERTIFICATE_HOST_RESULT@", "@CERTIFICATE_HOST_OK_TAG@", "@CERTIFICATE_HOST_ERROR_RESULT@");
        add_method_parameter(program, replacements, "@CERTIFICATE_VERIFY_HOST_ID@", M::CertificateVerify, 0);
        add_method_parameter(program, replacements, "@CERTIFICATE_VERIFY_CA_ID@", M::CertificateVerify, 1);
        add_method_parameter(program, replacements, "@CERTIFICATE_VERIFY_TIME_ID@", M::CertificateVerify, 2);
        add_result(program, replacements, M::CertificateVerify, "@CERTIFICATE_VERIFY_RESULT@", "@CERTIFICATE_VERIFY_OK_TAG@", "@CERTIFICATE_VERIFY_ERROR_RESULT@");
    }
}

pub(super) fn implementation(program: &t::Program) -> String {
    let key = class_id(program, |class| matches!(class, C::CryptoKey));
    let digest = class_id(program, |class| matches!(class, C::CryptoDigest));
    let mac = class_id(program, |class| matches!(class, C::CryptoMac));
    let certificate = class_id(program, |class| matches!(class, C::Certificate));
    if key.is_none() && digest.is_none() && mac.is_none() && certificate.is_none() { return String::new(); }

    let template = include_str!("../../stdlib/crypto_keys.c.in");
    let needs_algorithm = key.is_some() || digest.is_some() || mac.is_some();
    let needs_pem = key.is_some() || certificate.is_some();
    let first_group = "/* CRYPTOKEYS_GROUP ALGORITHM */";
    let mut body = template.split_once(first_group).expect("crypto keys template algorithm group").0.to_owned();
    if needs_algorithm { body.push_str(group(template, "ALGORITHM")); }
    if needs_pem { body.push_str(group(template, "PEM")); }
    let needs_buffer = key.is_some() || digest.is_some() || mac.is_some();
    if needs_buffer { body.push_str(group(template, "BUFFER")); }
    if digest.is_some() || mac.is_some() { body.push_str(group(template, "BUFFER_COPY")); }
    if key.is_some() { body.push_str(group(template, "KEY")); }
    if digest.is_some() { body.push_str(group(template, "DIGEST")); }
    if mac.is_some() { body.push_str(group(template, "MAC")); }
    if certificate.is_some() { body.push_str(group(template, "CERTIFICATE")); }

    let text = text_class(program).expect("crypto key APIs require Text");
    let mut replacements = vec![
        ("@TEXT_TYPE@", ctype(program, Type::Object(text)).into_owned()),
        ("@TEXT_FROM_BYTES@", format!("ncr_text_from_bytes_{text}")),
    ];
    if needs_buffer {
        let buffer = class_id(program, |class| matches!(class, C::ByteBuffer))
            .expect("crypto key, digest, and MAC APIs require ByteBuffer");
        replacements.push(("@BUFFER_TYPE@", ctype(program, Type::Object(buffer)).into_owned()));
    }
    if let Some(id) = key { replacements.push(("@KEY_TYPE@", ctype(program, Type::Object(id)).into_owned())); }
    if let Some(id) = digest { replacements.push(("@DIGEST_TYPE@", ctype(program, Type::Object(id)).into_owned())); }
    if let Some(id) = mac { replacements.push(("@MAC_TYPE@", ctype(program, Type::Object(id)).into_owned())); }
    if let Some(id) = certificate {
        let path = class_id(program, |class| matches!(class, C::Path))
            .expect("Certificate.verify requires Path");
        replacements.push(("@CERTIFICATE_TYPE@", ctype(program, Type::Object(id)).into_owned()));
        replacements.push(("@PATH_TYPE@", ctype(program, Type::Object(path)).into_owned()));
    }
    add_method_placeholders(program, &mut replacements, key.is_some(), digest.is_some(), mac.is_some(), certificate.is_some());
    body = replace_placeholders(body, &replacements);
    body = replace_method_ids(body, program, None);
    debug_assert!(!super::support::has_placeholder(&body), "unexpanded crypto keys placeholder");
    body
}
