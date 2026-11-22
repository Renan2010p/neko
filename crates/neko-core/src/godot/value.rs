//! Godot's variant text syntax.

#[allow(unused_imports)]
use neko_base::prelude::*;
#[cfg(not(feature = "std"))]
use neko_base::math::FloatExt;

use core::fmt::Write;

/// A value as written in Godot's text formats.
///
/// Built-in math types are kept as constructor calls (`Vector2(1, 2)` is
/// `Call("Vector2", [Int(1), Int(2)])`), which round-trips every type Godot
/// has, including ones Neko does not use yet.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    /// `&"name"`
    StringName(String),
    /// `^"path/to:node"`
    NodePath(String),
    /// `Name(args)`; `Name` may include a type, as in `Array[int]`.
    Call(String, Vec<Value>),
    Array(Vec<Value>),
    Dictionary(Vec<(Value, Value)>),
    /// `Object(Class,"prop":value,...)`: an inline object (input events in
    /// project settings).
    Object(String, Vec<(String, Value)>),
}

impl Value {
    pub fn call(name: &str, args: impl IntoIterator<Item = Value>) -> Self {
        Value::Call(name.to_string(), args.into_iter().collect())
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Int(i) => Some(*i as f64),
            Value::Float(f) => Some(*f),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            Value::Float(f) if f.fract() == 0.0 => Some(*f as i64),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// The text of a String, StringName or NodePath.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) | Value::StringName(s) | Value::NodePath(s) => Some(s),
            _ => None,
        }
    }

    /// Numeric arguments of `name(...)`, e.g. the components of `Vector2(...)`.
    pub fn numbers(&self, name: &str) -> Option<Vec<f64>> {
        match self {
            Value::Call(n, args) if n == name => args.iter().map(Value::as_f64).collect(),
            _ => None,
        }
    }

    /// The id inside `ExtResource("id")` or `SubResource("id")`.
    pub fn resource_ref(&self) -> Option<(&str, &str)> {
        match self {
            Value::Call(n, args) if (n == "ExtResource" || n == "SubResource") && args.len() == 1 => args[0].as_str().map(|id| (n.as_str(), id)),
            _ => None,
        }
    }

    pub fn vector2(x: f64, y: f64) -> Self {
        Value::call("Vector2", [Value::Float(x), Value::Float(y)])
    }

    pub fn vector3(x: f64, y: f64, z: f64) -> Self {
        Value::call("Vector3", [Value::Float(x), Value::Float(y), Value::Float(z)])
    }

    pub fn color(c: [f64; 4]) -> Self {
        Value::call("Color", c.map(Value::Float))
    }

    /// Parse one value from the start of `text`; returns it and the rest.
    pub fn parse_prefix(text: &str) -> Result<(Value, &str), String> {
        let mut p = Parser { s: text, pos: 0 };
        let v = p.value()?;
        Ok((v, &text[p.pos..]))
    }

    /// Parse exactly one value.
    pub fn parse(text: &str) -> Result<Value, String> {
        let (v, rest) = Self::parse_prefix(text)?;
        let mut p = Parser { s: rest, pos: 0 };
        p.skip_ws();
        if p.pos < rest.len() {
            return Err(format!("unexpected text after value: {:?}", &rest[p.pos..]));
        }
        Ok(v)
    }

    /// Write the value the way Godot's `VariantWriter` does.
    pub fn write(&self, out: &mut String) {
        match self {
            Value::Nil => out.push_str("null"),
            Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Value::Int(i) => {
                let _ = write!(out, "{i}");
            }
            Value::Float(f) => {
                let s = num(*f);
                out.push_str(&s);
                if !matches!(s.as_str(), "inf" | "-inf" | "nan") && !s.contains(['.', 'e', 'E']) {
                    out.push_str(".0");
                }
            }
            Value::String(s) => quote(out, s),
            Value::StringName(s) => {
                out.push('&');
                quote(out, s);
            }
            Value::NodePath(s) => {
                // Inside constructors Godot writes NodePath("..."); as a value, ^"...".
                out.push('^');
                quote(out, s);
            }
            Value::Call(name, args) => {
                out.push_str(name);
                out.push('(');
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    // Math constructor components are written without the ".0" suffix.
                    match a {
                        Value::Float(f) => out.push_str(&num(*f)),
                        other => other.write(out),
                    }
                }
                out.push(')');
            }
            Value::Object(class, props) => {
                out.push_str("Object(");
                out.push_str(class);
                out.push(',');
                for (i, (k, v)) in props.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    quote(out, k);
                    out.push(':');
                    v.write(out);
                }
                out.push_str(")\n");
            }
            Value::Array(items) => {
                out.push('[');
                for (i, a) in items.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    a.write(out);
                }
                out.push(']');
            }
            Value::Dictionary(pairs) => {
                if pairs.is_empty() {
                    out.push_str("{}");
                    return;
                }
                out.push_str("{\n");
                for (i, (k, v)) in pairs.iter().enumerate() {
                    k.write(out);
                    out.push_str(": ");
                    v.write(out);
                    if i + 1 < pairs.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                out.push('}');
            }
        }
    }

    pub fn to_text(&self) -> String {
        let mut s = String::new();
        self.write(&mut s);
        s
    }
}

