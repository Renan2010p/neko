//! Syntax tree, after `GDScriptParser`'s node types.

#[allow(unused_imports)]
use neko_base::prelude::*;

/// A type annotation: `int`, `Node2D`, `Array[String]`, `MyClass.Inner`.
#[derive(Clone, Debug, PartialEq)]
pub struct TypeRef {
    /// Dotted name, e.g. `["MyClass", "Inner"]`.
    pub path: Vec<String>,
    /// Container element types, e.g. `Array[int]` -> `[int]`.
    pub args: Vec<TypeRef>,
}

impl TypeRef {
    pub fn name(&self) -> String {
        self.path.join(".")
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Annotation {
    pub name: String,
    pub args: Vec<Expr>,
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Extends {
    /// `extends Node2D` or `extends MyClass.Inner`
    Class(Vec<String>),
    /// `extends "res://base.gd"` optionally followed by `.Inner`
    Path(String, Vec<String>),
}

/// A script file or an inner `class`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClassDecl {
    pub name: Option<String>,
    pub extends: Option<Extends>,
    pub tool: bool,
    pub icon: Option<String>,
    pub abstract_: bool,
    pub members: Vec<Member>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Member {
    Var(VarDecl),
    Const(ConstDecl),
    Signal(SignalDecl),
    Enum(EnumDecl),
    Func(FuncDecl),
    Class(ClassDecl),
    /// A standalone annotation such as `@export_category("Stats")`.
    Annotation(Annotation),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Accessor {
    /// `set(value): <body>` / `get: <body>`
    Inline { param: Option<String>, body: Vec<Stmt> },
    /// `set = method_name` / `get = method_name`
    Method(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct VarDecl {
    pub name: String,
    pub ty: Option<TypeRef>,
    /// `var x := value` (type inferred).
    pub infer: bool,
    pub value: Option<Expr>,
    pub annotations: Vec<Annotation>,
    pub is_static: bool,
    pub setter: Option<Accessor>,
    pub getter: Option<Accessor>,
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConstDecl {
    pub name: String,
    pub ty: Option<TypeRef>,
    pub value: Expr,
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SignalDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EnumDecl {
    /// `None` for an unnamed enum (its values become class constants).
    pub name: Option<String>,
    pub values: Vec<(String, Option<Expr>)>,
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub name: String,
    pub ty: Option<TypeRef>,
    pub infer: bool,
    pub default: Option<Expr>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FuncDecl {
    /// Empty for anonymous lambdas.
    pub name: String,
    pub params: Vec<Param>,
    /// `...args` variadic parameter (Godot 4.5+).
    pub rest: Option<Param>,
    pub ret: Option<TypeRef>,
    pub body: Vec<Stmt>,
    pub is_static: bool,
    pub annotations: Vec<Annotation>,
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StmtKind {
    Expr(Expr),
    Var(VarDecl),
    Const(ConstDecl),
    If { cond: Expr, then: Vec<Stmt>, elifs: Vec<(Expr, Vec<Stmt>)>, else_: Option<Vec<Stmt>> },
    While { cond: Expr, body: Vec<Stmt> },
    For { var: String, ty: Option<TypeRef>, iter: Expr, body: Vec<Stmt> },
    Match { value: Expr, branches: Vec<MatchBranch> },
    Return(Option<Expr>),
    Break,
    Continue,
    Pass,
    Breakpoint,
    Assert { cond: Expr, message: Option<Expr> },
}

#[derive(Clone, Debug, PartialEq)]
pub struct MatchBranch {
    pub patterns: Vec<Pattern>,
    pub guard: Option<Expr>,
    pub body: Vec<Stmt>,
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Pattern {
    /// A literal or constant expression (`1`, `"a"`, `State.IDLE`).
    Expr(Expr),
    /// `var name`
    Bind(String),
    /// `_`
    Wildcard,
    /// `[a, b, ..]`
    Array(Vec<Pattern>, bool),
    /// `{"key": pattern, ..}`
    Dict(Vec<(Expr, Option<Pattern>)>, bool),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Pos,
    Not,
    BitNot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Shl,
    Shr,
    BitAnd,
    BitOr,
    BitXor,
    And,
    Or,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    In,
    NotIn,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Literal {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    StringName(String),
    NodePath(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExprKind {
    Literal(Literal),
    Ident(String),
    SelfRef,
    /// `super` or `super.method` (call wraps it).
    Super(Option<String>),
    Array(Vec<Expr>),
    Dict(Vec<(Expr, Expr)>),
    Unary(UnaryOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    /// `a if cond else b`
    Ternary { cond: Box<Expr>, then: Box<Expr>, else_: Box<Expr> },
    Call { callee: Box<Expr>, args: Vec<Expr> },
    Attr { base: Box<Expr>, name: String },
    Index { base: Box<Expr>, index: Box<Expr> },
    Is { value: Box<Expr>, ty: TypeRef, negated: bool },
    As { value: Box<Expr>, ty: TypeRef },
    Await(Box<Expr>),
    /// `$Path/To` (unique = false) or `%Name` (unique = true).
    GetNode { path: String, unique: bool },
    Preload(String),
    Lambda(Box<FuncDecl>),
    /// `target op= value`; `op` is `None` for plain `=`.
    Assign { op: Option<BinOp>, target: Box<Expr>, value: Box<Expr> },
}
