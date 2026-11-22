//! Runs a scene in the game window: Godot's `SceneTree` + canvas drawing,
//! reduced to what Neko's nodes can do today (no scripts or physics yet).
//!
//! What it mirrors from Godot:
//! - `rendering/environment/defaults/default_clear_color` (0.3, 0.3, 0.3);
//! - Node2D transforms compose parent * translate * rotate * scale;
//! - the first enabled `Camera2D` centers the view on itself, `zoom` 2 = 2x;
//! - `visible = false` hides the whole branch; `modulate` multiplies down
//!   the tree; draw order is tree order, stably sorted by `z_index`;
//! - children of a `CanvasLayer` ignore the camera (screen space);
//! - controls use Godot's *default* (game) theme, not the editor theme:
//!   font size 16, `style_normal_color` (0.1, 0.1, 0.1, 0.6), radius 3;
//! - collision shapes are invisible unless "Visible Collision Shapes" is on,
//!   then drawn in `debug/shapes/collision/shape_color` (0, 0.6, 0.7, 0.42).

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::script_host::{Scripts, World};

use neko_core::godot::Value as GValue;
use neko_core::{ClassDb, NodeId, Scene, Variant};
use neko_ui::{Align2, Color32, ColorImage, CornerRadius, FontId, Mesh, Painter, Pos2, Rect, Shape, Stroke, TextureId, Ui, pos2, vec2};

/// Godot's `default_clear_color`.
pub const CLEAR_COLOR: [f32; 4] = [0.3, 0.3, 0.3, 1.0];
/// Godot's `display/window/size/viewport_width/height`.
pub const VIEWPORT_SIZE: (u32, u32) = (1152, 648);

const FONT_SIZE: f32 = 16.0;
const FONT_COLOR: Color32 = Color32::from_rgb(223, 223, 223); // control_font_color 0.875
const STYLE_NORMAL: Color32 = Color32::from_rgba_premultiplied(15, 15, 15, 153); // (0.1, 0.1, 0.1, 0.6)
const STYLE_DISABLED: Color32 = Color32::from_rgba_premultiplied(8, 8, 8, 77); // (0.1, 0.1, 0.1, 0.3)
const STYLE_PROGRESS: Color32 = Color32::from_rgba_premultiplied(102, 102, 102, 102); // (1, 1, 1, 0.4)
const RADIUS: u8 = 3;

/// A 2D affine transform: x' = a*x + c*y + tx, y' = b*x + d*y + ty (Godot's Transform2D).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Xform {
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    tx: f32,
    ty: f32,
}

impl Xform {
    const IDENTITY: Self = Self { a: 1.0, b: 0.0, c: 0.0, d: 1.0, tx: 0.0, ty: 0.0 };

    fn new(pos: [f32; 2], rot_deg: f32, scale: [f32; 2]) -> Self {
        let (s, c) = rot_deg.to_radians().sin_cos();
        Self { a: c * scale[0], b: s * scale[0], c: -s * scale[1], d: c * scale[1], tx: pos[0], ty: pos[1] }
    }

    fn mul(self, o: Self) -> Self {
        Self {
            a: self.a * o.a + self.c * o.b,
            b: self.b * o.a + self.d * o.b,
            c: self.a * o.c + self.c * o.d,
            d: self.b * o.c + self.d * o.d,
            tx: self.a * o.tx + self.c * o.ty + self.tx,
            ty: self.b * o.tx + self.d * o.ty + self.ty,
        }
    }

    fn apply(self, x: f32, y: f32) -> Pos2 {
        pos2(self.a * x + self.c * y + self.tx, self.b * x + self.d * y + self.ty)
    }

    fn origin(self) -> Pos2 {
        pos2(self.tx, self.ty)
    }

    /// Average scale, for sizes that cannot rotate (text, UI boxes).
    fn scale(self) -> f32 {
        ((self.a * self.a + self.b * self.b).sqrt() + (self.c * self.c + self.d * self.d).sqrt()) / 2.0
    }
}

/// One node ready to draw.
struct Item {
    id: NodeId,
    xform: Xform,
    modulate: [f32; 4],
    z: i64,
    /// Inside a CanvasLayer: not affected by the camera.
    screen_space: bool,
}

pub struct SceneRunner {
    /// The running scene, shared with the scripts' host.
    pub world: Rc<RefCell<World>>,
    scripts: Option<Scripts>,
    project_dir: PathBuf,
    textures: HashMap<String, Option<(TextureId, [usize; 2])>>,
    pub debug_collisions: bool,
    /// Path of the running scene (for reload_current_scene).
    pub current: String,
}

