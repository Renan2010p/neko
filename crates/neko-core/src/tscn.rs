//! Scenes as Godot `.tscn` files.
//!
//! Neko's node classes are a subset of Godot's, sometimes with friendlier
//! property forms (degrees instead of radians, `position`/`size` instead of
//! Control offsets, a shape name instead of a `Shape2D` sub-resource). This
//! module converts both ways:
//!
//! - load: Godot properties Neko models become Neko properties; everything
//!   else (unknown classes, properties, resources, connections) is kept in
//!   [`GodotNode`]/[`GodotScene`] and written back unchanged;
//! - save: Neko properties become Godot properties. When a property still has
//!   the value it was loaded with, the original text is written, so opening
//!   and saving a Godot scene does not change the file.

#[allow(unused_imports)]
use neko_base::prelude::*;
#[cfg(not(feature = "std"))]
use neko_base::math::FloatExt;

use neko_base::collections::{HashMap, HashSet};

use crate::classdb::ClassDb;
use crate::format::{FormatError, Loaded};
use crate::godot::Value;
use crate::godot::text::{Document, Section};
use crate::godot::uid::{self, IdSource};
use crate::scene::{GodotNode, NodeId, Scene};
use crate::variant::{PropType, Variant};

/// File extension of text scenes.
pub const EXTENSION: &str = "tscn";

/// Godot's text scene format version. 3 is what Godot 4.x writes for
/// ordinary scenes (4 only for a few newer value types).
const FORMAT: i64 = 3;

// ------------------------------------------------------------ class names

/// Neko class name -> Godot class name.
fn godot_class(neko: &str) -> &str {
    match neko {
        "GpuParticles2D" => "GPUParticles2D",
        "GpuParticles3D" => "GPUParticles3D",
        "HttpRequest" => "HTTPRequest",
        other => other,
    }
}

/// Godot class name -> Neko class, if Neko has it (and it is instanceable).
fn neko_class(godot: &str) -> Option<&'static str> {
    let name = match godot {
        "GPUParticles2D" => "GpuParticles2D",
        "GPUParticles3D" => "GpuParticles3D",
        "HTTPRequest" => "HttpRequest",
        other => other,
    };
    ClassDb::get().class(name).filter(|c| !c.is_abstract).map(|c| c.name)
}

/// The Neko class to stand in for a Godot class Neko does not have.
fn stand_in(godot: &str) -> &'static str {
    const UI: [&str; 14] = ["Container", "Button", "Label", "Panel", "Rect", "Edit", "Bar", "Box", "List", "Tree", "Slider", "Separator", "Control", "Popup"];
    if godot.ends_with("2D") {
        "Node2D"
    } else if godot.ends_with("3D") {
        "Node3D"
    } else if UI.iter().any(|w| godot.contains(w)) {
        "Control"
    } else {
        "Node"
    }
}

// ------------------------------------------------------------ enums

const PROCESS_MODE: &[(&str, i64)] = &[("inherit", 0), ("pausable", 1), ("when_paused", 2), ("always", 3), ("disabled", 4)];
const H_ALIGN: &[(&str, i64)] = &[("left", 0), ("center", 1), ("right", 2), ("fill", 3)];
const STRETCH: &[(&str, i64)] = &[("scale", 0), ("tile", 1), ("keep", 2), ("keep_centered", 3), ("keep_aspect", 4), ("keep_aspect_centered", 5), ("keep_aspect_covered", 6)];

fn enum_to_int(table: &[(&str, i64)], v: &Variant) -> Option<i64> {
    let s = v.as_str()?;
    table.iter().find(|(n, _)| *n == s).map(|(_, i)| *i)
}

fn int_to_enum(table: &[(&'static str, i64)], v: &Value) -> Option<Variant> {
    let i = v.as_i64()?;
    table.iter().find(|(_, n)| *n == i).map(|(s, _)| Variant::str(s))
}

// ------------------------------------------------------------ values

fn f32v(x: f64) -> Value {
    // Godot stores these as 32-bit reals; write the f32 form.
    Value::Float((x as f32) as f64)
}

fn to_value(v: &Variant) -> Value {
    match v {
        Variant::Bool(b) => Value::Bool(*b),
        Variant::Int(i) => Value::Int(*i),
        Variant::Float(f) => f32v(*f),
        Variant::Str(s) => Value::String(s.to_string()),
        Variant::Vec2([x, y]) => Value::vector2(*x as f64, *y as f64),
        Variant::Vec3([x, y, z]) => Value::vector3(*x as f64, *y as f64, *z as f64),
        Variant::Color(c) => Value::color(c.map(|x| x as f64)),
    }
}

fn from_value(ty: PropType, v: &Value) -> Option<Variant> {
    let nums = |names: &[&str]| names.iter().find_map(|n| v.numbers(n));
    match ty {
        PropType::Bool => v.as_bool().map(Variant::Bool),
        PropType::Int => v.as_i64().map(Variant::Int),
        PropType::Float => v.as_f64().map(Variant::Float),
        PropType::String => v.as_str().map(|s| Variant::Str(s.to_string().into())),
        PropType::Resource => None, // handled with the resource tables
        PropType::Vec2 => nums(&["Vector2", "Vector2i"]).filter(|n| n.len() == 2).map(|n| Variant::Vec2([n[0] as f32, n[1] as f32])),
        PropType::Vec3 => nums(&["Vector3", "Vector3i"]).filter(|n| n.len() == 3).map(|n| Variant::Vec3([n[0] as f32, n[1] as f32, n[2] as f32])),
        PropType::Color => nums(&["Color"]).filter(|n| n.len() == 4).map(|n| Variant::Color([n[0] as f32, n[1] as f32, n[2] as f32, n[3] as f32])),
    }
}

// ------------------------------------------------------------ 3D transforms

type Mat3 = [[f64; 3]; 3];

fn mat_mul(a: Mat3, b: Mat3) -> Mat3 {
    let mut m = [[0.0; 3]; 3];
    for (i, row) in m.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    m
}

/// `Basis::from_euler(rot, EULER_ORDER_YXZ).scaled_local(scale)`, Node3D's default.
fn basis_from(rot_deg: [f32; 3], scale: [f32; 3]) -> Mat3 {
    let [x, y, z] = rot_deg.map(|d| (d as f64).to_radians());
    let (sx, cx) = x.sin_cos();
    let (sy, cy) = y.sin_cos();
    let (sz, cz) = z.sin_cos();
    let ry = [[cy, 0.0, sy], [0.0, 1.0, 0.0], [-sy, 0.0, cy]];
    let rx = [[1.0, 0.0, 0.0], [0.0, cx, -sx], [0.0, sx, cx]];
    let rz = [[cz, -sz, 0.0], [sz, cz, 0.0], [0.0, 0.0, 1.0]];
    let r = mat_mul(mat_mul(ry, rx), rz);
    let mut m = r;
    for row in &mut m {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell *= scale[j] as f64;
        }
    }
    m
}

/// Split a basis into YXZ Euler degrees and per-axis scale (`Basis::get_euler`).
fn basis_split(m: Mat3) -> ([f32; 3], [f32; 3]) {
    let scale: [f64; 3] = core::array::from_fn(|j| (m[0][j] * m[0][j] + m[1][j] * m[1][j] + m[2][j] * m[2][j]).sqrt());
    let mut r = m;
    for row in &mut r {
        for (j, cell) in row.iter_mut().enumerate() {
            if scale[j] != 0.0 {
                *cell /= scale[j];
            }
        }
    }
    let eps = 1e-6;
    let m12 = r[1][2];
    let (ex, ey, ez) = if m12 < 1.0 - eps {
        if m12 > -(1.0 - eps) {
            if r[1][0] == 0.0 && r[0][1] == 0.0 && r[0][2] == 0.0 && r[2][0] == 0.0 && r[0][0] == 1.0 {
                ((-m12).atan2(r[1][1]), 0.0, 0.0)
            } else {
                ((-m12).asin(), r[0][2].atan2(r[2][2]), r[1][0].atan2(r[1][1]))
            }
        } else {
            (core::f64::consts::FRAC_PI_2, r[0][1].atan2(r[0][0]), 0.0)
        }
    } else {
        (-core::f64::consts::FRAC_PI_2, -r[0][1].atan2(r[0][0]), 0.0)
    };
    let deg = |v: f64| {
        let d = v.to_degrees();
        // Clean up float noise so 90 stays 90.
        let r = (d * 1e4).round() / 1e4;
        (if r == 0.0 { 0.0 } else { r }) as f32
    };
    ([deg(ex), deg(ey), deg(ez)], scale.map(|s| ((s * 1e5).round() / 1e5) as f32))
}

fn transform3d(pos: [f32; 3], rot: [f32; 3], scale: [f32; 3]) -> Value {
    let b = basis_from(rot, scale);
    let mut args: Vec<Value> = b.iter().flat_map(|row| row.iter().map(|&x| f32v(if x.abs() < 1e-7 { 0.0 } else { x }))).collect();
    args.extend(pos.map(|x| f32v(x as f64)));
    Value::Call("Transform3D".into(), args)
}

// ------------------------------------------------------------ resources

/// What the saver needs to turn Neko values into resource references.
struct Resources<'a> {
    sections: Vec<Section>,
    used: HashSet<String>,
    ids: &'a mut dyn IdSource,
}

