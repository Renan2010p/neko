//! Runtime values: Godot's `Variant` for the types scripts use.

use alloc::rc::Rc;
use core::cell::RefCell;
use core::fmt;

use neko_base::collections::HashMap;
#[allow(unused_imports)]
use neko_base::prelude::*;
#[cfg(not(feature = "std"))]
use neko_base::math::FloatExt;

use crate::interp::{Callable, Object};

/// A dynamically typed value (Godot's `Variant`).
#[derive(Clone, Default)]
pub enum Value {
    #[default]
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(Rc<str>),
    StringName(Rc<str>),
    NodePath(Rc<str>),
    Vector2(f32, f32),
    Vector2i(i32, i32),
    Vector3(f32, f32, f32),
    Vector3i(i32, i32, i32),
    Vector4(f32, f32, f32, f32),
    Rect2(f32, f32, f32, f32),
    Color(f32, f32, f32, f32),
    /// 3x3 row-major basis, like Godot's `Basis`.
    Basis([[f32; 3]; 3]),
    /// Godot's `Transform3D`: basis and origin.
    Transform3D { basis: [[f32; 3]; 3], origin: [f32; 3] },
    /// Godot's `AABB`: position and size.
    AABB { position: [f32; 3], size: [f32; 3] },
    /// Arrays are shared references, like in Godot.
    Array(Rc<RefCell<Vec<Value>>>),
    Dictionary(Rc<RefCell<Dict>>),
    Object(Object),
    Callable(Rc<Callable>),
    /// A signal of an object: `obj.signal_name`.
    Signal(Object, Rc<str>),
}

impl Value {
    pub fn str(s: &str) -> Self {
        Value::String(Rc::from(s))
    }

    pub fn array(v: Vec<Value>) -> Self {
        Value::Array(Rc::new(RefCell::new(v)))
    }

    pub fn dict(d: Dict) -> Self {
        Value::Dictionary(Rc::new(RefCell::new(d)))
    }

    /// Godot's `typeof()` numbers (Variant::Type).
    pub fn type_id(&self) -> i64 {
        match self {
            Value::Nil => 0,
            Value::Bool(_) => 1,
            Value::Int(_) => 2,
            Value::Float(_) => 3,
            Value::String(_) => 4,
            Value::Vector2(..) => 5,
            Value::Vector2i(..) => 6,
            Value::Rect2(..) => 7,
            Value::Vector3(..) => 9,
            Value::Vector3i(..) => 10,
            Value::Vector4(..) => 12,
            Value::AABB { .. } => 16,
            Value::Basis(..) => 17,
            Value::Transform3D { .. } => 18,
            Value::Color(..) => 20,
            Value::StringName(_) => 21,
            Value::NodePath(_) => 22,
            Value::Object(_) => 24,
            Value::Callable(_) => 25,
            Value::Signal(..) => 26,
            Value::Dictionary(_) => 27,
            Value::Array(_) => 28,
        }
    }

