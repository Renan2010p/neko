//! NekoScript code completion: the editor side of Godot's
//! `GDScriptLanguage::complete_code` (`modules/gdscript/gdscript_editor.cpp`).
//!
//! [`analyze`] reads the script's members, functions, parameters and locals
//! (tolerating incomplete code, which is when completion matters most) and
//! [`options`] turns that plus the engine ClassDB into completion entries for
//! the current caret context: member access after `.`, types after `extends`,
//! annotations after `@`, `res://` paths inside strings, or general symbols.

use std::collections::HashSet;

use neko::core::{ClassDb, PropType};
use neko_ui::{CompletionKind, CompletionOption, TextPos};

use crate::script_highlight;

/// A name declared by the script.
#[derive(Clone, Debug)]
pub struct Symbol {
    pub name: String,
    pub kind: CompletionKind,
    /// Declared or inferred type, for completing `symbol.`
    pub ty: Option<String>,
    /// `Some(false)` for functions that take no arguments (`queue_free()`).
    pub takes_args: Option<bool>,
    /// 0-based line of the declaration.
    pub line: usize,
}

/// A `func`, with its parameters and the locals declared before any line.
#[derive(Clone, Debug, Default)]
pub struct FuncInfo {
    pub name: String,
    pub line: usize,
    pub params: Vec<Symbol>,
    pub locals: Vec<Symbol>,
}

#[derive(Clone, Debug)]
pub struct EnumInfo {
    pub name: String,
    pub values: Vec<String>,
}

/// Everything `analyze` learned about one script.
#[derive(Clone, Debug)]
pub struct Symbols {
    /// `class_name Foo`, if any.
    pub class_name: Option<String>,
    /// `extends X`; `Node` when the script has no base type.
    pub base: String,
    pub members: Vec<Symbol>,
    pub funcs: Vec<FuncInfo>,
    pub enums: Vec<EnumInfo>,
}

impl Default for Symbols {
    fn default() -> Self {
        Self { class_name: None, base: "Node".into(), members: Vec::new(), funcs: Vec::new(), enums: Vec::new() }
    }
}

// ---------------------------------------------------------------- analysis

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn indent_of(line: &str) -> usize {
    line.chars().take_while(|c| *c == ' ' || *c == '\t').count()
}

/// A word at the start of `s`, if it is exactly the keyword.
fn keyword<'a>(s: &'a str, kw: &str) -> Option<&'a str> {
    let rest = s.strip_prefix(kw)?;
    rest.chars().next().is_none_or(|c| c.is_whitespace() || c == '(').then_some(rest.trim_start())
}

/// Skip leading annotations (`@export`, `@onready`, ...) and `static`.
fn decl_start(mut s: &str) -> &str {
    loop {
        s = s.trim_start();
        if let Some(rest) = s.strip_prefix('@') {
            let n = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
            s = &rest[n..];
        } else if let Some(rest) = keyword(s, "static") {
            s = rest;
        } else {
            return s;
        }
    }
}

fn ident(s: &str) -> &str {
    let n = s.char_indices().take_while(|(_, c)| is_ident_char(*c)).map(|(i, c)| i + c.len_utf8()).last().unwrap_or(0);
    &s[..n]
}

/// The text inside the parentheses starting at `open`.
fn paren_body(s: &str, open: usize) -> (&str, usize) {
    let mut depth = 0usize;
    for (i, c) in s.char_indices().skip(open) {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return (&s[open + 1..i], i);
                }
            }
            _ => {}
        }
    }
    (&s[open + 1..], s.len())
}

fn split_top(s: &str, sep: char) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            c if c == sep && depth == 0 => {
                out.push(&s[start..i]);
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    out.push(&s[start..]);
    out
}

fn normalize_type(t: &str) -> Option<String> {
    let mut t = t.trim().trim_end_matches(':').trim();
    if let Some((head, _)) = t.split_once('[')
        && matches!(head, "Array" | "Dictionary" | "Callable" | "Signal")
    {
        return Some(head.to_string());
    }
    t = t.trim();
    let path: Vec<&str> = t.split('.').collect();
    let last = path.last().copied().unwrap_or(t);
    (!last.is_empty() && ident(last) == last).then(|| last.to_string())
}

/// Guess the type of an initializer, as the parser does for `:=`.
fn expr_type(e: &str) -> Option<String> {
    let mut e = e.trim();
    if let Some(rest) = e.strip_prefix("await ") {
        e = rest.trim();
    }
    let first = e.chars().next()?;
    if matches!(first, '"' | '\'') {
        return Some("String".into());
    }
    if first == '&' && matches!(e.chars().nth(1), Some('"') | Some('\'')) {
        return Some("StringName".into());
    }
    if first == '^' && matches!(e.chars().nth(1), Some('"') | Some('\'')) {
        return Some("NodePath".into());
    }
    if e == "true" || e == "false" {
        return Some("bool".into());
    }
    if e.starts_with('[') {
        return Some("Array".into());
    }
    if e.starts_with('{') {
        return Some("Dictionary".into());
    }
    if let Some(rest) = e.strip_prefix("NodePath(") {
        let _ = rest;
        return Some("NodePath".into());
    }
    if let Some(rest) = e.strip_prefix("StringName(") {
        let _ = rest;
        return Some("StringName".into());
    }
    let t = e.trim_start_matches('(');
    let name = ident(t);
    if name.is_empty() {
        // Numbers, optionally signed.
        let n = e.trim_start_matches(['-', '+', ' ']);
        if let Some(c) = n.chars().next()
            && (c.is_ascii_digit() || c == '.')
        {
            return Some(if n.contains('.') || n.contains('e') || n.contains('E') { "float" } else { "int" }.into());
        }
        return None;
    }
    // `Vector2(...)`, `Node2D.new(...)`, `load(...)`.
    let after = t[name.len()..].trim_start();
    let known = matches!(name, "Vector2" | "Vector3" | "Vector2i" | "Vector3i" | "Color" | "Rect2" | "Transform2D" | "Transform3D" | "Basis" | "Quaternion" | "AABB" | "Plane" | "Projection" | "RID") || ClassDb::get().class(name).is_some();
    if (after.starts_with('(') || after.starts_with(".new(")) && (known || after.starts_with(".new(")) {
        return Some(name.to_string());
    }
    if name == "load" || name == "preload" {
        return Some("Resource".into());
    }
    if name == "get_node" || name == "get_parent" || name == "get_child" || name == "find_child" {
        return Some("Node".into());
    }
    None
}