impl Resources<'_> {
    fn ext_by_id(&self, id: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.tag == "ext_resource" && s.field_str("id") == Some(id))
    }

    fn sub_by_id(&self, id: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.tag == "sub_resource" && s.field_str("id") == Some(id))
    }

    fn sub_by_id_mut(&mut self, id: &str) -> Option<&mut Section> {
        self.sections.iter_mut().find(|s| s.tag == "sub_resource" && s.field_str("id") == Some(id))
    }

    /// `ExtResource("id")` for a `res://` path, reusing an existing entry.
    fn ext_for_path(&mut self, path: &str) -> Value {
        if let Some(id) = self.sections.iter().find(|s| s.tag == "ext_resource" && s.field_str("path") == Some(path)).and_then(|s| s.field_str("id")).map(str::to_string) {
            self.used.insert(id.clone());
            return Value::call("ExtResource", [Value::String(id)]);
        }
        let n = self.sections.iter().filter(|s| s.tag == "ext_resource").count() + 1;
        let id = format!("{n}_{}", uid::scene_unique_id_from(self.ids));
        let ty = match path.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str() {
            "png" | "svg" | "jpg" | "jpeg" | "webp" | "bmp" | "tga" | "ktx" | "dds" => "Texture2D",
            "ogg" | "wav" | "mp3" => "AudioStream",
            "tscn" | "scn" => "PackedScene",
            "gd" => "Script",
            "ttf" | "otf" | "woff" | "woff2" => "FontFile",
            "glb" | "gltf" | "obj" => "PackedScene",
            _ => "Resource",
        };
        let sec = Section::new("ext_resource").with("type", Value::String(ty.into())).with("path", Value::String(path.into())).with("id", Value::String(id.clone()));
        // Ext resources come first, in id order.
        let at = self.sections.iter().rposition(|s| s.tag == "ext_resource").map_or(0, |i| i + 1);
        self.sections.insert(at, sec);
        self.used.insert(id.clone());
        Value::call("ExtResource", [Value::String(id)])
    }

    /// Reuse sub-resource `prev` if it has type `ty`, else create one; set its props.
    fn sub(&mut self, prev: Option<&str>, ty: &str, props: Vec<(String, Value)>) -> Value {
        let id = match prev.filter(|id| self.sub_by_id(id).is_some_and(|s| s.field_str("type") == Some(ty))) {
            Some(id) => id.to_string(),
            None => {
                let id = format!("{ty}_{}", uid::scene_unique_id_from(self.ids));
                self.sections.push(Section::new("sub_resource").with("type", Value::String(ty.into())).with("id", Value::String(id.clone())));
                id
            }
        };
        let sec = self.sub_by_id_mut(&id).expect("exists");
        for (k, v) in props {
            match sec.props.iter_mut().find(|(n, _)| *n == k) {
                Some((_, old)) => {
                    // Keep the original text when the value is numerically the same.
                    if old.as_f64().zip(v.as_f64()).is_none_or(|(a, b)| a != b) && *old != v && !same_numbers(old, &v) {
                        *old = v;
                    }
                }
                None => sec.props.push((k, v)),
            }
        }
        self.used.insert(id.clone());
        Value::call("SubResource", [Value::String(id)])
    }

    /// Mark everything a value references (and what those reference) as used.
    fn mark(&mut self, v: &Value) {
        match v {
            Value::Call(_, args) => {
                if let Some((_, id)) = v.resource_ref()
                    && self.used.insert(id.to_string())
                    && let Some(sub) = self.sub_by_id(id)
                {
                    for (_, p) in sub.props.clone() {
                        self.mark(&p);
                    }
                }
                for a in args {
                    self.mark(a);
                }
            }
            Value::Array(items) => items.iter().for_each(|i| self.mark(i)),
            Value::Dictionary(pairs) => pairs.iter().for_each(|(k, v)| {
                self.mark(k);
                self.mark(v);
            }),
            _ => {}
        }
    }
}