fn vec2_prop(n: &neko_core::Node, p: &str, default: [f32; 2]) -> [f32; 2] {
    n.get(p).and_then(|v| v.as_vec2()).unwrap_or(default)
}
fn f32_prop(n: &neko_core::Node, p: &str, default: f32) -> f32 {
    n.get(p).and_then(|v| v.as_f64()).map(|v| v as f32).unwrap_or(default)
}
fn bool_prop(n: &neko_core::Node, p: &str, default: bool) -> bool {
    n.get(p).and_then(|v| v.as_bool()).unwrap_or(default)
}
fn str_prop(n: &neko_core::Node, p: &str) -> String {
    n.get(p).and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
}
fn color_prop(n: &neko_core::Node, p: &str) -> [f32; 4] {
    match n.get(p) {
        Some(Variant::Color(c)) => c,
        _ => [1.0; 4],
    }
}
fn color32(c: [f32; 4]) -> Color32 {
    let f = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgba_unmultiplied(f(c[0]), f(c[1]), f(c[2]), f(c[3]))
}
fn tint(c: Color32, m: [f32; 4]) -> Color32 {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    color32([r as f32 / 255.0 * m[0], g as f32 / 255.0 * m[1], b as f32 / 255.0 * m[2], a as f32 / 255.0 * m[3]])
}

impl SceneRunner {
    /// Load `path` (`res://...` or relative to the project directory).
    pub fn load(project_dir: &Path, path: &str) -> Result<Self, String> {
        let scene = load_scene(project_dir, path)?;
        let world = Rc::new(RefCell::new(World::new(scene, project_dir.to_path_buf())));
        Ok(Self { world, scripts: None, project_dir: project_dir.to_path_buf(), textures: HashMap::new(), debug_collisions: false, current: path.to_string() })
    }

    pub fn root_name(&self) -> String {
        let w = self.world.borrow();
        w.scene.get(w.scene.root()).map(|n| n.name.clone()).unwrap_or_default()
    }

    /// Attach and start the scene's scripts (`_init`, `@onready`, `_ready`),
    /// after registering `class_name` scripts, autoloads and input actions.
    pub fn start_scripts(&mut self, settings: &neko_core::godot::config::ConfigFile) {
        self.world.borrow_mut().settings = settings.clone();
        self.world.borrow_mut().input.actions = crate::script_host::input_map(settings);
        let mut sc = Scripts::new(self.world.clone());
        sc.load_globals(&self.project_dir, settings);
        sc.start();
        self.scripts = Some(sc);
    }

    /// Whether any node runs a script (the game must redraw every frame).
    pub fn has_scripts(&self) -> bool {
        self.scripts.is_some()
    }

    /// Advance scripts by `delta` seconds. Returns false when the game asked to quit.
    pub fn update(&mut self, delta: f64) -> bool {
        let Some(sc) = &mut self.scripts else { return true };
        sc.frame(delta);
        let change = self.world.borrow_mut().change_scene.take();
        if let Some(path) = change {
            let path = if path.is_empty() { self.current.clone() } else { path };
            match load_scene(&self.project_dir, &path) {
                Ok(scene) => {
                    self.current = path;
                    sc.change_scene(scene);
                }
                Err(e) => eprintln!("ERROR: change_scene_to_file: cannot load {path}: {e}"),
            }
        }
        !self.world.borrow().quit
    }

    /// Keyboard input for scripts (`Input`, `_input`).
    pub fn key(&mut self, code: i64, pressed: bool, echo: bool) {
        if let Some(sc) = &mut self.scripts {
            sc.key_event(code, pressed, echo);
        }
    }

    pub fn mouse_moved(&mut self, x: f32, y: f32) {
        self.world.borrow_mut().input.mouse = [x, y];
    }

    /// The texture for a `res://` path, loaded once (PNG for now).
    fn texture(&mut self, ui: &mut Ui, path: &str) -> Option<(TextureId, [usize; 2])> {
        if path.is_empty() {
            return None;
        }
        if let Some(t) = self.textures.get(path) {
            return *t;
        }
        let file = resolve(&self.project_dir, path);
        let loaded = match load_png(&file) {
            Ok(img) => {
                let size = img.size;
                Some((ui.add_texture(img), size))
            }
            Err(e) => {
                eprintln!("ERROR: cannot load texture {path}: {e}");
                None
            }
        };
        self.textures.insert(path.to_string(), loaded);
        loaded
    }