fn parse_var(rest: &str, line: usize, kind: CompletionKind) -> Option<Symbol> {
    let name = ident(rest).to_string();
    if name.is_empty() {
        return None;
    }
    let mut t = rest[name.len()..].trim_start();
    let mut ty = None;
    let mut value = None;
    if let Some(after) = t.strip_prefix(":=") {
        value = Some(after.trim());
    } else if t.starts_with(':') {
        t = &t[1..];
        let end = t.find(['=', '\n']).unwrap_or(t.len());
        ty = normalize_type(&t[..end]);
        let after = t[end..].trim_start();
        if let Some(v) = after.strip_prefix('=') {
            value = Some(v.trim());
        }
    } else if let Some(v) = t.strip_prefix('=') {
        value = Some(v.trim());
    }
    if ty.is_none() {
        ty = value.and_then(expr_type);
    }
    Some(Symbol { name, kind, ty, takes_args: None, line })
}

fn parse_params(params: &str, line: usize) -> Vec<Symbol> {
    split_top(params, ',')
        .into_iter()
        .filter_map(|p| {
            let p = p.trim();
            if p.is_empty() {
                return None;
            }
            let p = p.strip_prefix("...").unwrap_or(p);
            parse_var(p, line, CompletionKind::Variable).map(|mut s| {
                s.kind = CompletionKind::Variable;
                s
            })
        })
        .collect()
}

fn parse_func(rest: &str, line: usize) -> Option<(Symbol, FuncInfo)> {
    let name = ident(rest).to_string();
    if name.is_empty() {
        return None;
    }
    let after = &rest[name.len()..];
    let open = after.find('(')?;
    let (params, close) = paren_body(after, open);
    let params = parse_params(params, line);
    let tail = &after[(close + 1).min(after.len())..];
    let ret = tail.split_once("->").and_then(|(_, t)| normalize_type(t));
    let takes_args = !params.is_empty();
    let sym = Symbol { name: name.clone(), kind: CompletionKind::Function, ty: ret.clone(), takes_args: Some(takes_args), line };
    Some((sym, FuncInfo { name, line, params, locals: Vec::new() }))
}

fn parse_signal(rest: &str, line: usize) -> Option<(Symbol, Vec<Symbol>)> {
    let name = ident(rest).to_string();
    if name.is_empty() {
        return None;
    }
    let after = &rest[name.len()..];
    let params = after.find('(').map(|open| parse_params(paren_body(after, open).0, line)).unwrap_or_default();
    Some((Symbol { name, kind: CompletionKind::Signal, ty: None, takes_args: None, line }, params))
}

fn parse_enum(rest: &str) -> Option<(Option<String>, Vec<String>)> {
    let name = ident(rest);
    let name = (!name.is_empty()).then(|| name.to_string());
    let body = rest.split_once('{').map(|(_, b)| b.split('}').next().unwrap_or(b));
    let values = body.map(|b| {
        split_top(b, ',')
            .into_iter()
            .filter_map(|v| {
                let v = v.trim();
                let k = ident(v);
                (!k.is_empty()).then(|| k.to_string())
            })
            .collect()
    });
    Some((name, values.unwrap_or_default()))
}

/// Parse a script into the symbols completion uses.
pub fn analyze(text: &str) -> Symbols {
    let mut out = Symbols::default();
    let mut current: Option<usize> = None;
    for (i, raw) in text.lines().enumerate() {
        let indent = indent_of(raw);
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let t = decl_start(trimmed);
        if indent == 0 {
            current = None;
            if let Some(rest) = keyword(t, "extends") {
                out.base = rest.trim().trim_matches('"').split('.').next().unwrap_or("Node").to_string();
                if out.base.is_empty() || out.base.starts_with("res://") {
                    out.base = "Node".into();
                }
                continue;
            }
            if let Some(rest) = keyword(t, "class_name") {
                out.class_name = Some(ident(rest).to_string()).filter(|n| !n.is_empty());
                continue;
            }
            if let Some(rest) = keyword(t, "func") {
                if let Some((sym, info)) = parse_func(rest, i) {
                    out.members.push(sym);
                    out.funcs.push(info);
                    current = Some(out.funcs.len() - 1);
                }
                continue;
            }
            if let Some(rest) = keyword(t, "var") {
                if let Some(sym) = parse_var(rest, i, CompletionKind::Variable) {
                    out.members.push(sym);
                }
                continue;
            }
            if let Some(rest) = keyword(t, "const") {
                if let Some(sym) = parse_var(rest, i, CompletionKind::Constant) {
                    out.members.push(sym);
                }
                continue;
            }
            if let Some(rest) = keyword(t, "signal") {
                if let Some((sym, params)) = parse_signal(rest, i) {
                    out.members.push(sym);
                    // Signal argument names complete inside `signal_name.emit(`.
                    let _ = params;
                }
                continue;
            }
            if let Some(rest) = keyword(t, "enum") {
                if let Some((name, values)) = parse_enum(rest) {
                    match name {
                        Some(n) => {
                            out.members.push(Symbol { name: n.clone(), kind: CompletionKind::Enum, ty: None, takes_args: None, line: i });
                            out.enums.push(EnumInfo { name: n, values });
                        }
                        None => {
                            for v in values {
                                out.members.push(Symbol { name: v, kind: CompletionKind::Constant, ty: None, takes_args: None, line: i });
                            }
                        }
                    }
                }
                continue;
            }
            if let Some(rest) = keyword(t, "class") {
                let name = ident(rest).to_string();
                if !name.is_empty() {
                    out.members.push(Symbol { name, kind: CompletionKind::Class, ty: None, takes_args: None, line: i });
                }
                continue;
            }
            continue;
        }
        // Inside a function body: locals that completion can see.
        let Some(fi) = current else { continue };
        let Some(func) = out.funcs.get_mut(fi) else { continue };
        if let Some(rest) = keyword(t, "var") {
            if let Some(sym) = parse_var(rest, i, CompletionKind::Variable) {
                func.locals.push(sym);
            }
        } else if let Some(rest) = keyword(t, "const") {
            if let Some(sym) = parse_var(rest, i, CompletionKind::Constant) {
                func.locals.push(sym);
            }
        } else if let Some(rest) = keyword(t, "for") {
            let name = ident(rest).to_string();
            if !name.is_empty() {
                let after = &rest[name.len()..];
                let ty = after.trim_start().strip_prefix(':').and_then(|t| normalize_type(t.split_whitespace().next().unwrap_or("")));
                func.locals.push(Symbol { name, kind: CompletionKind::Variable, ty, takes_args: None, line: i });
            }
        }
    }
    out
}

