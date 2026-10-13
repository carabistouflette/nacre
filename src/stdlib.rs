//! Trusted prelude injection. User programs cannot declare native functions.
mod catalog;

use self::catalog::{builtin, PRELUDE_UNITS};
use crate::ast as a;
use crate::diagnostic::{Diagnostic, Span};
use crate::ir::{NativeClass as C, NativeMethod as M};

pub(crate) fn is_builtin_type_name(name: &str) -> bool {
    builtin(name).is_some()
}
pub fn inject(program: &mut a::Program) -> Result<(), Diagnostic> {
    for class in &program.classes {
        if is_builtin_type_name(&class.name) {
            return Err(Diagnostic::new(
                class.span,
                format!("[E5001] `{}` is reserved by the standard library", class.name),
            ));
        }
    }
    for unit in PRELUDE_UNITS {
        let mut tokens = crate::lexer::lex(unit.source)?;
        // Prelude locations must not index into unrelated user text.
        for token in &mut tokens { token.span = Span::default(); }
        let mut prelude = crate::parser::parse(tokens)?;
        for class in &mut prelude.classes {
            let builtin = unit.builtins.iter().find(|builtin| builtin.name == class.name)
                .expect("prelude declaration is missing its builtin catalog entry");
            class.is_builtin = true;
            class.qualified_name = class.name.clone();
            class.is_builtin_result = class.name == "Result";
            class.native = builtin.native_class();
            for method in &mut class.methods {
                method.qualified_name = format!("{}::{}", class.qualified_name, method.name);
            }
            if let Some(constructor) = &mut class.constructor {
                constructor.qualified_name = format!("{}::{}", class.qualified_name, constructor.name);
            }
        }
        program.classes.extend(prelude.classes);
    }
    Ok(())
}

pub fn method(class: C, name: &str, constructor: bool) -> M {
    if constructor {
        return match class { C::List(_) => M::ListNew, C::Map(_, _) => M::MapNew, C::Text => M::TextNew,
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
        (C::Map(_, _), "containsKey") => M::MapContainsKey,
        (C::Map(_, _), "size") => M::MapSize,
        (C::Map(_, _), "isEmpty") => M::MapIsEmpty,
        (C::Map(_, _), "put") => M::MapPut,
        (C::Map(_, _), "tryRemove") => M::MapTryRemove,
        (C::Map(_, _), "tryGet") => M::MapTryGet,
        (C::Map(_, _), "getRef") => M::MapGetRef,
        (C::Map(_, _), "getMut") => M::MapGetMut,
        (C::Map(_, _), "clear") => M::MapClear,
        (C::Text, "append") => M::TextAppend,
        (C::Text, "appendInt") => M::TextAppendInt,
        (C::Text, "appendByte") => M::TextAppendByte,
        (C::Text, "appendChar") => M::TextAppendChar,
        (C::Text, "appendLong") => M::TextAppendLong,
        (C::Text, "appendDouble") => M::TextAppendDouble,
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
        (C::Text, "parseLong") => M::TextParseLong,
        (C::Text, "parseDouble") => M::TextParseDouble,
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
