use crate::ast::{NativeClass, Type};
use crate::ir::NativeMethod;

#[derive(Clone, Copy)]
pub(crate) enum PoolSubmissionPolicy {
    Immediate,
    Delayed,
    Recurring { fixed_rate: bool },
}

pub(crate) struct PoolSubmission {
    name: &'static str,
    pub(crate) method: NativeMethod,
    pub(crate) policy: PoolSubmissionPolicy,
}

const POOL_SUBMISSIONS: &[PoolSubmission] = &[
    PoolSubmission { name: "submit", method: NativeMethod::ThreadPoolSubmit,
        policy: PoolSubmissionPolicy::Immediate },
    PoolSubmission { name: "schedule", method: NativeMethod::ThreadPoolSchedule,
        policy: PoolSubmissionPolicy::Delayed },
    PoolSubmission { name: "scheduleAtFixedRate", method: NativeMethod::ThreadPoolScheduleAtFixedRate,
        policy: PoolSubmissionPolicy::Recurring { fixed_rate: true } },
    PoolSubmission { name: "scheduleWithFixedDelay", method: NativeMethod::ThreadPoolScheduleWithFixedDelay,
        policy: PoolSubmissionPolicy::Recurring { fixed_rate: false } },
];

pub(crate) fn pool_submission_by_name(name: &str) -> Option<&'static PoolSubmission> {
    POOL_SUBMISSIONS.iter().find(|submission| submission.name == name)
}

pub(crate) fn pool_submission_policy(method: NativeMethod) -> Option<PoolSubmissionPolicy> {
    POOL_SUBMISSIONS.iter().find(|submission| submission.method == method)
        .map(|submission| submission.policy)
}

/// Native families whose state is stored in the shared `Mutex<Unit>` control.
/// Specialization names retain the declared family before their type arguments.
pub(crate) fn uses_unit_mutex(class_name: &str) -> bool {
    let family = class_name.split_once('<').map_or(class_name, |(family, _)| family);
    matches!(family, "AtomicInt" | "AtomicLong" | "AtomicBoolean" |
        "Semaphore" | "CountDownLatch" | "Barrier" | "CancellationToken" |
        "Future" | "ThreadPool")
}

#[derive(Clone, Copy)]
pub(super) struct Builtin {
    pub(super) name: &'static str,
    native: Option<fn() -> NativeClass>,
}

