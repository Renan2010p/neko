//! Tokenizer, after Godot's `GDScriptTokenizerText`
//! (modules/gdscript/gdscript_tokenizer.cpp).
//!
//! Indentation is significant: the lexer emits `Newline` at the end of each
//! logical line and `Indent`/`Dedent` when the indentation changes. Lines
//! inside `()`, `[]` or `{}` and lines ending in `\` continue the previous one.

use core::fmt;

#[allow(unused_imports)]
use neko_base::prelude::*;

#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    Ident(String),
    Annotation(String),
    Int(i64),
    Float(f64),
    Str(String),
    StringName(String),
    NodePath(String),
    // Operators and punctuation.
    Less, LessEq, Greater, GreaterEq, EqEq, BangEq,
    And, Or, Not, AmpAmp, PipePipe, Bang,
    Amp, Pipe, Tilde, Caret, Shl, Shr,
    Plus, Minus, Star, StarStar, Slash, Percent,
    Eq, PlusEq, MinusEq, StarEq, StarStarEq, SlashEq, PercentEq, ShlEq, ShrEq, AmpEq, PipeEq, CaretEq,
    BracketOpen, BracketClose, BraceOpen, BraceClose, ParenOpen, ParenClose,
    Comma, Semicolon, Period, PeriodPeriod, PeriodPeriodPeriod, Colon, Dollar, Arrow, Underscore, QuestionMark,
    // Keywords.
    If, Elif, Else, For, While, Break, Continue, Pass, Return, Match, When, As, Assert, Await, Breakpoint,
    Class, ClassName, Const, Enum, Extends, Func, In, Is, Namespace, Preload, SelfKw, Signal, Static, Super, Trait, Var, Void, Yield,
    True, False, Null,
    ConstPi, ConstTau, ConstInf, ConstNan,
    // Structure.
    Newline, Indent, Dedent, Eof,
    /// Indentation width of a line inside brackets. Expressions skip it; the
    /// parser uses it for the body of multi-line lambdas inside calls.
    LineIndent(u32),
}

impl fmt::Display for Tok {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Tok::Ident(s) => write!(f, "identifier \"{s}\""),
            Tok::Annotation(s) => write!(f, "@{s}"),
            Tok::Int(i) => write!(f, "{i}"),
            Tok::Float(x) => write!(f, "{x}"),
            Tok::Str(s) => write!(f, "\"{s}\""),
            Tok::Newline => write!(f, "end of line"),
            Tok::Indent => write!(f, "indent"),
            Tok::Dedent => write!(f, "dedent"),
            Tok::LineIndent(_) => write!(f, "line break"),
            Tok::Eof => write!(f, "end of file"),
            other => write!(f, "\"{}\"", symbol(other)),
        }
    }
}

fn symbol(t: &Tok) -> &'static str {
    use Tok::*;
    match t {
        Less => "<", LessEq => "<=", Greater => ">", GreaterEq => ">=", EqEq => "==", BangEq => "!=",
        And => "and", Or => "or", Not => "not", AmpAmp => "&&", PipePipe => "||", Bang => "!",
        Amp => "&", Pipe => "|", Tilde => "~", Caret => "^", Shl => "<<", Shr => ">>",
        Plus => "+", Minus => "-", Star => "*", StarStar => "**", Slash => "/", Percent => "%",
        Eq => "=", PlusEq => "+=", MinusEq => "-=", StarEq => "*=", StarStarEq => "**=", SlashEq => "/=", PercentEq => "%=",
        ShlEq => "<<=", ShrEq => ">>=", AmpEq => "&=", PipeEq => "|=", CaretEq => "^=",
        BracketOpen => "[", BracketClose => "]", BraceOpen => "{", BraceClose => "}", ParenOpen => "(", ParenClose => ")",
        Comma => ",", Semicolon => ";", Period => ".", PeriodPeriod => "..", PeriodPeriodPeriod => "...", Colon => ":",
        Dollar => "$", Arrow => "->", Underscore => "_", QuestionMark => "?",
        If => "if", Elif => "elif", Else => "else", For => "for", While => "while", Break => "break", Continue => "continue",
        Pass => "pass", Return => "return", Match => "match", When => "when", As => "as", Assert => "assert", Await => "await",
        Breakpoint => "breakpoint", Class => "class", ClassName => "class_name", Const => "const", Enum => "enum",
        Extends => "extends", Func => "func", In => "in", Is => "is", Namespace => "namespace", Preload => "preload",
        SelfKw => "self", Signal => "signal", Static => "static", Super => "super", Trait => "trait", Var => "var",
        Void => "void", Yield => "yield", True => "true", False => "false", Null => "null",
        ConstPi => "PI", ConstTau => "TAU", ConstInf => "INF", ConstNan => "NAN",
        _ => "?",
    }
}