/// Godot's `rtos_fix`: shortest text that reads back to the same float,
/// using the f32 form when the value is exactly an f32; no negative zero.
fn num(v: f64) -> String {
    if v == 0.0 {
        return "0".into();
    }
    if v.is_nan() {
        return "nan".into();
    }
    if v.is_infinite() {
        return if v > 0.0 { "inf".into() } else { "-inf".into() };
    }
    let s = if (v as f32) as f64 == v { format!("{}", v as f32) } else { format!("{v}") };
    // Very large or small magnitudes in scientific notation, like Godot.
    let a = v.abs();
    if !(1e-4..1e15).contains(&a) {
        let e = if (v as f32) as f64 == v { format!("{:e}", v as f32) } else { format!("{v:e}") };
        // Rust writes 1e-5, Godot writes 1e-05.
        return match e.split_once('e') {
            Some((m, exp)) => {
                let (sign, digits) = exp.strip_prefix('-').map_or(("+", exp), |d| ("-", d));
                format!("{m}e{sign}{digits:0>2}")
            }
            None => e,
        };
    }
    s
}

/// `c_escape_multiline`: escape backslashes and quotes, keep newlines.
fn quote(out: &mut String, s: &str) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            c => out.push(c),
        }
    }
    out.push('"');
}

struct Parser<'a> {
    s: &'a str,
    pos: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.s[self.pos..].chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.bump();
            } else if c == ';' {
                // Comment to end of line.
                while let Some(c) = self.bump() {
                    if c == '\n' {
                        break;
                    }
                }
            } else {
                break;
            }
        }
    }

    fn expect(&mut self, ch: char) -> Result<(), String> {
        self.skip_ws();
        match self.bump() {
            Some(c) if c == ch => Ok(()),
            other => Err(format!("expected '{ch}', found {other:?} at {}", self.pos)),
        }
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect('"')?;
        let mut out = String::new();
        loop {
            match self.bump() {
                None => return Err("unterminated string".into()),
                Some('"') => return Ok(out),
                Some('\\') => match self.bump() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some('r') => out.push('\r'),
                    Some('b') => out.push('\u{8}'),
                    Some('f') => out.push('\u{c}'),
                    Some('u') | Some('U') => {
                        let len = if self.s[..self.pos].ends_with('U') { 6 } else { 4 };
                        let hex = self.s.get(self.pos..self.pos + len).ok_or("bad unicode escape")?;
                        let c = u32::from_str_radix(hex, 16).ok().and_then(char::from_u32).ok_or("bad unicode escape")?;
                        self.pos += len;
                        out.push(c);
                    }
                    Some(c) => out.push(c),
                    None => return Err("unterminated string".into()),
                },
                Some(c) => out.push(c),
            }
        }
    }

    fn ident(&mut self) -> String {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' {
                self.bump();
            } else {
                break;
            }
        }
        self.s[start..self.pos].to_string()
    }

    fn list(&mut self, close: char) -> Result<Vec<Value>, String> {
        let mut items = Vec::new();
        loop {
            self.skip_ws();
            if self.peek() == Some(close) {
                self.bump();
                return Ok(items);
            }
            items.push(self.value()?);
            self.skip_ws();
            match self.bump() {
                Some(',') => {}
                Some(c) if c == close => return Ok(items),
                other => return Err(format!("expected ',' or '{close}', found {other:?}")),
            }
        }
    }

    fn value(&mut self) -> Result<Value, String> {
        self.skip_ws();
        let Some(c) = self.peek() else { return Err("expected a value".into()) };
        match c {
            '"' => Ok(Value::String(self.string()?)),
            '&' => {
                self.bump();
                Ok(Value::StringName(self.string()?))
            }
            '^' => {
                self.bump();
                Ok(Value::NodePath(self.string()?))
            }
            '[' => {
                self.bump();
                Ok(Value::Array(self.list(']')?))
            }
            '{' => {
                self.bump();
                let mut pairs = Vec::new();
                loop {
                    self.skip_ws();
                    if self.peek() == Some('}') {
                        self.bump();
                        return Ok(Value::Dictionary(pairs));
                    }
                    let k = self.value()?;
                    self.expect(':')?;
                    let v = self.value()?;
                    pairs.push((k, v));
                    self.skip_ws();
                    match self.bump() {
                        Some(',') => {}
                        Some('}') => return Ok(Value::Dictionary(pairs)),
                        other => return Err(format!("expected ',' or '}}', found {other:?}")),
                    }
                }
            }
            c if c == '-' || c == '+' || c == '.' || c.is_ascii_digit() => self.number(),
            c if c.is_alphabetic() || c == '_' => {
                let mut name = self.ident();
                match name.as_str() {
                    "true" => return Ok(Value::Bool(true)),
                    "false" => return Ok(Value::Bool(false)),
                    "null" | "nil" => return Ok(Value::Nil),
                    "inf" => return Ok(Value::Float(f64::INFINITY)),
                    "inf_neg" => return Ok(Value::Float(f64::NEG_INFINITY)),
                    "nan" => return Ok(Value::Float(f64::NAN)),
                    _ => {}
                }
                // Typed containers: Array[int](...), Dictionary[String, int](...).
                self.skip_ws();
                if self.peek() == Some('[') {
                    let start = self.pos;
                    let mut depth = 0;
                    while let Some(c) = self.bump() {
                        match c {
                            '[' => depth += 1,
                            ']' => {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                    name.push_str(&self.s[start..self.pos]);
                }
                self.expect('(')?;
                if name == "Object" {
                    self.skip_ws();
                    let class = self.ident();
                    let mut props = Vec::new();
                    loop {
                        self.skip_ws();
                        match self.bump() {
                            Some(',') => {
                                self.skip_ws();
                                if self.peek() == Some(')') {
                                    continue;
                                }
                                let k = self.string()?;
                                self.expect(':')?;
                                let v = self.value()?;
                                props.push((k, v));
                            }
                            Some(')') => return Ok(Value::Object(class, props)),
                            other => return Err(format!("expected ',' or ')' in Object(), found {other:?}")),
                        }
                    }
                }
                Ok(Value::Call(name, self.list(')')?))
            }
            other => Err(format!("unexpected character {other:?}")),
        }
    }

    fn number(&mut self) -> Result<Value, String> {
        let start = self.pos;
        if matches!(self.peek(), Some('-' | '+')) {
            self.bump();
        }
        if self.s[self.pos..].starts_with("inf") {
            self.pos += 3;
            return Ok(Value::Float(if self.s[start..].starts_with('-') { f64::NEG_INFINITY } else { f64::INFINITY }));
        }
        if self.s[self.pos..].starts_with("0x") || self.s[self.pos..].starts_with("0X") {
            self.pos += 2;
            let hs = self.pos;
            while self.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                self.bump();
            }
            let v = i64::from_str_radix(&self.s[hs..self.pos], 16).map_err(|e| e.to_string())?;
            return Ok(Value::Int(if self.s[start..].starts_with('-') { -v } else { v }));
        }
        let mut float = false;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                self.bump();
            } else if c == '.' || c == 'e' || c == 'E' {
                float = true;
                self.bump();
                if (c == 'e' || c == 'E') && matches!(self.peek(), Some('-' | '+')) {
                    self.bump();
                }
            } else {
                break;
            }
        }
        let t = &self.s[start..self.pos];
        if float {
            t.parse().map(Value::Float).map_err(|_| format!("bad number {t:?}"))
        } else {
            t.parse().map(Value::Int).or_else(|_| t.parse().map(Value::Float)).map_err(|_| format!("bad number {t:?}"))
        }
    }
}

