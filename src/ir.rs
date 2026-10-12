//! Resolved, typed, structured IR. Moves and explicit drops are visible.
//! ownership.rs lowers this into an analysis CFG; codegen inserts lexical drops.
use crate::ast::{BinaryOp, UnaryOp};
use crate::diagnostic::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type { Int, Boolean, String, Void, Object(usize), Borrow { class: usize, mutable: bool } }
impl Type {
    /// True only for an owning object/enum value, never for a borrowed pointer.
    pub fn is_object(self) -> bool { matches!(self, Self::Object(_)) }
    pub fn is_borrow(self) -> bool { matches!(self, Self::Borrow { .. }) }
    pub fn object_class(self) -> Option<usize> {
        match self { Self::Object(id) | Self::Borrow { class: id, .. } => Some(id), _ => None }
    }
}
#[derive(Debug)]
pub struct Program { pub classes: Vec<Class>, pub functions: Vec<Function>, pub main: usize }
#[derive(Debug, Clone)]
pub struct Variant { pub name: String, pub fields: Vec<Type> }
#[derive(Debug)]
pub struct Class {
    pub native: Option<NativeClass>,
    pub id: usize, pub name: String, pub fields: Vec<Type>,
    pub variants: Option<Vec<Variant>>,
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
    Int(i32), Boolean(bool), String(String), Read(Place), Move(usize),
    Borrow { place: Place, mutable: bool },
    Drop(Box<Expr>),
    /// Allocation-free interface upcast, preserving owned or borrowed status.
    Upcast { value: Box<Expr>, interface: usize, concrete: usize },
    New { class: usize, constructor: usize, args: Vec<Expr> },
    Variant { class: usize, variant: usize, args: Vec<Expr> },
    Call { target: CallTarget, receiver: Option<Place>, args: Vec<Expr> },
    Unary { op: UnaryOp, expr: Box<Expr> },
    Binary { op: BinaryOp, left: Box<Expr>, right: Box<Expr> },
}
#[derive(Debug, Clone, Copy)]
pub enum CallTarget { Function(usize), Println }

/// Trusted standard-library storage, never exposed as source fields.
#[derive(Debug, Clone, Copy)]
pub enum NativeClass { List(Type), Text, Console, Files, Args }
#[derive(Debug, Clone, Copy)]
pub enum NativeMethod {
    ListNew, ListAdd, ListSize, ListCapacity, ListIsEmpty, ListGet, ListGetRef, ListGetMut,
    ListSet, ListPop, ListRemove, ListClear, ListReserve,
    TextNew, TextAppend, TextAppendInt, TextAppendBoolean, TextAppendText,
    TextByteLength, TextByteAt, TextIsEmpty, TextClear, TextPrint,
    TextClone, TextEquals, TextEqualsString, TextParseInt, TextSliceBytes,
    ConsoleReadLine, ConsolePrint, ConsolePrintText,
    FilesReadText, FilesWriteText, FilesWriteTextAtomic, ArgsCount, ArgsGet,
}
