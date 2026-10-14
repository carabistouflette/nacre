//! Human-readable nominal types and specializations from fully checked IR.
use std::fmt::Write;
use crate::ir::Program;

pub(crate) fn dump(typed: &Program) -> String {
    let mut out = String::from("Nacre — nominal types and specializations\n");
    for class in &typed.classes {
        let kind = if class.variants.is_some() { "enum" } else if class.is_interface { "interface" } else { "class" };
        writeln!(out, "#{} {kind} {}", class.id, class.name).unwrap();
        if let Some(variants) = &class.variants {
            for variant in variants {
                write!(out, "  {}(", variant.name).unwrap();
                for (index, ty) in variant.fields.iter().enumerate() {
                    if index > 0 { out.push_str(", "); }
                    write!(out, "{}", ty.name(|id| typed.classes[id].name.as_str())).unwrap();
                }
                out.push_str(")\n");
            }
        }
        for (interface, _) in &class.implementations {
            writeln!(out, "  implements {}", typed.classes[*interface].name).unwrap();
        }
    }
    out
}