    /// Flatten the tree into draw items with global transforms.
    fn items(scene: &Scene) -> Vec<Item> {
        let db = ClassDb::get();
        let mut out = Vec::new();
        let root = scene.root();
        let mut stack = vec![(root, Xform::IDENTITY, [1.0f32; 4], 0i64, false)];
        while let Some((id, parent_x, parent_mod, parent_z, screen)) = stack.pop() {
            let Some(n) = scene.get(id) else { continue };
            let is_canvas_item = db.inherits(n.class, "Node2D") || db.inherits(n.class, "Control");
            if (is_canvas_item || n.class == "CanvasLayer") && !bool_prop(n, "visible", true) {
                continue;
            }
            let (mut xform, mut modulate, mut z, mut screen) = (parent_x, parent_mod, parent_z, screen);
            if n.class == "CanvasLayer" {
                // A new canvas: identity transform, unaffected by cameras.
                xform = Xform::IDENTITY;
                screen = true;
            } else if db.inherits(n.class, "Node2D") {
                xform = parent_x.mul(Xform::new(vec2_prop(n, "position", [0.0; 2]), f32_prop(n, "rotation_degrees", 0.0), vec2_prop(n, "scale", [1.0; 2])));
                z = parent_z + n.get("z_index").and_then(|v| v.as_f64()).unwrap_or(0.0) as i64;
            } else if db.inherits(n.class, "Control") {
                xform = parent_x.mul(Xform::new(vec2_prop(n, "position", [0.0; 2]), 0.0, [1.0; 2]));
            }
            if is_canvas_item {
                let m = color_prop(n, "modulate");
                modulate = [modulate[0] * m[0], modulate[1] * m[1], modulate[2] * m[2], modulate[3] * m[3]];
                out.push(Item { id, xform, modulate, z, screen_space: screen });
            }
            for &c in n.children().iter().rev() {
                stack.push((c, xform, modulate, z, screen));
            }
        }
        // Tree order, then z_index (stable, like Godot's canvas sorting).
        out.sort_by_key(|i| (i.screen_space, i.z));
        out
    }

    /// The camera transform: the first enabled Camera2D, in tree order.
    fn camera(scene: &Scene, items: &[Item], view: Rect) -> Xform {
        for it in items {
            let Some(n) = scene.get(it.id) else { continue };
            if n.class == "Camera2D" && bool_prop(n, "enabled", true) && !it.screen_space {
                let zoom = vec2_prop(n, "zoom", [1.0; 2]);
                let at = it.xform.origin();
                let c = view.center();
                return Xform { a: zoom[0], b: 0.0, c: 0.0, d: zoom[1], tx: c.x - at.x * zoom[0], ty: c.y - at.y * zoom[1] };
            }
        }
        Xform { tx: view.left(), ty: view.top(), ..Xform::IDENTITY }
    }

    /// Draw one frame of the scene into the `canvas` control.
    pub fn draw(&mut self, ui: &mut Ui, canvas: neko_ui::Id) {
        let world = self.world.clone();
        let w = world.borrow();
        let scene = &w.scene;
        let items = Self::items(scene);
        // Textures must be created before painting (painting borrows the Ui).
        let mut tex: HashMap<NodeId, (TextureId, [usize; 2])> = HashMap::new();
        for it in &items {
            let Some(n) = scene.get(it.id) else { continue };
            if matches!(n.class, "Sprite2D" | "TextureRect") {
                let path = str_prop(n, "texture");
                if let Some(t) = self.texture(ui, &path) {
                    tex.insert(it.id, t);
                }
            }
        }
        let view = ui.rect(canvas);
        let cam = Self::camera(scene, &items, view);
        let debug = self.debug_collisions;
        ui.paint_canvas(canvas, |p, view| {
            for it in &items {
                let Some(n) = scene.get(it.id) else { continue };
                let x = if it.screen_space { Xform { tx: it.xform.tx + view.left(), ty: it.xform.ty + view.top(), ..it.xform } } else { cam.mul(it.xform) };
                draw_node(p, n, x, it.modulate, tex.get(&it.id).copied(), debug);
            }
        });
    }
}

