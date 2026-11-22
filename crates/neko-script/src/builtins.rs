//! Built-in operators, utility functions (Godot's `@GlobalScope`), and the
//! methods of built-in types (String, Array, Dictionary, vectors, Color).

use alloc::rc::Rc;

#[allow(unused_imports)]
use neko_base::prelude::*;
#[cfg(not(feature = "std"))]
use neko_base::math::FloatExt;

use crate::ast::BinOp;
use crate::interp::{Callable, Interp, ObjKind, Object, RResult, ScriptError};
use crate::value::{Dict, Value, equals, float_str};

type SResult = Result<Value, String>;

fn err<T>(m: impl Into<String>) -> Result<T, String> {
    Err(m.into())
}

fn ierr<T>(it: &Interp, m: impl Into<String>) -> RResult<T> {
    let _ = it;
    Err(ScriptError { message: m.into(), file: String::new(), line: 0, func: String::new() })
}

pub fn to_int(v: &Value) -> Value {
    match v {
        Value::Int(i) => Value::Int(*i),
        Value::Float(f) => Value::Int(*f as i64),
        Value::Bool(b) => Value::Int(*b as i64),
        Value::String(s) | Value::StringName(s) => Value::Int(string_to_int(s)),
        _ => Value::Int(0),
    }
}

pub fn to_float(v: &Value) -> Value {
    match v {
        Value::Int(i) => Value::Float(*i as f64),
        Value::Float(f) => Value::Float(*f),
        Value::Bool(b) => Value::Float(*b as i64 as f64),
        Value::String(s) | Value::StringName(s) => Value::Float(s.trim().parse().unwrap_or(0.0)),
        _ => Value::Float(0.0),
    }
}

/// `String.to_int()`: leading sign and digits, ignoring the rest.
fn string_to_int(s: &str) -> i64 {
    let s = s.trim();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_ascii_digit() || (i == 0 && (c == '-' || c == '+')) {
            out.push(c);
        } else if c == '.' {
            break;
        }
    }
    out.parse().unwrap_or(0)
}

pub fn negate(v: &Value) -> Value {
    match v {
        Value::Int(i) => Value::Int(i.wrapping_neg()),
        Value::Float(f) => Value::Float(-f),
        Value::Vector2(x, y) => Value::Vector2(-x, -y),
        Value::Vector2i(x, y) => Value::Vector2i(-x, -y),
        Value::Vector3(x, y, z) => Value::Vector3(-x, -y, -z),
        Value::Vector3i(x, y, z) => Value::Vector3i(-x, -y, -z),
        Value::Vector4(x, y, z, w) => Value::Vector4(-x, -y, -z, -w),
        Value::Color(r, g, b, a) => Value::Color(-r, -g, -b, -a),
        other => other.clone(),
    }
}

// ------------------------------------------------------------ operators

fn vec_op(op: BinOp, a: &[f32], b: &[f32]) -> Option<Vec<f32>> {
    let f = |x: f32, y: f32| match op {
        BinOp::Add => Some(x + y),
        BinOp::Sub => Some(x - y),
        BinOp::Mul => Some(x * y),
        BinOp::Div => Some(x / y),
        BinOp::Mod => Some(x % y),
        _ => None,
    };
    a.iter().zip(b).map(|(x, y)| f(*x, *y)).collect()
}

fn floats(v: &Value) -> Option<(Vec<f32>, u8)> {
    Some(match v {
        Value::Vector2(x, y) => (vec![*x, *y], 2),
        Value::Vector3(x, y, z) => (vec![*x, *y, *z], 3),
        Value::Vector4(x, y, z, w) => (vec![*x, *y, *z, *w], 4),
        Value::Color(r, g, b, a) => (vec![*r, *g, *b, *a], 20),
        Value::Vector2i(x, y) => (vec![*x as f32, *y as f32], 6),
        Value::Vector3i(x, y, z) => (vec![*x as f32, *y as f32, *z as f32], 10),
        _ => return None,
    })
}

fn rebuild(kind: u8, c: &[f32]) -> Value {
    match kind {
        2 => Value::Vector2(c[0], c[1]),
        3 => Value::Vector3(c[0], c[1], c[2]),
        4 => Value::Vector4(c[0], c[1], c[2], c[3]),
        20 => Value::Color(c[0], c[1], c[2], c[3]),
        6 => Value::Vector2i(c[0] as i32, c[1] as i32),
        _ => Value::Vector3i(c[0] as i32, c[1] as i32, c[2] as i32),
    }
}

/// Binary operators with Godot's typing rules.
pub fn binary(op: BinOp, a: &Value, b: &Value) -> SResult {
    use Value::*;
    let bad = || err(format!("Invalid operands '{}' and '{}' in operator '{}'.", a.type_name(), b.type_name(), op_name(op)));
    match op {
        BinOp::Eq => return Ok(Bool(equals(a, b))),
        BinOp::Ne => return Ok(Bool(!equals(a, b))),
        BinOp::In | BinOp::NotIn => {
            let r = match b {
                Array(arr) => arr.borrow().iter().any(|x| equals(x, a)),
                Dictionary(d) => d.borrow().has(a),
                String(s) | StringName(s) => match a.as_str() {
                    Some(n) => s.contains(n),
                    None => return bad(),
                },
                _ => return bad(),
            };
            return Ok(Bool(r == (op == BinOp::In)));
        }
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
            let ord = match (a, b) {
                (Int(x), Int(y)) => x.partial_cmp(y),
                (Int(_) | Float(_), Int(_) | Float(_)) => a.as_f64().unwrap_or(0.0).partial_cmp(&b.as_f64().unwrap_or(0.0)),
                (String(x) | StringName(x), String(y) | StringName(y)) => x.partial_cmp(y),
                _ => match (floats(a), floats(b)) {
                    (Some((x, k1)), Some((y, k2))) if k1 == k2 => x.partial_cmp(&y),
                    _ => return bad(),
                },
            };
            let Some(o) = ord else { return Ok(Bool(false)) };
            use core::cmp::Ordering::*;
            return Ok(Bool(match op {
                BinOp::Lt => o == Less,
                BinOp::Le => o != Greater,
                BinOp::Gt => o == Greater,
                _ => o != Less,
            }));
        }
        _ => {}
    }
    let basis_mul = |x: &[[f32; 3]; 3], y: &[[f32; 3]; 3]| -> [[f32; 3]; 3] {
        let mut out = [[0.0; 3]; 3];
        for i in 0..3 {
            for j in 0..3 {
                out[i][j] = (0..3).map(|k| x[i][k] * y[k][j]).sum();
            }
        }
        out
    };
    let basis_vec = |m: &[[f32; 3]; 3], v: [f32; 3]| -> [f32; 3] {
        [0, 1, 2].map(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2])
    };
    match (a, b) {
        (Transform3D { basis, origin }, Transform3D { basis: b2, origin: o2 }) if op == BinOp::Mul => {
            let b = basis_mul(basis, b2);
            let o = basis_vec(basis, *o2);
            Ok(Transform3D { basis: b, origin: [o[0] + origin[0], o[1] + origin[1], o[2] + origin[2]] })
        }
        (Transform3D { basis, origin }, Vector3(x, y, z)) if op == BinOp::Mul => {
            let o = basis_vec(basis, [*x, *y, *z]);
            Ok(Vector3(o[0] + origin[0], o[1] + origin[1], o[2] + origin[2]))
        }
        (Basis(m), Vector3(x, y, z)) if op == BinOp::Mul => {
            let o = basis_vec(m, [*x, *y, *z]);
            Ok(Vector3(o[0], o[1], o[2]))
        }
        (Basis(x), Basis(y)) if op == BinOp::Mul => Ok(Basis(basis_mul(x, y))),
        (Int(x), Int(y)) => Ok(match op {
            BinOp::Add => Int(x.wrapping_add(*y)),
            BinOp::Sub => Int(x.wrapping_sub(*y)),
            BinOp::Mul => Int(x.wrapping_mul(*y)),
            BinOp::Div => {
                if *y == 0 {
                    return err("Division by zero error in operator '/'.");
                }
                Int(x.wrapping_div(*y))
            }
            BinOp::Mod => {
                if *y == 0 {
                    return err("Modulo by zero error in operator '%'.");
                }
                Int(x.wrapping_rem(*y))
            }
            BinOp::Pow => {
                if *y >= 0 {
                    Int(x.wrapping_pow(*y as u32))
                } else {
                    Float((*x as f64).powf(*y as f64))
                }
            }
            BinOp::Shl => Int(x.wrapping_shl(*y as u32)),
            BinOp::Shr => Int(x.wrapping_shr(*y as u32)),
            BinOp::BitAnd => Int(x & y),
            BinOp::BitOr => Int(x | y),
            BinOp::BitXor => Int(x ^ y),
            _ => return bad(),
        }),
        (Int(_) | Float(_), Int(_) | Float(_)) => {
            let (x, y) = (a.as_f64().unwrap_or(0.0), b.as_f64().unwrap_or(0.0));
            Ok(Float(match op {
                BinOp::Add => x + y,
                BinOp::Sub => x - y,
                BinOp::Mul => x * y,
                BinOp::Div => x / y,
                BinOp::Mod => x % y,
                BinOp::Pow => x.powf(y),
                _ => return bad(),
            }))
        }
        (String(x) | StringName(x), String(y) | StringName(y)) if op == BinOp::Add => Ok(Value::str(&format!("{x}{y}"))),
        (String(x), _) if op == BinOp::Mod => format_string(x, b).map(|s| Value::str(&s)),
        (Array(x), Array(y)) if op == BinOp::Add => {
            let mut v = x.borrow().clone();
            v.extend(y.borrow().iter().cloned());
            Ok(Value::array(v))
        }
        _ => {
            // Vector/Color with vector or scalar.
            if let (Some((x, k1)), Some((y, k2))) = (floats(a), floats(b))
                && k1 == k2
            {
                let int_kind = k1 == 6 || k1 == 10;
                if int_kind && op == BinOp::Div && y.contains(&0.0) {
                    return err("Division by zero error in operator '/'.");
                }
                return vec_op(op, &x, &y).map(|c| rebuild(k1, &c)).ok_or(()).or_else(|_| bad());
            }
            if let (Some((x, k)), Some(s)) = (floats(a), b.as_f64()) {
                if !matches!(op, BinOp::Mul | BinOp::Div | BinOp::Mod) {
                    return bad();
                }
                let int_kind = k == 6 || k == 10;
                if int_kind && !matches!(b, Int(_)) {
                    // Vector2i * float -> Vector2 (Godot promotes).
                    let kind = if k == 6 { 2 } else { 3 };
                    let ys = vec![s as f32; x.len()];
                    return vec_op(op, &x, &ys).map(|c| rebuild(kind, &c)).ok_or(()).or_else(|_| bad());
                }
                let ys = vec![s as f32; x.len()];
                return vec_op(op, &x, &ys).map(|c| rebuild(k, &c)).ok_or(()).or_else(|_| bad());
            }
            if let (Some(s), Some((y, k))) = (a.as_f64(), floats(b))
                && op == BinOp::Mul
            {
                let xs = vec![s as f32; y.len()];
                return vec_op(op, &xs, &y).map(|c| rebuild(k, &c)).ok_or(()).or_else(|_| bad());
            }
            bad()
        }
    }
}

fn op_name(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
        BinOp::Pow => "**",
        BinOp::Shl => "<<",
        BinOp::Shr => ">>",
        BinOp::BitAnd => "&",
        BinOp::BitOr => "|",
        BinOp::BitXor => "^",
        BinOp::And => "and",
        BinOp::Or => "or",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::In => "in",
        BinOp::NotIn => "not in",
    }
}

/// GDScript's `"%s %d" % args` (String::sprintf).
pub fn format_string(fmt: &str, args: &Value) -> Result<String, String> {
    let list: Vec<Value> = match args {
        Value::Array(a) => a.borrow().clone(),
        other => vec![other.clone()],
    };
    let mut out = String::new();
    let mut it = list.iter();
    let chars: Vec<char> = fmt.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c != '%' {
            out.push(c);
            i += 1;
            continue;
        }
        i += 1;
        if i >= chars.len() {
            return err("incomplete format");
        }
        if chars[i] == '%' {
            out.push('%');
            i += 1;
            continue;
        }
        // Flags, width, precision.
        let mut left = false;
        let mut zero = false;
        let mut plus = false;
        let mut width = String::new();
        let mut prec: Option<usize> = None;
        loop {
            match chars.get(i) {
                Some('-') => left = true,
                Some('0') if width.is_empty() && prec.is_none() => zero = true,
                Some('+') => plus = true,
                Some(d) if d.is_ascii_digit() && prec.is_none() => width.push(*d),
                Some('.') => {
                    i += 1;
                    let mut p = String::new();
                    while let Some(d) = chars.get(i).filter(|d| d.is_ascii_digit()) {
                        p.push(*d);
                        i += 1;
                    }
                    prec = Some(p.parse().unwrap_or(0));
                    continue;
                }
                Some('*') => {
                    let w = it.next().and_then(Value::as_i64).unwrap_or(0);
                    width = w.to_string();
                }
                _ => break,
            }
            i += 1;
        }
        let Some(&kind) = chars.get(i) else { return err("incomplete format") };
        i += 1;
        let Some(arg) = it.next() else { return err("not enough arguments for format string") };
        let mut s = match kind {
            's' => arg.to_string(),
            'd' | 'i' => {
                let n = arg.as_i64().ok_or("a number is required")?;
                if plus && n >= 0 { format!("+{n}") } else { n.to_string() }
            }
            'f' => {
                let n = arg.as_f64().ok_or("a number is required")?;
                let s = format!("{:.*}", prec.unwrap_or(6), n);
                if plus && n >= 0.0 { format!("+{s}") } else { s }
            }
            'x' => format!("{:x}", arg.as_i64().ok_or("a number is required")?),
            'X' => format!("{:X}", arg.as_i64().ok_or("a number is required")?),
            'o' => format!("{:o}", arg.as_i64().ok_or("a number is required")?),
            'c' => match arg {
                Value::Int(i) => char::from_u32(*i as u32).map(String::from).unwrap_or_default(),
                other => other.to_string().chars().next().map(String::from).unwrap_or_default(),
            },
            'v' => arg.to_string(),
            other => return err(format!("unsupported format character '{other}'")),
        };
        let w: usize = width.parse().unwrap_or(0);
        let len = s.chars().count();
        if len < w {
            let pad = w - len;
            if left {
                s.push_str(&" ".repeat(pad));
            } else if zero && kind != 's' {
                let (sign, digits) = if s.starts_with(['-', '+']) { s.split_at(1) } else { ("", s.as_str()) };
                s = format!("{sign}{}{digits}", "0".repeat(pad));
            } else {
                s = format!("{}{s}", " ".repeat(pad));
            }
        }
        out.push_str(&s);
    }
    if it.next().is_some() {
        return err("not all arguments converted during string formatting");
    }
    Ok(out)
}

// ------------------------------------------------------------ globals