    /// Name of the type, as in Godot error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Nil => "null",
            Value::Bool(_) => "bool",
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::String(_) => "String",
            Value::StringName(_) => "StringName",
            Value::NodePath(_) => "NodePath",
            Value::Vector2(..) => "Vector2",
            Value::Vector2i(..) => "Vector2i",
            Value::Vector3(..) => "Vector3",
            Value::Vector3i(..) => "Vector3i",
            Value::Vector4(..) => "Vector4",
            Value::Rect2(..) => "Rect2",
            Value::Color(..) => "Color",
            Value::Basis(..) => "Basis",
            Value::Transform3D { .. } => "Transform3D",
            Value::AABB { .. } => "AABB",
            Value::Array(_) => "Array",
            Value::Dictionary(_) => "Dictionary",
            Value::Object(_) => "Object",
            Value::Callable(_) => "Callable",
            Value::Signal(..) => "Signal",
        }
    }

    /// Godot's truthiness (`Variant::booleanize`).
    pub fn truthy(&self) -> bool {
        match self {
            Value::Nil => false,
            Value::Bool(b) => *b,
            Value::Int(i) => *i != 0,
            Value::Float(f) => *f != 0.0,
            Value::String(s) | Value::StringName(s) | Value::NodePath(s) => !s.is_empty(),
            Value::Vector2(x, y) => *x != 0.0 || *y != 0.0,
            Value::Vector2i(x, y) => *x != 0 || *y != 0,
            Value::Vector3(x, y, z) => *x != 0.0 || *y != 0.0 || *z != 0.0,
            Value::Vector3i(x, y, z) => *x != 0 || *y != 0 || *z != 0,
            Value::Vector4(x, y, z, w) => [*x, *y, *z, *w].iter().any(|c| *c != 0.0),
            Value::Rect2(..) | Value::Color(..) | Value::Basis(..) | Value::Transform3D { .. } | Value::AABB { .. } => true,
            Value::Array(a) => !a.borrow().is_empty(),
            Value::Dictionary(d) => !d.borrow().is_empty(),
            Value::Object(o) => o.is_valid(),
            Value::Callable(_) | Value::Signal(..) => true,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Int(i) => Some(*i as f64),
            Value::Float(f) => Some(*f),
            Value::Bool(b) => Some(*b as i64 as f64),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            Value::Float(f) => Some(*f as i64),
            Value::Bool(b) => Some(*b as i64),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) | Value::StringName(s) | Value::NodePath(s) => Some(s),
            _ => None,
        }
    }

    pub fn is_string_like(&self) -> bool {
        matches!(self, Value::String(_) | Value::StringName(_) | Value::NodePath(_))
    }

    /// Hashable form for dictionary keys.
    pub fn key(&self) -> Key {
        match self {
            Value::Nil => Key::Nil,
            Value::Bool(b) => Key::Int(*b as i64),
            Value::Int(i) => Key::Int(*i),
            Value::Float(f) => Key::Float(f.to_bits()),
            // String and StringName keys are interchangeable in Godot 4 lookups.
            Value::String(s) | Value::StringName(s) => Key::Str(s.clone()),
            Value::NodePath(s) => Key::Path(s.clone()),
            Value::Vector2(x, y) => Key::Vec([x.to_bits(), y.to_bits(), 0, 0], 2),
            Value::Vector2i(x, y) => Key::Vec([*x as u32, *y as u32, 0, 0], 12),
            Value::Vector3(x, y, z) => Key::Vec([x.to_bits(), y.to_bits(), z.to_bits(), 0], 3),
            Value::Vector3i(x, y, z) => Key::Vec([*x as u32, *y as u32, *z as u32, 0], 13),
            Value::Vector4(x, y, z, w) => Key::Vec([x.to_bits(), y.to_bits(), z.to_bits(), w.to_bits()], 4),
            Value::Rect2(x, y, w, h) => Key::Vec([x.to_bits(), y.to_bits(), w.to_bits(), h.to_bits()], 5),
            Value::Color(r, g, b, a) => Key::Vec([r.to_bits(), g.to_bits(), b.to_bits(), a.to_bits()], 6),
            Value::Basis(_) | Value::Transform3D { .. } | Value::AABB { .. } => Key::Str(Rc::from(self.to_string().as_str())),
            Value::Object(o) => Key::Ptr(o.id()),
            Value::Array(a) => Key::Ptr(Rc::as_ptr(a) as usize as u64),
            Value::Dictionary(d) => Key::Ptr(Rc::as_ptr(d) as usize as u64),
            Value::Callable(c) => Key::Ptr(Rc::as_ptr(c) as usize as u64),
            Value::Signal(o, n) => Key::Signal(o.id(), n.clone()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Nil,
    Int(i64),
    Float(u64),
    Str(Rc<str>),
    Path(Rc<str>),
    Vec([u32; 4], u8),
    Ptr(u64),
    Signal(u64, Rc<str>),
}

/// An insertion-ordered dictionary, like Godot's.
#[derive(Clone, Default)]
pub struct Dict {
    entries: Vec<(Value, Value)>,
    index: HashMap<Key, usize>,
}

impl Dict {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, k: &Value) -> Option<&Value> {
        self.index.get(&k.key()).map(|&i| &self.entries[i].1)
    }

    pub fn has(&self, k: &Value) -> bool {
        self.index.contains_key(&k.key())
    }

    pub fn set(&mut self, k: Value, v: Value) {
        match self.index.get(&k.key()) {
            Some(&i) => self.entries[i].1 = v,
            None => {
                self.index.insert(k.key(), self.entries.len());
                self.entries.push((k, v));
            }
        }
    }

    pub fn erase(&mut self, k: &Value) -> bool {
        let Some(i) = self.index.remove(&k.key()) else { return false };
        self.entries.remove(i);
        for idx in self.index.values_mut() {
            if *idx > i {
                *idx -= 1;
            }
        }
        true
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.index.clear();
    }

    pub fn iter(&self) -> impl Iterator<Item = &(Value, Value)> {
        self.entries.iter()
    }

    pub fn keys(&self) -> Vec<Value> {
        self.entries.iter().map(|(k, _)| k.clone()).collect()
    }

    pub fn values(&self) -> Vec<Value> {
        self.entries.iter().map(|(_, v)| v.clone()).collect()
    }
}

/// Godot's float to text (`String::num`, as `print` shows floats).
pub fn float_str(f: f64) -> String {
    if f.is_nan() {
        return "nan".into();
    }
    if f.is_infinite() {
        return if f > 0.0 { "inf".into() } else { "-inf".into() };
    }
    if f == f.trunc() && f.abs() < 1e15 {
        return format!("{f:.1}");
    }
    // Godot prints up to 14 significant decimals, trimmed.
    let s = format!("{:.*}", (14 - f.abs().log10().floor().max(0.0) as usize).min(14), f);
    let s = s.trim_end_matches('0');
    if s.ends_with('.') { format!("{s}0") } else { s.to_string() }
}

/// Vector components (`String::num_real`, fewer digits since they are 32-bit).
fn real_str(f: f32) -> String {
    let d = f as f64;
    if d == d.trunc() && d.abs() < 1e15 {
        return format!("{d:.1}");
    }
    let s = format!("{:.6}", d);
    let s = s.trim_end_matches('0');
    if s.ends_with('.') { format!("{s}0") } else { s.to_string() }
}

impl fmt::Display for Value {
    /// What `str()` and `print()` produce.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Nil => write!(f, "<null>"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Int(i) => write!(f, "{i}"),
            Value::Float(x) => write!(f, "{}", float_str(*x)),
            Value::String(s) | Value::StringName(s) | Value::NodePath(s) => write!(f, "{s}"),
            Value::Vector2(x, y) => write!(f, "({}, {})", real_str(*x), real_str(*y)),
            Value::Vector2i(x, y) => write!(f, "({x}, {y})"),
            Value::Vector3(x, y, z) => write!(f, "({}, {}, {})", real_str(*x), real_str(*y), real_str(*z)),
            Value::Vector3i(x, y, z) => write!(f, "({x}, {y}, {z})"),
            Value::Vector4(x, y, z, w) => write!(f, "({}, {}, {}, {})", real_str(*x), real_str(*y), real_str(*z), real_str(*w)),
            Value::Rect2(x, y, w, h) => write!(f, "[P: ({}, {}), S: ({}, {})]", real_str(*x), real_str(*y), real_str(*w), real_str(*h)),
            Value::Color(r, g, b, a) => write!(f, "({}, {}, {}, {})", real_str(*r), real_str(*g), real_str(*b), real_str(*a)),
            Value::Basis(b) => write!(
                f,
                "[X: ({}, {}, {}), Y: ({}, {}, {}), Z: ({}, {}, {})]",
                real_str(b[0][0]), real_str(b[0][1]), real_str(b[0][2]),
                real_str(b[1][0]), real_str(b[1][1]), real_str(b[1][2]),
                real_str(b[2][0]), real_str(b[2][1]), real_str(b[2][2])
            ),
            Value::Transform3D { basis, origin } => write!(
                f,
                "[X: ({}, {}, {}), Y: ({}, {}, {}), Z: ({}, {}, {}), O: ({}, {}, {})]",
                real_str(basis[0][0]), real_str(basis[0][1]), real_str(basis[0][2]),
                real_str(basis[1][0]), real_str(basis[1][1]), real_str(basis[1][2]),
                real_str(basis[2][0]), real_str(basis[2][1]), real_str(basis[2][2]),
                real_str(origin[0]), real_str(origin[1]), real_str(origin[2])
            ),
            Value::AABB { position, size } => write!(
                f,
                "[P: ({}, {}, {}), S: ({}, {}, {})]",
                real_str(position[0]), real_str(position[1]), real_str(position[2]),
                real_str(size[0]), real_str(size[1]), real_str(size[2])
            ),
            Value::Array(a) => {
                write!(f, "[")?;
                for (i, v) in a.borrow().iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    v.fmt_nested(f)?;
                }
                write!(f, "]")
            }
            Value::Dictionary(d) => {
                write!(f, "{{ ")?;
                for (i, (k, v)) in d.borrow().iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    k.fmt_nested(f)?;
                    write!(f, ": ")?;
                    v.fmt_nested(f)?;
                }
                if d.borrow().is_empty() { write!(f, "}}") } else { write!(f, " }}") }
            }
            Value::Object(o) => write!(f, "{}", o.describe()),
            Value::Callable(c) => write!(f, "{}", c.describe()),
            Value::Signal(o, n) => write!(f, "{}::[signal]{n}", o.describe()),
        }
    }
}