fn draw_node(p: &mut Painter, n: &neko_core::Node, x: Xform, m: [f32; 4], tex: Option<(TextureId, [usize; 2])>, debug: bool) {
    let s = x.scale();
    let size = |n: &neko_core::Node| {
        let v = vec2_prop(n, "size", [0.0; 2]);
        vec2(v[0], v[1]) * s
    };
    match n.class {
        "Sprite2D" => {
            let Some((id, [tw, th])) = tex else { return };
            let (hf, vf) = (f32_prop(n, "hframes", 1.0).max(1.0), f32_prop(n, "vframes", 1.0).max(1.0));
            let frame = f32_prop(n, "frame", 0.0).max(0.0);
            let (fw, fh) = (tw as f32 / hf, th as f32 / vf);
            let (col, row) = (frame % hf, (frame / hf).floor().min(vf - 1.0));
            let mut u0 = col / hf;
            let mut v0 = row / vf;
            let mut u1 = u0 + 1.0 / hf;
            let mut v1 = v0 + 1.0 / vf;
            if bool_prop(n, "flip_h", false) {
                std::mem::swap(&mut u0, &mut u1);
            }
            if bool_prop(n, "flip_v", false) {
                std::mem::swap(&mut v0, &mut v1);
            }
            let off = vec2_prop(n, "offset", [0.0; 2]);
            let (x0, y0) = if bool_prop(n, "centered", true) { (off[0] - fw / 2.0, off[1] - fh / 2.0) } else { (off[0], off[1]) };
            let color = color32(m);
            let mut mesh = Mesh::with_texture(id);
            let corners = [(x0, y0, u0, v0), (x0 + fw, y0, u1, v0), (x0 + fw, y0 + fh, u1, v1), (x0, y0 + fh, u0, v1)];
            for (cx, cy, u, v) in corners {
                mesh.vertices.push(neko_render::epaint::Vertex { pos: x.apply(cx, cy), uv: pos2(u, v), color });
            }
            mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
            p.add(Shape::mesh(mesh));
        }
        "CollisionShape2D" if debug && !bool_prop(n, "disabled", false) => {
            let color = Color32::from_rgba_unmultiplied(0, 153, 179, 107);
            let v = vec2_prop(n, "size", [32.0, 32.0]);
            if str_prop(n, "shape") == "circle" {
                p.circle_filled(x.origin(), v[0] / 2.0 * s, color);
            } else {
                let (hw, hh) = (v[0] / 2.0, v[1] / 2.0);
                let pts = vec![x.apply(-hw, -hh), x.apply(hw, -hh), x.apply(hw, hh), x.apply(-hw, hh)];
                p.add(Shape::convex_polygon(pts, color, Stroke::NONE));
            }
        }
        "Label" | "RichTextLabel" => {
            let text = str_prop(n, "text");
            let text = if n.class == "RichTextLabel" { strip_bbcode(&text) } else { text };
            let r = Rect::from_min_size(x.origin(), size(n));
            let align = match n.get("horizontal_alignment") {
                Some(Variant::Int(1)) => Align2::CENTER_TOP,
                Some(Variant::Int(2)) => Align2::RIGHT_TOP,
                Some(Variant::Str(s)) if s.as_ref() == "center" => Align2::CENTER_TOP,
                Some(Variant::Str(s)) if s.as_ref() == "right" => Align2::RIGHT_TOP,
                _ => Align2::LEFT_TOP,
            };
            let at = match align {
                Align2::CENTER_TOP => pos2(r.center().x, r.top()),
                Align2::RIGHT_TOP => r.right_top(),
                _ => r.left_top(),
            };
            p.text(at, align, &text, FontId::proportional(FONT_SIZE * s), tint(FONT_COLOR, m));
        }
        "Button" => {
            let r = Rect::from_min_size(x.origin(), size(n));
            let disabled = bool_prop(n, "disabled", false);
            p.rect_filled(r, CornerRadius::same(RADIUS), tint(if disabled { STYLE_DISABLED } else { STYLE_NORMAL }, m));
            let fc = if disabled { FONT_COLOR.gamma_multiply(0.5) } else { FONT_COLOR };
            p.text(r.center(), Align2::CENTER_CENTER, &str_prop(n, "text"), FontId::proportional(FONT_SIZE * s), tint(fc, m));
        }
        "LineEdit" => {
            let r = Rect::from_min_size(x.origin(), size(n));
            p.rect_filled(r, CornerRadius::same(RADIUS), tint(STYLE_NORMAL, m));
            let text = str_prop(n, "text");
            let (t, c) = if text.is_empty() { (str_prop(n, "placeholder"), FONT_COLOR.gamma_multiply(0.6)) } else { (text, FONT_COLOR) };
            p.text(r.left_center() + vec2(4.0 * s, 0.0), Align2::LEFT_CENTER, &t, FontId::proportional(FONT_SIZE * s), tint(c, m));
        }
        "Panel" => p.rect_filled(Rect::from_min_size(x.origin(), size(n)), 0, tint(STYLE_NORMAL, m)),
        "PanelContainer" => p.rect_filled(Rect::from_min_size(x.origin(), size(n)), 0, tint(STYLE_NORMAL, m)),
        "ColorRect" => p.rect_filled(Rect::from_min_size(x.origin(), size(n)), 0, color32([0, 1, 2, 3].map(|i| color_prop(n, "color")[i] * m[i]))),
        "HSeparator" | "VSeparator" => {
            // StyleBoxLine, 1 px of style_separator_color (0.5 gray), centered.
            let r = Rect::from_min_size(x.origin(), size(n));
            let c = tint(Color32::from_gray(128), m);
            if n.class == "HSeparator" {
                p.line_segment([pos2(r.left() + 4.0 * s, r.center().y), pos2(r.right() - 4.0 * s, r.center().y)], Stroke::new(s.max(1.0), c));
            } else {
                p.line_segment([pos2(r.center().x, r.top() + 4.0 * s), pos2(r.center().x, r.bottom() - 4.0 * s)], Stroke::new(s.max(1.0), c));
            }
        }
        "HSlider" | "VSlider" => {
            // style_slider (normal color, radius 4) with the filled part as grabber_area.
            let r = Rect::from_min_size(x.origin(), size(n));
            let (min, max) = (f32_prop(n, "min_value", 0.0), f32_prop(n, "max_value", 100.0));
            let t = if max > min { ((f32_prop(n, "value", 0.0) - min) / (max - min)).clamp(0.0, 1.0) } else { 0.0 };
            let thick = 4.0 * s;
            let (track, fill, knob) = if n.class == "HSlider" {
                let tr = Rect::from_center_size(r.center(), vec2(r.width(), thick));
                let kx = tr.left() + tr.width() * t;
                (tr, Rect::from_min_max(tr.min, pos2(kx, tr.bottom())), pos2(kx, tr.center().y))
            } else {
                let tr = Rect::from_center_size(r.center(), vec2(thick, r.height()));
                let ky = tr.bottom() - tr.height() * t;
                (tr, Rect::from_min_max(pos2(tr.left(), ky), tr.max), pos2(tr.center().x, ky))
            };
            p.rect_filled(track, CornerRadius::same(4), tint(STYLE_NORMAL, m));
            p.rect_filled(fill, CornerRadius::same(4), tint(STYLE_PROGRESS, m));
            p.circle_filled(knob, 7.0 * s, tint(Color32::from_gray(223), m));
        }
        "CheckBox" | "CheckButton" => {
            let r = Rect::from_min_size(x.origin(), size(n));
            let on = bool_prop(n, "button_pressed", false);
            let box_r = if n.class == "CheckBox" {
                let b = Rect::from_min_size(pos2(r.left() + 4.0 * s, r.center().y - 8.0 * s), vec2(16.0 * s, 16.0 * s));
                p.rect_filled(b, CornerRadius::same(RADIUS), tint(STYLE_NORMAL, m));
                if on {
                    p.rect_filled(b.shrink(4.0 * s), CornerRadius::same(1), tint(FONT_COLOR, m));
                }
                b
            } else {
                let b = Rect::from_min_size(pos2(r.right() - 40.0 * s, r.center().y - 8.0 * s), vec2(36.0 * s, 16.0 * s));
                p.rect_filled(b, CornerRadius::same(8), tint(if on { STYLE_PROGRESS } else { STYLE_NORMAL }, m));
                let kx = if on { b.right() - 8.0 * s } else { b.left() + 8.0 * s };
                p.circle_filled(pos2(kx, b.center().y), 6.0 * s, tint(FONT_COLOR, m));
                Rect::from_min_size(r.min, vec2(0.0, 0.0))
            };
            let tx = if n.class == "CheckBox" { box_r.right() + 4.0 * s } else { r.left() + 4.0 * s };
            p.text(pos2(tx, r.center().y), Align2::LEFT_CENTER, &str_prop(n, "text"), FontId::proportional(FONT_SIZE * s), tint(FONT_COLOR, m));
        }
        "OptionButton" | "ColorPickerButton" => {
            let r = Rect::from_min_size(x.origin(), size(n));
            p.rect_filled(r, CornerRadius::same(RADIUS), tint(STYLE_NORMAL, m));
            if n.class == "ColorPickerButton" {
                p.rect_filled(r.shrink(4.0 * s), CornerRadius::same(2), color32([0, 1, 2, 3].map(|i| color_prop(n, "color")[i] * m[i])));
            } else {
                p.text(r.left_center() + vec2(8.0 * s, 0.0), Align2::LEFT_CENTER, &str_prop(n, "text"), FontId::proportional(FONT_SIZE * s), tint(FONT_COLOR, m));
                // option_button_arrow
                let c = pos2(r.right() - 12.0 * s, r.center().y);
                p.add(Shape::convex_polygon(vec![c + vec2(-4.0 * s, -2.0 * s), c + vec2(4.0 * s, -2.0 * s), c + vec2(0.0, 3.0 * s)], tint(FONT_COLOR, m), Stroke::NONE));
            }
        }
        "ProgressBar" => {
            let r = Rect::from_min_size(x.origin(), size(n));
            let (min, max) = (f32_prop(n, "min_value", 0.0), f32_prop(n, "max_value", 100.0));
            let ratio = if max > min { ((f32_prop(n, "value", 0.0) - min) / (max - min)).clamp(0.0, 1.0) } else { 0.0 };
            p.rect_filled(r, CornerRadius::same(6), tint(STYLE_DISABLED, m));
            if ratio > 0.0 {
                p.rect_filled(Rect::from_min_size(r.min, vec2(r.width() * ratio, r.height())), CornerRadius::same(6), tint(STYLE_PROGRESS, m));
            }
            if bool_prop(n, "show_percentage", true) {
                p.text(r.center(), Align2::CENTER_CENTER, &format!("{:.0}%", ratio * 100.0), FontId::proportional(FONT_SIZE * s), tint(FONT_COLOR, m));
            }
        }
        "TextureRect" => {
            let Some((id, [tw, th])) = tex else { return };
            let sz = size(n);
            let sz = if sz.x > 0.0 && sz.y > 0.0 { sz } else { vec2(tw as f32, th as f32) * s };
            p.image(neko_ui::Image::new(id, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), sz).tinted(color32(m)), Rect::from_min_size(x.origin(), sz));
        }
        _ => {}
    }
}