const UTILITIES: &[&str] = &[
    "print", "prints", "printt", "printerr", "print_rich", "print_verbose", "push_error", "push_warning", "str", "len", "range", "abs", "absf",
    "absi", "sign", "signf", "signi", "min", "max", "minf", "maxf", "mini", "maxi", "clamp", "clampf", "clampi", "lerp", "lerpf", "lerp_angle",
    "inverse_lerp", "remap", "move_toward", "smoothstep", "sin", "cos", "tan", "asin", "acos", "atan", "atan2", "sinh", "cosh", "tanh", "sqrt",
    "pow", "exp", "log", "floor", "floorf", "floori", "ceil", "ceilf", "ceili", "round", "roundf", "roundi", "fmod", "fposmod", "posmod",
    "snapped", "snappedf", "snappedi", "deg_to_rad", "rad_to_deg", "randi", "randf", "randi_range", "randf_range", "randfn", "randomize", "seed",
    "is_nan", "is_inf", "is_finite", "is_equal_approx", "is_zero_approx", "typeof", "type_string", "wrapi", "wrapf", "wrap", "ease",
    "is_instance_valid", "instance_from_id", "char", "ord", "hash", "var_to_str", "str_to_var", "load", "weakref", "get_stack", "print_stack",
    "is_same", "rid_allocate_id", "error_string", "linear_to_db", "db_to_linear", "step_decimals", "nearest_po2", "type_exists", "bytes_to_var",
    "var_to_bytes", "is_instance_of", "convert", "pingpong", "rotate_toward", "bezier_interpolate", "cubic_interpolate", "angle_difference",
];

pub fn is_utility(n: &str) -> bool {
    UTILITIES.contains(&n)
}

const BUILTIN_TYPES: &[&str] = &[
    "int", "float", "bool", "String", "StringName", "NodePath", "Vector2", "Vector2i", "Vector3", "Vector3i", "Vector4", "Vector4i", "Rect2",
    "Rect2i", "Color", "Array", "Dictionary", "Callable", "Signal", "Object", "Transform2D", "Transform3D", "Basis", "Quaternion", "Plane", "AABB",
    "Projection", "RID", "PackedByteArray", "PackedInt32Array", "PackedInt64Array", "PackedFloat32Array", "PackedFloat64Array",
    "PackedStringArray", "PackedVector2Array", "PackedVector3Array", "PackedColorArray", "PackedVector4Array",
];

pub fn is_builtin_type(n: &str) -> bool {
    BUILTIN_TYPES.contains(&n)
}

/// `x is int` and friends for built-in types; `None` for class names.
pub fn is_builtin_instance(v: &Value, ty: &str) -> Option<bool> {
    Some(match ty {
        "int" => matches!(v, Value::Int(_)),
        "float" => matches!(v, Value::Float(_)),
        "bool" => matches!(v, Value::Bool(_)),
        "String" => matches!(v, Value::String(_)),
        "StringName" => matches!(v, Value::StringName(_)),
        "NodePath" => matches!(v, Value::NodePath(_)),
        "Vector2" => matches!(v, Value::Vector2(..)),
        "Vector2i" => matches!(v, Value::Vector2i(..)),
        "Vector3" => matches!(v, Value::Vector3(..)),
        "Vector3i" => matches!(v, Value::Vector3i(..)),
        "Vector4" => matches!(v, Value::Vector4(..)),
        "Rect2" => matches!(v, Value::Rect2(..)),
        "Color" => matches!(v, Value::Color(..)),
        "Array" => matches!(v, Value::Array(_)),
        "Dictionary" => matches!(v, Value::Dictionary(_)),
        "Callable" => matches!(v, Value::Callable(_)),
        "Signal" => matches!(v, Value::Signal(..)),
        t if t.starts_with("Packed") && t.ends_with("Array") => matches!(v, Value::Array(_)),
        _ => return None,
    })
}

/// `@GlobalScope` constants Neko knows (key codes, mouse buttons, errors...).
pub fn global_constant(n: &str) -> Option<Value> {
    if let Some(k) = n.strip_prefix("KEY_") {
        return key_code(k).map(Value::Int);
    }
    let v = match n {
        "OK" => 0,
        "FAILED" => 1,
        "ERR_UNAVAILABLE" => 2,
        "ERR_FILE_NOT_FOUND" => 7,
        "ERR_FILE_CANT_OPEN" => 12,
        "ERR_PARSE_ERROR" => 43,
        "ERR_DOES_NOT_EXIST" => 33,
        "ERR_ALREADY_EXISTS" => 32,
        "ERR_INVALID_PARAMETER" => 31,
        "ERR_TIMEOUT" => 24,
        "ERR_CANT_CONNECT" => 25,
        "ERR_BUSY" => 44,
        "NOTIFICATION_WM_CLOSE_REQUEST" => 1006,
        "NOTIFICATION_WM_GO_BACK_REQUEST" => 1007,
        "NOTIFICATION_WM_SIZE_CHANGED" => 1008,
        "NOTIFICATION_WM_FOCUS_IN" => 1004,
        "NOTIFICATION_WM_FOCUS_OUT" => 1005,
        "NOTIFICATION_PREDELETE" => 1,
        "NOTIFICATION_READY" => 13,
        "NOTIFICATION_PROCESS" => 17,
        "NOTIFICATION_PHYSICS_PROCESS" => 16,
        "MOUSE_BUTTON_NONE" => 0,
        "MOUSE_BUTTON_LEFT" => 1,
        "MOUSE_BUTTON_RIGHT" => 2,
        "MOUSE_BUTTON_MIDDLE" => 3,
        "MOUSE_BUTTON_WHEEL_UP" => 4,
        "MOUSE_BUTTON_WHEEL_DOWN" => 5,
        "MOUSE_BUTTON_WHEEL_LEFT" => 6,
        "MOUSE_BUTTON_WHEEL_RIGHT" => 7,
        "MOUSE_BUTTON_XBUTTON1" => 8,
        "MOUSE_BUTTON_XBUTTON2" => 9,
        "JOY_BUTTON_A" => 0,
        "JOY_BUTTON_B" => 1,
        "JOY_BUTTON_X" => 2,
        "JOY_BUTTON_Y" => 3,
        "JOY_BUTTON_BACK" => 4,
        "JOY_BUTTON_GUIDE" => 5,
        "JOY_BUTTON_START" => 6,
        "JOY_BUTTON_LEFT_STICK" => 7,
        "JOY_BUTTON_RIGHT_STICK" => 8,
        "JOY_BUTTON_LEFT_SHOULDER" => 9,
        "JOY_BUTTON_RIGHT_SHOULDER" => 10,
        "JOY_BUTTON_DPAD_UP" => 11,
        "JOY_BUTTON_DPAD_DOWN" => 12,
        "JOY_BUTTON_DPAD_LEFT" => 13,
        "JOY_BUTTON_DPAD_RIGHT" => 14,
        "JOY_AXIS_LEFT_X" => 0,
        "JOY_AXIS_LEFT_Y" => 1,
        "JOY_AXIS_RIGHT_X" => 2,
        "JOY_AXIS_RIGHT_Y" => 3,
        "JOY_AXIS_TRIGGER_LEFT" => 4,
        "JOY_AXIS_TRIGGER_RIGHT" => 5,
        "TYPE_NIL" => 0,
        "TYPE_BOOL" => 1,
        "TYPE_INT" => 2,
        "TYPE_FLOAT" => 3,
        "TYPE_STRING" => 4,
        "TYPE_VECTOR2" => 5,
        "TYPE_VECTOR2I" => 6,
        "TYPE_RECT2" => 7,
        "TYPE_VECTOR3" => 9,
        "TYPE_VECTOR3I" => 10,
        "TYPE_COLOR" => 20,
        "TYPE_STRING_NAME" => 21,
        "TYPE_NODE_PATH" => 22,
        "TYPE_OBJECT" => 24,
        "TYPE_CALLABLE" => 25,
        "TYPE_SIGNAL" => 26,
        "TYPE_DICTIONARY" => 27,
        "TYPE_ARRAY" => 28,
        "SIDE_LEFT" => 0,
        "SIDE_TOP" => 1,
        "SIDE_RIGHT" => 2,
        "SIDE_BOTTOM" => 3,
        "HORIZONTAL_ALIGNMENT_LEFT" => 0,
        "HORIZONTAL_ALIGNMENT_CENTER" => 1,
        "HORIZONTAL_ALIGNMENT_RIGHT" => 2,
        "HORIZONTAL_ALIGNMENT_FILL" => 3,
        "VERTICAL_ALIGNMENT_TOP" => 0,
        "VERTICAL_ALIGNMENT_CENTER" => 1,
        "VERTICAL_ALIGNMENT_BOTTOM" => 2,
        "VERTICAL_ALIGNMENT_FILL" => 3,
        "CORNER_TOP_LEFT" => 0,
        "CORNER_TOP_RIGHT" => 1,
        "CORNER_BOTTOM_RIGHT" => 2,
        "CORNER_BOTTOM_LEFT" => 3,
        "PROPERTY_HINT_NONE" => 0,
        "PROPERTY_HINT_RANGE" => 1,
        "PROPERTY_USAGE_DEFAULT" => 6,
        "CONNECT_DEFERRED" => 1,
        "CONNECT_PERSIST" => 2,
        "CONNECT_ONE_SHOT" => 4,
        "CONNECT_REFERENCE_COUNTED" => 8,
        "KEY_MASK_SHIFT" => 1 << 25,
        "KEY_MASK_ALT" => 1 << 26,
        "KEY_MASK_META" => 1 << 27,
        "KEY_MASK_CTRL" => 1 << 28,
        _ => return None,
    };
    Some(Value::Int(v))
}

/// Godot's `Key` enum values.
fn key_code(k: &str) -> Option<i64> {
    const SPECIAL: i64 = 1 << 22;
    if k.len() == 1 {
        let c = k.chars().next()?;
        if c.is_ascii_uppercase() || c.is_ascii_digit() {
            return Some(c as i64);
        }
    }
    if let Some(n) = k.strip_prefix('F').and_then(|d| d.parse::<i64>().ok()) {
        return Some(SPECIAL | (0x1B + n));
    }
    if let Some(n) = k.strip_prefix("KP_").and_then(|d| d.parse::<i64>().ok()) {
        return Some(SPECIAL | (0x86 + n));
    }
    Some(match k {
        "NONE" => 0,
        "ESCAPE" => SPECIAL | 0x01,
        "TAB" => SPECIAL | 0x02,
        "BACKTAB" => SPECIAL | 0x03,
        "BACKSPACE" => SPECIAL | 0x04,
        "ENTER" => SPECIAL | 0x05,
        "KP_ENTER" => SPECIAL | 0x06,
        "INSERT" => SPECIAL | 0x07,
        "DELETE" => SPECIAL | 0x08,
        "PAUSE" => SPECIAL | 0x09,
        "PRINT" => SPECIAL | 0x0A,
        "SYSREQ" => SPECIAL | 0x0B,
        "CLEAR" => SPECIAL | 0x0C,
        "HOME" => SPECIAL | 0x0D,
        "END" => SPECIAL | 0x0E,
        "LEFT" => SPECIAL | 0x0F,
        "UP" => SPECIAL | 0x10,
        "RIGHT" => SPECIAL | 0x11,
        "DOWN" => SPECIAL | 0x12,
        "PAGEUP" => SPECIAL | 0x13,
        "PAGEDOWN" => SPECIAL | 0x14,
        "SHIFT" => SPECIAL | 0x15,
        "CTRL" => SPECIAL | 0x16,
        "META" => SPECIAL | 0x17,
        "ALT" => SPECIAL | 0x18,
        "CAPSLOCK" => SPECIAL | 0x19,
        "NUMLOCK" => SPECIAL | 0x1A,
        "SCROLLLOCK" => SPECIAL | 0x1B,
        "SPACE" => 0x20,
        "EXCLAM" => 0x21,
        "QUOTEDBL" => 0x22,
        "NUMBERSIGN" => 0x23,
        "DOLLAR" => 0x24,
        "PERCENT" => 0x25,
        "AMPERSAND" => 0x26,
        "APOSTROPHE" => 0x27,
        "PARENLEFT" => 0x28,
        "PARENRIGHT" => 0x29,
        "ASTERISK" => 0x2A,
        "PLUS" => 0x2B,
        "COMMA" => 0x2C,
        "MINUS" => 0x2D,
        "PERIOD" => 0x2E,
        "SLASH" => 0x2F,
        "COLON" => 0x3A,
        "SEMICOLON" => 0x3B,
        "LESS" => 0x3C,
        "EQUAL" => 0x3D,
        "GREATER" => 0x3E,
        "QUESTION" => 0x3F,
        "AT" => 0x40,
        "BRACKETLEFT" => 0x5B,
        "BACKSLASH" => 0x5C,
        "BRACKETRIGHT" => 0x5D,
        "ASCIICIRCUM" => 0x5E,
        "UNDERSCORE" => 0x5F,
        "QUOTELEFT" => 0x60,
        "BRACELEFT" => 0x7B,
        "BAR" => 0x7C,
        "BRACERIGHT" => 0x7D,
        "ASCIITILDE" => 0x7E,
        _ => return None,
    })
}

