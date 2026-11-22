//! Parser, after Godot's `GDScriptParser` (modules/gdscript/gdscript_parser.cpp):
//! a recursive-descent parser for declarations and statements and a Pratt
//! parser for expressions with Godot's precedence table.

#[allow(unused_imports)]
use neko_base::prelude::*;

use crate::ast::*;
use crate::lexer::{LexError, Tok, Token, tokenize};

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("line {line}:{col}: {message}")]
pub struct ParseError {
    pub line: u32,
    pub col: u32,
    pub message: String,
}

impl From<LexError> for ParseError {
    fn from(e: LexError) -> Self {
        Self { line: e.line, col: e.col, message: e.message }
    }
}

type PResult<T> = Result<T, ParseError>;

/// Parse a whole `.gd` file.
pub fn parse(src: &str) -> PResult<ClassDecl> {
    let toks = tokenize(src)?;
    let mut p = Parser { toks, pos: 0, multiline: Vec::new() };
    p.script()
}

/// Godot's precedence levels (`GDScriptParser::Precedence`), lowest first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Prec {
    Cast,
    Ternary,
    LogicOr,
    LogicAnd,
    LogicNot,
    ContentTest,
    Comparison,
    BitOr,
    BitXor,
    BitAnd,
    BitShift,
    AddSub,
    Factor,
    Sign,
    BitNot,
    Power,
    TypeTest,
    Await,
    Call,
}

impl Prec {
    fn next(self) -> Self {
        use Prec::*;
        match self {
            Cast => Ternary,
            Ternary => LogicOr,
            LogicOr => LogicAnd,
            LogicAnd => LogicNot,
            LogicNot => ContentTest,
            ContentTest => Comparison,
            Comparison => BitOr,
            BitOr => BitXor,
            BitXor => BitAnd,
            BitAnd => BitShift,
            BitShift => AddSub,
            AddSub => Factor,
            Factor => Sign,
            Sign => BitNot,
            BitNot => Power,
            Power => TypeTest,
            TypeTest => Await,
            Await | Call => Call,
        }
    }
}

struct Parser {
    toks: Vec<Token>,
    pos: usize,
    /// Godot's "multiline" stack: inside (), [] and {} line breaks are
    /// skipped (true); inside a lambda body within brackets they matter (false).
    multiline: Vec<bool>,
}

impl Parser {
    // ------------------------------------------------------------ tokens

    fn skipping(&self) -> bool {
        self.multiline.last().copied().unwrap_or(false)
    }

    fn skip_breaks(&mut self) {
        if self.skipping() {
            while matches!(self.toks[self.pos].tok, Tok::Newline | Tok::LineIndent(_)) {
                self.pos += 1;
            }
        }
    }

    fn peek(&mut self) -> &Tok {
        self.skip_breaks();
        &self.toks[self.pos].tok
    }

    fn peek_at(&mut self, n: usize) -> &Tok {
        self.skip_breaks();
        let mut i = self.pos;
        let mut left = n;
        while left > 0 && i + 1 < self.toks.len() {
            i += 1;
            if self.skipping() && matches!(self.toks[i].tok, Tok::Newline | Tok::LineIndent(_)) {
                continue;
            }
            left -= 1;
        }
        &self.toks[i].tok
    }

    fn line(&mut self) -> u32 {
        self.skip_breaks();
        self.toks[self.pos].line
    }

    fn next(&mut self) -> Token {
        self.skip_breaks();
        let t = self.toks[self.pos].clone();
        if self.pos + 1 < self.toks.len() {
            self.pos += 1;
        }
        t
    }

    fn at(&mut self, t: &Tok) -> bool {
        self.peek() == t
    }

    fn eat(&mut self, t: &Tok) -> bool {
        if self.at(t) {
            self.next();
            true
        } else {
            false
        }
    }

    fn err<T>(&mut self, msg: impl Into<String>) -> PResult<T> {
        self.skip_breaks();
        let t = &self.toks[self.pos];
        Err(ParseError { line: t.line, col: t.col, message: msg.into() })
    }

    fn expect(&mut self, t: &Tok, what: &str) -> PResult<()> {
        if self.eat(t) {
            Ok(())
        } else {
            let found = self.peek().clone();
            self.err(format!("expected {what}, found {found}"))
        }
    }

    fn ident(&mut self, what: &str) -> PResult<String> {
        match self.peek().clone() {
            Tok::Ident(s) => {
                self.next();
                Ok(s)
            }
            // Keywords Godot accepts as names in some positions.
            Tok::Match | Tok::When | Tok::Namespace | Tok::Trait => {
                let t = self.next();
                Ok(format!("{}", t.tok).trim_matches('"').to_string())
            }
            other => self.err(format!("expected {what}, found {other}")),
        }
    }

    /// After a statement: newline, `;`, or the end of an enclosing block.
    fn end_statement(&mut self) -> PResult<()> {
        // A statement whose last part was an indented block (a lambda) ends with it.
        if self.pos > 0 && matches!(self.toks[self.pos - 1].tok, Tok::Dedent | Tok::Newline) {
            return Ok(());
        }
        if self.eat(&Tok::Semicolon) {
            self.eat(&Tok::Newline);
            return Ok(());
        }
        match self.peek() {
            Tok::Newline => {
                self.next();
                Ok(())
            }
            // Ends of blocks and of lambdas inside expressions.
            Tok::Dedent | Tok::Eof | Tok::LineIndent(_) | Tok::ParenClose | Tok::BracketClose | Tok::BraceClose | Tok::Comma => Ok(()),
            other => {
                let o = other.clone();
                self.err(format!("expected end of statement, found {o}"))
            }
        }
    }