/// `[b]Hi[/b]` -> `Hi` (until RichTextLabel gets real BBCode).
fn strip_bbcode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '[' => in_tag = true,
            ']' if in_tag => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}

/// Rasterize an SVG at its own size, like Godot's SVG importer at scale 1.
#[cfg(feature = "image-svg")]
fn load_svg(file: &Path) -> Result<ColorImage, String> {
    let data = std::fs::read(file).map_err(|e| e.to_string())?;
    let tree = resvg::usvg::Tree::from_data(&data, &resvg::usvg::Options::default()).map_err(|e| e.to_string())?;
    let size = tree.size().to_int_size();
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size.width(), size.height()).ok_or("empty SVG")?;
    resvg::render(&tree, resvg::tiny_skia::Transform::identity(), &mut pixmap.as_mut());
    // tiny-skia stores premultiplied RGBA.
    Ok(ColorImage::from_rgba_premultiplied([size.width() as usize, size.height() as usize], pixmap.data()))
}

#[cfg(not(feature = "image-svg"))]
fn load_svg(_file: &Path) -> Result<ColorImage, String> {
    Err("SVG textures need the `image-svg` feature of the neko crate".into())
}

/// `res://a/b.png` -> `<project>/a/b.png`.
pub fn resolve(project_dir: &Path, path: &str) -> PathBuf {
    match path.strip_prefix("res://") {
        Some(rest) => project_dir.join(rest),
        None => project_dir.join(path),
    }
}

