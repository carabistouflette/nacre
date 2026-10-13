use crate::ir::{self as t, CallTarget, Expr, ExprKind, NativeClass as C, NativeMethod as M, Stmt};
use super::support::{native_function, replace_method_ids, result_id, variant_id};

struct MethodBinding {
    name: &'static str,
    method: M,
    parameters: &'static [(&'static str, usize)],
    result: bool,
}

const METHODS: &[MethodBinding] = &[
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

fn is_bigint_method(method: M) -> bool {
    matches!(method,
        M::BigIntFromLong | M::BigIntParse | M::BigIntClone | M::BigIntToText |
        M::BigIntCompareTo | M::BigIntSignum | M::BigIntBitLength | M::BigIntAdd |
        M::BigIntSubtract | M::BigIntMultiply | M::BigIntDivide | M::BigIntRemainder |
        M::BigIntGcd | M::BigIntMod | M::BigIntPow | M::BigIntModPow |
        M::BigIntModInverse | M::BigIntNegate | M::BigIntAbs | M::BigIntShiftLeft |
        M::BigIntShiftRight | M::BigIntBitAnd | M::BigIntBitOr | M::BigIntBitXor |
        M::BigIntBitNot | M::BigIntTestBit | M::BigIntToLong)
}

fn is_bigdecimal_method(method: M) -> bool {
    matches!(method,
        M::BigDecimalParse | M::BigDecimalFromLong | M::BigDecimalClone |
        M::BigDecimalToText | M::BigDecimalScale | M::BigDecimalCompareTo |
        M::BigDecimalSignum | M::BigDecimalAdd | M::BigDecimalSubtract |
        M::BigDecimalMultiply | M::BigDecimalDivide | M::BigDecimalRescale |
        M::BigDecimalNegate | M::BigDecimalAbs | M::BigDecimalToLong)
}

fn expression_uses_class(program: &t::Program, expression: &Expr, bigint: bool) -> bool {
    match &expression.kind {
        ExprKind::Call { target, args, .. } => {
            if let CallTarget::Function(id) = target {
                if let Some(method) = program.functions[*id].native {
                    if (bigint && is_bigint_method(method)) || (!bigint && is_bigdecimal_method(method)) {
                        return true;
                    }
                }
            }
            args.iter().any(|argument| expression_uses_class(program, argument, bigint))
        }
        ExprKind::Drop(value) | ExprKind::Unary { expr: value, .. } |
        ExprKind::Try { value, .. } | ExprKind::Cast { value } => expression_uses_class(program, value, bigint),
        ExprKind::Binary { left, right, .. } => expression_uses_class(program, left, bigint) ||
            expression_uses_class(program, right, bigint),
        ExprKind::New { args, .. } | ExprKind::Variant { args, .. } =>
            args.iter().any(|argument| expression_uses_class(program, argument, bigint)),
        ExprKind::MapSlot { index, .. } => expression_uses_class(program, index, bigint),
        ExprKind::Upcast { value, .. } => expression_uses_class(program, value, bigint),
        ExprKind::Char(_) | ExprKind::Int(_) | ExprKind::Long(_) | ExprKind::Double(_) |
        ExprKind::Unit | ExprKind::Boolean(_) | ExprKind::String(_) | ExprKind::Read(_) |
        ExprKind::Move(_) | ExprKind::Borrow { .. } | ExprKind::MapCapacity(_) => false,
    }
}

fn block_uses_class(program: &t::Program, block: &t::Block, bigint: bool) -> bool {
    block.iter().any(|statement| match statement {
        Stmt::Let { value, .. } => value.as_ref().is_some_and(|value| expression_uses_class(program, value, bigint)),
        Stmt::Assign { value, .. } | Stmt::Expr(value) => expression_uses_class(program, value, bigint),
        Stmt::If { condition, then_block, else_block } => expression_uses_class(program, condition, bigint) ||
            block_uses_class(program, then_block, bigint) || else_block.as_ref().is_some_and(|block| block_uses_class(program, block, bigint)),
        Stmt::While { condition, body } => expression_uses_class(program, condition, bigint) || block_uses_class(program, body, bigint),
        Stmt::Match { value, arms } => expression_uses_class(program, value, bigint) ||
            arms.iter().any(|arm| block_uses_class(program, &arm.body, bigint)),
        Stmt::Return(value) => value.as_ref().is_some_and(|value| expression_uses_class(program, value, bigint)),
        Stmt::Block(block) => block_uses_class(program, block, bigint),
        Stmt::Break | Stmt::Continue => false,
    })
}

fn class_is_used(program: &t::Program, bigint: bool) -> bool {
    program.functions.iter().filter(|function| function.native.is_none())
        .any(|function| block_uses_class(program, &function.body, bigint))
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
    is_bigint_method(method) || is_bigdecimal_method(method)
}

pub(super) fn implementation(program: &t::Program) -> String {
    if !is_used(program) { return String::new(); }
    let bigint = program.classes.iter().find(|class| matches!(class.native, Some(C::BigInt)))
        .expect("BigInt prelude class").id;
    let decimal = program.classes.iter().find(|class| matches!(class.native, Some(C::BigDecimal)))
        .expect("BigDecimal prelude class").id;
    let rounding = program.classes.iter().find(|class| class.name == "RoundingMode")
        .expect("RoundingMode prelude enum").id;
    let text = program.classes.iter().find(|class| matches!(class.native, Some(C::Text)))
        .expect("Text prelude class").id;
    let mut body = include_str!("../../stdlib/big_numbers.c.in").to_owned();
    for (placeholder, value) in [
        ("@BIGINT_CLASS@", bigint.to_string()),
        ("@BIGDECIMAL_CLASS@", decimal.to_string()),
        ("@ROUNDING_CLASS@", rounding.to_string()),
        ("@TEXT_CLASS@", text.to_string()),
        ("@ROUND_DOWN@", variant_id(program, rounding, "Down").to_string()),
        ("@ROUND_UP@", variant_id(program, rounding, "Up").to_string()),
        ("@ROUND_FLOOR@", variant_id(program, rounding, "Floor").to_string()),
        ("@ROUND_CEILING@", variant_id(program, rounding, "Ceiling").to_string()),
        ("@ROUND_HALF_UP@", variant_id(program, rounding, "HalfUp").to_string()),
        ("@ROUND_HALF_DOWN@", variant_id(program, rounding, "HalfDown").to_string()),
        ("@ROUND_HALF_EVEN@", variant_id(program, rounding, "HalfEven").to_string()),
        ("@ROUND_UNNECESSARY@", variant_id(program, rounding, "Unnecessary").to_string()),
    ] {
        body = body.replace(placeholder, &value);
    }
    for binding in METHODS {
        let function = native_function(program, |method| method == binding.method);
        for (placeholder, index) in binding.parameters {
            body = body.replace(placeholder, &format!("ncr_v_{}", function.params[*index].0));
        }
        if binding.result {
            let class = result_id(function);
            body = body.replace(&format!("@{}_RESULT@", binding.name), &class.to_string());
            body = body.replace(&format!("@{}_OK@", binding.name),
                &variant_id(program, class, "Ok").to_string());
        }
    }
    body = replace_method_ids(body, program, None);
    debug_assert!(!body.contains('@'), "unexpanded BigInt/BigDecimal placeholder");
    body
}
