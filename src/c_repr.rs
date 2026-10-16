//! C value representations shared by expression and native-method emission.
use crate::ir::{self as t, Type};
use std::borrow::Cow;
pub(crate) fn ctype(program: &t::Program, ty: Type) -> Cow<'static, str> {
    match ty {
        Type::Byte => Cow::Borrowed("uint8_t"), Type::Char => Cow::Borrowed("uint32_t"),
        Type::Int => Cow::Borrowed("int32_t"), Type::Long => Cow::Borrowed("int64_t"),
        Type::Double => Cow::Borrowed("double"), Type::Unit => Cow::Borrowed("uint8_t"),
        Type::Boolean => Cow::Borrowed("bool"), Type::String => Cow::Borrowed("ncr_string"),
        Type::Void => Cow::Borrowed("void"),
        Type::Object(id) => {
            if program.classes[id].variants.is_some() || program.classes[id].is_interface {
                Cow::Owned(format!("ncr_obj_{id}"))
            } else {
                Cow::Owned(format!("ncr_obj_{id} *"))
            }
        }
        Type::Borrow { class: id, .. } => {
            if program.classes[id].is_interface { Cow::Owned(format!("ncr_obj_{id}")) }
            else { Cow::Owned(format!("ncr_obj_{id} *")) }
        }
    }
}
pub(crate) fn ctype_param(program: &t::Program, ty: Type) -> Cow<'static, str> {
    match ty {
        Type::Borrow { class: id, mutable: true } if !program.classes[id].is_interface => {
            Cow::Owned(format!("ncr_obj_{id} *restrict"))
        }
        _ => ctype(program, ty),
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DirectInputAbi {
    /// A scalar or thin pointer value; ownership is not duplicated.
    Value,
    /// An address of value storage, without C aggregate ABI classification.
    Address,
}

/// Shared input transport policy for LLVM definitions/calls and their C adapters.
///
/// Owned values still move once; borrowed values never acquire ownership.
/// Result slots and native/interface dispatch contracts are independent of this
/// policy. Unit, String, inline sums, and fat interface values retain addresses.
pub(crate) fn direct_input_abi(program: &t::Program, ty: Type) -> DirectInputAbi {
    match ty {
        Type::Byte | Type::Char | Type::Int | Type::Long | Type::Double | Type::Boolean => {
            DirectInputAbi::Value
        }
        Type::Object(id)
            if !program.classes[id].is_interface && program.classes[id].variants.is_none() =>
        {
            DirectInputAbi::Value
        }
        Type::Borrow { class, .. } if !program.classes[class].is_interface => {
            DirectInputAbi::Value
        }
        Type::Void | Type::Unit | Type::String | Type::Object(_) | Type::Borrow { .. } => {
            DirectInputAbi::Address
        }
    }
}
pub(crate) fn object_zero(program: &t::Program, id: usize) -> String {
    if program.classes[id].is_interface { format!("(ncr_obj_{id}){{NULL, NULL}}") }
    else if program.classes[id].variants.is_some() { format!("(ncr_obj_{id}){{.tag = -1}}") }
    else { "NULL".to_owned() }
}