/// `Vector2.ZERO`, `Color.RED`, `Vector3.UP`...
pub fn type_constant(cls: &str, n: &str) -> Option<Value> {
    use Value::*;
    let inf = f32::INFINITY;
    Some(match (cls, n) {
        ("Vector2", "ZERO") => Vector2(0.0, 0.0),
        ("Vector2", "ONE") => Vector2(1.0, 1.0),
        ("Vector2", "INF") => Vector2(inf, inf),
        ("Vector2", "LEFT") => Vector2(-1.0, 0.0),
        ("Vector2", "RIGHT") => Vector2(1.0, 0.0),
        ("Vector2", "UP") => Vector2(0.0, -1.0),
        ("Vector2", "DOWN") => Vector2(0.0, 1.0),
        ("Vector2", "AXIS_X") | ("Vector3", "AXIS_X") => Int(0),
        ("Vector2", "AXIS_Y") | ("Vector3", "AXIS_Y") => Int(1),
        ("Vector3", "AXIS_Z") => Int(2),
        ("Vector2i", "ZERO") => Vector2i(0, 0),
        ("Vector2i", "ONE") => Vector2i(1, 1),
        ("Vector2i", "LEFT") => Vector2i(-1, 0),
        ("Vector2i", "RIGHT") => Vector2i(1, 0),
        ("Vector2i", "UP") => Vector2i(0, -1),
        ("Vector2i", "DOWN") => Vector2i(0, 1),
        ("Vector3", "ZERO") => Vector3(0.0, 0.0, 0.0),
        ("Vector3", "ONE") => Vector3(1.0, 1.0, 1.0),
        ("Vector3", "INF") => Vector3(inf, inf, inf),
        ("Vector3", "LEFT") => Vector3(-1.0, 0.0, 0.0),
        ("Vector3", "RIGHT") => Vector3(1.0, 0.0, 0.0),
        ("Vector3", "UP") => Vector3(0.0, 1.0, 0.0),
        ("Vector3", "DOWN") => Vector3(0.0, -1.0, 0.0),
        ("Vector3", "FORWARD") => Vector3(0.0, 0.0, -1.0),
        ("Vector3", "BACK") => Vector3(0.0, 0.0, 1.0),
        ("Vector3", "MODEL_LEFT") => Vector3(1.0, 0.0, 0.0),
        ("Vector3", "MODEL_RIGHT") => Vector3(-1.0, 0.0, 0.0),
        ("Vector3", "MODEL_TOP") => Vector3(0.0, 1.0, 0.0),
        ("Vector3", "MODEL_BOTTOM") => Vector3(0.0, -1.0, 0.0),
        ("Vector3", "MODEL_FRONT") => Vector3(0.0, 0.0, 1.0),
        ("Vector3", "MODEL_REAR") => Vector3(0.0, 0.0, -1.0),
        ("Vector3i", "ZERO") => Vector3i(0, 0, 0),
        ("Vector3i", "ONE") => Vector3i(1, 1, 1),
        ("Vector3i", "UP") => Vector3i(0, 1, 0),
        ("Vector3i", "DOWN") => Vector3i(0, -1, 0),
        ("Vector3i", "LEFT") => Vector3i(-1, 0, 0),
        ("Vector3i", "RIGHT") => Vector3i(1, 0, 0),
        ("Vector3i", "FORWARD") => Vector3i(0, 0, -1),
        ("Vector3i", "BACK") => Vector3i(0, 0, 1),
        ("Basis", "IDENTITY") => Basis([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]),
        ("Transform3D", "IDENTITY") => Transform3D { basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], origin: [0.0, 0.0, 0.0] },
        ("Transform3D", "FLIP_X") => Transform3D { basis: [[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], origin: [0.0, 0.0, 0.0] },
        ("Transform3D", "FLIP_Y") => Transform3D { basis: [[1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 1.0]], origin: [0.0, 0.0, 0.0] },
        ("Transform3D", "FLIP_Z") => Transform3D { basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, -1.0]], origin: [0.0, 0.0, 0.0] },
        ("FastNoiseLite", "TYPE_SIMPLEX") => Int(0),
        ("FastNoiseLite", "TYPE_SIMPLEX_SMOOTH") => Int(1),
        ("FastNoiseLite", "TYPE_PERLIN") => Int(2),
        ("FastNoiseLite", "TYPE_VALUE") => Int(3),
        ("FastNoiseLite", "TYPE_CELLULAR") => Int(4),
        ("Color", name) => return named_color(name),
        // Engine enums scripts use through their class (Godot's ClassDB constants).
        ("Node", name) => return node_constant(name),
        ("Control", name) => return control_constant(name),
        ("Input", name) => return input_constant(name),
        ("Mesh", name) => return mesh_constant(name),
        ("GeometryInstance3D", name) => return geometry_constant(name),
        ("Environment", name) => return environment_constant(name),
        ("Image", name) => return image_constant(name),
        ("FileAccess", name) => return file_access_constant(name),
        ("AudioStreamWAV", name) => return pair(name, &[("FORMAT_8_BITS", 0), ("FORMAT_16_BITS", 1), ("FORMAT_IMA_ADPCM", 2), ("FORMAT_QOA", 3)]),
        ("DisplayServer", name) => return display_server_constant(name),
        ("ResourceLoader", name) => return resource_loader_constant(name),
        ("TextureRect", name) => return pair(name, &[("EXPAND_KEEP_SIZE", 0), ("EXPAND_IGNORE_SIZE", 1), ("EXPAND_FIT_WIDTH", 2), ("EXPAND_FIT_WIDTH_PROPORTIONAL", 3), ("EXPAND_FIT_HEIGHT", 4), ("EXPAND_FIT_HEIGHT_PROPORTIONAL", 5), ("STRETCH_SCALE", 0), ("STRETCH_TILE", 1), ("STRETCH_KEEP", 2), ("STRETCH_KEEP_CENTERED", 3), ("STRETCH_KEEP_ASPECT", 4), ("STRETCH_KEEP_ASPECT_CENTERED", 5), ("STRETCH_KEEP_ASPECT_COVERED", 6)]),
        ("BoxContainer", name) => return pair(name, &[("ALIGNMENT_BEGIN", 0), ("ALIGNMENT_CENTER", 1), ("ALIGNMENT_END", 2)]),
        _ => return None,
    })
}

fn pair(name: &str, table: &[(&str, i64)]) -> Option<Value> {
    table.iter().find(|(n, _)| *n == name).map(|(_, v)| Value::Int(*v))
}

fn node_constant(n: &str) -> Option<Value> {
    pair(n, &[
        ("PROCESS_MODE_INHERIT", 0), ("PROCESS_MODE_PAUSABLE", 1), ("PROCESS_MODE_WHEN_PAUSED", 2),
        ("PROCESS_MODE_ALWAYS", 3), ("PROCESS_MODE_DISABLED", 4),
        ("INTERNAL_MODE_DISABLED", 0), ("INTERNAL_MODE_FRONT", 1), ("INTERNAL_MODE_BACK", 2),
    ])
}

fn control_constant(n: &str) -> Option<Value> {
    pair(n, &[
        ("MOUSE_FILTER_STOP", 0), ("MOUSE_FILTER_PASS", 1), ("MOUSE_FILTER_IGNORE", 2),
        ("PRESET_TOP_LEFT", 0), ("PRESET_TOP_RIGHT", 1), ("PRESET_BOTTOM_LEFT", 2), ("PRESET_BOTTOM_RIGHT", 3),
        ("PRESET_CENTER_LEFT", 4), ("PRESET_CENTER_TOP", 5), ("PRESET_CENTER_RIGHT", 6), ("PRESET_CENTER_BOTTOM", 7),
        ("PRESET_CENTER", 8), ("PRESET_LEFT_WIDE", 9), ("PRESET_TOP_WIDE", 10), ("PRESET_RIGHT_WIDE", 11),
        ("PRESET_BOTTOM_WIDE", 12), ("PRESET_VCENTER_WIDE", 13), ("PRESET_HCENTER_WIDE", 14), ("PRESET_FULL_RECT", 15),
        ("GROW_DIRECTION_BEGIN", 0), ("GROW_DIRECTION_END", 1), ("GROW_DIRECTION_BOTH", 2),
        ("CURSOR_ARROW", 0), ("CURSOR_IBEAM", 1), ("CURSOR_POINTING_HAND", 2), ("CURSOR_CROSS", 3),
        ("CURSOR_WAIT", 4), ("CURSOR_BUSY", 5), ("CURSOR_DRAG", 6), ("CURSOR_CAN_DROP", 7),
        ("CURSOR_FORBIDDEN", 8), ("CURSOR_HELP", 16),
        ("SIZE_SHRINK_BEGIN", 0), ("SIZE_FILL", 1), ("SIZE_EXPAND", 2), ("SIZE_EXPAND_FILL", 3),
        ("SIZE_SHRINK_CENTER", 4), ("SIZE_SHRINK_END", 5),
        ("FOCUS_NONE", 0), ("FOCUS_CLICK", 1), ("FOCUS_ALL", 2),
    ])
}

fn input_constant(n: &str) -> Option<Value> {
    pair(n, &[
        ("MOUSE_MODE_VISIBLE", 0), ("MOUSE_MODE_HIDDEN", 1), ("MOUSE_MODE_CAPTURED", 2),
        ("MOUSE_MODE_CONFINED", 3), ("MOUSE_MODE_CONFINED_HIDDEN", 4),
        ("CURSOR_ARROW", 0), ("CURSOR_IBEAM", 1), ("CURSOR_POINTING_HAND", 2), ("CURSOR_CROSS", 3),
    ])
}

fn mesh_constant(n: &str) -> Option<Value> {
    pair(n, &[
        ("PRIMITIVE_POINTS", 0), ("PRIMITIVE_LINES", 1), ("PRIMITIVE_LINE_STRIP", 2), ("PRIMITIVE_LINE_LOOP", 3),
        ("PRIMITIVE_TRIANGLES", 4), ("PRIMITIVE_TRIANGLE_STRIP", 5), ("PRIMITIVE_TRIANGLE_FAN", 6),
        ("ARRAY_VERTEX", 0), ("ARRAY_NORMAL", 1), ("ARRAY_TANGENT", 2), ("ARRAY_COLOR", 3),
        ("ARRAY_TEX_UV", 4), ("ARRAY_TEX_UV2", 5), ("ARRAY_CUSTOM0", 6), ("ARRAY_BONES", 10),
        ("ARRAY_WEIGHTS", 11), ("ARRAY_INDEX", 12), ("ARRAY_MAX", 13),
    ])
}

fn geometry_constant(n: &str) -> Option<Value> {
    pair(n, &[
        ("SHADOW_CASTING_SETTING_OFF", 0), ("SHADOW_CASTING_SETTING_ON", 1),
        ("SHADOW_CASTING_SETTING_DOUBLE_SIDED", 2), ("SHADOW_CASTING_SETTING_SHADOWS_ONLY", 3),
    ])
}

fn environment_constant(n: &str) -> Option<Value> {
    pair(n, &[
        ("BG_CLEAR_COLOR", 0), ("BG_COLOR", 1), ("BG_SKY", 2), ("BG_CANVAS", 3), ("BG_KEEP", 4), ("BG_CAMERA_FEED", 5),
        ("TONE_MAPPER_LINEAR", 0), ("TONE_MAPPER_REINHARDT", 1), ("TONE_MAPPER_FILMIC", 2), ("TONE_MAPPER_ACES", 3),
        ("FOG_MODE_EXPONENTIAL", 0), ("FOG_MODE_DEPTH", 1),
        ("GLOW_BLEND_MODE_ADDITIVE", 0), ("GLOW_BLEND_MODE_SCREEN", 1), ("GLOW_BLEND_MODE_SOFTLIGHT", 2),
        ("GLOW_BLEND_MODE_REPLACE", 3), ("GLOW_BLEND_MODE_MIX", 4),
    ])
}

fn image_constant(n: &str) -> Option<Value> {
    pair(n, &[
        ("FORMAT_L8", 0), ("FORMAT_LA8", 1), ("FORMAT_R8", 2), ("FORMAT_RG8", 3), ("FORMAT_RGB8", 4),
        ("FORMAT_RGBA8", 5), ("FORMAT_RGBA4444", 6), ("FORMAT_RGB565", 7),
        ("INTERPOLATE_NEAREST", 0), ("INTERPOLATE_BILINEAR", 1), ("INTERPOLATE_CUBIC", 2),
    ])
}

fn file_access_constant(n: &str) -> Option<Value> {
    pair(n, &[
        ("READ", 1), ("WRITE", 2), ("READ_WRITE", 3), ("WRITE_READ", 7),
        ("COMPRESSION_FASTLZ", 0), ("COMPRESSION_DEFLATE", 1), ("COMPRESSION_ZSTD", 2), ("COMPRESSION_GZIP", 3),
    ])
}

fn display_server_constant(n: &str) -> Option<Value> {
    pair(n, &[
        ("WINDOW_MODE_WINDOWED", 0), ("WINDOW_MODE_MINIMIZED", 1), ("WINDOW_MODE_MAXIMIZED", 2),
        ("WINDOW_MODE_FULLSCREEN", 3), ("WINDOW_MODE_EXCLUSIVE_FULLSCREEN", 4),
        ("VSYNC_DISABLED", 0), ("VSYNC_ENABLED", 1), ("VSYNC_ADAPTIVE", 2),
        ("SCREEN_OF_MAIN_WINDOW", -1),
    ])
}

fn resource_loader_constant(n: &str) -> Option<Value> {
    pair(n, &[
        ("CACHE_MODE_IGNORE", 0), ("CACHE_MODE_REUSE", 1), ("CACHE_MODE_REPLACE", 2),
    ])
}

/// A few of Godot's named colors (`Color.RED`...); the rest come from the
/// full table when needed.
fn named_color(n: &str) -> Option<Value> {
    let hex: u32 = match n {
        "BLACK" => 0x000000,
        "WHITE" => 0xffffff,
        "RED" => 0xff0000,
        "GREEN" => 0x00ff00,
        "BLUE" => 0x0000ff,
        "YELLOW" => 0xffff00,
        "CYAN" | "AQUA" => 0x00ffff,
        "MAGENTA" | "FUCHSIA" => 0xff00ff,
        "ORANGE" => 0xffa500,
        "GRAY" => 0xbebebe,
        "DARK_GRAY" => 0xa9a9a9,
        "LIGHT_GRAY" => 0xd3d3d3,
        "DIM_GRAY" => 0x696969,
        "PURPLE" => 0xa020f0,
        "PINK" => 0xffc0cb,
        "BROWN" => 0xa52a2a,
        "GOLD" => 0xffd700,
        "LIME" => 0x00ff00,
        "LIME_GREEN" => 0x32cd32,
        "DARK_GREEN" => 0x006400,
        "FOREST_GREEN" => 0x228b22,
        "NAVY_BLUE" => 0x000080,
        "SKY_BLUE" => 0x87ceeb,
        "DODGER_BLUE" => 0x1e90ff,
        "CORNFLOWER_BLUE" => 0x6495ed,
        "DARK_RED" => 0x8b0000,
        "CRIMSON" => 0xdc143c,
        "TOMATO" => 0xff6347,
        "ORANGE_RED" => 0xff4500,
        "SILVER" => 0xc0c0c0,
        "SALMON" => 0xfa8072,
        "VIOLET" => 0xee82ee,
        "INDIGO" => 0x4b0082,
        "TEAL" => 0x008080,
        "OLIVE" => 0x808000,
        "MAROON" => 0xb03060,
        "BEIGE" => 0xf5f5dc,
        "KHAKI" => 0xf0e68c,
        "CORAL" => 0xff7f50,
        "TURQUOISE" => 0x40e0d0,
        "TRANSPARENT" => return Some(Value::Color(1.0, 1.0, 1.0, 0.0)),
        _ => return None,
    };
    let c = |s: u32| ((hex >> s) & 0xff) as f32 / 255.0;
    Some(Value::Color(c(16), c(8), c(0), 1.0))
}

/// Parse `#rgb`, `#rrggbb`, `#rrggbbaa` (Color.html).
fn color_html(s: &str) -> Option<Value> {
    let h = s.trim_start_matches('#');
    let p = |i: usize, len: usize| u32::from_str_radix(h.get(i..i + len)?, 16).ok();
    let (r, g, b, a) = match h.len() {
        3 => (p(0, 1)? * 17, p(1, 1)? * 17, p(2, 1)? * 17, 255),
        4 => (p(0, 1)? * 17, p(1, 1)? * 17, p(2, 1)? * 17, p(3, 1)? * 17),
        6 => (p(0, 2)?, p(2, 2)?, p(4, 2)?, 255),
        8 => (p(0, 2)?, p(2, 2)?, p(4, 2)?, p(6, 2)?),
        _ => return None,
    };
    Some(Value::Color(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a as f32 / 255.0))
}

