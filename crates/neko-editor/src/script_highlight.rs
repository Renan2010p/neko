//! NekoScript syntax colors: Godot's `GDScriptSyntaxHighlighter`
//! (modules/gdscript/editor/gdscript_highlighter.cpp) with the default dark
//! editor theme colors (editor/themes/editor_theme_manager.cpp).

use std::collections::HashSet;
use std::rc::Rc;

use neko_ui::{Color32, Highlighter, Spans};

fn rgb(r: f32, g: f32, b: f32) -> Color32 {
    Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

fn rgba(r: f32, g: f32, b: f32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8, (a * 255.0) as u8)
}

/// `text_editor/theme/highlighting/*` for a dark theme.
struct Colors {
    symbol: Color32,
    keyword: Color32,
    control_flow: Color32,
    base_type: Color32,
    engine_type: Color32,
    user_type: Color32,
    comment: Color32,
    doc_comment: Color32,
    string: Color32,
    number: Color32,
    function: Color32,
    member: Color32,
    function_definition: Color32,
    global_function: Color32,
    node_path: Color32,
    node_reference: Color32,
    annotation: Color32,
    string_name: Color32,
}

impl Colors {
    fn dark() -> Self {
        Self {
            symbol: rgb(0.67, 0.79, 1.0),
            keyword: rgb(1.0, 0.44, 0.52),
            control_flow: rgb(1.0, 0.55, 0.8),
            base_type: rgb(0.26, 1.0, 0.76),
            engine_type: rgb(0.56, 1.0, 0.86),
            user_type: rgb(0.78, 1.0, 0.93),
            // dim_color: the font color at half alpha.
            comment: rgba(1.0, 1.0, 1.0, 0.375),
            doc_comment: rgba(0.6, 0.7, 0.8, 0.8),
            string: rgb(1.0, 0.93, 0.63),
            number: rgb(0.63, 1.0, 0.88),
            function: rgb(0.34, 0.7, 1.0),
            // function_color lerped 60% to the mono color.
            member: rgb(0.736, 0.88, 1.0),
            function_definition: rgb(0.4, 0.9, 1.0),
            global_function: rgb(0.64, 0.64, 0.96),
            node_path: rgb(0.72, 0.77, 0.49),
            node_reference: rgb(0.39, 0.76, 0.35),
            annotation: rgb(1.0, 0.7, 0.45),
            string_name: rgb(1.0, 0.76, 0.65),
        }
    }
}

pub(crate) const CONTROL_FLOW: &[&str] = &["break", "continue", "elif", "else", "for", "if", "match", "pass", "return", "when", "while"];

/// GDScriptTokenizer keywords plus the reserved constants.
pub(crate) const KEYWORDS: &[&str] = &[
    "as", "and", "assert", "await", "breakpoint", "class", "class_name", "const", "enum", "extends", "func", "in", "is", "namespace", "not", "or",
    "preload", "self", "signal", "static", "super", "trait", "var", "void", "yield", "true", "false", "null", "PI", "TAU", "INF", "NAN",
];

/// Variant types (`base_type_color`).
pub(crate) const BUILTIN_TYPES: &[&str] = &[
    "bool", "int", "float", "String", "StringName", "NodePath", "Vector2", "Vector2i", "Rect2", "Rect2i", "Vector3", "Vector3i", "Vector4", "Vector4i",
    "Transform2D", "Transform3D", "Plane", "Quaternion", "AABB", "Basis", "Projection", "Color", "RID", "Object", "Callable", "Signal", "Dictionary",
    "Array", "PackedByteArray", "PackedInt32Array", "PackedInt64Array", "PackedFloat32Array", "PackedFloat64Array", "PackedStringArray",
    "PackedVector2Array", "PackedVector3Array", "PackedColorArray", "PackedVector4Array", "Variant",
];

/// Engine classes Neko's ClassDB does not list as nodes (singletons, resources).
pub(crate) const ENGINE_EXTRA: &[&str] = &[
    "Input", "InputEvent", "InputEventKey", "InputEventMouseButton", "InputEventMouseMotion", "InputEventJoypadButton", "InputEventJoypadMotion",
    "InputEventAction", "InputMap", "Engine", "OS", "Time", "ResourceLoader", "ResourceSaver", "ProjectSettings", "DisplayServer", "SceneTree",
    "PackedScene", "Resource", "RefCounted", "Texture2D", "Image", "Tween", "RandomNumberGenerator", "FileAccess", "DirAccess", "JSON",
    "PhysicsServer2D", "RenderingServer", "AudioServer", "AudioStream", "Shape2D", "RectangleShape2D", "CircleShape2D", "CapsuleShape2D",
    "KinematicCollision2D", "SceneTreeTimer", "Marshalls", "ClassDB", "GDScript", "Mathf", "Font", "Theme", "StyleBox", "Shader", "Material",
    "ShaderMaterial", "Mesh", "Curve", "Gradient", "Environment", "World2D", "World3D", "Viewport", "Window", "MainLoop", "WeakRef",
];