fn same_numbers(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Call(n1, a1), Value::Call(n2, a2)) => n1 == n2 && a1.len() == a2.len() && a1.iter().zip(a2).all(|(x, y)| x.as_f64().zip(y.as_f64()).is_some_and(|(x, y)| (x - y).abs() <= 1e-6 * x.abs().max(1.0))),
        _ => a.as_f64().zip(b.as_f64()).is_some_and(|(x, y)| (x - y).abs() <= 1e-6 * x.abs().max(1.0)),
    }
}

// ------------------------------------------------------------ property mapping

/// Godot property name for a Neko property, when it differs by name only.
fn renamed(class: &str, prop: &str) -> Option<&'static str> {
    let db = ClassDb::get();
    Some(match prop {
        "placeholder" if class == "LineEdit" => "placeholder_text",
        "shadows" if db.inherits(class, "Light3D") || class == "PointLight2D" => "shadow_enabled",
        "range" if class == "OmniLight3D" => "omni_range",
        "range" if class == "SpotLight3D" => "spot_range",
        "angle" if class == "SpotLight3D" => "spot_angle",
        "position_smoothing" if class == "Camera2D" => "position_smoothing_enabled",
        "separation" => "theme_override_constants/separation",
        "gravity_override" if class == "Area2D" => "gravity_space_override",
        _ => return None,
    })
}

/// Neko properties a group of Godot properties decodes to, for one node.
/// Returns the Neko values, or `None` if the group does not apply.
fn decode_group(class: &str, godot: &HashMap<String, Value>, res: &Resources) -> Vec<(&'static str, Variant)> {
    let db = ClassDb::get();
    let mut out = Vec::new();
    let get = |k: &str| godot.get(k);
    if db.inherits(class, "Node3D")
        && let Some(t) = get("transform").and_then(|v| v.numbers("Transform3D")).filter(|n| n.len() == 12)
    {
            let m = [[t[0], t[1], t[2]], [t[3], t[4], t[5]], [t[6], t[7], t[8]]];
            let (rot, scale) = basis_split(m);
            out.push(("position", Variant::Vec3([t[9] as f32, t[10] as f32, t[11] as f32])));
            out.push(("rotation_degrees", Variant::Vec3(rot)));
            out.push(("scale", Variant::Vec3(scale)));
    }
    if db.inherits(class, "Node2D") && let Some(r) = get("rotation").and_then(Value::as_f64) {
        let d = (r.to_degrees() * 1e4).round() / 1e4;
        out.push(("rotation_degrees", Variant::Float(d)));
    }
    if db.inherits(class, "Control") {
        let off = |k: &str| get(k).and_then(Value::as_f64).unwrap_or(0.0) as f32;
        let any = ["offset_left", "offset_top", "offset_right", "offset_bottom"].iter().any(|k| godot.contains_key(*k));
        if any {
            let (l, t, r, b) = (off("offset_left"), off("offset_top"), off("offset_right"), off("offset_bottom"));
            out.push(("position", Variant::Vec2([l, t])));
            out.push(("size", Variant::Vec2([r - l, b - t])));
        }
        match get("anchors_preset").and_then(Value::as_i64) {
            Some(15) => out.push(("anchors", Variant::str("full_rect"))),
            Some(8) => out.push(("anchors", Variant::str("center"))),
            _ => {}
        }
    }
    // Collision shapes: a Shape2D/Shape3D sub-resource.
    if (class == "CollisionShape2D" || class == "CollisionShape3D")
        && let Some(sub) = get("shape").and_then(|v| v.resource_ref()).and_then(|(_, id)| res.sub_by_id(id))
    {
        let ty = sub.field_str("type").unwrap_or("");
        let num = |k: &str, d: f64| sub.prop(k).and_then(Value::as_f64).unwrap_or(d) as f32;
        let v2 = |k: &str, d: [f32; 2]| sub.prop(k).and_then(|v| from_value(PropType::Vec2, v)).and_then(|v| v.as_vec2()).unwrap_or(d);
        let v3 = |k: &str, d: [f32; 3]| match sub.prop(k).and_then(|v| from_value(PropType::Vec3, v)) {
            Some(Variant::Vec3(v)) => v,
            _ => d,
        };
        match ty {
            "RectangleShape2D" => out.extend([("shape", Variant::str("rectangle")), ("size", Variant::Vec2(v2("size", [20.0, 20.0])))]),
            "CircleShape2D" => {
                let r = num("radius", 10.0);
                out.extend([("shape", Variant::str("circle")), ("size", Variant::Vec2([2.0 * r, 2.0 * r]))]);
            }
            "CapsuleShape2D" => {
                let r = num("radius", 10.0);
                out.extend([("shape", Variant::str("capsule")), ("size", Variant::Vec2([2.0 * r, num("height", 30.0)]))]);
            }
            "BoxShape3D" => out.extend([("shape", Variant::str("box")), ("size", Variant::Vec3(v3("size", [1.0; 3])))]),
            "SphereShape3D" => {
                let d = 2.0 * num("radius", 0.5);
                out.extend([("shape", Variant::str("sphere")), ("size", Variant::Vec3([d; 3]))]);
            }
            "CapsuleShape3D" => {
                let d = 2.0 * num("radius", 0.5);
                out.extend([("shape", Variant::str("capsule")), ("size", Variant::Vec3([d, num("height", 2.0), d]))]);
            }
            "ConvexPolygonShape3D" => out.push(("shape", Variant::str("convex"))),
            _ => {}
        }
    }
    // Friction and bounce live in a PhysicsMaterial.
    if (db.inherits(class, "PhysicsBody2D") || db.inherits(class, "PhysicsBody3D"))
        && let Some(sub) = get("physics_material_override").and_then(|v| v.resource_ref()).and_then(|(_, id)| res.sub_by_id(id))
    {
        out.push(("friction", Variant::Float(sub.prop("friction").and_then(Value::as_f64).unwrap_or(1.0))));
        out.push(("bounce", Variant::Float(sub.prop("bounce").and_then(Value::as_f64).unwrap_or(0.0))));
    }
    if class == "MarginContainer" {
        let m: Vec<i64> = ["left", "top", "right", "bottom"].iter().filter_map(|s| get(&format!("theme_override_constants/margin_{s}")).and_then(Value::as_i64)).collect();
        if m.len() == 4 && m.iter().all(|&x| x == m[0]) {
            out.push(("margin", Variant::Int(m[0])));
        }
    }
    out
}

