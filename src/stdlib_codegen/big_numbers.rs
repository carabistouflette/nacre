use std::borrow::Cow;
use crate::ir::{self as t, NativeClass as C, NativeMethod as M};
use super::support::{has_placeholder, native_function, replace_method_ids, replace_placeholders, result_id, variant_id};

struct MethodBinding {
    name: &'static str,
    method: M,
    parameters: &'static [(&'static str, usize)],
    result: bool,
}

const BIGINT_METHODS: &[MethodBinding] = &[
    MethodBinding { name: "BIGINT_FROM_LONG", method: M::BigIntFromLong, parameters: &[("@BIGINT_FROM_LONG_VALUE@", 0)], result: false },
    MethodBinding { name: "BIGINT_PARSE", method: M::BigIntParse, parameters: &[("@BIGINT_PARSE_TEXT@", 0), ("@BIGINT_PARSE_RADIX@", 1)], result: true },
    MethodBinding { name: "BIGINT_CLONE", method: M::BigIntClone, parameters: &[], result: false },
    MethodBinding { name: "BIGINT_TO_TEXT", method: M::BigIntToText, parameters: &[("@BIGINT_TO_TEXT_RADIX@", 0)], result: true },
    MethodBinding { name: "BIGINT_COMPARE_TO", method: M::BigIntCompareTo, parameters: &[("@BIGINT_COMPARE_TO_OTHER@", 0)], result: false },
    MethodBinding { name: "BIGINT_SIGNUM", method: M::BigIntSignum, parameters: &[], result: false },
    MethodBinding { name: "BIGINT_BIT_LENGTH", method: M::BigIntBitLength, parameters: &[], result: true },
    MethodBinding { name: "BIGINT_ADD", method: M::BigIntAdd, parameters: &[("@BIGINT_ADD_OTHER@", 0)], result: false },
    MethodBinding { name: "BIGINT_SUBTRACT", method: M::BigIntSubtract, parameters: &[("@BIGINT_SUBTRACT_OTHER@", 0)], result: false },
    MethodBinding { name: "BIGINT_MULTIPLY", method: M::BigIntMultiply, parameters: &[("@BIGINT_MULTIPLY_OTHER@", 0)], result: false },
    MethodBinding { name: "BIGINT_DIVIDE", method: M::BigIntDivide, parameters: &[("@BIGINT_DIVIDE_OTHER@", 0)], result: true },
    MethodBinding { name: "BIGINT_REMAINDER", method: M::BigIntRemainder, parameters: &[("@BIGINT_REMAINDER_OTHER@", 0)], result: true },
    MethodBinding { name: "BIGINT_GCD", method: M::BigIntGcd, parameters: &[("@BIGINT_GCD_OTHER@", 0)], result: false },
    MethodBinding { name: "BIGINT_MOD", method: M::BigIntMod, parameters: &[("@BIGINT_MOD_MODULUS@", 0)], result: true },
    MethodBinding { name: "BIGINT_POW", method: M::BigIntPow, parameters: &[("@BIGINT_POW_EXPONENT@", 0)], result: true },
    MethodBinding { name: "BIGINT_MOD_POW", method: M::BigIntModPow, parameters: &[("@BIGINT_MOD_POW_EXPONENT@", 0), ("@BIGINT_MOD_POW_MODULUS@", 1)], result: true },
    MethodBinding { name: "BIGINT_MOD_INVERSE", method: M::BigIntModInverse, parameters: &[("@BIGINT_MOD_INVERSE_MODULUS@", 0)], result: true },
    MethodBinding { name: "BIGINT_NEGATE", method: M::BigIntNegate, parameters: &[], result: false },
    MethodBinding { name: "BIGINT_ABS", method: M::BigIntAbs, parameters: &[], result: false },
    MethodBinding { name: "BIGINT_SHIFT_LEFT", method: M::BigIntShiftLeft, parameters: &[("@BIGINT_SHIFT_LEFT_BITS@", 0)], result: true },
    MethodBinding { name: "BIGINT_SHIFT_RIGHT", method: M::BigIntShiftRight, parameters: &[("@BIGINT_SHIFT_RIGHT_BITS@", 0)], result: true },
    MethodBinding { name: "BIGINT_BIT_AND", method: M::BigIntBitAnd, parameters: &[("@BIGINT_BIT_AND_OTHER@", 0)], result: false },
    MethodBinding { name: "BIGINT_BIT_OR", method: M::BigIntBitOr, parameters: &[("@BIGINT_BIT_OR_OTHER@", 0)], result: false },
    MethodBinding { name: "BIGINT_BIT_XOR", method: M::BigIntBitXor, parameters: &[("@BIGINT_BIT_XOR_OTHER@", 0)], result: false },
    MethodBinding { name: "BIGINT_BIT_NOT", method: M::BigIntBitNot, parameters: &[], result: false },
    MethodBinding { name: "BIGINT_TEST_BIT", method: M::BigIntTestBit, parameters: &[("@BIGINT_TEST_BIT_BIT@", 0)], result: true },
    MethodBinding { name: "BIGINT_TO_LONG", method: M::BigIntToLong, parameters: &[], result: true },
];

