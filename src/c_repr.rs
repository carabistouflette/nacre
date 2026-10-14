//! C value representations shared by expression and native-method emission.
use crate::ir::{self as t, Type};
pub(crate) fn ctype(program: &t::Program, ty: Type) -> String {
    match ty {
        Type::Byte => "uint8_t".to_owned(), Type::Char => "uint32_t".to_owned(),
        Type::Int => "int32_t".to_owned(), Type::Long => "int64_t".to_owned(),
        Type::Double => "double".to_owned(), Type::Unit => "uint8_t".to_owned(),
        Type::Boolean => "bool".to_owned(), Type::String => "ncr_string".to_owned(),
        Type::Void => "void".to_owned(),
        Type::Object(id) => {
            if program.classes[id].variants.is_some() || program.classes[id].is_interface {
                format!("ncr_obj_{id}")
            } else {
                format!("ncr_obj_{id} *")
            }
        }
        Type::Borrow { class: id, .. } => {
            if program.classes[id].is_interface { format!("ncr_obj_{id}") }
            else { format!("ncr_obj_{id} *") }
        }
    }
}
pub(crate) fn object_zero(program: &t::Program, id: usize) -> String {
    if program.classes[id].is_interface { format!("(ncr_obj_{id}){{NULL, NULL}}") }
    else if program.classes[id].variants.is_some() { format!("(ncr_obj_{id}){{.tag = -1}}") }
    else { "NULL".to_owned() }
}