/// The Godot properties a group decodes from (so they are not kept as extras).
fn group_keys(class: &str) -> Vec<&'static str> {
    let db = ClassDb::get();
    let mut k = Vec::new();
    if db.inherits(class, "Node3D") {
        k.push("transform");
    }
    if db.inherits(class, "Node2D") {
        k.push("rotation");
    }
    if db.inherits(class, "Control") {
        k.extend(["offset_left", "offset_top", "offset_right", "offset_bottom", "anchors_preset", "anchor_left", "anchor_top", "anchor_right", "anchor_bottom", "grow_horizontal", "grow_vertical"]);
    }
    if class == "CollisionShape2D" || class == "CollisionShape3D" {
        k.push("shape");
    }
    if db.inherits(class, "PhysicsBody2D") || db.inherits(class, "PhysicsBody3D") {
        k.push("physics_material_override");
    }
    if class == "MarginContainer" {
        k.extend(["theme_override_constants/margin_left", "theme_override_constants/margin_top", "theme_override_constants/margin_right", "theme_override_constants/margin_bottom"]);
    }
    k
}

/// Neko props handled by groups (not written one by one).
fn grouped_props(class: &str) -> Vec<&'static str> {
    let db = ClassDb::get();
    let mut v = Vec::new();
    if db.inherits(class, "Node3D") {
        v.extend(["position", "rotation_degrees", "scale"]);
    }
    if db.inherits(class, "Node2D") {
        v.push("rotation_degrees");
    }
    if db.inherits(class, "Control") {
        v.extend(["position", "size", "anchors"]);
    }
    if class == "CollisionShape2D" || class == "CollisionShape3D" {
        v.extend(["shape", "size"]);
    }
    if db.inherits(class, "PhysicsBody2D") || db.inherits(class, "PhysicsBody3D") {
        v.extend(["friction", "bounce"]);
    }
    if class == "MarginContainer" {
        v.push("margin");
    }
    v
}

/// Godot values for the grouped Neko properties of `node`.
fn encode_groups(scene: &Scene, id: NodeId, res: &mut Resources) -> Vec<(String, Value)> {
    let n = scene.get(id).expect("live");
    let class = n.class;
    let db = ClassDb::get();
    let mut out: Vec<(String, Value)> = Vec::new();
    let v3 = |p: &str, d: [f32; 3]| match n.get(p) {
        Some(Variant::Vec3(v)) => v,
        _ => d,
    };
    let v2 = |p: &str, d: [f32; 2]| n.get(p).and_then(|v| v.as_vec2()).unwrap_or(d);
    if db.inherits(class, "Node3D") {
        let (pos, rot, scale) = (v3("position", [0.0; 3]), v3("rotation_degrees", [0.0; 3]), v3("scale", [1.0; 3]));
        if pos != [0.0; 3] || rot != [0.0; 3] || scale != [1.0; 3] {
            out.push(("transform".into(), transform3d(pos, rot, scale)));
        }
    }
    if db.inherits(class, "Node2D") {
        let r = n.get("rotation_degrees").and_then(|v| v.as_f64()).unwrap_or(0.0);
        if r != 0.0 {
            out.push(("rotation".into(), f32v(r.to_radians())));
        }
    }
    if db.inherits(class, "Control") {
        let anchors = n.get("anchors").and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
        match anchors.as_str() {
            "full_rect" => {
                out.push(("anchors_preset".into(), Value::Int(15)));
                out.push(("anchor_right".into(), Value::Float(1.0)));
                out.push(("anchor_bottom".into(), Value::Float(1.0)));
                out.push(("grow_horizontal".into(), Value::Int(2)));
                out.push(("grow_vertical".into(), Value::Int(2)));
            }
            "center" => {
                out.push(("anchors_preset".into(), Value::Int(8)));
                for a in ["anchor_left", "anchor_top", "anchor_right", "anchor_bottom"] {
                    out.push((a.into(), Value::Float(0.5)));
                }
                out.push(("grow_horizontal".into(), Value::Int(2)));
                out.push(("grow_vertical".into(), Value::Int(2)));
            }
            _ => {}
        }
        let (p, s) = (v2("position", [0.0; 2]), v2("size", [0.0; 2]));
        for (k, v) in [("offset_left", p[0]), ("offset_top", p[1]), ("offset_right", p[0] + s[0]), ("offset_bottom", p[1] + s[1])] {
            if v != 0.0 {
                out.push((k.into(), f32v(v as f64)));
            }
        }
    }
    if class == "CollisionShape2D" || class == "CollisionShape3D" {
        let shape = n.get("shape").and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
        let prev = original(n, "shape").and_then(|v| v.resource_ref()).map(|(_, id)| id.to_string());
        let (ty, props): (&str, Vec<(String, Value)>) = if class == "CollisionShape2D" {
            let s = v2("size", [32.0, 32.0]);
            match shape.as_str() {
                "circle" => ("CircleShape2D", vec![("radius".into(), f32v(s[0] as f64 / 2.0))]),
                "capsule" => ("CapsuleShape2D", vec![("radius".into(), f32v(s[0] as f64 / 2.0)), ("height".into(), f32v(s[1] as f64))]),
                _ => ("RectangleShape2D", vec![("size".into(), Value::vector2(s[0] as f64, s[1] as f64))]),
            }
        } else {
            let s = v3("size", [1.0; 3]);
            match shape.as_str() {
                "sphere" => ("SphereShape3D", vec![("radius".into(), f32v(s[0] as f64 / 2.0))]),
                "capsule" => ("CapsuleShape3D", vec![("radius".into(), f32v(s[0] as f64 / 2.0)), ("height".into(), f32v(s[1] as f64))]),
                "convex" => ("ConvexPolygonShape3D", vec![]),
                _ => ("BoxShape3D", vec![("size".into(), Value::vector3(s[0] as f64, s[1] as f64, s[2] as f64))]),
            }
        };
        out.push(("shape".into(), res.sub(prev.as_deref(), ty, props)));
    }
    if (db.inherits(class, "PhysicsBody2D") || db.inherits(class, "PhysicsBody3D")) && (n.is_overridden("friction") || n.is_overridden("bounce")) {
        let prev = original(n, "physics_material_override").and_then(|v| v.resource_ref()).map(|(_, id)| id.to_string());
        let f = n.get("friction").and_then(|v| v.as_f64()).unwrap_or(1.0);
        let b = n.get("bounce").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let mut props = Vec::new();
        if f != 1.0 {
            props.push(("friction".into(), f32v(f)));
        }
        if b != 0.0 {
            props.push(("bounce".into(), f32v(b)));
        }
        out.push(("physics_material_override".into(), res.sub(prev.as_deref(), "PhysicsMaterial", props)));
    }
    if class == "MarginContainer" && n.is_overridden("margin") {
        let m = n.get("margin").and_then(|v| v.as_f64()).unwrap_or(0.0) as i64;
        for side in ["left", "top", "right", "bottom"] {
            out.push((format!("theme_override_constants/margin_{side}"), Value::Int(m)));
        }
    }
    out
}

/// The value a Godot property had when the scene was loaded.
fn original<'a>(n: &'a crate::scene::Node, godot_name: &str) -> Option<&'a Value> {
    n.godot.props.iter().find(|(k, _)| k.strip_prefix(ORIG) == Some(godot_name)).map(|(_, v)| v)
}