/// Load a scene and expand `instance=ExtResource(...)` sub-scenes, the way
/// `PackedScene::instantiate` does in Godot. Editor tabs keep the compact
/// form; only the running game sees the flattened tree.
pub fn load_scene(project_dir: &Path, path: &str) -> Result<Scene, String> {
    load_scene_at(project_dir, path, 0)
}

fn load_scene_at(project_dir: &Path, path: &str, depth: usize) -> Result<Scene, String> {
    if depth > 16 {
        return Err(format!("scene instances nest too deeply at {path}"));
    }
    let file = resolve(project_dir, path);
    let text = std::fs::read_to_string(&file).map_err(|e| format!("cannot open scene {}: {e}", file.display()))?;
    let loaded = neko_core::format::from_str(&text).map_err(|e| format!("cannot load scene {}: {e}", file.display()))?;
    let mut scene = loaded.scene;
    expand_instances(&mut scene, project_dir, depth)?;
    Ok(scene)
}

/// `instance=ExtResource("id")` -> the referenced `.tscn`, grafted in place.
/// Node overrides, children and `[connection]` sections of the outer scene
/// are kept, like Godot's instancing.
fn expand_instances(scene: &mut Scene, project_dir: &Path, depth: usize) -> Result<(), String> {
    loop {
        let found = scene.walk().into_iter().find_map(|(id, _)| {
            let n = scene.get(id)?;
            let (_, rid) = n.godot.header.iter().find(|(k, _)| k == "instance").and_then(|(_, v)| v.resource_ref())?;
            let path = scene.godot.resources.iter().find(|s| s.tag == "ext_resource" && s.field_str("id") == Some(rid))?.field_str("path")?.to_string();
            Some((id, path))
        });
        let Some((place, sub_path)) = found else { break };
        let Some(n) = scene.get(place) else { break };
        let Some(parent) = n.parent() else {
            // Root instances are rare; keep the placeholder rather than
            // guessing how to replace the whole scene.
            break;
        };
        let name = n.name.clone();
        let overrides: Vec<(String, Variant)> = n.overrides().map(|(k, v)| (k.to_string(), v.clone())).collect();
        let raw_props = n.godot.props.clone();
        let header = n.godot.header.clone();
        let children = n.children().to_vec();

        let sub = load_scene_at(project_dir, &sub_path, depth + 1)?;
        let id_map = merge_resources(scene, &sub);
        let sub_root = sub.root();
        let map = scene.graft(parent, &sub).map_err(|e| format!("instantiate {sub_path}: {e}"))?;
        let new_root = map.iter().find(|(old, _)| *old == sub_root).map(|(_, n)| *n).ok_or("instantiate: missing root")?;
        for (_, new) in &map {
            if let Some(n) = scene.get_mut(*new) {
                for (_, v) in n.godot.props.iter_mut().chain(n.godot.header.iter_mut()) {
                    rewrite_refs(v, &id_map);
                }
            }
        }
        for (prop, value) in &overrides {
            let _ = scene.get_mut(new_root).map(|n| n.set(prop, value.clone()));
        }
        // Instance placeholders have no `type`, so the loader keeps their
        // overrides as raw Godot values; decode them against the sub-scene root.
        if let Some(root) = scene.get(new_root) {
            let class = root.class;
            for (k, v) in &raw_props {
                if let Some(def) = ClassDb::get().property(class, k)
                    && let Some(val) = variant_from_godot(&def.default, v)
                {
                    let _ = scene.get_mut(new_root).map(|n| n.set(k, val));
                }
            }
        }
        // Outer raw properties (script override, unknown Godot keys) win.
        if let Some(root) = scene.get_mut(new_root) {
            for (k, v) in raw_props {
                if k == "script" || !root.godot.props.iter().any(|(n, _)| *n == k) {
                    root.godot.props.push((k, v));
                }
            }
            for (k, v) in header.into_iter().filter(|(k, _)| k != "instance") {
                if !root.godot.header.iter().any(|(n, _)| *n == k) {
                    root.godot.header.push((k, v));
                }
            }
        }
        // Children declared in the outer scene (editable children) move in.
        for (i, child) in children.into_iter().enumerate() {
            let _ = scene.move_node(child, new_root, i);
        }
        let prefix = match scene.path(new_root).as_str() {
            "." => String::new(),
            p => format!("{p}/"),
        };
        let sub_prefix = match sub.path(sub_root).as_str() {
            "." => String::new(),
            p => format!("{p}/"),
        };
        for sec in &sub.godot.tail {
            if sec.tag != "connection" {
                continue;
            }
            let mut sec = sec.clone();
            for f in &mut sec.fields {
                if f.0 == "from" || f.0 == "to" {
                    if let Some(p) = f.1.as_str() {
                        let local = p.strip_prefix(&sub_prefix).unwrap_or(p);
                        f.1 = GValue::String(if local == "." || local.is_empty() { if prefix.is_empty() { ".".into() } else { prefix.trim_end_matches('/').into() } } else { format!("{prefix}{local}") }.into());
                    }
                }
            }
            scene.godot.tail.push(sec);
        }
        scene.remove(place).map_err(|e| format!("instantiate {sub_path}: {e}"))?;
        let _ = scene.rename(new_root, &name);
    }
    Ok(())
}