impl Value {
    /// Strings inside containers are quoted, like Godot prints them.
    fn fmt_nested(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::String(s) => write!(f, "\"{s}\""),
            Value::StringName(s) => write!(f, "&\"{s}\""),
            Value::NodePath(s) => write!(f, "^\"{s}\""),
            other => write!(f, "{other}"),
        }
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}({self})", self.type_name())
    }
}

/// Godot's `==` between variants (numbers compare across int/float).
pub fn equals(a: &Value, b: &Value) -> bool {
    use Value::*;
    match (a, b) {
        (Nil, Nil) => true,
        (Bool(x), Bool(y)) => x == y,
        (Int(x), Int(y)) => x == y,
        (Int(_) | Float(_), Int(_) | Float(_)) => a.as_f64() == b.as_f64(),
        (String(x) | StringName(x), String(y) | StringName(y)) => x == y,
        (NodePath(x), NodePath(y)) => x == y,
        (Vector2(a1, a2), Vector2(b1, b2)) => a1 == b1 && a2 == b2,
        (Vector2i(a1, a2), Vector2i(b1, b2)) => a1 == b1 && a2 == b2,
        (Vector3(a1, a2, a3), Vector3(b1, b2, b3)) => a1 == b1 && a2 == b2 && a3 == b3,
        (Vector3i(a1, a2, a3), Vector3i(b1, b2, b3)) => a1 == b1 && a2 == b2 && a3 == b3,
        (Vector4(..), Vector4(..)) | (Rect2(..), Rect2(..)) | (Color(..), Color(..)) | (Basis(..), Basis(..)) | (Transform3D { .. }, Transform3D { .. }) | (AABB { .. }, AABB { .. }) => a.key() == b.key(),
        (Array(x), Array(y)) => Rc::ptr_eq(x, y) || {
            let (x, y) = (x.borrow(), y.borrow());
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(p, q)| equals(p, q))
        },
        (Dictionary(x), Dictionary(y)) => Rc::ptr_eq(x, y) || {
            let (x, y) = (x.borrow(), y.borrow());
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| equals(v, w)))
        },
        (Object(x), Object(y)) => x.id() == y.id(),
        (Object(o), Nil) | (Nil, Object(o)) => !o.is_valid(),
        (Callable(x), Callable(y)) => Rc::ptr_eq(x, y) || x.same_target(y),
        (Signal(o1, n1), Signal(o2, n2)) => o1.id() == o2.id() && n1 == n2,
        _ => false,
    }
}
