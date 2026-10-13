use crate::ast::{NativeClass, Type};

#[derive(Clone, Copy)]
enum NativeBuiltin {
    List,
    Array,
    Deque,
    Map,
    Set,
    Comparator,
    BigInt,
    BigDecimal,
    OrderedMap,
    OrderedSet,
    PriorityQueue,
    BitSet,
    Predicate,
    ListIterator,
    MapIterator,
    FilterIterator,
    TakeIterator,
    SkipIterator,
    Iterators,
    Hasher,
    Text,
    Arrays,
    ByteBuffer,
    Encoding,
    Crypto,
    Regex,
    RegexMatch,
    Path,
    File,
    Directory,
    FileInfo,
    Watcher,
    WatchEvent,
    BinaryReader,
    BinaryWriter,
    Console,
    Files,
    Args,
    Math,
    Random,
    Time,
    Compression,
    Checksums,
    DateTime,
    SocketAddress,
    TcpListener,
    TcpStream,
    Datagram,
    UdpSocket,
    HttpRequest,
    HttpResponse,
    CryptoKey,
    CryptoDigest,
    CryptoMac,
    Certificate,
    Uri,
    Dns,
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
    Unicode,
    Character,
    Charset,
    Locale,
    Database,
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
                hasher: None,
            },
            Self::Set => NativeClass::Set {
                key: Type::Named("T".to_owned()),
                hasher: None,
            },
            Self::Comparator => NativeClass::Comparator(Type::Named("T".to_owned())),
            Self::OrderedMap => NativeClass::OrderedMap {
                key: Type::Named("K".to_owned()),
                value: Type::Named("V".to_owned()),
            },
            Self::OrderedSet => NativeClass::OrderedSet(Type::Named("T".to_owned())),
            Self::PriorityQueue => NativeClass::PriorityQueue(Type::Named("T".to_owned())),
            Self::BitSet => NativeClass::BitSet,
            Self::Predicate => NativeClass::Predicate(Type::Named("T".to_owned())),
            Self::ListIterator => NativeClass::ListIterator(Type::Named("T".to_owned())),
            Self::MapIterator => NativeClass::MapIterator(
                Type::Named("T".to_owned()),
                Type::Named("U".to_owned()),
            ),
            Self::FilterIterator => NativeClass::FilterIterator(Type::Named("T".to_owned())),
            Self::TakeIterator => NativeClass::TakeIterator(Type::Named("T".to_owned())),
            Self::SkipIterator => NativeClass::SkipIterator(Type::Named("T".to_owned())),
            Self::Iterators => NativeClass::Iterators(Type::Named("T".to_owned())),
            Self::Hasher => NativeClass::Hasher(Type::Named("T".to_owned())),
            Self::Text => NativeClass::Text,
            Self::BigInt => NativeClass::BigInt,
            Self::BigDecimal => NativeClass::BigDecimal,
            Self::Unicode => NativeClass::Unicode,
            Self::Character => NativeClass::Character,
            Self::Charset => NativeClass::Charset,
            Self::Locale => NativeClass::Locale,
            Self::Arrays => NativeClass::Arrays,
            Self::ByteBuffer => NativeClass::ByteBuffer,
            Self::Encoding => NativeClass::Encoding,
            Self::Crypto => NativeClass::Crypto,
            Self::Path => NativeClass::Path,
            Self::File => NativeClass::File,
            Self::Directory => NativeClass::Directory,
            Self::FileInfo => NativeClass::FileInfo,
            Self::Watcher => NativeClass::Watcher,
            Self::WatchEvent => NativeClass::WatchEvent,
            Self::BinaryReader => NativeClass::BinaryReader,
            Self::BinaryWriter => NativeClass::BinaryWriter,
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
            Self::DateTime => NativeClass::DateTime,
            Self::SocketAddress => NativeClass::SocketAddress,
            Self::TcpListener => NativeClass::TcpListener,
            Self::TcpStream => NativeClass::TcpStream,
            Self::Datagram => NativeClass::Datagram,
            Self::UdpSocket => NativeClass::UdpSocket,
            Self::HttpRequest => NativeClass::HttpRequest,
            Self::HttpResponse => NativeClass::HttpResponse,
            Self::CryptoKey => NativeClass::CryptoKey,
            Self::CryptoDigest => NativeClass::CryptoDigest,
            Self::CryptoMac => NativeClass::CryptoMac,
            Self::Certificate => NativeClass::Certificate,
            Self::Uri => NativeClass::Uri,
            Self::Dns => NativeClass::Dns,
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
            Self::Database => NativeClass::Database,
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
        source: include_str!("../../stdlib/prelude/numeric.ncr"),
        builtins: &[
            Builtin { name: "BigInt", native: Some(NativeBuiltin::BigInt) },
            Builtin { name: "BigDecimal", native: Some(NativeBuiltin::BigDecimal) },
            Builtin { name: "RoundingMode", native: None },
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
            Builtin { name: "OrderedMap", native: Some(NativeBuiltin::OrderedMap) },
            Builtin { name: "OrderedSet", native: Some(NativeBuiltin::OrderedSet) },
            Builtin { name: "PriorityQueue", native: Some(NativeBuiltin::PriorityQueue) },
            Builtin { name: "BitSet", native: Some(NativeBuiltin::BitSet) },
            Builtin { name: "Iterator", native: None },
            Builtin { name: "Mapper", native: None },
            Builtin { name: "Predicate", native: Some(NativeBuiltin::Predicate) },
            Builtin { name: "ListIterator", native: Some(NativeBuiltin::ListIterator) },
            Builtin { name: "MapIterator", native: Some(NativeBuiltin::MapIterator) },
            Builtin { name: "FilterIterator", native: Some(NativeBuiltin::FilterIterator) },
            Builtin { name: "TakeIterator", native: Some(NativeBuiltin::TakeIterator) },
            Builtin { name: "SkipIterator", native: Some(NativeBuiltin::SkipIterator) },
            Builtin { name: "Iterators", native: Some(NativeBuiltin::Iterators) },
            Builtin { name: "Hasher", native: Some(NativeBuiltin::Hasher) },
        ],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/text.ncr"),
        builtins: &[Builtin { name: "Text", native: Some(NativeBuiltin::Text) }],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/unicode.ncr"),
        builtins: &[
            Builtin { name: "Unicode", native: Some(NativeBuiltin::Unicode) },
            Builtin { name: "Character", native: Some(NativeBuiltin::Character) },
            Builtin { name: "Charset", native: Some(NativeBuiltin::Charset) },
        ],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/locale.ncr"),
        builtins: &[Builtin { name: "Locale", native: Some(NativeBuiltin::Locale) }],
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
        source: include_str!("../../stdlib/prelude/crypto_keys.ncr"),
        builtins: &[
            Builtin { name: "CryptoKey", native: Some(NativeBuiltin::CryptoKey) },
            Builtin { name: "CryptoDigest", native: Some(NativeBuiltin::CryptoDigest) },
            Builtin { name: "CryptoMac", native: Some(NativeBuiltin::CryptoMac) },
            Builtin { name: "Certificate", native: Some(NativeBuiltin::Certificate) },
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
        source: include_str!("../../stdlib/prelude/streams.ncr"),
        builtins: &[
            Builtin { name: "ByteSource", native: None },
            Builtin { name: "ByteSink", native: None },
            Builtin { name: "Streams", native: None },
            Builtin { name: "BinaryReader", native: Some(NativeBuiltin::BinaryReader) },
            Builtin { name: "BinaryWriter", native: Some(NativeBuiltin::BinaryWriter) },
        ],
    },

    PreludeUnit {
        source: include_str!("../../stdlib/prelude/filesystem.ncr"),
        builtins: &[
            Builtin { name: "Path", native: Some(NativeBuiltin::Path) },
            Builtin { name: "File", native: Some(NativeBuiltin::File) },
            Builtin { name: "Directory", native: Some(NativeBuiltin::Directory) },
            Builtin { name: "FileInfo", native: Some(NativeBuiltin::FileInfo) },
            Builtin { name: "Files", native: Some(NativeBuiltin::Files) },
            Builtin { name: "WatchKind", native: None },
            Builtin { name: "Watcher", native: Some(NativeBuiltin::Watcher) },
            Builtin { name: "WatchEvent", native: Some(NativeBuiltin::WatchEvent) },
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
        source: include_str!("../../stdlib/prelude/date_time.ncr"),
        builtins: &[
            Builtin { name: "DateFields", native: None },
            Builtin { name: "DateTime", native: Some(NativeBuiltin::DateTime) },
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
            Builtin { name: "HttpRequest", native: Some(NativeBuiltin::HttpRequest) },
            Builtin { name: "HttpResponse", native: Some(NativeBuiltin::HttpResponse) },
        ],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/uri_dns.ncr"),
        builtins: &[
            Builtin { name: "Uri", native: Some(NativeBuiltin::Uri) },
            Builtin { name: "Dns", native: Some(NativeBuiltin::Dns) },
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
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/database.ncr"),
        builtins: &[Builtin { name: "Database", native: Some(NativeBuiltin::Database) }],
    },
];

pub(super) fn builtin(name: &str) -> Option<Builtin> {
    PRELUDE_UNITS
        .iter()
        .flat_map(|unit| unit.builtins)
        .find(|builtin| builtin.name == name)
        .copied()
}