/// Static methods on built-in types: `Color.html()`, `Vector2.from_angle()`...
pub fn static_call(it: &mut Interp, cls: &str, name: &str, args: &[Value]) -> Option<RResult<Value>> {
    let f = |i: usize| args.get(i).and_then(Value::as_f64).unwrap_or(0.0);
    let r = match (cls, name) {
        ("Color", "html") => Ok(args.first().and_then(|v| v.as_str()).and_then(color_html).unwrap_or(Value::Color(0.0, 0.0, 0.0, 1.0))),
        ("Color", "html_is_valid") => Ok(Value::Bool(args.first().and_then(|v| v.as_str()).and_then(color_html).is_some())),
        ("Color", "from_hsv") => {
            let (r, g, b) = hsv_to_rgb(f(0), f(1), f(2));
            Ok(Value::Color(r as f32, g as f32, b as f32, args.get(3).and_then(Value::as_f64).unwrap_or(1.0) as f32))
        }
        ("Color", "from_rgba8") | ("Color", "Color8") => Ok(Value::Color((f(0) / 255.0) as f32, (f(1) / 255.0) as f32, (f(2) / 255.0) as f32, (args.get(3).and_then(Value::as_f64).unwrap_or(255.0) / 255.0) as f32)),
        ("Color", "from_string") => Ok(args.first().and_then(|v| v.as_str()).and_then(|s| color_html(s).or_else(|| named_color(&s.to_uppercase().replace(' ', "_")))).unwrap_or_else(|| args.get(1).cloned().unwrap_or(Value::Nil))),
        ("Vector2", "from_angle") => Ok(Value::Vector2(f(0).cos() as f32, f(0).sin() as f32)),
        ("String", "num") => {
            let d = args.get(1).and_then(Value::as_i64).unwrap_or(-1);
            Ok(Value::str(&if d < 0 { float_str(f(0)).trim_end_matches(".0").to_string() } else { format!("{:.*}", d as usize, f(0)) }))
        }
        ("String", "num_int64") => Ok(Value::str(&(f(0) as i64).to_string())),
        ("String", "chr") => Ok(Value::str(&char::from_u32(f(0) as u32).map(String::from).unwrap_or_default())),
        ("Time", _) | ("OS", _) | ("Engine", _) | ("Input", _) => return None,
        _ => return None,
    };
    let _ = it;
    Some(r.map_err(|m: String| ScriptError { message: m, file: String::new(), line: 0, func: String::new() }))
}

fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (f64, f64, f64) {
    let h = (h.rem_euclid(1.0)) * 6.0;
    let i = h.floor();
    let f = h - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    match i as i64 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    }
}

/// Built-in type constructors: `Vector2(1, 2)`, `Color(...)`, `int("3")`...
pub fn construct(name: &str, a: &[Value]) -> Option<SResult> {
    let f = |i: usize| a.get(i).and_then(Value::as_f64).unwrap_or(0.0) as f32;
    let n = a.len();
    let copy_or = |d: Value| if n == 1 { a[0].clone() } else { d };
    Some(Ok(match name {
        "Vector2" => match a.first() {
            Some(Value::Vector2i(x, y)) => Value::Vector2(*x as f32, *y as f32),
            Some(Value::Vector2(x, y)) => Value::Vector2(*x, *y),
            _ => Value::Vector2(f(0), f(1)),
        },
        "Vector2i" => match a.first() {
            Some(Value::Vector2(x, y)) => Value::Vector2i(*x as i32, *y as i32),
            Some(Value::Vector2i(x, y)) => Value::Vector2i(*x, *y),
            _ => Value::Vector2i(f(0) as i32, f(1) as i32),
        },
        "Vector3" => match a.first() {
            Some(Value::Vector3i(x, y, z)) => Value::Vector3(*x as f32, *y as f32, *z as f32),
            Some(Value::Vector3(x, y, z)) => Value::Vector3(*x, *y, *z),
            _ => Value::Vector3(f(0), f(1), f(2)),
        },
        "Vector3i" => match a.first() {
            Some(Value::Vector3(x, y, z)) => Value::Vector3i(*x as i32, *y as i32, *z as i32),
            _ => Value::Vector3i(f(0) as i32, f(1) as i32, f(2) as i32),
        },
        "Vector4" => Value::Vector4(f(0), f(1), f(2), f(3)),
        "Rect2" => match (a.first(), a.get(1)) {
            (Some(Value::Vector2(x, y)), Some(Value::Vector2(w, h))) => Value::Rect2(*x, *y, *w, *h),
            _ => Value::Rect2(f(0), f(1), f(2), f(3)),
        },
        "Color" => match a.first() {
            Some(Value::String(s)) => {
                let base = color_html(s).or_else(|| named_color(&s.to_uppercase())).unwrap_or(Value::Color(0.0, 0.0, 0.0, 1.0));
                match (base, a.get(1).and_then(Value::as_f64)) {
                    (Value::Color(r, g, b, _), Some(al)) => Value::Color(r, g, b, al as f32),
                    (c, _) => c,
                }
            }
            Some(Value::Color(r, g, b, al)) => Value::Color(*r, *g, *b, a.get(1).and_then(Value::as_f64).map(|x| x as f32).unwrap_or(*al)),
            _ if n == 0 => Value::Color(0.0, 0.0, 0.0, 1.0),
            _ => Value::Color(f(0), f(1), f(2), if n > 3 { f(3) } else { 1.0 }),
        },
        "int" => to_int(a.first().unwrap_or(&Value::Int(0))),
        "float" => to_float(a.first().unwrap_or(&Value::Float(0.0))),
        "bool" => Value::Bool(a.first().is_some_and(Value::truthy)),
        "String" => Value::str(&a.first().map(|v| v.to_string()).unwrap_or_default()),
        "StringName" => Value::StringName(Rc::from(a.first().map(|v| v.to_string()).unwrap_or_default().as_str())),
        "NodePath" => Value::NodePath(Rc::from(a.first().map(|v| v.to_string()).unwrap_or_default().as_str())),
        "Array" => match a.first() {
            Some(Value::Array(x)) => Value::array(x.borrow().clone()),
            _ => Value::array(Vec::new()),
        },
        "Dictionary" => match a.first() {
            Some(Value::Dictionary(d)) => Value::dict(d.borrow().clone()),
            _ => Value::dict(Dict::new()),
        },
        "Callable" => match (a.first(), a.get(1)) {
            (Some(Value::Object(o)), Some(m)) => Value::Callable(Rc::new(Callable::Method { obj: o.clone(), name: Rc::from(m.to_string().as_str()), bound: Vec::new() })),
            _ => return Some(err("Callable() needs an object and a method name.")),
        },
        n2 if n2.starts_with("Packed") && n2.ends_with("Array") => match a.first() {
            Some(Value::Array(x)) => Value::array(x.borrow().clone()),
            _ => Value::array(Vec::new()),
        },
        "Basis" => {
            let mut b = [[0.0f32; 3]; 3];
            b[0][0] = 1.0;
            b[1][1] = 1.0;
            b[2][2] = 1.0;
            match a.first() {
                Some(Value::Basis(x)) => Value::Basis(*x),
                Some(Value::Vector3(x, y, z)) => {
                    if let (Some(Value::Vector3(x2, y2, z2)), Some(Value::Vector3(x3, y3, z3))) = (a.get(1), a.get(2)) {
                        Value::Basis([[*x, *y, *z], [*x2, *y2, *z2], [*x3, *y3, *z3]])
                    } else {
                        Value::Basis(b)
                    }
                }
                _ if n == 9 => {
                    for i in 0..3 {
                        for j in 0..3 {
                            b[i][j] = f(i * 3 + j);
                        }
                    }
                    Value::Basis(b)
                }
                _ => Value::Basis(b),
            }
        }
        "Transform3D" => {
            let identity = [[1.0f32, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
            match a.first() {
                Some(Value::Transform3D { basis, origin }) => Value::Transform3D { basis: *basis, origin: *origin },
                Some(Value::Basis(b)) => {
                    let origin = match a.get(1) {
                        Some(Value::Vector3(x, y, z)) => [*x, *y, *z],
                        _ => [0.0; 3],
                    };
                    Value::Transform3D { basis: *b, origin }
                }
                _ => {
                    let mut b = identity;
                    let mut origin = [0.0f32; 3];
                    if n >= 12 {
                        for i in 0..3 {
                            for j in 0..3 {
                                b[i][j] = f(i * 3 + j);
                            }
                        }
                        origin = [f(9), f(10), f(11)];
                    }
                    Value::Transform3D { basis: b, origin }
                }
            }
        }
        "AABB" => match (a.first(), a.get(1)) {
            (Some(Value::Vector3(px, py, pz)), Some(Value::Vector3(sx, sy, sz))) => Value::AABB { position: [*px, *py, *pz], size: [*sx, *sy, *sz] },
            _ => Value::AABB { position: [0.0; 3], size: [0.0; 3] },
        },
        "Transform2D" | "Quaternion" | "Plane" => return Some(err(format!("{name} is not supported by NekoScript yet."))),
        _ => {
            let _ = copy_or;
            return None;
        }
    }))
}

/// xorshift64 on the interpreter's state; good enough for gameplay randomness.
fn rand_u32(it: &Interp) -> u32 {
    let mut x = it.rng.get();
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    it.rng.set(x);
    (x >> 32) as u32
}

fn randf(it: &Interp) -> f64 {
    rand_u32(it) as f64 / u32::MAX as f64
}

/// `@GlobalScope` utility functions.
pub fn utility(it: &mut Interp, name: &str, a: &[Value]) -> Option<RResult<Value>> {
    let f = |i: usize| a.get(i).and_then(Value::as_f64).unwrap_or(0.0);
    let all_int = a.iter().all(|v| matches!(v, Value::Int(_)));
    let num = |x: f64| if all_int { Value::Int(x as i64) } else { Value::Float(x) };
    let joined = |sep: &str| a.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(sep);
    let r: RResult<Value> = Ok(match name {
        "print" | "print_rich" | "print_verbose" => {
            let s = joined("");
            it.print(&s);
            Value::Nil
        }
        "prints" => {
            let s = joined(" ");
            it.print(&s);
            Value::Nil
        }
        "printt" => {
            let s = joined("\t");
            it.print(&s);
            Value::Nil
        }
        "printerr" | "push_error" | "push_warning" => {
            let s = joined("");
            let s = if name == "push_warning" { format!("WARNING: {s}") } else { s };
            it.with_host(|h, _| h.print_error(&s));
            Value::Nil
        }
        "print_stack" | "get_stack" => Value::array(Vec::new()),
        "str" => Value::str(&joined("")),
        "len" => match a.first() {
            Some(Value::String(s) | Value::StringName(s) | Value::NodePath(s)) => Value::Int(s.chars().count() as i64),
            Some(Value::Array(x)) => Value::Int(x.borrow().len() as i64),
            Some(Value::Dictionary(d)) => Value::Int(d.borrow().len() as i64),
            Some(other) => return Some(ierr(it, format!("len() cannot be used on a value of type '{}'.", other.type_name()))),
            None => return Some(ierr(it, "len() needs one argument.")),
        },
        "range" => {
            let (s, e, st) = match a.len() {
                1 => (0, f(0) as i64, 1),
                2 => (f(0) as i64, f(1) as i64, 1),
                _ => (f(0) as i64, f(1) as i64, f(2) as i64),
            };
            if st == 0 {
                return Some(ierr(it, "range() step cannot be zero."));
            }
            let mut v = Vec::new();
            let mut i = s;
            while (st > 0 && i < e) || (st < 0 && i > e) {
                v.push(Value::Int(i));
                i += st;
            }
            Value::array(v)
        }
        "abs" => match a.first() {
            Some(Value::Int(i)) => Value::Int(i.abs()),
            Some(Value::Vector2(x, y)) => Value::Vector2(x.abs(), y.abs()),
            Some(Value::Vector3(x, y, z)) => Value::Vector3(x.abs(), y.abs(), z.abs()),
            _ => Value::Float(f(0).abs()),
        },
        "absf" => Value::Float(f(0).abs()),
        "absi" => Value::Int((f(0) as i64).abs()),
        "sign" => match a.first() {
            Some(Value::Int(i)) => Value::Int(i.signum()),
            _ => Value::Float(if f(0) > 0.0 { 1.0 } else if f(0) < 0.0 { -1.0 } else { 0.0 }),
        },
        "signf" => Value::Float(if f(0) > 0.0 { 1.0 } else if f(0) < 0.0 { -1.0 } else { 0.0 }),
        "signi" => Value::Int((f(0) as i64).signum()),
        "min" | "minf" | "mini" => {
            let m = a.iter().filter_map(Value::as_f64).fold(f64::INFINITY, f64::min);
            if name == "mini" { Value::Int(m as i64) } else if name == "minf" { Value::Float(m) } else { num(m) }
        }
        "max" | "maxf" | "maxi" => {
            let m = a.iter().filter_map(Value::as_f64).fold(f64::NEG_INFINITY, f64::max);
            if name == "maxi" { Value::Int(m as i64) } else if name == "maxf" { Value::Float(m) } else { num(m) }
        }
        "clamp" | "clampf" | "clampi" => {
            if let (Some(Value::Vector2(x, y)), Some(Value::Vector2(lx, ly)), Some(Value::Vector2(hx, hy))) = (a.first(), a.get(1), a.get(2)) {
                Value::Vector2(x.clamp(*lx, *hx), y.clamp(*ly, *hy))
            } else {
                let v = f(0).max(f(1)).min(f(2));
                if name == "clampi" { Value::Int(v as i64) } else if name == "clampf" { Value::Float(v) } else { num(v) }
            }
        }
        "lerp" | "lerpf" => match (a.first(), a.get(1)) {
            (Some(x), Some(y)) if !matches!(x, Value::Int(_) | Value::Float(_)) => {
                let t = Value::Float(f(2));
                let d = match binary(BinOp::Sub, y, x) {
                    Ok(d) => d,
                    Err(m) => return Some(ierr(it, m)),
                };
                match binary(BinOp::Mul, &d, &t).and_then(|m| binary(BinOp::Add, x, &m)) {
                    Ok(v) => v,
                    Err(m) => return Some(ierr(it, m)),
                }
            }
            _ => Value::Float(f(0) + (f(1) - f(0)) * f(2)),
        },
        "lerp_angle" => {
            let diff = (f(1) - f(0)).rem_euclid(core::f64::consts::TAU);
            let d = (2.0 * diff).rem_euclid(core::f64::consts::TAU) - diff;
            Value::Float(f(0) + d * f(2))
        }
        "angle_difference" => {
            let diff = (f(1) - f(0)).rem_euclid(core::f64::consts::TAU);
            Value::Float((2.0 * diff).rem_euclid(core::f64::consts::TAU) - diff)
        }
        "inverse_lerp" => Value::Float((f(2) - f(0)) / (f(1) - f(0))),
        "remap" => Value::Float(f(3) + (f(4) - f(3)) * ((f(0) - f(1)) / (f(2) - f(1)))),
        "move_toward" => {
            let (from, to, delta) = (f(0), f(1), f(2));
            Value::Float(if (to - from).abs() <= delta { to } else { from + (to - from).signum() * delta })
        }
        "smoothstep" => {
            if f(0) == f(1) {
                Value::Float(0.0)
            } else {
                let t = ((f(2) - f(0)) / (f(1) - f(0))).clamp(0.0, 1.0);
                Value::Float(t * t * (3.0 - 2.0 * t))
            }
        }
        "ease" => {
            let (x, c) = (f(0).clamp(0.0, 1.0), f(1));
            Value::Float(if c > 0.0 {
                if c < 1.0 { 1.0 - (1.0 - x).powf(1.0 / c) } else { x.powf(c) }
            } else if c < 0.0 {
                if x < 0.5 { (x * 2.0).powf(-c) * 0.5 } else { (1.0 - (1.0 - (x - 0.5) * 2.0).powf(-c)) * 0.5 + 0.5 }
            } else {
                0.0
            })
        }
        "sin" => Value::Float(f(0).sin()),
        "cos" => Value::Float(f(0).cos()),
        "tan" => Value::Float(f(0).tan()),
        "asin" => Value::Float(f(0).asin()),
        "acos" => Value::Float(f(0).acos()),
        "atan" => Value::Float(f(0).atan()),
        "atan2" => Value::Float(f(0).atan2(f(1))),
        "sinh" => Value::Float(f(0).sinh()),
        "cosh" => Value::Float(f(0).cosh()),
        "tanh" => Value::Float(f(0).tanh()),
        "sqrt" => Value::Float(f(0).sqrt()),
        "pow" => Value::Float(f(0).powf(f(1))),
        "exp" => Value::Float(f(0).exp()),
        "log" => Value::Float(f(0).ln()),
        "floor" => match a.first() {
            Some(Value::Int(i)) => Value::Int(*i),
            Some(Value::Vector2(x, y)) => Value::Vector2(x.floor(), y.floor()),
            _ => Value::Float(f(0).floor()),
        },
        "floorf" => Value::Float(f(0).floor()),
        "floori" => Value::Int(f(0).floor() as i64),
        "ceil" => match a.first() {
            Some(Value::Int(i)) => Value::Int(*i),
            _ => Value::Float(f(0).ceil()),
        },
        "ceilf" => Value::Float(f(0).ceil()),
        "ceili" => Value::Int(f(0).ceil() as i64),
        "round" => match a.first() {
            Some(Value::Int(i)) => Value::Int(*i),
            Some(Value::Vector2(x, y)) => Value::Vector2(x.round(), y.round()),
            _ => Value::Float(f(0).round()),
        },
        "roundf" => Value::Float(f(0).round()),
        "roundi" => Value::Int(f(0).round() as i64),
        "fmod" => Value::Float(f(0) % f(1)),
        "fposmod" => Value::Float(f(0).rem_euclid(f(1))),
        "posmod" => {
            let (x, y) = (f(0) as i64, f(1) as i64);
            if y == 0 {
                return Some(ierr(it, "posmod() by zero."));
            }
            Value::Int(x.rem_euclid(y))
        }
        "snapped" | "snappedf" | "snappedi" => {
            let v = if f(1) == 0.0 { f(0) } else { (f(0) / f(1) + 0.5).floor() * f(1) };
            if name == "snappedi" { Value::Int(v as i64) } else if name == "snappedf" { Value::Float(v) } else { num(v) }
        }
        "wrapi" => {
            let (v, lo, hi) = (f(0) as i64, f(1) as i64, f(2) as i64);
            let r = hi - lo;
            Value::Int(if r == 0 { lo } else { lo + (v - lo).rem_euclid(r) })
        }
        "wrapf" | "wrap" => {
            let (v, lo, hi) = (f(0), f(1), f(2));
            let r = hi - lo;
            let w = if r == 0.0 { lo } else { v - r * ((v - lo) / r).floor() };
            if name == "wrap" { num(w) } else { Value::Float(w) }
        }
        "pingpong" => {
            let (v, len) = (f(0), f(1));
            Value::Float(if len != 0.0 { (((v - len) / (len * 2.0)).fract().abs() * len * 2.0 - len).abs() } else { 0.0 })
        }
        "deg_to_rad" => Value::Float(f(0).to_radians()),
        "rad_to_deg" => Value::Float(f(0).to_degrees()),
        "linear_to_db" => Value::Float(f(0).ln() * 8.685_889_638_065_037),
        "db_to_linear" => Value::Float((f(0) * 0.115_129_254_649_702_28).exp()),
        "randi" => Value::Int(rand_u32(it) as i64),
        "randf" => Value::Float(randf(it)),
        "randi_range" => {
            let (lo, hi) = (f(0) as i64, f(1) as i64);
            let (lo, hi) = (lo.min(hi), lo.max(hi));
            Value::Int(lo + (rand_u32(it) as i64 % (hi - lo + 1)))
        }
        "randf_range" => Value::Float(f(0) + randf(it) * (f(1) - f(0))),
        "randfn" => {
            let (u1, u2) = (randf(it).max(1e-12), randf(it));
            Value::Float(f(0) + f(1) * (-2.0 * u1.ln()).sqrt() * (core::f64::consts::TAU * u2).cos())
        }
        "randomize" => {
            // Mix in the clock where there is one; otherwise just advance the state.
            #[cfg(feature = "std")]
            let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1);
            #[cfg(not(feature = "std"))]
            let t = it.rng.get().rotate_left(17) ^ 0x9E37_79B9_7F4A_7C15;
            it.rng.set(t | 1);
            Value::Nil
        }
        "seed" => {
            it.rng.set((f(0) as u64) | 1);
            Value::Nil
        }
        "is_nan" => Value::Bool(f(0).is_nan()),
        "is_inf" => Value::Bool(f(0).is_infinite()),
        "is_finite" => Value::Bool(f(0).is_finite()),
        "is_equal_approx" => Value::Bool((f(0) - f(1)).abs() < 1e-5 * f(0).abs().max(1.0)),
        "is_zero_approx" => Value::Bool(f(0).abs() < 1e-5),
        "step_decimals" => {
            let s = float_str(f(0));
            Value::Int(s.split('.').nth(1).map(|d| d.trim_end_matches('0').len() as i64).unwrap_or(0))
        }
        "nearest_po2" => Value::Int((f(0) as i64).max(1).cast_unsigned().next_power_of_two() as i64),
        "typeof" => Value::Int(a.first().map(Value::type_id).unwrap_or(0)),
        "type_string" => Value::str(type_string(f(0) as i64)),
        "is_instance_valid" => Value::Bool(matches!(a.first(), Some(Value::Object(o)) if o.is_valid())),
        "is_same" => Value::Bool(match (a.first(), a.get(1)) {
            (Some(Value::Object(x)), Some(Value::Object(y))) => x.id() == y.id(),
            (Some(x), Some(y)) => equals(x, y) && x.type_id() == y.type_id(),
            _ => false,
        }),
        "char" => Value::str(&char::from_u32(f(0) as u32).map(String::from).unwrap_or_default()),
        "ord" => Value::Int(a.first().and_then(|v| v.as_str()).and_then(|s| s.chars().next()).map(|c| c as i64).unwrap_or(0)),
        "hash" => {
            // FNV-1a: the same value on every platform and run.
            let text = a.first().map(|v| v.to_string()).unwrap_or_default();
            let h = text.bytes().fold(0x811c_9dc5u32, |h, b| (h ^ b as u32).wrapping_mul(0x0100_0193));
            Value::Int(h as i64)
        }
        "var_to_str" => Value::str(&a.first().map(|v| v.to_string()).unwrap_or_default()),
        "error_string" => Value::str(if f(0) == 0.0 { "OK" } else { "Failed" }),
        "type_exists" => Value::Bool(a.first().and_then(|v| v.as_str()).is_some_and(is_builtin_type)),
        "load" => {
            let p = a.first().map(|v| v.to_string()).unwrap_or_default();
            return Some(it.load_resource(&p));
        }
        "weakref" => a.first().cloned().unwrap_or(Value::Nil),
        "instance_from_id" | "str_to_var" | "bytes_to_var" | "var_to_bytes" | "rid_allocate_id" | "convert" | "is_instance_of" | "rotate_toward" | "bezier_interpolate" | "cubic_interpolate" => Value::Nil,
        _ => return None,
    });
    Some(r)
}

fn type_string(t: i64) -> &'static str {
    match t {
        0 => "Nil",
        1 => "bool",
        2 => "int",
        3 => "float",
        4 => "String",
        5 => "Vector2",
        6 => "Vector2i",
        7 => "Rect2",
        9 => "Vector3",
        10 => "Vector3i",
        20 => "Color",
        21 => "StringName",
        22 => "NodePath",
        24 => "Object",
        25 => "Callable",
        26 => "Signal",
        27 => "Dictionary",
        28 => "Array",
        _ => "Variant",
    }
}