// ---------------------------------------------------------------- language data

/// Functions every `Object`/`Node` has in NekoScript (`script_host.rs`).
/// `true` means it takes arguments.
const OBJECT_METHODS: &[(&str, bool)] = &[
    ("get_node", true),
    ("get_node_or_null", true),
    ("has_node", true),
    ("get_parent", false),
    ("get_children", false),
    ("get_child", true),
    ("get_child_count", false),
    ("add_child", true),
    ("remove_child", true),
    ("move_child", true),
    ("queue_free", false),
    ("free", false),
    ("is_inside_tree", false),
    ("get_tree", false),
    ("get_name", false),
    ("get_path", false),
    ("get_owner", false),
    ("set_owner", true),
    ("set_process", true),
    ("set_physics_process", true),
    ("set_process_input", true),
    ("set_process_unhandled_input", true),
    ("is_processing", false),
    ("is_physics_processing", false),
    ("show", false),
    ("hide", false),
    ("is_visible", false),
    ("is_visible_in_tree", false),
    ("add_to_group", true),
    ("remove_from_group", true),
    ("is_in_group", true),
    ("translate", true),
    ("global_translate", true),
    ("rotate", true),
    ("look_at", true),
    ("move_and_slide", false),
    ("is_on_floor", false),
    ("is_on_wall", false),
    ("is_on_ceiling", false),
    ("get_viewport", false),
    ("get_viewport_rect", false),
    ("get_global_mouse_position", false),
    ("get_local_mouse_position", false),
    ("get_index", false),
    ("get_scene_file_path", false),
    ("is_node_ready", false),
    ("find_children", true),
    ("find_child", true),
    ("get_minimum_size", false),
    ("get_combined_minimum_size", false),
    ("queue_redraw", false),
    ("grab_focus", false),
    ("release_focus", false),
    ("set_anchors_preset", true),
    ("set_anchors_and_offsets_preset", true),
    ("queue_sort", false),
    ("get_theme_font", true),
    ("get_theme_font_size", true),
    ("get_theme_color", true),
    ("get_theme_constant", true),
    ("get_theme_stylebox", true),
    ("add_theme_color_override", true),
    ("add_theme_font_size_override", true),
    ("add_theme_constant_override", true),
    ("add_theme_stylebox_override", true),
    ("add_theme_font_override", true),
    ("set_text", true),
    ("get_text", false),
    ("set_position", true),
    ("get_position", false),
    ("set_global_position", true),
    ("get_global_position", false),
    ("set_visible", true),
    ("set_modulate", true),
    ("get_modulate", false),
    ("set_scale", true),
    ("get_scale", false),
    ("set_rotation", true),
    ("get_rotation", false),
    ("start", true),
    ("stop", false),
    ("is_stopped", false),
    ("connect", true),
    ("disconnect", true),
    ("is_connected", true),
    ("emit", true),
    ("set_meta", true),
    ("set_deferred", true),
    ("accept_event", false),
    ("force_update_transform", false),
];

const STRING_METHODS: &[(&str, bool)] = &[
    ("length", false),
    ("is_empty", false),
    ("to_upper", false),
    ("to_lower", false),
    ("capitalize", false),
    ("to_camel_case", false),
    ("to_pascal_case", false),
    ("to_snake_case", false),
    ("contains", true),
    ("begins_with", true),
    ("ends_with", true),
    ("find", true),
    ("rfind", true),
    ("replace", true),
    ("split", true),
    ("join", true),
    ("strip_edges", true),
    ("lstrip", true),
    ("rstrip", true),
    ("substr", true),
    ("left", true),
    ("right", true),
    ("trim_prefix", true),
    ("trim_suffix", true),
    ("to_int", false),
    ("to_float", false),
    ("is_valid_int", false),
    ("is_valid_float", false),
    ("is_valid_hex_number", true),
    ("hex_to_int", false),
    ("format", true),
    ("pad_decimals", true),
    ("pad_zeros", true),
    ("repeat", true),
    ("reverse", false),
    ("split_floats", true),
    ("uri_decode", false),
    ("uri_encode", false),
    ("xml_escape", true),
    ("xml_unescape", false),
    ("c_escape", false),
    ("c_unescape", false),
    ("json_escape", false),
    ("num", true),
    ("dedent", false),
    ("indent", true),
    ("get_slice", true),
    ("get_slice_count", true),
    ("insert", true),
    ("remove_char", true),
    ("remove_at", true),
    ("count", true),
    ("erase", true),
    ("hash", false),
    ("md5_text", false),
    ("sha256_text", false),
    ("validate_filename", false),
    ("validate_node_name", false),
    ("is_absolute_path", false),
    ("is_relative_path", false),
    ("simplify_path", false),
    ("get_base_dir", false),
    ("get_file", false),
    ("get_extension", false),
    ("get_basename", false),
    ("path_join", true),
    ("plus_file", true),
];