/// Parse `key=value` pairs inside a section tag like `[node name="A" type="B"]`.
pub(crate) fn parse_tag_fields(mut s: &str) -> Result<Vec<(String, Value)>, String> {
    let mut out = Vec::new();
    loop {
        s = s.trim_start();
        if s.is_empty() {
            return Ok(out);
        }
        let eq = s.find('=').ok_or_else(|| format!("expected key=value in tag at {s:?}"))?;
        let key = s[..eq].trim().to_string();
        let (v, rest) = Value::parse_prefix(&s[eq + 1..])?;
        out.push((key, v));
        s = rest;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rt(text: &str) {
        let v = Value::parse(text).unwrap_or_else(|e| panic!("{text}: {e}"));
        assert_eq!(v.to_text(), text, "{v:?}");
    }

    #[test]
    fn round_trips_like_godot_writes() {
        rt("Vector2(300, 420)");
        rt("Vector2(0.5, -1.25)");
        rt("Color(1, 0.5, 0, 1)");
        rt("0.785398");
        rt("1.0");
        rt("-3");
        rt("\"Hello \\\"cat\\\"\"");
        rt("&\"enemies\"");
        rt("^\"Player/Sprite2D\"");
        rt("ExtResource(\"1_cat\")");
        rt("[1, 2.5, \"a\"]");
        rt("Array[StringName]([&\"a\", &\"b\"])");
        rt("PackedStringArray(\"4.7\", \"Forward Plus\")");
        rt("Transform3D(1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 2, 0)");
        rt("null");
        rt("{}");
    }

    #[test]
    fn inline_objects() {
        let text = "{\n\"deadzone\": 0.2,\n\"events\": [Object(InputEventKey,\"device\":-1,\"keycode\":0,\"unicode\":97)\n, Object(InputEventJoypadButton,\"button_index\":0)\n]\n}";
        let v = Value::parse(text).unwrap();
        assert_eq!(v.to_text(), text);
    }

    #[test]
    fn floats_match_rtos_fix() {
        assert_eq!(Value::Float(0.1).to_text(), "0.1");
        assert_eq!(Value::Float(2.0).to_text(), "2.0");
        assert_eq!(Value::Float(-0.0).to_text(), "0.0");
        assert_eq!(Value::vector2(-0.0, 1.0).to_text(), "Vector2(0, 1)");
        assert_eq!(Value::Float(0.123_456_789_012_345_6).to_text(), "0.1234567890123456");
        assert_eq!(Value::Float(0.123_457_f32 as f64).to_text(), "0.123457");
    }

    #[test]
    fn dictionaries_and_multiline() {
        let v = Value::parse("{\n\"a\": 1,\n\"b\": [2]\n}").unwrap();
        assert_eq!(v, Value::Dictionary(vec![(Value::String("a".into()), Value::Int(1)), (Value::String("b".into()), Value::Array(vec![Value::Int(2)]))]));
        assert_eq!(Value::parse(&v.to_text()).unwrap(), v);
    }

    #[test]
    fn tag_fields() {
        let f = parse_tag_fields("name=\"Player\" type=\"CharacterBody2D\" parent=\".\" unique_id=12345 groups=[\"a\"]").unwrap();
        assert_eq!(f[0], ("name".into(), Value::String("Player".into())));
        assert_eq!(f[3], ("unique_id".into(), Value::Int(12345)));
        assert_eq!(f.len(), 5);
    }
}