// ------------------------------------------------------------ members of built-in types

pub fn attr_get(v: &Value, n: &str) -> SResult {
    use Value::*;
    Ok(match (v, n) {
        (Vector2(x, _), "x") | (Vector3(x, _, _), "x") | (Vector4(x, _, _, _), "x") => Float(*x as f64),
        (Vector2(_, y), "y") | (Vector3(_, y, _), "y") | (Vector4(_, y, _, _), "y") => Float(*y as f64),
        (Vector3(_, _, z), "z") | (Vector4(_, _, z, _), "z") => Float(*z as f64),
        (Vector4(_, _, _, w), "w") => Float(*w as f64),
        (Vector2i(x, _), "x") | (Vector3i(x, _, _), "x") => Int(*x as i64),
        (Vector2i(_, y), "y") | (Vector3i(_, y, _), "y") => Int(*y as i64),
        (Vector3i(_, _, z), "z") => Int(*z as i64),
        (Color(r, ..), "r") => Float(*r as f64),
        (Color(_, g, ..), "g") => Float(*g as f64),
        (Color(_, _, b, _), "b") => Float(*b as f64),
        (Color(.., a), "a") => Float(*a as f64),
        (Color(r, ..), "r8") => Int((*r * 255.0).round() as i64),
        (Color(_, g, ..), "g8") => Int((*g * 255.0).round() as i64),
        (Color(_, _, b, _), "b8") => Int((*b * 255.0).round() as i64),
        (Color(.., a), "a8") => Int((*a * 255.0).round() as i64),
        (Color(r, g, b, _), "h" | "s" | "v") => {
            let (r, g, b) = (*r as f64, *g as f64, *b as f64);
            let max = r.max(g).max(b);
            let min = r.min(g).min(b);
            let d = max - min;
            match n {
                "v" => Float(max),
                "s" => Float(if max == 0.0 { 0.0 } else { d / max }),
                _ => {
                    let h = if d == 0.0 {
                        0.0
                    } else if max == r {
                        ((g - b) / d).rem_euclid(6.0)
                    } else if max == g {
                        (b - r) / d + 2.0
                    } else {
                        (r - g) / d + 4.0
                    };
                    Float(h / 6.0)
                }
            }
        }
        (Rect2(x, y, ..), "position") => Vector2(*x, *y),
        (Rect2(_, _, w, h), "size") => Vector2(*w, *h),
        (Rect2(x, y, w, h), "end") => Vector2(x + w, y + h),
        (Transform3D { basis, .. }, "basis") => Basis(*basis),
        (Transform3D { origin, .. }, "origin") => Vector3(origin[0], origin[1], origin[2]),
        (Transform3D { basis, .. }, "x") => Vector3(basis[0][0], basis[1][0], basis[2][0]),
        (Transform3D { basis, .. }, "y") => Vector3(basis[0][1], basis[1][1], basis[2][1]),
        (Transform3D { basis, .. }, "z") => Vector3(basis[0][2], basis[1][2], basis[2][2]),
        (Basis(b), "x") => Vector3(b[0][0], b[1][0], b[2][0]),
        (Basis(b), "y") => Vector3(b[0][1], b[1][1], b[2][1]),
        (Basis(b), "z") => Vector3(b[0][2], b[1][2], b[2][2]),
        (AABB { position, .. }, "position") => Vector3(position[0], position[1], position[2]),
        (AABB { size, .. }, "size") => Vector3(size[0], size[1], size[2]),
        (AABB { position, size }, "end") => Vector3(position[0] + size[0], position[1] + size[1], position[2] + size[2]),
        (Dictionary(d), key) => match d.borrow().get(&Value::str(key)) {
            Some(v) => v.clone(),
            None => return err(format!("Invalid access to property or key '{key}' on a base object of type 'Dictionary'.")),
        },
        _ => return err(format!("Invalid access to property or key '{n}' on a base object of type '{}'.", v.type_name())),
    })
}

pub fn attr_set(v: &Value, n: &str, nv: Value) -> SResult {
    use Value::*;
    let x = nv.as_f64().unwrap_or(0.0) as f32;
    let xi = nv.as_i64().unwrap_or(0) as i32;
    Ok(match (v, n) {
        (Vector2(_, y), "x") => Vector2(x, *y),
        (Vector2(a, _), "y") => Vector2(*a, x),
        (Vector3(_, y, z), "x") => Vector3(x, *y, *z),
        (Vector3(a, _, z), "y") => Vector3(*a, x, *z),
        (Vector3(a, b, _), "z") => Vector3(*a, *b, x),
        (Vector4(_, y, z, w), "x") => Vector4(x, *y, *z, *w),
        (Vector4(a, _, z, w), "y") => Vector4(*a, x, *z, *w),
        (Vector4(a, b, _, w), "z") => Vector4(*a, *b, x, *w),
        (Vector4(a, b, c, _), "w") => Vector4(*a, *b, *c, x),
        (Vector2i(_, y), "x") => Vector2i(xi, *y),
        (Vector2i(a, _), "y") => Vector2i(*a, xi),
        (Vector3i(_, y, z), "x") => Vector3i(xi, *y, *z),
        (Vector3i(a, _, z), "y") => Vector3i(*a, xi, *z),
        (Vector3i(a, b, _), "z") => Vector3i(*a, *b, xi),
        (Color(_, g, b, a), "r") => Color(x, *g, *b, *a),
        (Color(r, _, b, a), "g") => Color(*r, x, *b, *a),
        (Color(r, g, _, a), "b") => Color(*r, *g, x, *a),
        (Color(r, g, b, _), "a") => Color(*r, *g, *b, x),
        (Rect2(_, _, w, h), "position") => match nv {
            Vector2(px, py) => Rect2(px, py, *w, *h),
            _ => v.clone(),
        },
        (Rect2(px, py, _, _), "size") => match nv {
            Vector2(w, h) => Rect2(*px, *py, w, h),
            _ => v.clone(),
        },
        (Transform3D { basis, .. }, "origin") => match nv {
            Vector3(x, y, z) => Transform3D { basis: *basis, origin: [x, y, z] },
            _ => v.clone(),
        },
        (Transform3D { origin, .. }, "basis") => match nv {
            Basis(b) => Transform3D { basis: b, origin: *origin },
            _ => v.clone(),
        },
        (AABB { size, .. }, "position") => match nv {
            Vector3(x, y, z) => AABB { position: [x, y, z], size: *size },
            _ => v.clone(),
        },
        (AABB { position, .. }, "size") => match nv {
            Vector3(x, y, z) => AABB { position: *position, size: [x, y, z] },
            _ => v.clone(),
        },
        (Dictionary(d), key) => {
            d.borrow_mut().set(Value::str(key), nv);
            v.clone()
        }
        _ => return err(format!("Invalid assignment of property or key '{n}' on a base object of type '{}'.", v.type_name())),
    })
}