    // ------------------------------------------------------------ script / class

    fn script(&mut self) -> PResult<ClassDecl> {
        let mut class = ClassDecl::default();
        self.class_body(&mut class, true)?;
        if !self.at(&Tok::Eof) {
            let t = self.peek().clone();
            return self.err(format!("unexpected {t}"));
        }
        Ok(class)
    }

    fn class_body(&mut self, class: &mut ClassDecl, top: bool) -> PResult<()> {
        let mut pending: Vec<Annotation> = Vec::new();
        loop {
            let t = self.peek().clone();
            match t {
                Tok::Eof => break,
                Tok::Dedent if !top => break,
                Tok::Newline | Tok::Semicolon => {
                    self.next();
                }
                Tok::Annotation(name) => {
                    let a = self.annotation()?;
                    match name.as_str() {
                        "tool" => class.tool = true,
                        "icon" => class.icon = a.args.first().and_then(|e| if let ExprKind::Literal(Literal::Str(s)) = &e.kind { Some(s.clone()) } else { None }),
                        "abstract" if matches!(self.peek(), Tok::Newline | Tok::ClassName | Tok::Extends | Tok::Annotation(_) | Tok::Eof) => class.abstract_ = true,
                        "static_unload" => {}
                        // Group/category annotations stand alone.
                        "export_category" | "export_group" | "export_subgroup" => class.members.push(Member::Annotation(a)),
                        _ => pending.push(a),
                    }
                    self.eat(&Tok::Newline);
                }
                Tok::ClassName => {
                    self.next();
                    class.name = Some(self.ident("class name")?);
                    if self.eat(&Tok::Extends) {
                        class.extends = Some(self.extends()?);
                    }
                    self.end_statement()?;
                }
                Tok::Extends => {
                    self.next();
                    class.extends = Some(self.extends()?);
                    if self.eat(&Tok::ClassName) {
                        class.name = Some(self.ident("class name")?);
                    }
                    self.end_statement()?;
                }
                Tok::Var => {
                    let mut v = self.var_decl(false)?;
                    v.annotations = core::mem::take(&mut pending);
                    class.members.push(Member::Var(v));
                }
                Tok::Static => {
                    self.next();
                    match self.peek() {
                        Tok::Func => {
                            let mut f = self.func_decl()?;
                            f.is_static = true;
                            f.annotations = core::mem::take(&mut pending);
                            class.members.push(Member::Func(f));
                        }
                        Tok::Var => {
                            let mut v = self.var_decl(false)?;
                            v.is_static = true;
                            v.annotations = core::mem::take(&mut pending);
                            class.members.push(Member::Var(v));
                        }
                        other => {
                            let o = other.clone();
                            return self.err(format!("expected \"func\" or \"var\" after \"static\", found {o}"));
                        }
                    }
                }
                Tok::Const => {
                    let c = self.const_decl()?;
                    class.members.push(Member::Const(c));
                }
                Tok::Signal => {
                    let line = self.line();
                    self.next();
                    let name = self.ident("signal name")?;
                    let mut params = Vec::new();
                    if self.at(&Tok::ParenOpen) {
                        params = self.params()?.0;
                    }
                    self.end_statement()?;
                    class.members.push(Member::Signal(SignalDecl { name, params, line }));
                }
                Tok::Enum => {
                    let e = self.enum_decl()?;
                    class.members.push(Member::Enum(e));
                }
                Tok::Func => {
                    let mut f = self.func_decl()?;
                    f.annotations = core::mem::take(&mut pending);
                    class.members.push(Member::Func(f));
                }
                Tok::Class => {
                    self.next();
                    let mut inner = ClassDecl { name: Some(self.ident("class name")?), ..ClassDecl::default() };
                    if self.eat(&Tok::Extends) {
                        inner.extends = Some(self.extends()?);
                    }
                    self.expect(&Tok::Colon, "\":\" after class declaration")?;
                    self.expect(&Tok::Newline, "end of line")?;
                    self.expect(&Tok::Indent, "an indented class body")?;
                    self.class_body(&mut inner, false)?;
                    self.expect(&Tok::Dedent, "end of class body")?;
                    class.members.push(Member::Class(inner));
                }
                Tok::Pass => {
                    self.next();
                    self.end_statement()?;
                }
                other => return self.err(format!("unexpected {other} in class body")),
            }
        }
        Ok(())
    }

    fn extends(&mut self) -> PResult<Extends> {
        if let Tok::Str(path) = self.peek().clone() {
            self.next();
            let mut sub = Vec::new();
            while self.eat(&Tok::Period) {
                sub.push(self.ident("class name")?);
            }
            return Ok(Extends::Path(path, sub));
        }
        let mut path = vec![self.ident("class to extend")?];
        while self.eat(&Tok::Period) {
            path.push(self.ident("class name")?);
        }
        Ok(Extends::Class(path))
    }

    fn annotation(&mut self) -> PResult<Annotation> {
        let line = self.line();
        let Tok::Annotation(name) = self.next().tok else { unreachable!("checked by caller") };
        let mut args = Vec::new();
        if self.at(&Tok::ParenOpen) {
            args = self.call_args()?;
        }
        Ok(Annotation { name, args, line })
    }

