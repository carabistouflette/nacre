//! Resolved, typed, structured IR. Moves and explicit drops are visible.
//! ownership.rs lowers this into an analysis CFG; codegen inserts lexical drops.
use crate::ast::{BinaryOp, MapKeyKind, UnaryOp};
use crate::diagnostic::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type { Byte, Char, Int, Long, Double, Unit, Boolean, String, Void, Object(usize), Borrow { class: usize, mutable: bool } }
impl Type {
    /// True only for an owning object/enum value, never for a borrowed pointer.
    pub fn is_object(self) -> bool { matches!(self, Self::Object(_)) }
    pub fn is_borrow(self) -> bool { matches!(self, Self::Borrow { .. }) }
    pub fn object_class(self) -> Option<usize> {
        match self { Self::Object(id) | Self::Borrow { class: id, .. } => Some(id), _ => None }
    }
    pub fn is_primitive(self) -> bool {
        matches!(self, Self::Byte | Self::Char | Self::Int | Self::Long | Self::Double |
            Self::Unit | Self::Boolean | Self::String)
    }
    pub fn is_copy(self) -> bool { self.is_primitive() }
    pub fn is_numeric(self) -> bool {
        matches!(self, Self::Byte | Self::Char | Self::Int | Self::Long | Self::Double)
    }
    pub fn is_integer(self) -> bool { matches!(self, Self::Byte | Self::Char | Self::Int | Self::Long) }
    pub fn numeric_rank(self) -> Option<u8> {
        match self {
            Self::Byte | Self::Char => Some(0),
            Self::Int => Some(1),
            Self::Long => Some(2),
            Self::Double => Some(3),
            _ => None,
        }
    }
    pub fn promoted_numeric(self) -> Option<Self> {
        match self {
            Self::Byte | Self::Char => Some(Self::Int),
            ty if ty.is_numeric() => Some(ty),
            _ => None,
        }
    }
}
#[derive(Debug)]
pub struct Program { pub classes: Vec<Class>, pub functions: Vec<Function>, pub main: usize }
#[derive(Debug, Clone)]
pub struct Variant { pub name: String, pub fields: Vec<Type> }
#[derive(Debug, Clone, Copy)]
pub struct ResultLayout {
    pub ok_variant: usize,
    pub err_variant: usize,
    pub ok_type: Type,
    pub err_type: Type,
}
#[derive(Debug)]
pub struct Class {
    pub native: Option<NativeClass>,
    pub id: usize, pub name: String, pub fields: Vec<Type>,
    pub variants: Option<Vec<Variant>>,
    pub result: Option<ResultLayout>,
    pub has_borrow: bool,
    pub is_interface: bool,
    /// Interface method IDs in source order (the vtable slot order).
    pub interface_methods: Vec<usize>,
    /// (interface ID, concrete implementation method IDs in slot order).
    pub implementations: Vec<(usize, Vec<usize>)>,
    /// For each borrowed field, the constructor parameter supplying its referent.
    pub borrowed_from: Vec<Option<usize>>,
}
impl Class { pub fn is_view(&self) -> bool { self.has_borrow } }
#[derive(Debug)]
pub struct Function {
    pub native: Option<NativeMethod>,
    pub virtual_slot: Option<usize>,
    pub id: usize, pub name: String, pub receiver: Option<usize>, pub readonly: bool, pub return_type: Type,
    pub locals: Vec<Local>,
    pub params: Vec<(usize, Type)>, pub body: Block,
}
#[derive(Debug)]
pub struct Local { pub name: String, pub ty: Type, pub span: Span }
pub type Block = Vec<Stmt>;
#[derive(Debug)]
pub enum Stmt {
    Let { id: usize, ty: Type, value: Option<Expr> },
    Assign { target: Place, value: Expr },
    Expr(Expr),
    If { condition: Expr, then_block: Block, else_block: Option<Block> },
    While { condition: Expr, body: Block },
    Match { value: Expr, arms: Vec<MatchArm> },
    Return(Option<Expr>), Break, Continue, Block(Block),
}
#[derive(Debug)]
pub struct MatchArm { pub variant: usize, pub bindings: Vec<usize>, pub body: Block }
#[derive(Debug, Clone)]
pub struct Place { pub kind: PlaceKind, pub ty: Type }
#[derive(Debug, Clone)]
pub enum PlaceKind { Var(usize), This, Field { base: Box<Place>, field: usize } }
impl Place {
    pub fn root_var(&self) -> Option<usize> {
        match &self.kind { PlaceKind::Var(id) => Some(*id), PlaceKind::This => None,
            PlaceKind::Field { base, .. } => base.root_var() }
    }
}
#[derive(Debug)]
pub struct Expr { pub kind: ExprKind, pub ty: Type, pub span: Span }
#[derive(Debug)]
pub enum ExprKind {
    Char(char), Int(i32), Long(i64), Double(f64), Unit, Boolean(bool), String(String), Read(Place), Move(usize),
    Borrow { place: Place, mutable: bool },
    /// Compiler-only capacity read for a structurally frozen map.
    MapCapacity(Place),
    /// Compiler-only direct Map slot access; `pair` links key/value slots to one loop loan.
    MapSlot { map: Place, index: Box<Expr>, kind: MapSlotKind, pair: Option<usize> },
    Drop(Box<Expr>),
    /// Allocation-free interface upcast, preserving owned or borrowed status.
    Upcast { value: Box<Expr>, interface: usize, concrete: usize },
    New { class: usize, constructor: usize, args: Vec<Expr> },
    Variant { class: usize, variant: usize, args: Vec<Expr> },
    Call { target: CallTarget, receiver: Option<Place>, args: Vec<Expr> },
    Unary { op: UnaryOp, expr: Box<Expr> },
    Try { value: Box<Expr>, return_result: usize },
    Binary { op: BinaryOp, left: Box<Expr>, right: Box<Expr> },
    Cast { value: Box<Expr> },
}
#[derive(Debug, Clone, Copy)]
pub enum MapSlotKind { Occupied, Key, Value }
#[derive(Debug, Clone, Copy)]
pub enum CallTarget { Function(usize), Println }