fn keyword(s: &str) -> Option<Tok> {
    use Tok::*;
    Some(match s {
        "if" => If, "elif" => Elif, "else" => Else, "for" => For, "while" => While, "break" => Break,
        "continue" => Continue, "pass" => Pass, "return" => Return, "match" => Match, "when" => When,
        "as" => As, "assert" => Assert, "await" => Await, "breakpoint" => Breakpoint, "class" => Class,
        "class_name" => ClassName, "const" => Const, "enum" => Enum, "extends" => Extends, "func" => Func,
        "in" => In, "is" => Is, "namespace" => Namespace, "preload" => Preload, "self" => SelfKw,
        "signal" => Signal, "static" => Static, "super" => Super, "trait" => Trait, "var" => Var,
        "void" => Void, "yield" => Yield, "and" => And, "or" => Or, "not" => Not,
        "true" => True, "false" => False, "null" => Null,
        "PI" => ConstPi, "TAU" => ConstTau, "INF" => ConstInf, "NAN" => ConstNan,
        "_" => Underscore,
        _ => return None,
    })
}

/// A token with its 1-based line and column.
#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub tok: Tok,
    pub line: u32,
    pub col: u32,
    /// Indentation width of the token's line (a tab counts as 4).
    pub indent: u32,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("line {line}:{col}: {message}")]
pub struct LexError {
    pub line: u32,
    pub col: u32,
    pub message: String,
}

pub fn tokenize(src: &str) -> Result<Vec<Token>, LexError> {
    Lexer { chars: src.chars().collect(), pos: 0, line: 1, col: 1, out: Vec::new(), indents: vec![0], depth: 0, line_start: true, bracket_line_start: false, width: 0 }.run()
}

struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: u32,
    col: u32,
    out: Vec<Token>,
    indents: Vec<u32>,
    /// Nesting of (), [], {}: newlines inside are ignored.
    depth: i32,
    line_start: bool,
    /// At the start of a line inside brackets.
    bracket_line_start: bool,
    /// Indentation width of the current line.
    width: u32,
}

impl Lexer {
    fn peek(&self, o: usize) -> char {
        self.chars.get(self.pos + o).copied().unwrap_or('\0')
    }

    fn bump(&mut self) -> char {
        let c = self.peek(0);
        self.pos += 1;
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        c
    }

    fn err(&self, m: impl Into<String>) -> LexError {
        LexError { line: self.line, col: self.col, message: m.into() }
    }

    fn push(&mut self, tok: Tok, line: u32, col: u32) {
        self.out.push(Token { tok, line, col, indent: self.width });
    }

    fn newline(&mut self) {
        if !matches!(self.out.last().map(|t| &t.tok), None | Some(Tok::Newline) | Some(Tok::Indent) | Some(Tok::Dedent) | Some(Tok::LineIndent(_))) {
            self.push(Tok::Newline, self.line, self.col);
        }
    }