/// Copy a sub-scene's ext/sub resources into the outer scene, remapping ids
/// so scripts and materials of the instance resolve at runtime.
fn merge_resources(scene: &mut Scene, sub: &Scene) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let base = scene.godot.resources.len();
    for (i, sec) in sub.godot.resources.iter().enumerate() {
        if sec.tag != "ext_resource" && sec.tag != "sub_resource" {
            continue;
        }
        let Some(old) = sec.field_str("id") else { continue };
        let new = format!("{old}_s{}_{i}", base);
        map.insert(old.to_string(), new.clone());
        let mut sec = sec.clone();
        for (k, v) in &mut sec.fields {
            if k == "id" {
                *v = GValue::String(new.clone().into());
            }
        }
        scene.godot.resources.push(sec);
    }
    map
}

/// Decode a raw Godot value into the type of a known engine property.
fn variant_from_godot(like: &Variant, v: &GValue) -> Option<Variant> {
    Some(match like {
        Variant::Bool(_) => Variant::Bool(v.as_bool()?),
        Variant::Int(_) => Variant::Int(v.as_i64()?),
        Variant::Float(_) => Variant::Float(v.as_f64()?),
        Variant::Str(_) => Variant::Str(v.as_str()?.to_string().into()),
        Variant::Vec2(_) => {
            let n = v.numbers("Vector2")?;
            Variant::Vec2([*n.first()? as f32, *n.get(1)? as f32])
        }
        Variant::Vec3(_) => {
            let n = v.numbers("Vector3")?;
            Variant::Vec3([*n.first()? as f32, *n.get(1)? as f32, *n.get(2)? as f32])
        }
        Variant::Color(_) => {
            let n = v.numbers("Color")?;
            Variant::Color([*n.first()? as f32, *n.get(1)? as f32, *n.get(2)? as f32, *n.get(3).unwrap_or(&1.0) as f32])
        }
    })
}