/// Trusted standard-library storage, never exposed as source fields.
#[derive(Debug, Clone, Copy)]
pub enum NativeClass {
    List(Type), Array(Type), Deque(Type), Map(MapKeyKind, Type), Set(MapKeyKind),
    Text, Regex, RegexMatch, Arrays, ByteBuffer, Encoding, Crypto, Path, File, Directory, FileInfo,
    Console, Files, Args, Math, Random, Time, SocketAddress, TcpListener, TcpStream,
    Datagram, UdpSocket, Thread(Type), Mutex(Type), Guard(Type), Channel(Type),
    Sender(Type), Receiver(Type), Env, Command, Process, ProcessOutput
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeMethod {
    ListNew, ListAdd, ListSize, ListCapacity, ListIsEmpty, ListGet, ListGetRef, ListGetMut,
    ListSet, ListPop, ListRemove, ListTryGet, ListTryPop, ListTryRemove, ListClear, ListReserve,
    ListSort, ListSortBy, ListBinarySearch, ListBinarySearchBy,
    DequeNew, DequePushFront, DequePushBack, DequePopFront, DequePopBack,
    DequeTryPopFront, DequeTryPopBack, DequeSize, DequeCapacity, DequeIsEmpty,
    DequeGet, DequeGetRef, DequeGetMut, DequeSet, DequeTryGet, DequeClear, DequeReserve,
    ArrayNew, ArraySize, ArrayGet, ArrayGetRef, ArrayGetMut, ArraySet, ArrayTryGet,
    ArraysBytes, ArraysChars, ArraysInts, ArraysLongs, ArraysDoubles, ArraysBooleans, ArraysUnits,
    ByteBufferWithCapacity, ByteBufferLength, ByteBufferCapacity, ByteBufferGet, ByteBufferSet,
    ByteBufferAppend, ByteBufferAppendBytes, ByteBufferClear, ByteBufferTruncate,
    EncodingHexEncode, EncodingHexDecode, EncodingBase64Encode, EncodingBase64Decode,
    EncodingBase64UrlEncode, EncodingBase64UrlDecode, EncodingUtf8Encode, EncodingUtf8Decode,
    PathFromText, PathClone, PathJoin, PathIsAbsolute, PathAsText,
    FileOpenRead, FileCreate, FileOpenAppend, FileReadSome, FileWriteSome, FileWriteAll, FileFlush, FileClose,
    DirectoryOpen, DirectoryNextName, DirectoryClose,
    FileInfoSize, FileInfoIsFile, FileInfoIsDirectory, FileInfoIsSymlink,
    FilesMetadata, FilesCreateDirectory, FilesRemoveFile, FilesRemoveDirectory, FilesRename,
    MapNew, MapContainsKey, MapSize, MapIsEmpty, MapPut, MapTryRemove, MapTryGet,
    MapGetRef, MapGetMut, MapClear,
    SetNew, SetAdd, SetContains, SetSize, SetIsEmpty, SetRemove, SetClear,
    TextNew, TextAppend, TextAppendInt, TextAppendByte, TextAppendChar, TextAppendLong, TextAppendDouble,
    TextAppendBoolean, TextAppendText,
    TextByteLength, TextByteAt, TextIsEmpty, TextClear, TextPrint,
    TextClone, TextEquals, TextEqualsString, TextParseInt, TextParseLong, TextParseDouble, TextSliceBytes,
    TextCharCount, TextCharAt, TextCompareTo, TextIndexOf, TextStartsWith, TextEndsWith,
    TextSlice, TextTrim, TextReplace, TextSplit,
    RegexCompile, RegexMatches, RegexSearch,
    RegexMatchGroupCount, RegexMatchHasGroup, RegexMatchGroup, RegexMatchStart, RegexMatchEnd,
    ConsoleReadLine, ConsolePrint, ConsolePrintText,
    FilesReadText, FilesWriteText, FilesWriteTextAtomic, ArgsCount, ArgsGet,
    MathAbsInt, MathAbsLong, MathAbsDouble, MathMinInt, MathMinLong, MathMinDouble, MathMaxInt, MathMaxLong, MathMaxDouble,
    MathSqrt, MathPow, MathLog, MathExp, MathSin, MathCos, MathTan, MathFloor, MathCeil, MathRound, MathHypot, MathIsNaN, MathIsFinite,
    MathCbrt, MathLog10, MathLog1p, MathExpm1, MathAsin, MathAcos, MathAtan, MathAtan2,
    MathSinh, MathCosh, MathTanh, MathToRadians, MathToDegrees, MathSignum, MathCopySign,
    MathNextUp, MathNextDown, MathUlp, MathGetExponent, MathScalb, MathIeeeRemainder,
    MathAddExactInt, MathAddExactLong, MathSubtractExactInt, MathSubtractExactLong,
    MathMultiplyExactInt, MathMultiplyExactLong, MathNegateExactInt, MathNegateExactLong,
    MathIncrementExactInt, MathIncrementExactLong, MathDecrementExactInt, MathDecrementExactLong,
    MathFloorDivInt, MathFloorDivLong, MathFloorModInt, MathFloorModLong,
    RandomNew, RandomFromSystem, RandomReseed, RandomNextLong, RandomNextInt, RandomNextDouble,
    CryptoRandomBytes, CryptoDigest, CryptoHmac, CryptoPbkdf2, CryptoHkdf,
    CryptoAesGcmEncrypt, CryptoAesGcmDecrypt, CryptoConstantTimeEquals,
    TimeMonotonicNanos, TimeUnixMillis, TimeSleepMillis, TimeDeadlineAfterMillis,
    SocketAddressResolve, SocketAddressToText,
    TcpListenerBind, TcpListenerAccept, TcpListenerLocalAddress, TcpListenerClose,
    TcpStreamConnect, TcpStreamReadSome, TcpStreamWriteSome, TcpStreamWriteAll,
    TcpStreamPeerAddress, TcpStreamLocalAddress, TcpStreamClose,
    DatagramSource, DatagramBytesRead, DatagramTruncated,
    UdpSocketBind, UdpSocketSendTo, UdpSocketReceiveFrom, UdpSocketLocalAddress, UdpSocketClose,
    ThreadSpawn, ThreadJoin, MutexCreate, MutexCloneHandle, MutexLock, MutexTryLock,
    GuardGetRef, GuardGetMut, GuardRead, GuardWrite,
    ChannelBounded, ChannelTakeSender, ChannelTakeReceiver,
    SenderSend, SenderCloneSender, SenderClose, ReceiverReceive, ReceiverClose,
    EnvGet, EnvSet, EnvUnset,
    CommandCreate, CommandAddArg, CommandSetEnv, CommandRedirectStdin, CommandRedirectStdout,
    CommandRedirectStderr, CommandClearEnv, CommandSpawn, CommandOutput,
    ProcessWait, ProcessWaitFor, ProcessTerminate, ProcessKill,
    ProcessOutputStdout, ProcessOutputStderr, ProcessOutputStatus,
    ProcessOutputStdoutTruncated, ProcessOutputStderrTruncated, ProcessOutputTimedOut,
}