    fn run(mut self) -> Result<Vec<Token>, LexError> {
        loop {
            if self.line_start && self.depth == 0 {
                self.line_start = false;
                // Measure indentation; skip blank and comment-only lines.
                let mut width = 0u32;
                loop {
                    match self.peek(0) {
                        ' ' => width += 1,
                        '\t' => width += 4,
                        '\r' => {}
                        _ => break,
                    }
                    self.bump();
                }
                match self.peek(0) {
                    '\n' => {
                        self.bump();
                        self.line_start = true;
                        continue;
                    }
                    '#' => {
                        while !matches!(self.peek(0), '\n' | '\0') {
                            self.bump();
                        }
                        continue;
                    }
                    '\0' => {}
                    _ => {
                        self.width = width;
                        let cur = *self.indents.last().expect("never empty");
                        if width > cur {
                            self.indents.push(width);
                            self.push(Tok::Indent, self.line, 1);
                        } else if width < cur {
                            while width < *self.indents.last().expect("never empty") {
                                self.indents.pop();
                                self.push(Tok::Dedent, self.line, 1);
                            }
                            if width != *self.indents.last().expect("never empty") {
                                return Err(self.err("unindent does not match any outer indentation level"));
                            }
                        }
                    }
                }
            }
            if self.bracket_line_start && self.depth > 0 {
                // Inside brackets: report the line's indentation without
                // touching the indentation stack (see Tok::LineIndent).
                let mut width = 0u32;
                loop {
                    match self.peek(0) {
                        ' ' => width += 1,
                        '\t' => width += 4,
                        '\r' => {}
                        _ => break,
                    }
                    self.bump();
                }
                match self.peek(0) {
                    '\n' => {
                        self.bump();
                        continue;
                    }
                    '#' => {
                        while !matches!(self.peek(0), '\n' | '\0') {
                            self.bump();
                        }
                        continue;
                    }
                    '\0' => {}
                    _ => {
                        self.bracket_line_start = false;
                        self.width = width;
                        self.push(Tok::LineIndent(width), self.line, 1);
                    }
                }
            }
            let (line, col) = (self.line, self.col);
            let c = self.peek(0);
            match c {
                '\0' => break,
                '\n' => {
                    self.bump();
                    if self.depth == 0 {
                        self.newline();
                        self.line_start = true;
                    } else {
                        self.newline();
                        self.bracket_line_start = true;
                    }
                }
                ' ' | '\t' | '\r' => {
                    self.bump();
                }
                '#' => {
                    while !matches!(self.peek(0), '\n' | '\0') {
                        self.bump();
                    }
                }
                '\\' if matches!(self.peek(1), '\n' | '\r') => {
                    // Line continuation.
                    self.bump();
                    while self.peek(0) != '\n' && self.peek(0) != '\0' {
                        self.bump();
                    }
                    self.bump();
                }
                '"' | '\'' => {
                    let s = self.string(false)?;
                    self.push(Tok::Str(s), line, col);
                }
                'r' if matches!(self.peek(1), '"' | '\'') => {
                    self.bump();
                    let s = self.string(true)?;
                    self.push(Tok::Str(s), line, col);
                }
                '&' if matches!(self.peek(1), '"' | '\'') => {
                    self.bump();
                    let s = self.string(false)?;
                    self.push(Tok::StringName(s), line, col);
                }
                '^' if matches!(self.peek(1), '"' | '\'') => {
                    self.bump();
                    let s = self.string(false)?;
                    self.push(Tok::NodePath(s), line, col);
                }
                '@' => {
                    self.bump();
                    let name = self.ident_text();
                    if name.is_empty() {
                        return Err(self.err("expected annotation name after \"@\""));
                    }
                    self.push(Tok::Annotation(name), line, col);
                }
                c if c.is_ascii_digit() || (c == '.' && self.peek(1).is_ascii_digit()) => {
                    let t = self.number()?;
                    self.push(t, line, col);
                }
                c if c.is_alphabetic() || c == '_' => {
                    let name = self.ident_text();
                    let t = keyword(&name).unwrap_or(Tok::Ident(name));
                    self.push(t, line, col);
                }
                _ => {
                    let t = self.operator()?;
                    match t {
                        Tok::ParenOpen | Tok::BracketOpen | Tok::BraceOpen => self.depth += 1,
                        Tok::ParenClose | Tok::BracketClose | Tok::BraceClose => self.depth = (self.depth - 1).max(0),
                        _ => {}
                    }
                    self.push(t, line, col);
                }
            }
        }
        self.newline();
        while self.indents.len() > 1 {
            self.indents.pop();
            self.push(Tok::Dedent, self.line, 1);
        }
        self.push(Tok::Eof, self.line, self.col);
        Ok(self.out)
    }

    fn ident_text(&mut self) -> String {
        let mut s = String::new();
        while self.peek(0).is_alphanumeric() || self.peek(0) == '_' {
            s.push(self.bump());
        }
        s
    }

