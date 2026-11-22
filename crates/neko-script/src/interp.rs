//! The interpreter: classes, objects, calls and statements.
//!
//! It walks the syntax tree (Godot compiles to bytecode for its VM; a
//! bytecode backend can replace this later without changing scripts).

use alloc::rc::{Rc, Weak};
use core::cell::{Cell, OnceCell, RefCell};

use neko_base::collections::HashMap;
#[allow(unused_imports)]
use neko_base::prelude::*;
#[cfg(not(feature = "std"))]
use neko_base::math::FloatExt;

use crate::ast::*;
use crate::builtins;
use crate::value::{Dict, Value, equals};

// ------------------------------------------------------------ errors

#[derive(Debug, Clone, PartialEq)]
pub struct ScriptError {
    pub message: String,
    pub file: String,
    pub line: u32,
    pub func: String,
}

impl core::fmt::Display for ScriptError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Godot's format: "SCRIPT ERROR: <msg>\n   at: <func> (<file>:<line>)"
        write!(f, "{}\n   at: {} ({}:{})", self.message, if self.func.is_empty() { "<global>" } else { &self.func }, self.file, self.line)
    }
}

pub type RResult<T> = Result<T, ScriptError>;

fn rerr<T>(msg: impl Into<String>) -> RResult<T> {
    Err(ScriptError { message: msg.into(), file: String::new(), line: 0, func: String::new() })
}

// ------------------------------------------------------------ classes

/// What a script class extends.
#[derive(Clone)]
pub enum Base {
    Script(Rc<ScriptClass>),
    /// An engine class (`Node2D`, `RefCounted`...).
    Native(String),
}

/// A compiled script (a `.gd` file or an inner `class`).
pub struct ScriptClass {
    pub name: Option<String>,
    pub path: String,
    pub base: Base,
    pub vars: Vec<VarDecl>,
    pub var_index: HashMap<String, usize>,
    pub consts: HashMap<String, ConstDecl>,
    const_cache: RefCell<HashMap<String, Value>>,
    pub funcs: HashMap<String, Rc<FuncDecl>>,
    pub signals: Vec<String>,
    pub enums: HashMap<String, Value>,
    pub inner: HashMap<String, Rc<ScriptClass>>,
    pub statics: RefCell<HashMap<String, Value>>,
    statics_ready: Cell<bool>,
    /// Weak link to the enclosing class, for inner classes.
    pub outer: RefCell<Weak<ScriptClass>>,
    /// The class's own `Rc` (set once when built), so methods found on a
    /// `&ScriptClass` can hand out owners.
    self_ref: OnceCell<Weak<ScriptClass>>,
}

impl ScriptClass {
    /// The native class at the root of the inheritance chain.
    pub fn native_base(&self) -> String {
        match &self.base {
            Base::Script(s) => s.native_base(),
            Base::Native(n) => n.clone(),
        }
    }

    pub fn find_func(&self, name: &str) -> Option<(Rc<FuncDecl>, Rc<ScriptClass>)> {
        if let Some(f) = self.funcs.get(name) {
            // The owner is found by the caller through `owner_of` when needed.
            return Some((f.clone(), self.self_rc()?));
        }
        match &self.base {
            Base::Script(b) => b.find_func(name),
            Base::Native(_) => None,
        }
    }

    fn self_rc(&self) -> Option<Rc<ScriptClass>> {
        self.self_ref.get().and_then(Weak::upgrade)
    }

    pub fn find_var(&self, name: &str) -> Option<VarDecl> {
        if let Some(&i) = self.var_index.get(name) {
            return Some(self.vars[i].clone());
        }
        match &self.base {
            Base::Script(b) => b.find_var(name),
            Base::Native(_) => None,
        }
    }

    pub fn has_signal(&self, name: &str) -> bool {
        self.signals.iter().any(|s| s == name) || matches!(&self.base, Base::Script(b) if b.has_signal(name))
    }

    pub fn inherits_script(&self, other: &ScriptClass) -> bool {
        core::ptr::eq(self, other) || matches!(&self.base, Base::Script(b) if b.inherits_script(other))
    }

    pub fn display_name(&self) -> String {
        self.name.clone().unwrap_or_else(|| self.path.clone())
    }
}

