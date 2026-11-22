//! The 3D viewport (Godot's Node3DEditor): an orbit camera over a perspective
//! grid, drawn as 2D shapes on a neko-ui Canvas until the 3D renderer lands.

use neko::core::{ClassDb, NodeId, Variant};
use neko::platform::MouseButton;
use neko_ui::{Align2, CanvasInput, Color32, FontId, Mesh, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};

use super::{Doc, Editor, Orbit};
use crate::theme::*;

type V3 = [f32; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn mul(a: V3, s: f32) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn norm(a: V3) -> V3 {
    let l = dot(a, a).sqrt().max(1e-6);
    mul(a, 1.0 / l)
}

const NEAR: f32 = 0.05;
const FOV_Y: f32 = 70.0;

struct Cam {
    eye: V3,
    right: V3,
    up: V3,
    fwd: V3,
    center: Pos2,
    focal: f32,
}

impl Cam {
    fn new(o: &Orbit, rect: Rect) -> Self {
        let dir = [o.pitch.cos() * o.yaw.sin(), o.pitch.sin(), o.pitch.cos() * o.yaw.cos()];
        let eye = add(o.target, mul(dir, o.distance));
        let fwd = norm(sub(o.target, eye));
        let right = norm(cross(fwd, [0.0, 1.0, 0.0]));
        let up = cross(right, fwd);
        let focal = rect.height().max(1.0) / 2.0 / (FOV_Y.to_radians() / 2.0).tan();
        Self { eye, right, up, fwd, center: rect.center(), focal }
    }

    fn view(&self, p: V3) -> V3 {
        let d = sub(p, self.eye);
        [dot(d, self.right), dot(d, self.up), dot(d, self.fwd)]
    }

    fn screen(&self, v: V3) -> Pos2 {
        self.center + vec2(v[0] / v[2] * self.focal, -v[1] / v[2] * self.focal)
    }

    fn project(&self, p: V3) -> Option<Pos2> {
        let v = self.view(p);
        (v[2] > NEAR).then(|| self.screen(v))
    }

    /// Project a segment, clipped against the near plane.
    fn segment(&self, a: V3, b: V3) -> Option<[Pos2; 2]> {
        let (mut va, mut vb) = (self.view(a), self.view(b));
        if va[2] <= NEAR && vb[2] <= NEAR {
            return None;
        }
        if va[2] < NEAR {
            let t = (NEAR - va[2]) / (vb[2] - va[2]);
            va = add(va, mul(sub(vb, va), t));
        } else if vb[2] < NEAR {
            let t = (NEAR - vb[2]) / (va[2] - vb[2]);
            vb = add(vb, mul(sub(va, vb), t));
        }
        Some([self.screen(va), self.screen(vb)])
    }

    fn ray(&self, p: Pos2) -> V3 {
        let d = p - self.center;
        norm(add(add(self.fwd, mul(self.right, d.x / self.focal)), mul(self.up, -d.y / self.focal)))
    }
}

fn draw(p: &mut Painter, rect: Rect, doc: &Doc, orbit: &Orbit, show_grid: bool, show_collisions: bool) -> Vec<(NodeId, Pos2)> {
    let cam = Cam::new(orbit, rect);
    // Sky and ground split at the horizon (the default environment's look).
    let flat = norm([cam.fwd[0], 0.0, cam.fwd[2]]);
    let v = cam.view(add(cam.eye, mul(flat, 1e4)));
    let horizon = if v[2] > NEAR { cam.screen(v).y } else { rect.center().y }.clamp(rect.top() - 1.0, rect.bottom() + 1.0);
    let mut mesh = Mesh::default();
    let mut band = |y0: f32, y1: f32, c0: Color32, c1: Color32| {
        if y1 <= y0 {
            return;
        }
        let i = mesh.vertices.len() as u32;
        for (pt, c) in [(pos2(rect.left(), y0), c0), (pos2(rect.right(), y0), c0), (pos2(rect.right(), y1), c1), (pos2(rect.left(), y1), c1)] {
            mesh.colored_vertex(pt, c);
        }
        mesh.add_triangle(i, i + 1, i + 2);
        mesh.add_triangle(i, i + 2, i + 3);
    };
    band(rect.top(), horizon, Color32::from_rgb(0x33, 0x36, 0x3d), Color32::from_rgb(0x8a, 0x8d, 0x93));
    band(horizon, rect.bottom(), Color32::from_rgb(0x3d, 0x37, 0x30), Color32::from_rgb(0x24, 0x21, 0x1e));
    p.add(Shape::mesh(mesh));

    if show_grid {
        let n = 40;
        let center = [orbit.target[0].round(), 0.0, orbit.target[2].round()];
        for i in -n..=n {
            let f = i as f32;
            let fade = 1.0 - (f.abs() / n as f32);
            let alpha = ((if i % 8 == 0 { 70.0 } else { 34.0 }) * fade) as u8;
            let col = Color32::from_rgba_unmultiplied(255, 255, 255, alpha);
            for (a, b) in [
                ([center[0] + f, 0.0, center[2] - n as f32], [center[0] + f, 0.0, center[2] + n as f32]),
                ([center[0] - n as f32, 0.0, center[2] + f], [center[0] + n as f32, 0.0, center[2] + f]),
            ] {
                if let Some(s) = cam.segment(a, b) {
                    p.line_segment(s, Stroke::new(1.0, col));
                }
            }
        }
    }
    let far = 1000.0;
    for (a, b, c) in [([-far, 0.0, 0.0], [far, 0.0, 0.0], AXIS_X), ([0.0, 0.0, -far], [0.0, 0.0, far], AXIS_Z), ([0.0, -far, 0.0], [0.0, far, 0.0], AXIS_Y)] {
        if let Some(s) = cam.segment(a, b) {
            p.line_segment(s, Stroke::new(1.5, c));
        }
    }

    let mut hits = Vec::new();
    if doc.has_root {
        let db = ClassDb::get();
        for (id, _) in doc.scene.walk() {
            let Some(n) = doc.scene.get(id) else { continue };
            if !db.inherits(n.class, "Node3D") || n.get("visible").and_then(|v| v.as_bool()) == Some(false) {
                continue;
            }
            let sel = doc.selected == Some(id);
            let pt = doc.scene.global_position_3d(id);
            let scale = match n.get("scale") {
                Some(Variant::Vec3(s)) => s,
                _ => [1.0; 3],
            };
            let col = if sel { ACCENT } else { Color32::from_rgb(0xff, 0xa0, 0xa8) };
            let mut wire_box = |half: V3, color: Color32| {
                let c = |sx: f32, sy: f32, sz: f32| [pt[0] + sx * half[0], pt[1] + sy * half[1], pt[2] + sz * half[2]];
                let corners = [c(-1., -1., -1.), c(1., -1., -1.), c(1., -1., 1.), c(-1., -1., 1.), c(-1., 1., -1.), c(1., 1., -1.), c(1., 1., 1.), c(-1., 1., 1.)];
                for (a, b) in [(0, 1), (1, 2), (2, 3), (3, 0), (4, 5), (5, 6), (6, 7), (7, 4), (0, 4), (1, 5), (2, 6), (3, 7)] {
                    if let Some(s) = cam.segment(corners[a], corners[b]) {
                        p.line_segment(s, Stroke::new(if sel { 2.0 } else { 1.2 }, color));
                    }
                }
            };
            match n.class {
                "MeshInstance3D" => wire_box(mul(scale, 0.5), col),
                // Godot's Marker3D gizmo: three axis-colored lines through the point.
                "Marker3D" => {
                    let e = n.get("gizmo_extents").and_then(|v| v.as_f64()).unwrap_or(0.25) as f32;
                    for (axis, c) in [([e, 0.0, 0.0], AXIS_X), ([0.0, e, 0.0], AXIS_Y), ([0.0, 0.0, e], AXIS_Z)] {
                        let a = [pt[0] - axis[0], pt[1] - axis[1], pt[2] - axis[2]];
                        let b = add(pt, axis);
                        if let Some(seg) = cam.segment(a, b) {
                            p.line_segment(seg, Stroke::new(if sel { 2.5 } else { 1.5 }, c));
                        }
                    }
                }
                "Label3D" => {
                    if let Some(at) = cam.project(pt) {
                        let text = n.get("text").and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
                        p.text(at, Align2::CENTER_CENTER, &text, FontId::proportional(13.0), if sel { ACCENT } else { Color32::WHITE });
                    }
                }
                "CollisionShape3D" if show_collisions || sel => {
                    let s = match n.get("size") {
                        Some(Variant::Vec3(s)) => s,
                        _ => [1.0; 3],
                    };
                    wire_box([s[0] * scale[0] * 0.5, s[1] * scale[1] * 0.5, s[2] * scale[2] * 0.5], if sel { ACCENT } else { Color32::from_rgb(0x4f, 0xc3, 0xf7) });
                }
                "Camera3D" => {
                    let c = if sel { ACCENT } else { Color32::from_rgb(0xc7, 0x9b, 0xff) };
                    let base = [[-0.4, 0.25, -0.7], [0.4, 0.25, -0.7], [0.4, -0.25, -0.7], [-0.4, -0.25, -0.7]].map(|o| add(pt, o));
                    for i in 0..4 {
                        for s in [cam.segment(pt, base[i]), cam.segment(base[i], base[(i + 1) % 4])].into_iter().flatten() {
                            p.line_segment(s, Stroke::new(1.5, c));
                        }
                    }
                }
                "DirectionalLight3D" | "OmniLight3D" | "SpotLight3D" => {
                    if let Some(sp) = cam.project(pt) {
                        let c = if sel { ACCENT } else { Color32::from_rgb(0xff, 0xe0, 0x8a) };
                        p.circle_stroke(sp, 7.0, Stroke::new(1.5, c));
                        for k in 0..8 {
                            let a = k as f32 * std::f32::consts::FRAC_PI_4;
                            let d = vec2(a.cos(), a.sin());
                            p.line_segment([sp + d * 10.0, sp + d * 14.0], Stroke::new(1.5, c));
                        }
                    }
                }
                _ => {}
            }
            if let Some(sp) = cam.project(pt) {
                p.circle_filled(sp, if sel { 4.5 } else { 3.0 }, col);
                if sel {
                    p.text(sp + vec2(8.0, -8.0), Align2::LEFT_BOTTOM, &n.name, FontId::proportional(11.0), ACCENT);
                }
                hits.push((id, sp));
            }
        }
    }

    // Orientation gizmo, top right.
    let g = pos2(rect.right() - 46.0, rect.top() + 46.0);
    let mut axes = [("X", [1.0, 0.0, 0.0], AXIS_X), ("Y", [0.0, 1.0, 0.0], AXIS_Y), ("Z", [0.0, 0.0, 1.0], AXIS_Z)];
    axes.sort_by(|a, b| dot(b.1, cam.fwd).total_cmp(&dot(a.1, cam.fwd)));
    for (label, dir, c) in axes {
        let d = vec2(dot(dir, cam.right), -dot(dir, cam.up)) * 28.0;
        p.line_segment([g, g + d], Stroke::new(2.0, c));
        p.circle_filled(g + d, 8.0, c);
        p.text(g + d, Align2::CENTER_CENTER, label, FontId::proportional(10.0), BG);
        p.circle_filled(g - d, 5.5, c.gamma_multiply(0.55));
    }
    p.text(rect.left_top() + vec2(10.0, 10.0), Align2::LEFT_TOP, "Perspective", FontId::proportional(13.0), FG);
    hits
}

impl Editor {
    pub(super) fn paint_viewport3d(&mut self) {
        let doc = &self.docs[self.current];
        let (orbit, grid, coll) = (self.orbit, self.show_grid, self.show_collisions);
        let mut hits = Vec::new();
        self.ui.paint_canvas(self.w.viewport, |p, rect| hits = draw(p, rect, doc, &orbit, grid, coll));
        self.hits3d = hits;
    }

    pub(super) fn viewport3d_input(&mut self, input: CanvasInput) {
        let rect = self.ui.rect(self.w.viewport);
        match input {
            CanvasInput::Wheel { delta, .. } => {
                self.orbit.distance = (self.orbit.distance * (-delta.y * 0.12).exp()).clamp(0.5, 500.0);
            }
            CanvasInput::Drag { button: MouseButton::Middle | MouseButton::Right, delta, .. } => {
                let shift = self.ui.modifiers().shift;
                if shift {
                    let cam = Cam::new(&self.orbit, rect);
                    let s = self.orbit.distance / cam.focal;
                    self.orbit.target = add(self.orbit.target, add(mul(cam.right, -delta.x * s), mul(cam.up, delta.y * s)));
                } else {
                    self.orbit.yaw -= delta.x * 0.008;
                    self.orbit.pitch = (self.orbit.pitch + delta.y * 0.008).clamp(-1.5, 1.5);
                }
            }
            CanvasInput::Press { button: MouseButton::Left, pos } => {
                if !self.doc().has_root {
                    return;
                }
                let picked = self.hits3d.iter().filter(|(_, sp)| sp.distance(pos) < 14.0).min_by(|a, b| a.1.distance(pos).total_cmp(&b.1.distance(pos))).map(|(id, _)| *id);
                let root = self.doc().scene.root();
                self.doc_mut().selected = picked.or(Some(root));
                self.dragging_node = picked.is_some();
                self.drag_moved = false;
            }
            CanvasInput::Drag { button: MouseButton::Left, pos, .. } if self.dragging_node => {
                if !self.drag_moved {
                    self.drag_moved = true;
                    self.doc_mut().checkpoint(None);
                }
                let cam = Cam::new(&self.orbit, rect);
                let snap = self.snap;
                let doc = self.doc_mut();
                let Some(id) = doc.selected else { return };
                let global = doc.scene.global_position_3d(id);
                let ray = cam.ray(pos);
                if ray[1].abs() < 1e-4 {
                    return;
                }
                let t = (global[1] - cam.eye[1]) / ray[1];
                if t <= 0.0 {
                    return;
                }
                let hit = add(cam.eye, mul(ray, t));
                if let Some(n) = doc.scene.get_mut(id)
                    && let Some(Variant::Vec3(local)) = n.get("position")
                {
                    let mut nx = local[0] + hit[0] - global[0];
                    let mut nz = local[2] + hit[2] - global[2];
                    if snap {
                        nx = (nx * 2.0).round() / 2.0;
                        nz = (nz * 2.0).round() / 2.0;
                    }
                    let r = |v: f32| (v * 100.0).round() / 100.0;
                    let _ = n.set("position", Variant::Vec3([r(nx), local[1], r(nz)]));
                }
            }
            CanvasInput::Release { button: MouseButton::Left, .. } => self.dragging_node = false,
            _ => {}
        }
    }
}
