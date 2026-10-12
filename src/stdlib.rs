//! Trusted prelude injection. User programs cannot declare native functions.
use crate::ast::{self as a, NativeClass, Type};
use crate::diagnostic::{Diagnostic, Span};
use crate::ir::{NativeClass as C, NativeMethod as M};

pub fn inject(program: &mut a::Program) -> Result<(), Diagnostic> {
    for class in &program.classes {
        if matches!(class.name.as_str(), "List" | "Text" | "Option" | "Result" | "Console" | "Files" | "Args") {
            return Err(Diagnostic::new(class.span,
                format!("[E5001] `{}` est un type reserve de la bibliotheque standard", class.name)));
        }
    }
    let mut tokens = crate::lexer::lex(include_str!("../stdlib/prelude.ncr"))?;
    // Prelude locations must not index into unrelated user text.
    for token in &mut tokens { token.span = Span::default(); }
    let mut prelude = crate::parser::parse(tokens)?;
    for class in &mut prelude.classes {
        class.is_builtin = true;
        class.native = Some(match class.name.as_str() {
            "List" => NativeClass::List(Type::Named("T".to_owned())),
            "Text" => NativeClass::Text,
            "Console" => NativeClass::Console,
            "Files" => NativeClass::Files,
            "Args" => NativeClass::Args,
            _ => continue,
        });
    }
    program.classes.extend(prelude.classes);
    Ok(())
}

pub fn method(class: C, name: &str, constructor: bool) -> M {
    if constructor {
        return match class { C::List(_) => M::ListNew, C::Text => M::TextNew,
            C::Console | C::Files | C::Args => unreachable!("static native class has no constructor") };
    }
    match (class, name) {
        (C::List(_), "add") => M::ListAdd,
        (C::List(_), "size") => M::ListSize,
        (C::List(_), "capacity") => M::ListCapacity,
        (C::List(_), "isEmpty") => M::ListIsEmpty,
        (C::List(_), "get") => M::ListGet,
        (C::List(_), "getRef") => M::ListGetRef,
        (C::List(_), "getMut") => M::ListGetMut,
        (C::List(_), "set") => M::ListSet,
        (C::List(_), "pop") => M::ListPop,
        (C::List(_), "remove") => M::ListRemove,
        (C::List(_), "tryGet") => M::ListTryGet,
        (C::List(_), "tryPop") => M::ListTryPop,
        (C::List(_), "tryRemove") => M::ListTryRemove,
        (C::List(_), "clear") => M::ListClear,
        (C::List(_), "reserve") => M::ListReserve,
        (C::Text, "append") => M::TextAppend,
        (C::Text, "appendInt") => M::TextAppendInt,
        (C::Text, "appendBoolean") => M::TextAppendBoolean,
        (C::Text, "appendText") => M::TextAppendText,
        (C::Text, "byteLength") => M::TextByteLength,
        (C::Text, "byteAt") => M::TextByteAt,
        (C::Text, "isEmpty") => M::TextIsEmpty,
        (C::Text, "clear") => M::TextClear,
        (C::Text, "print") => M::TextPrint,
        (C::Text, "clone") => M::TextClone,
        (C::Text, "equals") => M::TextEquals,
        (C::Text, "equalsString") => M::TextEqualsString,
        (C::Text, "parseInt") => M::TextParseInt,
        (C::Text, "sliceBytes") => M::TextSliceBytes,
        (C::Console, "readLine") => M::ConsoleReadLine,
        (C::Console, "print") => M::ConsolePrint,
        (C::Console, "printText") => M::ConsolePrintText,
        (C::Files, "readText") => M::FilesReadText,
        (C::Files, "writeText") => M::FilesWriteText,
        (C::Files, "writeTextAtomic") => M::FilesWriteTextAtomic,
        (C::Args, "count") => M::ArgsCount,
        (C::Args, "get") => M::ArgsGet,
        _ => unreachable!("compiler-owned native method"),
    }
}