const ARRAY_METHODS: &[(&str, bool)] = &[
    ("append", true),
    ("push_back", true),
    ("push_front", true),
    ("append_array", true),
    ("insert", true),
    ("erase", true),
    ("remove_at", true),
    ("pop_back", false),
    ("pop_front", false),
    ("pop_at", true),
    ("size", false),
    ("is_empty", false),
    ("clear", false),
    ("find", true),
    ("rfind", true),
    ("has", true),
    ("count", true),
    ("sort", false),
    ("sort_custom", true),
    ("reverse", false),
    ("duplicate", true),
    ("slice", true),
    ("map", true),
    ("filter", true),
    ("reduce", true),
    ("any", true),
    ("all", true),
    ("max", false),
    ("min", false),
    ("shuffle", false),
    ("pick_random", false),
    ("bsearch", true),
    ("bsearch_custom", true),
    ("resize", true),
    ("fill", true),
    ("back", false),
    ("front", false),
    ("get", true),
    ("set", true),
    ("assign", true),
    ("is_read_only", false),
    ("make_read_only", false),
];

const DICT_METHODS: &[(&str, bool)] = &[
    ("size", false),
    ("is_empty", false),
    ("clear", false),
    ("has", true),
    ("has_all", true),
    ("keys", false),
    ("values", false),
    ("get", true),
    ("get_or_add", true),
    ("set", true),
    ("erase", true),
    ("duplicate", true),
    ("merge", true),
    ("merged", true),
    ("filtered", true),
    ("find_key", true),
    ("hash", false),
    ("is_read_only", false),
    ("make_read_only", false),
    ("is_typed", false),
];

const VEC2_METHODS: &[(&str, bool)] = &[
    ("length", false),
    ("length_squared", false),
    ("normalized", false),
    ("normalize", false),
    ("is_normalized", false),
    ("distance_to", true),
    ("distance_squared_to", true),
    ("direction_to", true),
    ("angle", false),
    ("angle_to", true),
    ("angle_to_point", true),
    ("dot", true),
    ("cross", true),
    ("slide", true),
    ("bounce", true),
    ("reflect", true),
    ("lerp", true),
    ("move_toward", true),
    ("rotated", true),
    ("orthogonal", false),
    ("posmod", true),
    ("snapped", true),
    ("clamp", true),
    ("min", true),
    ("max", true),
    ("floor", false),
    ("ceil", false),
    ("round", false),
    ("abs", false),
    ("sign", false),
    ("is_equal_approx", true),
    ("is_zero_approx", false),
    ("limit_length", true),
    ("aspect", false),
    ("to_vector2i", false),
];

const VEC3_METHODS: &[(&str, bool)] = &[
    ("length", false),
    ("length_squared", false),
    ("normalized", false),
    ("normalize", false),
    ("is_normalized", false),
    ("distance_to", true),
    ("distance_squared_to", true),
    ("direction_to", true),
    ("dot", true),
    ("cross", true),
    ("slide", true),
    ("bounce", true),
    ("reflect", true),
    ("lerp", true),
    ("move_toward", true),
    ("rotated", true),
    ("posmod", true),
    ("snapped", true),
    ("clamp", true),
    ("min", true),
    ("max", true),
    ("floor", false),
    ("ceil", false),
    ("round", false),
    ("abs", false),
    ("sign", false),
    ("is_equal_approx", true),
    ("is_zero_approx", false),
    ("limit_length", true),
    ("to_vector3i", false),
];

const COLOR_METHODS: &[(&str, bool)] = &[
    ("lerp", true),
    ("blend", true),
    ("darkened", true),
    ("lightened", true),
    ("srgb_to_linear", false),
    ("linear_to_srgb", false),
    ("to_html", true),
    ("is_equal_approx", true),
    ("clamp", true),
    ("inverted", false),
];

fn builtin_methods(ty: &str) -> &'static [(&'static str, bool)] {
    match ty {
        "String" | "StringName" | "NodePath" => STRING_METHODS,
        "Array" | "PackedStringArray" | "PackedInt32Array" | "PackedInt64Array" | "PackedFloat32Array" | "PackedFloat64Array" | "PackedVector2Array" | "PackedVector3Array" | "PackedColorArray" | "PackedByteArray" => ARRAY_METHODS,
        "Dictionary" => DICT_METHODS,
        "Vector2" | "Vector2i" => VEC2_METHODS,
        "Vector3" | "Vector3i" | "Vector4" | "Vector4i" => VEC3_METHODS,
        "Color" => COLOR_METHODS,
        _ => &[],
    }
}

fn builtin_constants(ty: &str) -> &'static [&'static str] {
    match ty {
        "Vector2" | "Vector2i" => &["ZERO", "ONE", "INF", "LEFT", "RIGHT", "UP", "DOWN"],
        "Vector3" | "Vector3i" => &["ZERO", "ONE", "INF", "LEFT", "RIGHT", "UP", "DOWN", "FORWARD", "BACK"],
        "Vector4" | "Vector4i" => &["ZERO", "ONE", "INF"],
        "Color" => &["WHITE", "BLACK", "TRANSPARENT", "RED", "GREEN", "BLUE", "YELLOW", "CYAN", "MAGENTA", "GRAY"],
        "PI" => &[],
        _ => &[],
    }
}

fn builtin_properties(ty: &str) -> &'static [(&'static str, &'static str)] {
    match ty {
        "Vector2" | "Vector2i" => &[("x", "float"), ("y", "float")],
        "Vector3" | "Vector3i" => &[("x", "float"), ("y", "float"), ("z", "float")],
        "Vector4" | "Vector4i" => &[("x", "float"), ("y", "float"), ("z", "float"), ("w", "float")],
        "Color" => &[("r", "float"), ("g", "float"), ("b", "float"), ("a", "float")],
        "Rect2" | "Rect2i" => &[("position", "Vector2"), ("size", "Vector2"), ("end", "Vector2")],
        _ => &[],
    }
}