pub fn index_get(b: &Value, i: &Value) -> SResult {
    match b {
        Value::Array(a) => {
            let a = a.borrow();
            let Some(mut idx) = i.as_i64() else { return err("Array index must be an int.") };
            if idx < 0 {
                idx += a.len() as i64;
            }
            a.get(idx as usize).cloned().filter(|_| idx >= 0).ok_or_else(|| format!("Out of bounds get index '{i}' (on base: 'Array')"))
        }
        Value::Dictionary(d) => d.borrow().get(i).cloned().ok_or_else(|| format!("Invalid access to key '{i}' on a base object of type 'Dictionary'.")),
        Value::String(s) | Value::StringName(s) => {
            let chars: Vec<char> = s.chars().collect();
            let mut idx = i.as_i64().unwrap_or(0);
            if idx < 0 {
                idx += chars.len() as i64;
            }
            chars.get(idx as usize).map(|c| Value::str(&c.to_string())).ok_or_else(|| format!("Out of bounds get index '{i}' (on base: 'String')"))
        }
        Value::Vector2(..) | Value::Vector3(..) | Value::Vector4(..) | Value::Vector2i(..) | Value::Vector3i(..) | Value::Color(..) => match i {
            Value::Int(n) => attr_get(b, ["x", "y", "z", "w"].get(*n as usize).copied().unwrap_or("?")).or_else(|_| attr_get(b, ["r", "g", "b", "a"].get(*n as usize).copied().unwrap_or("?"))),
            other => attr_get(b, &other.to_string()),
        },
        other => err(format!("Invalid access of index '{i}' on a base object of type '{}'.", other.type_name())),
    }
}

pub fn index_set(b: &Value, i: &Value, v: Value) -> SResult {
    let key = match i {
        Value::Int(n) => match b {
            Value::Color(..) => ["r", "g", "b", "a"].get(*n as usize).copied().unwrap_or("?").to_string(),
            _ => ["x", "y", "z", "w"].get(*n as usize).copied().unwrap_or("?").to_string(),
        },
        other => other.to_string(),
    };
    attr_set(b, &key, v)
}

fn vlen(c: &[f32]) -> f32 {
    c.iter().map(|x| x * x).sum::<f32>().sqrt()
}

/// Methods on built-in values.
pub fn method(it: &mut Interp, v: &Value, n: &str, a: &[Value]) -> RResult<Value> {
    let fail = |m: String| -> RResult<Value> { Err(ScriptError { message: m, file: String::new(), line: 0, func: String::new() }) };
    let f = |i: usize| a.get(i).and_then(Value::as_f64).unwrap_or(0.0);
    match v {
        Value::String(s) | Value::StringName(s) | Value::NodePath(s) => string_method(s, n, a).map_or_else(|| fail(format!("Invalid call. Nonexistent function '{n}' in base 'String'.")), Ok),
        Value::Array(arr) => array_method(it, arr, n, a),
        Value::Dictionary(d) => dict_method(it, d, n, a),
        Value::Int(_) | Value::Float(_) => fail(format!("Invalid call. Nonexistent function '{n}' in base '{}'.", v.type_name())),
        Value::Basis(m) => Ok(match n {
            "scaled" => {
                let s = match a.first().unwrap_or(&Value::Vector3(1.0, 1.0, 1.0)) {
                    Value::Vector3(x, y, z) => [*x, *y, *z],
                    _ => [1.0; 3],
                };
                Value::Basis([[m[0][0] * s[0], m[0][1] * s[1], m[0][2] * s[2]], [m[1][0] * s[0], m[1][1] * s[1], m[1][2] * s[2]], [m[2][0] * s[0], m[2][1] * s[1], m[2][2] * s[2]]])
            }
            "transposed" => Value::Basis([[m[0][0], m[1][0], m[2][0]], [m[0][1], m[1][1], m[2][1]], [m[0][2], m[1][2], m[2][2]]]),
            "determinant" => Value::Float((m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0]) + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])) as f64),
            "get_scale" => Value::Vector3(
                (m[0][0] * m[0][0] + m[1][0] * m[1][0] + m[2][0] * m[2][0]).sqrt(),
                (m[0][1] * m[0][1] + m[1][1] * m[1][1] + m[2][1] * m[2][1]).sqrt(),
                (m[0][2] * m[0][2] + m[1][2] * m[1][2] + m[2][2] * m[2][2]).sqrt(),
            ),
            "inverse" | "affine_inverse" | "orthonormalized" | "rotated" => v.clone(),
            _ => return fail(format!("Invalid call. Nonexistent function '{n}' in base 'Basis'.")),
        }),
        Value::Transform3D { basis, origin } => Ok(match n {
            "scaled" => {
                let s = match a.first().unwrap_or(&Value::Vector3(1.0, 1.0, 1.0)) {
                    Value::Vector3(x, y, z) => [*x, *y, *z],
                    _ => [1.0; 3],
                };
                Value::Transform3D { basis: [[basis[0][0] * s[0], basis[0][1] * s[1], basis[0][2] * s[2]], [basis[1][0] * s[0], basis[1][1] * s[1], basis[1][2] * s[2]], [basis[2][0] * s[0], basis[2][1] * s[1], basis[2][2] * s[2]]], origin: *origin }
            }
            "translated" => {
                let d = match a.first().unwrap_or(&Value::Vector3(0.0, 0.0, 0.0)) {
                    Value::Vector3(x, y, z) => [*x, *y, *z],
                    _ => [0.0; 3],
                };
                Value::Transform3D { basis: *basis, origin: [origin[0] + d[0], origin[1] + d[1], origin[2] + d[2]] }
            }
            "inverse" | "affine_inverse" | "orthonormalized" | "looking_at" => v.clone(),
            "basis" => Value::Basis(*basis),
            "origin" => Value::Vector3(origin[0], origin[1], origin[2]),
            _ => return fail(format!("Invalid call. Nonexistent function '{n}' in base 'Transform3D'.")),
        }),
        Value::Color(r, g, b, al) => Ok(match n {
            "lerp" => match a.first() {
                Some(Value::Color(r2, g2, b2, a2)) => {
                    let t = f(1) as f32;
                    Value::Color(r + (r2 - r) * t, g + (g2 - g) * t, b + (b2 - b) * t, al + (a2 - al) * t)
                }
                _ => v.clone(),
            },
            "lightened" => {
                let t = f(0) as f32;
                Value::Color(r + (1.0 - r) * t, g + (1.0 - g) * t, b + (1.0 - b) * t, *al)
            }
            "darkened" => {
                let t = 1.0 - f(0) as f32;
                Value::Color(r * t, g * t, b * t, *al)
            }
            "inverted" => Value::Color(1.0 - r, 1.0 - g, 1.0 - b, *al),
            "to_html" => {
                let c = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
                let with_alpha = a.first().is_none_or(Value::truthy);
                Value::str(&if with_alpha { format!("{:02x}{:02x}{:02x}{:02x}", c(*r), c(*g), c(*b), c(*al)) } else { format!("{:02x}{:02x}{:02x}", c(*r), c(*g), c(*b)) })
            }
            "is_equal_approx" => Value::Bool(match a.first() {
                Some(Value::Color(r2, g2, b2, a2)) => (r - r2).abs() < 1e-5 && (g - g2).abs() < 1e-5 && (b - b2).abs() < 1e-5 && (al - a2).abs() < 1e-5,
                _ => false,
            }),
            "get_luminance" => Value::Float((0.2126 * r + 0.7152 * g + 0.0722 * b) as f64),
            _ => return fail(format!("Invalid call. Nonexistent function '{n}' in base 'Color'.")),
        }),
        Value::Rect2(x, y, w, h) => Ok(match n {
            "has_point" => match a.first() {
                Some(Value::Vector2(px, py)) => Value::Bool(px >= x && py >= y && *px < x + w && *py < y + h),
                _ => Value::Bool(false),
            },
            "get_center" => Value::Vector2(x + w / 2.0, y + h / 2.0),
            "get_area" => Value::Float((w * h) as f64),
            "intersects" => match a.first() {
                Some(Value::Rect2(x2, y2, w2, h2)) => Value::Bool(x < &(x2 + w2) && x2 < &(x + w) && y < &(y2 + h2) && y2 < &(y + h)),
                _ => Value::Bool(false),
            },
            "grow" => {
                let g = f(0) as f32;
                Value::Rect2(x - g, y - g, w + 2.0 * g, h + 2.0 * g)
            }
            _ => return fail(format!("Invalid call. Nonexistent function '{n}' in base 'Rect2'.")),
        }),
        _ => match floats(v) {
            Some((c, kind)) => vector_method(v, &c, kind, n, a).map_or_else(|| fail(format!("Invalid call. Nonexistent function '{n}' in base '{}'.", v.type_name())), Ok),
            None => fail(format!("Invalid call. Nonexistent function '{n}' in base '{}'.", v.type_name())),
        },
    }
}

fn vector_method(v: &Value, c: &[f32], kind: u8, n: &str, a: &[Value]) -> Option<Value> {
    let other = a.first().and_then(floats).map(|(o, _)| o);
    let f = |i: usize| a.get(i).and_then(Value::as_f64).unwrap_or(0.0) as f32;
    let len = vlen(c);
    let fkind = match kind {
        6 => 2,
        10 => 3,
        k => k,
    };
    Some(match n {
        "length" => Value::Float(len as f64),
        "length_squared" => Value::Float((len * len) as f64),
        "normalized" => {
            if len == 0.0 {
                rebuild(fkind, c)
            } else {
                rebuild(fkind, &c.iter().map(|x| x / len).collect::<Vec<_>>())
            }
        }
        "is_normalized" => Value::Bool((len - 1.0).abs() < 1e-4),
        "distance_to" => Value::Float(vlen(&c.iter().zip(other?.iter()).map(|(x, y)| x - y).collect::<Vec<_>>()) as f64),
        "distance_squared_to" => {
            let d = vlen(&c.iter().zip(other?.iter()).map(|(x, y)| x - y).collect::<Vec<_>>());
            Value::Float((d * d) as f64)
        }
        "direction_to" => {
            let d: Vec<f32> = other?.iter().zip(c).map(|(y, x)| y - x).collect();
            let l = vlen(&d);
            rebuild(fkind, &d.iter().map(|x| if l == 0.0 { 0.0 } else { x / l }).collect::<Vec<_>>())
        }
        "dot" => Value::Float(c.iter().zip(other?.iter()).map(|(x, y)| x * y).sum::<f32>() as f64),
        "cross" => {
            let o = other?;
            if c.len() == 2 {
                Value::Float((c[0] * o[1] - c[1] * o[0]) as f64)
            } else {
                Value::Vector3(c[1] * o[2] - c[2] * o[1], c[2] * o[0] - c[0] * o[2], c[0] * o[1] - c[1] * o[0])
            }
        }
        "angle" => Value::Float((c[1] as f64).atan2(c[0] as f64)),
        "angle_to" => {
            let o = other?;
            Value::Float(((c[0] * o[1] - c[1] * o[0]) as f64).atan2((c[0] * o[0] + c[1] * o[1]) as f64))
        }
        "angle_to_point" => {
            let o = other?;
            Value::Float(((o[1] - c[1]) as f64).atan2((o[0] - c[0]) as f64))
        }
        "rotated" if c.len() == 2 => {
            let (s, co) = (f(0) as f64).sin_cos();
            Value::Vector2((c[0] as f64 * co - c[1] as f64 * s) as f32, (c[0] as f64 * s + c[1] as f64 * co) as f32)
        }
        "lerp" => {
            let o = other?;
            let t = f(1);
            rebuild(fkind, &c.iter().zip(o.iter()).map(|(x, y)| x + (y - x) * t).collect::<Vec<_>>())
        }
        "move_toward" => {
            let o = other?;
            let d: Vec<f32> = o.iter().zip(c).map(|(y, x)| y - x).collect();
            let l = vlen(&d);
            let delta = f(1);
            if l <= delta || l < 1e-6 {
                rebuild(fkind, &o)
            } else {
                rebuild(fkind, &c.iter().zip(d.iter()).map(|(x, dd)| x + dd / l * delta).collect::<Vec<_>>())
            }
        }
        "limit_length" => {
            let m = if a.is_empty() { 1.0 } else { f(0) };
            if len > m && len > 0.0 { rebuild(fkind, &c.iter().map(|x| x / len * m).collect::<Vec<_>>()) } else { v.clone() }
        }
        "abs" => rebuild(kind, &c.iter().map(|x| x.abs()).collect::<Vec<_>>()),
        "floor" => rebuild(kind, &c.iter().map(|x| x.floor()).collect::<Vec<_>>()),
        "ceil" => rebuild(kind, &c.iter().map(|x| x.ceil()).collect::<Vec<_>>()),
        "round" => rebuild(kind, &c.iter().map(|x| x.round()).collect::<Vec<_>>()),
        "sign" => rebuild(kind, &c.iter().map(|x| if *x > 0.0 { 1.0 } else if *x < 0.0 { -1.0 } else { 0.0 }).collect::<Vec<_>>()),
        "clamp" => {
            let (lo, _) = floats(a.first()?)?;
            let (hi, _) = floats(a.get(1)?)?;
            rebuild(kind, &c.iter().enumerate().map(|(i, x)| x.max(lo[i]).min(hi[i])).collect::<Vec<_>>())
        }
        "snapped" => {
            let (s, _) = floats(a.first()?)?;
            rebuild(kind, &c.iter().enumerate().map(|(i, x)| if s[i] == 0.0 { *x } else { (x / s[i] + 0.5).floor() * s[i] }).collect::<Vec<_>>())
        }
        "is_zero_approx" => Value::Bool(c.iter().all(|x| x.abs() < 1e-5)),
        "is_equal_approx" => Value::Bool(c.iter().zip(other?.iter()).all(|(x, y)| (x - y).abs() < 1e-5)),
        "min_axis_index" => Value::Int(c.iter().enumerate().min_by(|a, b| a.1.total_cmp(b.1)).map(|x| x.0 as i64).unwrap_or(0)),
        "max_axis_index" => Value::Int(c.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).map(|x| x.0 as i64).unwrap_or(0)),
        "orthogonal" if c.len() == 2 => Value::Vector2(c[1], -c[0]),
        "aspect" if c.len() == 2 => Value::Float((c[0] / c[1]) as f64),
        "reflect" => {
            let nrm = other?;
            let d = 2.0 * c.iter().zip(nrm.iter()).map(|(x, y)| x * y).sum::<f32>();
            rebuild(fkind, &nrm.iter().zip(c).map(|(nn, x)| d * nn - x).collect::<Vec<_>>())
        }
        "bounce" => {
            let nrm = other?;
            let d = 2.0 * c.iter().zip(nrm.iter()).map(|(x, y)| x * y).sum::<f32>();
            rebuild(fkind, &nrm.iter().zip(c).map(|(nn, x)| x - d * nn).collect::<Vec<_>>())
        }
        "slide" => {
            let nrm = other?;
            let d = c.iter().zip(nrm.iter()).map(|(x, y)| x * y).sum::<f32>();
            rebuild(fkind, &nrm.iter().zip(c).map(|(nn, x)| x - d * nn).collect::<Vec<_>>())
        }
        _ => return None,
    })
}