    fn number(&mut self) -> Result<Tok, LexError> {
        let mut s = String::new();
        if self.peek(0) == '0' && matches!(self.peek(1), 'x' | 'X' | 'b' | 'B') {
            self.bump();
            let radix = if matches!(self.bump(), 'x' | 'X') { 16 } else { 2 };
            while self.peek(0).is_ascii_alphanumeric() || self.peek(0) == '_' {
                let c = self.bump();
                if c != '_' {
                    s.push(c);
                }
            }
            return i64::from_str_radix(&s, radix).map(Tok::Int).map_err(|_| self.err(format!("invalid number \"{s}\"")));
        }
        let mut float = false;
        loop {
            let c = self.peek(0);
            if c.is_ascii_digit() {
                s.push(self.bump());
            } else if c == '_' {
                self.bump();
            } else if c == '.' && !float && self.peek(1) != '.' && !s.contains('e') {
                // `1.` is a float; `1..` is not (range syntax is not GDScript, but be safe).
                if self.peek(1).is_alphabetic() && self.peek(1) != 'e' {
                    break; // `1.abs()` style member access is not valid on literals anyway
                }
                float = true;
                s.push(self.bump());
            } else if (c == 'e' || c == 'E') && !s.contains(['e', 'E']) {
                float = true;
                s.push(self.bump());
                if matches!(self.peek(0), '+' | '-') {
                    s.push(self.bump());
                }
            } else {
                break;
            }
        }
        if float {
            let t = if s.ends_with('.') { format!("{s}0") } else { s.clone() };
            t.parse().map(Tok::Float).map_err(|_| self.err(format!("invalid number \"{s}\"")))
        } else {
            s.parse().map(Tok::Int).or_else(|_| s.parse().map(Tok::Float)).map_err(|_| self.err(format!("invalid number \"{s}\"")))
        }
    }