fn builtin_method_return(ty: &str, name: &str) -> Option<&'static str> {
    match ty {
        "String" | "StringName" | "NodePath" => match name {
            "to_upper" | "to_lower" | "capitalize" | "to_camel_case" | "to_pascal_case" | "to_snake_case" | "replace" | "strip_edges" | "lstrip" | "rstrip" | "substr" | "left" | "right" | "trim_prefix" | "trim_suffix" | "uri_decode" | "uri_encode" | "xml_escape" | "xml_unescape" | "c_escape" | "c_unescape" | "json_escape" | "dedent" | "get_slice" | "path_join" | "plus_file" | "num" => Some("String"),
            "split" => Some("PackedStringArray"),
            "length" | "count" | "find" | "rfind" | "to_int" | "hex_to_int" | "hash" | "get_slice_count" => Some("int"),
            "to_float" => Some("float"),
            _ => None,
        },
        "Array" | "PackedStringArray" => match name {
            "duplicate" | "slice" | "filter" | "map" | "keys" | "values" => Some("Array"),
            "pop_back" | "pop_front" | "pop_at" | "max" | "min" | "front" | "back" | "pick_random" => Some("Variant"),
            "size" | "find" | "rfind" | "bsearch" | "count" => Some("int"),
            _ => None,
        },
        "Dictionary" => match name {
            "keys" | "values" => Some("Array"),
            "size" => Some("int"),
            "duplicate" | "merged" | "filtered" => Some("Dictionary"),
            "get" | "get_or_add" => Some("Variant"),
            _ => None,
        },
        "Vector2" | "Vector2i" => match name {
            "normalized" | "slide" | "bounce" | "reflect" | "lerp" | "move_toward" | "rotated" | "snapped" | "clamp" | "min" | "max" | "floor" | "ceil" | "round" | "abs" | "sign" | "limit_length" | "orthogonal" => Some("Vector2"),
            "length" | "length_squared" | "distance_to" | "distance_squared_to" | "angle" | "angle_to" | "angle_to_point" | "dot" | "cross" | "posmod" | "aspect" => Some("float"),
            _ => None,
        },
        "Vector3" | "Vector3i" => match name {
            "normalized" | "slide" | "bounce" | "reflect" | "lerp" | "move_toward" | "rotated" | "snapped" | "clamp" | "min" | "max" | "floor" | "ceil" | "round" | "abs" | "sign" | "limit_length" => Some("Vector3"),
            "length" | "length_squared" | "distance_to" | "distance_squared_to" | "dot" | "cross" | "posmod" => Some("float"),
            _ => None,
        },
        "Color" => match name {
            "lerp" | "blend" | "darkened" | "lightened" | "clamp" | "inverted" => Some("Color"),
            _ => None,
        },
        _ => match name {
            "get_node" | "get_node_or_null" | "get_parent" | "get_child" | "find_child" => Some("Node"),
            "get_children" | "find_children" => Some("Array"),
            "get_tree" => Some("SceneTree"),
            "get_viewport" => Some("Viewport"),
            "get_global_mouse_position" | "get_local_mouse_position" => Some("Vector2"),
            "get_name" | "get_path" | "get_scene_file_path" => Some("String"),
            "get_text" => Some("String"),
            "get_position" | "get_global_position" => Some("Vector2"),
            "get_scale" | "get_modulate" => Some("Vector2"),
            "get_rotation" => Some("float"),
            "get_index" | "get_child_count" => Some("int"),
            "is_inside_tree" | "is_processing" | "is_physics_processing" | "is_visible" | "is_visible_in_tree" | "is_in_group" | "is_on_floor" | "is_on_wall" | "is_on_ceiling" | "has_node" | "is_node_ready" | "is_stopped" | "is_connected" => Some("bool"),
            _ => None,
        },
    }
}

fn prop_type_name(t: PropType) -> &'static str {
    match t {
        PropType::Bool => "bool",
        PropType::Int => "int",
        PropType::Float => "float",
        PropType::String | PropType::Resource => "String",
        PropType::Vec2 => "Vector2",
        PropType::Vec3 => "Vector3",
        PropType::Color => "Color",
    }
}

fn known_class(name: &str) -> bool {
    ClassDb::get().class(name).is_some() || script_highlight::ENGINE_EXTRA.contains(&name)
}

// ---------------------------------------------------------------- options

fn function_option(name: &str, takes_args: bool, location: u8) -> CompletionOption {
    let display = if takes_args { format!("{name}(…)") } else { format!("{name}()") };
    let insert = if takes_args { format!("{name}(") } else { format!("{name}()") };
    CompletionOption::new(display, CompletionKind::Function).insert(insert).location(location)
}

fn symbol_option(sym: &Symbol, location: u8) -> CompletionOption {
    match sym.kind {
        CompletionKind::Function => function_option(&sym.name, sym.takes_args.unwrap_or(true), location),
        kind => CompletionOption::new(sym.name.clone(), kind).location(location),
    }
}

fn local_symbols(symbols: &Symbols, line: usize) -> Vec<&Symbol> {
    let Some(func) = symbols.funcs.iter().filter(|f| f.line <= line).max_by_key(|f| f.line) else { return Vec::new() };
    func.params.iter().chain(func.locals.iter().filter(|s| s.line <= line)).collect()
}

fn find_symbol<'a>(symbols: &'a Symbols, line: usize, name: &str) -> Option<&'a Symbol> {
    local_symbols(symbols, line).into_iter().rev().find(|s| s.name == name).or_else(|| symbols.members.iter().find(|s| s.name == name))
}

fn class_members(ty: &str, location: u8) -> Vec<CompletionOption> {
    let db = ClassDb::get();
    let mut out = Vec::new();
    for (_, prop) in db.properties(ty) {
        out.push(CompletionOption::new(prop.name, CompletionKind::Member).location(location));
    }
    for &name in &["name", "owner"] {
        out.push(CompletionOption::new(name, CompletionKind::Member).location(location));
    }
    for &(name, args) in OBJECT_METHODS {
        out.push(function_option(name, args, location));
    }
    out
}

fn type_members(ty: &str, location: u8) -> Vec<CompletionOption> {
    let mut out = Vec::new();
    for &(name, args) in builtin_methods(ty) {
        out.push(function_option(name, args, location));
    }
    for &(name, prop_ty) in builtin_properties(ty) {
        let _ = prop_ty;
        out.push(CompletionOption::new(name, CompletionKind::Member).location(location));
    }
    for &name in builtin_constants(ty) {
        out.push(CompletionOption::new(name, CompletionKind::Constant).location(location));
    }
    if known_class(ty) {
        out.extend(class_members(ty, location));
    }
    out
}

fn property_type(ty: &str, name: &str) -> Option<String> {
    for &(n, t) in builtin_properties(ty) {
        if n == name {
            return Some(t.to_string());
        }
    }
    ClassDb::get().property(ty, name).map(|p| prop_type_name(p.ty).to_string())
}