fn rewrite_refs(v: &mut GValue, map: &std::collections::HashMap<String, String>) {
    match v {
        GValue::Call(n, args) if (n == "ExtResource" || n == "SubResource") => {
            if let Some(GValue::String(id)) = args.first_mut()
                && let Some(new) = map.get(id.as_str())
            {
                *id = new.clone().into();
            }
        }
        GValue::Array(items) => {
            for v in items {
                rewrite_refs(v, map);
            }
        }
        GValue::Dictionary(pairs) => {
            for (k, v) in pairs {
                rewrite_refs(k, map);
                rewrite_refs(v, map);
            }
        }
        GValue::Call(_, args) => {
            for v in args {
                rewrite_refs(v, map);
            }
        }
        _ => {}
    }
}

fn load_png(file: &Path) -> Result<ColorImage, String> {
    let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if ext == "svg" {
        return load_svg(file);
    }
    if ext != "png" {
        return Err(format!("only PNG and SVG textures are supported for now (got .{ext})"));
    }
    let bytes = std::fs::read(file).map_err(|e| e.to_string())?;
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = dec.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or("image too large")?];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let (w, h) = (info.width as usize, info.height as usize);
    let px = &buf[..info.buffer_size()];
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => px.to_vec(),
        png::ColorType::Rgb => px.as_chunks::<3>().0.iter().flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => px.as_chunks::<2>().0.iter().flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
        png::ColorType::Grayscale => px.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return Err("unexpanded palette".into()),
    };
    Ok(ColorImage::from_rgba_unmultiplied([w, h], &rgba))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transforms_compose_like_godot() {
        let parent = Xform::new([100.0, 0.0], 90.0, [2.0, 2.0]);
        let child = Xform::new([10.0, 0.0], 0.0, [1.0, 1.0]);
        let p = parent.mul(child).origin();
        // Rotated 90 degrees and scaled 2x: (10, 0) -> (0, 20), plus the parent's offset.
        assert!((p.x - 100.0).abs() < 1e-4 && (p.y - 20.0).abs() < 1e-4, "{p:?}");
    }

    #[test]
    fn hidden_branches_and_camera() {
        let mut s = Scene::new("Node2D", "Main").unwrap();
        let root = s.root();
        let a = s.add_child(root, "Node2D", Some("A")).unwrap();
        s.get_mut(a).unwrap().set("visible", Variant::Bool(false)).unwrap();
        let hidden_child = s.add_child(a, "Sprite2D", Some("S")).unwrap();
        let cam = s.add_child(root, "Camera2D", Some("Cam")).unwrap();
        s.get_mut(cam).unwrap().set("position", Variant::Vec2([200.0, 100.0])).unwrap();
        let items = SceneRunner::items(&s);
        assert!(items.iter().all(|i| i.id != hidden_child && i.id != a));
        let view = Rect::from_min_size(Pos2::ZERO, vec2(1152.0, 648.0));
        // The camera's position lands in the middle of the view.
        assert_eq!(SceneRunner::camera(&s, &items, view).apply(200.0, 100.0), pos2(576.0, 324.0));
    }

    #[test]
    fn bbcode_is_stripped() {
        assert_eq!(strip_bbcode("[b]Score[/b]: 3"), "Score: 3");
    }

    #[test]
    fn instanced_scenes_are_flattened() {
        let dir = std::env::temp_dir().join(format!("neko-instance-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("sub.gd"), "extends Node2D\n").unwrap();
        std::fs::write(dir.join("sub.tscn"), r#"[gd_scene load_steps=2 format=3]

[ext_resource type="Script" path="res://sub.gd" id="1_script"]

[node name="SubRoot" type="Node2D"]
script = ExtResource("1_script")
position = Vector2(4, 5)

[node name="Inside" type="Node2D" parent="."]
"#).unwrap();
        std::fs::write(dir.join("main.tscn"), r#"[gd_scene load_steps=2 format=3]

[ext_resource type="PackedScene" path="res://sub.tscn" id="1_sub"]

[node name="Main" type="Node"]

[node name="Sub" parent="." instance=ExtResource("1_sub")]
position = Vector2(100, 0)

[node name="OuterChild" type="Node" parent="Sub"]
"#).unwrap();
        let scene = load_scene(&dir, "res://main.tscn").unwrap();
        let root = scene.root();
        let sub = scene.find("Sub").expect("instance is renamed and grafted");
        assert_eq!(scene.get(sub).unwrap().class, "Node2D");
        assert_eq!(scene.get(sub).unwrap().get("position"), Some(Variant::Vec2([100.0, 0.0])));
        assert!(scene.find("Sub/Inside").is_some());
        assert!(scene.find("Sub/OuterChild").is_some());
        assert_eq!(World::script_path(&scene, sub).as_deref(), Some("res://sub.gd"));
        assert!(scene.find("SubRoot").is_none());
        let _ = std::fs::remove_dir_all(&dir);
        let _ = root;
    }
}