/// Instance ids from a shared atomic counter: unique across threads, never reused.
#[cfg(target_has_atomic = "64")]
fn next_object_id() -> u64 {
    use core::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

// ------------------------------------------------------------ objects

pub enum ObjKind {
    /// An instance (of a script, a native class, or both).
    Instance,
    /// A script class used as a value (`MyClass.new()`, `MyClass.CONST`).
    Class(Rc<ScriptClass>),
    /// A native class used as a value (`Node2D.new()`).
    NativeClass(String),
}

pub struct ObjData {
    #[cfg(target_has_atomic = "64")]
    id: u64,
    pub kind: ObjKind,
    pub script: RefCell<Option<Rc<ScriptClass>>>,
    pub vars: RefCell<HashMap<String, Value>>,
    /// Engine class of the instance (`Node2D`, `RefCounted`...).
    pub native_class: String,
    /// Opaque handle the host uses to find the engine-side object.
    pub native: Cell<u64>,
    pub connections: RefCell<HashMap<String, Vec<Connection>>>,
    pub freed: Cell<bool>,
}

#[derive(Clone)]
pub struct Connection {
    pub callable: Rc<Callable>,
    pub one_shot: bool,
}

/// A reference to an object. Engine objects are freed explicitly (Godot's
/// `free`/`queue_free`); after that the reference reads as invalid.
#[derive(Clone)]
pub struct Object(pub Rc<ObjData>);

impl Object {
    pub fn new(kind: ObjKind, native_class: &str, script: Option<Rc<ScriptClass>>) -> Self {
        Object(Rc::new(ObjData {
            #[cfg(target_has_atomic = "64")]
            id: next_object_id(),
            kind,
            script: RefCell::new(script),
            vars: RefCell::new(HashMap::default()),
            native_class: native_class.to_string(),
            native: Cell::new(0),
            connections: RefCell::new(HashMap::default()),
            freed: Cell::new(false),
        }))
    }

    /// Unique while the object lives (Godot's instance id).
    pub fn id(&self) -> u64 {
        #[cfg(target_has_atomic = "64")]
        {
            self.0.id
        }
        // Without atomics (e.g. the PS1) the object's address is the id.
        #[cfg(not(target_has_atomic = "64"))]
        {
            Rc::as_ptr(&self.0) as usize as u64
        }
    }

    pub fn is_valid(&self) -> bool {
        !self.0.freed.get()
    }

    pub fn script(&self) -> Option<Rc<ScriptClass>> {
        self.0.script.borrow().clone()
    }

    pub fn describe(&self) -> String {
        if !self.is_valid() {
            return "<Freed Object>".into();
        }
        match &self.0.kind {
            ObjKind::Class(c) => format!("<GDScript#{}>", c.display_name()),
            ObjKind::NativeClass(n) => n.clone(),
            ObjKind::Instance => format!("<{}#{}>", self.0.native_class, self.id()),
        }
    }
}

// ------------------------------------------------------------ callables

pub enum Callable {
    /// `obj.method` (optionally with bound arguments).
    Method { obj: Object, name: Rc<str>, bound: Vec<Value> },
    /// A lambda with the values it captured.
    Lambda { func: Rc<FuncDecl>, class: Rc<ScriptClass>, this: Option<Object>, captures: HashMap<String, Value>, bound: Vec<Value> },
    /// A utility function by name (`print`, `str`...).
    Builtin { name: Rc<str>, bound: Vec<Value> },
}

impl Callable {
    pub fn describe(&self) -> String {
        match self {
            Callable::Method { obj, name, .. } => format!("{}::{name}", obj.describe()),
            Callable::Lambda { func, .. } => if func.name.is_empty() { "<anonymous lambda>".into() } else { format!("{}(lambda)", func.name) },
            Callable::Builtin { name, .. } => format!("@GlobalScope::{name}"),
        }
    }

    pub fn same_target(&self, o: &Callable) -> bool {
        match (self, o) {
            (Callable::Method { obj: a, name: n, .. }, Callable::Method { obj: b, name: m, .. }) => a.id() == b.id() && n == m,
            _ => false,
        }
    }

    fn with_bound(&self, extra: &[Value]) -> Callable {
        let add = |b: &Vec<Value>| b.iter().cloned().chain(extra.iter().cloned()).collect();
        match self {
            Callable::Method { obj, name, bound } => Callable::Method { obj: obj.clone(), name: name.clone(), bound: add(bound) },
            Callable::Lambda { func, class, this, captures, bound } => Callable::Lambda { func: func.clone(), class: class.clone(), this: this.clone(), captures: captures.clone(), bound: add(bound) },
            Callable::Builtin { name, bound } => Callable::Builtin { name: name.clone(), bound: add(bound) },
        }
    }
}

// ------------------------------------------------------------ host

/// A script to attach to an object after the host call returns.
pub struct PendingScript {
    pub obj: Object,
    pub path: String,
}

/// A call to a script method queued by a host call (`call_group`...).
pub struct PendingCall {
    pub obj: Object,
    pub method: String,
    pub args: Vec<Value>,
}

/// A signal connection queued by a host call (`PackedScene.instantiate`).
pub struct PendingConnection {
    pub src: Object,
    pub signal: String,
    pub dst: Object,
    pub method: String,
    pub binds: Vec<Value>,
    pub flags: i64,
}

/// The engine side the interpreter talks to.
pub trait Host {
    /// An engine property of an object (`position`, `visible`...).
    fn get_property(&mut self, obj: &Object, name: &str) -> Option<Value>;
    /// Set an engine property; false if the object has no such property.
    fn set_property(&mut self, obj: &Object, name: &str, value: &Value) -> bool;
    /// Call an engine method; `None` if the object has no such method.
    fn call_method(&mut self, it: &mut Interp, obj: &Object, name: &str, args: &[Value]) -> Option<RResult<Value>>;
    /// `$Path` / `%Unique` / `get_node()` relative to `from`.
    fn get_node(&mut self, from: &Object, path: &str) -> Option<Object>;
    /// Engine singletons and global classes (`Input`, `OS`, `Engine`...).
    fn global(&mut self, it: &mut Interp, name: &str) -> Option<Value>;
    /// Whether an engine object is (or inherits) a native class.
    fn is_class(&self, obj: &Object, class: &str) -> bool;
    /// Whether an engine object has a native signal (`timeout`, `pressed`...).
    fn has_signal(&self, _obj: &Object, _name: &str) -> bool {
        false
    }
    /// Create a native object (`Node2D.new()`).
    fn instantiate(&mut self, it: &mut Interp, class: &str) -> Option<Object>;
    /// `load()` / `preload()` of a resource path.
    fn load(&mut self, it: &mut Interp, path: &str) -> Option<Value>;
    fn print(&mut self, text: &str);
    fn print_error(&mut self, text: &str);
    /// Work created inside a host call that needs the interpreter again.
    /// It is processed by [`Interp::call_method`] after the host is restored,
    /// so engine methods never re-enter the interpreter directly.
    fn take_pending_scripts(&mut self) -> Vec<PendingScript> {
        Vec::new()
    }
    fn take_pending_calls(&mut self) -> Vec<PendingCall> {
        Vec::new()
    }
    fn take_pending_connections(&mut self) -> Vec<PendingConnection> {
        Vec::new()
    }
}

/// A host with no engine, for running pure scripts (tests, tools).
#[derive(Default)]
pub struct NullHost {
    pub output: Vec<String>,
}

impl Host for NullHost {
    fn get_property(&mut self, _: &Object, _: &str) -> Option<Value> {
        None
    }
    fn set_property(&mut self, _: &Object, _: &str, _: &Value) -> bool {
        false
    }
    fn call_method(&mut self, _: &mut Interp, _: &Object, _: &str, _: &[Value]) -> Option<RResult<Value>> {
        None
    }
    fn get_node(&mut self, _: &Object, _: &str) -> Option<Object> {
        None
    }
    fn global(&mut self, _: &mut Interp, _: &str) -> Option<Value> {
        None
    }
    fn is_class(&self, obj: &Object, class: &str) -> bool {
        obj.0.native_class == class || class == "Object" || (class == "RefCounted" && obj.0.native_class != "Object")
    }
    fn instantiate(&mut self, _: &mut Interp, class: &str) -> Option<Object> {
        Some(Object::new(ObjKind::Instance, class, None))
    }
    fn load(&mut self, _: &mut Interp, _: &str) -> Option<Value> {
        None
    }
    fn print(&mut self, text: &str) {
        self.output.push(text.to_string());
    }
    fn print_error(&mut self, text: &str) {
        self.output.push(format!("ERROR: {text}"));
    }
}

// ------------------------------------------------------------ interpreter

enum Flow {
    Normal,
    Return(Value),
    Break,
    Continue,
}

struct Frame {
    scopes: Vec<HashMap<String, Value>>,
    /// Declared types of locals that convert on assignment (`int`, `float`).
    types: Vec<HashMap<String, String>>,
    this: Option<Object>,
    class: Rc<ScriptClass>,
    func: String,
}

// ------------------------------------------------------------ await

/// One compiled instruction of a function that contains `await` (Godot's
/// coroutine state machine). Only statement-level awaits need compiling.
#[derive(Clone)]
enum Instr {
    Eval(Expr),
    Declare(String, Option<TypeRef>, Option<Expr>),
    DeclareAwait(String, Option<TypeRef>),
    Jump(usize),
    JumpIfFalse(Expr, usize),
    JumpIfFalseAwait(usize),
    /// Jump and drop `n` active `for` iterators (break/continue).
    JumpPop(usize, usize),
    Await(Expr),
    Return(Option<Expr>),
    ReturnAwait,
    ForInit(Expr),
    ForNext { var: String, ty: Option<TypeRef>, body: usize, end: usize },
    Match(Expr, Vec<MatchBranch>),
    Assert(Expr, Option<Expr>),
}

#[derive(Clone)]
struct AsyncCode {
    instrs: Vec<Instr>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Wait {
    Ready,
    Signal,
    Task(u64),
    /// Resume after this many `run_tasks()` calls (process_frame etc).
    Frames(u32),
}

struct ForIter {
    items: Vec<Value>,
    next: usize,
}

struct AsyncTask {
    id: u64,
    code: Rc<AsyncCode>,
    pc: usize,
    frame: Frame,
    line: u32,
    wait: Wait,
    await_value: Value,
    parent: Option<u64>,
    loops: Vec<ForIter>,
    done: bool,
    result: Value,
    ret: Option<TypeRef>,
    error: Option<ScriptError>,
}

fn expr_has_await(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Await(_) => true,
        ExprKind::Array(items) => items.iter().any(expr_has_await),
        ExprKind::Dict(items) => items.iter().any(|(k, v)| expr_has_await(k) || expr_has_await(v)),
        ExprKind::Unary(_, v) => expr_has_await(v),
        ExprKind::Binary(_, a, b) => expr_has_await(a) || expr_has_await(b),
        ExprKind::Ternary { cond, then, else_ } => expr_has_await(cond) || expr_has_await(then) || expr_has_await(else_),
        ExprKind::Call { callee, args } => expr_has_await(callee) || args.iter().any(expr_has_await),
        ExprKind::Attr { base, .. } => expr_has_await(base),
        ExprKind::Index { base, index } => expr_has_await(base) || expr_has_await(index),
        ExprKind::Is { value, .. } | ExprKind::As { value, .. } => expr_has_await(value),
        ExprKind::Assign { target, value, .. } => expr_has_await(target) || expr_has_await(value),
        // A lambda's awaits make the lambda async, not the enclosing function.
        ExprKind::Literal(_) | ExprKind::Ident(_) | ExprKind::SelfRef | ExprKind::Super(_) | ExprKind::GetNode { .. } | ExprKind::Preload(_) | ExprKind::Lambda(_) => false,
    }
}

fn stmt_has_await(s: &Stmt) -> bool {
    match &s.kind {
        StmtKind::Expr(e) => expr_has_await(e),
        StmtKind::Var(v) => v.value.as_ref().is_some_and(expr_has_await) || v.getter.as_ref().is_some_and(|a| matches!(a, Accessor::Inline { body, .. } if body.iter().any(stmt_has_await))) || v.setter.as_ref().is_some_and(|a| matches!(a, Accessor::Inline { body, .. } if body.iter().any(stmt_has_await))),
        StmtKind::Const(c) => expr_has_await(&c.value),
        StmtKind::If { cond, then, elifs, else_ } => expr_has_await(cond) || then.iter().any(stmt_has_await) || elifs.iter().any(|(c, b)| expr_has_await(c) || b.iter().any(stmt_has_await)) || else_.as_ref().is_some_and(|b| b.iter().any(stmt_has_await)),
        StmtKind::While { cond, body } => expr_has_await(cond) || body.iter().any(stmt_has_await),
        StmtKind::For { iter, body, .. } => expr_has_await(iter) || body.iter().any(stmt_has_await),
        StmtKind::Match { value, branches } => expr_has_await(value) || branches.iter().any(|b| b.patterns.iter().any(pattern_has_await) || b.guard.as_ref().is_some_and(expr_has_await) || b.body.iter().any(stmt_has_await)),
        StmtKind::Return(v) => v.as_ref().is_some_and(expr_has_await),
        StmtKind::Assert { cond, message } => expr_has_await(cond) || message.as_ref().is_some_and(expr_has_await),
        StmtKind::Break | StmtKind::Continue | StmtKind::Pass | StmtKind::Breakpoint => false,
    }
}

fn pattern_has_await(p: &Pattern) -> bool {
    match p {
        Pattern::Expr(e) => expr_has_await(e),
        Pattern::Array(ps, _) => ps.iter().any(pattern_has_await),
        Pattern::Dict(ps, _) => ps.iter().any(|(k, v)| expr_has_await(k) || v.as_ref().is_some_and(pattern_has_await)),
        Pattern::Bind(_) | Pattern::Wildcard => false,
    }
}

fn func_has_await(f: &FuncDecl) -> bool {
    f.body.iter().any(stmt_has_await)
}

struct LoopCtx {
    is_for: bool,
    continue_pc: usize,
    /// Number of active `for` iterators before this loop was entered.
    for_depth: usize,
    breaks: Vec<usize>,
}

struct Compiler {
    code: Vec<Instr>,
    loops: Vec<LoopCtx>,
}

impl Compiler {
    fn emit(&mut self, i: Instr) -> usize {
        self.code.push(i);
        self.code.len() - 1
    }

    fn for_count(&self) -> usize {
        self.loops.iter().filter(|l| l.is_for).count()
    }

    fn unsupported() -> RResult<()> {
        rerr("await inside this expression is not supported yet; move it to its own statement")
    }

    /// Top-level `await e` of a statement, if any.
    fn top_await(e: &Expr) -> Option<&Expr> {
        match &e.kind {
            ExprKind::Await(inner) => Some(inner),
            _ => None,
        }
    }

    fn stmts(&mut self, stmts: &[Stmt]) -> RResult<()> {
        for s in stmts {
            self.stmt(s)?;
        }
        Ok(())
    }

    fn stmt(&mut self, s: &Stmt) -> RResult<()> {
        match &s.kind {
            StmtKind::Expr(e) => {
                if let Some(inner) = Self::top_await(e) {
                    self.emit(Instr::Await(inner.clone()));
                } else if expr_has_await(e) {
                    return Self::unsupported();
                } else {
                    self.emit(Instr::Eval(e.clone()));
                }
            }
            StmtKind::Var(v) => {
                if let Some(inner) = v.value.as_ref().and_then(Self::top_await) {
                    self.emit(Instr::Await(inner.clone()));
                    self.emit(Instr::DeclareAwait(v.name.clone(), v.ty.clone()));
                } else if v.value.as_ref().is_some_and(expr_has_await) {
                    return Self::unsupported();
                } else {
                    self.emit(Instr::Declare(v.name.clone(), v.ty.clone(), v.value.clone()));
                }
            }
            StmtKind::Const(c) => {
                if expr_has_await(&c.value) {
                    return Self::unsupported();
                }
                self.emit(Instr::Declare(c.name.clone(), c.ty.clone(), Some(c.value.clone())));
            }
            StmtKind::If { cond, then, elifs, else_ } => {
                let mut end_jumps = Vec::new();
                let mut cond_jump = if let Some(inner) = Self::top_await(cond) {
                    self.emit(Instr::Await(inner.clone()));
                    Some(self.emit(Instr::JumpIfFalseAwait(usize::MAX)))
                } else if expr_has_await(cond) {
                    return Self::unsupported();
                } else {
                    Some(self.emit(Instr::JumpIfFalse(cond.clone(), usize::MAX)))
                };
                self.stmts(then)?;
                end_jumps.push(self.emit(Instr::Jump(usize::MAX)));
                for (c, body) in elifs {
                    let next = self.code.len();
                    if let Some(j) = cond_jump.take() {
                        self.patch(j, next);
                    }
                    cond_jump = if let Some(inner) = Self::top_await(c) {
                        self.emit(Instr::Await(inner.clone()));
                        Some(self.emit(Instr::JumpIfFalseAwait(usize::MAX)))
                    } else {
                        Some(self.emit(Instr::JumpIfFalse(c.clone(), usize::MAX)))
                    };
                    self.stmts(body)?;
                    end_jumps.push(self.emit(Instr::Jump(usize::MAX)));
                }
                if let Some(b) = else_ {
                    let next = self.code.len();
                    if let Some(j) = cond_jump.take() {
                        self.patch(j, next);
                    }
                    self.stmts(b)?;
                } else {
                    let end = self.code.len();
                    if let Some(j) = cond_jump.take() {
                        self.patch(j, end);
                    }
                }
                let end = self.code.len();
                for j in end_jumps {
                    self.patch(j, end);
                }
            }
            StmtKind::While { cond, body } => {
                let start = self.code.len();
                let cond_jump = if let Some(inner) = Self::top_await(cond) {
                    self.emit(Instr::Await(inner.clone()));
                    self.emit(Instr::JumpIfFalseAwait(usize::MAX))
                } else if expr_has_await(cond) {
                    return Self::unsupported();
                } else {
                    self.emit(Instr::JumpIfFalse(cond.clone(), usize::MAX))
                };
                let depth = self.for_count();
                self.loops.push(LoopCtx { is_for: false, continue_pc: start, for_depth: depth, breaks: Vec::new() });
                self.stmts(body)?;
                let ctx = self.loops.pop().expect("loop");
                self.emit(Instr::Jump(start));
                let end = self.code.len();
                self.patch(cond_jump, end);
                for j in ctx.breaks {
                    self.patch(j, end);
                }
            }
            StmtKind::For { var, ty, iter, body } => {
                if expr_has_await(iter) {
                    return Self::unsupported();
                }
                self.emit(Instr::ForInit(iter.clone()));
                let next = self.emit(Instr::ForNext { var: var.clone(), ty: ty.clone(), body: usize::MAX, end: usize::MAX });
                let body_start = self.code.len();
                let depth = self.for_count();
                self.loops.push(LoopCtx { is_for: true, continue_pc: next, for_depth: depth, breaks: Vec::new() });
                self.stmts(body)?;
                let ctx = self.loops.pop().expect("loop");
                self.emit(Instr::Jump(next));
                let end = self.code.len();
                if let Instr::ForNext { body, end: e, .. } = &mut self.code[next] {
                    *body = body_start;
                    *e = end;
                }
                for j in ctx.breaks {
                    self.patch(j, end);
                }
            }
            StmtKind::Match { value, branches } => {
                if expr_has_await(value) || branches.iter().any(|b| b.patterns.iter().any(pattern_has_await) || b.guard.as_ref().is_some_and(expr_has_await) || b.body.iter().any(stmt_has_await)) {
                    return Self::unsupported();
                }
                self.emit(Instr::Match(value.clone(), branches.clone()));
            }
            StmtKind::Return(v) => {
                if let Some(inner) = v.as_ref().and_then(Self::top_await) {
                    self.emit(Instr::Await(inner.clone()));
                    self.emit(Instr::ReturnAwait);
                } else if v.as_ref().is_some_and(expr_has_await) {
                    return Self::unsupported();
                } else {
                    self.emit(Instr::Return(v.clone()));
                }
            }
            StmtKind::Break => {
                let Some(ctx) = self.loops.last() else { return rerr("break outside a loop"); };
                let pop = self.for_count() - ctx.for_depth;
                let at = self.emit(Instr::JumpPop(usize::MAX, pop));
                self.loops.last_mut().expect("loop").breaks.push(at);
            }
            StmtKind::Continue => {
                let Some(ctx) = self.loops.last() else { return rerr("continue outside a loop"); };
                let keep = usize::from(ctx.is_for);
                let pop = self.for_count() - ctx.for_depth - keep;
                let target = ctx.continue_pc;
                self.emit(Instr::JumpPop(target, pop));
            }
            StmtKind::Assert { cond, message } => {
                if expr_has_await(cond) || message.as_ref().is_some_and(expr_has_await) {
                    return Self::unsupported();
                }
                self.emit(Instr::Assert(cond.clone(), message.clone()));
            }
            StmtKind::Pass | StmtKind::Breakpoint => {}
        }
        Ok(())
    }

    fn patch(&mut self, at: usize, target: usize) {
        match &mut self.code[at] {
            Instr::Jump(t) | Instr::JumpIfFalseAwait(t) => *t = target,
            Instr::JumpIfFalse(_, t) => *t = target,
            Instr::JumpPop(t, _) => *t = target,
            _ => {}
        }
    }
}

fn compile_async(f: &FuncDecl) -> RResult<Rc<AsyncCode>> {
    let mut c = Compiler { code: Vec::new(), loops: Vec::new() };
    c.stmts(&f.body)?;
    c.emit(Instr::Return(None));
    Ok(Rc::new(AsyncCode { instrs: c.code }))
}

/// Reads a script's source by `res://` path.
pub type SourceLoader = Box<dyn FnMut(&str) -> Option<String>>;

pub struct Interp {
    /// `class_name` classes and autoloads by name.
    pub globals: HashMap<String, Value>,
    /// Loaded scripts by `res://` path.
    pub scripts: HashMap<String, Rc<ScriptClass>>,
    frames: Vec<Frame>,
    /// Members being accessed from their own setter/getter (no recursion).
    in_accessor: Vec<(u64, String)>,
    line: u32,
    /// Source loader: `res://` path to text.
    pub source: SourceLoader,
    pub host: Option<Box<dyn Host>>,
    warned_await: bool,
    /// Pending `await` coroutines (functions containing `await`).
    tasks: HashMap<u64, AsyncTask>,
    /// `(object id, signal)` -> tasks waiting for that signal.
    signal_waiters: HashMap<(u64, String), Vec<u64>>,
    next_task_id: u64,
    /// Random state for `randi()`/`randf()` (Godot's global RandomNumberGenerator),
    /// per interpreter so several can run on different threads.
    pub(crate) rng: Cell<u64>,
}

impl Default for Interp {
    fn default() -> Self {
        Self::new()
    }
}

impl Interp {
    pub fn new() -> Self {
        Self {
            globals: HashMap::default(),
            scripts: HashMap::default(),
            frames: Vec::new(),
            in_accessor: Vec::new(),
            line: 0,
            source: Box::new(|_| None),
            host: Some(Box::new(NullHost::default())),
            warned_await: false,
            tasks: HashMap::default(),
            signal_waiters: HashMap::default(),
            next_task_id: 1,
            rng: Cell::new(0x2545_F491_4F6C_DD1D),
        }
    }

    /// Temporarily take the host to call it with `&mut self`.
    pub(crate) fn with_host<R>(&mut self, f: impl FnOnce(&mut dyn Host, &mut Interp) -> R) -> R {
        let mut h = self.host.take().expect("host is present outside host calls");
        let r = f(h.as_mut(), self);
        self.host = Some(h);
        r
    }

    pub fn print(&mut self, text: &str) {
        self.with_host(|h, _| h.print(text));
    }

    fn fail<T>(&self, msg: impl Into<String>) -> RResult<T> {
        let (file, func) = self.frames.last().map(|f| (f.class.path.clone(), f.func.clone())).unwrap_or_default();
        Err(ScriptError { message: msg.into(), file, line: self.line, func })
    }

    fn locate(&self, mut e: ScriptError) -> ScriptError {
        if e.file.is_empty() {
            if let Some(f) = self.frames.last() {
                e.file = f.class.path.clone();
                e.func = f.func.clone();
            }
            e.line = self.line;
        }
        e
    }

    // ------------------------------------------------------------ loading

    /// Compile a script from source.
    pub fn compile(&mut self, path: &str, src: &str) -> RResult<Rc<ScriptClass>> {
        let decl = crate::parser::parse(src).map_err(|e| ScriptError { message: format!("Parse Error: {}", e.message), file: path.into(), line: e.line, func: String::new() })?;
        let c = self.build_class(&decl, path)?;
        if let Some(n) = &decl.name {
            self.globals.insert(n.clone(), Value::Object(Object::new(ObjKind::Class(c.clone()), "GDScript", None)));
        }
        self.scripts.insert(path.to_string(), c.clone());
        Ok(c)
    }

    /// Load a script by path (cached).
    pub fn load_script(&mut self, path: &str) -> RResult<Rc<ScriptClass>> {
        if let Some(c) = self.scripts.get(path) {
            return Ok(c.clone());
        }
        let Some(src) = (self.source)(path) else { return rerr(format!("Cannot open script \"{path}\".")) };
        self.compile(path, &src)
    }

    fn build_class(&mut self, decl: &ClassDecl, path: &str) -> RResult<Rc<ScriptClass>> {
        let base = match &decl.extends {
            None => Base::Native("RefCounted".into()),
            Some(Extends::Path(p, sub)) => {
                let mut c = self.load_script(p)?;
                for s in sub {
                    c = c.inner.get(s).cloned().ok_or_else(|| ScriptError { message: format!("No inner class \"{s}\" in {p}"), file: path.into(), line: 0, func: String::new() })?;
                }
                Base::Script(c)
            }
            Some(Extends::Class(names)) => {
                let first = &names[0];
                match self.class_by_name(first) {
                    Some(mut c) => {
                        for s in &names[1..] {
                            c = c.inner.get(s).cloned().ok_or_else(|| ScriptError { message: format!("No inner class \"{s}\""), file: path.into(), line: 0, func: String::new() })?;
                        }
                        Base::Script(c)
                    }
                    None => Base::Native(first.clone()),
                }
            }
        };
        let mut c = ScriptClass {
            name: decl.name.clone(),
            path: path.to_string(),
            base,
            vars: Vec::new(),
            var_index: HashMap::default(),
            consts: HashMap::default(),
            const_cache: RefCell::new(HashMap::default()),
            funcs: HashMap::default(),
            signals: Vec::new(),
            enums: HashMap::default(),
            inner: HashMap::default(),
            statics: RefCell::new(HashMap::default()),
            statics_ready: Cell::new(false),
            outer: RefCell::new(Weak::new()),
            self_ref: OnceCell::new(),
        };
        for m in &decl.members {
            match m {
                Member::Var(v) => {
                    c.var_index.insert(v.name.clone(), c.vars.len());
                    c.vars.push(v.clone());
                }
                Member::Const(k) => {
                    c.consts.insert(k.name.clone(), k.clone());
                }
                Member::Func(f) => {
                    c.funcs.insert(f.name.clone(), Rc::new(f.clone()));
                }
                Member::Signal(s) => c.signals.push(s.name.clone()),
                Member::Enum(e) => {
                    // Enum values are evaluated now (they are constant expressions).
                    let mut d = Dict::new();
                    let mut next = 0i64;
                    for (n, v) in &e.values {
                        let val = match v {
                            Some(expr) => self.const_eval(expr, &c)?.as_i64().unwrap_or(next),
                            None => next,
                        };
                        next = val + 1;
                        d.set(Value::str(n), Value::Int(val));
                        if e.name.is_none() {
                            c.const_cache.borrow_mut().insert(n.clone(), Value::Int(val));
                        }
                    }
                    if let Some(n) = &e.name {
                        c.enums.insert(n.clone(), Value::dict(d));
                    }
                }
                Member::Class(inner) => {
                    let ic = self.build_class(inner, &format!("{path}::{}", inner.name.clone().unwrap_or_default()))?;
                    c.inner.insert(inner.name.clone().unwrap_or_default(), ic);
                }
                Member::Annotation(_) => {}
            }
        }
        let rc = Rc::new(c);
        let _ = rc.self_ref.set(Rc::downgrade(&rc));
        for ic in rc.inner.values() {
            *ic.outer.borrow_mut() = Rc::downgrade(&rc);
        }
        Ok(rc)
    }

    /// Constant expressions while building a class (enum values).
    fn const_eval(&mut self, e: &Expr, c: &ScriptClass) -> RResult<Value> {
        match &e.kind {
            ExprKind::Literal(l) => Ok(lit(l)),
            ExprKind::Ident(n) => Ok(c.const_cache.borrow().get(n).cloned().unwrap_or(Value::Nil)),
            ExprKind::Unary(UnaryOp::Neg, x) => Ok(builtins::negate(&self.const_eval(x, c)?)),
            ExprKind::Binary(op, a, b) => {
                let (a, b) = (self.const_eval(a, c)?, self.const_eval(b, c)?);
                builtins::binary(*op, &a, &b).or_else(rerr)
            }
            _ => Ok(Value::Nil),
        }
    }

    fn class_by_name(&mut self, name: &str) -> Option<Rc<ScriptClass>> {
        match self.globals.get(name) {
            Some(Value::Object(o)) => match &o.0.kind {
                ObjKind::Class(c) => Some(c.clone()),
                _ => None,
            },
            _ => None,
        }
    }

    // ------------------------------------------------------------ instances

    /// Create an object running `class` (Godot's `Script.new()` / attaching a
    /// script to a node). Member defaults are initialized, base first.
    pub fn instantiate(&mut self, class: &Rc<ScriptClass>, native: Option<Object>) -> RResult<Object> {
        let obj = match native {
            Some(o) => {
                *o.0.script.borrow_mut() = Some(class.clone());
                o
            }
            None => {
                let nb = class.native_base();
                let o = self.with_host(|h, it| h.instantiate(it, &nb)).unwrap_or_else(|| Object::new(ObjKind::Instance, &nb, None));
                *o.0.script.borrow_mut() = Some(class.clone());
                o
            }
        };
        self.init_members(class, &obj)?;
        Ok(obj)
    }

    fn init_members(&mut self, class: &Rc<ScriptClass>, obj: &Object) -> RResult<()> {
        if let Base::Script(b) = &class.base {
            self.init_members(b, obj)?;
        }
        self.ensure_statics(class)?;
        for v in &class.vars {
            if v.is_static {
                continue;
            }
            // @onready values are assigned right before _ready (see `ready`).
            let onready = v.annotations.iter().any(|a| a.name == "onready");
            // A resource loaded from disk already has its serialized values;
            // script defaults must not overwrite them.
            if !onready && obj.0.vars.borrow().contains_key(&v.name) {
                continue;
            }
            let value = match (&v.value, onready) {
                (Some(e), false) => {
                    self.push_frame(Some(obj.clone()), class.clone(), "<member init>");
                    let r = self.eval(e);
                    self.frames.pop();
                    convert_typed(r?, v.ty.as_ref())
                }
                _ => default_for(v.ty.as_ref()),
            };
            obj.0.vars.borrow_mut().insert(v.name.clone(), value);
        }
        Ok(())
    }

    fn ensure_statics(&mut self, class: &Rc<ScriptClass>) -> RResult<()> {
        if class.statics_ready.replace(true) {
            return Ok(());
        }
        for v in class.vars.iter().filter(|v| v.is_static) {
            let value = match &v.value {
                Some(e) => {
                    self.push_frame(None, class.clone(), "<static init>");
                    let r = self.eval(e);
                    self.frames.pop();
                    convert_typed(r?, v.ty.as_ref())
                }
                None => default_for(v.ty.as_ref()),
            };
            class.statics.borrow_mut().insert(v.name.clone(), value);
        }
        Ok(())
    }

    /// Godot's order before `_ready`: assign `@onready` members.
    pub fn assign_onready(&mut self, obj: &Object) -> RResult<()> {
        let Some(class) = obj.script() else { return Ok(()) };
        let mut chain = Vec::new();
        let mut c = Some(class);
        while let Some(cl) = c {
            c = match &cl.base {
                Base::Script(b) => Some(b.clone()),
                Base::Native(_) => None,
            };
            chain.push(cl);
        }
        for cl in chain.into_iter().rev() {
            for v in cl.vars.iter().filter(|v| v.annotations.iter().any(|a| a.name == "onready")) {
                if let Some(e) = &v.value {
                    self.push_frame(Some(obj.clone()), cl.clone(), "<onready>");
                    let r = self.eval(e);
                    self.frames.pop();
                    obj.0.vars.borrow_mut().insert(v.name.clone(), convert_typed(r?, v.ty.as_ref()));
                }
            }
        }
        Ok(())
    }

    fn push_frame(&mut self, this: Option<Object>, class: Rc<ScriptClass>, func: &str) {
        self.frames.push(Frame { scopes: vec![HashMap::default()], types: vec![HashMap::default()], this, class, func: func.to_string() });
    }

    // ------------------------------------------------------------ calls

    /// Call a method on an object if it has one (script or engine).
    pub fn call_method(&mut self, obj: &Object, name: &str, args: &[Value]) -> RResult<Value> {
        if let Some(class) = obj.script()
            && let Some((f, owner)) = class.find_func(name)
        {
            return self.call_func(&f, &owner, Some(obj.clone()), args, &HashMap::default());
        }
        if let ObjKind::Class(c) = &obj.0.kind {
            if name == "new" {
                let c = c.clone();
                let o = self.instantiate(&c, None)?;
                if let Some((f, owner)) = c.find_func("_init") {
                    self.call_func(&f, &owner, Some(o.clone()), args, &HashMap::default())?;
                }
                return Ok(Value::Object(o));
            }
            if let Some((f, owner)) = c.find_func(name) {
                return self.call_func(&f, &owner, None, args, &HashMap::default());
            }
        }
        if let ObjKind::NativeClass(n) = &obj.0.kind
            && name == "new"
        {
            let n = n.clone();
            return Ok(self.with_host(|h, it| h.instantiate(it, &n)).map(Value::Object).unwrap_or(Value::Nil));
        }
        if let Some(r) = builtins::object_method(self, obj, name, args) {
            return r;
        }
        if let Some(r) = self.with_host(|h, it| h.call_method(it, obj, name, args)) {
            self.flush_pending();
            return r.map_err(|e| self.locate(e));
        }
        self.fail(format!("Invalid call. Nonexistent function '{name}' in base '{}'.", obj.describe()))
    }

    /// Whether an object has a method (script or engine).
    pub fn has_method(&mut self, obj: &Object, name: &str) -> bool {
        obj.script().is_some_and(|c| c.find_func(name).is_some())
    }

    /// Process work a host call queued (script attachment, connections, calls).
    /// Runs with the host restored, so `_init` and callbacks can call engine
    /// methods normally.
    fn flush_pending(&mut self) {
        loop {
            let (scripts, conns, calls) = {
                let Some(h) = self.host.as_mut() else { return };
                (h.take_pending_scripts(), h.take_pending_connections(), h.take_pending_calls())
            };
            if scripts.is_empty() && conns.is_empty() && calls.is_empty() {
                break;
            }
            for ps in scripts {
                match self.load_script(&ps.path).and_then(|c| self.instantiate(&c, Some(ps.obj.clone()))) {
                    Ok(o) => {
                        if o.script().is_some_and(|c| c.find_func("_init").is_some())
                            && let Err(e) = self.call_method(&o, "_init", &[])
                        {
                            self.report_pending(e);
                        }
                    }
                    Err(e) => self.report_pending(e),
                }
            }
            for pc in conns {
                let c = Callable::Method { obj: pc.dst, name: Rc::from(pc.method.as_str()), bound: pc.binds };
                self.connect(&pc.src, &pc.signal, Rc::new(c), pc.flags);
            }
            for p in calls {
                if let Err(e) = self.call_method(&p.obj, &p.method, &p.args) {
                    self.report_pending(e);
                }
            }
        }
    }

    fn report_pending(&mut self, e: ScriptError) {
        let msg = format!("SCRIPT ERROR: {e}");
        self.with_host(|h, _| h.print_error(&msg));
    }

    pub fn call_callable(&mut self, c: &Callable, args: &[Value]) -> RResult<Value> {
        match c {
            Callable::Method { obj, name, bound } => {
                let all: Vec<Value> = args.iter().cloned().chain(bound.iter().cloned()).collect();
                if !obj.is_valid() {
                    return self.fail(format!("Attempt to call function '{name}' on a previously freed instance."));
                }
                self.call_method(obj, name, &all)
            }
            Callable::Lambda { func, class, this, captures, bound } => {
                let all: Vec<Value> = args.iter().cloned().chain(bound.iter().cloned()).collect();
                self.call_func(func, class, this.clone(), &all, captures)
            }
            Callable::Builtin { name, bound } => {
                let all: Vec<Value> = args.iter().cloned().chain(bound.iter().cloned()).collect();
                match builtins::utility(self, name, &all) {
                    Some(r) => r,
                    None => self.fail(format!("Unknown function '{name}'.")),
                }
            }
        }
    }

    /// Build a frame for `f` (parameters, defaults and captures). Defaults
    /// are evaluated with a temporary frame, so this leaves `self.frames` as
    /// it was.
    fn make_frame(&mut self, f: &Rc<FuncDecl>, class: &Rc<ScriptClass>, this: Option<Object>, args: &[Value], captures: &HashMap<String, Value>) -> RResult<Frame> {
        if self.frames.len() > 1024 {
            return self.fail("Stack overflow (stack size: 1024). Check for infinite recursion in your script.");
        }
        self.push_frame(this, class.clone(), &f.name);
        {
            let fr = self.frames.last_mut().expect("pushed");
            for (k, v) in captures {
                fr.scopes[0].insert(k.clone(), v.clone());
            }
        }
        let required = f.params.iter().filter(|p| p.default.is_none()).count();
        if args.len() < required || (args.len() > f.params.len() && f.rest.is_none()) {
            let msg = format!("Invalid call to function '{}'. Expected {} argument(s), got {}.", if f.name.is_empty() { "<lambda>" } else { &f.name }, f.params.len(), args.len());
            self.frames.pop();
            return self.fail(msg);
        }
        for (i, p) in f.params.iter().enumerate() {
            let v = match args.get(i) {
                Some(v) => v.clone(),
                None => match &p.default {
                    Some(e) => match self.eval(e) {
                        Ok(v) => v,
                        Err(e) => {
                            self.frames.pop();
                            return Err(e);
                        }
                    },
                    None => Value::Nil,
                },
            };
            let v = convert_typed(v, p.ty.as_ref());
            let fr = self.frames.last_mut().expect("pushed");
            fr.scopes[0].insert(p.name.clone(), v);
            if let Some(t) = typed_name(p.ty.as_ref()) {
                fr.types[0].insert(p.name.clone(), t);
            }
        }
        if let Some(r) = &f.rest {
            let rest: Vec<Value> = args.iter().skip(f.params.len()).cloned().collect();
            self.frames.last_mut().expect("pushed").scopes[0].insert(r.name.clone(), Value::array(rest));
        }
        Ok(self.frames.pop().expect("pushed"))
    }

    fn call_func(&mut self, f: &Rc<FuncDecl>, class: &Rc<ScriptClass>, this: Option<Object>, args: &[Value], captures: &HashMap<String, Value>) -> RResult<Value> {
        // Functions containing `await` compile to a coroutine. Calling one
        // starts it and runs until the first suspension, like Godot's
        // GDScriptFunctionState.
        if func_has_await(f)
            && let Ok(code) = compile_async(f)
        {
            let id = self.start_task(f, class, this, args, captures, code)?;
            // A coroutine that suspends returns Nil for now; one that finishes
            // synchronously keeps Godot's normal return value.
            return Ok(self.step_task(id).unwrap_or(Value::Nil));
        }
        let saved_line = self.line;
        let frame = self.make_frame(f, class, this, args, captures)?;
        self.frames.push(frame);
        let result = self.exec_block(&f.body);
        self.frames.pop();
        self.line = saved_line;
        let v = match result? {
            Flow::Return(v) => v,
            _ => Value::Nil,
        };
        Ok(convert_typed(v, f.ret.as_ref()))
    }

    // ------------------------------------------------------------ await tasks

    fn start_task(&mut self, f: &Rc<FuncDecl>, class: &Rc<ScriptClass>, this: Option<Object>, args: &[Value], captures: &HashMap<String, Value>, code: Rc<AsyncCode>) -> RResult<u64> {
        if self.tasks.len() > 1024 {
            return self.fail("Too many suspended coroutines.");
        }
        let frame = self.make_frame(f, class, this, args, captures)?;
        let id = self.next_task_id;
        self.next_task_id += 1;
        self.tasks.insert(
            id,
            AsyncTask {
                id,
                code,
                pc: 0,
                frame,
                line: f.line,
                wait: Wait::Ready,
                await_value: Value::Nil,
                parent: None,
                loops: Vec::new(),
                done: false,
                result: Value::Nil,
                ret: f.ret.clone(),
                error: None,
            },
        );
        Ok(id)
    }

    /// Run every coroutine that is ready. Called once per game frame by the
    /// engine, after timers and `_process`.
    pub fn run_tasks(&mut self) {
        for task in self.tasks.values_mut() {
            if let Wait::Frames(n) = task.wait {
                task.wait = if n <= 1 { Wait::Ready } else { Wait::Frames(n - 1) };
            }
        }
        let mut budget = 20_000usize;
        loop {
            let ready: Vec<u64> = self.tasks.iter().filter(|(_, t)| !t.done && t.wait == Wait::Ready).map(|(&id, _)| id).collect();
            if ready.is_empty() || budget == 0 {
                break;
            }
            for id in ready {
                budget = budget.saturating_sub(1);
                let _ = self.step_task(id);
            }
        }
    }

    /// Wake coroutines waiting on a signal. Called by `emit`.
    fn wake_signal(&mut self, obj: u64, signal: &str, args: &[Value]) {
        let Some(ids) = self.signal_waiters.remove(&(obj, signal.to_string())) else { return };
        let value = match args {
            [] => Value::Nil,
            [v] => v.clone(),
            many => Value::array(many.to_vec()),
        };
        for id in ids {
            if let Some(t) = self.tasks.get_mut(&id)
                && t.wait == Wait::Signal
            {
                t.wait = Wait::Ready;
                t.await_value = value.clone();
            }
        }
    }

    fn step_task(&mut self, id: u64) -> Option<Value> {
        let mut task = self.tasks.remove(&id)?;
        if task.done {
            self.tasks.insert(id, task);
            return None;
        }
        let task_path = task.frame.class.path.clone();
        let task_func = task.frame.func.clone();
        let placeholder = Frame { scopes: vec![HashMap::default()], types: vec![HashMap::default()], this: None, class: task.frame.class.clone(), func: task.frame.func.clone() };
        let task_frame = core::mem::replace(&mut task.frame, placeholder);
        let saved_frames = core::mem::replace(&mut self.frames, vec![task_frame]);
        let saved_line = self.line;
        self.line = task.line;
        let mut steps = 0usize;
        while task.wait == Wait::Ready && !task.done {
            if steps > 20_000 {
                task.error = Some(ScriptError { message: "Coroutine ran too long without suspending.".into(), file: task_path.clone(), line: task.line, func: task_func.clone() });
                task.done = true;
                break;
            }
            steps += 1;
            if task.pc >= task.code.instrs.len() {
                task.done = true;
                break;
            }
            let instr = task.code.instrs[task.pc].clone();
            task.pc += 1;
            let r = self.run_instr(&instr, &mut task);
            if let Err(e) = r {
                task.error = Some(self.locate(e));
                task.done = true;
            }
        }
        task.line = self.line;
        task.frame = self.frames.pop().expect("task frame");
        self.frames = saved_frames;
        self.line = saved_line;
        if task.done {
            Some(self.finish_task(task))
        } else {
            self.tasks.insert(id, task);
            None
        }
    }

    fn finish_task(&mut self, task: AsyncTask) -> Value {
        let result = if task.error.is_none() { convert_typed(task.result, task.ret.as_ref()) } else { Value::Nil };
        if let Some(e) = task.error {
            self.report_pending(e);
        }
        if let Some(parent) = task.parent
            && let Some(p) = self.tasks.get_mut(&parent)
            && p.wait == Wait::Task(task.id)
        {
            p.wait = Wait::Ready;
            p.await_value = result.clone();
        }
        result
    }

    fn run_instr(&mut self, instr: &Instr, task: &mut AsyncTask) -> RResult<()> {
        match instr {
            Instr::Eval(e) => {
                self.eval(e)?;
            }
            Instr::Declare(name, ty, value) => {
                let val = match value {
                    Some(e) => self.eval(e)?,
                    None => default_for(ty.as_ref()),
                };
                let val = convert_typed(val, ty.as_ref());
                self.declare(name, val, ty.as_ref());
            }
            Instr::DeclareAwait(name, ty) => {
                let val = convert_typed(core::mem::take(&mut task.await_value), ty.as_ref());
                self.declare(name, val, ty.as_ref());
            }
            Instr::Jump(to) => task.pc = *to,
            Instr::JumpIfFalse(e, to) => {
                if !self.eval(e)?.truthy() {
                    task.pc = *to;
                }
            }
            Instr::JumpIfFalseAwait(to) => {
                if !task.await_value.truthy() {
                    task.pc = *to;
                }
            }
            Instr::JumpPop(to, pop) => {
                let keep = task.loops.len().saturating_sub(*pop);
                task.loops.truncate(keep);
                task.pc = *to;
            }
            Instr::Await(e) => {
                if self.begin_await(e, task)? {
                    // Suspended: `step_task` stops because wait is no longer Ready.
                }
            }
            Instr::Return(e) => {
                task.result = match e {
                    Some(e) => self.eval(e)?,
                    None => Value::Nil,
                };
                task.done = true;
            }
            Instr::ReturnAwait => {
                task.result = core::mem::take(&mut task.await_value);
                task.done = true;
            }
            Instr::ForInit(e) => {
                let it = self.eval(e)?;
                let items = self.iterate(&it)?;
                task.loops.push(ForIter { items, next: 0 });
            }
            Instr::ForNext { var, ty, body, end } => {
                let next = task.loops.last().and_then(|it| if it.next < it.items.len() { Some((it.items[it.next].clone(), it.next)) } else { None });
                if let Some((item, i)) = next {
                    if let Some(it) = task.loops.last_mut() {
                        it.next = i + 1;
                    }
                    let item = convert_typed(item, ty.as_ref());
                    self.declare(var, item, ty.as_ref());
                    task.pc = *body;
                } else {
                    task.loops.pop();
                    task.pc = *end;
                }
            }
            Instr::Match(value, branches) => {
                let v = self.eval(value)?;
                for b in branches {
                    {
                        let fr = self.frames.last_mut().expect("frame");
                        fr.scopes.push(HashMap::default());
                        fr.types.push(HashMap::default());
                    }
                    let mut matched = false;
                    for p in &b.patterns {
                        if self.pattern_matches(p, &v)? {
                            matched = true;
                            break;
                        }
                    }
                    if matched && let Some(g) = &b.guard {
                        matched = self.eval(g)?.truthy();
                    }
                    let r = if matched { Some(self.exec_stmts(&b.body)) } else { None };
                    {
                        let fr = self.frames.last_mut().expect("frame");
                        fr.scopes.pop();
                        fr.types.pop();
                    }
                    if let Some(r) = r {
                        match r? {
                            Flow::Return(v) => {
                                task.result = v;
                                task.done = true;
                            }
                            Flow::Break | Flow::Continue | Flow::Normal => {}
                        }
                        break;
                    }
                }
            }
            Instr::Assert(cond, message) => {
                if !self.eval(cond)?.truthy() {
                    let m = match message {
                        Some(e) => self.eval(e)?.to_string(),
                        None => "Assertion failed.".into(),
                    };
                    return self.fail(format!("Assertion failed: {m}"));
                }
            }
        }
        Ok(())
    }

    /// Resolve `await e`; returns true when the task must suspend.
    fn begin_await(&mut self, e: &Expr, task: &mut AsyncTask) -> RResult<bool> {
        if let ExprKind::Call { callee, args } = &e.kind {
            let target = self.eval(callee)?;
            let Value::Callable(c) = target else {
                // Attribute chains such as `timer.timeout` are values, not calls.
                return self.await_value(target, task);
            };
            let mut vals = Vec::with_capacity(args.len());
            for a in args {
                vals.push(self.eval(a)?);
            }
            if let Some((code, f, owner, this, captures, all)) = self.async_target(&c, &vals) {
                let id = self.start_task(&f, &owner, this, &all, &captures, code)?;
                if let Some(child) = self.tasks.get_mut(&id) {
                    child.parent = Some(task.id);
                }
                task.wait = Wait::Task(id);
                return Ok(true);
            }
            let v = self.call_callable(&c, &vals)?;
            return self.await_value(v, task);
        }
        let v = self.eval(e)?;
        self.await_value(v, task)
    }

    fn await_value(&mut self, v: Value, task: &mut AsyncTask) -> RResult<bool> {
        match v {
            Value::Signal(obj, signal) => {
                let signal = signal.to_string();
                if matches!(signal.as_str(), "process_frame" | "physics_frame" | "frame_post_draw" | "frame_pre_draw") {
                    task.wait = Wait::Frames(1);
                } else {
                    self.signal_waiters.entry((obj.id(), signal)).or_default().push(task.id);
                    task.wait = Wait::Signal;
                }
                Ok(true)
            }
            other => {
                task.await_value = other;
                Ok(false)
            }
        }
    }

    /// If a callable points at a function containing `await`, return everything
    /// needed to start it as a child coroutine.
    #[allow(clippy::type_complexity)]
    fn async_target(&mut self, c: &Callable, args: &[Value]) -> Option<(Rc<AsyncCode>, Rc<FuncDecl>, Rc<ScriptClass>, Option<Object>, HashMap<String, Value>, Vec<Value>)> {
        match c {
            Callable::Method { obj, name, bound } => {
                let all: Vec<Value> = args.iter().cloned().chain(bound.iter().cloned()).collect();
                let script = obj.script()?;
                let (f, owner) = script.find_func(name)?;
                if !func_has_await(&f) {
                    return None;
                }
                let code = compile_async(&f).ok()?;
                Some((code, f, owner, Some(obj.clone()), HashMap::default(), all))
            }
            Callable::Lambda { func, class, this, captures, bound } => {
                let all: Vec<Value> = args.iter().cloned().chain(bound.iter().cloned()).collect();
                if !func_has_await(func) {
                    return None;
                }
                let code = compile_async(func).ok()?;
                Some((code, func.clone(), class.clone(), this.clone(), captures.clone(), all))
            }
            Callable::Builtin { .. } => None,
        }
    }

    // ------------------------------------------------------------ signals

    pub fn connect(&mut self, obj: &Object, signal: &str, c: Rc<Callable>, flags: i64) {
        let mut conns = obj.0.connections.borrow_mut();
        let list = conns.entry(signal.to_string()).or_default();
        if !list.iter().any(|x| Rc::ptr_eq(&x.callable, &c) || x.callable.same_target(&c)) {
            list.push(Connection { callable: c, one_shot: flags & 4 != 0 });
        }
    }

    /// Emit a signal: call every connected callable, in connection order.
    pub fn emit(&mut self, obj: &Object, signal: &str, args: &[Value]) -> RResult<()> {
        let list = obj.0.connections.borrow().get(signal).cloned().unwrap_or_default();
        if list.iter().any(|c| c.one_shot)
            && let Some(l) = obj.0.connections.borrow_mut().get_mut(signal)
        {
            l.retain(|c| !c.one_shot);
        }
        for c in list {
            if let Callable::Method { obj: target, .. } = c.callable.as_ref()
                && !target.is_valid()
            {
                continue;
            }
            self.call_callable(&c.callable, args)?;
        }
        self.wake_signal(obj.id(), signal, args);
        Ok(())
    }

    // ------------------------------------------------------------ statements

    fn exec_block(&mut self, stmts: &[Stmt]) -> RResult<Flow> {
        {
            let fr = self.frames.last_mut().expect("in a frame");
            fr.scopes.push(HashMap::default());
            fr.types.push(HashMap::default());
        }
        let r = self.exec_stmts(stmts);
        let fr = self.frames.last_mut().expect("in a frame");
        fr.scopes.pop();
        fr.types.pop();
        r
    }

    fn exec_stmts(&mut self, stmts: &[Stmt]) -> RResult<Flow> {
        for s in stmts {
            self.line = s.line;
            match self.exec(s)? {
                Flow::Normal => {}
                other => return Ok(other),
            }
        }
        Ok(Flow::Normal)
    }

    fn declare(&mut self, name: &str, v: Value, ty: Option<&TypeRef>) {
        let fr = self.frames.last_mut().expect("in a frame");
        fr.scopes.last_mut().expect("scope").insert(name.to_string(), v);
        if let Some(t) = typed_name(ty) {
            fr.types.last_mut().expect("scope").insert(name.to_string(), t);
        }
    }

    fn exec(&mut self, s: &Stmt) -> RResult<Flow> {
        match &s.kind {
            StmtKind::Expr(e) => {
                self.eval(e)?;
            }
            StmtKind::Var(v) => {
                let val = match &v.value {
                    Some(e) => self.eval(e)?,
                    None => default_for(v.ty.as_ref()),
                };
                let val = convert_typed(val, v.ty.as_ref());
                self.declare(&v.name, val, v.ty.as_ref());
            }
            StmtKind::Const(c) => {
                let val = self.eval(&c.value)?;
                self.declare(&c.name, val, None);
            }
            StmtKind::If { cond, then, elifs, else_ } => {
                if self.eval(cond)?.truthy() {
                    return self.exec_block(then);
                }
                for (c, body) in elifs {
                    if self.eval(c)?.truthy() {
                        return self.exec_block(body);
                    }
                }
                if let Some(b) = else_ {
                    return self.exec_block(b);
                }
            }
            StmtKind::While { cond, body } => {
                while self.eval(cond)?.truthy() {
                    match self.exec_block(body)? {
                        Flow::Break => break,
                        Flow::Return(v) => return Ok(Flow::Return(v)),
                        _ => {}
                    }
                }
            }
            StmtKind::For { var, ty, iter, body } => {
                let it = self.eval(iter)?;
                let items = self.iterate(&it)?;
                for item in items {
                    {
                        let fr = self.frames.last_mut().expect("frame");
                        fr.scopes.push(HashMap::default());
                        fr.types.push(HashMap::default());
                    }
                    self.declare(var, convert_typed(item, ty.as_ref()), ty.as_ref());
                    let r = self.exec_stmts(body);
                    {
                        let fr = self.frames.last_mut().expect("frame");
                        fr.scopes.pop();
                        fr.types.pop();
                    }
                    match r? {
                        Flow::Break => break,
                        Flow::Return(v) => return Ok(Flow::Return(v)),
                        _ => {}
                    }
                }
            }
            StmtKind::Match { value, branches } => {
                let v = self.eval(value)?;
                for b in branches {
                    {
                        let fr = self.frames.last_mut().expect("frame");
                        fr.scopes.push(HashMap::default());
                        fr.types.push(HashMap::default());
                    }
                    let mut matched = false;
                    for p in &b.patterns {
                        if self.pattern_matches(p, &v)? {
                            matched = true;
                            break;
                        }
                    }
                    if matched && let Some(g) = &b.guard {
                        matched = self.eval(g)?.truthy();
                    }
                    let r = if matched { Some(self.exec_stmts(&b.body)) } else { None };
                    {
                        let fr = self.frames.last_mut().expect("frame");
                        fr.scopes.pop();
                        fr.types.pop();
                    }
                    // A matched branch ends the match (Godot 4 has no fallthrough);
                    // `continue`/`break` inside it apply to the enclosing loop.
                    if let Some(r) = r {
                        return r;
                    }
                }
            }
            StmtKind::Return(v) => {
                let val = match v {
                    Some(e) => self.eval(e)?,
                    None => Value::Nil,
                };
                return Ok(Flow::Return(val));
            }
            StmtKind::Break => return Ok(Flow::Break),
            StmtKind::Continue => return Ok(Flow::Continue),
            StmtKind::Pass | StmtKind::Breakpoint => {}
            StmtKind::Assert { cond, message } => {
                if !self.eval(cond)?.truthy() {
                    let m = match message {
                        Some(e) => self.eval(e)?.to_string(),
                        None => "Assertion failed.".into(),
                    };
                    return self.fail(format!("Assertion failed: {m}"));
                }
            }
        }
        Ok(Flow::Normal)
    }

    fn iterate(&mut self, v: &Value) -> RResult<Vec<Value>> {
        Ok(match v {
            Value::Array(a) => a.borrow().clone(),
            Value::Dictionary(d) => d.borrow().keys(),
            Value::Int(n) => (0..*n).map(Value::Int).collect(),
            Value::Float(f) => (0..f.ceil() as i64).map(Value::Int).collect(),
            Value::String(s) => s.chars().map(|c| Value::str(&c.to_string())).collect(),
            Value::Vector2i(a, b) => (*a as i64..*b as i64).map(Value::Int).collect(),
            Value::Vector3i(a, b, c) => {
                let step = (*c).max(1) as usize;
                (*a as i64..*b as i64).step_by(step).map(Value::Int).collect()
            }
            other => return self.fail(format!("Unable to iterate on object of type '{}'.", other.type_name())),
        })
    }

    fn pattern_matches(&mut self, p: &Pattern, v: &Value) -> RResult<bool> {
        Ok(match p {
            Pattern::Wildcard => true,
            Pattern::Bind(n) => {
                self.declare(n, v.clone(), None);
                true
            }
            Pattern::Expr(e) => {
                let pv = self.eval(e)?;
                // Godot's match is type-strict: 1 does not match 1.0.
                pv.type_id() == v.type_id() && equals(&pv, v) || (pv.is_string_like() && v.is_string_like() && equals(&pv, v))
            }
            Pattern::Array(items, rest) => {
                let Value::Array(a) = v else { return Ok(false) };
                let a = a.borrow().clone();
                if (!rest && a.len() != items.len()) || (*rest && a.len() < items.len()) {
                    return Ok(false);
                }
                for (pi, item) in items.iter().zip(a.iter()) {
                    if !self.pattern_matches(pi, item)? {
                        return Ok(false);
                    }
                }
                true
            }
            Pattern::Dict(items, rest) => {
                let Value::Dictionary(d) = v else { return Ok(false) };
                let d = d.borrow().clone();
                if !rest && d.len() != items.len() {
                    return Ok(false);
                }
                for (k, pv) in items {
                    let key = self.eval(k)?;
                    let Some(val) = d.get(&key) else { return Ok(false) };
                    if let Some(pv) = pv
                        && !self.pattern_matches(pv, val)?
                    {
                        return Ok(false);
                    }
                }
                true
            }
        })
    }

    // ------------------------------------------------------------ expressions

    fn this(&self) -> Option<Object> {
        self.frames.last().and_then(|f| f.this.clone())
    }

    fn class(&self) -> Rc<ScriptClass> {
        self.frames.last().expect("in a frame").class.clone()
    }

    pub fn eval(&mut self, e: &Expr) -> RResult<Value> {
        if e.line != 0 {
            self.line = e.line;
        }
        match &e.kind {
            ExprKind::Literal(l) => Ok(lit(l)),
            ExprKind::Ident(n) => self.lookup(n),
            ExprKind::SelfRef => Ok(self.this().map(Value::Object).unwrap_or(Value::Nil)),
            ExprKind::Super(None) => Ok(self.this().map(Value::Object).unwrap_or(Value::Nil)),
            ExprKind::Super(Some(m)) => {
                // `super.method` without a call: a callable to the base version.
                let this = self.this();
                let base = match &self.class().base {
                    Base::Script(b) => b.clone(),
                    Base::Native(_) => return Ok(Value::Nil),
                };
                match (base.find_func(m), this) {
                    (Some((f, owner)), this) => Ok(Value::Callable(Rc::new(Callable::Lambda { func: f, class: owner, this, captures: HashMap::default(), bound: Vec::new() }))),
                    _ => Ok(Value::Nil),
                }
            }
            ExprKind::Array(items) => {
                let mut v = Vec::with_capacity(items.len());
                for i in items {
                    v.push(self.eval(i)?);
                }
                Ok(Value::array(v))
            }
            ExprKind::Dict(pairs) => {
                let mut d = Dict::new();
                for (k, v) in pairs {
                    let key = self.eval(k)?;
                    let val = self.eval(v)?;
                    d.set(key, val);
                }
                Ok(Value::dict(d))
            }
            ExprKind::Unary(op, x) => {
                let v = self.eval(x)?;
                Ok(match op {
                    UnaryOp::Neg => builtins::negate(&v),
                    UnaryOp::Pos => v,
                    UnaryOp::Not => Value::Bool(!v.truthy()),
                    UnaryOp::BitNot => match v {
                        Value::Int(i) => Value::Int(!i),
                        o => return self.fail(format!("Invalid operand of type '{}' for unary operator '~'.", o.type_name())),
                    },
                })
            }
            ExprKind::Binary(BinOp::And, a, b) => {
                let l = self.eval(a)?;
                Ok(Value::Bool(l.truthy() && self.eval(b)?.truthy()))
            }
            ExprKind::Binary(BinOp::Or, a, b) => {
                let l = self.eval(a)?;
                Ok(Value::Bool(l.truthy() || self.eval(b)?.truthy()))
            }
            ExprKind::Binary(op, a, b) => {
                let l = self.eval(a)?;
                let r = self.eval(b)?;
                if matches!(op, BinOp::In | BinOp::NotIn)
                    && let Value::Object(o) = &r
                {
                    // `"prop" in object`
                    let name = l.to_string();
                    let has = o.0.vars.borrow().contains_key(&name) || self.with_host(|h, _| h.get_property(o, &name)).is_some();
                    return Ok(Value::Bool(has == (*op == BinOp::In)));
                }
                builtins::binary(*op, &l, &r).or_else(|m| self.fail(m))
            }
            ExprKind::Ternary { cond, then, else_ } => {
                if self.eval(cond)?.truthy() {
                    self.eval(then)
                } else {
                    self.eval(else_)
                }
            }
            ExprKind::Call { callee, args } => self.eval_call(callee, args),
            ExprKind::Attr { base, name } => {
                let b = self.eval(base)?;
                self.get_attr(&b, name)
            }
            ExprKind::Index { base, index } => {
                let b = self.eval(base)?;
                let i = self.eval(index)?;
                builtins::index_get(&b, &i).or_else(|m| self.fail(m))
            }
            ExprKind::Is { value, ty, negated } => {
                let v = self.eval(value)?;
                let r = self.is_type(&v, ty);
                Ok(Value::Bool(r != *negated))
            }
            ExprKind::As { value, ty } => {
                let v = self.eval(value)?;
                Ok(self.cast(v, ty))
            }
            ExprKind::Await(x) => {
                let v = self.eval(x)?;
                if matches!(v, Value::Signal(..)) && !self.warned_await {
                    self.warned_await = true;
                    let msg = "await on a signal does not suspend yet in NekoScript; the function continues immediately.";
                    self.with_host(|h, _| h.print_error(msg));
                }
                Ok(match v {
                    Value::Signal(..) => Value::Nil,
                    other => other,
                })
            }
            ExprKind::GetNode { path, unique } => {
                let Some(this) = self.this() else { return self.fail("Cannot use $ outside of a node script.") };
                let p = if *unique { format!("%{path}") } else { path.clone() };
                match self.with_host(|h, _| h.get_node(&this, &p)) {
                    Some(o) => Ok(Value::Object(o)),
                    None => {
                        let msg = format!("Node not found: \"{p}\" (relative to \"{}\").", this.describe());
                        self.with_host(|h, _| h.print_error(&msg));
                        Ok(Value::Nil)
                    }
                }
            }
            ExprKind::Preload(p) => self.load_resource(p),
            ExprKind::Lambda(f) => {
                // Capture locals by value, like Godot.
                let captures = self.frames.last().map(|fr| {
                    let mut c = HashMap::default();
                    for s in &fr.scopes {
                        for (k, v) in s {
                            c.insert(k.clone(), v.clone());
                        }
                    }
                    c
                });
                let func = Rc::new((**f).clone());
                Ok(Value::Callable(Rc::new(Callable::Lambda { func, class: self.class(), this: self.this(), captures: captures.unwrap_or_default(), bound: Vec::new() })))
            }
            ExprKind::Assign { op, target, value } => {
                let v = self.eval(value)?;
                let new = match op {
                    None => v,
                    Some(o) => {
                        let cur = self.eval(target)?;
                        builtins::binary(*o, &cur, &v).or_else(|m| self.fail(m))?
                    }
                };
                self.assign(target, new)?;
                Ok(Value::Nil)
            }
        }
    }

    pub fn load_resource(&mut self, path: &str) -> RResult<Value> {
        if path.ends_with(".gd") {
            let c = self.load_script(path).map_err(|e| self.locate(e))?;
            return Ok(Value::Object(Object::new(ObjKind::Class(c), "GDScript", None)));
        }
        let loaded = self.with_host(|h, it| h.load(it, path));
        // `load()` of a .tres queues its script; attach it now that the host is back.
        self.flush_pending();
        match loaded {
            Some(v) => Ok(v),
            None => {
                let msg = format!("Resource file not found: {path}");
                self.with_host(|h, _| h.print_error(&msg));
                Ok(Value::Nil)
            }
        }
    }

    fn lookup(&mut self, n: &str) -> RResult<Value> {
        // Locals, innermost first.
        if let Some(fr) = self.frames.last() {
            for s in fr.scopes.iter().rev() {
                if let Some(v) = s.get(n) {
                    return Ok(v.clone());
                }
            }
        }
        // Members of self.
        if let Some(this) = self.this()
            && let Some(v) = self.member_get(&this, n, true)?
        {
            return Ok(v);
        }
        // Constants, enums, inner classes, static members, functions of the class chain.
        let class = self.class();
        if let Some(v) = self.class_member(&class, n)? {
            return Ok(v);
        }
        if let Some(v) = self.globals.get(n) {
            return Ok(v.clone());
        }
        if let Some(v) = builtins::global_constant(n) {
            return Ok(v);
        }
        if builtins::is_utility(n) {
            return Ok(Value::Callable(Rc::new(Callable::Builtin { name: Rc::from(n), bound: Vec::new() })));
        }
        if let Some(v) = self.with_host(|h, it| h.global(it, n)) {
            return Ok(v);
        }
        if builtins::is_builtin_type(n) || n.chars().next().is_some_and(char::is_uppercase) {
            return Ok(Value::Object(Object::new(ObjKind::NativeClass(n.to_string()), n, None)));
        }
        self.fail(format!("Identifier \"{n}\" not declared in the current scope."))
    }

    /// Constants, enums, inner classes, statics and methods by name, searching
    /// the class, its bases and its outer classes.
    fn class_member(&mut self, class: &Rc<ScriptClass>, n: &str) -> RResult<Option<Value>> {
        let mut c = Some(class.clone());
        while let Some(cl) = c {
            if let Some(v) = cl.const_cache.borrow().get(n) {
                return Ok(Some(v.clone()));
            }
            if let Some(k) = cl.consts.get(n).cloned() {
                self.push_frame(None, cl.clone(), "<const>");
                let r = self.eval(&k.value);
                self.frames.pop();
                let v = r?;
                cl.const_cache.borrow_mut().insert(n.to_string(), v.clone());
                return Ok(Some(v));
            }
            if let Some(v) = cl.enums.get(n) {
                return Ok(Some(v.clone()));
            }
            if let Some(ic) = cl.inner.get(n) {
                return Ok(Some(Value::Object(Object::new(ObjKind::Class(ic.clone()), "GDScript", None))));
            }
            self.ensure_statics(&cl)?;
            if let Some(v) = cl.statics.borrow().get(n) {
                return Ok(Some(v.clone()));
            }
            if let Some(f) = cl.funcs.get(n) {
                let this = self.this();
                return Ok(Some(match (f.is_static, this) {
                    (false, Some(o)) => Value::Callable(Rc::new(Callable::Method { obj: o, name: Rc::from(n), bound: Vec::new() })),
                    _ => Value::Callable(Rc::new(Callable::Lambda { func: f.clone(), class: cl.clone(), this: None, captures: HashMap::default(), bound: Vec::new() })),
                }));
            }
            c = match &cl.base {
                Base::Script(b) => Some(b.clone()),
                Base::Native(_) => cl.outer.borrow().upgrade(),
            };
        }
        Ok(None)
    }

    /// A member of an object: script vars (through getters), signals,
    /// methods, then engine properties.
    fn member_get(&mut self, obj: &Object, n: &str, implicit: bool) -> RResult<Option<Value>> {
        if let ObjKind::Class(c) = &obj.0.kind {
            let c = c.clone();
            return self.class_member(&c, n);
        }
        if let Some(class) = obj.script() {
            if let Some(var) = class.find_var(n) {
                if var.is_static {
                    return self.class_member(&class, n);
                }
                let inside = self.in_accessor.iter().any(|(id, m)| *id == obj.id() && m == n);
                if let Some(getter) = var.getter.as_ref().filter(|_| !inside) {
                    return self.run_getter(obj, &class, n, getter).map(Some);
                }
                return Ok(obj.0.vars.borrow().get(n).cloned());
            }
            if class.has_signal(n) {
                return Ok(Some(Value::Signal(obj.clone(), Rc::from(n))));
            }
            if !implicit && class.find_func(n).is_some() {
                return Ok(Some(Value::Callable(Rc::new(Callable::Method { obj: obj.clone(), name: Rc::from(n), bound: Vec::new() }))));
            }
            if !implicit
                && let Some(v) = self.class_member(&class, n)?
            {
                return Ok(Some(v));
            }
        }
        if let Some(v) = obj.0.vars.borrow().get(n) {
            return Ok(Some(v.clone()));
        }
        // Native signals are identifiers too: `resized.connect(...)`, `timeout`.
        if (obj.0.native.get() != 0 || obj.script().is_some())
            && self.with_host(|h, _| h.has_signal(obj, n))
        {
            return Ok(Some(Value::Signal(obj.clone(), Rc::from(n))));
        }
        if (obj.0.native.get() != 0 || obj.script().is_some() || !implicit)
            && let Some(v) = self.with_host(|h, _| h.get_property(obj, n))
        {
            return Ok(Some(v));
        }
        Ok(None)
    }

    fn run_getter(&mut self, obj: &Object, class: &Rc<ScriptClass>, n: &str, getter: &Accessor) -> RResult<Value> {
        self.in_accessor.push((obj.id(), n.to_string()));
        let r = match getter {
            Accessor::Method(m) => self.call_method(obj, m, &[]),
            Accessor::Inline { body, .. } => {
                self.push_frame(Some(obj.clone()), class.clone(), &format!("@{n}_getter"));
                let r = self.exec_block(body);
                self.frames.pop();
                r.map(|f| if let Flow::Return(v) = f { v } else { Value::Nil })
            }
        };
        self.in_accessor.pop();
        r
    }

    fn get_attr(&mut self, b: &Value, name: &str) -> RResult<Value> {
        match b {
            Value::Object(o) => {
                if !o.is_valid() {
                    return self.fail(format!("Invalid access to property or key '{name}' on a base object of type 'previously freed'."));
                }
                if let Some(v) = self.member_get(o, name, false)? {
                    return Ok(v);
                }
                if let ObjKind::NativeClass(cls) = &o.0.kind {
                    // `Vector2.ZERO`, `Color.RED`, `Input.is_action_pressed`...
                    if let Some(v) = builtins::type_constant(cls, name) {
                        return Ok(v);
                    }
                }
                if self.host.as_ref().is_some_and(|h| h.has_signal(o, name)) {
                    return Ok(Value::Signal(o.clone(), Rc::from(name)));
                }
                // A method used as a value: `obj.method` -> Callable.
                Ok(Value::Callable(Rc::new(Callable::Method { obj: o.clone(), name: Rc::from(name), bound: Vec::new() })))
            }
            other => builtins::attr_get(other, name).or_else(|m| self.fail(m)),
        }
    }

    fn eval_call(&mut self, callee: &Expr, args: &[Expr]) -> RResult<Value> {
        let mut argv = Vec::with_capacity(args.len());
        for a in args {
            argv.push(self.eval(a)?);
        }
        match &callee.kind {
            ExprKind::Ident(n) => {
                // Local callables (lambdas in variables) shadow methods.
                if let Some(fr) = self.frames.last()
                    && let Some(v) = fr.scopes.iter().rev().find_map(|s| s.get(n))
                {
                    let v = v.clone();
                    return match v {
                        Value::Callable(c) => self.call_callable(&c, &argv),
                        other => self.fail(format!("'{n}' is a {} and cannot be called.", other.type_name())),
                    };
                }
                // Methods of self and of the class (static). Use the object's
                // script, not the class that declared the running base method:
                // `build()` inside `Room.setup` must run `Landing.build`.
                let class = self.this().and_then(|t| t.script()).unwrap_or_else(|| self.class());
                if let Some((f, owner)) = class.find_func(n) {
                    let this = if f.is_static { None } else { self.this() };
                    return self.call_func(&f, &owner, this, &argv, &HashMap::default());
                }
                // Outer class static functions.
                let mut outer = class.outer.borrow().upgrade();
                while let Some(oc) = outer {
                    if let Some((f, owner)) = oc.find_func(n) {
                        return self.call_func(&f, &owner, None, &argv, &HashMap::default());
                    }
                    outer = oc.outer.borrow().upgrade();
                }
                if let Some(r) = builtins::utility(self, n, &argv) {
                    return r;
                }
                if let Some(r) = builtins::construct(n, &argv) {
                    return r.or_else(|m| self.fail(m));
                }
                // Engine methods of self (`get_node`, `add_child`, `queue_free`...).
                if let Some(this) = self.this() {
                    return self.call_method(&this, n, &argv);
                }
                self.fail(format!("Function \"{n}()\" not found in base self."))
            }
            ExprKind::Super(method) => {
                let base = match &self.class().base {
                    Base::Script(b) => Some(b.clone()),
                    Base::Native(_) => None,
                };
                let name = method.clone().unwrap_or_else(|| self.frames.last().map(|f| f.func.clone()).unwrap_or_default());
                let this = self.this();
                if let Some(b) = base
                    && let Some((f, owner)) = b.find_func(&name)
                {
                    return self.call_func(&f, &owner, this, &argv, &HashMap::default());
                }
                // Native base method.
                match this {
                    Some(t) => match self.with_host(|h, it| h.call_method(it, &t, &name, &argv)) {
                        Some(r) => r,
                        None => Ok(Value::Nil),
                    },
                    None => Ok(Value::Nil),
                }
            }
            ExprKind::Attr { base, name } => {
                let b = self.eval(base)?;
                self.call_on(&b, name, &argv)
            }
            _ => {
                let c = self.eval(callee)?;
                match c {
                    Value::Callable(c) => self.call_callable(&c, &argv),
                    other => self.fail(format!("Value of type '{}' is not callable.", other.type_name())),
                }
            }
        }
    }

    /// `value.method(args)`.
    pub fn call_on(&mut self, b: &Value, name: &str, argv: &[Value]) -> RResult<Value> {
        match b {
            Value::Object(o) => {
                if !o.is_valid() {
                    return self.fail(format!("Attempt to call function '{name}' in base 'previously freed' on a null instance."));
                }
                if let ObjKind::NativeClass(cls) = &o.0.kind
                    && name != "new"
                {
                    let cls = cls.clone();
                    if let Some(r) = builtins::static_call(self, &cls, name, argv) {
                        return r;
                    }
                }
                // A member holding a callable (`obj.callback.call()` handled below).
                self.call_method(o, name, argv)
            }
            Value::Nil => self.fail(format!("Attempt to call function '{name}' in base 'null instance' on a null instance.")),
            Value::Callable(c) => match name {
                "call" => self.call_callable(c, argv),
                "callv" => {
                    let args = match argv.first() {
                        Some(Value::Array(a)) => a.borrow().clone(),
                        _ => Vec::new(),
                    };
                    self.call_callable(c, &args)
                }
                // Deferred calls to freed objects are dropped in Godot; without
                // a deferred queue, drop them all rather than erroring.
                "call_deferred" => Ok(Value::Nil),
                "bind" => Ok(Value::Callable(Rc::new(c.with_bound(argv)))),
                "is_valid" => Ok(Value::Bool(match c.as_ref() {
                    Callable::Method { obj, .. } => obj.is_valid(),
                    _ => true,
                })),
                "get_method" => Ok(match c.as_ref() {
                    Callable::Method { name, .. } => Value::StringName(name.clone()),
                    _ => Value::Nil,
                }),
                "get_object" => Ok(match c.as_ref() {
                    Callable::Method { obj, .. } => Value::Object(obj.clone()),
                    _ => Value::Nil,
                }),
                _ => self.fail(format!("Invalid call. Nonexistent function '{name}' in base 'Callable'.")),
            },
            Value::Signal(o, sig) => {
                let (o, sig) = (o.clone(), sig.clone());
                match name {
                    "connect" => {
                        let Some(Value::Callable(c)) = argv.first() else { return self.fail("Signal.connect() needs a Callable.") };
                        let flags = argv.get(1).and_then(Value::as_i64).unwrap_or(0);
                        self.connect(&o, &sig, c.clone(), flags);
                        Ok(Value::Int(0))
                    }
                    "disconnect" => {
                        if let Some(Value::Callable(c)) = argv.first()
                            && let Some(l) = o.0.connections.borrow_mut().get_mut(sig.as_ref())
                        {
                            l.retain(|x| !(Rc::ptr_eq(&x.callable, c) || x.callable.same_target(c)));
                        }
                        Ok(Value::Nil)
                    }
                    "is_connected" => {
                        let r = match argv.first() {
                            Some(Value::Callable(c)) => o.0.connections.borrow().get(sig.as_ref()).is_some_and(|l| l.iter().any(|x| Rc::ptr_eq(&x.callable, c) || x.callable.same_target(c))),
                            _ => false,
                        };
                        Ok(Value::Bool(r))
                    }
                    "emit" => {
                        self.emit(&o, &sig, argv)?;
                        Ok(Value::Nil)
                    }
                    "get_name" => Ok(Value::StringName(sig.clone())),
                    "get_connections" => {
                        let list = o.0.connections.borrow().get(sig.as_ref()).cloned().unwrap_or_default();
                        Ok(Value::array(list.into_iter().map(|c| {
                            let mut d = Dict::new();
                            d.set(Value::str("callable"), Value::Callable(c.callable));
                            Value::dict(d)
                        }).collect()))
                    }
                    _ => self.fail(format!("Invalid call. Nonexistent function '{name}' in base 'Signal'.")),
                }
            }
            other => builtins::method(self, other, name, argv),
        }
    }

    fn assign(&mut self, target: &Expr, v: Value) -> RResult<()> {
        match &target.kind {
            ExprKind::Ident(n) => {
                // Local?
                if let Some(fr) = self.frames.last_mut() {
                    for (i, s) in fr.scopes.iter_mut().enumerate().rev() {
                        if s.contains_key(n) {
                            let ty = fr.types[i].get(n).cloned();
                            let v = match ty.as_deref() {
                                Some("int") => builtins::to_int(&v),
                                Some("float") => builtins::to_float(&v),
                                _ => v,
                            };
                            s.insert(n.clone(), v);
                            return Ok(());
                        }
                    }
                }
                if let Some(this) = self.this() {
                    return self.member_set(&this, n, v);
                }
                // Static vars of the class.
                let class = self.class();
                let mut c = Some(class);
                while let Some(cl) = c {
                    if cl.statics.borrow().contains_key(n) {
                        cl.statics.borrow_mut().insert(n.clone(), v);
                        return Ok(());
                    }
                    c = match &cl.base {
                        Base::Script(b) => Some(b.clone()),
                        Base::Native(_) => None,
                    };
                }
                self.fail(format!("Identifier \"{n}\" not declared in the current scope."))
            }
            ExprKind::Attr { base, name } => {
                let b = self.eval(base)?;
                match b {
                    Value::Object(o) => self.member_set(&o, name, v),
                    other => {
                        // Value types: `pos.x = 1` writes back through the base expression.
                        let updated = builtins::attr_set(&other, name, v).or_else(|m| self.fail(m))?;
                        self.assign(base, updated)
                    }
                }
            }
            ExprKind::Index { base, index } => {
                let b = self.eval(base)?;
                let i = self.eval(index)?;
                match &b {
                    Value::Array(a) => {
                        let len = a.borrow().len() as i64;
                        let Some(mut idx) = i.as_i64() else { return self.fail("Array index must be an int.") };
                        if idx < 0 {
                            idx += len;
                        }
                        if idx < 0 || idx >= len {
                            return self.fail(format!("Out of bounds set index '{}' (on base: 'Array')", i));
                        }
                        a.borrow_mut()[idx as usize] = v;
                        Ok(())
                    }
                    Value::Dictionary(d) => {
                        d.borrow_mut().set(i, v);
                        Ok(())
                    }
                    Value::Object(o) => {
                        let name = i.to_string();
                        self.member_set(o, &name, v)
                    }
                    other => {
                        let updated = builtins::index_set(other, &i, v).or_else(|m| self.fail(m))?;
                        self.assign(base, updated)
                    }
                }
            }
            _ => self.fail("Cannot assign to this expression."),
        }
    }

    pub fn member_set(&mut self, obj: &Object, n: &str, v: Value) -> RResult<()> {
        if let Some(class) = obj.script()
            && let Some(var) = class.find_var(n)
        {
            let v = convert_typed(v, var.ty.as_ref());
            if var.is_static {
                let mut c = Some(class.clone());
                while let Some(cl) = c {
                    if cl.var_index.contains_key(n) {
                        cl.statics.borrow_mut().insert(n.to_string(), v);
                        return Ok(());
                    }
                    c = match &cl.base {
                        Base::Script(b) => Some(b.clone()),
                        Base::Native(_) => None,
                    };
                }
            }
            let inside = self.in_accessor.iter().any(|(id, m)| *id == obj.id() && m == n);
            if let Some(setter) = var.setter.as_ref().filter(|_| !inside) {
                self.in_accessor.push((obj.id(), n.to_string()));
                let r = match setter {
                    Accessor::Method(m) => self.call_method(obj, m, &[v]).map(|_| ()),
                    Accessor::Inline { param, body } => {
                        self.push_frame(Some(obj.clone()), class.clone(), &format!("@{n}_setter"));
                        if let Some(p) = param {
                            self.declare(p, v, None);
                        }
                        let r = self.exec_block(body);
                        self.frames.pop();
                        r.map(|_| ())
                    }
                };
                self.in_accessor.pop();
                return r;
            }
            obj.0.vars.borrow_mut().insert(n.to_string(), v);
            return Ok(());
        }
        if self.with_host(|h, _| h.set_property(obj, n, &v)) {
            return Ok(());
        }
        if obj.0.vars.borrow().contains_key(n) || obj.0.native.get() == 0 {
            // Plain objects accept new members only through set(); keep it simple.
            obj.0.vars.borrow_mut().insert(n.to_string(), v);
            return Ok(());
        }
        self.fail(format!("Invalid assignment of property or key '{n}' with value of type '{}' on a base object of type '{}'.", v.type_name(), obj.0.native_class))
    }

    // ------------------------------------------------------------ types

    pub fn is_type(&mut self, v: &Value, ty: &TypeRef) -> bool {
        let name = ty.name();
        if let Some(r) = builtins::is_builtin_instance(v, &name) {
            return r;
        }
        let Value::Object(o) = v else { return false };
        if !o.is_valid() {
            return false;
        }
        if let Some(c) = self.resolve_class(ty)
            && let Some(s) = o.script()
        {
            return s.inherits_script(&c);
        }
        if self.resolve_class(ty).is_some() {
            return false;
        }
        if name == "Object" {
            return true;
        }
        if o.script().is_some_and(|s| s.native_base() == name) {
            return true;
        }
        self.with_host(|h, _| h.is_class(o, &name))
    }

    fn resolve_class(&mut self, ty: &TypeRef) -> Option<Rc<ScriptClass>> {
        let mut c = self.class_by_name(&ty.path[0]).or_else(|| {
            let cur = self.frames.last()?.class.clone();
            let mut k = Some(cur);
            while let Some(cl) = k {
                if let Some(ic) = cl.inner.get(&ty.path[0]) {
                    return Some(ic.clone());
                }
                k = cl.outer.borrow().upgrade();
            }
            None
        })?;
        for p in &ty.path[1..] {
            c = c.inner.get(p)?.clone();
        }
        Some(c)
    }

    fn cast(&mut self, v: Value, ty: &TypeRef) -> Value {
        let name = ty.name();
        match name.as_str() {
            "int" => builtins::to_int(&v),
            "float" => builtins::to_float(&v),
            "String" => Value::str(&v.to_string()),
            "StringName" => Value::StringName(Rc::from(v.to_string().as_str())),
            "bool" => Value::Bool(v.truthy()),
            _ => {
                if matches!(v, Value::Nil) || self.is_type(&v, ty) || !matches!(v, Value::Object(_)) {
                    v
                } else {
                    Value::Nil
                }
            }
        }
    }

    /// The top-level frame for running code outside methods (tests, tools).
    pub fn eval_in(&mut self, obj: Option<&Object>, class: &Rc<ScriptClass>, e: &Expr) -> RResult<Value> {
        self.push_frame(obj.cloned(), class.clone(), "<eval>");
        let r = self.eval(e);
        self.frames.pop();
        r
    }
}

pub(crate) fn lit(l: &Literal) -> Value {
    match l {
        Literal::Null => Value::Nil,
        Literal::Bool(b) => Value::Bool(*b),
        Literal::Int(i) => Value::Int(*i),
        Literal::Float(f) => Value::Float(*f),
        Literal::Str(s) => Value::str(s),
        Literal::StringName(s) => Value::StringName(Rc::from(s.as_str())),
        Literal::NodePath(s) => Value::NodePath(Rc::from(s.as_str())),
    }
}

fn typed_name(ty: Option<&TypeRef>) -> Option<String> {
    let t = ty?;
    matches!(t.name().as_str(), "int" | "float").then(|| t.name())
}

/// Typed variables convert ints and floats, like GDScript's typed assignment.
pub(crate) fn convert_typed(v: Value, ty: Option<&TypeRef>) -> Value {
    match (ty.map(TypeRef::name).as_deref(), &v) {
        (Some("float"), Value::Int(i)) => Value::Float(*i as f64),
        (Some("int"), Value::Float(f)) => Value::Int(*f as i64),
        (Some("String"), Value::StringName(s)) => Value::String(s.clone()),
        (Some("StringName"), Value::String(s)) => Value::StringName(s.clone()),
        _ => v,
    }
}

/// Default value of an uninitialized typed variable.
pub(crate) fn default_for(ty: Option<&TypeRef>) -> Value {
    let Some(t) = ty else { return Value::Nil };
    match t.name().as_str() {
        "int" => Value::Int(0),
        "float" => Value::Float(0.0),
        "bool" => Value::Bool(false),
        "String" => Value::str(""),
        "StringName" => Value::StringName(Rc::from("")),
        "NodePath" => Value::NodePath(Rc::from("")),
        "Vector2" => Value::Vector2(0.0, 0.0),
        "Vector2i" => Value::Vector2i(0, 0),
        "Vector3" => Value::Vector3(0.0, 0.0, 0.0),
        "Vector3i" => Value::Vector3i(0, 0, 0),
        "Vector4" => Value::Vector4(0.0, 0.0, 0.0, 0.0),
        "Rect2" => Value::Rect2(0.0, 0.0, 0.0, 0.0),
        "Color" => Value::Color(0.0, 0.0, 0.0, 1.0),
        "Array" => Value::array(Vec::new()),
        "Dictionary" => Value::dict(Dict::new()),
        n if n.starts_with("Packed") && n.ends_with("Array") => Value::array(Vec::new()),
        _ => Value::Nil,
    }
}
