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
        Type::Borrow { class: id, mutable } => {
            if program.classes[id].is_interface { Cow::Owned(format!("ncr_obj_{id}")) }
            else if mutable { Cow::Owned(format!("ncr_obj_{id} *restrict")) }
            else { Cow::Owned(format!("ncr_obj_{id} *")) }
        }
    }
}
pub(crate) fn object_zero(program: &t::Program, id: usize) -> String {
    if program.classes[id].is_interface { format!("(ncr_obj_{id}){{NULL, NULL}}") }
    else if program.classes[id].variants.is_some() { format!("(ncr_obj_{id}){{.tag = -1}}") }
    else { "NULL".to_owned() }
}
