//! Property values.

#[allow(unused_imports)]
use neko_base::prelude::*;

use alloc::borrow::Cow;
use core::fmt;

/// The type of a property, decided by its class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropType {
    Bool,
    Int,
    Float,
    String,
    /// A path to a resource such as `res://player.png`.
    Resource,
    Vec2,
    Vec3,
    Color,
}

/// A property value. `Str` uses `Cow` so class defaults can be `const`.
#[derive(Clone, Debug, PartialEq)]
pub enum Variant {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(Cow<'static, str>),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    /// Linear RGBA.
    Color([f32; 4]),
}

impl Variant {
    pub const fn str(s: &'static str) -> Self {
        Variant::Str(Cow::Borrowed(s))
    }

    pub fn as_vec2(&self) -> Option<[f32; 2]> {
        match self {
            Variant::Vec2(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Variant::Float(f) => Some(*f),
            Variant::Int(i) => Some(*i as f64),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Variant::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Variant::Str(s) => Some(s),
            _ => None,
        }
    }

    #[cfg(feature = "std")]
    pub fn to_toml(&self) -> toml::Value {
        use toml::Value as V;
        let arr = |v: &[f32]| V::Array(v.iter().map(|x| V::Float(round(*x as f64))).collect());
        match self {
            Variant::Bool(b) => V::Boolean(*b),
            Variant::Int(i) => V::Integer(*i),
            Variant::Float(f) => V::Float(round(*f)),
            Variant::Str(s) => V::String(s.to_string()),
            Variant::Vec2(v) => arr(v),
            Variant::Vec3(v) => arr(v),
            Variant::Color(v) => arr(v),
        }
    }

    /// Convert a TOML value to the given property type.
    #[cfg(feature = "std")]
    pub fn from_toml(ty: PropType, v: &toml::Value) -> Option<Self> {
        use toml::Value as V;
        let num = |v: &V| match v {
            V::Float(f) => Some(*f),
            V::Integer(i) => Some(*i as f64),
            _ => None,
        };
        let floats = |n: usize| -> Option<Vec<f32>> {
            let a = v.as_array()?;
            (a.len() == n).then_some(())?;
            a.iter().map(|x| num(x).map(|f| f as f32)).collect()
        };
        Some(match ty {
            PropType::Bool => Variant::Bool(v.as_bool()?),
            PropType::Int => Variant::Int(v.as_integer()?),
            PropType::Float => Variant::Float(num(v)?),
            PropType::String | PropType::Resource => Variant::Str(Cow::Owned(v.as_str()?.to_string())),
            PropType::Vec2 => {
                let f = floats(2)?;
                Variant::Vec2([f[0], f[1]])
            }
            PropType::Vec3 => {
                let f = floats(3)?;
                Variant::Vec3([f[0], f[1], f[2]])
            }
            PropType::Color => {
                let f = floats(4)?;
                Variant::Color([f[0], f[1], f[2], f[3]])
            }
        })
    }

    pub fn type_matches(&self, ty: PropType) -> bool {
        matches!(
            (self, ty),
            (Variant::Bool(_), PropType::Bool)
                | (Variant::Int(_), PropType::Int)
                | (Variant::Float(_), PropType::Float)
                | (Variant::Str(_), PropType::String | PropType::Resource)
                | (Variant::Vec2(_), PropType::Vec2)
                | (Variant::Vec3(_), PropType::Vec3)
                | (Variant::Color(_), PropType::Color)
        )
    }
}

/// Keep saved files readable: 0.1 instead of 0.10000000149011612.
#[cfg(feature = "std")]
fn round(f: f64) -> f64 {
    (f * 1e5).round() / 1e5
}

impl fmt::Display for Variant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Variant::Bool(b) => write!(f, "{b}"),
            Variant::Int(i) => write!(f, "{i}"),
            Variant::Float(x) => write!(f, "{x}"),
            Variant::Str(s) => write!(f, "{s:?}"),
            Variant::Vec2([x, y]) => write!(f, "({x}, {y})"),
            Variant::Vec3([x, y, z]) => write!(f, "({x}, {y}, {z})"),
            Variant::Color([r, g, b, a]) => write!(f, "rgba({r}, {g}, {b}, {a})"),
        }
    }
}