const BIGDECIMAL_METHODS: &[MethodBinding] = &[
    MethodBinding { name: "BIGDECIMAL_PARSE", method: M::BigDecimalParse, parameters: &[("@BIGDECIMAL_PARSE_TEXT@", 0)], result: true },
    MethodBinding { name: "BIGDECIMAL_FROM_LONG", method: M::BigDecimalFromLong, parameters: &[("@BIGDECIMAL_FROM_LONG_VALUE@", 0)], result: false },
    MethodBinding { name: "BIGDECIMAL_CLONE", method: M::BigDecimalClone, parameters: &[], result: false },
    MethodBinding { name: "BIGDECIMAL_TO_TEXT", method: M::BigDecimalToText, parameters: &[], result: true },
    MethodBinding { name: "BIGDECIMAL_SCALE", method: M::BigDecimalScale, parameters: &[], result: false },
    MethodBinding { name: "BIGDECIMAL_COMPARE_TO", method: M::BigDecimalCompareTo, parameters: &[("@BIGDECIMAL_COMPARE_TO_OTHER@", 0)], result: false },
    MethodBinding { name: "BIGDECIMAL_SIGNUM", method: M::BigDecimalSignum, parameters: &[], result: false },
    MethodBinding { name: "BIGDECIMAL_ADD", method: M::BigDecimalAdd, parameters: &[("@BIGDECIMAL_ADD_OTHER@", 0)], result: true },
    MethodBinding { name: "BIGDECIMAL_SUBTRACT", method: M::BigDecimalSubtract, parameters: &[("@BIGDECIMAL_SUBTRACT_OTHER@", 0)], result: true },
    MethodBinding { name: "BIGDECIMAL_MULTIPLY", method: M::BigDecimalMultiply, parameters: &[("@BIGDECIMAL_MULTIPLY_OTHER@", 0)], result: true },
    MethodBinding { name: "BIGDECIMAL_DIVIDE", method: M::BigDecimalDivide, parameters: &[("@BIGDECIMAL_DIVIDE_OTHER@", 0), ("@BIGDECIMAL_DIVIDE_RESULT_SCALE@", 1), ("@BIGDECIMAL_DIVIDE_ROUNDING@", 2)], result: true },
    MethodBinding { name: "BIGDECIMAL_RESCALE", method: M::BigDecimalRescale, parameters: &[("@BIGDECIMAL_RESCALE_SCALE@", 0), ("@BIGDECIMAL_RESCALE_ROUNDING@", 1)], result: true },
    MethodBinding { name: "BIGDECIMAL_NEGATE", method: M::BigDecimalNegate, parameters: &[], result: false },
    MethodBinding { name: "BIGDECIMAL_ABS", method: M::BigDecimalAbs, parameters: &[], result: false },
    MethodBinding { name: "BIGDECIMAL_TO_LONG", method: M::BigDecimalToLong, parameters: &[], result: true },
];

fn class_is_used(program: &t::Program, bigint: bool) -> bool {
    program.classes.iter().any(|class| {
        if bigint {
            matches!(class.native, Some(C::BigInt))
        } else {
            matches!(class.native, Some(C::BigDecimal))
        }
    })
}

pub(super) fn is_used(program: &t::Program) -> bool {
    class_is_used(program, true) || class_is_used(program, false)
}

pub(super) fn fields(class: C) -> String {
    match class {
        C::BigInt => "    void *value;\n".to_owned(),
        C::BigDecimal => "    void *coefficient;\n    int32_t scale;\n".to_owned(),
        _ => unreachable!("numeric native class fields"),
    }
}

pub(super) fn drop_fields(program: &t::Program, class: C) -> String {
    match class {
        C::BigInt if class_is_used(program, true) =>
            "    mpz_clear((mpz_ptr)p->value);\n    ncr_deallocate(p->value);\n".to_owned(),
        C::BigDecimal if class_is_used(program, false) =>
            "    mpz_clear((mpz_ptr)p->coefficient);\n    ncr_deallocate(p->coefficient);\n".to_owned(),
        C::BigInt | C::BigDecimal => String::new(),
        _ => unreachable!("numeric native class fields"),
    }
}
pub(super) fn is_numeric_method(method: M) -> bool {
    BIGINT_METHODS.iter().chain(BIGDECIMAL_METHODS)
        .any(|binding| binding.method == method)
}