fn string_method(s: &str, n: &str, a: &[Value]) -> Option<Value> {
    let arg = |i: usize| a.get(i).map(|v| v.to_string()).unwrap_or_default();
    let int = |i: usize| a.get(i).and_then(Value::as_i64).unwrap_or(0);
    let chars: Vec<char> = s.chars().collect();
    let sub = |from: i64, len: i64| -> String {
        let from = from.max(0) as usize;
        let len = if len < 0 { chars.len().saturating_sub(from) } else { len as usize };
        chars.iter().skip(from).take(len).collect()
    };
    Some(match n {
        "length" => Value::Int(chars.len() as i64),
        "is_empty" => Value::Bool(s.is_empty()),
        "to_upper" => Value::str(&s.to_uppercase()),
        "to_lower" => Value::str(&s.to_lowercase()),
        "capitalize" => Value::str(&capitalize(s)),
        "strip_edges" => {
            let (l, r) = (a.first().is_none_or(Value::truthy), a.get(1).is_none_or(Value::truthy));
            let t = if l { s.trim_start() } else { s };
            Value::str(if r { t.trim_end() } else { t })
        }
        "lstrip" => Value::str(s.trim_start_matches(|c| arg(0).contains(c))),
        "rstrip" => Value::str(s.trim_end_matches(|c| arg(0).contains(c))),
        "contains" => Value::Bool(s.contains(arg(0).as_str())),
        "containsn" => Value::Bool(s.to_lowercase().contains(&arg(0).to_lowercase())),
        "begins_with" => Value::Bool(s.starts_with(arg(0).as_str())),
        "ends_with" => Value::Bool(s.ends_with(arg(0).as_str())),
        "trim_prefix" => Value::str(s.strip_prefix(arg(0).as_str()).unwrap_or(s)),
        "trim_suffix" => Value::str(s.strip_suffix(arg(0).as_str()).unwrap_or(s)),
        "sha256_text" | "md5_text" => Value::str(&format!("{:016x}", s.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3)))),
        "hex_to_int" => Value::Int(i64::from_str_radix(s.trim_start_matches("0x"), 16).unwrap_or(0)),
        "is_valid_hex_number" => Value::Bool(!s.is_empty() && s.trim_start_matches("0x").chars().all(|c| c.is_ascii_hexdigit())),
        "find" => Value::Int(s.find(arg(0).as_str()).map(|b| s[..b].chars().count() as i64).unwrap_or(-1)),
        "rfind" => Value::Int(s.rfind(arg(0).as_str()).map(|b| s[..b].chars().count() as i64).unwrap_or(-1)),
        "findn" => Value::Int(s.to_lowercase().find(&arg(0).to_lowercase()).map(|b| b as i64).unwrap_or(-1)),
        "count" => Value::Int(if arg(0).is_empty() { 0 } else { s.matches(arg(0).as_str()).count() as i64 }),
        "replace" => Value::str(&s.replace(arg(0).as_str(), &arg(1))),
        "replacen" => Value::str(&s.replacen(arg(0).as_str(), &arg(1), 1)),
        "split" => {
            let sep = arg(0);
            let allow_empty = a.get(1).is_none_or(Value::truthy);
            let parts: Vec<Value> = if sep.is_empty() { chars.iter().map(|c| Value::str(&c.to_string())).collect() } else { s.split(sep.as_str()).filter(|p| allow_empty || !p.is_empty()).map(Value::str).collect() };
            Value::array(parts)
        }
        "split_floats" => Value::array(s.split(arg(0).as_str()).map(|p| Value::Float(p.trim().parse().unwrap_or(0.0))).collect()),
        "join" => match a.first() {
            Some(Value::Array(x)) => Value::str(&x.borrow().iter().map(|v| v.to_string()).collect::<Vec<_>>().join(s)),
            _ => Value::str(""),
        },
        "substr" => Value::str(&sub(int(0), if a.len() > 1 { int(1) } else { -1 })),
        "left" => {
            let k = int(0);
            let k = if k < 0 { (chars.len() as i64 + k).max(0) } else { k };
            Value::str(&sub(0, k))
        }
        "right" => {
            let k = int(0);
            let k = if k < 0 { (chars.len() as i64 + k).max(0) } else { k };
            Value::str(&sub(chars.len() as i64 - k, k))
        }
        "to_int" => Value::Int(string_to_int(s)),
        "to_float" => Value::Float(s.trim().parse().unwrap_or(0.0)),
        "is_valid_int" => Value::Bool(s.trim().parse::<i64>().is_ok()),
        "is_valid_float" => Value::Bool(s.trim().parse::<f64>().is_ok()),
        "pad_zeros" => {
            let k = int(0) as usize;
            let (sign, rest) = if let Some(r) = s.strip_prefix('-') { ("-", r) } else { ("", s) };
            let int_part = rest.split('.').next().unwrap_or("").len();
            Value::str(&format!("{sign}{}{rest}", "0".repeat(k.saturating_sub(int_part))))
        }
        "pad_decimals" => {
            let k = int(0) as usize;
            let v: f64 = s.parse().unwrap_or(0.0);
            Value::str(&format!("{v:.k$}"))
        }
        "lpad" => {
            let k = int(0) as usize;
            let c = if a.len() > 1 { arg(1) } else { " ".into() };
            Value::str(&format!("{}{s}", c.repeat(k.saturating_sub(chars.len()))))
        }
        "rpad" => {
            let k = int(0) as usize;
            let c = if a.len() > 1 { arg(1) } else { " ".into() };
            Value::str(&format!("{s}{}", c.repeat(k.saturating_sub(chars.len()))))
        }
        "repeat" => Value::str(&s.repeat(int(0).max(0) as usize)),
        "reverse" => Value::str(&chars.iter().rev().collect::<String>()),
        "format" => {
            let mut out = s.to_string();
            match a.first() {
                Some(Value::Dictionary(d)) => {
                    for (k, v) in d.borrow().iter() {
                        out = out.replace(&format!("{{{k}}}"), &v.to_string());
                    }
                }
                Some(Value::Array(x)) => {
                    for (i, v) in x.borrow().iter().enumerate() {
                        out = out.replace(&format!("{{{i}}}"), &v.to_string());
                    }
                }
                _ => {}
            }
            Value::str(&out)
        }
        "unicode_at" => Value::Int(chars.get(int(0) as usize).map(|c| *c as i64).unwrap_or(0)),
        "hash" => Value::Int(s.bytes().fold(5381i64, |h, b| h.wrapping_mul(33) ^ b as i64) & 0xffff_ffff),
        "is_absolute_path" => Value::Bool(s.starts_with("res://") || s.starts_with("user://") || s.starts_with('/')),
        "get_file" => Value::str(s.rsplit('/').next().unwrap_or(s)),
        "get_base_dir" => Value::str(s.rsplit_once('/').map(|(d, _)| d).unwrap_or("")),
        "get_extension" => Value::str(s.rsplit_once('.').map(|(_, e)| e).filter(|e| !e.contains('/')).unwrap_or("")),
        "get_basename" => Value::str(s.rsplit_once('.').map(|(b, _)| b).unwrap_or(s)),
        "path_join" => {
            let other = arg(0);
            let joined = if other.is_empty() { s.to_string() } else if s.is_empty() { other } else { format!("{}/{}", s.trim_end_matches('/'), other.trim_start_matches('/')) };
            Value::str(&joined)
        }
        "simplify_path" => Value::str(&simplify_path(s)),
        "to_snake_case" => Value::str(&to_snake(s)),
        "to_camel_case" | "to_pascal_case" => {
            let mut out = String::new();
            let mut up = n == "to_pascal_case";
            for c in s.chars() {
                if c == '_' || c == ' ' {
                    up = true;
                } else if up {
                    out.extend(c.to_uppercase());
                    up = false;
                } else {
                    out.push(c);
                }
            }
            Value::str(&out)
        }
        "similarity" => Value::Float(if s == arg(0) { 1.0 } else { 0.0 }),
        "c_escape" => Value::str(&s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")),
        "json_escape" => Value::str(&s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")),
        "to_utf8_buffer" | "to_ascii_buffer" => Value::array(s.bytes().map(|b| Value::Int(b as i64)).collect()),
        "erase" => {
            let (from, cnt) = (int(0).max(0) as usize, int(1).max(0) as usize);
            Value::str(&chars.iter().enumerate().filter(|(i, _)| *i < from || *i >= from + cnt).map(|(_, c)| c).collect::<String>())
        }
        "insert" => {
            let at = (int(0).max(0) as usize).min(chars.len());
            let mut out: String = chars[..at].iter().collect();
            out.push_str(&arg(1));
            out.extend(chars[at..].iter());
            Value::str(&out)
        }
        "naturalnocasecmp_to" | "nocasecmp_to" | "casecmp_to" => {
            let (x, y) = if n == "casecmp_to" { (s.to_string(), arg(0)) } else { (s.to_lowercase(), arg(0).to_lowercase()) };
            Value::Int(match x.cmp(&y) {
                core::cmp::Ordering::Less => -1,
                core::cmp::Ordering::Equal => 0,
                core::cmp::Ordering::Greater => 1,
            })
        }
        _ => return None,
    })
}

fn simplify_path(s: &str) -> String {
    let absolute = s.starts_with('/');
    let mut parts: Vec<&str> = Vec::new();
    for p in s.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                if !parts.is_empty() {
                    parts.pop();
                }
            }
            x => parts.push(x),
        }
    }
    let body = parts.join("/");
    if absolute { format!("/{body}") } else { body }
}

fn to_snake(s: &str) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.extend(c.to_lowercase());
    }
    out
}