fn member_type(symbols: &Symbols, cur: &str, name: &str, call: bool) -> Option<String> {
    if (cur == symbols.base || symbols.class_name.as_deref() == Some(cur))
        && let Some(sym) = symbols.members.iter().find(|s| s.name == name)
    {
        return sym.ty.clone();
    }
    if call {
        return builtin_method_return(cur, name).map(str::to_string);
    }
    property_type(cur, name)
}

fn chain_start(chars: &[char], end: usize) -> usize {
    let mut i = end;
    loop {
        while i > 0 && chars[i - 1].is_whitespace() {
            i -= 1;
        }
        if i == 0 {
            return 0;
        }
        match chars[i - 1] {
            ')' | ']' => {
                let close = chars[i - 1];
                let open_c = if close == ')' { '(' } else { '[' };
                let mut depth = 1;
                let mut j = i - 1;
                while j > 0 {
                    j -= 1;
                    if chars[j] == close {
                        depth += 1;
                    } else if chars[j] == open_c {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                }
                i = j;
            }
            c if is_ident_char(c) => {
                while i > 0 && is_ident_char(chars[i - 1]) {
                    i -= 1;
                }
            }
            '$' | '%' => {
                i -= 1;
            }
            _ => return i,
        }
        let mut j = i;
        while j > 0 && chars[j - 1].is_whitespace() {
            j -= 1;
        }
        if j > 0 && chars[j - 1] == '.' {
            i = j - 1;
            continue;
        }
        return i;
    }
}

fn split_chain(chain: &str) -> Vec<&str> {
    split_top(chain, '.').into_iter().map(str::trim).filter(|s| !s.is_empty()).collect()
}

fn segment_name(seg: &str) -> (&str, bool) {
    let call = seg.contains('(');
    let end = seg.find(['(', '[']).unwrap_or(seg.len());
    let name = seg[..end].trim();
    (name, call)
}

fn resolve_chain(symbols: &Symbols, line: usize, chain: &str) -> Option<String> {
    let segs = split_chain(chain);
    let mut ty: Option<String> = None;
    for (i, seg) in segs.iter().enumerate() {
        let (name, call) = segment_name(seg);
        if name.is_empty() {
            return None;
        }
        if i == 0 {
            if name == "self" {
                ty = Some(symbols.base.clone());
            } else if name.starts_with('$') || name.starts_with('%') {
                return None;
            } else if let Some(sym) = find_symbol(symbols, line, name) {
                ty = sym.ty.clone().or_else(|| matches!(sym.kind, CompletionKind::Class | CompletionKind::Enum).then(|| name.to_string()));
            } else if known_class(name) || script_highlight::BUILTIN_TYPES.contains(&name) {
                ty = Some(name.to_string());
            } else {
                return None;
            }
        } else {
            let cur = ty.take()?;
            ty = member_type(symbols, &cur, name, call);
            ty.as_ref()?;
        }
    }
    ty
}

fn is_in_string(chars: &[char], col: usize) -> Option<usize> {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut i = 0;
    while i < col {
        let c = chars[i];
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == q {
                quote = None;
            }
        } else if c == '#' {
            return None;
        } else if c == '"' || c == '\'' {
            quote = Some(c);
        }
        i += 1;
    }
    quote.map(|_| {
        // The content starts after the opening quote (or the last one).
        let mut j = col;
        while j > 0 && chars[j - 1] != '"' && chars[j - 1] != '\'' {
            j -= 1;
        }
        j
    })
}

fn annotations() -> &'static [&'static str] {
    &[
        "export", "export_range", "export_enum", "export_file", "export_dir", "export_global_file", "export_global_dir", "export_multiline",
        "export_placeholder", "export_color_no_alpha", "export_node_path", "export_flags", "export_flags_2d_physics", "export_flags_2d_render",
        "export_flags_3d_physics", "export_flags_3d_render", "export_group", "export_subgroup", "export_category", "export_custom",
        "export_storage", "export_tool_button", "onready", "tool", "icon", "rpc", "static_unload", "abstract", "warning_ignore",
        "warning_ignore_start", "warning_ignore_restore",
    ]
}

fn type_options(symbols: &Symbols) -> Vec<CompletionOption> {
    let mut out = Vec::new();
    for &t in script_highlight::BUILTIN_TYPES {
        out.push(CompletionOption::new(t, CompletionKind::Class));
    }
    if let Some(name) = &symbols.class_name {
        out.push(CompletionOption::new(name.clone(), CompletionKind::Class));
    }
    for c in ClassDb::get().all() {
        out.push(CompletionOption::new(c.name, CompletionKind::Class).location(1));
    }
    for &name in script_highlight::ENGINE_EXTRA {
        out.push(CompletionOption::new(name, CompletionKind::Class).location(1));
    }
    out
}

fn general_options(symbols: &Symbols, line: usize) -> Vec<CompletionOption> {
    let mut out = Vec::new();
    for &kw in script_highlight::CONTROL_FLOW.iter().chain(script_highlight::KEYWORDS.iter()) {
        out.push(CompletionOption::new(kw, CompletionKind::Keyword).location(1));
    }
    out.extend(type_options(symbols));
    for &f in script_highlight::GLOBAL_FUNCTIONS {
        out.push(function_option(f, true, 1));
    }
    for sym in local_symbols(symbols, line) {
        out.push(symbol_option(sym, 0));
    }
    for sym in &symbols.members {
        out.push(symbol_option(sym, 0));
    }
    out.extend(class_members(&symbols.base, 1));
    out
}

fn member_options(symbols: &Symbols, base_ty: Option<String>, own: bool) -> Vec<CompletionOption> {
    let mut out = Vec::new();
    if own {
        for sym in &symbols.members {
            out.push(symbol_option(sym, 0));
        }
    }
    match &base_ty {
        Some(ty) if script_highlight::BUILTIN_TYPES.contains(&ty.as_str()) && !known_class(ty) => out.extend(type_members(ty, 1)),
        Some(ty) => out.extend(type_members(ty, 1)),
        None => {
            for sym in &symbols.members {
                out.push(symbol_option(sym, 0));
            }
            out.extend(class_members(&symbols.base, 1));
        }
    }
    if let Some(ty) = &base_ty {
        for e in &symbols.enums {
            if &e.name == ty {
                for v in &e.values {
                    out.push(CompletionOption::new(v.clone(), CompletionKind::Constant));
                }
            }
        }
    }
    out
}