fn selected_template(bigint: bool, decimal: bool) -> String {
    let source = include_str!("../../stdlib/big_numbers.c.in");
    let mut body = String::with_capacity(source.len());
    let mut include = true;
    for line in source.split_inclusive('\n') {
        if let Some(name) = line.strip_prefix("/* BIG_NUMBERS_GROUP ")
            .and_then(|line| line.strip_suffix(" */\n"))
        {
            include = if name.starts_with("bigint_") {
                bigint
            } else if name.starts_with("decimal_") || name == "rounding_macros" {
                decimal
            } else {
                unreachable!("unknown numeric template group: {name}")
            };
        } else if line.starts_with("/* BIG_NUMBERS_GROUP_END ") {
            include = true;
        } else if include {
            body.push_str(line);
        }
    }
    body
}

pub(super) fn implementation(program: &t::Program) -> String {
    let bigint = program.classes.iter().find(|class| matches!(class.native, Some(C::BigInt)))
        .map(|class| class.id);
    let decimal = program.classes.iter().find(|class| matches!(class.native, Some(C::BigDecimal)))
        .map(|class| class.id);
    let bigint_used = bigint.is_some() && class_is_used(program, true);
    let decimal_used = decimal.is_some() && class_is_used(program, false);
    if !bigint_used && !decimal_used { return String::new(); }

    let text = program.classes.iter().find(|class| matches!(class.native, Some(C::Text)))
        .expect("numeric prelude requires Text").id;
    let rounding = decimal_used.then(|| program.classes.iter()
        .find(|class| class.name == "RoundingMode").expect("BigDecimal API requires RoundingMode").id);

    let mut substitutions: Vec<(Cow<'static, str>, String)> = Vec::new();
    if let Some(bigint) = bigint {
        substitutions.push((Cow::Borrowed("@BIGINT_CLASS@"), bigint.to_string()));
    }
    if let Some(decimal) = decimal {
        substitutions.push((Cow::Borrowed("@BIGDECIMAL_CLASS@"), decimal.to_string()));
    }
    substitutions.push((Cow::Borrowed("@TEXT_CLASS@"), text.to_string()));
    if let Some(rounding) = rounding {
        for (placeholder, value) in [
            ("@ROUNDING_CLASS@", rounding.to_string()),
            ("@ROUND_DOWN@", variant_id(program, rounding, "Down").to_string()),
            ("@ROUND_UP@", variant_id(program, rounding, "Up").to_string()),
            ("@ROUND_FLOOR@", variant_id(program, rounding, "Floor").to_string()),
            ("@ROUND_CEILING@", variant_id(program, rounding, "Ceiling").to_string()),
            ("@ROUND_HALF_UP@", variant_id(program, rounding, "HalfUp").to_string()),
            ("@ROUND_HALF_DOWN@", variant_id(program, rounding, "HalfDown").to_string()),
            ("@ROUND_HALF_EVEN@", variant_id(program, rounding, "HalfEven").to_string()),
            ("@ROUND_UNNECESSARY@", variant_id(program, rounding, "Unnecessary").to_string()),
        ] {
            substitutions.push((Cow::Borrowed(placeholder), value));
        }
    }
    for (present_class, bindings) in [
        (bigint_used, BIGINT_METHODS),
        (decimal_used, BIGDECIMAL_METHODS),
    ] {
        if !present_class { continue; }
        for binding in bindings {
            let function = native_function(program, |method| method == binding.method);
            for (placeholder, index) in binding.parameters {
                substitutions.push((
                    Cow::Borrowed(placeholder),
                    format!("ncr_v_{}", function.params[*index].0),
                ));
            }
            if binding.result {
                let class = result_id(function);
                substitutions.push((
                    Cow::Owned(format!("@{}_RESULT@", binding.name)),
                    class.to_string(),
                ));
                substitutions.push((
                    Cow::Owned(format!("@{}_OK@", binding.name)),
                    variant_id(program, class, "Ok").to_string(),
                ));
            }
        }
    }
    let mut body = selected_template(bigint_used, decimal_used);
    body = replace_placeholders(body, &substitutions);
    body = replace_method_ids(body, program, None);
    debug_assert!(!has_placeholder(&body), "unexpanded BigInt/BigDecimal placeholder");
    body
}