/// `String.capitalize()`: "move_speed" -> "Move Speed".
fn capitalize(s: &str) -> String {
    to_snake(s)
        .split(['_', ' '])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn array_method(it: &mut Interp, arr: &Rc<core::cell::RefCell<Vec<Value>>>, n: &str, a: &[Value]) -> RResult<Value> {
    let fail = |m: String| -> RResult<Value> { Err(ScriptError { message: m, file: String::new(), line: 0, func: String::new() }) };
    let int = |i: usize| a.get(i).and_then(Value::as_i64).unwrap_or(0);
    let callable = |i: usize| match a.get(i) {
        Some(Value::Callable(c)) => Some(c.clone()),
        _ => None,
    };
    Ok(match n {
        "size" => Value::Int(arr.borrow().len() as i64),
        "is_empty" => Value::Bool(arr.borrow().is_empty()),
        "append" | "push_back" => {
            arr.borrow_mut().push(a.first().cloned().unwrap_or(Value::Nil));
            Value::Nil
        }
        "push_front" => {
            arr.borrow_mut().insert(0, a.first().cloned().unwrap_or(Value::Nil));
            Value::Nil
        }
        "append_array" => {
            if let Some(Value::Array(o)) = a.first() {
                let o = o.borrow().clone();
                arr.borrow_mut().extend(o);
            }
            Value::Nil
        }
        "insert" => {
            let mut v = arr.borrow_mut();
            let at = (int(0).max(0) as usize).min(v.len());
            v.insert(at, a.get(1).cloned().unwrap_or(Value::Nil));
            Value::Int(0)
        }
        "pop_back" => arr.borrow_mut().pop().unwrap_or(Value::Nil),
        "pop_front" => {
            let mut v = arr.borrow_mut();
            if v.is_empty() { Value::Nil } else { v.remove(0) }
        }
        "pop_at" => {
            let mut v = arr.borrow_mut();
            let mut i = int(0);
            if i < 0 {
                i += v.len() as i64;
            }
            if i >= 0 && (i as usize) < v.len() { v.remove(i as usize) } else { Value::Nil }
        }
        "remove_at" => {
            let mut v = arr.borrow_mut();
            let i = int(0);
            if i >= 0 && (i as usize) < v.len() {
                v.remove(i as usize);
            } else {
                drop(v);
                return fail(format!("Index p_index = {i} is out of bounds."));
            }
            Value::Nil
        }
        "erase" => {
            let mut v = arr.borrow_mut();
            if let Some(x) = a.first()
                && let Some(i) = v.iter().position(|e| equals(e, x))
            {
                v.remove(i);
            }
            Value::Nil
        }
        "clear" => {
            arr.borrow_mut().clear();
            Value::Nil
        }
        "has" => Value::Bool(a.first().is_some_and(|x| arr.borrow().iter().any(|e| equals(e, x)))),
        "find" => Value::Int(a.first().and_then(|x| arr.borrow().iter().skip(int(1).max(0) as usize).position(|e| equals(e, x)).map(|i| i as i64 + int(1).max(0))).unwrap_or(-1)),
        "rfind" => Value::Int(a.first().and_then(|x| arr.borrow().iter().rposition(|e| equals(e, x)).map(|i| i as i64)).unwrap_or(-1)),
        "count" => Value::Int(a.first().map(|x| arr.borrow().iter().filter(|e| equals(e, x)).count() as i64).unwrap_or(0)),
        "front" => arr.borrow().first().cloned().unwrap_or(Value::Nil),
        "back" => arr.borrow().last().cloned().unwrap_or(Value::Nil),
        "get" => arr.borrow().get(int(0) as usize).cloned().unwrap_or(Value::Nil),
        "set" => {
            if let Some(slot) = arr.borrow_mut().get_mut(int(0) as usize) {
                *slot = a.get(1).cloned().unwrap_or(Value::Nil);
            }
            Value::Nil
        }
        "resize" => {
            arr.borrow_mut().resize(int(0).max(0) as usize, Value::Nil);
            Value::Int(0)
        }
        "fill" => {
            let x = a.first().cloned().unwrap_or(Value::Nil);
            for e in arr.borrow_mut().iter_mut() {
                *e = x.clone();
            }
            Value::Nil
        }
        "reverse" => {
            arr.borrow_mut().reverse();
            Value::Nil
        }
        "duplicate" => {
            let deep = a.first().is_some_and(Value::truthy);
            Value::array(arr.borrow().iter().map(|v| if deep { deep_copy(v) } else { v.clone() }).collect())
        }
        "slice" => {
            let v = arr.borrow();
            let len = v.len() as i64;
            let fix = |i: i64| if i < 0 { (i + len).max(0) } else { i.min(len) };
            let (b, e) = (fix(int(0)), if a.len() > 1 { fix(int(1)) } else { len });
            Value::array(if b < e { v[b as usize..e as usize].to_vec() } else { Vec::new() })
        }
        "sort" => {
            arr.borrow_mut().sort_by(compare);
            Value::Nil
        }
        "sort_custom" => {
            let Some(c) = callable(0) else { return fail("sort_custom() needs a Callable.".into()) };
            let mut v = arr.borrow().clone();
            // Insertion sort with the script comparator (stable, avoids panics on bad comparators).
            for i in 1..v.len() {
                let mut j = i;
                while j > 0 {
                    let less = it.call_callable(&c, &[v[j].clone(), v[j - 1].clone()])?.truthy();
                    if !less {
                        break;
                    }
                    v.swap(j, j - 1);
                    j -= 1;
                }
            }
            *arr.borrow_mut() = v;
            Value::Nil
        }
        "shuffle" => {
            let mut v = arr.borrow_mut();
            for i in (1..v.len()).rev() {
                let j = (rand_u32(it) as usize) % (i + 1);
                v.swap(i, j);
            }
            Value::Nil
        }
        "pick_random" => {
            let v = arr.borrow();
            if v.is_empty() { Value::Nil } else { v[rand_u32(it) as usize % v.len()].clone() }
        }
        "max" | "min" => {
            let v = arr.borrow();
            let mut best: Option<Value> = None;
            for x in v.iter() {
                best = Some(match best {
                    None => x.clone(),
                    Some(b) => {
                        let o = compare(x, &b);
                        if (n == "max" && o.is_gt()) || (n == "min" && o.is_lt()) { x.clone() } else { b }
                    }
                });
            }
            best.unwrap_or(Value::Nil)
        }
        "map" | "filter" | "any" | "all" | "reduce" => {
            let Some(c) = callable(0) else { return fail(format!("{n}() needs a Callable.")) };
            let v = arr.borrow().clone();
            match n {
                "map" => {
                    let mut out = Vec::with_capacity(v.len());
                    for x in v {
                        out.push(it.call_callable(&c, &[x])?);
                    }
                    Value::array(out)
                }
                "filter" => {
                    let mut out = Vec::new();
                    for x in v {
                        if it.call_callable(&c, core::slice::from_ref(&x))?.truthy() {
                            out.push(x);
                        }
                    }
                    Value::array(out)
                }
                "any" => {
                    for x in v {
                        if it.call_callable(&c, &[x])?.truthy() {
                            return Ok(Value::Bool(true));
                        }
                    }
                    Value::Bool(false)
                }
                "all" => {
                    for x in v {
                        if !it.call_callable(&c, &[x])?.truthy() {
                            return Ok(Value::Bool(false));
                        }
                    }
                    Value::Bool(true)
                }
                _ => {
                    let mut iter = v.into_iter();
                    let mut acc = match a.get(1) {
                        Some(init) => init.clone(),
                        None => iter.next().unwrap_or(Value::Nil),
                    };
                    for x in iter {
                        acc = it.call_callable(&c, &[acc, x])?;
                    }
                    acc
                }
            }
        }
        "bsearch" => {
            let x = a.first().cloned().unwrap_or(Value::Nil);
            let v = arr.borrow();
            Value::Int(v.partition_point(|e| compare(e, &x).is_lt()) as i64)
        }
        "hash" => Value::Int(arr.borrow().len() as i64),
        // PackedByteArray binary access (little endian, like Godot).
        "encode_s16" | "encode_u16" | "encode_s32" | "encode_u32" | "encode_float" => {
            let at = int(0).max(0) as usize;
            let val = a.get(1).cloned().unwrap_or(Value::Int(0));
            let bytes: Vec<u8> = match n {
                "encode_s16" => (val.as_i64().unwrap_or(0) as i16).to_le_bytes().to_vec(),
                "encode_u16" => (val.as_i64().unwrap_or(0) as u16).to_le_bytes().to_vec(),
                "encode_s32" => (val.as_i64().unwrap_or(0) as i32).to_le_bytes().to_vec(),
                "encode_u32" => (val.as_i64().unwrap_or(0) as u32).to_le_bytes().to_vec(),
                _ => (val.as_f64().unwrap_or(0.0) as f32).to_le_bytes().to_vec(),
            };
            let mut v = arr.borrow_mut();
            if v.len() < at + bytes.len() {
                v.resize(at + bytes.len(), Value::Int(0));
            }
            for (i, b) in bytes.into_iter().enumerate() {
                v[at + i] = Value::Int(b as i64);
            }
            Value::Nil
        }
        "decode_s16" | "decode_u16" | "decode_s32" | "decode_u32" | "decode_float" => {
            let at = int(0).max(0) as usize;
            let v = arr.borrow();
            let byte = |i: usize| v.get(at + i).and_then(Value::as_i64).unwrap_or(0) as u8;
            match n {
                "decode_s16" => Value::Int(i16::from_le_bytes([byte(0), byte(1)]) as i64),
                "decode_u16" => Value::Int(u16::from_le_bytes([byte(0), byte(1)]) as i64),
                "decode_s32" => Value::Int(i32::from_le_bytes([byte(0), byte(1), byte(2), byte(3)]) as i64),
                "decode_u32" => Value::Int(u32::from_le_bytes([byte(0), byte(1), byte(2), byte(3)]) as i64),
                _ => Value::Float(f32::from_le_bytes([byte(0), byte(1), byte(2), byte(3)]) as f64),
            }
        }
        "is_typed" | "is_read_only" => Value::Bool(false),
        "make_read_only" | "assign" => {
            if let (true, Some(Value::Array(o))) = (n == "assign", a.first()) {
                let o = o.borrow().clone();
                *arr.borrow_mut() = o;
            }
            Value::Nil
        }
        _ => return fail(format!("Invalid call. Nonexistent function '{n}' in base 'Array'.")),
    })
}

fn deep_copy(v: &Value) -> Value {
    match v {
        Value::Array(a) => Value::array(a.borrow().iter().map(deep_copy).collect()),
        Value::Dictionary(d) => {
            let mut out = Dict::new();
            for (k, x) in d.borrow().iter() {
                out.set(k.clone(), deep_copy(x));
            }
            Value::dict(out)
        }
        other => other.clone(),
    }
}

/// Ordering used by `Array.sort()`.
fn compare(a: &Value, b: &Value) -> core::cmp::Ordering {
    use core::cmp::Ordering;
    match binary(BinOp::Lt, a, b) {
        Ok(Value::Bool(true)) => Ordering::Less,
        _ => match binary(BinOp::Lt, b, a) {
            Ok(Value::Bool(true)) => Ordering::Greater,
            _ => Ordering::Equal,
        },
    }
}

fn dict_method(it: &mut Interp, d: &Rc<core::cell::RefCell<Dict>>, n: &str, a: &[Value]) -> RResult<Value> {
    let _ = it;
    let key = a.first().cloned().unwrap_or(Value::Nil);
    Ok(match n {
        "size" => Value::Int(d.borrow().len() as i64),
        "is_empty" => Value::Bool(d.borrow().is_empty()),
        "has" => Value::Bool(d.borrow().has(&key)),
        "has_all" => Value::Bool(match a.first() {
            Some(Value::Array(ks)) => ks.borrow().iter().all(|k| d.borrow().has(k)),
            _ => false,
        }),
        "get" => d.borrow().get(&key).cloned().unwrap_or_else(|| a.get(1).cloned().unwrap_or(Value::Nil)),
        "get_or_add" => {
            let existing = d.borrow().get(&key).cloned();
            match existing {
                Some(v) => v,
                None => {
                    let v = a.get(1).cloned().unwrap_or(Value::Nil);
                    d.borrow_mut().set(key, v.clone());
                    v
                }
            }
        }
        "set" => {
            d.borrow_mut().set(key, a.get(1).cloned().unwrap_or(Value::Nil));
            Value::Bool(true)
        }
        "erase" => Value::Bool(d.borrow_mut().erase(&key)),
        "clear" => {
            d.borrow_mut().clear();
            Value::Nil
        }
        "keys" => Value::array(d.borrow().keys()),
        "values" => Value::array(d.borrow().values()),
        "find_key" => d.borrow().iter().find(|(_, v)| equals(v, &key)).map(|(k, _)| k.clone()).unwrap_or(Value::Nil),
        "merge" | "merged" => {
            let overwrite = a.get(1).is_some_and(Value::truthy);
            let target = if n == "merged" { Rc::new(core::cell::RefCell::new(d.borrow().clone())) } else { d.clone() };
            if let Some(Value::Dictionary(o)) = a.first() {
                for (k, v) in o.borrow().iter() {
                    let has = target.borrow().has(k);
                    if overwrite || !has {
                        target.borrow_mut().set(k.clone(), v.clone());
                    }
                }
            }
            if n == "merged" { Value::Dictionary(target) } else { Value::Nil }
        }
        "duplicate" => {
            let deep = a.first().is_some_and(Value::truthy);
            if deep { deep_copy(&Value::Dictionary(d.clone())) } else { Value::dict(d.borrow().clone()) }
        }
        "hash" => Value::Int(d.borrow().len() as i64),
        "is_read_only" | "is_typed" => Value::Bool(false),
        "make_read_only" => Value::Nil,
        _ => return Err(ScriptError { message: format!("Invalid call. Nonexistent function '{n}' in base 'Dictionary'."), file: String::new(), line: 0, func: String::new() }),
    })
}

/// Methods every Object has (Godot's `Object` class), handled before the host.
pub fn object_method(it: &mut Interp, obj: &Object, n: &str, a: &[Value]) -> Option<RResult<Value>> {
    let name_arg = || a.first().map(|v| v.to_string()).unwrap_or_default();
    Some(match n {
        "has_method" => {
            let m = name_arg();
            Ok(Value::Bool(it.has_method(obj, &m)))
        }
        "call" => {
            let m = name_arg();
            it.call_method(obj, &m, &a[a.len().min(1)..])
        }
        "call_deferred" => Ok(Value::Nil),
        "callv" => {
            let m = name_arg();
            let args = match a.get(1) {
                Some(Value::Array(x)) => x.borrow().clone(),
                _ => Vec::new(),
            };
            it.call_method(obj, &m, &args)
        }
        "get" => {
            let p = name_arg();
            let own = obj.0.vars.borrow().get(&p).cloned();
            Ok(own.or_else(|| it.with_host(|h, _| h.get_property(obj, &p))).unwrap_or(Value::Nil))
        }
        "set" => {
            let p = name_arg();
            it.member_set(obj, &p, a.get(1).cloned().unwrap_or(Value::Nil)).map(|_| Value::Nil)
        }
        "connect" => {
            let sig = name_arg();
            let Some(Value::Callable(c)) = a.get(1) else { return Some(Err(ScriptError { message: "connect() needs a Callable.".into(), file: String::new(), line: 0, func: String::new() })) };
            let flags = a.get(2).and_then(Value::as_i64).unwrap_or(0);
            it.connect(obj, &sig, c.clone(), flags);
            Ok(Value::Int(0))
        }
        "disconnect" => {
            let sig = name_arg();
            if let Some(Value::Callable(c)) = a.get(1)
                && let Some(l) = obj.0.connections.borrow_mut().get_mut(&sig)
            {
                l.retain(|x| !(Rc::ptr_eq(&x.callable, c) || x.callable.same_target(c)));
            }
            Ok(Value::Nil)
        }
        "is_connected" => {
            let sig = name_arg();
            let r = match a.get(1) {
                Some(Value::Callable(c)) => obj.0.connections.borrow().get(&sig).is_some_and(|l| l.iter().any(|x| Rc::ptr_eq(&x.callable, c) || x.callable.same_target(c))),
                _ => false,
            };
            Ok(Value::Bool(r))
        }
        "emit_signal" => {
            let sig = name_arg();
            it.emit(obj, &sig, &a[a.len().min(1)..]).map(|_| Value::Int(0))
        }
        "has_signal" => Ok(Value::Bool(obj.script().is_some_and(|s| s.has_signal(&name_arg())))),
        "get_script" => Ok(obj.script().map(|c| Value::Object(Object::new(ObjKind::Class(c), "GDScript", None))).unwrap_or(Value::Nil)),
        "get_instance_id" => Ok(Value::Int(obj.id() as i64)),
        "is_class" => {
            let c = name_arg();
            Ok(Value::Bool(obj.0.native_class == c || it.with_host(|h, _| h.is_class(obj, &c))))
        }
        "get_class" => Ok(Value::str(&obj.0.native_class)),
        "to_string" | "_to_string" if obj.script().and_then(|s| s.find_func("_to_string")).is_none() => Ok(Value::str(&obj.describe())),
        "free" if obj.0.native.get() == 0 => {
            obj.0.freed.set(true);
            Ok(Value::Nil)
        }
        "notification" | "property_list_changed" | "notify_property_list_changed" => Ok(Value::Nil),
        "set_meta" => {
            let key = format!("meta/{}", a.first().map(|v| v.to_string()).unwrap_or_default());
            obj.0.vars.borrow_mut().insert(key, a.get(1).cloned().unwrap_or(Value::Nil));
            Ok(Value::Nil)
        }
        "get_meta" => {
            let key = format!("meta/{}", a.first().map(|v| v.to_string()).unwrap_or_default());
            Ok(obj.0.vars.borrow().get(&key).cloned().or_else(|| a.get(1).cloned()).unwrap_or(Value::Nil))
        }
        "has_meta" => {
            let key = format!("meta/{}", a.first().map(|v| v.to_string()).unwrap_or_default());
            Ok(Value::Bool(obj.0.vars.borrow().contains_key(&key)))
        }
        "remove_meta" => {
            let key = format!("meta/{}", a.first().map(|v| v.to_string()).unwrap_or_default());
            obj.0.vars.borrow_mut().remove(&key);
            Ok(Value::Nil)
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprintf_matches_godot() {
        assert_eq!(format_string("%d/%d", &Value::array(vec![Value::Int(3), Value::Int(10)])).unwrap(), "3/10");
        assert_eq!(format_string("%.2f", &Value::Float(1.23456)).unwrap(), "1.23");
        assert_eq!(format_string("%05d", &Value::Int(-42)).unwrap(), "-0042");
        assert_eq!(format_string("[%-4s]", &Value::str("ab")).unwrap(), "[ab  ]");
        assert_eq!(format_string("100%%", &Value::array(vec![])).unwrap(), "100%");
    }

    #[test]
    fn arithmetic_follows_godot() {
        assert!(matches!(binary(BinOp::Div, &Value::Int(7), &Value::Int(2)).unwrap(), Value::Int(3)));
        assert!(matches!(binary(BinOp::Div, &Value::Float(7.0), &Value::Int(2)).unwrap(), Value::Float(f) if f == 3.5));
        assert!(binary(BinOp::Div, &Value::Int(1), &Value::Int(0)).is_err());
        assert!(matches!(binary(BinOp::Mul, &Value::Vector2(1.0, 2.0), &Value::Int(3)).unwrap(), Value::Vector2(x, y) if x == 3.0 && y == 6.0));
        assert!(binary(BinOp::Add, &Value::str("a"), &Value::Int(1)).is_err());
    }
}