    fn type_ref(&mut self) -> PResult<TypeRef> {
        if self.eat(&Tok::Void) {
            return Ok(TypeRef { path: vec!["void".into()], args: Vec::new() });
        }
        if self.at(&Tok::Null) {
            self.next();
            return Ok(TypeRef { path: vec!["null".into()], args: Vec::new() });
        }
        let mut path = vec![self.ident("a type")?];
        while self.at(&Tok::Period) && matches!(self.peek_at(1), Tok::Ident(_)) {
            self.next();
            path.push(self.ident("a type")?);
        }
        let mut args = Vec::new();
        if self.at(&Tok::BracketOpen) {
            self.next();
            self.multiline.push(true);
            loop {
                args.push(self.type_ref()?);
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
            self.multiline.pop();
            self.expect(&Tok::BracketClose, "\"]\" after container element type")?;
        }
        Ok(TypeRef { path, args })
    }

    /// `: Type`, `:= value` / `: Type = value` / `= value` after a name.
    fn typed_value(&mut self) -> PResult<(Option<TypeRef>, bool, Option<Expr>)> {
        let mut ty = None;
        if self.eat(&Tok::Colon) {
            if self.eat(&Tok::Eq) {
                return Ok((None, true, Some(self.expr()?)));
            }
            ty = Some(self.type_ref()?);
        }
        let value = if self.eat(&Tok::Eq) { Some(self.expr()?) } else { None };
        Ok((ty, false, value))
    }

    fn var_decl(&mut self, local: bool) -> PResult<VarDecl> {
        let line = self.line();
        self.expect(&Tok::Var, "\"var\"")?;
        let name = self.ident("variable name")?;
        let (ty, infer, value) = self.typed_value()?;
        let mut v = VarDecl { name, ty, infer, value, annotations: Vec::new(), is_static: false, setter: None, getter: None, line };
        if !local && self.eat(&Tok::Colon) {
            self.accessors(&mut v)?;
            return Ok(v);
        }
        self.end_statement()?;
        Ok(v)
    }

    /// `set(value): ...` / `get: ...` blocks, or `set = f, get = g` inline.
    fn accessors(&mut self, v: &mut VarDecl) -> PResult<()> {
        if self.eat(&Tok::Newline) {
            self.expect(&Tok::Indent, "indented set/get")?;
            while !self.at(&Tok::Dedent) && !self.at(&Tok::Eof) {
                if self.eat(&Tok::Newline) {
                    continue;
                }
                self.accessor(v)?;
            }
            self.expect(&Tok::Dedent, "end of set/get block")?;
        } else {
            loop {
                self.accessor(v)?;
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
            self.end_statement()?;
        }
        Ok(())
    }

    fn accessor(&mut self, v: &mut VarDecl) -> PResult<()> {
        let which = self.ident("\"set\" or \"get\"")?;
        let acc = if self.eat(&Tok::Eq) {
            Accessor::Method(self.ident("method name")?)
        } else {
            let mut param = None;
            if self.eat(&Tok::ParenOpen) {
                if !self.at(&Tok::ParenClose) {
                    param = Some(self.ident("parameter name")?);
                    if self.eat(&Tok::Colon) {
                        self.type_ref()?;
                    }
                }
                self.expect(&Tok::ParenClose, "\")\"")?;
            }
            self.expect(&Tok::Colon, "\":\"")?;
            let body = self.suite()?;
            Accessor::Inline { param, body }
        };
        match which.as_str() {
            "set" => v.setter = Some(acc),
            "get" => v.getter = Some(acc),
            other => return self.err(format!("expected \"set\" or \"get\", found \"{other}\"")),
        }
        Ok(())
    }

    fn const_decl(&mut self) -> PResult<ConstDecl> {
        let line = self.line();
        self.expect(&Tok::Const, "\"const\"")?;
        let name = self.ident("constant name")?;
        let (ty, _, value) = self.typed_value()?;
        let Some(value) = value else { return self.err("a constant needs a value") };
        self.end_statement()?;
        Ok(ConstDecl { name, ty, value, line })
    }

    fn enum_decl(&mut self) -> PResult<EnumDecl> {
        let line = self.line();
        self.expect(&Tok::Enum, "\"enum\"")?;
        let name = if let Tok::Ident(n) = self.peek().clone() {
            self.next();
            Some(n)
        } else {
            None
        };
        self.expect(&Tok::BraceOpen, "\"{\" after enum")?;
        self.multiline.push(true);
        let mut values = Vec::new();
        while !self.at(&Tok::BraceClose) {
            let n = self.ident("enum value name")?;
            let v = if self.eat(&Tok::Eq) { Some(self.expr()?) } else { None };
            values.push((n, v));
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.multiline.pop();
        self.expect(&Tok::BraceClose, "\"}\" after enum values")?;
        self.end_statement()?;
        Ok(EnumDecl { name, values, line })
    }

    /// `(a, b: int, c := 1, ...rest)`
    fn params(&mut self) -> PResult<(Vec<Param>, Option<Param>)> {
        self.expect(&Tok::ParenOpen, "\"(\"")?;
        self.multiline.push(true);
        let mut params = Vec::new();
        let mut rest = None;
        while !self.at(&Tok::ParenClose) {
            let variadic = self.eat(&Tok::PeriodPeriodPeriod);
            let name = self.ident("parameter name")?;
            let (ty, infer, default) = self.typed_value()?;
            let p = Param { name, ty, infer, default };
            if variadic {
                rest = Some(p);
            } else {
                params.push(p);
            }
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.multiline.pop();
        self.expect(&Tok::ParenClose, "\")\" after parameters")?;
        Ok((params, rest))
    }

    fn func_decl(&mut self) -> PResult<FuncDecl> {
        let line = self.line();
        self.expect(&Tok::Func, "\"func\"")?;
        let name = self.ident("function name")?;
        let (params, rest) = self.params()?;
        let ret = if self.eat(&Tok::Arrow) { Some(self.type_ref()?) } else { None };
        self.expect(&Tok::Colon, "\":\" after function declaration")?;
        let body = self.suite()?;
        Ok(FuncDecl { name, params, rest, ret, body, is_static: false, annotations: Vec::new(), line })
    }

    // ------------------------------------------------------------ statements

    /// The block after a `:`. Three shapes, like Godot:
    /// - same line: `if x: return` (statements until the end of the line);
    /// - indented block: NEWLINE INDENT ... DEDENT;
    /// - inside brackets (multi-line lambdas): lines with a deeper
    ///   `LineIndent` than the line the lambda started on.
    fn suite(&mut self) -> PResult<Vec<Stmt>> {
        // Look at the raw next token (line breaks matter here).
        let raw = self.toks[self.pos].tok.clone();
        let skipping = self.skipping();
        let mut k = self.pos;
        while self.toks[k].tok == Tok::Newline {
            k += 1;
        }
        let bracket_block = matches!(raw, Tok::Newline | Tok::LineIndent(_)) && matches!(self.toks[k].tok, Tok::LineIndent(_));
        if raw == Tok::Newline && !bracket_block {
            self.next();
            self.expect(&Tok::Indent, "an indented block")?;
            let mut body = Vec::new();
            while !self.at(&Tok::Dedent) && !self.at(&Tok::Eof) {
                if self.eat(&Tok::Newline) {
                    continue;
                }
                body.extend(self.statement()?);
            }
            self.expect(&Tok::Dedent, "end of block")?;
            return Ok(body);
        }
        if bracket_block {
            // Block inside brackets: find its indentation.
            let mut i = self.pos;
            while self.toks[i].tok == Tok::Newline {
                i += 1;
            }
            let Tok::LineIndent(width) = self.toks[i].tok else { return self.err("expected an indented block") };
            let outer = self.toks[self.pos.saturating_sub(1)].indent;
            if width <= outer {
                return self.err("expected an indented block");
            }
            self.multiline.push(false);
            let mut body = Vec::new();
            loop {
                // Consume the line break(s) before the next statement.
                let mut j = self.pos;
                while self.toks[j].tok == Tok::Newline {
                    j += 1;
                }
                match self.toks[j].tok {
                    Tok::LineIndent(w) if w == width => {
                        self.pos = j + 1;
                        body.extend(self.statement()?);
                    }
                    Tok::LineIndent(w) if w > width => {
                        self.multiline.pop();
                        return self.err("unexpected indentation");
                    }
                    _ => break,
                }
            }
            self.multiline.pop();
            return Ok(body);
        }
        // Same-line body (it may itself be `if x: y`, like in Godot).
        let mut body = Vec::new();
        self.multiline.push(false);
        loop {
            if matches!(self.peek(), Tok::If | Tok::While | Tok::For) {
                body.extend(self.statement()?);
                break;
            }
            body.extend(self.simple_statement()?);
            if !self.eat(&Tok::Semicolon) {
                break;
            }
            if matches!(self.peek(), Tok::Newline | Tok::Eof) {
                break;
            }
        }
        self.multiline.pop();
        if !skipping {
            self.end_statement()?;
        }
        Ok(body)
    }

    /// Statements that fit on one line after a `:` (no nested blocks).
    fn simple_statement(&mut self) -> PResult<Vec<Stmt>> {
        let line = self.line();
        let kind = match self.peek().clone() {
            Tok::Pass => {
                self.next();
                StmtKind::Pass
            }
            Tok::Break => {
                self.next();
                StmtKind::Break
            }
            Tok::Continue => {
                self.next();
                StmtKind::Continue
            }
            Tok::Breakpoint => {
                self.next();
                StmtKind::Breakpoint
            }
            Tok::Return => {
                self.next();
                let v = if matches!(self.peek(), Tok::Newline | Tok::Semicolon | Tok::Dedent | Tok::Eof | Tok::LineIndent(_) | Tok::ParenClose | Tok::BracketClose | Tok::BraceClose | Tok::Comma) { None } else { Some(self.expr()?) };
                StmtKind::Return(v)
            }
            Tok::Var => {
                let ln = self.line();
                self.next();
                let name = self.ident("variable name")?;
                let (ty, infer, value) = self.typed_value()?;
                StmtKind::Var(VarDecl { name, ty, infer, value, annotations: Vec::new(), is_static: false, setter: None, getter: None, line: ln })
            }
            Tok::Assert => {
                self.next();
                let args = self.call_args()?;
                let mut it = args.into_iter();
                let Some(cond) = it.next() else { return self.err("assert() needs a condition") };
                StmtKind::Assert { cond, message: it.next() }
            }
            _ => StmtKind::Expr(self.expr_or_assign()?),
        };
        Ok(vec![Stmt { kind, line }])
    }

    fn statement(&mut self) -> PResult<Vec<Stmt>> {
        let line = self.line();
        let t = self.peek().clone();
        let kind = match t {
            Tok::Annotation(_) => {
                // @warning_ignore(...) and friends apply to the next statement.
                self.annotation()?;
                self.eat(&Tok::Newline);
                return Ok(Vec::new());
            }
            Tok::If => {
                let if_indent = self.toks[self.pos].indent;
                self.next();
                let cond = self.expr()?;
                self.expect(&Tok::Colon, "\":\" after if condition")?;
                let then = self.suite()?;
                let mut elifs = Vec::new();
                let mut else_ = None;
                loop {
                    if self.block_continues(&Tok::Elif, if_indent) {
                        self.next();
                        let c = self.expr()?;
                        self.expect(&Tok::Colon, "\":\" after elif condition")?;
                        elifs.push((c, self.suite()?));
                    } else if self.block_continues(&Tok::Else, if_indent) {
                        self.next();
                        self.expect(&Tok::Colon, "\":\" after else")?;
                        else_ = Some(self.suite()?);
                        break;
                    } else {
                        break;
                    }
                }
                return Ok(vec![Stmt { kind: StmtKind::If { cond, then, elifs, else_ }, line }]);
            }
            Tok::While => {
                self.next();
                let cond = self.expr()?;
                self.expect(&Tok::Colon, "\":\" after while condition")?;
                StmtKind::While { cond, body: self.suite()? }
            }
            Tok::For => {
                self.next();
                let var = self.ident("loop variable")?;
                let ty = if self.eat(&Tok::Colon) { Some(self.type_ref()?) } else { None };
                self.expect(&Tok::In, "\"in\" after for variable")?;
                let iter = self.expr()?;
                self.expect(&Tok::Colon, "\":\" after for")?;
                StmtKind::For { var, ty, iter, body: self.suite()? }
            }
            Tok::Match => {
                self.next();
                let value = self.expr()?;
                self.expect(&Tok::Colon, "\":\" after match value")?;
                StmtKind::Match { value, branches: self.match_branches()? }
            }
            Tok::Const => StmtKind::Const(self.const_decl_local()?),
            _ => {
                let s = self.simple_statement()?;
                self.end_statement()?;
                return Ok(s);
            }
        };
        Ok(vec![Stmt { kind, line }])
    }

    /// `elif`/`else` right after a block. Inside a lambda body within
    /// brackets they follow a line break at the `if` line's width.
    fn block_continues(&mut self, t: &Tok, if_indent: u32) -> bool {
        if self.toks[self.pos].tok == *t {
            return true;
        }
        if self.skipping() {
            return false;
        }
        let mut j = self.pos;
        while self.toks[j].tok == Tok::Newline {
            j += 1;
        }
        if self.toks[j].tok == Tok::LineIndent(if_indent) && self.toks.get(j + 1).map(|x| &x.tok) == Some(t) {
            self.pos = j + 1;
            return true;
        }
        false
    }

    fn const_decl_local(&mut self) -> PResult<ConstDecl> {
        let line = self.line();
        self.expect(&Tok::Const, "\"const\"")?;
        let name = self.ident("constant name")?;
        let (ty, _, value) = self.typed_value()?;
        let Some(value) = value else { return self.err("a constant needs a value") };
        self.end_statement()?;
        Ok(ConstDecl { name, ty, value, line })
    }

    fn match_branches(&mut self) -> PResult<Vec<MatchBranch>> {
        self.expect(&Tok::Newline, "end of line after match")?;
        self.expect(&Tok::Indent, "indented match branches")?;
        let mut branches = Vec::new();
        while !self.at(&Tok::Dedent) && !self.at(&Tok::Eof) {
            if self.eat(&Tok::Newline) {
                continue;
            }
            if let Tok::Annotation(_) = self.peek() {
                self.annotation()?;
                self.eat(&Tok::Newline);
                continue;
            }
            let line = self.line();
            let mut patterns = vec![self.pattern()?];
            while self.eat(&Tok::Comma) {
                patterns.push(self.pattern()?);
            }
            let guard = if self.eat(&Tok::When) { Some(self.expr()?) } else { None };
            self.expect(&Tok::Colon, "\":\" after match pattern")?;
            let body = self.suite()?;
            branches.push(MatchBranch { patterns, guard, body, line });
        }
        self.expect(&Tok::Dedent, "end of match")?;
        Ok(branches)
    }

    fn pattern(&mut self) -> PResult<Pattern> {
        match self.peek().clone() {
            Tok::Underscore => {
                self.next();
                Ok(Pattern::Wildcard)
            }
            Tok::Var => {
                self.next();
                Ok(Pattern::Bind(self.ident("binding name")?))
            }
            Tok::BracketOpen => {
                self.next();
                self.multiline.push(true);
                let mut items = Vec::new();
                let mut rest = false;
                while !self.at(&Tok::BracketClose) {
                    if self.eat(&Tok::PeriodPeriod) {
                        rest = true;
                    } else {
                        items.push(self.pattern()?);
                    }
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                self.multiline.pop();
                self.expect(&Tok::BracketClose, "\"]\"")?;
                Ok(Pattern::Array(items, rest))
            }
            Tok::BraceOpen => {
                self.next();
                self.multiline.push(true);
                let mut items = Vec::new();
                let mut rest = false;
                while !self.at(&Tok::BraceClose) {
                    if self.eat(&Tok::PeriodPeriod) {
                        rest = true;
                    } else {
                        let key = self.expr_prec(Prec::LogicOr)?;
                        let value = if self.eat(&Tok::Colon) { Some(self.pattern()?) } else { None };
                        items.push((key, value));
                    }
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                self.multiline.pop();
                self.expect(&Tok::BraceClose, "\"}\"")?;
                Ok(Pattern::Dict(items, rest))
            }
            _ => Ok(Pattern::Expr(self.expr_prec(Prec::LogicOr)?)),
        }
    }

    // ------------------------------------------------------------ expressions

    fn expr(&mut self) -> PResult<Expr> {
        self.expr_prec(Prec::Cast)
    }

    fn expr_or_assign(&mut self) -> PResult<Expr> {
        let target = self.expr()?;
        let op = match self.peek() {
            Tok::Eq => None,
            Tok::PlusEq => Some(BinOp::Add),
            Tok::MinusEq => Some(BinOp::Sub),
            Tok::StarEq => Some(BinOp::Mul),
            Tok::SlashEq => Some(BinOp::Div),
            Tok::PercentEq => Some(BinOp::Mod),
            Tok::StarStarEq => Some(BinOp::Pow),
            Tok::ShlEq => Some(BinOp::Shl),
            Tok::ShrEq => Some(BinOp::Shr),
            Tok::AmpEq => Some(BinOp::BitAnd),
            Tok::PipeEq => Some(BinOp::BitOr),
            Tok::CaretEq => Some(BinOp::BitXor),
            _ => return Ok(target),
        };
        self.next();
        let line = target.line;
        let value = self.expr()?;
        Ok(Expr { kind: ExprKind::Assign { op, target: Box::new(target), value: Box::new(value) }, line })
    }

    fn expr_prec(&mut self, min: Prec) -> PResult<Expr> {
        let mut left = self.prefix()?;
        loop {
            let t = self.peek().clone();
            let (prec, op) = match &t {
                Tok::As => (Prec::Cast, None),
                Tok::If => (Prec::Ternary, None),
                Tok::Or | Tok::PipePipe => (Prec::LogicOr, Some(BinOp::Or)),
                Tok::And | Tok::AmpAmp => (Prec::LogicAnd, Some(BinOp::And)),
                Tok::In => (Prec::ContentTest, Some(BinOp::In)),
                Tok::Not if matches!(self.peek_at(1), Tok::In) => (Prec::ContentTest, Some(BinOp::NotIn)),
                Tok::EqEq => (Prec::Comparison, Some(BinOp::Eq)),
                Tok::BangEq => (Prec::Comparison, Some(BinOp::Ne)),
                Tok::Less => (Prec::Comparison, Some(BinOp::Lt)),
                Tok::LessEq => (Prec::Comparison, Some(BinOp::Le)),
                Tok::Greater => (Prec::Comparison, Some(BinOp::Gt)),
                Tok::GreaterEq => (Prec::Comparison, Some(BinOp::Ge)),
                Tok::Pipe => (Prec::BitOr, Some(BinOp::BitOr)),
                Tok::Caret => (Prec::BitXor, Some(BinOp::BitXor)),
                Tok::Amp => (Prec::BitAnd, Some(BinOp::BitAnd)),
                Tok::Shl => (Prec::BitShift, Some(BinOp::Shl)),
                Tok::Shr => (Prec::BitShift, Some(BinOp::Shr)),
                Tok::Plus => (Prec::AddSub, Some(BinOp::Add)),
                Tok::Minus => (Prec::AddSub, Some(BinOp::Sub)),
                Tok::Star => (Prec::Factor, Some(BinOp::Mul)),
                Tok::Slash => (Prec::Factor, Some(BinOp::Div)),
                Tok::Percent => (Prec::Factor, Some(BinOp::Mod)),
                Tok::StarStar => (Prec::Power, Some(BinOp::Pow)),
                Tok::Is => (Prec::TypeTest, None),
                Tok::ParenOpen | Tok::BracketOpen | Tok::Period => (Prec::Call, None),
                _ => break,
            };
            if prec < min {
                break;
            }
            let line = left.line;
            left = match t {
                Tok::As => {
                    self.next();
                    let ty = self.type_ref()?;
                    Expr { kind: ExprKind::As { value: Box::new(left), ty }, line }
                }
                Tok::If => {
                    self.next();
                    let cond = self.expr_prec(Prec::Ternary.next())?;
                    self.expect(&Tok::Else, "\"else\" in ternary expression")?;
                    let else_ = self.expr_prec(Prec::Ternary)?;
                    Expr { kind: ExprKind::Ternary { cond: Box::new(cond), then: Box::new(left), else_: Box::new(else_) }, line }
                }
                Tok::Is => {
                    self.next();
                    let negated = self.eat(&Tok::Not);
                    let ty = self.type_ref()?;
                    Expr { kind: ExprKind::Is { value: Box::new(left), ty, negated }, line }
                }
                Tok::ParenOpen => {
                    let args = self.call_args()?;
                    Expr { kind: ExprKind::Call { callee: Box::new(left), args }, line }
                }
                Tok::BracketOpen => {
                    self.next();
                    self.multiline.push(true);
                    let index = self.expr()?;
                    self.multiline.pop();
                    self.expect(&Tok::BracketClose, "\"]\"")?;
                    Expr { kind: ExprKind::Index { base: Box::new(left), index: Box::new(index) }, line }
                }
                Tok::Period => {
                    self.next();
                    let name = match self.peek().clone() {
                        Tok::Ident(s) => {
                            self.next();
                            s
                        }
                        // Keywords valid as member names (`x.match`, `Tween.set_trans`...).
                        other if !matches!(other, Tok::Newline | Tok::Eof | Tok::Indent | Tok::Dedent) => {
                            let tok = self.next();
                            let text = format!("{}", tok.tok);
                            text.trim_matches('"').to_string()
                        }
                        other => return self.err(format!("expected member name after \".\", found {other}")),
                    };
                    Expr { kind: ExprKind::Attr { base: Box::new(left), name }, line }
                }
                _ => {
                    let op = op.expect("binary operators set op");
                    self.next();
                    if op == BinOp::NotIn {
                        self.next(); // `in`
                    }
                    // Left-associative, including `**` (Godot).
                    let right = self.expr_prec(prec.next())?;
                    Expr { kind: ExprKind::Binary(op, Box::new(left), Box::new(right)), line }
                }
            };
        }
        Ok(left)
    }

    fn call_args(&mut self) -> PResult<Vec<Expr>> {
        self.expect(&Tok::ParenOpen, "\"(\"")?;
        self.multiline.push(true);
        let mut args = Vec::new();
        while !self.at(&Tok::ParenClose) {
            args.push(self.expr()?);
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.multiline.pop();
        self.expect(&Tok::ParenClose, "\")\" after arguments")?;
        Ok(args)
    }

    fn prefix(&mut self) -> PResult<Expr> {
        let line = self.line();
        let t = self.next();
        let lit = |l: Literal| Ok(Expr { kind: ExprKind::Literal(l), line });
        match t.tok {
            Tok::Int(i) => lit(Literal::Int(i)),
            Tok::Float(f) => lit(Literal::Float(f)),
            Tok::Str(s) => lit(Literal::Str(s)),
            Tok::StringName(s) => lit(Literal::StringName(s)),
            Tok::NodePath(s) => lit(Literal::NodePath(s)),
            Tok::True => lit(Literal::Bool(true)),
            Tok::False => lit(Literal::Bool(false)),
            Tok::Null => lit(Literal::Null),
            Tok::ConstPi => lit(Literal::Float(core::f64::consts::PI)),
            Tok::ConstTau => lit(Literal::Float(core::f64::consts::TAU)),
            Tok::ConstInf => lit(Literal::Float(f64::INFINITY)),
            Tok::ConstNan => lit(Literal::Float(f64::NAN)),
            Tok::Ident(name) => Ok(Expr { kind: ExprKind::Ident(name), line }),
            Tok::SelfKw => Ok(Expr { kind: ExprKind::SelfRef, line }),
            Tok::Super => {
                let method = if self.at(&Tok::Period) {
                    self.next();
                    Some(self.ident("method name")?)
                } else {
                    None
                };
                Ok(Expr { kind: ExprKind::Super(method), line })
            }
            Tok::Minus => Ok(Expr { kind: ExprKind::Unary(UnaryOp::Neg, Box::new(self.expr_prec(Prec::Sign)?)), line }),
            Tok::Plus => Ok(Expr { kind: ExprKind::Unary(UnaryOp::Pos, Box::new(self.expr_prec(Prec::Sign)?)), line }),
            Tok::Not | Tok::Bang => Ok(Expr { kind: ExprKind::Unary(UnaryOp::Not, Box::new(self.expr_prec(Prec::LogicNot)?)), line }),
            Tok::Tilde => Ok(Expr { kind: ExprKind::Unary(UnaryOp::BitNot, Box::new(self.expr_prec(Prec::BitNot)?)), line }),
            Tok::Await => Ok(Expr { kind: ExprKind::Await(Box::new(self.expr_prec(Prec::Await)?)), line }),
            Tok::ParenOpen => {
                self.multiline.push(true);
                let e = self.expr()?;
                self.multiline.pop();
                self.expect(&Tok::ParenClose, "\")\"")?;
                Ok(e)
            }
            Tok::BracketOpen => {
                self.multiline.push(true);
                let mut items = Vec::new();
                while !self.at(&Tok::BracketClose) {
                    items.push(self.expr()?);
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                self.multiline.pop();
                self.expect(&Tok::BracketClose, "\"]\" after array")?;
                Ok(Expr { kind: ExprKind::Array(items), line })
            }
            Tok::BraceOpen => {
                self.multiline.push(true);
                let mut pairs = Vec::new();
                while !self.at(&Tok::BraceClose) {
                    // Lua-style `{key = value}` or Python-style `{expr: value}`.
                    if let (Tok::Ident(k), Tok::Eq) = (self.peek().clone(), self.peek_at(1).clone()) {
                        self.next();
                        self.next();
                        let key = Expr { kind: ExprKind::Literal(Literal::StringName(k)), line: self.line() };
                        pairs.push((key, self.expr()?));
                    } else {
                        let key = self.expr()?;
                        self.expect(&Tok::Colon, "\":\" in dictionary")?;
                        pairs.push((key, self.expr()?));
                    }
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                self.multiline.pop();
                self.expect(&Tok::BraceClose, "\"}\" after dictionary")?;
                Ok(Expr { kind: ExprKind::Dict(pairs), line })
            }
            Tok::Dollar => {
                let path = self.node_path()?;
                Ok(Expr { kind: ExprKind::GetNode { path, unique: false }, line })
            }
            Tok::Percent => {
                let path = self.node_path()?;
                Ok(Expr { kind: ExprKind::GetNode { path, unique: true }, line })
            }
            Tok::Preload => {
                let args = self.call_args()?;
                match args.into_iter().next().map(|e| e.kind) {
                    Some(ExprKind::Literal(Literal::Str(p))) => Ok(Expr { kind: ExprKind::Preload(p), line }),
                    _ => self.err("preload() needs a constant path string"),
                }
            }
            Tok::Func => {
                // Lambda: `func(args) -> T: body` (optionally named).
                let name = if let Tok::Ident(n) = self.peek().clone() {
                    self.next();
                    n
                } else {
                    String::new()
                };
                let (params, rest) = self.params()?;
                let ret = if self.eat(&Tok::Arrow) { Some(self.type_ref()?) } else { None };
                self.expect(&Tok::Colon, "\":\" after lambda declaration")?;
                let body = self.lambda_body()?;
                Ok(Expr { kind: ExprKind::Lambda(Box::new(FuncDecl { name, params, rest, ret, body, is_static: false, annotations: Vec::new(), line })), line })
            }
            other => Err(ParseError { line: t.line, col: t.col, message: format!("expected an expression, found {other}") }),
        }
    }

    /// A lambda body: an indented block, or statements on the same line.
    fn lambda_body(&mut self) -> PResult<Vec<Stmt>> {
        let raw = self.toks[self.pos].tok.clone();
        if raw == Tok::Newline || matches!(raw, Tok::LineIndent(_)) {
            return self.suite();
        }
        // Same line: one or more simple statements; stops at `,` `)` etc.
        let mut body = Vec::new();
        let outer = self.skipping();
        self.multiline.push(false);
        loop {
            if matches!(self.peek(), Tok::If | Tok::While | Tok::For) {
                body.extend(self.statement()?);
                break;
            }
            body.extend(self.simple_statement()?);
            if !self.eat(&Tok::Semicolon) {
                break;
            }
        }
        self.multiline.pop();
        if !outer && self.at(&Tok::Newline) {
            // Top-level `var f = func(): return 1` ends with the line.
        }
        Ok(body)
    }

    /// `$Node/Child`, `$"../Path"`, `%Unique`.
    fn node_path(&mut self) -> PResult<String> {
        let mut path = String::new();
        loop {
            match self.peek().clone() {
                Tok::Str(s) => {
                    self.next();
                    path.push_str(&s);
                }
                Tok::Ident(s) => {
                    self.next();
                    path.push_str(&s);
                }
                Tok::PeriodPeriod => {
                    self.next();
                    path.push_str("..");
                }
                Tok::Period => {
                    self.next();
                    path.push('.');
                }
                Tok::Percent if path.is_empty() || path.ends_with('/') => {
                    self.next();
                    path.push('%');
                    continue;
                }
                other => {
                    if path.is_empty() {
                        return self.err(format!("expected a node path after \"$\", found {other}"));
                    }
                }
            }
            // Continue only through `/` (no spaces matter in tokens).
            if self.toks[self.pos].tok == Tok::Slash {
                self.next();
                path.push('/');
            } else {
                break;
            }
        }
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(src: &str) -> ClassDecl {
        parse(src).unwrap_or_else(|e| panic!("{e}\n---\n{src}"))
    }

    #[test]
    fn declarations() {
        let c = ok(r#"@tool
class_name Player extends CharacterBody2D

signal hit(damage: int)
enum State { IDLE, RUN = 5, JUMP }
const SPEED := 300.0
@export var health: int = 100
@export_range(0, 10) var level := 1
@onready var sprite: Sprite2D = $Sprite2D
var items: Array[String] = []
var hp: int = 3:
	set(value):
		hp = clamp(value, 0, 10)
	get:
		return hp
var mana := 1: set = set_mana, get = get_mana

static func make() -> Player:
	return null

func _ready() -> void:
	pass

class Inner extends RefCounted:
	var x = 1
"#);
        assert!(c.tool);
        assert_eq!(c.name.as_deref(), Some("Player"));
        assert_eq!(c.extends, Some(Extends::Class(vec!["CharacterBody2D".into()])));
        assert_eq!(c.members.len(), 12);
        let Member::Var(hp) = &c.members[7] else { panic!() };
        assert!(hp.setter.is_some() && hp.getter.is_some());
    }

    #[test]
    fn precedence_matches_godot() {
        let c = ok("var a = 1 + 2 * 3 ** 2 ** 2\nvar b = not x in y\nvar c = -2 ** 2\nvar d = x as int if t else 0\nvar e = a or b and c\n");
        let Member::Var(a) = &c.members[0] else { panic!() };
        // 1 + (2 * ((3 ** 2) ** 2))
        let ExprKind::Binary(BinOp::Add, _, r) = &a.value.as_ref().unwrap().kind else { panic!("{a:?}") };
        let ExprKind::Binary(BinOp::Mul, _, p) = &r.kind else { panic!() };
        let ExprKind::Binary(BinOp::Pow, l, _) = &p.kind else { panic!() };
        assert!(matches!(l.kind, ExprKind::Binary(BinOp::Pow, _, _)));
        let Member::Var(b) = &c.members[1] else { panic!() };
        assert!(matches!(b.value.as_ref().unwrap().kind, ExprKind::Unary(UnaryOp::Not, _)));
        let Member::Var(cc) = &c.members[2] else { panic!() };
        assert!(matches!(cc.value.as_ref().unwrap().kind, ExprKind::Unary(UnaryOp::Neg, _)));
        let Member::Var(d) = &c.members[3] else { panic!() };
        assert!(matches!(d.value.as_ref().unwrap().kind, ExprKind::Ternary { .. }));
    }

    #[test]
    fn statements_and_match() {
        ok(r#"func f(x, y := 2, z: int = 3) -> int:
	var total := 0
	for i in range(10):
		if i % 2 == 0: continue
		elif i > 5:
			break
		else:
			total += i
	while total > 0: total -= 1
	match x:
		1, 2:
			return 1
		[var a, _, ..]:
			return a
		{"k": var v}:
			return v
		var other when other > 10:
			return other
		_:
			pass
	assert(total == 0, "bad")
	await get_tree().create_timer(1.0).timeout
	$Path/To/Node.visible = false
	%Unique.text = "hi %d" % total
	var d = {a = 1, "b": 2}
	return x if x else y
"#);
    }

    #[test]
    fn lambdas_inline_and_multiline() {
        ok(r#"func _ready():
	button.pressed.connect(func(): print("hi"))
	slider.value_changed.connect(func(v):
		value = v
		if v > 1:
			print(v)
		else:
			print("small")
	)
	ui.fade(func():
		done = true, 0.5)
	var f := func(a, b): return a + b
	t.timeout.connect(func(): if peer: peer.close())
	match key:
		KEY_LEFT: if tab == 0: move(-1)
		_: pass
	var g = func():
		return 1
	items.sort_custom(func(a, b) -> bool: return a.x < b.x)
"#);
    }
}
