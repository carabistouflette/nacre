use crate::ast::{NativeClass, Type};

#[derive(Clone, Copy)]
enum NativeBuiltin {
    List,
    Array,
    Deque,
    Map,
    Set,
    Comparator,
    Text,
    Arrays,
    ByteBuffer,
    Encoding,
    Crypto,
    Regex,
    Path,
    File,
    Directory,
    FileInfo,
    Console,
    Files,
    Args,
    Math,
    Random,
    Time,
    Compression,
    Checksums,
    SocketAddress,
    TcpListener,
    TcpStream,
    Datagram,
    UdpSocket,
    Thread,
    Mutex,
    Guard,
    Channel,
    Sender,
    Receiver,
    Env,
    Command,
    Process,
    ProcessOutput,
}

impl NativeBuiltin {
    fn into_ast(self) -> NativeClass {
        match self {
            Self::List => NativeClass::List(Type::Named("T".to_owned())),
            Self::Array => NativeClass::Array(Type::Named("T".to_owned())),
            Self::Deque => NativeClass::Deque(Type::Named("T".to_owned())),
            Self::Map => NativeClass::Map {
                key: Type::Named("K".to_owned()),
                value: Type::Named("V".to_owned()),
                key_kind: None,
            },
            Self::Set => NativeClass::Set {
                key: Type::Named("T".to_owned()),
                key_kind: None,
            },
            Self::Comparator => NativeClass::Comparator(Type::Named("T".to_owned())),
            Self::Text => NativeClass::Text,
            Self::Arrays => NativeClass::Arrays,
            Self::ByteBuffer => NativeClass::ByteBuffer,
            Self::Encoding => NativeClass::Encoding,
            Self::Crypto => NativeClass::Crypto,
            Self::Path => NativeClass::Path,
            Self::File => NativeClass::File,
            Self::Directory => NativeClass::Directory,
            Self::FileInfo => NativeClass::FileInfo,
            Self::Regex => NativeClass::Regex,
            Self::RegexMatch => NativeClass::RegexMatch,
            Self::Console => NativeClass::Console,
            Self::Files => NativeClass::Files,
            Self::Args => NativeClass::Args,
            Self::Math => NativeClass::Math,
            Self::Random => NativeClass::Random,
            Self::Time => NativeClass::Time,
            Self::Compression => NativeClass::Compression,
            Self::Checksums => NativeClass::Checksums,
            Self::SocketAddress => NativeClass::SocketAddress,
            Self::TcpListener => NativeClass::TcpListener,
            Self::TcpStream => NativeClass::TcpStream,
            Self::Datagram => NativeClass::Datagram,
            Self::UdpSocket => NativeClass::UdpSocket,
            Self::Thread => NativeClass::Thread(Type::Named("T".to_owned())),
            Self::Mutex => NativeClass::Mutex(Type::Named("T".to_owned())),
            Self::Guard => NativeClass::Guard(Type::Named("T".to_owned())),
            Self::Channel => NativeClass::Channel(Type::Named("T".to_owned())),
            Self::Sender => NativeClass::Sender(Type::Named("T".to_owned())),
            Self::Receiver => NativeClass::Receiver(Type::Named("T".to_owned())),
            Self::Env => NativeClass::Env,
            Self::Command => NativeClass::Command,
            Self::Process => NativeClass::Process,
            Self::ProcessOutput => NativeClass::ProcessOutput,
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
            Builtin { name: "Array", native: Some(NativeBuiltin::Array) },
            Builtin { name: "Arrays", native: Some(NativeBuiltin::Arrays) },
            Builtin { name: "Deque", native: Some(NativeBuiltin::Deque) },
            Builtin { name: "Map", native: Some(NativeBuiltin::Map) },
            Builtin { name: "Set", native: Some(NativeBuiltin::Set) },
            Builtin { name: "Comparator", native: Some(NativeBuiltin::Comparator) },
        ],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/text.ncr"),
        builtins: &[Builtin { name: "Text", native: Some(NativeBuiltin::Text) }],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/regex.ncr"),
        builtins: &[
            Builtin { name: "Regex", native: Some(NativeBuiltin::Regex) },
            Builtin { name: "RegexMatch", native: Some(NativeBuiltin::RegexMatch) },
        ],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/io.ncr"),
        builtins: &[
            Builtin { name: "ByteBuffer", native: Some(NativeBuiltin::ByteBuffer) },
            Builtin { name: "Console", native: Some(NativeBuiltin::Console) },
            Builtin { name: "Args", native: Some(NativeBuiltin::Args) },
        ],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/encoding.ncr"),
        builtins: &[Builtin { name: "Encoding", native: Some(NativeBuiltin::Encoding) }],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/crypto.ncr"),
        builtins: &[Builtin { name: "Crypto", native: Some(NativeBuiltin::Crypto) }],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/filesystem.ncr"),
        builtins: &[
            Builtin { name: "Path", native: Some(NativeBuiltin::Path) },
            Builtin { name: "File", native: Some(NativeBuiltin::File) },
            Builtin { name: "Directory", native: Some(NativeBuiltin::Directory) },
            Builtin { name: "FileInfo", native: Some(NativeBuiltin::FileInfo) },
            Builtin { name: "Files", native: Some(NativeBuiltin::Files) },
        ],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/math.ncr"),
        builtins: &[Builtin { name: "Math", native: Some(NativeBuiltin::Math) }],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/random.ncr"),
        builtins: &[Builtin { name: "Random", native: Some(NativeBuiltin::Random) }],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/time.ncr"),
        builtins: &[Builtin { name: "Time", native: Some(NativeBuiltin::Time) }],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/compression.ncr"),
        builtins: &[
            Builtin { name: "Compression", native: Some(NativeBuiltin::Compression) },
            Builtin { name: "Checksums", native: Some(NativeBuiltin::Checksums) },
        ],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/network.ncr"),
        builtins: &[
            Builtin { name: "SocketAddress", native: Some(NativeBuiltin::SocketAddress) },
            Builtin { name: "TcpListener", native: Some(NativeBuiltin::TcpListener) },
            Builtin { name: "TcpStream", native: Some(NativeBuiltin::TcpStream) },
            Builtin { name: "Datagram", native: Some(NativeBuiltin::Datagram) },
            Builtin { name: "UdpSocket", native: Some(NativeBuiltin::UdpSocket) },
        ],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/concurrency.ncr"),
        builtins: &[
            Builtin { name: "Task", native: None },
            Builtin { name: "Thread", native: Some(NativeBuiltin::Thread) },
            Builtin { name: "Mutex", native: Some(NativeBuiltin::Mutex) },
            Builtin { name: "Guard", native: Some(NativeBuiltin::Guard) },
            Builtin { name: "Channel", native: Some(NativeBuiltin::Channel) },
            Builtin { name: "Sender", native: Some(NativeBuiltin::Sender) },
            Builtin { name: "Receiver", native: Some(NativeBuiltin::Receiver) },
            Builtin { name: "SendError", native: None },
        ],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/systems.ncr"),
        builtins: &[
            Builtin { name: "Env", native: Some(NativeBuiltin::Env) },
            Builtin { name: "Command", native: Some(NativeBuiltin::Command) },
            Builtin { name: "Process", native: Some(NativeBuiltin::Process) },
            Builtin { name: "ProcessOutput", native: Some(NativeBuiltin::ProcessOutput) },
            Builtin { name: "ExitStatus", native: None }
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