/// Case-insensitive subsequence test, the same match the CodeEdit uses.
fn is_subsequence(needle: &str, haystack: &str) -> bool {
    let mut chars = needle.chars().flat_map(char::to_lowercase);
    let mut want = chars.next();
    for c in haystack.chars().flat_map(char::to_lowercase) {
        if Some(c) == want {
            want = chars.next();
            if want.is_none() {
                return true;
            }
        }
    }
    want.is_none()
}

fn file_options(prefix: &str, files: &[String]) -> Vec<CompletionOption> {
    let mut out = Vec::new();
    for f in files {
        if prefix.is_empty() || is_subsequence(prefix, f) {
            out.push(CompletionOption::new(f.clone(), CompletionKind::FilePath));
            if out.len() >= 300 {
                break;
            }
        }
    }
    out
}

/// Everything outside the script that completion can reference: `res://`
/// paths, `$Node/Paths` and `%UniqueNames` from the open scene.
#[derive(Default)]
pub struct Sources<'a> {
    pub files: &'a [String],
    pub nodes: &'a [String],
    pub unique: &'a [String],
}

/// Completion entries for the caret. Returns the options and where the typed
/// prefix starts (the CodeEdit replaces from there on accept).
#[cfg(test)]
pub fn options(symbols: &Symbols, text: &str, caret: TextPos, files: &[String], forced: bool) -> (Vec<CompletionOption>, TextPos) {
    options_with(symbols, text, caret, &Sources { files, ..Sources::default() }, forced)
}

/// Like [`options`], with scene node paths and unique names.
pub fn options_with(symbols: &Symbols, text: &str, caret: TextPos, sources: &Sources<'_>, forced: bool) -> (Vec<CompletionOption>, TextPos) {
    let files = sources.files;
    let line_text = text.lines().nth(caret.line).unwrap_or("");
    let chars: Vec<char> = line_text.chars().collect();
    let col = caret.col.min(chars.len());
    let mut start = col;
    while start > 0 && is_ident_char(chars[start - 1]) {
        start -= 1;
    }
    let base = TextPos::new(caret.line, start);
    let prefix: String = chars[start..col].iter().collect();

    // Inside a string: only resource paths (preload/load/extends).
    if let Some(content_start) = is_in_string(&chars, col) {
        let content: String = chars[content_start..col].iter().collect();
        let before: String = chars[..content_start.saturating_sub(1)].iter().collect();
        let context = before.trim_end();
        let wants = content.starts_with("res://")
            || content.starts_with("user://")
            || context.ends_with("preload(")
            || context.ends_with("load(")
            || context.trim_end().ends_with("extends");
        if !wants {
            return (Vec::new(), base);
        }
        return (file_options(&content, files), TextPos::new(caret.line, content_start));
    }
    // A comment before the caret has nothing to complete.
    {
        let mut in_str = false;
        let mut quote = ' ';
        let mut escaped = false;
        for &c in &chars[..col] {
            if in_str {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == quote {
                    in_str = false;
                }
            } else if c == '#' {
                return (Vec::new(), base);
            } else if c == '"' || c == '\'' {
                in_str = true;
                quote = c;
            }
        }
    }

    let prev = chars[..start].iter().rev().find(|c| !c.is_whitespace()).copied();
    let before_prefix: String = chars[..start].iter().collect();
    let trimmed = before_prefix.trim_end();

    // `@` starts an annotation.
    if trimmed.ends_with('@') {
        let opts = annotations().iter().map(|a| CompletionOption::new(*a, CompletionKind::Annotation)).collect();
        return (opts, base);
    }

    // `.` starts member access.
    if prev == Some('.') {
        let chars_before = trimmed.chars().collect::<Vec<_>>();
        let dot = chars_before.len().saturating_sub(1);
        let e_start = chain_start(&chars_before, dot);
        let expr: String = chars_before[e_start..dot].iter().collect();
        let expr = expr.trim();
        let ty = resolve_chain(symbols, caret.line, expr);
        let own = expr == "self" || expr.is_empty() || symbols.class_name.as_deref() == Some(expr);
        return (member_options(symbols, ty, own), base);
    }

    // `$Node/Path` and `%UniqueName` from the open scene (also after `/`).
    if let Some(at) = chars[..col].iter().rposition(|c| *c == '$' || *c == '%') {
        let rest: String = chars[at + 1..col].iter().collect();
        let pathy = rest.chars().all(|c| !c.is_whitespace() && !matches!(c, '"' | '\'' | ')' | ']' | '}' | ',' | '(' | '[' | '{'));
        // `%` is also the modulo operator; only an operand position is a
        // unique-name reference.
        let unique_marker = at == 0 || {
            let p = chars[at - 1];
            !is_ident_char(p) && !matches!(p, ')' | ']' | '"' | '\'')
        };
        if pathy && (chars[at] == '$' || unique_marker) {
            let (list, kind) = if chars[at] == '$' { (sources.nodes, CompletionKind::NodePath) } else { (sources.unique, CompletionKind::NodePath) };
            let opts = list.iter().map(|p| CompletionOption::new(p.clone(), kind)).collect();
            return (opts, TextPos::new(caret.line, at + 1));
        }
    }

    // Type positions: after `extends`, `is`, `as`, `->` and in annotations.
    let last_word = trimmed.rsplit(|c: char| c.is_whitespace()).next().unwrap_or("");
    let type_pos = trimmed.ends_with("->")
        || matches!(last_word, "extends" | "is" | "as")
        || (trimmed.ends_with(':') && (trimmed.contains("var ") || trimmed.contains("const ") || trimmed.contains("func ") || last_word.contains(')') || last_word.contains(',')));
    if type_pos {
        return (type_options(symbols), base);
    }

    // Declaration names (`func |`, `var |`) have nothing useful to offer.
    if trimmed.ends_with("func") || trimmed.ends_with("class_name") || trimmed.ends_with("signal") {
        return (Vec::new(), base);
    }

    if prefix.is_empty() && !forced {
        return (Vec::new(), base);
    }
    (general_options(symbols, caret.line), base)
}