impl Builtin {
    pub(super) fn native_class(self) -> Option<NativeClass> {
        self.native.map(|factory| factory())
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
            Builtin { name: "BigInt", native: Some(|| NativeClass::BigInt) },
            Builtin { name: "BigDecimal", native: Some(|| NativeClass::BigDecimal) },
            Builtin { name: "RoundingMode", native: None },
        ],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/collections.ncr"),
        builtins: &[
            Builtin { name: "List", native: Some(|| NativeClass::List(Type::Named("T".to_owned()))) },
            Builtin { name: "Array", native: Some(|| NativeClass::Array(Type::Named("T".to_owned()))) },
            Builtin { name: "Arrays", native: Some(|| NativeClass::Arrays) },
            Builtin { name: "Deque", native: Some(|| NativeClass::Deque(Type::Named("T".to_owned()))) },
            Builtin { name: "Map", native: Some(|| NativeClass::Map {
                key: Type::Named("K".to_owned()),
                value: Type::Named("V".to_owned()),
                hasher: None,
            }) },
            Builtin { name: "Set", native: Some(|| NativeClass::Set {
                key: Type::Named("T".to_owned()),
                hasher: None,
            }) },
            Builtin { name: "Comparator", native: Some(|| NativeClass::Comparator(Type::Named("T".to_owned()))) },
            Builtin { name: "OrderedMap", native: Some(|| NativeClass::OrderedMap {
                key: Type::Named("K".to_owned()),
                value: Type::Named("V".to_owned()),
            }) },
            Builtin { name: "OrderedSet", native: Some(|| NativeClass::OrderedSet(Type::Named("T".to_owned()))) },
            Builtin { name: "PriorityQueue", native: Some(|| NativeClass::PriorityQueue(Type::Named("T".to_owned()))) },
            Builtin { name: "BitSet", native: Some(|| NativeClass::BitSet) },
            Builtin { name: "Iterator", native: Some(|| NativeClass::Iterator(Type::Named("T".to_owned()))) },
            Builtin { name: "Mapper", native: None },
            Builtin { name: "Predicate", native: Some(|| NativeClass::Predicate(Type::Named("T".to_owned()))) },
            Builtin { name: "ListIterator", native: Some(|| NativeClass::ListIterator(Type::Named("T".to_owned()))) },
            Builtin { name: "MapIterator", native: Some(|| NativeClass::MapIterator(
                Type::Named("T".to_owned()), Type::Named("U".to_owned()))) },
            Builtin { name: "FilterIterator", native: Some(|| NativeClass::FilterIterator(Type::Named("T".to_owned()))) },
            Builtin { name: "TakeIterator", native: Some(|| NativeClass::TakeIterator(Type::Named("T".to_owned()))) },
            Builtin { name: "SkipIterator", native: Some(|| NativeClass::SkipIterator(Type::Named("T".to_owned()))) },
            Builtin { name: "Iterators", native: Some(|| NativeClass::Iterators(Type::Named("T".to_owned()))) },
            Builtin { name: "Hasher", native: Some(|| NativeClass::Hasher(Type::Named("T".to_owned()))) },
        ],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/text.ncr"),
        builtins: &[Builtin { name: "Text", native: Some(|| NativeClass::Text) }],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/unicode.ncr"),
        builtins: &[
            Builtin { name: "Unicode", native: Some(|| NativeClass::Unicode) },
            Builtin { name: "Character", native: Some(|| NativeClass::Character) },
            Builtin { name: "Charset", native: Some(|| NativeClass::Charset) },
        ],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/locale.ncr"),
        builtins: &[Builtin { name: "Locale", native: Some(|| NativeClass::Locale) }],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/regex.ncr"),
        builtins: &[
            Builtin { name: "Regex", native: Some(|| NativeClass::Regex) },
            Builtin { name: "RegexMatch", native: Some(|| NativeClass::RegexMatch) },
        ],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/io.ncr"),
        builtins: &[
            Builtin { name: "ByteBuffer", native: Some(|| NativeClass::ByteBuffer) },
            Builtin { name: "Console", native: Some(|| NativeClass::Console) },
            Builtin { name: "Args", native: Some(|| NativeClass::Args) },
        ],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/crypto_keys.ncr"),
        builtins: &[
            Builtin { name: "CryptoKey", native: Some(|| NativeClass::CryptoKey) },
            Builtin { name: "CryptoDigest", native: Some(|| NativeClass::CryptoDigest) },
            Builtin { name: "CryptoMac", native: Some(|| NativeClass::CryptoMac) },
            Builtin { name: "Certificate", native: Some(|| NativeClass::Certificate) },
        ],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/encoding.ncr"),
        builtins: &[Builtin { name: "Encoding", native: Some(|| NativeClass::Encoding) }],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/crypto.ncr"),
        builtins: &[Builtin { name: "Crypto", native: Some(|| NativeClass::Crypto) }],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/streams.ncr"),
        builtins: &[
            Builtin { name: "ByteSource", native: None },
            Builtin { name: "ByteSink", native: None },
            Builtin { name: "Streams", native: None },
            Builtin { name: "BinaryReader", native: Some(|| NativeClass::BinaryReader) },
            Builtin { name: "BinaryWriter", native: Some(|| NativeClass::BinaryWriter) },
        ],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/serialization.ncr"),
        builtins: &[
            Builtin { name: "Codec", native: None },
            Builtin { name: "Serialization", native: None },
            Builtin { name: "ByteCodec", native: None },
            Builtin { name: "CharCodec", native: None },
            Builtin { name: "UnitCodec", native: None },
            Builtin { name: "IntCodec", native: None },
            Builtin { name: "LongCodec", native: None },
            Builtin { name: "DoubleCodec", native: None },
            Builtin { name: "BooleanCodec", native: None },
            Builtin { name: "TextCodec", native: None },
            Builtin { name: "ListCodec", native: None },
            Builtin { name: "OptionCodec", native: None },
            Builtin { name: "ResultCodec", native: None },
            Builtin { name: "UuidCodec", native: None },
            Builtin { name: "BytesCodec", native: None },
        ],
    },

    PreludeUnit {
        source: include_str!("../../stdlib/prelude/uuid.ncr"),
        builtins: &[Builtin { name: "Uuid", native: Some(|| NativeClass::Uuid) }],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/filesystem.ncr"),
        builtins: &[
            Builtin { name: "Path", native: Some(|| NativeClass::Path) },
            Builtin { name: "File", native: Some(|| NativeClass::File) },
            Builtin { name: "Directory", native: Some(|| NativeClass::Directory) },
            Builtin { name: "FileInfo", native: Some(|| NativeClass::FileInfo) },
            Builtin { name: "Files", native: Some(|| NativeClass::Files) },
            Builtin { name: "WatchKind", native: None },
            Builtin { name: "Watcher", native: Some(|| NativeClass::Watcher) },
            Builtin { name: "WatchEvent", native: Some(|| NativeClass::WatchEvent) },
        ],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/zip.ncr"),
        builtins: &[
            Builtin { name: "ZipReader", native: Some(|| NativeClass::ZipReader) },
            Builtin { name: "ZipWriter", native: Some(|| NativeClass::ZipWriter) },
            Builtin { name: "ZipEntry", native: Some(|| NativeClass::ZipEntry) },
        ],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/math.ncr"),
        builtins: &[Builtin { name: "Math", native: Some(|| NativeClass::Math) }],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/random.ncr"),
        builtins: &[Builtin { name: "Random", native: Some(|| NativeClass::Random) }],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/time.ncr"),
        builtins: &[Builtin { name: "Time", native: Some(|| NativeClass::Time) }],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/compression.ncr"),
        builtins: &[
            Builtin { name: "Compression", native: Some(|| NativeClass::Compression) },
            Builtin { name: "Checksums", native: Some(|| NativeClass::Checksums) },
        ],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/date_time.ncr"),
        builtins: &[
            Builtin { name: "DateFields", native: None },
            Builtin { name: "CalendarFields", native: None },
            Builtin { name: "DateTime", native: Some(|| NativeClass::DateTime) },
            Builtin { name: "Temporal", native: Some(|| NativeClass::Temporal) },
        ],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/xml.ncr"),
        builtins: &[
            Builtin { name: "XmlDocument", native: Some(|| NativeClass::XmlDocument) },
            Builtin { name: "XmlReader", native: Some(|| NativeClass::XmlReader) },
            Builtin { name: "XmlEvent", native: Some(|| NativeClass::XmlEvent) },
            Builtin { name: "XmlWriter", native: Some(|| NativeClass::XmlWriter) },
            Builtin { name: "XsltStylesheet", native: Some(|| NativeClass::XsltStylesheet) },
        ],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/network.ncr"),
        builtins: &[
            Builtin { name: "SocketAddress", native: Some(|| NativeClass::SocketAddress) },
            Builtin { name: "TcpListener", native: Some(|| NativeClass::TcpListener) },
            Builtin { name: "TcpStream", native: Some(|| NativeClass::TcpStream) },
            Builtin { name: "Datagram", native: Some(|| NativeClass::Datagram) },
            Builtin { name: "UdpSocket", native: Some(|| NativeClass::UdpSocket) },
            Builtin { name: "HttpRequest", native: Some(|| NativeClass::HttpRequest) },
            Builtin { name: "HttpResponse", native: Some(|| NativeClass::HttpResponse) },
        ],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/uri_dns.ncr"),
        builtins: &[
            Builtin { name: "Uri", native: Some(|| NativeClass::Uri) },
            Builtin { name: "Dns", native: Some(|| NativeClass::Dns) },
        ],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/concurrency.ncr"),
        builtins: &[
            Builtin { name: "Task", native: None },
            Builtin { name: "Thread", native: Some(|| NativeClass::Thread(Type::Named("T".to_owned()))) },
            Builtin { name: "Mutex", native: Some(|| NativeClass::Mutex(Type::Named("T".to_owned()))) },
            Builtin { name: "Guard", native: Some(|| NativeClass::Guard(Type::Named("T".to_owned()))) },
            Builtin { name: "Channel", native: Some(|| NativeClass::Channel(Type::Named("T".to_owned()))) },
            Builtin { name: "Sender", native: Some(|| NativeClass::Sender(Type::Named("T".to_owned()))) },
            Builtin { name: "Receiver", native: Some(|| NativeClass::Receiver(Type::Named("T".to_owned()))) },
            Builtin { name: "SendError", native: None },
            Builtin { name: "AtomicInt", native: Some(|| NativeClass::AtomicInt) },
            Builtin { name: "AtomicLong", native: Some(|| NativeClass::AtomicLong) },
            Builtin { name: "AtomicBoolean", native: Some(|| NativeClass::AtomicBoolean) },
            Builtin { name: "Semaphore", native: Some(|| NativeClass::Semaphore) },
            Builtin { name: "CountDownLatch", native: Some(|| NativeClass::CountDownLatch) },
            Builtin { name: "Barrier", native: Some(|| NativeClass::Barrier) },
            Builtin { name: "Condition", native: Some(|| NativeClass::Condition(Type::Named("T".to_owned()))) },
            Builtin { name: "CancellationToken", native: Some(|| NativeClass::CancellationToken) },
            Builtin { name: "CancelableTask", native: None },
            Builtin { name: "Future", native: Some(|| NativeClass::Future(Type::Named("T".to_owned()))) },
            Builtin { name: "ThreadPool", native: Some(|| NativeClass::ThreadPool(Type::Named("T".to_owned()))) },
            Builtin { name: "SubmitError", native: None },
        ],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/systems.ncr"),
        builtins: &[
            Builtin { name: "Env", native: Some(|| NativeClass::Env) },
            Builtin { name: "Command", native: Some(|| NativeClass::Command) },
            Builtin { name: "Process", native: Some(|| NativeClass::Process) },
            Builtin { name: "ProcessOutput", native: Some(|| NativeClass::ProcessOutput) },
            Builtin { name: "ExitStatus", native: None }
        ],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/database.ncr"),
        builtins: &[
            Builtin { name: "Database", native: Some(|| NativeClass::Database) },
            Builtin { name: "SqlStatement", native: Some(|| NativeClass::SqlStatement) },
        ],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/tls.ncr"),
        builtins: &[
            Builtin { name: "TlsConfig", native: Some(|| NativeClass::TlsConfig) },
            Builtin { name: "TlsStream", native: Some(|| NativeClass::TlsStream) },
        ],
    },
    PreludeUnit {
        source: include_str!("../../stdlib/prelude/websocket.ncr"),
        builtins: &[
            Builtin { name: "WebSocket", native: Some(|| NativeClass::WebSocket) },
            Builtin { name: "WebSocketMessage", native: Some(|| NativeClass::WebSocketMessage) },
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