/// Originals of mapped properties are stored in `GodotNode::props` under this
/// prefix, so an unchanged value is written back with its original text.
const ORIG: &str = "\u{1}orig:";

/// Properties whose Neko default differs from Godot's default.
fn neko_default_differs(class: &str, prop: &str) -> Option<Variant> {
    // Godot's buttons start empty; Neko's Button says "Button".
    if prop == "text" && ClassDb::get().inherits(class, "Button") {
        return Some(Variant::str(""));
    }
    None
}

/// The Neko property a (non-grouped) Godot property maps to.
fn neko_prop_for(class: &str, godot_key: &str) -> Option<&'static crate::classdb::PropDef> {
    ClassDb::get().properties(class).into_iter().map(|(_, d)| d).find(|d| renamed(class, d.name) == Some(godot_key) || (d.name == godot_key && renamed(class, d.name).is_none()))
}

/// Decode one Godot property value into its Neko form.
fn decode_single(class: &str, def: &crate::classdb::PropDef, v: &Value, res: &Resources) -> Option<Variant> {
    match (def.name, class) {
        ("process_mode", _) => int_to_enum(PROCESS_MODE, v),
        ("horizontal_alignment", _) => int_to_enum(H_ALIGN, v),
        ("stretch_mode", "TextureRect") => int_to_enum(STRETCH, v),
        ("cast_shadow", _) => v.as_i64().map(|i| Variant::Bool(i != 0)),
        ("floor_max_angle", _) => v.as_f64().map(|r| Variant::Float((r.to_degrees() * 1e4).round() / 1e4)),
        ("gravity_override", _) => v.as_i64().map(|i| Variant::Bool(i != 0)),
        (_, _) if def.ty == PropType::Resource => match v.resource_ref() {
            Some(("ExtResource", id)) => res.ext_by_id(id).and_then(|e| e.field_str("path")).map(|p| Variant::Str(p.to_string().into())),
            _ => None,
        },
        _ => from_value(def.ty, v),
    }
}

// ------------------------------------------------------------ load