    fn string(&mut self, raw: bool) -> Result<String, LexError> {
        let q = self.bump();
        let triple = self.peek(0) == q && self.peek(1) == q;
        if triple {
            self.bump();
            self.bump();
        }
        let mut s = String::new();
        loop {
            let c = self.peek(0);
            if c == '\0' {
                return Err(self.err("unterminated string"));
            }
            if c == q {
                if !triple {
                    self.bump();
                    return Ok(s);
                }
                if self.peek(1) == q && self.peek(2) == q {
                    self.bump();
                    self.bump();
                    self.bump();
                    return Ok(s);
                }
            }
            if c == '\n' && !triple {
                return Err(self.err("unterminated string (use \"\"\" for multiline strings)"));
            }
            self.bump();
            if c == '\\' && !raw {
                let e = self.bump();
                match e {
                    'n' => s.push('\n'),
                    't' => s.push('\t'),
                    'r' => s.push('\r'),
                    'a' => s.push('\u{7}'),
                    'b' => s.push('\u{8}'),
                    'f' => s.push('\u{c}'),
                    'v' => s.push('\u{b}'),
                    '0' => s.push('\0'),
                    '\n' => {} // escaped newline joins lines
                    'u' | 'U' => {
                        let len = if e == 'u' { 4 } else { 6 };
                        let mut hex = String::new();
                        for _ in 0..len {
                            hex.push(self.bump());
                        }
                        let ch = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32).ok_or_else(|| self.err("invalid unicode escape"))?;
                        s.push(ch);
                    }
                    other => s.push(other),
                }
            } else if c == '\\' && raw && (self.peek(0) == q || self.peek(0) == '\\') {
                // r"\"" keeps both characters, like Godot.
                s.push('\\');
                s.push(self.bump());
            } else {
                s.push(c);
            }
        }
    }

    fn operator(&mut self) -> Result<Tok, LexError> {
        use Tok::*;
        let c = self.bump();
        let n = self.peek(0);
        let n2 = self.peek(1);
        let mut take = |t: Tok, extra: usize| {
            for _ in 0..extra {
                self.bump();
            }
            t
        };
        Ok(match (c, n, n2) {
            ('<', '<', '=') => take(ShlEq, 2),
            ('>', '>', '=') => take(ShrEq, 2),
            ('*', '*', '=') => take(StarStarEq, 2),
            ('.', '.', '.') => take(PeriodPeriodPeriod, 2),
            ('<', '=', _) => take(LessEq, 1),
            ('>', '=', _) => take(GreaterEq, 1),
            ('=', '=', _) => take(EqEq, 1),
            ('!', '=', _) => take(BangEq, 1),
            ('&', '&', _) => take(AmpAmp, 1),
            ('|', '|', _) => take(PipePipe, 1),
            ('<', '<', _) => take(Shl, 1),
            ('>', '>', _) => take(Shr, 1),
            ('*', '*', _) => take(StarStar, 1),
            ('+', '=', _) => take(PlusEq, 1),
            ('-', '=', _) => take(MinusEq, 1),
            ('*', '=', _) => take(StarEq, 1),
            ('/', '=', _) => take(SlashEq, 1),
            ('%', '=', _) => take(PercentEq, 1),
            ('&', '=', _) => take(AmpEq, 1),
            ('|', '=', _) => take(PipeEq, 1),
            ('^', '=', _) => take(CaretEq, 1),
            ('-', '>', _) => take(Arrow, 1),
            ('.', '.', _) => take(PeriodPeriod, 1),
            ('<', _, _) => Less,
            ('>', _, _) => Greater,
            ('=', _, _) => Eq,
            ('!', _, _) => Bang,
            ('&', _, _) => Amp,
            ('|', _, _) => Pipe,
            ('~', _, _) => Tilde,
            ('^', _, _) => Caret,
            ('+', _, _) => Plus,
            ('-', _, _) => Minus,
            ('*', _, _) => Star,
            ('/', _, _) => Slash,
            ('%', _, _) => Percent,
            ('[', _, _) => BracketOpen,
            (']', _, _) => BracketClose,
            ('{', _, _) => BraceOpen,
            ('}', _, _) => BraceClose,
            ('(', _, _) => ParenOpen,
            (')', _, _) => ParenClose,
            (',', _, _) => Comma,
            (';', _, _) => Semicolon,
            ('.', _, _) => Period,
            (':', _, _) => Colon,
            ('$', _, _) => Dollar,
            ('?', _, _) => QuestionMark,
            (other, _, _) => return Err(LexError { line: self.line, col: self.col.saturating_sub(1), message: format!("invalid character \"{other}\"") }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Tok::*;

    fn toks(src: &str) -> Vec<Tok> {
        tokenize(src).unwrap().into_iter().map(|t| t.tok).collect()
    }

    #[test]
    fn indentation_and_continuation() {
        let src = "func f():\n\tvar a = [1,\n\t\t2]\n\tif a:\n\t\tpass\n\n# c\nvar b = 1 \\\n\t+ 2\n";
        assert_eq!(
            toks(src),
            vec![
                Func, Ident("f".into()), ParenOpen, ParenClose, Colon, Newline, Indent,
                Var, Ident("a".into()), Eq, BracketOpen, Int(1), Comma, Newline, LineIndent(8), Int(2), BracketClose, Newline,
                If, Ident("a".into()), Colon, Newline, Indent, Pass, Newline, Dedent, Dedent,
                Var, Ident("b".into()), Eq, Int(1), Plus, Int(2), Newline, Eof
            ]
        );
    }

    #[test]
    fn literals() {
        assert_eq!(toks("0x1F 0b101 1_000 1.5 .5 1e3 2.0e-1"), vec![Int(31), Int(5), Int(1000), Float(1.5), Float(0.5), Float(1000.0), Float(0.2), Newline, Eof]);
        assert_eq!(toks(r#""a\nb" 'c' &"name" ^"path" r"\d""#), vec![Str("a\nb".into()), Str("c".into()), StringName("name".into()), NodePath("path".into()), Str("\\d".into()), Newline, Eof]);
        assert_eq!(toks("\"\"\"multi\nline\"\"\""), vec![Str("multi\nline".into()), Newline, Eof]);
        assert_eq!(toks("@export var x := 2 ** 3"), vec![Annotation("export".into()), Var, Ident("x".into()), Colon, Eq, Int(2), StarStar, Int(3), Newline, Eof]);
        assert_eq!(toks("$Sprite/Child %Unique"), vec![Dollar, Ident("Sprite".into()), Slash, Ident("Child".into()), Percent, Ident("Unique".into()), Newline, Eof]);
    }
}
