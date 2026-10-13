use crate::ast::{NativeClass, Type};

#[derive(Clone, Copy)]
enum NativeBuiltin {
    List,
    Map,
    Text,
    Console,
    Files,
    Args,
}

impl NativeBuiltin {
    fn into_ast(self) -> NativeClass {
        match self {
            Self::List => NativeClass::List(Type::Named("T".to_owned())),
            Self::Map => NativeClass::Map {
                key: Type::Named("K".to_owned()),
                value: Type::Named("V".to_owned()),
                key_kind: None,
            },
            Self::Text => NativeClass::Text,
            Self::Console => NativeClass::Console,
            Self::Files => NativeClass::Files,
            Self::Args => NativeClass::Args,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct Builtin {
    pub(super) name: &'static str,
    native: Option<NativeBuiltin>,
}

impl Builtin {
    pub(super) fn native_class(self) -> Option<NativeClass> {
        self.native.map(NativeBuiltin::into_ast)
    }
}

pub(super) struct PreludeUnit {
    pub(super) source: &'static str,
    pub(super) builtins: &'static [Builtin],
}

pub(super) const PRELUDE_UNITS: &[PreludeUnit] = &[
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/types.ncr"),
        builtins: &[
            Builtin { name: "Option", native: None },
            Builtin { name: "Result", native: None },
        ],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/collections.ncr"),
        builtins: &[
            Builtin { name: "List", native: Some(NativeBuiltin::List) },
            Builtin { name: "Map", native: Some(NativeBuiltin::Map) },
        ],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/text.ncr"),
        builtins: &[Builtin { name: "Text", native: Some(NativeBuiltin::Text) }],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/io.ncr"),
        builtins: &[
            Builtin { name: "Console", native: Some(NativeBuiltin::Console) },
            Builtin { name: "Files", native: Some(NativeBuiltin::Files) },
            Builtin { name: "Args", native: Some(NativeBuiltin::Args) },
        ],
    },
];

pub(super) fn builtin(name: &str) -> Option<Builtin> {
    PRELUDE_UNITS
        .iter()
        .flat_map(|unit| unit.builtins)
        .find(|builtin| builtin.name == name)
        .copied()
}