/// `@GlobalScope` and `@GDScript` functions (`gdscript/global_function_color`).
pub(crate) const GLOBAL_FUNCTIONS: &[&str] = &[
    "abs", "absf", "absi", "acos", "acosh", "angle_difference", "asin", "asinh", "atan", "atan2", "atanh", "bezier_derivative", "bezier_interpolate",
    "bytes_to_var", "ceil", "ceilf", "ceili", "clamp", "clampf", "clampi", "cos", "cosh", "cubic_interpolate", "db_to_linear", "deg_to_rad", "ease",
    "error_string", "exp", "floor", "floorf", "floori", "fmod", "fposmod", "hash", "instance_from_id", "inverse_lerp", "is_equal_approx",
    "is_finite", "is_inf", "is_instance_id_valid", "is_instance_valid", "is_nan", "is_same", "is_zero_approx", "lerp", "lerp_angle", "lerpf",
    "linear_to_db", "log", "max", "maxf", "maxi", "min", "minf", "mini", "move_toward", "nearest_po2", "pingpong", "posmod", "pow", "print",
    "print_rich", "print_verbose", "printerr", "printraw", "prints", "printt", "push_error", "push_warning", "rad_to_deg", "rand_from_seed",
    "randf", "randf_range", "randfn", "randi", "randi_range", "randomize", "remap", "rotate_toward", "round", "roundf", "roundi", "seed", "sign",
    "signf", "signi", "sin", "sinh", "smoothstep", "snapped", "snappedf", "snappedi", "sqrt", "step_decimals", "str", "str_to_var", "tan", "tanh",
    "type_convert", "type_string", "typeof", "var_to_bytes", "var_to_str", "weakref", "wrap", "wrapf", "wrapi", "Color8", "assert", "char",
    "convert", "dict_to_inst", "get_stack", "inst_to_dict", "is_instance_of", "len", "load", "ord", "preload", "print_debug", "print_stack",
    "range", "type_exists",
];

struct Ctx {
    c: Colors,
    keywords: HashSet<&'static str>,
    control: HashSet<&'static str>,
    builtin: HashSet<&'static str>,
    engine: HashSet<String>,
    globals: HashSet<&'static str>,
    members: HashSet<String>,
}

/// Highlighter for a script; `members` are the base class's properties
/// (Godot colors them like `self.x` even without the dot).
pub fn gdscript(members: HashSet<String>) -> Highlighter {
    let mut engine: HashSet<String> = neko::core::ClassDb::get().all().iter().map(|c| c.name.to_string()).collect();
    engine.extend(ENGINE_EXTRA.iter().map(|s| s.to_string()));
    let ctx = Ctx {
        c: Colors::dark(),
        keywords: KEYWORDS.iter().copied().collect(),
        control: CONTROL_FLOW.iter().copied().collect(),
        builtin: BUILTIN_TYPES.iter().copied().collect(),
        engine,
        globals: GLOBAL_FUNCTIONS.iter().copied().collect(),
        members,
    };
    Rc::new(move |line: &str, state: u32| highlight_line(&ctx, line, state))
}

/// The base class's property names, for `extends <class>`.
pub fn members_of(script: &str) -> HashSet<String> {
    let symbols = crate::script_completion::analyze(script);
    let mut out = crate::script_completion::base_members(&symbols);
    // Members every class sees.
    out.extend(["name", "owner"].map(String::from));
    out
}

/// States carried between lines: inside a `"""` or `'''` string.
const IN_TRIPLE_DQ: u32 = 1;
const IN_TRIPLE_SQ: u32 = 2;

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// End (exclusive) of a string starting at `i` (the opening quote), or None if it runs past the line.
fn string_end(ch: &[char], i: usize, quote: char, raw: bool) -> Option<usize> {
    let mut j = i + 1;
    while j < ch.len() {
        if ch[j] == '\\' && !raw {
            j += 2;
            continue;
        }
        if ch[j] == quote {
            return Some(j + 1);
        }
        j += 1;
    }
    None
}