/// The base class's property names (used by the syntax highlighter).
pub fn base_members(symbols: &Symbols) -> HashSet<String> {
    ClassDb::get().properties(&symbols.base).into_iter().map(|(_, p)| p.name.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCRIPT: &str = r#"class_name Player
extends CharacterBody2D

const SPEED := 300.0
@export var health: int = 100
var sprite: Sprite2D
var target := Vector2(10, 20)
signal died
enum State { IDLE, RUN }

func _ready() -> void:
	var local: Node2D
	var count := 3
	for item in range(3):
		print(item)

func jump() -> void:
	var velocity := Vector2.ZERO
"#;

    #[test]
    fn analyzes_members_types_and_locals() {
        let s = analyze(SCRIPT);
        assert_eq!(s.class_name.as_deref(), Some("Player"));
        assert_eq!(s.base, "CharacterBody2D");
        let names: Vec<&str> = s.members.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"SPEED") && names.contains(&"health") && names.contains(&"sprite") && names.contains(&"jump"));
        let sprite = s.members.iter().find(|m| m.name == "sprite").unwrap();
        assert_eq!(sprite.ty.as_deref(), Some("Sprite2D"));
        let target = s.members.iter().find(|m| m.name == "target").unwrap();
        assert_eq!(target.ty.as_deref(), Some("Vector2"));
        let ready = s.funcs.iter().find(|f| f.name == "_ready").unwrap();
        let locals: Vec<&str> = ready.locals.iter().map(|l| l.name.as_str()).collect();
        assert!(locals.contains(&"local") && locals.contains(&"count") && locals.contains(&"item"));
        assert_eq!(ready.locals.iter().find(|l| l.name == "local").unwrap().ty.as_deref(), Some("Node2D"));
        assert_eq!(s.enums[0].values, ["IDLE", "RUN"]);
    }

    #[test]
    fn completes_self_and_typed_members() {
        let s = analyze(SCRIPT);
        // `self.` offers script members and inherited Node methods.
        let (opts, base) = options(&s, "self.", TextPos::new(0, 5), &[], false);
        assert!(opts.iter().any(|o| o.display == "sprite"));
        assert!(opts.iter().any(|o| o.display == "queue_free()"));
        assert_eq!(base.col, 5);
        // `sprite.` uses the declared type.
        let (opts, _) = options(&s, "sprite.", TextPos::new(0, 7), &[], false);
        assert!(opts.iter().any(|o| o.display == "texture"));
        assert!(opts.iter().any(|o| o.display == "position"));
        // `target.` resolves the inferred Vector2 type.
        let (opts, _) = options(&s, "target.", TextPos::new(0, 7), &[], false);
        assert!(opts.iter().any(|o| o.display == "x"));
        assert!(opts.iter().any(|o| o.display == "normalized()"));
        // Named enum values complete after `Enum.`.
        let (opts, _) = options(&s, "State.", TextPos::new(0, 6), &[], false);
        assert!(opts.iter().any(|o| o.display == "IDLE"));
        assert!(opts.iter().any(|o| o.display == "RUN"));
    }

    #[test]
    fn general_types_and_paths() {
        let s = analyze(SCRIPT);
        let (opts, base) = options(&s, "extends ", TextPos::new(0, 8), &[], false);
        assert!(opts.iter().any(|o| o.display == "Node2D"));
        assert_eq!(base.col, 8);

        let files = vec!["res://player.png".to_string(), "res://sounds/jump.ogg".to_string()];
        let text = "preload(\"res://pla";
        let (opts, base) = options(&s, text, TextPos::new(0, text.chars().count()), &files, false);
        assert_eq!(opts.len(), 1);
        assert_eq!(opts[0].display, "res://player.png");
        assert_eq!(base.col, "preload(\"".len());
        // With the auto-inserted closing quote and parenthesis still there, and
        // the tab auto-indent adds before the statement.
        let text = "\tpreload(\"res://pla\")";
        let (opts, base) = options(&s, text, TextPos::new(0, 19), &files, false);
        assert_eq!(opts.len(), 1);
        assert_eq!(base.col, "\tpreload(\"".len());

        let (opts, _) = options(&s, "pr", TextPos::new(0, 2), &[], false);
        assert!(opts.iter().any(|o| o.display == "print(…)"));

        // Annotations after `@`, without the `@` in the inserted text.
        let (opts, base) = options(&s, "@exp", TextPos::new(0, 4), &[], false);
        assert!(opts.iter().any(|o| o.display == "export"));
        assert_eq!(base.col, 1);

        // Nothing is offered after a blank space unless Ctrl+Space forces it.
        let (opts, _) = options(&s, "", TextPos::new(0, 0), &[], false);
        assert!(opts.is_empty());
        let (opts, _) = options(&s, "", TextPos::new(0, 0), &[], true);
        assert!(opts.iter().any(|o| o.display == "Node2D"));

        // `$` node paths (also after `/`) and `%` unique names from the scene.
        let nodes = vec!["Player/Sprite".to_string(), "Camera2D".to_string()];
        let unique = vec!["Score".to_string()];
        let sources = Sources { files: &[], nodes: &nodes, unique: &unique };
        let (opts, base) = options_with(&s, "$Pla", TextPos::new(0, 4), &sources, false);
        assert!(opts.iter().any(|o| o.display == "Player/Sprite"));
        assert_eq!(base.col, 1);
        let (opts, base) = options_with(&s, "$Player/Spr", TextPos::new(0, 11), &sources, false);
        assert!(opts.iter().any(|o| o.display == "Player/Sprite"));
        assert_eq!(base.col, 1);
        let (opts, _) = options_with(&s, "%Sco", TextPos::new(0, 4), &sources, false);
        assert!(opts.iter().any(|o| o.display == "Score"));
        // Modulo is not a unique-name reference.
        let (opts, _) = options_with(&s, "x%2", TextPos::new(0, 3), &sources, false);
        assert!(opts.iter().all(|o| o.display != "Score"));
    }
}
