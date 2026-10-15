use crate::diagnostic::Span;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type { Byte, Char, Int, Long, Double, Unit, Boolean, String, Void, Named(String), Applied { name: String, args: Vec<Type> }, Borrow { name: String, args: Vec<Type>, mutable: bool } }
impl Type {
    pub fn is_primitive(&self) -> bool {
        matches!(self, Self::Byte | Self::Char | Self::Int | Self::Long | Self::Double |
            Self::Unit | Self::Boolean | Self::String)
    }
    pub fn is_copy(&self) -> bool { self.is_primitive() }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility { Public, Private }
#[derive(Debug, Clone)]
pub struct Program {
    pub units: Vec<SourceUnit>,
    pub functions: Vec<Function>,
    pub classes: Vec<Class>,
    pub traits: Vec<Trait>,
    pub impls: Vec<Impl>,
}
#[derive(Debug, Clone)]
pub struct Trait {
    pub name: String,
    pub span: Span,
    pub methods: Vec<Function>,
    pub visibility: Visibility,
}
#[derive(Debug, Clone)]
pub struct Impl {
    pub trait_name: String,
    pub target_class: String,
    pub span: Span,
    pub methods: Vec<Function>,
}
#[derive(Debug, Clone)]
pub struct SourceUnit { pub module: String, pub imports: Vec<Import> }
#[derive(Debug, Clone)]
pub struct Import { pub path: String, pub wildcard: bool, pub span: Span }
#[derive(Debug, Clone)]
pub enum NativeClass {
    List(Type), Array(Type), Deque(Type),
    Map { key: Type, value: Type, hasher: Option<Type> },
    Set { key: Type, hasher: Option<Type> }, Comparator(Type), Hasher(Type),
    OrderedMap { key: Type, value: Type }, OrderedSet(Type), PriorityQueue(Type), BitSet,
    Predicate(Type),
    Iterator(Type),
    ListIterator(Type), MapIterator(Type, Type), FilterIterator(Type),
    TakeIterator(Type), SkipIterator(Type), Iterators(Type),
    Text, BigInt, BigDecimal, Regex, RegexMatch, Arrays, ByteBuffer, Encoding, Crypto, Uuid, Path, File, Directory, FileInfo,
    Console, Files, Args, Math, Random, Time, DateTime, Temporal, Compression, Checksums,
    Unicode, Character, Charset, Locale,
    SocketAddress, TcpListener, TcpStream, Datagram, UdpSocket, HttpRequest, HttpResponse,
    CryptoKey, CryptoDigest, CryptoMac, Certificate,
    Watcher, WatchEvent, BinaryReader, BinaryWriter, Uri, Dns,
    TlsConfig, TlsStream, WebSocket, WebSocketMessage,
    ZipReader, ZipWriter, ZipEntry,
    XmlDocument, XmlReader, XmlEvent, XmlWriter, XsltStylesheet,
    Thread(Type), Mutex(Type), Guard(Type), Channel(Type), Sender(Type), Receiver(Type),
    Condition(Type), Future(Type), ThreadPool(Type),
    AtomicInt, AtomicLong, AtomicBoolean, Semaphore, CountDownLatch, Barrier, CancellationToken,
    Env, Command, Process, ProcessOutput, Database, SqlStatement,
}
#[derive(Debug, Clone)]
pub struct Variant { pub name: String, pub fields: Vec<Type>, pub span: Span }
#[derive(Debug, Clone)]
pub struct Class {
    /// Set only by the trusted prelude injector, never by source syntax.
    pub native: Option<NativeClass>,
    /// Prelude-owned declarations, including non-native Option/Result enums.
    pub is_builtin: bool,
    /// Marks the trusted prelude Result template and its specializations.
    pub is_builtin_result: bool,
    pub name: String, pub span: Span, pub fields: Vec<Field>,
    pub type_params: Vec<String>, pub type_bounds: Vec<Option<String>>, pub implements: Vec<Type>, pub is_interface: bool,
    pub variants: Option<Vec<Variant>>,
    pub methods: Vec<Function>, pub constructor: Option<Function>,
    pub qualified_name: String, pub unit: usize, pub visibility: Visibility,
}
#[derive(Debug, Clone)]
pub struct Field {
    pub name: String, pub ty: Type, pub visibility: Visibility,
    pub initializer: Option<Expr>, pub span: Span,
}
#[derive(Debug, Clone)]
pub struct Function {
    pub name: String, pub span: Span, pub return_type: Type,
    pub params: Vec<Param>, pub body: Block,
    pub visibility: Visibility, pub is_static: bool, pub readonly: bool,
    pub is_async: bool,
    pub qualified_name: String, pub unit: usize,
}
#[derive(Debug, Clone)]
pub struct Param { pub name: String, pub ty: Type, pub span: Span }
pub type Block = Vec<Stmt>;
#[derive(Debug, Clone)]
pub struct Stmt { pub kind: StmtKind, pub span: Span }
#[derive(Debug, Clone)]
pub enum StmtKind {
    Let { ty: Type, name: String, value: Option<Expr> },
    Assign { target: Expr, value: Expr },
    Expr(Expr),
    If { condition: Expr, then_block: Block, else_block: Option<Block> },
    While { condition: Expr, body: Block },
    ForEach { ty: Type, name: String, key: Option<(Type, String)>, collection: Expr, body: Block, header_span: Span },
    Match { value: Expr, arms: Vec<MatchArm> },
    Return(Option<Expr>), Break, Continue, Block(Block),
}
#[derive(Debug, Clone)]
pub struct MatchArm { pub variant: String, pub bindings: Vec<String>, pub body: Block, pub span: Span }
#[derive(Debug, Clone)]
pub struct Expr { pub kind: ExprKind, pub span: Span, pub depth: usize }
#[derive(Debug, Clone)]
pub enum ExprKind {
    Char(char), Int(i32), Long(i64), Double(f64), Unit, Boolean(bool), String(String), Var(String), This,
    Field { base: Box<Expr>, name: String },
    Call { name: String, args: Vec<Expr> },
    MethodCall { receiver: Box<Expr>, name: String, args: Vec<Expr> },
    StaticCall { class: String, type_args: Vec<Type>, name: String, args: Vec<Expr> },
    New { class: String, type_args: Vec<Type>, args: Vec<Expr> },
    Variant { class: String, type_args: Vec<Type>, variant: String, args: Vec<Expr> },
    Borrow { expr: Box<Expr>, mutable: bool },
    Cast { ty: Type, expr: Box<Expr> },
    Unary { op: UnaryOp, expr: Box<Expr> },
    Try(Box<Expr>),
    Binary { op: BinaryOp, left: Box<Expr>, right: Box<Expr> },
    Await(Box<Expr>),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp { Neg, Not, Plus, BitNot }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add, Sub, Mul, Div, Rem, Eq, Ne, Lt, Le, Gt, Ge, And, Or,
    BitAnd, BitOr, BitXor, ShiftLeft, ShiftRight,
}