fn triple_end(ch: &[char], from: usize, quote: char) -> Option<usize> {
    let mut j = from;
    while j + 3 <= ch.len() {
        if ch[j] == '\\' {
            j += 2;
            continue;
        }
        if ch[j] == quote && ch[j + 1] == quote && ch[j + 2] == quote {
            return Some(j + 3);
        }
        j += 1;
    }
    None
}

fn highlight_line(x: &Ctx, line: &str, state: u32) -> (Spans, u32) {
    let ch: Vec<char> = line.chars().collect();
    let n = ch.len();
    let mut spans: Spans = Vec::new();
    let mut i = 0;
    // Continue a multi-line string.
    if state == IN_TRIPLE_DQ || state == IN_TRIPLE_SQ {
        let q = if state == IN_TRIPLE_DQ { '"' } else { '\'' };
        match triple_end(&ch, 0, q) {
            Some(e) => {
                spans.push((0, e, x.c.string));
                i = e;
            }
            None => return (vec![(0, n, x.c.string)], state),
        }
    }
    // The word before the current one ("func", "extends"...).
    let mut prev_word = String::new();
    let mut expect_type = false;
    while i < n {
        let c = ch[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        let prev_sig = ch[..i].iter().rev().find(|c| !c.is_whitespace()).copied();
        // Comments.
        if c == '#' {
            let color = if ch.get(i + 1) == Some(&'#') { x.c.doc_comment } else { x.c.comment };
            spans.push((i, n, color));
            break;
        }
        // Strings, with the r / & / ^ prefixes.
        let (prefix, q_at) = match c {
            'r' | '&' | '^' if matches!(ch.get(i + 1), Some('"') | Some('\'')) && !(c == 'r' && prev_sig.is_some_and(is_ident) && i > 0 && is_ident(ch[i - 1])) => (Some(c), i + 1),
            _ => (None, i),
        };
        if matches!(ch.get(q_at), Some('"') | Some('\'')) && (prefix.is_some() || q_at == i) {
            let q = ch[q_at];
            let color = match prefix {
                Some('&') => x.c.string_name,
                Some('^') => x.c.node_path,
                _ => x.c.string,
            };
            if ch.get(q_at + 1) == Some(&q) && ch.get(q_at + 2) == Some(&q) {
                match triple_end(&ch, q_at + 3, q) {
                    Some(e) => {
                        spans.push((i, e, color));
                        i = e;
                        continue;
                    }
                    None => {
                        spans.push((i, n, color));
                        return (spans, if q == '"' { IN_TRIPLE_DQ } else { IN_TRIPLE_SQ });
                    }
                }
            }
            let e = string_end(&ch, q_at, q, prefix == Some('r')).unwrap_or(n);
            spans.push((i, e, color));
            i = e;
            continue;
        }
        // Annotations.
        if c == '@' && ch.get(i + 1).is_some_and(|c| is_ident_start(*c)) {
            let mut e = i + 1;
            while e < n && is_ident(ch[e]) {
                e += 1;
            }
            spans.push((i, e, x.c.annotation));
            i = e;
            continue;
        }
        // $NodePath and %UniqueName.
        let operand_before = prev_sig.is_some_and(|p| is_ident(p) || p == ')' || p == ']' || p == '"' || p == '\'');
        if c == '$' || (c == '%' && !operand_before && ch.get(i + 1).is_some_and(|c| is_ident_start(*c) || *c == '"')) {
            let color = if c == '$' { x.c.node_path } else { x.c.node_reference };
            let mut e = i + 1;
            if matches!(ch.get(e), Some('"') | Some('\'')) {
                e = string_end(&ch, e, ch[e], false).unwrap_or(n);
            } else {
                while e < n && (is_ident(ch[e]) || ch[e] == '/' || (c == '$' && ch[e] == '%')) {
                    e += 1;
                }
            }
            spans.push((i, e, color));
            i = e;
            continue;
        }
        // Numbers.
        if c.is_ascii_digit() || (c == '.' && ch.get(i + 1).is_some_and(|d| d.is_ascii_digit()) && !operand_before) {
            let mut e = i + 1;
            let hex = c == '0' && matches!(ch.get(i + 1), Some('x') | Some('X'));
            while e < n {
                let d = ch[e];
                let ok = d.is_ascii_digit()
                    || d == '_'
                    || (d == '.' && !hex && ch.get(e + 1).is_none_or(|n| !is_ident_start(*n) || *n == 'e'))
                    || (hex && d.is_ascii_hexdigit())
                    || matches!(d, 'x' | 'X' | 'b' | 'B') && e == i + 1
                    || (!hex && matches!(d, 'e' | 'E'))
                    || (matches!(d, '+' | '-') && matches!(ch[e - 1], 'e' | 'E') && !hex);
                if !ok {
                    break;
                }
                e += 1;
            }
            spans.push((i, e, x.c.number));
            i = e;
            continue;
        }
        // Identifiers and keywords.
        if is_ident_start(c) {
            let mut e = i;
            while e < n && is_ident(ch[e]) {
                e += 1;
            }
            let word: String = ch[i..e].iter().collect();
            let next_sig = ch[e..].iter().find(|c| !c.is_whitespace()).copied();
            let after_dot = prev_sig == Some('.');
            let color = if x.control.contains(word.as_str()) && !after_dot {
                Some(x.c.control_flow)
            } else if x.keywords.contains(word.as_str()) && !after_dot {
                Some(x.c.keyword)
            } else if prev_word == "func" {
                Some(x.c.function_definition)
            } else if prev_word == "signal" {
                Some(x.c.member)
            } else if x.builtin.contains(word.as_str()) {
                Some(x.c.base_type)
            } else if next_sig == Some('(') {
                if !after_dot && x.globals.contains(word.as_str()) {
                    Some(x.c.global_function)
                } else if x.engine.contains(&word) {
                    Some(x.c.engine_type)
                } else {
                    Some(x.c.function)
                }
            } else if x.engine.contains(&word) && !after_dot {
                Some(x.c.engine_type)
            } else if after_dot {
                Some(x.c.member)
            } else if expect_type || (word.chars().next().is_some_and(char::is_uppercase) && word.chars().any(char::is_lowercase)) {
                Some(x.c.user_type)
            } else if x.members.contains(&word) {
                Some(x.c.member)
            } else {
                None
            };
            if let Some(col) = color {
                spans.push((i, e, col));
            }
            expect_type = matches!(word.as_str(), "extends" | "class_name" | "class" | "is" | "as");
            prev_word = word;
            i = e;
            continue;
        }
        // Symbols. A ':' or '->' is followed by a type.
        expect_type = c == ':' || (c == '>' && i > 0 && ch[i - 1] == '-');
        prev_word.clear();
        spans.push((i, i + 1, x.c.symbol));
        i += 1;
    }
    (spans, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn color_of(line: &str, word: &str) -> Option<Color32> {
        let h = gdscript(["position".to_string()].into_iter().collect());
        let (spans, _) = h(line, 0);
        let start = line.find(word).map(|b| line[..b].chars().count())?;
        spans.iter().find(|(a, b, _)| start >= *a && start < *b).map(|(_, _, c)| *c)
    }

    #[test]
    fn colors_like_godot() {
        let c = Colors::dark();
        assert_eq!(color_of("func _ready() -> void:", "func"), Some(c.keyword));
        assert_eq!(color_of("func _ready() -> void:", "_ready"), Some(c.function_definition));
        assert_eq!(color_of("\tif x > 1:", "if"), Some(c.control_flow));
        assert_eq!(color_of("var v := Vector2(1, 2)", "Vector2"), Some(c.base_type));
        assert_eq!(color_of("var v := Vector2(1, 2)", "1"), Some(c.number));
        assert_eq!(color_of("print(\"hi\")", "print"), Some(c.global_function));
        assert_eq!(color_of("print(\"hi\")", "\"hi\""), Some(c.string));
        assert_eq!(color_of("extends Node2D", "Node2D"), Some(c.engine_type));
        assert_eq!(color_of("@export var speed := 10.0", "@export"), Some(c.annotation));
        assert_eq!(color_of("position.x += 1 # move", "# move"), Some(c.comment));
        assert_eq!(color_of("position.x += 1", "position"), Some(c.member));
        assert_eq!(color_of("position.x += 1", "x"), Some(c.member));
        assert_eq!(color_of("$Player/Sprite.play()", "$Player"), Some(c.node_path));
        assert_eq!(color_of("%Score.text = str(n % 3)", "%Score"), Some(c.node_reference));
        assert_eq!(color_of("Input.is_action_pressed(&\"jump\")", "&\"jump\""), Some(c.string_name));
        assert_eq!(color_of("player.jump()", "jump"), Some(c.function));
    }

    #[test]
    fn triple_quoted_strings_span_lines() {
        let h = gdscript(HashSet::new());
        let (_, s1) = h("var doc = \"\"\"first", 0);
        assert_eq!(s1, IN_TRIPLE_DQ);
        let (spans, s2) = h("second\"\"\" + x", s1);
        assert_eq!(s2, 0);
        assert_eq!(spans[0], (0, 9, Colors::dark().string));
    }
}