pub fn from_str(text: &str) -> Result<Loaded, FormatError> {
    let trimmed = text.trim_start();
    if !trimmed.starts_with('[') {
        return crate::format::from_legacy_toml(text);
    }
    let doc = Document::parse(text).map_err(FormatError::Syntax)?;
    let mut sections = doc.sections.into_iter();
    let head = sections.next().ok_or(FormatError::Empty)?;
    if head.tag != "gd_scene" {
        return Err(FormatError::Syntax(format!("expected [gd_scene], found [{}] (is this a resource?)", head.tag)));
    }
    let mut warnings = Vec::new();
    let mut resources = Vec::new();
    let mut tail = Vec::new();
    let mut nodes = Vec::new();
    for s in sections {
        match s.tag.as_str() {
            "ext_resource" | "sub_resource" => resources.push(s),
            "node" => nodes.push(s),
            "connection" | "editable" => tail.push(s),
            other => {
                warnings.push(format!("unknown section [{other}] kept as is"));
                tail.push(s);
            }
        }
    }
    // Loading creates no ids; the source is never asked.
    let mut no_ids = uid::SeededIds(0);
    let res = Resources { sections: resources, used: HashSet::default(), ids: &mut no_ids };
    let mut scene: Option<Scene> = None;
    for sec in nodes {
        let name = sec.field_str("name").unwrap_or("Node").to_string();
        let gtype = sec.field_str("type").map(str::to_string);
        let parent = sec.field_str("parent").map(str::to_string);
        let (class, foreign) = match gtype.as_deref() {
            Some(t) => match neko_class(t) {
                Some(c) => (c, None),
                None => {
                    warnings.push(format!("{name}: Neko has no {t} yet; kept as {} and saved back as {t}", stand_in(t)));
                    (stand_in(t), Some(t.to_string()))
                }
            },
            None => ("Node", None),
        };
        let id = match (&mut scene, parent.as_deref()) {
            (None, _) => {
                let s = Scene::new(class, &name).map_err(|e| FormatError::Root(e.to_string()))?;
                let root = s.root();
                scene = Some(s);
                root
            }
            (Some(s), Some(p)) => {
                let Some(pid) = s.find(p) else {
                    warnings.push(format!("'{name}' skipped: parent '{p}' not found"));
                    continue;
                };
                s.add_child(pid, class, Some(&name)).map_err(|e| FormatError::Syntax(e.to_string()))?
            }
            (Some(_), None) => {
                warnings.push(format!("'{name}' skipped: a second root node"));
                continue;
            }
        };
        let s = scene.as_mut().expect("set above");
        let mut godot = GodotNode { class: foreign.clone(), untyped: gtype.is_none(), from_file: true, ..GodotNode::default() };
        for (k, v) in sec.fields.iter().filter(|(k, _)| !matches!(k.as_str(), "name" | "type" | "parent")) {
            if k == "unique_id" {
                godot.unique_id = v.as_i64().unwrap_or(0) as i32;
            }
            godot.header.push((k.clone(), v.clone()));
        }
        // Godot props by name, for groups.
        let gp: HashMap<String, Value> = sec.props.iter().cloned().collect();
        let mut handled: HashSet<String> = HashSet::default();
        let mut set: Vec<(&'static str, Variant)> = Vec::new();
        if foreign.is_none() {
            let grouped = decode_group(class, &gp, &res);
            if !grouped.is_empty() || !group_keys(class).is_empty() {
                for k in group_keys(class) {
                    if gp.contains_key(k) {
                        // Only consume group keys that decoded into something.
                        let consumed = match k {
                            "shape" => grouped.iter().any(|(p, _)| *p == "shape"),
                            "physics_material_override" => grouped.iter().any(|(p, _)| *p == "friction"),
                            k if k.starts_with("theme_override_constants/margin_") => grouped.iter().any(|(p, _)| *p == "margin"),
                            k if k.starts_with("anchor") || k.starts_with("grow_") => grouped.iter().any(|(p, _)| *p == "anchors") || k.starts_with("grow_") && grouped.iter().any(|(p, _)| *p == "position"),
                            _ => true,
                        };
                        if consumed {
                            handled.insert(k.to_string());
                        }
                    }
                }
            }
            set.extend(grouped);
            for (k, v) in &sec.props {
                if handled.contains(k) {
                    continue;
                }
                let Some(def) = neko_prop_for(class, k) else { continue };
                let val = decode_single(class, def, v, &res);
                if let Some(val) = val {
                    set.push((def.name, val));
                    handled.insert(k.clone());
                }
            }
        }
        let node = s.get_mut(id).expect("just added");
        if foreign.is_none()
            && let Some(d) = neko_default_differs(class, "text")
        {
            let _ = node.set("text", d);
        }
        for (p, v) in set {
            if let Err(e) = node.set(p, v) {
                warnings.push(format!("{name}: {e}"));
            }
        }
        // Keep every original property: unmapped ones verbatim, mapped ones
        // under the ORIG prefix (to write back unchanged values byte-for-byte).
        for (k, v) in &sec.props {
            if handled.contains(k) {
                godot.props.push((format!("{ORIG}{k}"), v.clone()));
            } else {
                godot.props.push((k.clone(), v.clone()));
            }
        }
        node.godot = godot;
    }
    let mut scene = scene.ok_or(FormatError::Empty)?;
    scene.godot.uid = head.field_str("uid").map(str::to_string);
    scene.godot.resources = res.sections;
    scene.godot.tail = tail;
    Ok(Loaded { scene, warnings })
}

// ------------------------------------------------------------ save

/// Assign the scene's `uid://` and node `unique_id`s that are still missing
/// (from the OS's entropy), then write the scene as `.tscn` text.
#[cfg(feature = "std")]
pub fn to_string(scene: &mut Scene) -> String {
    to_string_with(scene, &mut uid::OsIds)
}

/// [`to_string`] with new ids taken from `ids` (for targets without an OS).
pub fn to_string_with(scene: &mut Scene, ids: &mut dyn IdSource) -> String {
    if scene.godot.uid.is_none() {
        scene.godot.uid = Some(uid::id_to_text(uid::create_id_from(ids)));
    }
    let node_ids: Vec<NodeId> = scene.walk().into_iter().map(|(id, _)| id).collect();
    let mut taken: HashSet<i32> = node_ids.iter().filter_map(|&id| scene.get(id)).map(|n| n.godot.unique_id).filter(|&u| u != 0).collect();
    for &id in &node_ids {
        let n = scene.get_mut(id).expect("live");
        if n.godot.unique_id == 0 {
            let mut u = uid::create_node_id_from(ids);
            while taken.contains(&u) {
                u = uid::create_node_id_from(ids);
            }
            n.godot.unique_id = u;
            taken.insert(u);
        }
    }
    write(scene, ids)
}

fn write(scene: &Scene, ids: &mut dyn IdSource) -> String {
    let db = ClassDb::get();
    let mut res = Resources { sections: scene.godot.resources.clone(), used: HashSet::default(), ids };
    let mut node_sections = Vec::new();
    for (id, _) in scene.walk() {
        let n = scene.get(id).expect("live");
        let mut sec = Section::new("node").with("name", Value::String(n.name.clone()));
        if !n.godot.untyped {
            let ty = n.godot.class.clone().unwrap_or_else(|| godot_class(n.class).to_string());
            sec.fields.push(("type".into(), Value::String(ty)));
        }
        if let Some(p) = n.parent() {
            sec.fields.push(("parent".into(), Value::String(scene.path(p))));
        }
        // Header fields in Godot's order, with unique_id after index/owner.
        let early = ["parent_id_path", "owner", "owner_uid_path", "index"];
        for (k, v) in n.godot.header.iter().filter(|(k, _)| early.contains(&k.as_str())) {
            sec.fields.push((k.clone(), v.clone()));
        }
        sec.fields.push(("unique_id".into(), Value::Int(n.godot.unique_id as i64)));
        for (k, v) in n.godot.header.iter().filter(|(k, _)| !early.contains(&k.as_str()) && k != "unique_id") {
            res.mark(v);
            sec.fields.push((k.clone(), v.clone()));
        }

        // Godot values for Neko properties.
        let mut mapped: Vec<(String, Value)> = Vec::new();
        if n.godot.class.is_none() {
            // Grouped properties (transforms, Control offsets, shapes...): if
            // the Neko values still equal what the file had, write the file's
            // values back untouched; re-encoding would change their text.
            let orig: HashMap<String, Value> = n.godot.props.iter().filter_map(|(k, v)| k.strip_prefix(ORIG).map(|k| (k.to_string(), v.clone()))).collect();
            let unchanged = n.godot.from_file && {
                let dec = decode_group(n.class, &orig, &res);
                grouped_props(n.class).iter().all(|p| match dec.iter().find(|(q, _)| q == p) {
                    Some((_, v)) => n.get(p).as_ref() == Some(v),
                    None => !n.is_overridden(p) || neko_default_differs(n.class, p).is_some(),
                })
            };
            if unchanged {
                for k in group_keys(n.class) {
                    if let Some(v) = orig.get(k) {
                        mapped.push((k.to_string(), v.clone()));
                    }
                }
            } else {
                mapped.extend(encode_groups(scene, id, &mut res));
            }
            let grouped = grouped_props(n.class);
            for (_, def) in db.properties(n.class) {
                if grouped.contains(&def.name) {
                    continue;
                }
                // Write values that differ from *Godot's* default.
                let value_now = n.get(def.name);
                match neko_default_differs(n.class, def.name) {
                    Some(godot_default) if value_now.as_ref() == Some(&godot_default) => continue,
                    Some(_) => {}
                    None if !n.is_overridden(def.name) => continue,
                    None => {}
                }
                let v = n.get(def.name).expect("class property");
                let gname = renamed(n.class, def.name).unwrap_or(def.name).to_string();
                let gv = match def.name {
                    "process_mode" => enum_to_int(PROCESS_MODE, &v).map(Value::Int),
                    "horizontal_alignment" => enum_to_int(H_ALIGN, &v).map(Value::Int),
                    "stretch_mode" if n.class == "TextureRect" => enum_to_int(STRETCH, &v).map(Value::Int),
                    "cast_shadow" => v.as_bool().map(|b| Value::Int(b as i64)),
                    "gravity_override" => v.as_bool().map(|b| Value::Int(if b { 3 } else { 0 })),
                    "floor_max_angle" => v.as_f64().map(|d| f32v(d.to_radians())),
                    "separation" | "columns" | "layer" | "z_index" | "hframes" | "vframes" | "frame" | "amount" | "collision_layer" | "collision_mask" => v.as_f64().map(|x| Value::Int(x as i64)),
                    _ if def.ty == PropType::Resource => match v.as_str() {
                        Some(p) if !p.is_empty() => Some(res.ext_for_path(p)),
                        _ => None,
                    },
                    _ => Some(to_value(&v)),
                };
                if let Some(gv) = gv {
                    mapped.push((gname, gv));
                }
            }
        }
        // New properties in Godot's property-list order: Node, CanvasItem,
        // then the transform/layout of Node2D, Node3D or Control, then the rest.
        let rank = |k: &str| -> usize {
            const ORDER: [&str; 21] = [
                "process_mode", "visible", "modulate", "self_modulate", "z_index", "transform", "layout_mode", "anchors_preset", "anchor_left", "anchor_top", "anchor_right",
                "anchor_bottom", "offset_left", "offset_top", "offset_right", "offset_bottom", "grow_horizontal", "grow_vertical", "position", "rotation", "scale",
            ];
            let is_3d = db.inherits(n.class, "Node3D");
            match ORDER.iter().position(|o| *o == k) {
                // Node3D owns `visible` after its transform.
                Some(1) if is_3d => ORDER.len(),
                Some(i) => i,
                None => ORDER.len() + 1,
            }
        };
        mapped.sort_by_key(|(k, _)| rank(k));
        // Write in the original order where possible, using original text for
        // values that did not change; then new properties; extras verbatim.
        let mut props: Vec<(String, Value)> = Vec::new();
        let mapped_names: HashSet<&str> = mapped.iter().map(|(k, _)| k.as_str()).collect();
        for (k, v) in &n.godot.props {
            if let Some(orig_name) = k.strip_prefix(ORIG) {
                if let Some((_, new)) = mapped.iter().find(|(m, _)| m == orig_name) {
                    let keep = new == v || same_numbers(new, v) || (new.resource_ref().is_some() && new.resource_ref().map(|r| r.0) == v.resource_ref().map(|r| r.0) && new.resource_ref() == v.resource_ref());
                    props.push((orig_name.to_string(), if keep { v.clone() } else { new.clone() }));
                } else if n.godot.class.is_none()
                    && let Some(def) = neko_prop_for(n.class, orig_name)
                    && !grouped_props(n.class).contains(&def.name)
                    && !n.is_overridden(def.name)
                    && def.default.type_matches(def.ty)
                    && decode_single(n.class, def, v, &res).as_ref() == n.get(def.name).as_ref()
                {
                    // The file spelled out a value equal to Neko's default
                    // (e.g. `value = 0.0`); keep writing it.
                    props.push((orig_name.to_string(), v.clone()));
                }
            } else if !mapped_names.contains(k.as_str()) {
                props.push((k.clone(), v.clone()));
            }
        }
        // New properties go before `script`: Godot writes the class's own
        // properties, then `script`, then the script's exported variables.
        // hashbrown sets keep their borrows until dropped.
        drop(mapped_names);
        for (k, v) in mapped {
            if !props.iter().any(|(p, _)| *p == k) {
                match props.iter().position(|(p, _)| p == "script") {
                    Some(i) => props.insert(i, (k, v)),
                    None => props.push((k, v)),
                }
            }
        }
        for (_, v) in &props {
            res.mark(v);
        }
        sec.props = props;
        node_sections.push(sec);
    }
    for s in &scene.godot.tail {
        for (_, v) in &s.fields {
            res.mark(v);
        }
    }

    let mut head = Section::new("gd_scene").with("format", Value::Int(FORMAT));
    if let Some(u) = &scene.godot.uid {
        head.fields.push(("uid".into(), Value::String(u.clone())));
    }
    let mut doc = Document { sections: vec![head] };
    let used = res.used.clone();
    doc.sections.extend(res.sections.into_iter().filter(|s| s.field_str("id").is_some_and(|id| used.contains(id))));
    doc.sections.extend(node_sections);
    doc.sections.extend(scene.godot.tail.iter().cloned());
    doc.write()
}

// ------------------------------------------------------------ node scripts

/// The `res://` path of a node's script (`script = ExtResource(...)`), if any.
pub fn script_path(scene: &Scene, id: NodeId) -> Option<String> {
    let n = scene.get(id)?;
    let (_, v) = n.godot.props.iter().find(|(k, _)| k == "script")?;
    let (_, rid) = v.resource_ref()?;
    scene.godot.resources.iter().find(|s| s.tag == "ext_resource" && s.field_str("id") == Some(rid)).and_then(|s| s.field_str("path")).map(str::to_string)
}

/// Attach the script at `path` (`res://...`, with its `uid://` if known) to a
/// node, as Godot's `Node::set_script` saves it: an `[ext_resource
/// type="Script"]` and a `script` property after the node's own properties.
pub fn attach_script(scene: &mut Scene, id: NodeId, path: &str, script_uid: Option<&str>, ids: &mut dyn IdSource) {
    let existing = scene.godot.resources.iter().find(|s| s.tag == "ext_resource" && s.field_str("path") == Some(path)).and_then(|s| s.field_str("id")).map(str::to_string);
    let rid = match existing {
        Some(rid) => rid,
        None => {
            let n = scene.godot.resources.iter().filter(|s| s.tag == "ext_resource").count() + 1;
            let rid = format!("{n}_{}", uid::scene_unique_id_from(ids));
            let mut sec = Section::new("ext_resource").with("type", Value::String("Script".into()));
            if let Some(u) = script_uid {
                sec = sec.with("uid", Value::String(u.into()));
            }
            sec = sec.with("path", Value::String(path.into())).with("id", Value::String(rid.clone()));
            let at = scene.godot.resources.iter().rposition(|s| s.tag == "ext_resource").map_or(0, |i| i + 1);
            scene.godot.resources.insert(at, sec);
            rid
        }
    };
    let Some(node) = scene.get_mut(id) else { return };
    let value = Value::call("ExtResource", [Value::String(rid)]);
    match node.godot.props.iter_mut().find(|(k, _)| k == "script") {
        Some((_, v)) => *v = value,
        None => node.godot.props.push(("script".into(), value)),
    }
}

/// Godot's "Detach Script": drop `script` and the script's exported values
/// saved after it. The unused `[ext_resource]` is dropped on save.
pub fn detach_script(scene: &mut Scene, id: NodeId) {
    if let Some(node) = scene.get_mut(id)
        && let Some(i) = node.godot.props.iter().position(|(k, _)| k == "script")
    {
        node.godot.props.truncate(i);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GODOT_SCENE: &str = r#"[gd_scene format=3 uid="uid://cd3k1x0c5oqfb"]

[ext_resource type="Texture2D" uid="uid://b5m4hq1x3d7e2" path="res://icon.svg" id="1_abcde"]
[ext_resource type="Script" path="res://player.gd" id="2_fghij"]

[sub_resource type="RectangleShape2D" id="RectangleShape2D_klmno"]
size = Vector2(48, 56)

[node name="Main" type="Node2D" unique_id=1820301468]

[node name="Player" type="CharacterBody2D" parent="." unique_id=95512398 groups=["player"]]
position = Vector2(300, 420)
rotation = 0.785398
script = ExtResource("2_fghij")

[node name="Sprite2D" type="Sprite2D" parent="Player" unique_id=410395720]
texture = ExtResource("1_abcde")

[node name="CollisionShape2D" type="CollisionShape2D" parent="Player" unique_id=2048337421]
shape = SubResource("RectangleShape2D_klmno")

[node name="Tree" type="AnimationTree" parent="." unique_id=77]
active = false

[node name="Label" type="Label" parent="." unique_id=11]
offset_right = 40.0
offset_bottom = 23.0
text = "Score"
horizontal_alignment = 1

[node name="Cube" type="MeshInstance3D" parent="." unique_id=12]
transform = Transform3D(1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 2, 0)

[connection signal="ready" from="." to="Player" method="_on_ready"]
"#;

    #[test]
    fn godot_scene_loads_into_neko_properties() {
        let l = from_str(GODOT_SCENE).unwrap();
        let s = &l.scene;
        let p = s.get(s.find("Player").unwrap()).unwrap();
        assert_eq!(p.get("position"), Some(Variant::Vec2([300.0, 420.0])));
        assert_eq!(p.get("rotation_degrees"), Some(Variant::Float(45.0)));
        let spr = s.get(s.find("Player/Sprite2D").unwrap()).unwrap();
        assert_eq!(spr.get("texture"), Some(Variant::str("res://icon.svg")));
        let col = s.get(s.find("Player/CollisionShape2D").unwrap()).unwrap();
        assert_eq!(col.get("size"), Some(Variant::Vec2([48.0, 56.0])));
        let label = s.get(s.find("Label").unwrap()).unwrap();
        assert_eq!(label.get("size"), Some(Variant::Vec2([40.0, 23.0])));
        assert_eq!(label.get("horizontal_alignment"), Some(Variant::str("center")));
        let cube = s.get(s.find("Cube").unwrap()).unwrap();
        assert_eq!(cube.get("position"), Some(Variant::Vec3([0.0, 2.0, 0.0])));
        let tree = s.get(s.find("Tree").unwrap()).unwrap();
        assert_eq!(tree.godot.class.as_deref(), Some("AnimationTree"));
        assert!(l.warnings.iter().any(|w| w.contains("AnimationTree")), "{:?}", l.warnings);
    }

    #[test]
    fn unchanged_godot_scene_saves_byte_for_byte() {
        let mut l = from_str(GODOT_SCENE).unwrap();
        assert_eq!(to_string(&mut l.scene), GODOT_SCENE);
    }

    #[test]
    fn edits_are_written_in_godot_form() {
        let mut l = from_str(GODOT_SCENE).unwrap();
        let s = &mut l.scene;
        let p = s.find("Player").unwrap();
        s.get_mut(p).unwrap().set("rotation_degrees", Variant::Float(90.0)).unwrap();
        let col = s.find("Player/CollisionShape2D").unwrap();
        s.get_mut(col).unwrap().set("size", Variant::Vec2([10.0, 20.0])).unwrap();
        let spr = s.find("Player/Sprite2D").unwrap();
        s.get_mut(spr).unwrap().set("texture", Variant::str("res://cat.png")).unwrap();
        let text = to_string(s);
        assert!(text.contains("rotation = 1.5707964\n"), "{text}");
        assert!(text.contains("size = Vector2(10, 20)"), "{text}");
        assert!(text.contains("path=\"res://cat.png\""), "{text}");
        // The icon is no longer used, so its ext_resource is dropped.
        assert!(!text.contains("res://icon.svg"), "{text}");
        // And it all loads back.
        let again = from_str(&text).unwrap();
        let p = again.scene.get(again.scene.find("Player").unwrap()).unwrap();
        assert_eq!(p.get("rotation_degrees"), Some(Variant::Float(90.0)));
    }

    #[test]
    fn new_neko_scene_writes_valid_tscn() {
        let mut s = Scene::new("Node3D", "World").unwrap();
        let root = s.root();
        let cam = s.add_child(root, "Camera3D", None).unwrap();
        s.get_mut(cam).unwrap().set("rotation_degrees", Variant::Vec3([-30.0, 45.0, 0.0])).unwrap();
        s.get_mut(cam).unwrap().set("position", Variant::Vec3([1.0, 2.0, 3.0])).unwrap();
        let ui = s.add_child(root, "Button", None).unwrap();
        s.get_mut(ui).unwrap().set("position", Variant::Vec2([10.0, 20.0])).unwrap();
        let text = to_string(&mut s);
        assert!(text.starts_with("[gd_scene format=3 uid=\"uid://"), "{text}");
        assert!(text.contains("[node name=\"Camera3D\" type=\"Camera3D\" parent=\".\" unique_id="), "{text}");
        assert!(text.contains("text = \"Button\""), "{text}");
        let l = from_str(&text).unwrap();
        let c = l.scene.get(l.scene.find("Camera3D").unwrap()).unwrap();
        assert_eq!(c.get("rotation_degrees"), Some(Variant::Vec3([-30.0, 45.0, 0.0])));
        assert_eq!(c.get("position"), Some(Variant::Vec3([1.0, 2.0, 3.0])));
        let b = l.scene.get(l.scene.find("Button").unwrap()).unwrap();
        assert_eq!(b.get("position"), Some(Variant::Vec2([10.0, 20.0])));
        // Saving again changes nothing (ids are kept).
        let mut l = l;
        assert_eq!(to_string(&mut l.scene), text);
    }

    #[test]
    fn euler_round_trip() {
        for rot in [[0.0, 90.0, 0.0], [-30.0, 45.0, 10.0], [89.0, -120.0, 33.0]] {
            let (r, s) = basis_split(basis_from(rot, [2.0, 1.0, 0.5]));
            for i in 0..3 {
                assert!((r[i] - rot[i]).abs() < 1e-3, "{rot:?} -> {r:?}");
            }
            assert_eq!(s, [2.0, 1.0, 0.5]);
        }
    }

    #[test]
    fn attach_and_detach_script() {
        let mut scene = from_str("[gd_scene format=3]\n\n[node name=\"Main\" type=\"Node2D\"]\n").unwrap().scene;
        let root = scene.root();
        let mut ids = uid::SeededIds(7);
        attach_script(&mut scene, root, "res://main.gd", Some("uid://abc"), &mut ids);
        assert_eq!(script_path(&scene, root).as_deref(), Some("res://main.gd"));
        let text = to_string_with(&mut scene, &mut ids);
        assert!(text.contains("[ext_resource type=\"Script\" uid=\"uid://abc\" path=\"res://main.gd\" id=\"1_"), "{text}");
        assert!(text.contains("script = ExtResource(\"1_"), "{text}");
        // Loads back.
        let mut back = from_str(&text).unwrap().scene;
        let r = back.root();
        assert_eq!(script_path(&back, r).as_deref(), Some("res://main.gd"));
        detach_script(&mut back, r);
        let text = to_string_with(&mut back, &mut ids);
        assert!(!text.contains("ext_resource") && !text.contains("script ="), "{text}");
    }

}
