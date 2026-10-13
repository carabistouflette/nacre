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
        return match class {
            C::List(_) => M::ListNew,
            C::Array(_) => M::ArrayNew,
            C::Deque(_) => M::DequeNew,
            C::Map(_, _) => M::MapNew,
            C::Set(_) => M::SetNew,
            C::Text => M::TextNew,
            C::Random => M::RandomNew,
            C::Regex | C::RegexMatch | C::Arrays | C::ByteBuffer |
            C::Console | C::Files | C::Args | C::Math | C::Time =>
                unreachable!("static or factory-only native class has no constructor"),
        };
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
        (C::Array(_), "size") => M::ArraySize,
        (C::Array(_), "get") => M::ArrayGet,
        (C::Array(_), "getRef") => M::ArrayGetRef,
        (C::Array(_), "getMut") => M::ArrayGetMut,
        (C::Array(_), "tryGet") => M::ArrayTryGet,
        (C::Array(_), "set") => M::ArraySet,
        (C::Arrays, "zeroedBytes") => M::ArraysBytes,
        (C::Arrays, "zeroedChars") => M::ArraysChars,
        (C::Arrays, "zeroedInts") => M::ArraysInts,
        (C::Arrays, "zeroedLongs") => M::ArraysLongs,
        (C::Arrays, "zeroedDoubles") => M::ArraysDoubles,
        (C::Arrays, "zeroedBooleans") => M::ArraysBooleans,
        (C::Arrays, "zeroedUnits") => M::ArraysUnits,
        (C::ByteBuffer, "withCapacity") => M::ByteBufferWithCapacity,
        (C::ByteBuffer, "length") => M::ByteBufferLength,
        (C::ByteBuffer, "capacity") => M::ByteBufferCapacity,
        (C::ByteBuffer, "get") => M::ByteBufferGet,
        (C::ByteBuffer, "set") => M::ByteBufferSet,
        (C::ByteBuffer, "append") => M::ByteBufferAppend,
        (C::ByteBuffer, "appendBytes") => M::ByteBufferAppendBytes,
        (C::ByteBuffer, "clear") => M::ByteBufferClear,
        (C::ByteBuffer, "truncate") => M::ByteBufferTruncate,
        (C::List(_), "sort") => M::ListSort,
        (C::List(_), "sortBy") => M::ListSortBy,
        (C::List(_), "binarySearch") => M::ListBinarySearch,
        (C::List(_), "binarySearchBy") => M::ListBinarySearchBy,
        (C::Deque(_), "pushFront") => M::DequePushFront,
        (C::Deque(_), "pushBack") => M::DequePushBack,
        (C::Deque(_), "popFront") => M::DequePopFront,
        (C::Deque(_), "popBack") => M::DequePopBack,
        (C::Deque(_), "tryPopFront") => M::DequeTryPopFront,
        (C::Deque(_), "tryPopBack") => M::DequeTryPopBack,
        (C::Deque(_), "size") => M::DequeSize,
        (C::Deque(_), "capacity") => M::DequeCapacity,
        (C::Deque(_), "isEmpty") => M::DequeIsEmpty,
        (C::Deque(_), "get") => M::DequeGet,
        (C::Deque(_), "getRef") => M::DequeGetRef,
        (C::Deque(_), "getMut") => M::DequeGetMut,
        (C::Deque(_), "set") => M::DequeSet,
        (C::Deque(_), "tryGet") => M::DequeTryGet,
        (C::Deque(_), "clear") => M::DequeClear,
        (C::Deque(_), "reserve") => M::DequeReserve,
        (C::Set(_), "add") => M::SetAdd,
        (C::Set(_), "contains") => M::SetContains,
        (C::Set(_), "size") => M::SetSize,
        (C::Set(_), "isEmpty") => M::SetIsEmpty,
        (C::Set(_), "remove") => M::SetRemove,
        (C::Set(_), "clear") => M::SetClear,
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
        (C::Text, "charCount") => M::TextCharCount,
        (C::Text, "charAt") => M::TextCharAt,
        (C::Text, "compareTo") => M::TextCompareTo,
        (C::Text, "indexOf") => M::TextIndexOf,
        (C::Text, "startsWith") => M::TextStartsWith,
        (C::Text, "endsWith") => M::TextEndsWith,
        (C::Text, "slice") => M::TextSlice,
        (C::Text, "trim") => M::TextTrim,
        (C::Text, "replace") => M::TextReplace,
        (C::Text, "split") => M::TextSplit,
        (C::Regex, "compile") => M::RegexCompile,
        (C::Regex, "matches") => M::RegexMatches,
        (C::Regex, "search") => M::RegexSearch,
        (C::RegexMatch, "groupCount") => M::RegexMatchGroupCount,
        (C::RegexMatch, "hasGroup") => M::RegexMatchHasGroup,
        (C::RegexMatch, "group") => M::RegexMatchGroup,
        (C::RegexMatch, "start") => M::RegexMatchStart,
        (C::RegexMatch, "end") => M::RegexMatchEnd,
        (C::Text, "sliceBytes") => M::TextSliceBytes,
        (C::Math, "absInt") => M::MathAbsInt,
        (C::Math, "absLong") => M::MathAbsLong,
        (C::Math, "absDouble") => M::MathAbsDouble,
        (C::Math, "minInt") => M::MathMinInt,
        (C::Math, "minLong") => M::MathMinLong,
        (C::Math, "minDouble") => M::MathMinDouble,
        (C::Math, "maxInt") => M::MathMaxInt,
        (C::Math, "maxLong") => M::MathMaxLong,
        (C::Math, "maxDouble") => M::MathMaxDouble,
        (C::Math, "sqrt") => M::MathSqrt,
        (C::Math, "pow") => M::MathPow,
        (C::Math, "log") => M::MathLog,
        (C::Math, "exp") => M::MathExp,
        (C::Math, "sin") => M::MathSin,
        (C::Math, "cos") => M::MathCos,
        (C::Math, "tan") => M::MathTan,
        (C::Math, "floor") => M::MathFloor,
        (C::Math, "ceil") => M::MathCeil,
        (C::Math, "round") => M::MathRound,
        (C::Math, "hypot") => M::MathHypot,
        (C::Math, "isNaN") => M::MathIsNaN,
        (C::Math, "isFinite") => M::MathIsFinite,
        (C::Random, "fromSystem") => M::RandomFromSystem,
        (C::Random, "reseed") => M::RandomReseed,
        (C::Random, "nextLong") => M::RandomNextLong,
        (C::Random, "nextInt") => M::RandomNextInt,
        (C::Random, "nextDouble") => M::RandomNextDouble,
        (C::Time, "monotonicNanos") => M::TimeMonotonicNanos,
        (C::Time, "unixMillis") => M::TimeUnixMillis,
        (C::Time, "sleepMillis") => M::TimeSleepMillis,
        (C::Time, "deadlineAfterMillis") => M::TimeDeadlineAfterMillis,
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
