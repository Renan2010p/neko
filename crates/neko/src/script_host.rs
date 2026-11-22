//! Runs NekoScript (GDScript) on scene nodes: the engine side of the
//! interpreter (Godot's ClassDB bindings for Node, SceneTree and Input),
//! reduced to what Neko's nodes support today.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use neko_core::godot::Value as GValue;
use neko_core::godot::config::ConfigFile;
use neko_core::{ClassDb, NodeId, Scene, Variant};
use neko_script::interp::{Host, Interp, ObjKind, Object, RResult, ScriptError};
use neko_script::{Dict, Value};

/// Where a node lives: in the running scene, or in a scene that was
/// instantiated but not added to the tree yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Loc {
    Tree(NodeId),
    Orphan(u64, NodeId),
}

#[derive(Default)]
pub struct InputState {
    pub keys: HashSet<i64>,
    /// Godot's `Input.MOUSE_MODE_*`.
    pub mouse_mode: i64,
    pub just_pressed: HashSet<i64>,
    pub just_released: HashSet<i64>,
    pub mouse: [f32; 2],
    pub mouse_buttons: HashSet<i64>,
    pub joy_buttons: HashSet<i64>,
    pub joy_axes: HashMap<i64, f32>,
    /// Action name -> key codes (`Key` values) and joypad buttons (`JOY_BASE + index`).
    pub actions: HashMap<String, Vec<i64>>,
    /// Actions held directly with `Input.action_press()` (tests, UI).
    pub pressed_actions: HashSet<String>,
    pub just_pressed_actions: HashSet<String>,
    pub just_released_actions: HashSet<String>,
    /// Actions pressed by a coroutine after the physics step, delivered to the
    /// next frame's `_physics_process` (Godot's one-frame input semantics).
    pending_pressed_actions: HashSet<String>,
    pending_released_actions: HashSet<String>,
    /// Action -> (joy axis, direction) for `get_action_strength()`.
    pub axis_actions: HashMap<String, Vec<(i64, f32)>>,
}

/// Joypad buttons in action lists are `JOY_BASE + button index`, far above
/// any keyboard code (Godot's special keys use bit 22).
pub const JOY_BASE: i64 = 1 << 40;

impl InputState {
    fn action_codes(&self, a: &str) -> &[i64] {
        self.actions.get(a).map(Vec::as_slice).unwrap_or(&[])
    }

    fn pressed(&self, code: i64) -> bool {
        if code >= JOY_BASE { self.joy_buttons.contains(&(code - JOY_BASE)) } else { self.keys.contains(&code) }
    }

    pub fn action_pressed(&self, a: &str) -> bool {
        self.pressed_actions.contains(a) || self.action_codes(a).iter().any(|&c| self.pressed(c)) || self.action_strength(a) > 0.3
    }

    /// Godot's `get_action_strength`: keys/buttons are 1.0, axes analog.
    pub fn action_strength(&self, a: &str) -> f32 {
        if self.pressed_actions.contains(a) || self.action_codes(a).iter().any(|&c| self.pressed(c)) {
            return 1.0;
        }
        let mut strength = 0.0_f32;
        if let Some(axes) = self.axis_actions.get(a) {
            for &(axis, dir) in axes {
                let v = self.joy_axes.get(&axis).copied().unwrap_or(0.0) * dir;
                strength = strength.max(v);
            }
        }
        strength.clamp(0.0, 1.0)
    }

    fn action_just(&self, a: &str, pressed: bool) -> bool {
        if if pressed { self.just_pressed_actions.contains(a) } else { self.just_released_actions.contains(a) } {
            return true;
        }
        let set = if pressed { &self.just_pressed } else { &self.just_released };
        self.action_codes(a).iter().any(|c| set.contains(c))
    }

    /// Move actions pressed since the last frame into the "just pressed" sets.
    pub fn begin_frame(&mut self) {
        self.just_pressed_actions.extend(self.pending_pressed_actions.drain());
        self.just_released_actions.extend(self.pending_released_actions.drain());
    }

    pub fn end_frame(&mut self) {
        self.just_pressed.clear();
        self.just_released.clear();
        self.just_pressed_actions.clear();
        self.just_released_actions.clear();
    }
}

struct Timer {
    obj: Object,
    left: f64,
}

/// A running Tween. Neko does not animate properties yet, but the duration
/// and callbacks drive `await tween.finished` and camera effects.
struct TweenState {
    obj: Object,
    total: f64,
    elapsed: f64,
    callbacks: Vec<(f64, Value)>,
    done: bool,
}

/// CharacterBody2D state after the last `move_and_slide()`.
#[derive(Clone, Copy, Default)]
struct BodyState2D {
    on_floor: bool,
    on_wall: bool,
    on_ceiling: bool,
}

#[derive(Clone, Copy)]
struct Aabb2 {
    min: [f32; 2],
    max: [f32; 2],
}

impl Aabb2 {
    fn overlaps(&self, o: &Aabb2) -> bool {
        self.min[0] < o.max[0] && self.max[0] > o.min[0] && self.min[1] < o.max[1] && self.max[1] > o.min[1]
    }
}

#[derive(Clone, Copy)]
struct Shape2 {
    aabb: Aabb2,
    one_way: bool,
}

#[derive(Clone, Copy)]
struct RayHit2 {
    point: [f32; 2],
    normal: [f32; 2],
    collider: NodeId,
}

/// State of a `Timer` node.
#[derive(Clone, Copy)]
struct NodeTimer {
    left: f64,
    running: bool,
}

/// State of a script-created resource (`ConfigFile`, `Image`, `FileAccess`...).
enum ResourceState {
    Config(ConfigFile),
    File { path: PathBuf, data: Vec<u8>, write: bool, pos: usize },
    Image { w: usize, h: usize, data: Vec<u8> },
    ImageTexture { image: u64 },
}

/// The running scene plus everything scripts can reach.
pub struct World {
    pub scene: Scene,
    pub project_dir: PathBuf,
    /// Parsed `project.neko` / `project.godot`, for `ProjectSettings`.
    pub settings: ConfigFile,
    resources: HashMap<u64, ResourceState>,
    /// `load()` cache: `res://path` -> Object (resources keep identity).
    resource_cache: HashMap<String, Object>,
    objects: HashMap<NodeId, Object>,
    locs: HashMap<u64, Loc>,
    orphans: HashMap<u64, Scene>,
    /// Objects of nodes not yet added to the tree, by object id.
    orphan_objs: HashMap<u64, Object>,
    pub input: InputState,
    timers: Vec<Timer>,
    tweens: Vec<TweenState>,
    /// Seconds used by `move_and_slide` for the callback being run.
    move_delta: f64,
    body_states: HashMap<u64, BodyState2D>,
    /// Area node -> body nodes currently overlapping.
    area_overlaps: HashMap<NodeId, HashSet<NodeId>>,
    ray_hits: HashMap<u64, RayHit2>,
    node_timers: HashMap<NodeId, NodeTimer>,
    to_free: Vec<Object>,
    /// Nodes added this frame whose `_ready` must run.
    pending_ready: Vec<Object>,
    pub quit: bool,
    pub change_scene: Option<String>,
    no_process: HashSet<u64>,
    groups: HashMap<String, Vec<u64>>,
    extra: HashMap<(u64, String), Value>,
    tree_obj: Option<Object>,
    input_obj: Option<Object>,
    multiplayer_obj: Option<Object>,
    viewport_obj: Option<Object>,
    window_obj: Option<Object>,
    pub viewport_size: [f32; 2],
    pub output: Vec<String>,
    pub time: f64,
    pending_scripts: Vec<neko_script::interp::PendingScript>,
    pending_calls: Vec<neko_script::interp::PendingCall>,
    pending_connections: Vec<neko_script::interp::PendingConnection>,
}

pub struct ScriptHost(pub Rc<RefCell<World>>);

fn s(v: &str) -> Value {
    Value::str(v)
}

fn to_value(v: &Variant) -> Value {
    match v {
        Variant::Bool(b) => Value::Bool(*b),
        Variant::Int(i) => Value::Int(*i),
        Variant::Float(f) => Value::Float(*f),
        Variant::Str(x) => s(x),
        Variant::Vec2([x, y]) => Value::Vector2(*x, *y),
        Variant::Vec3([x, y, z]) => Value::Vector3(*x, *y, *z),
        Variant::Color([r, g, b, a]) => Value::Color(*r, *g, *b, *a),
    }
}

fn from_value(v: &Value, like: &Variant) -> Option<Variant> {
    Some(match like {
        Variant::Bool(_) => Variant::Bool(v.truthy()),
        Variant::Int(_) => Variant::Int(v.as_i64()?),
        Variant::Float(_) => Variant::Float(v.as_f64()?),
        Variant::Str(_) => Variant::Str(v.to_string().into()),
        Variant::Vec2(_) => match v {
            Value::Vector2(x, y) => Variant::Vec2([*x, *y]),
            Value::Vector2i(x, y) => Variant::Vec2([*x as f32, *y as f32]),
            _ => return None,
        },
        Variant::Vec3(_) => match v {
            Value::Vector3(x, y, z) => Variant::Vec3([*x, *y, *z]),
            _ => return None,
        },
        Variant::Color(_) => match v {
            Value::Color(r, g, b, a) => Variant::Color([*r, *g, *b, *a]),
            _ => return None,
        },
    })
}

/// Godot text values kept on nodes (properties Neko does not model) as script values.
fn from_godot(v: &GValue) -> Value {
    match v {
        GValue::Nil => Value::Nil,
        GValue::Bool(b) => Value::Bool(*b),
        GValue::Int(i) => Value::Int(*i),
        GValue::Float(f) => Value::Float(*f),
        GValue::String(x) => s(x),
        GValue::StringName(x) => Value::StringName(Rc::from(x.as_str())),
        GValue::NodePath(x) => Value::NodePath(Rc::from(x.as_str())),
        GValue::Array(items) => Value::array(items.iter().map(from_godot).collect()),
        GValue::Dictionary(pairs) => {
            let mut d = Dict::new();
            for (k, x) in pairs {
                d.set(from_godot(k), from_godot(x));
            }
            Value::dict(d)
        }
        GValue::Call(name, args) => {
            let n: Vec<f32> = args.iter().filter_map(|a| a.as_f64()).map(|x| x as f32).collect();
            match (name.as_str(), n.as_slice()) {
                ("Vector2", [x, y]) => Value::Vector2(*x, *y),
                ("Vector2i", [x, y]) => Value::Vector2i(*x as i32, *y as i32),
                ("Vector3", [x, y, z]) => Value::Vector3(*x, *y, *z),
                ("Color", [r, g, b, a]) => Value::Color(*r, *g, *b, *a),
                ("Rect2", [x, y, w, h]) => Value::Rect2(*x, *y, *w, *h),
                ("Transform3D", _) if n.len() == 12 => {
                    let mut basis = [[0.0f32; 3]; 3];
                    for i in 0..3 {
                        for j in 0..3 {
                            basis[i][j] = n[i * 3 + j];
                        }
                    }
                    Value::Transform3D { basis, origin: [n[9], n[10], n[11]] }
                }
                ("AABB", _) => Value::AABB { position: [0.0; 3], size: [0.0; 3] },
                _ if name.starts_with("Packed") => Value::array(args.iter().map(from_godot).collect()),
                _ => Value::Nil,
            }
        }
        GValue::Object(..) => Value::Nil,
    }
}

/// Script value -> Godot text value (ConfigFile, settings, resource props).
fn to_godot(v: &Value) -> GValue {
    match v {
        Value::Nil => GValue::Nil,
        Value::Bool(b) => GValue::Bool(*b),
        Value::Int(i) => GValue::Int(*i),
        Value::Float(f) => GValue::Float(*f),
        Value::String(s) => GValue::String(s.to_string()),
        Value::StringName(s) => GValue::StringName(s.to_string()),
        Value::NodePath(s) => GValue::NodePath(s.to_string()),
        Value::Array(a) => GValue::Array(a.borrow().iter().map(to_godot).collect()),
        Value::Dictionary(d) => GValue::Dictionary(d.borrow().iter().map(|(k, v)| (to_godot(k), to_godot(v))).collect()),
        Value::Vector2(x, y) => GValue::Call("Vector2".into(), vec![GValue::Float(*x as f64), GValue::Float(*y as f64)]),
        Value::Vector2i(x, y) => GValue::Call("Vector2i".into(), vec![GValue::Int(*x as i64), GValue::Int(*y as i64)]),
        Value::Vector3(x, y, z) => GValue::Call("Vector3".into(), vec![GValue::Float(*x as f64), GValue::Float(*y as f64), GValue::Float(*z as f64)]),
        Value::Color(r, g, b, a) => GValue::Call("Color".into(), vec![GValue::Float(*r as f64), GValue::Float(*g as f64), GValue::Float(*b as f64), GValue::Float(*a as f64)]),
        Value::Transform3D { basis, origin } => {
            let mut args: Vec<GValue> = basis.iter().flat_map(|row| row.iter().map(|x| GValue::Float(*x as f64))).collect();
            args.extend(origin.iter().map(|x| GValue::Float(*x as f64)));
            GValue::Call("Transform3D".into(), args)
        }
        Value::Basis(b) => GValue::Call("Basis".into(), b.iter().flat_map(|row| row.iter().map(|x| GValue::Float(*x as f64))).collect()),
        Value::AABB { position, size } => GValue::Call("AABB".into(), vec![
            GValue::Call("Vector3".into(), position.iter().map(|x| GValue::Float(*x as f64)).collect()),
            GValue::Call("Vector3".into(), size.iter().map(|x| GValue::Float(*x as f64)).collect()),
        ]),
        other => GValue::String(other.to_string()),
    }
}

/// Godot key code for a platform key (`Key` enum in core/os/keyboard.h).
pub fn godot_keycode(k: neko_platform::Key) -> i64 {
    use neko_platform::Key as K;
    const SPECIAL: i64 = 1 << 22;
    match k {
        K::Char(c) => c.to_ascii_uppercase() as i64,
        K::Escape => SPECIAL | 0x01,
        K::Tab => SPECIAL | 0x02,
        K::Backspace => SPECIAL | 0x04,
        K::Enter => SPECIAL | 0x05,
        K::Insert => SPECIAL | 0x07,
        K::Delete => SPECIAL | 0x08,
        K::Home => SPECIAL | 0x0D,
        K::End => SPECIAL | 0x0E,
        K::ArrowLeft => SPECIAL | 0x0F,
        K::ArrowUp => SPECIAL | 0x10,
        K::ArrowRight => SPECIAL | 0x11,
        K::ArrowDown => SPECIAL | 0x12,
        K::PageUp => SPECIAL | 0x13,
        K::PageDown => SPECIAL | 0x14,
        K::Space => 0x20,
        K::F(n) => SPECIAL | (0x1B + n as i64),
        K::Unknown => 0,
    }
}

/// Godot's built-in `ui_*` actions plus the project's `[input]` actions.
pub fn input_map(settings: &neko_core::godot::config::ConfigFile) -> HashMap<String, Vec<i64>> {
    const SPECIAL: i64 = 1 << 22;
    let mut m: HashMap<String, Vec<i64>> = HashMap::new();
    let mut add = |a: &str, codes: &[i64]| m.entry(a.to_string()).or_default().extend_from_slice(codes);
    // core/input/input_map.cpp defaults (keyboard + joypad d-pad/A/B).
    add("ui_accept", &[SPECIAL | 0x05, SPECIAL | 0x06, 0x20, JOY_BASE]);
    add("ui_select", &[0x20, JOY_BASE + 3]);
    add("ui_cancel", &[SPECIAL | 0x01, JOY_BASE + 1]);
    add("ui_left", &[SPECIAL | 0x0F, JOY_BASE + 13]);
    add("ui_right", &[SPECIAL | 0x11, JOY_BASE + 14]);
    add("ui_up", &[SPECIAL | 0x10, JOY_BASE + 11]);
    add("ui_down", &[SPECIAL | 0x12, JOY_BASE + 12]);
    add("ui_focus_next", &[SPECIAL | 0x02]);
    add("ui_page_up", &[SPECIAL | 0x13]);
    add("ui_page_down", &[SPECIAL | 0x14]);
    add("ui_home", &[SPECIAL | 0x0D]);
    add("ui_end", &[SPECIAL | 0x0E]);
    if let Some((_, keys)) = settings.sections.iter().find(|(s, _)| s == "input") {
        for (action, v) in keys {
            let mut codes = Vec::new();
            if let GValue::Dictionary(pairs) = v {
                for (k, ev) in pairs {
                    if k.as_str() != Some("events") {
                        continue;
                    }
                    let GValue::Array(events) = ev else { continue };
                    for e in events {
                        let GValue::Object(class, props) = e else { continue };
                        let p = |n: &str| props.iter().find(|(k, _)| k == n).and_then(|(_, v)| v.as_i64()).unwrap_or(0);
                        match class.as_str() {
                            "InputEventKey" => {
                                let code = if p("keycode") != 0 { p("keycode") } else { p("physical_keycode") };
                                if code != 0 {
                                    codes.push(code);
                                }
                            }
                            "InputEventJoypadButton" => codes.push(JOY_BASE + p("button_index")),
                            _ => {}
                        }
                    }
                }
            }
            m.insert(action.clone(), codes);
        }
    }
    m
}

impl World {
    pub fn new(scene: Scene, project_dir: PathBuf) -> Self {
        World {
            scene,
            project_dir,
            settings: ConfigFile::default(),
            resources: HashMap::new(),
            resource_cache: HashMap::new(),
            objects: HashMap::new(),
            locs: HashMap::new(),
            orphans: HashMap::new(),
            orphan_objs: HashMap::new(),
            input: InputState::default(),
            timers: Vec::new(),
            tweens: Vec::new(),
            move_delta: 1.0 / 60.0,
            body_states: HashMap::new(),
            area_overlaps: HashMap::new(),
            ray_hits: HashMap::new(),
            node_timers: HashMap::new(),
            to_free: Vec::new(),
            pending_ready: Vec::new(),
            quit: false,
            change_scene: None,
            no_process: HashSet::new(),
            groups: HashMap::new(),
            extra: HashMap::new(),
            tree_obj: None,
            input_obj: None,
            multiplayer_obj: None,
            viewport_obj: None,
            window_obj: None,
            viewport_size: [1152.0, 648.0],
            output: Vec::new(),
            time: 0.0,
            pending_scripts: Vec::new(),
            pending_calls: Vec::new(),
            pending_connections: Vec::new(),
        }
    }

    fn scene_of(&self, loc: Loc) -> Option<(&Scene, NodeId)> {
        match loc {
            Loc::Tree(id) => Some((&self.scene, id)),
            Loc::Orphan(root, id) => self.orphans.get(&root).map(|s| (s, id)),
        }
    }

    fn scene_of_mut(&mut self, loc: Loc) -> Option<(&mut Scene, NodeId)> {
        match loc {
            Loc::Tree(id) => Some((&mut self.scene, id)),
            Loc::Orphan(root, id) => self.orphans.get_mut(&root).map(|s| (s, id)),
        }
    }

    /// The object for a node in the tree, created on first use.
    pub fn object(&mut self, id: NodeId) -> Object {
        if let Some(o) = self.objects.get(&id) {
            return o.clone();
        }
        let class = self.scene.get(id).map(|n| n.godot.class.clone().unwrap_or_else(|| n.class.to_string())).unwrap_or_default();
        let o = Object::new(ObjKind::Instance, &class, None);
        o.0.native.set(id.to_bits() | (1 << 63));
        self.objects.insert(id, o.clone());
        self.locs.insert(o.id(), Loc::Tree(id));
        // Groups from the scene file.
        if let Some(n) = self.scene.get(id) {
            for (k, v) in &n.godot.header {
                if k == "groups"
                    && let GValue::Array(gs) = v
                {
                    for g in gs.iter().filter_map(|g| g.as_str()) {
                        self.groups.entry(g.to_string()).or_default().push(o.id());
                    }
                }
            }
        }
        o
    }

    fn loc(&self, o: &Object) -> Option<Loc> {
        self.locs.get(&o.id()).copied()
    }

    /// The `script = ExtResource(...)` path of a node, if any.
    pub fn script_path(scene: &Scene, id: NodeId) -> Option<String> {
        neko_core::tscn::script_path(scene, id)
    }

    fn resolve_path(&self, from: Loc, path: &str) -> Option<Loc> {
        let (scene, start) = self.scene_of(from)?;
        let wrap = |id: NodeId| match from {
            Loc::Tree(_) => Loc::Tree(id),
            Loc::Orphan(r, _) => Loc::Orphan(r, id),
        };
        if let Some(unique) = path.strip_prefix('%') {
            // Unique names: prefer nodes marked `unique_name_in_owner`.
            let ids: Vec<NodeId> = scene.walk().into_iter().map(|(id, _)| id).collect();
            let marked = |id: &NodeId| scene.get(*id).is_some_and(|n| n.name == unique && n.godot.header.iter().any(|(k, v)| k == "unique_name_in_owner" && matches!(v, GValue::Bool(true))));
            let found = ids.iter().find(|id| marked(id)).or_else(|| ids.iter().find(|id| scene.get(**id).is_some_and(|n| n.name == unique))).copied();
            if std::env::var_os("NEKO_DEBUG_UNIQUE").is_some()
                && let Some(id) = found
            {
                eprintln!("unique %{} -> {} ({})", unique, scene.path(id), scene.get(id).map(|n| n.class).unwrap_or("?"));
            }
            return found.map(wrap);
        }
        let (mut cur, rest) = match path.strip_prefix("/root/") {
            Some(r) => {
                let r = r.split_once('/').map(|(_, x)| x).unwrap_or("");
                (scene.root(), r.to_string())
            }
            None => (start, path.to_string()),
        };
        for part in rest.split('/').filter(|p| !p.is_empty() && *p != ".") {
            if part == ".." {
                cur = scene.get(cur)?.parent()?;
                continue;
            }
            let part = part.strip_prefix('%').unwrap_or(part);
            cur = *scene.get(cur)?.children().iter().find(|&&c| scene.get(c).is_some_and(|n| n.name == part))?;
        }
        Some(wrap(cur))
    }

    fn object_at(&mut self, loc: Loc) -> Option<Object> {
        match loc {
            Loc::Tree(id) => Some(self.object(id)),
            Loc::Orphan(root, id) => self.orphan_objs.values().find(|o| self.locs.get(&o.id()) == Some(&Loc::Orphan(root, id))).cloned().or_else(|| {
                let class = self.orphans.get(&root)?.get(id)?.class.to_string();
                let o = Object::new(ObjKind::Instance, &class, None);
                o.0.native.set(1 << 62);
                self.locs.insert(o.id(), loc);
                self.orphan_objs.insert(o.id(), o.clone());
                Some(o)
            }),
        }
    }
}

impl ScriptHost {
    /// CharacterBody2D.move_and_slide() over the axis-aligned colliders in the
    /// scene. Follows Godot's grounded behavior: slide on the collision normal,
    /// report floor/wall/ceiling and snap down when walking over a ledge.
    fn body_move(&mut self, obj: &Object) -> Option<RResult<Value>> {
        let mut w = self.0.borrow_mut();
        let loc = w.loc(obj)?;
        let (id, layer, mask) = {
            let (scene, id) = w.scene_of(loc)?;
            let n = scene.get(id)?;
            (id, node_int(n, "collision_layer", 1), node_int(n, "collision_mask", 1))
        };
        let (offsets, _) = body_offsets(&w, loc)?;
        let origin = {
            let (scene, id) = w.scene_of(loc)?;
            scene.global_position_2d(id)
        };
        let velocity = match Self::prop_get(&mut w, obj, "velocity") {
            Some(Value::Vector2(x, y)) => [x, y],
            _ => [0.0, 0.0],
        };
        let dt = w.move_delta.max(1e-4) as f32;
        let solids = collect_solids(&w, id, layer, mask);
        let prev = w.body_states.get(&obj.id()).copied().unwrap_or_default();
        let body_at = |p: [f32; 2]| Aabb2 { min: [p[0] + offsets.min[0], p[1] + offsets.min[1]], max: [p[0] + offsets.max[0], p[1] + offsets.max[1]] };
        let mut pos = origin;
        let mut vel = velocity;
        let (mut on_floor, mut on_wall, mut on_ceiling, mut collided) = (false, false, false, false);

        // Horizontal first, ignoring one-way platforms.
        let dx = vel[0] * dt;
        if dx.abs() > 1e-6 {
            let mut p = pos;
            p[0] += dx;
            let mut dest = p[0];
            let mut hit = false;
            for s in &solids {
                if s.one_way || !body_at(p).overlaps(&s.aabb) {
                    continue;
                }
                dest = if dx > 0.0 { dest.min(s.aabb.min[0] - offsets.max[0] - 0.001) } else { dest.max(s.aabb.max[0] - offsets.min[0] + 0.001) };
                hit = true;
            }
            if hit {
                pos[0] = dest;
                vel[0] = 0.0;
                on_wall = true;
                collided = true;
            } else {
                pos[0] = p[0];
            }
        }

        // Vertical, with one-way platforms solid only from above.
        let prev_bottom = origin[1] + offsets.max[1];
        let dy = vel[1] * dt;
        if dy.abs() > 1e-6 {
            let mut p = pos;
            p[1] += dy;
            let mut dest = p[1];
            let mut hit = false;
            for s in &solids {
                if !body_at(p).overlaps(&s.aabb) {
                    continue;
                }
                if s.one_way && (dy <= 0.0 || prev_bottom > s.aabb.min[1] + 1.0) {
                    continue;
                }
                if dy > 0.0 {
                    dest = dest.min(s.aabb.min[1] - offsets.max[1] - 0.001);
                    on_floor = true;
                } else {
                    dest = dest.max(s.aabb.max[1] - offsets.min[1] + 0.001);
                    on_ceiling = true;
                }
                hit = true;
            }
            if hit {
                pos[1] = dest;
                vel[1] = 0.0;
                collided = true;
            } else {
                pos[1] = p[1];
            }
        }

        // Floor snap (Godot's `_snap_on_floor`): keep the body grounded when it
        // walks off a small ledge. Never snap while moving up (a jump).
        if !on_floor && prev.on_floor && vel[1] >= 0.0 {
            let snap = object_f32(&w, obj, "floor_snap_length", 8.0).max(1.0);
            let bottom = pos[1] + offsets.max[1];
            let b = body_at(pos);
            let mut best = f32::INFINITY;
            for s in &solids {
                if s.aabb.min[1] < bottom - 0.5 || s.aabb.min[1] > bottom + snap {
                    continue;
                }
                if b.max[0] <= s.aabb.min[0] || b.min[0] >= s.aabb.max[0] {
                    continue;
                }
                best = best.min(s.aabb.min[1] - offsets.max[1]);
            }
            if best.is_finite() {
                pos[1] = best;
                vel[1] = 0.0;
                on_floor = true;
            }
        }

        w.body_states.insert(obj.id(), BodyState2D { on_floor, on_wall, on_ceiling });
        Self::prop_set(&mut w, obj, "global_position", &Value::Vector2(pos[0], pos[1]));
        Self::prop_set(&mut w, obj, "velocity", &Value::Vector2(vel[0], vel[1]));
        ok(Value::Bool(collided))
    }

    /// RayCast2D.force_raycast_update(): test the ray against physics bodies.
    fn ray_update(&mut self, obj: &Object) -> Option<RResult<Value>> {
        let mut w = self.0.borrow_mut();
        let loc = w.loc(obj)?;
        let (origin, target, mask, enabled, exclude, inside) = {
            let (scene, id) = w.scene_of(loc)?;
            let n = scene.get(id)?;
            let origin = scene.global_position_2d(id);
            let target = n.get("target_position").and_then(|v| v.as_vec2()).unwrap_or([0.0, 50.0]);
            let mask = node_int(n, "collision_mask", 1);
            let enabled = n.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true) && n.get("collide_with_bodies").and_then(|v| v.as_bool()).unwrap_or(true);
            let exclude = n.get("exclude_parent").and_then(|v| v.as_bool()).unwrap_or(true).then(|| n.parent()).flatten();
            let inside = n.get("hit_from_inside").and_then(|v| v.as_bool()).unwrap_or(false);
            (origin, target, mask, enabled, exclude, inside)
        };
        if !enabled {
            w.ray_hits.remove(&obj.id());
            return ok(Value::Nil);
        }
        let hit = raycast_world(&w, origin, [origin[0] + target[0], origin[1] + target[1]], mask, exclude, inside);
        match hit {
            Some(h) => {
                w.ray_hits.insert(obj.id(), h);
            }
            None => {
                w.ray_hits.remove(&obj.id());
            }
        }
        ok(Value::Nil)
    }

    fn body_flag(&self, obj: &Object, pick: fn(&BodyState2D) -> bool) -> bool {
        self.0.borrow().body_states.get(&obj.id()).is_some_and(pick)
    }

    fn prop_get(w: &mut World, obj: &Object, name: &str) -> Option<Value> {
        let loc = w.loc(obj)?;
        let (scene, id) = w.scene_of(loc)?;
        let n = scene.get(id)?;
        if let Some(v) = w.extra.get(&(obj.id(), name.to_string())) {
            return Some(v.clone());
        }
        let db = ClassDb::get();
        let is2d = db.inherits(n.class, "Node2D");
        let is3d = db.inherits(n.class, "Node3D");
        let v = match name {
            "name" => s(&n.name),
            "rotation" if is2d => Value::Float(n.get("rotation_degrees").and_then(|v| v.as_f64()).unwrap_or(0.0).to_radians()),
            "rotation" if is3d => match n.get("rotation_degrees") {
                Some(Variant::Vec3([x, y, z])) => Value::Vector3(x.to_radians(), y.to_radians(), z.to_radians()),
                _ => Value::Vector3(0.0, 0.0, 0.0),
            },
            "global_position" if is2d => {
                let [x, y] = scene.global_position_2d(id);
                Value::Vector2(x, y)
            }
            "global_position" if is3d => {
                let [x, y, z] = scene.global_position_3d(id);
                Value::Vector3(x, y, z)
            }
            "transform" if is3d => {
                let [x, y, z] = match n.get("position") { Some(Variant::Vec3(v)) => v, _ => [0.0; 3] };
                let [sx, sy, sz] = match n.get("scale") { Some(Variant::Vec3(v)) => v, _ => [1.0; 3] };
                Value::Transform3D { basis: [[sx, 0.0, 0.0], [0.0, sy, 0.0], [0.0, 0.0, sz]], origin: [x, y, z] }
            }
            "placeholder_text" => to_value(&n.get("placeholder")?),
            "shadow_enabled" => to_value(&n.get("shadows")?),
            _ => {
                if let Some(v) = n.get(name) {
                    to_value(&v)
                } else if let Some((_, gv)) = n.godot.props.iter().find(|(k, _)| k == name) {
                    from_godot(gv)
                } else {
                    w.extra.get(&(obj.id(), name.to_string()))?.clone()
                }
            }
        };
        Some(v)
    }

    fn prop_set(w: &mut World, obj: &Object, name: &str, v: &Value) -> bool {
        let Some(loc) = w.loc(obj) else { return false };
        let oid = obj.id();
        let Some((scene, id)) = w.scene_of_mut(loc) else { return false };
        let db = ClassDb::get();
        let Some(class) = scene.get(id).map(|n| n.class) else { return false };
        let is2d = db.inherits(class, "Node2D");
        let is3d = db.inherits(class, "Node3D");
        let (prop, val): (&str, Option<Variant>) = match name {
            "name" => {
                let _ = scene.rename(id, &v.to_string());
                return true;
            }
            "rotation" if is2d => ("rotation_degrees", v.as_f64().map(|r| Variant::Float(r.to_degrees()))),
            "rotation" if is3d => (
                "rotation_degrees",
                match v {
                    Value::Vector3(x, y, z) => Some(Variant::Vec3([x.to_degrees(), y.to_degrees(), z.to_degrees()])),
                    _ => None,
                },
            ),
            "global_position" if is2d => {
                let Value::Vector2(x, y) = v else { return false };
                let parent = scene.get(id).and_then(|n| n.parent()).map(|p| scene.global_position_2d(p)).unwrap_or([0.0, 0.0]);
                ("position", Some(Variant::Vec2([x - parent[0], y - parent[1]])))
            }
            "global_position" if is3d => {
                let Value::Vector3(x, y, z) = v else { return false };
                let parent = scene.get(id).and_then(|n| n.parent()).map(|p| scene.global_position_3d(p)).unwrap_or([0.0; 3]);
                ("position", Some(Variant::Vec3([x - parent[0], y - parent[1], z - parent[2]])))
            }
            "placeholder_text" => ("placeholder", Some(Variant::Str(v.to_string().into()))),
            "shadow_enabled" => ("shadows", Some(Variant::Bool(v.truthy()))),
            _ => match db.property(class, name) {
                // Object-typed resource properties (`environment`, `texture`,
                // materials...) keep the object so scripts can read it back.
                Some(def) if matches!(v, Value::Object(_)) && matches!(def.default, Variant::Str(_)) => {
                    w.extra.insert((oid, name.to_string()), v.clone());
                    return true;
                }
                Some(def) => (name, from_value(v, &def.default)),
                None => {
                    w.extra.insert((oid, name.to_string()), v.clone());
                    return true;
                }
            },
        };
        let Some(val) = val else { return false };
        let Some((scene, id)) = w.scene_of_mut(loc) else { return false };
        scene.get_mut(id).is_some_and(|n| n.set(prop, val).is_ok())
    }
}

fn arg_str(a: &[Value], i: usize) -> String {
    a.get(i).map(|v| v.to_string()).unwrap_or_default()
}

fn ok(v: Value) -> Option<RResult<Value>> {
    Some(Ok(v))
}

fn fail(m: impl Into<String>) -> Option<RResult<Value>> {
    Some(Err(ScriptError { message: m.into(), file: String::new(), line: 0, func: String::new() }))
}

// ------------------------------------------------------------ 2D physics
// A small AABB implementation of the parts of Godot's physics that 2D games
// use: CharacterBody2D.move_and_slide(), one-way platforms, Area2D overlaps
// and RayCast2D. Godot's own solver (modules/godot_physics_2d) handles shapes,
// slopes and rotation; Neko currently supports axis-aligned rectangles,
// capsules (as boxes) and circles, which is enough for platformers.

fn object_prop(w: &World, obj: &Object, name: &str) -> Option<Value> {
    w.extra.get(&(obj.id(), name.to_string())).cloned().or_else(|| obj.0.vars.borrow().get(name).cloned())
}

fn object_f32(w: &World, obj: &Object, name: &str, default: f32) -> f32 {
    object_prop(w, obj, name).and_then(|v| v.as_f64()).map(|v| v as f32).unwrap_or(default)
}

fn object_vec2(w: &World, obj: &Object, name: &str, default: [f32; 2]) -> [f32; 2] {
    match object_prop(w, obj, name) {
        Some(Value::Vector2(x, y)) => [x, y],
        Some(Value::Vector2i(x, y)) => [x as f32, y as f32],
        _ => default,
    }
}

fn node_int(node: &neko_core::Node, name: &str, default: i64) -> i64 {
    match node.get(name) {
        Some(Variant::Int(i)) => i,
        Some(Variant::Float(f)) => f as i64,
        _ => default,
    }
}

/// The collision shapes under one body, in global (axis-aligned) space.
fn node_shapes(w: &World, id: NodeId) -> Vec<Shape2> {
    let mut out = Vec::new();
    let Some(node) = w.scene.get(id) else { return out };
    let origin = w.scene.global_position_2d(id);
    let db = ClassDb::get();
    for &child in node.children() {
        let Some(cn) = w.scene.get(child) else { continue };
        if !db.inherits(cn.class, "CollisionShape2D") || cn.get("disabled").and_then(|v| v.as_bool()).unwrap_or(false) {
            continue;
        }
        let Some(cs) = w.objects.get(&child) else { continue };
        let Some(Value::Object(shape)) = object_prop(w, cs, "shape") else { continue };
        let local = cn.get("position").and_then(|v| v.as_vec2()).unwrap_or([0.0, 0.0]);
        let center = [origin[0] + local[0], origin[1] + local[1]];
        let half = match shape.0.native_class.as_str() {
            "CircleShape2D" => [object_f32(w, &shape, "radius", 10.0); 2],
            "CapsuleShape2D" => {
                let r = object_f32(w, &shape, "radius", 10.0);
                let h = object_f32(w, &shape, "height", 2.0 * r);
                [r, h / 2.0]
            }
            _ => {
                let size = object_vec2(w, &shape, "size", [32.0, 32.0]);
                [size[0] / 2.0, size[1] / 2.0]
            }
        };
        let one_way = object_prop(w, cs, "one_way_collision").is_some_and(|v| v.truthy()) || cn.get("one_way_collision").and_then(|v| v.as_bool()).unwrap_or(false);
        out.push(Shape2 {
            aabb: Aabb2 { min: [center[0] - half[0], center[1] - half[1]], max: [center[0] + half[0], center[1] + half[1]] },
            one_way,
        });
    }
    out
}

fn union_shapes(shapes: &[Shape2]) -> Option<Aabb2> {
    let mut out: Option<Aabb2> = None;
    for s in shapes {
        out = Some(match out {
            None => s.aabb,
            Some(a) => Aabb2 { min: [a.min[0].min(s.aabb.min[0]), a.min[1].min(s.aabb.min[1])], max: [a.max[0].max(s.aabb.max[0]), a.max[1].max(s.aabb.max[1])] },
        });
    }
    out
}

/// The body's shapes relative to its origin, for moving it.
fn body_offsets(w: &World, loc: Loc) -> Option<(Aabb2, bool)> {
    let (scene, id) = w.scene_of(loc)?;
    let origin = scene.global_position_2d(id);
    let shapes = node_shapes(w, id);
    let union = union_shapes(&shapes)?;
    let one_way = shapes.iter().any(|s| s.one_way);
    Some((
        Aabb2 { min: [union.min[0] - origin[0], union.min[1] - origin[1]], max: [union.max[0] - origin[0], union.max[1] - origin[1]] },
        one_way,
    ))
}

fn collect_solids(w: &World, self_id: NodeId, self_layer: i64, self_mask: i64) -> Vec<Shape2> {
    let db = ClassDb::get();
    let mut out = Vec::new();
    for (id, _) in w.scene.walk() {
        if id == self_id {
            continue;
        }
        let Some(n) = w.scene.get(id) else { continue };
        if !db.inherits(n.class, "CollisionObject2D") || db.inherits(n.class, "Area2D") {
            continue;
        }
        let layer = node_int(n, "collision_layer", 1);
        let mask = node_int(n, "collision_mask", 1);
        // Godot's can_collide_with(): either mask sees the other layer.
        if (self_mask & layer) == 0 && (mask & self_layer) == 0 {
            continue;
        }
        out.extend(node_shapes(w, id));
    }
    out
}

/// Physics bodies (not areas) with their layer and world AABB.
fn collect_collision_bodies(w: &World) -> Vec<(NodeId, i64, Aabb2)> {
    let db = ClassDb::get();
    let mut out = Vec::new();
    for (id, _) in w.scene.walk() {
        let Some(n) = w.scene.get(id) else { continue };
        if !db.inherits(n.class, "CollisionObject2D") || db.inherits(n.class, "Area2D") {
            continue;
        }
        if let Some(aabb) = union_shapes(&node_shapes(w, id)) {
            out.push((id, node_int(n, "collision_layer", 1), aabb));
        }
    }
    out
}

fn collect_areas(w: &World) -> Vec<(NodeId, i64, i64, Aabb2)> {
    let db = ClassDb::get();
    let mut out = Vec::new();
    for (id, _) in w.scene.walk() {
        let Some(n) = w.scene.get(id) else { continue };
        if !db.inherits(n.class, "Area2D") || !n.get("monitoring").and_then(|v| v.as_bool()).unwrap_or(true) {
            continue;
        }
        if let Some(aabb) = union_shapes(&node_shapes(w, id)) {
            out.push((id, node_int(n, "collision_layer", 1), node_int(n, "collision_mask", 1), aabb));
        }
    }
    out
}

fn ray_aabb(origin: [f32; 2], dir: [f32; 2], aabb: &Aabb2) -> Option<(f32, [f32; 2])> {
    let mut tmin = 0.0f32;
    let mut tmax = 1.0f32;
    let mut normal = [0.0f32; 2];
    for axis in 0..2 {
        if dir[axis].abs() < 1e-6 {
            if origin[axis] < aabb.min[axis] || origin[axis] > aabb.max[axis] {
                return None;
            }
        } else {
            let inv = 1.0 / dir[axis];
            let mut t1 = (aabb.min[axis] - origin[axis]) * inv;
            let mut t2 = (aabb.max[axis] - origin[axis]) * inv;
            let mut n = [0.0f32; 2];
            n[axis] = -1.0;
            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
                n[axis] = 1.0;
            }
            if t1 > tmin {
                tmin = t1;
                normal = n;
            }
            if t2 < tmax {
                tmax = t2;
            }
            if tmin > tmax {
                return None;
            }
        }
    }
    (0.0..=1.0).contains(&tmin).then_some((tmin, normal))
}

fn raycast_world(w: &World, origin: [f32; 2], target: [f32; 2], mask: i64, exclude: Option<NodeId>, hit_from_inside: bool) -> Option<RayHit2> {
    let dir = [target[0] - origin[0], target[1] - origin[1]];
    if dir[0].abs() < 1e-6 && dir[1].abs() < 1e-6 {
        return None;
    }
    let db = ClassDb::get();
    let mut best: Option<(f32, RayHit2)> = None;
    for (id, _) in w.scene.walk() {
        if Some(id) == exclude {
            continue;
        }
        let Some(n) = w.scene.get(id) else { continue };
        if !db.inherits(n.class, "CollisionObject2D") || db.inherits(n.class, "Area2D") {
            continue;
        }
        if node_int(n, "collision_layer", 1) & mask == 0 {
            continue;
        }
        let Some(obj) = w.objects.get(&id) else { continue };
        for shape in node_shapes(w, id) {
            let Some((t, normal)) = ray_aabb(origin, dir, &shape.aabb) else { continue };
            if t <= 1e-6 && !hit_from_inside {
                continue;
            }
            if best.as_ref().is_none_or(|(bt, _)| t < *bt) {
                let _ = obj;
                best = Some((t, RayHit2 { point: [origin[0] + dir[0] * t, origin[1] + dir[1] * t], normal, collider: id }));
            }
        }
    }
    best.map(|(_, hit)| hit)
}

impl Host for ScriptHost {
    fn get_property(&mut self, obj: &Object, name: &str) -> Option<Value> {
        if name == "multiplayer" {
            return Some(Value::Object(self.multiplayer_object()));
        }
        if obj.0.native_class == "SceneTree" && (name == "root" || name == "current_scene") {
            let root = self.0.borrow().scene.root();
            return Some(Value::Object(self.0.borrow_mut().object(root)));
        }
        let mut w = self.0.borrow_mut();
        if obj.0.native_class == "Input" && name == "mouse_mode" {
            return Some(Value::Int(w.input.mouse_mode));
        }
        if obj.0.native_class.starts_with("InputEvent") {
            return obj.0.vars.borrow().get(name).cloned();
        }
        if let Some(v) = w.extra.get(&(obj.id(), name.to_string())) {
            return Some(v.clone());
        }
        if obj.0.native_class == "Window" {
            let [width, height] = w.viewport_size;
            match name {
                "size" => return Some(Value::Vector2i(width as i32, height as i32)),
                "min_size" | "max_size" => return Some(Value::Vector2i(0, 0)),
                "visible" => return Some(Value::Bool(true)),
                _ => {}
            }
        }
        Self::prop_get(&mut w, obj, name)
    }

    fn set_property(&mut self, obj: &Object, name: &str, value: &Value) -> bool {
        if obj.0.native_class.starts_with("InputEvent") {
            obj.0.vars.borrow_mut().insert(name.to_string(), value.clone());
            return true;
        }
        let mut w = self.0.borrow_mut();
        if obj.0.native_class == "Input" && name == "mouse_mode" {
            w.input.mouse_mode = value.as_i64().unwrap_or(0);
            return true;
        }
        if is_singleton_class(&obj.0.native_class) || w.loc(obj).is_none() {
            // Singletons, resources and plain engine objects keep what scripts
            // assign even when the renderer does not use it yet.
            w.extra.insert((obj.id(), name.to_string()), value.clone());
            return true;
        }
        Self::prop_set(&mut w, obj, name, value)
    }

    fn call_method(&mut self, it: &mut Interp, obj: &Object, name: &str, a: &[Value]) -> Option<RResult<Value>> {
        let class = obj.0.native_class.clone();
        match class.as_str() {
            "SceneTree" => return self.tree_method(it, name, a),
            "Input" => return self.input_method(name, a),
            "PackedScene" => return self.packed_scene_method(it, obj, name),
            "InputEventKey" | "InputEventMouseButton" | "InputEventMouseMotion" | "InputEventJoypadButton" | "InputEventJoypadMotion" => return self.event_method(obj, name, a),
            "Engine" | "OS" | "Time" | "InputMap" | "DisplayServer" | "ProjectSettings" | "ResourceLoader" | "AudioServer" | "RenderingServer" | "JSON" => return self.singleton_method(&class, name, a),
            "ConfigFile" => return self.config_method(obj, name, a),
            "DirAccess" => return self.dir_method(obj, name, a),
            "FileAccess" => return self.file_method(obj, name, a),
            "Image" => return self.image_method(obj, name, a),
            "ImageTexture" => return self.image_texture_method(obj, name, a),
            "ClassDB" => return self.classdb_method(it, name, a),
            "Tween" => return self.tween_method(obj, name, a),
            "MultiplayerAPI" => return self.multiplayer_method(name),
            "RandomNumberGenerator" => return self.rng_method(obj, name, a),
            "Viewport" | "ViewportTexture" => return self.viewport_method(it, obj, name, a),
            "Window" => return self.window_method(name),
            "StyleBoxFlat" | "StyleBox" | "StyleBoxTexture" | "ShaderMaterial" | "StandardMaterial3D" | "Environment" | "Sky"
            | "ProceduralSkyMaterial" | "SurfaceTool" | "ArrayMesh" | "BoxMesh" | "SphereMesh" | "CapsuleMesh" | "CylinderMesh"
            | "PlaneMesh" | "Font" | "FontFile" | "Theme" | "Resource" | "Texture2D" | "FastNoiseLite" | "NoiseTexture2D" | "MultiMesh"
            | "Gradient" | "GradientTexture1D" | "GradientTexture2D" => return self.resource_method(obj, name, a),
            _ => {}
        }
        if class.starts_with("AudioStreamPlayer") {
            return match name {
                "play" | "stop" | "pause" | "seek" => ok(Value::Nil),
                "is_playing" | "has_stream_playback" => ok(Value::Bool(false)),
                "get_stream_playback" => ok(Value::Nil),
                "get_playback_position" => ok(Value::Float(0.0)),
                _ => None,
            };
        }
        if name == "create_tween" {
            let o = Object::new(ObjKind::Instance, "Tween", None);
            self.0.borrow_mut().tweens.push(TweenState { obj: o.clone(), total: 0.0, elapsed: 0.0, callbacks: Vec::new(), done: false });
            return ok(Value::Object(o));
        }
        if class.starts_with("PacketPeer") || class.starts_with("StreamPeer") || class.starts_with("UDPServer") || class.starts_with("WebSocketPeer") {
            return self.network_method(name, a);
        }
        let loc = self.0.borrow().loc(obj)?;
        let tree = matches!(loc, Loc::Tree(_));
        match name {
            "get_node" | "get_node_or_null" | "has_node" => {
                let path = arg_str(a, 0);
                let found = {
                    let w = self.0.borrow();
                    w.resolve_path(loc, &path)
                };
                let o = found.and_then(|l| self.0.borrow_mut().object_at(l));
                match (name, o) {
                    ("has_node", o) => ok(Value::Bool(o.is_some())),
                    (_, Some(o)) => ok(Value::Object(o)),
                    ("get_node", None) => {
                        self.print_error(&format!("Node not found: \"{path}\" (relative to \"{}\").", obj.describe()));
                        ok(Value::Nil)
                    }
                    _ => ok(Value::Nil),
                }
            }
            "get_parent" => {
                let mut w = self.0.borrow_mut();
                let parent = w.scene_of(loc).and_then(|(s, id)| s.get(id)?.parent());
                let o = parent.and_then(|p| {
                    let l = match loc {
                        Loc::Tree(_) => Loc::Tree(p),
                        Loc::Orphan(r, _) => Loc::Orphan(r, p),
                    };
                    w.object_at(l)
                });
                ok(o.map(Value::Object).unwrap_or(Value::Nil))
            }
            "get_children" | "get_child" | "get_child_count" => {
                let mut w = self.0.borrow_mut();
                let kids: Vec<NodeId> = w.scene_of(loc).and_then(|(s, id)| s.get(id).map(|n| n.children().to_vec())).unwrap_or_default();
                let wrap = |c: NodeId| match loc {
                    Loc::Tree(_) => Loc::Tree(c),
                    Loc::Orphan(r, _) => Loc::Orphan(r, c),
                };
                match name {
                    "get_child_count" => ok(Value::Int(kids.len() as i64)),
                    "get_child" => {
                        let mut i = a.first().and_then(Value::as_i64).unwrap_or(0);
                        if i < 0 {
                            i += kids.len() as i64;
                        }
                        ok(kids.get(i as usize).and_then(|&c| w.object_at(wrap(c))).map(Value::Object).unwrap_or(Value::Nil))
                    }
                    _ => ok(Value::array(kids.into_iter().filter_map(|c| w.object_at(wrap(c))).map(Value::Object).collect())),
                }
            }
            "add_child" => {
                let Some(Value::Object(child)) = a.first() else { return fail("add_child() needs a Node.") };
                self.add_child(it, loc, child)
            }
            "queue_free" | "free" => {
                if tree {
                    self.0.borrow_mut().to_free.push(obj.clone());
                } else {
                    obj.0.freed.set(true);
                }
                ok(Value::Nil)
            }
            "is_inside_tree" => ok(Value::Bool(tree)),
            "get_tree" => ok(Value::Object(self.tree_object())),
            "get_name" => ok(Self::prop_get(&mut self.0.borrow_mut(), obj, "name").unwrap_or(Value::Nil)),
            "get_path" => {
                let w = self.0.borrow();
                let p = w.scene_of(loc).map(|(s, id)| s.path(id)).unwrap_or_default();
                ok(Value::NodePath(Rc::from(format!("/root/{}", p.trim_start_matches('.')).as_str())))
            }
            "set_process" | "set_physics_process" | "set_process_input" | "set_process_unhandled_input" => {
                let mut w = self.0.borrow_mut();
                if a.first().is_some_and(Value::truthy) {
                    w.no_process.remove(&obj.id());
                } else {
                    w.no_process.insert(obj.id());
                }
                ok(Value::Nil)
            }
            "is_processing" | "is_physics_processing" => ok(Value::Bool(!self.0.borrow().no_process.contains(&obj.id()))),
            "show" | "hide" => {
                Self::prop_set(&mut self.0.borrow_mut(), obj, "visible", &Value::Bool(name == "show"));
                ok(Value::Nil)
            }
            "is_visible_in_tree" | "is_visible" => ok(Self::prop_get(&mut self.0.borrow_mut(), obj, "visible").unwrap_or(Value::Bool(true))),
            "add_to_group" => {
                let g = arg_str(a, 0);
                let mut w = self.0.borrow_mut();
                let list = w.groups.entry(g).or_default();
                if !list.contains(&obj.id()) {
                    list.push(obj.id());
                }
                ok(Value::Nil)
            }
            "remove_from_group" => {
                let g = arg_str(a, 0);
                if let Some(l) = self.0.borrow_mut().groups.get_mut(&g) {
                    l.retain(|&x| x != obj.id());
                }
                ok(Value::Nil)
            }
            "is_in_group" => {
                let g = arg_str(a, 0);
                ok(Value::Bool(self.0.borrow().groups.get(&g).is_some_and(|l| l.contains(&obj.id()))))
            }
            "translate" | "global_translate" => {
                let mut w = self.0.borrow_mut();
                let cur = Self::prop_get(&mut w, obj, "position");
                if let (Some(c), Some(d)) = (cur, a.first()) {
                    let n = neko_script::builtins::binary(neko_script::ast::BinOp::Add, &c, d).unwrap_or(c);
                    Self::prop_set(&mut w, obj, "position", &n);
                }
                ok(Value::Nil)
            }
            "rotate" => {
                let mut w = self.0.borrow_mut();
                let r = Self::prop_get(&mut w, obj, "rotation").and_then(|v| v.as_f64()).unwrap_or(0.0);
                Self::prop_set(&mut w, obj, "rotation", &Value::Float(r + a.first().and_then(Value::as_f64).unwrap_or(0.0)));
                ok(Value::Nil)
            }
            "look_at" => {
                let mut w = self.0.borrow_mut();
                if let (Some(Value::Vector2(px, py)), Some(Value::Vector2(tx, ty))) = (Self::prop_get(&mut w, obj, "global_position"), a.first()) {
                    let ang = ((ty - py) as f64).atan2((tx - px) as f64);
                    Self::prop_set(&mut w, obj, "rotation", &Value::Float(ang));
                }
                ok(Value::Nil)
            }
            "move_and_slide" => self.body_move(obj),
            "is_on_floor" => ok(Value::Bool(self.body_flag(obj, |s| s.on_floor))),
            "is_on_wall" => ok(Value::Bool(self.body_flag(obj, |s| s.on_wall))),
            "is_on_ceiling" => ok(Value::Bool(self.body_flag(obj, |s| s.on_ceiling))),
            "force_raycast_update" => self.ray_update(obj),
            "is_colliding" => ok(Value::Bool(self.0.borrow().ray_hits.contains_key(&obj.id()))),
            "get_collision_point" => {
                let hit = self.0.borrow().ray_hits.get(&obj.id()).copied();
                ok(hit.map(|h| Value::Vector2(h.point[0], h.point[1])).unwrap_or(Value::Nil))
            }
            "get_collision_normal" => {
                let hit = self.0.borrow().ray_hits.get(&obj.id()).copied();
                ok(hit.map(|h| Value::Vector2(h.normal[0], h.normal[1])).unwrap_or(Value::Nil))
            }
            "get_collider" => {
                let id = self.0.borrow().ray_hits.get(&obj.id()).map(|h| h.collider);
                let o = id.and_then(|id| self.0.borrow().objects.get(&id).cloned());
                ok(o.map(Value::Object).unwrap_or(Value::Nil))
            }
            "get_viewport" => ok(Value::Object(self.viewport_object())),
            "make_current" | "clear_current" | "reset_smoothing" | "force_update_scroll" => ok(Value::Nil),
            "is_current" => ok(Value::Bool(true)),
            "get_window" => ok(Value::Object(self.window_object())),
            "get_viewport_rect" => {
                let [vw, vh] = self.0.borrow().viewport_size;
                ok(Value::Rect2(0.0, 0.0, vw, vh))
            }
            "get_global_mouse_position" | "get_local_mouse_position" => {
                let [x, y] = self.0.borrow().input.mouse;
                ok(Value::Vector2(x, y))
            }
            "get_index" => {
                let w = self.0.borrow();
                let i = w.scene_of(loc).and_then(|(s, id)| {
                    let p = s.get(id)?.parent()?;
                    s.get(p)?.children().iter().position(|&c| c == id)
                });
                ok(Value::Int(i.map(|i| i as i64).unwrap_or(-1)))
            }
            "get_scene_file_path" => ok(obj.0.vars.borrow().get("scene_file").cloned().unwrap_or(Value::str(""))),
            "remove_child" => {
                let Some(Value::Object(child)) = a.first() else { return fail("remove_child() needs a Node.") };
                let mut w = self.0.borrow_mut();
                let Some(cl) = w.loc(child) else { return fail("remove_child(): unknown child.") };
                match (loc, cl) {
                    (Loc::Tree(_), Loc::Tree(cid)) => match w.scene.extract(cid) {
                        Ok((sub, map)) => {
                            let key = child.id();
                            w.orphans.insert(key, sub);
                            let nodes: Vec<NodeId> = w.objects.keys().copied().collect();
                            for old in nodes {
                                if let Some((_, new)) = map.iter().find(|(o, _)| *o == old)
                                    && let Some(o) = w.objects.remove(&old)
                                {
                                    let oid = o.id();
                                    w.locs.insert(oid, Loc::Orphan(key, *new));
                                    w.orphan_objs.insert(oid, o);
                                }
                            }
                            ok(Value::Nil)
                        }
                        Err(e) => fail(format!("remove_child(): {e}")),
                    },
                    _ => ok(Value::Nil),
                }
            }
            "is_node_ready" => ok(Value::Bool(tree)),
            "move_child" => {
                let Some(Value::Object(child)) = a.first() else { return fail("move_child() needs a Node.") };
                let mut w = self.0.borrow_mut();
                let Some(cl) = w.loc(child) else { return fail("move_child(): unknown child.") };
                let index = a.get(1).and_then(Value::as_i64).unwrap_or(0).max(0) as usize;
                let result = match (loc, cl) {
                    (Loc::Tree(p), Loc::Tree(c)) => w.scene.move_node(c, p, index),
                    (Loc::Orphan(k, _), Loc::Orphan(ck, c)) if k == ck => match w.orphans.get(&k).and_then(|sc| sc.get(c)?.parent()) {
                        Some(pid) => w.orphans.get_mut(&k).expect("listed").move_node(c, pid, index),
                        None => Err(neko_core::SceneError::NoSuchNode),
                    },
                    _ => Err(neko_core::SceneError::NoSuchNode),
                };
                ok(Value::Bool(result.is_ok()))
            }
            "get_theme_font" | "get_theme_font_size" | "get_theme_color" | "get_theme_constant" | "get_theme_stylebox" => match name {
                "get_theme_font" => ok(Value::Object(Object::new(ObjKind::Instance, "Font", None))),
                "get_theme_font_size" => ok(Value::Int(16)),
                "get_theme_constant" => ok(Value::Int(0)),
                _ => ok(Value::Nil),
            },
            "set_anchors_and_offsets_preset" | "queue_sort" | "set_owner" => ok(Value::Nil),
            "get_owner" => ok(Value::Nil),
            "get_minimum_size" | "get_combined_minimum_size" => ok(Value::Vector2(0.0, 0.0)),
            "add_item" if self.is_class(obj, "OptionButton") => {
                let text = arg_str(a, 0);
                let mut vars = obj.0.vars.borrow_mut();
                let mut items = match vars.get("items") {
                    Some(Value::Array(x)) => x.borrow().clone(),
                    _ => Vec::new(),
                };
                items.push(Value::str(&text));
                vars.insert("items".into(), Value::array(items));
                ok(Value::Nil)
            }
            "clear" if self.is_class(obj, "OptionButton") => {
                obj.0.vars.borrow_mut().insert("items".into(), Value::array(Vec::new()));
                ok(Value::Nil)
            }
            "get_item_count" if self.is_class(obj, "OptionButton") => {
                let n = match obj.0.vars.borrow().get("items") {
                    Some(Value::Array(x)) => x.borrow().len(),
                    _ => 0,
                };
                ok(Value::Int(n as i64))
            }
            "get_item_text" if self.is_class(obj, "OptionButton") => {
                let i = a.first().and_then(Value::as_i64).unwrap_or(0);
                let text = match obj.0.vars.borrow().get("items") {
                    Some(Value::Array(x)) => x.borrow().get(i.max(0) as usize).map(|v| v.to_string()).unwrap_or_default(),
                    _ => String::new(),
                };
                ok(s(&text))
            }
            "select" | "get_selected" | "get_selected_id" if self.is_class(obj, "OptionButton") => {
                if name == "select" {
                    let i = a.first().and_then(Value::as_i64).unwrap_or(-1);
                    obj.0.vars.borrow_mut().insert("selected".into(), Value::Int(i));
                    ok(Value::Nil)
                } else {
                    ok(obj.0.vars.borrow().get("selected").cloned().unwrap_or(Value::Int(-1)))
                }
            }
            "find_children" => {
                let pattern = arg_str(a, 0);
                let class = arg_str(a, 1);
                let recursive = a.get(2).is_none_or(Value::truthy);
                let mut w = self.0.borrow_mut();
                let kids = w.scene_of(loc).map(|(s, id)| s.get(id).map(|n| n.children().to_vec()).unwrap_or_default()).unwrap_or_default();
                let mut found = Vec::new();
                let mut stack: Vec<NodeId> = kids;
                while let Some(c) = stack.pop() {
                    let info = w.scene_of(loc).and_then(|(s, _)| s.get(c)).map(|n| (n.name.clone(), n.class));
                    let Some((name, kclass)) = info else { continue };
                    if wildcard(&pattern, &name) && (class.is_empty() || class == "*" || ClassDb::get().inherits(kclass, &class) || kclass == class) {
                        let l = match loc {
                            Loc::Tree(_) => Loc::Tree(c),
                            Loc::Orphan(_, _) => loc,
                        };
                        if let Some(o) = w.object_at(l) {
                            found.push(Value::Object(o));
                        }
                    }
                    if recursive {
                        let children = w.scene_of(loc).and_then(|(s, _)| s.get(c)).map(|n| n.children().to_vec()).unwrap_or_default();
                        stack.extend(children);
                    }
                }
                ok(Value::array(found))
            }
            "find_child" => {
                let pat = arg_str(a, 0);
                let mut w = self.0.borrow_mut();
                let found = w.scene_of(loc).and_then(|(sc, id)| {
                    let mut stack: Vec<NodeId> = sc.get(id)?.children().to_vec();
                    while let Some(c) = stack.pop() {
                        let n = sc.get(c)?;
                        if n.name == pat || (pat.contains('*') && n.name.starts_with(pat.trim_end_matches('*'))) {
                            return Some(c);
                        }
                        stack.extend(n.children().iter().rev());
                    }
                    None
                });
                let o = found.and_then(|c| {
                    let l = match loc {
                        Loc::Tree(_) => Loc::Tree(c),
                        Loc::Orphan(r, _) => Loc::Orphan(r, c),
                    };
                    w.object_at(l)
                });
                ok(o.map(Value::Object).unwrap_or(Value::Nil))
            }
            // Timer node.
            "start" | "stop" | "is_stopped" | "get_time_left" if self.is_class(obj, "Timer") => {
                let Loc::Tree(id) = loc else { return ok(Value::Nil) };
                let mut w = self.0.borrow_mut();
                let wait = w.scene.get(id).and_then(|n| n.get("wait_time")).and_then(|v| v.as_f64()).unwrap_or(1.0);
                match name {
                    "start" => {
                        let t = a.first().and_then(Value::as_f64).filter(|t| *t > 0.0).unwrap_or(wait);
                        w.node_timers.insert(id, NodeTimer { left: t, running: true });
                        ok(Value::Nil)
                    }
                    "stop" => {
                        w.node_timers.remove(&id);
                        ok(Value::Nil)
                    }
                    "is_stopped" => ok(Value::Bool(!w.node_timers.get(&id).is_some_and(|t| t.running))),
                    _ => ok(Value::Float(w.node_timers.get(&id).map(|t| t.left).unwrap_or(0.0))),
                }
            }
            "set_text" | "set_position" | "set_visible" | "set_modulate" | "set_scale" | "set_rotation" | "set_value" | "set_global_position" => {
                let p = &name[4..];
                Self::prop_set(&mut self.0.borrow_mut(), obj, p, a.first().unwrap_or(&Value::Nil));
                ok(Value::Nil)
            }
            "get_text" | "get_position" | "get_scale" | "get_rotation" | "get_value" | "get_global_position" | "get_modulate" => ok(Self::prop_get(&mut self.0.borrow_mut(), obj, &name[4..]).unwrap_or(Value::Nil)),
            // Rendering/UI calls that need no work yet.
            "queue_redraw" | "grab_focus" | "release_focus" | "set_anchors_preset" | "add_theme_color_override" | "add_theme_font_size_override" | "add_theme_constant_override" | "add_theme_stylebox_override" | "add_theme_font_override" | "set_meta" | "accept_event" | "set_physics_process_internal" | "reset_physics_interpolation" | "set_deferred" | "force_update_transform" => ok(Value::Nil),
            _ => None,
        }
    }

    fn get_node(&mut self, from: &Object, path: &str) -> Option<Object> {
        let loc = self.0.borrow().loc(from)?;
        let found = self.0.borrow().resolve_path(loc, path)?;
        self.0.borrow_mut().object_at(found)
    }

    fn global(&mut self, _it: &mut Interp, name: &str) -> Option<Value> {
        match name {
            "Input" => Some(Value::Object(self.input_object())),
            "Engine" | "OS" | "Time" | "ProjectSettings" | "DisplayServer" | "ResourceLoader" | "InputMap" | "AudioServer" | "ClassDB" | "RenderingServer" | "JSON" => {
                let o = Object::new(ObjKind::Instance, name, None);
                o.0.native.set(1 << 61);
                Some(Value::Object(o))
            }
            _ => None,
        }
    }

    fn is_class(&self, obj: &Object, class: &str) -> bool {
        let c = &obj.0.native_class;
        c == class || ClassDb::get().inherits(c, class) || class == "Object" || (class == "Node" && ClassDb::get().class(c).is_some()) || (class == "CanvasItem" && (ClassDb::get().inherits(c, "Node2D") || ClassDb::get().inherits(c, "Control")))
    }

    fn has_signal(&self, obj: &Object, name: &str) -> bool {
        let c = obj.0.native_class.as_str();
        let db = ClassDb::get();
        let is = |base: &str| c == base || db.inherits(c, base);
        match name {
            "process_frame" | "physics_frame" | "node_added" | "node_removed" | "node_renamed" | "tree_changed" => c == "SceneTree",
            "peer_connected" | "peer_disconnected" | "connected_to_server" | "connection_failed" | "server_disconnected" => c == "MultiplayerAPI",
            "finished" if c == "Tween" => true,
            "ready" | "tree_entered" | "tree_exiting" | "tree_exited" | "renamed" | "child_entered_tree" | "child_exiting_tree" | "child_order_changed" => db.class(c).is_some(),
            "visibility_changed" | "draw" | "hidden" => is("Node2D") || is("Control"),
            "timeout" => c == "SceneTreeTimer" || is("Timer"),
            "pressed" | "button_down" | "button_up" | "toggled" => is("Button"),
            "item_selected" | "item_focused" => c == "OptionButton",
            "color_changed" | "picker_created" | "popup_closed" => c == "ColorPickerButton",
            "value_changed" | "changed" | "drag_started" | "drag_ended" => is("Range"),
            "text_changed" | "text_submitted" | "editing_toggled" => c == "LineEdit",
            "mouse_entered" | "mouse_exited" | "focus_entered" | "focus_exited" | "resized" | "gui_input" | "minimum_size_changed" => is("Control"),
            "body_entered" | "body_exited" | "area_entered" | "area_exited" | "body_shape_entered" | "body_shape_exited" => is("Area2D") || is("Area3D"),
            "animation_finished" | "animation_started" | "animation_changed" => c == "AnimationPlayer" || c == "AnimatedSprite2D",
            "frame_changed" => c == "AnimatedSprite2D" || c == "Sprite2D",
            "finished" => c.starts_with("AudioStreamPlayer") || c.starts_with("GpuParticles") || c.starts_with("CPUParticles"),
            "request_completed" => c == "HttpRequest",
            "frame_post_draw" | "frame_pre_draw" => c == "RenderingServer",
            "screen_entered" | "screen_exited" => c.starts_with("VisibleOnScreen"),
            _ => false,
        }
    }

    fn instantiate(&mut self, _it: &mut Interp, class: &str) -> Option<Object> {
        let o = Object::new(ObjKind::Instance, class, None);
        let mut w = self.0.borrow_mut();
        // Node classes become a one-node orphan scene; other engine classes
        // (`ConfigFile`, `Image`, input events, materials...) are plain objects.
        if ClassDb::get().class(class).is_some() {
            let scene = Scene::new(class, class).ok()?;
            let root = scene.root();
            o.0.native.set(1 << 62);
            w.orphans.insert(o.id(), scene);
            w.locs.insert(o.id(), Loc::Orphan(o.id(), root));
            w.orphan_objs.insert(o.id(), o.clone());
            return Some(o);
        }
        let state = match class {
            "ConfigFile" => Some(ResourceState::Config(ConfigFile::default())),
            "Image" => Some(ResourceState::Image { w: 1, h: 1, data: vec![0; 4] }),
            _ => None,
        };
        if let Some(s) = state {
            w.resources.insert(o.id(), s);
        }
        Some(o)
    }

    fn load(&mut self, it: &mut Interp, path: &str) -> Option<Value> {
        if let Some(o) = self.0.borrow().resource_cache.get(path) {
            return Some(Value::Object(o.clone()));
        }
        let file = crate::scene_runner::resolve(&self.0.borrow().project_dir, path);
        if !file.exists() {
            return None;
        }
        if path.ends_with(".tres") {
            return self.load_tres(it, path, &file);
        }
        if path.ends_with(".tscn") || path.ends_with(".scn") {
            let o = Object::new(ObjKind::Instance, "PackedScene", None);
            o.0.vars.borrow_mut().insert("resource_path".into(), s(path));
            self.0.borrow_mut().resource_cache.insert(path.to_string(), o.clone());
            return Some(Value::Object(o));
        }
        // Textures and other resources: an object with its path.
        let class = if path.ends_with(".png") || path.ends_with(".svg") || path.ends_with(".jpg") || path.ends_with(".webp") { "Texture2D" } else { "Resource" };
        let o = Object::new(ObjKind::Instance, class, None);
        o.0.vars.borrow_mut().insert("resource_path".into(), s(path));
        self.0.borrow_mut().resource_cache.insert(path.to_string(), o.clone());
        Some(Value::Object(o))
    }

    fn print(&mut self, text: &str) {
        println!("{text}");
        self.0.borrow_mut().output.push(text.to_string());
    }

    fn print_error(&mut self, text: &str) {
        eprintln!("ERROR: {text}");
        self.0.borrow_mut().output.push(format!("ERROR: {text}"));
    }

    fn take_pending_scripts(&mut self) -> Vec<neko_script::interp::PendingScript> {
        std::mem::take(&mut self.0.borrow_mut().pending_scripts)
    }

    fn take_pending_calls(&mut self) -> Vec<neko_script::interp::PendingCall> {
        std::mem::take(&mut self.0.borrow_mut().pending_calls)
    }

    fn take_pending_connections(&mut self) -> Vec<neko_script::interp::PendingConnection> {
        std::mem::take(&mut self.0.borrow_mut().pending_connections)
    }
}

impl ScriptHost {
    fn tree_object(&mut self) -> Object {
        let mut w = self.0.borrow_mut();
        if let Some(o) = &w.tree_obj {
            return o.clone();
        }
        let o = Object::new(ObjKind::Instance, "SceneTree", None);
        o.0.native.set(1 << 60);
        w.tree_obj = Some(o.clone());
        o
    }

    fn input_object(&mut self) -> Object {
        let mut w = self.0.borrow_mut();
        if let Some(o) = &w.input_obj {
            return o.clone();
        }
        let o = Object::new(ObjKind::Instance, "Input", None);
        o.0.native.set(1 << 59);
        w.input_obj = Some(o.clone());
        o
    }

    fn window_object(&mut self) -> Object {
        let mut w = self.0.borrow_mut();
        if let Some(o) = &w.window_obj {
            return o.clone();
        }
        let o = Object::new(ObjKind::Instance, "Window", None);
        o.0.native.set(1 << 55);
        w.window_obj = Some(o.clone());
        o
    }

    fn viewport_object(&mut self) -> Object {
        let mut w = self.0.borrow_mut();
        if let Some(o) = &w.viewport_obj {
            return o.clone();
        }
        let o = Object::new(ObjKind::Instance, "Viewport", None);
        o.0.native.set(1 << 56);
        w.viewport_obj = Some(o.clone());
        o
    }

    fn viewport_method(&mut self, _it: &mut Interp, obj: &Object, name: &str, _a: &[Value]) -> Option<RResult<Value>> {
        let [w, h] = self.0.borrow().viewport_size;
        match (obj.0.native_class.as_str(), name) {
            ("Viewport", "get_visible_rect") => ok(Value::Rect2(0.0, 0.0, w, h)),
            ("Viewport", "get_size") | ("Viewport", "get_size_override") => ok(Value::Vector2i(w as i32, h as i32)),
            ("Viewport", "get_texture") => {
                let o = Object::new(ObjKind::Instance, "ViewportTexture", None);
                ok(Value::Object(o))
            }
            ("Viewport", "get_camera_3d") => ok(Value::Nil),
            ("Viewport", "get_window") => ok(Value::Object(obj.clone())),
            ("ViewportTexture", "get_image") => {
                let o = Object::new(ObjKind::Instance, "Image", None);
                let iw = (w as usize).max(1);
                let ih = (h as usize).max(1);
                self.0.borrow_mut().resources.insert(o.id(), ResourceState::Image { w: iw, h: ih, data: vec![0; iw * ih * 4] });
                ok(Value::Object(o))
            }
            ("ViewportTexture", "get_width") => ok(Value::Int(w as i64)),
            ("ViewportTexture", "get_height") => ok(Value::Int(h as i64)),
            _ => ok(Value::Nil),
        }
    }

    fn window_method(&mut self, name: &str) -> Option<RResult<Value>> {
        match name {
            "set_title" | "set_min_size" | "set_max_size" | "set_size" | "set_mode" | "set_flag" | "set_ime_enabled"
            | "set_ime_position" | "show" | "hide" | "close_requested" => ok(Value::Nil),
            "get_title" => ok(Value::str("")),
            "get_size" => {
                let [w, h] = self.0.borrow().viewport_size;
                ok(Value::Vector2i(w as i32, h as i32))
            }
            "get_min_size" | "get_max_size" => ok(Value::Vector2i(0, 0)),
            "has_focus" | "is_maximized" | "is_minimized" | "is_visible" => ok(Value::Bool(true)),
            "get_window_id" => ok(Value::Int(1)),
            _ => None,
        }
    }

    fn multiplayer_object(&mut self) -> Object {
        let mut w = self.0.borrow_mut();
        if let Some(o) = &w.multiplayer_obj {
            return o.clone();
        }
        let o = Object::new(ObjKind::Instance, "MultiplayerAPI", None);
        o.0.native.set(1 << 57);
        w.multiplayer_obj = Some(o.clone());
        o
    }

    fn tree_method(&mut self, it: &mut Interp, name: &str, a: &[Value]) -> Option<RResult<Value>> {
        match name {
            "create_timer" => {
                let t = Object::new(ObjKind::Instance, "SceneTreeTimer", None);
                let secs = a.first().and_then(Value::as_f64).unwrap_or(1.0);
                self.0.borrow_mut().timers.push(Timer { obj: t.clone(), left: secs });
                ok(Value::Object(t))
            }
            "quit" => {
                self.0.borrow_mut().quit = true;
                ok(Value::Nil)
            }
            "change_scene_to_file" => {
                self.0.borrow_mut().change_scene = Some(arg_str(a, 0));
                ok(Value::Int(0))
            }
            "reload_current_scene" => {
                self.0.borrow_mut().change_scene = Some(String::new());
                ok(Value::Int(0))
            }
            "get_nodes_in_group" | "get_first_node_in_group" => {
                let g = arg_str(a, 0);
                let ids = self.0.borrow().groups.get(&g).cloned().unwrap_or_default();
                let objs: Vec<Object> = {
                    let w = self.0.borrow();
                    ids.iter().filter_map(|id| w.objects.values().find(|o| o.id() == *id && o.is_valid()).cloned()).collect()
                };
                if name == "get_first_node_in_group" {
                    return ok(objs.into_iter().next().map(Value::Object).unwrap_or(Value::Nil));
                }
                ok(Value::array(objs.into_iter().map(Value::Object).collect()))
            }
            "call_group" => {
                let g = arg_str(a, 0);
                let m = arg_str(a, 1);
                let args = a[2.min(a.len())..].to_vec();
                let mut w = self.0.borrow_mut();
                let ids = w.groups.get(&g).cloned().unwrap_or_default();
                let objs: Vec<Object> = ids.iter().filter_map(|id| w.objects.values().find(|o| o.id() == *id && o.is_valid()).cloned()).collect();
                for obj in objs {
                    w.pending_calls.push(neko_script::interp::PendingCall { obj, method: m.clone(), args: args.clone() });
                }
                let _ = it;
                drop(w);
                ok(Value::Nil)
            }
            "get_root" | "get_current_scene" => {
                let root = self.0.borrow().scene.root();
                ok(Value::Object(self.0.borrow_mut().object(root)))
            }
            "set_pause" | "set_auto_accept_quit" => ok(Value::Nil),
            "get_frame" => ok(Value::Int((self.0.borrow().time * 60.0) as i64)),
            _ => None,
        }
    }

    fn input_method(&mut self, name: &str, a: &[Value]) -> Option<RResult<Value>> {
        if name == "action_press" || name == "action_release" {
            let act = arg_str(a, 0);
            let mut w = self.0.borrow_mut();
            if name == "action_press" {
                if w.input.pressed_actions.insert(act.clone()) {
                    w.input.pending_pressed_actions.insert(act);
                }
            } else if w.input.pressed_actions.remove(&act) {
                w.input.pending_released_actions.insert(act);
            }
            return ok(Value::Nil);
        }
        let w = self.0.borrow();
        let i = &w.input;
        let act = arg_str(a, 0);
        let strength = |n: &str| i.action_strength(n) as f64;
        match name {
            "is_action_pressed" => ok(Value::Bool(i.action_pressed(&act))),
            "is_action_just_pressed" => ok(Value::Bool(i.action_just(&act, true))),
            "is_action_just_released" => ok(Value::Bool(i.action_just(&act, false))),
            "get_action_strength" | "get_action_raw_strength" => ok(Value::Float(i.action_strength(&act) as f64)),
            "get_axis" => ok(Value::Float(strength(&arg_str(a, 1)) - strength(&act))),
            "get_vector" => {
                let x = strength(&arg_str(a, 1)) - strength(&act);
                let y = strength(&arg_str(a, 3)) - strength(&arg_str(a, 2));
                let l = ((x * x + y * y) as f32).sqrt();
                let (x, y) = if l > 1.0 { (x as f32 / l, y as f32 / l) } else { (x as f32, y as f32) };
                ok(Value::Vector2(x, y))
            }
            "is_key_pressed" | "is_physical_key_pressed" => ok(Value::Bool(i.keys.contains(&a.first().and_then(Value::as_i64).unwrap_or(-1)))),
            "is_mouse_button_pressed" => ok(Value::Bool(i.mouse_buttons.contains(&a.first().and_then(Value::as_i64).unwrap_or(-1)))),
            "is_joy_button_pressed" => ok(Value::Bool(i.joy_buttons.contains(&a.get(1).and_then(Value::as_i64).unwrap_or(-1)))),
            "get_joy_axis" => ok(Value::Float(i.joy_axes.get(&a.get(1).and_then(Value::as_i64).unwrap_or(-1)).copied().unwrap_or(0.0) as f64)),
            "get_mouse_position" | "get_last_mouse_velocity" => ok(Value::Vector2(i.mouse[0], i.mouse[1])),
            "has_action" => ok(Value::Bool(i.actions.contains_key(&act))),
            "get_connected_joypads" => ok(Value::array(Vec::new())),
            "set_mouse_mode" | "start_joy_vibration" | "stop_joy_vibration" | "warp_mouse" | "flush_buffered_events"
            | "set_custom_mouse_cursor" | "set_default_cursor_shape" | "vibrate_handheld" => ok(Value::Nil),
            "get_mouse_mode" => ok(Value::Int(i.mouse_mode)),
            _ => None,
        }
    }

    /// Engine singletons (`Engine`, `OS`, `Time`, `InputMap`...).
    fn singleton_method(&mut self, class: &str, name: &str, a: &[Value]) -> Option<RResult<Value>> {
        if class == "InputMap" && name == "add_action" {
            self.0.borrow_mut().input.actions.entry(arg_str(a, 0)).or_default();
            return ok(Value::Nil);
        }
        if class == "InputMap" && name == "action_add_event" {
            let action = arg_str(a, 0);
            let Some(Value::Object(ev)) = a.get(1) else { return ok(Value::Nil) };
            let vars = ev.0.vars.borrow();
            let mut w = self.0.borrow_mut();
            let codes = w.input.actions.entry(action.clone()).or_default();
            match ev.0.native_class.as_str() {
                "InputEventKey" => {
                    let code = vars.get("physical_keycode").or_else(|| vars.get("keycode")).and_then(Value::as_i64).unwrap_or(0);
                    if code != 0 && !codes.contains(&code) {
                        codes.push(code);
                    }
                }
                "InputEventJoypadButton" => {
                    let code = vars.get("button_index").and_then(Value::as_i64).unwrap_or(-1);
                    if code >= 0 {
                        let code = JOY_BASE + code;
                        if !codes.contains(&code) {
                            codes.push(code);
                        }
                    }
                }
                "InputEventJoypadMotion" => {
                    let axis = vars.get("axis").and_then(Value::as_i64).unwrap_or(0);
                    let value = vars.get("axis_value").and_then(Value::as_f64).unwrap_or(0.0) as f32;
                    w.input.axis_actions.entry(action).or_default().push((axis, value));
                }
                _ => {}
            }
            return ok(Value::Nil);
        }
        if class == "ProjectSettings" && name == "set_setting" {
            let key = arg_str(a, 0);
            let (section, key) = setting_parts(&key);
            self.0.borrow_mut().settings.set(&section, &key, to_godot(a.get(1).unwrap_or(&Value::Nil)));
            return ok(Value::Nil);
        }
        let w = self.0.borrow();
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
        match (class, name) {
            ("ProjectSettings", "get_setting" | "get_setting_with_override") => {
                let (section, key) = setting_parts(&arg_str(a, 0));
                ok(w.settings.get(&section, &key).map(from_godot).or_else(|| a.get(1).cloned()).unwrap_or(Value::Nil))
            }
            ("ProjectSettings", "has_setting") => {
                let (section, key) = setting_parts(&arg_str(a, 0));
                ok(Value::Bool(w.settings.get(&section, &key).is_some()))
            }
            ("ProjectSettings", "get_order") => ok(Value::Int(0)),
            ("Engine", "is_editor_hint") => ok(Value::Bool(false)),
            ("Engine", "get_frames_drawn" | "get_process_frames" | "get_physics_frames") => ok(Value::Int((w.time * 60.0) as i64)),
            ("Engine", "get_frames_per_second") => ok(Value::Float(60.0)),
            ("Engine", "get_physics_ticks_per_second") => ok(Value::Int(60)),
            ("Engine", "get_time_scale") => ok(Value::Float(1.0)),
            ("Engine", "has_singleton") => ok(Value::Bool(false)),
            ("Engine", "get_version_info") => {
                let mut d = Dict::new();
                for (k, v) in [("major", 4), ("minor", 7), ("patch", 2)] {
                    d.set(s(k), Value::Int(v));
                }
                d.set(s("string"), s("4.7.2.stable (Neko)"));
                ok(Value::dict(d))
            }
            ("OS", "get_name") => ok(s(match std::env::consts::OS {
                "linux" => "Linux",
                "windows" => "Windows",
                "macos" => "macOS",
                "android" => "Android",
                "ios" => "iOS",
                other => other,
            })),
            ("OS", "has_feature") => ok(Value::Bool(matches!(arg_str(a, 0).as_str(), "pc" | "standalone" | "release" | "64"))),
            ("OS", "get_ticks_msec") | ("Time", "get_ticks_msec") => ok(Value::Int((w.time * 1000.0) as i64)),
            ("OS", "get_ticks_usec") | ("Time", "get_ticks_usec") => ok(Value::Int((w.time * 1e6) as i64)),
            ("OS", "get_processor_count") => ok(Value::Int(std::thread::available_parallelism().map(|n| n.get() as i64).unwrap_or(1))),
            ("OS", "get_cmdline_args") => ok(Value::array(std::env::args().skip(1).take_while(|x| x != "--").map(|x| s(&x)).collect())),
            // Godot: arguments after `--` are for the game.
            ("OS", "get_cmdline_user_args") => ok(Value::array(std::env::args().skip_while(|x| x != "--").skip(1).map(|x| s(&x)).collect())),
            ("OS", "get_user_data_dir") => ok(s(&dirs_user(&w.project_dir))),
            ("OS", "is_debug_build") => ok(Value::Bool(cfg!(debug_assertions))),
            ("OS", "get_locale" | "get_locale_language") => ok(s("en")),
            ("Time", "get_unix_time_from_system") => ok(Value::Float(now)),
            ("InputMap", "has_action") => ok(Value::Bool(w.input.actions.contains_key(&arg_str(a, 0)))),
            ("InputMap", "get_actions") => ok(Value::array(w.input.actions.keys().map(|k| Value::StringName(Rc::from(k.as_str()))).collect())),
            ("InputMap", "add_action" | "action_erase_events" | "action_set_deadzone" | "load_from_project_settings") => ok(Value::Nil),
            ("DisplayServer", "window_get_size" | "screen_get_size") => ok(Value::Vector2i(w.viewport_size[0] as i32, w.viewport_size[1] as i32)),
            ("DisplayServer", "get_name") => ok(s("neko")),
            ("DisplayServer", "window_set_mode" | "window_set_vsync_mode" | "window_set_title" | "window_set_size" | "screen_set_orientation") => ok(Value::Nil),
            ("DisplayServer", _) => ok(Value::Nil),
            ("AudioServer", "set_bus_volume_db" | "set_bus_mute" | "set_bus_solo" | "set_bus_bypass_effects") => ok(Value::Nil),
            ("AudioServer", "get_bus_volume_db" | "get_bus_peak_volume_left_db" | "get_bus_peak_volume_right_db") => ok(Value::Float(0.0)),
            ("AudioServer", "get_bus_index" | "get_bus_count") => ok(Value::Int(0)),
            ("ResourceLoader", "exists") => {
                let path = crate::scene_runner::resolve(&w.project_dir, &arg_str(a, 0));
                ok(Value::Bool(path.exists()))
            }
            ("RenderingServer", _) => ok(Value::Nil),
            ("JSON", "stringify") => {
                let json = value_to_json(a.first().unwrap_or(&Value::Nil));
                let indent = arg_str(a, 1);
                let text = if indent.is_empty() { serde_json::to_string(&json) } else { serde_json::to_string_pretty(&json) };
                ok(Value::str(text.as_deref().unwrap_or("null")))
            }
            ("JSON", "parse_string" | "parse") => {
                let text = arg_str(a, 0);
                match serde_json::from_str::<serde_json::Value>(&text) {
                    Ok(j) => ok(json_to_value(&j)),
                    Err(_) => ok(Value::Nil),
                }
            }
            ("JSON", _) => ok(Value::Nil),
            ("OS", "get_keycode_string") => ok(s(&keycode_string(a.first().and_then(Value::as_i64).unwrap_or(0)))),
            ("ProjectSettings", "globalize_path") => ok(s(&resolve_io(&w.project_dir, &arg_str(a, 0)).to_string_lossy())),
            _ => None,
        }
    }

    fn config_method(&mut self, obj: &Object, name: &str, a: &[Value]) -> Option<RResult<Value>> {
        let project = self.0.borrow().project_dir.clone();
        let mut w = self.0.borrow_mut();
        let Some(ResourceState::Config(cfg)) = w.resources.get_mut(&obj.id()) else { return None };
        match name {
            "load" => {
                let path = resolve_io(&project, &arg_str(a, 0));
                let parsed = std::fs::read_to_string(&path).ok().and_then(|t| ConfigFile::parse(&t).ok());
                match parsed {
                    Some(c) => {
                        *cfg = c;
                        ok(Value::Int(0))
                    }
                    None => ok(Value::Int(7)),
                }
            }
            "save" => {
                let path = resolve_io(&project, &arg_str(a, 0));
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                match std::fs::write(&path, cfg.write("")) {
                    Ok(()) => ok(Value::Int(0)),
                    Err(_) => ok(Value::Int(12)),
                }
            }
            "get_value" => {
                let (section, key) = (arg_str(a, 0), arg_str(a, 1));
                let value = cfg.get(&section, &key).map(from_godot).or_else(|| a.get(2).cloned()).unwrap_or(Value::Nil);
                ok(value)
            }
            "set_value" => {
                let value = to_godot(a.get(2).unwrap_or(&Value::Nil));
                cfg.set(&arg_str(a, 0), &arg_str(a, 1), value);
                ok(Value::Nil)
            }
            "has_section" => ok(Value::Bool(cfg.sections.iter().any(|(s, _)| *s == arg_str(a, 0)))),
            "has_section_key" => ok(Value::Bool(cfg.get(&arg_str(a, 0), &arg_str(a, 1)).is_some())),
            "get_sections" => ok(Value::array(cfg.sections.iter().map(|(name, _)| s(name)).collect())),
            "get_section_keys" => {
                let keys = cfg.sections.iter().find(|(s, _)| *s == arg_str(a, 0)).map(|(_, k)| k.iter().map(|(k, _)| s(k)).collect()).unwrap_or_default();
                ok(Value::array(keys))
            }
            "erase_section" => {
                cfg.sections.retain(|(s, _)| *s != arg_str(a, 0));
                ok(Value::Nil)
            }
            "erase_section_key" => {
                cfg.remove(&arg_str(a, 0), &arg_str(a, 1));
                ok(Value::Nil)
            }
            _ => None,
        }
    }

    fn dir_method(&mut self, obj: &Object, name: &str, a: &[Value]) -> Option<RResult<Value>> {
        let project = self.0.borrow().project_dir.clone();
        if matches!(obj.0.kind, ObjKind::NativeClass(_)) {
            return match name {
                "open" => {
                    let path = resolve_io(&project, &arg_str(a, 0));
                    let o = Object::new(ObjKind::Instance, "DirAccess", None);
                    o.0.vars.borrow_mut().insert("path".into(), s(&path.to_string_lossy()));
                    ok(Value::Object(o))
                }
                "get_files_at" => ok(dir_entries(&resolve_io(&project, &arg_str(a, 0)), false)),
                "get_directories_at" => ok(dir_entries(&resolve_io(&project, &arg_str(a, 0)), true)),
                "dir_exists_absolute" => ok(Value::Bool(resolve_io(&project, &arg_str(a, 0)).is_dir())),
                "remove_absolute" => {
                    let path = resolve_io(&project, &arg_str(a, 0));
                    let r = if path.is_dir() { std::fs::remove_dir_all(&path) } else { std::fs::remove_file(&path) };
                    ok(Value::Int(if r.is_ok() || !path.exists() { 0 } else { 7 }))
                }
                "make_dir_absolute" | "make_dir_recursive_absolute" => {
                    let path = resolve_io(&project, &arg_str(a, 0));
                    ok(Value::Int(if std::fs::create_dir_all(&path).is_ok() { 0 } else { 12 }))
                }
                "rename_absolute" => {
                    let from = resolve_io(&project, &arg_str(a, 0));
                    let to = resolve_io(&project, &arg_str(a, 1));
                    ok(Value::Int(if std::fs::rename(from, to).is_ok() { 0 } else { 7 }))
                }
                _ => None,
            };
        }
        let path = PathBuf::from(obj.0.vars.borrow().get("path").map(|v| v.to_string()).unwrap_or_default());
        match name {
            "get_files" => ok(dir_entries(&path, false)),
            "get_directories" => ok(dir_entries(&path, true)),
            "get_current_dir" | "get_current_dir_name" => ok(s(&path.to_string_lossy())),
            "change_dir" => {
                let next = if Path::new(&arg_str(a, 0)).is_absolute() { PathBuf::from(arg_str(a, 0)) } else { path.join(arg_str(a, 0)) };
                obj.0.vars.borrow_mut().insert("path".into(), s(&next.to_string_lossy()));
                ok(Value::Int(if next.is_dir() { 0 } else { 7 }))
            }
            "list_dir_begin" | "list_dir_end" => ok(Value::Int(0)),
            "get_open_error" => ok(Value::Int(0)),
            "remove" => {
                let full = path.join(arg_str(a, 0));
                let r = if full.is_dir() { std::fs::remove_dir_all(&full) } else { std::fs::remove_file(&full) };
                ok(Value::Int(if r.is_ok() || !full.exists() { 0 } else { 7 }))
            }
            _ => None,
        }
    }

    fn file_method(&mut self, obj: &Object, name: &str, a: &[Value]) -> Option<RResult<Value>> {
        let project = self.0.borrow().project_dir.clone();
        if matches!(obj.0.kind, ObjKind::NativeClass(_)) {
            return match name {
                "file_exists" => ok(Value::Bool(resolve_io(&project, &arg_str(a, 0)).is_file())),
                "get_open_error" => ok(Value::Int(0)),
                "open" | "open_compressed" => {
                    let path = resolve_io(&project, &arg_str(a, 0));
                    let mode = a.get(1).and_then(Value::as_i64).unwrap_or(1);
                    let write = mode & 2 != 0;
                    let data = if write { Vec::new() } else { std::fs::read(&path).unwrap_or_default() };
                    let o = Object::new(ObjKind::Instance, "FileAccess", None);
                    self.0.borrow_mut().resources.insert(o.id(), ResourceState::File { path, data, write, pos: 0 });
                    ok(Value::Object(o))
                }
                "get_as_text" => ok(s(&std::fs::read_to_string(resolve_io(&project, &arg_str(a, 0))).unwrap_or_default())),
                _ => None,
            };
        }
        let mut w = self.0.borrow_mut();
        let Some(ResourceState::File { path, data, write, pos }) = w.resources.get_mut(&obj.id()) else { return None };
        let result = match name {
            "get_buffer" => {
                let n = a.first().and_then(Value::as_i64).unwrap_or(0).max(0) as usize;
                let end = (*pos + n).min(data.len());
                let out: Vec<Value> = data[*pos..end].iter().map(|b| Value::Int(*b as i64)).collect();
                *pos = end;
                ok(Value::array(out))
            }
            "store_buffer" => {
                if let Some(Value::Array(bytes)) = a.first() {
                    data.extend(bytes.borrow().iter().filter_map(Value::as_i64).map(|b| b as u8));
                }
                ok(Value::Nil)
            }
            "store_pascal_string" => {
                let text = arg_str(a, 0);
                data.extend((text.len() as u32).to_le_bytes());
                data.extend(text.as_bytes());
                ok(Value::Nil)
            }
            "get_pascal_string" => {
                if *pos + 4 <= data.len() {
                    let n = u32::from_le_bytes(data[*pos..*pos + 4].try_into().unwrap()) as usize;
                    *pos += 4;
                    let end = (*pos + n).min(data.len());
                    let text = String::from_utf8_lossy(&data[*pos..end]).into_owned();
                    *pos = end;
                    ok(s(&text))
                } else {
                    ok(s(""))
                }
            }
            "store_32" => {
                data.extend((a.first().and_then(Value::as_i64).unwrap_or(0) as u32).to_le_bytes());
                ok(Value::Nil)
            }
            "store_64" => {
                data.extend((a.first().and_then(Value::as_i64).unwrap_or(0) as u64).to_le_bytes());
                ok(Value::Nil)
            }
            "get_32" => {
                let v = if *pos + 4 <= data.len() { let v = u32::from_le_bytes(data[*pos..*pos + 4].try_into().unwrap()) as i64; *pos += 4; v } else { 0 };
                ok(Value::Int(v))
            }
            "get_64" => {
                let v = if *pos + 8 <= data.len() { let v = u64::from_le_bytes(data[*pos..*pos + 8].try_into().unwrap()) as i64; *pos += 8; v } else { 0 };
                ok(Value::Int(v))
            }
            "store_string" | "store_line" => {
                data.extend(arg_str(a, 0).as_bytes());
                if name == "store_line" {
                    data.push(b'\n');
                }
                ok(Value::Nil)
            }
            "get_line" | "get_string" => {
                let end = data[*pos..].iter().position(|b| *b == b'\n').map(|i| *pos + i).unwrap_or(data.len());
                let text = String::from_utf8_lossy(&data[*pos..end]).into_owned();
                *pos = (end + 1).min(data.len());
                ok(s(&text))
            }
            "eof_reached" => ok(Value::Bool(*pos >= data.len())),
            "get_length" => ok(Value::Int(data.len() as i64)),
            "get_position" => ok(Value::Int(*pos as i64)),
            "seek" => {
                *pos = (a.first().and_then(Value::as_i64).unwrap_or(0).max(0) as usize).min(data.len());
                ok(Value::Nil)
            }
            "close" | "flush" => {
                if *write {
                    if let Some(dir) = path.parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    let _ = std::fs::write(&*path, &*data);
                }
                ok(Value::Nil)
            }
            "get_file_as_string" => ok(s(&String::from_utf8_lossy(data))),
            _ => None,
        };
        // Godot closes FileAccess when the reference goes away; scripts often
        // rely on that instead of calling close(), so persist writes now.
        if *write && matches!(name, "store_buffer" | "store_pascal_string" | "store_32" | "store_64" | "store_string" | "store_line") {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(&*path, &*data);
        }
        result
    }

    fn image_method(&mut self, obj: &Object, name: &str, a: &[Value]) -> Option<RResult<Value>> {
        let project = self.0.borrow().project_dir.clone();
        if matches!(obj.0.kind, ObjKind::NativeClass(_)) {
            if name != "create_empty" && name != "create" {
                return None;
            }
            let w = a.first().and_then(Value::as_i64).unwrap_or(1).max(1) as usize;
            let h = a.get(1).and_then(Value::as_i64).unwrap_or(1).max(1) as usize;
            let o = Object::new(ObjKind::Instance, "Image", None);
            self.0.borrow_mut().resources.insert(o.id(), ResourceState::Image { w, h, data: vec![0; w * h * 4] });
            return ok(Value::Object(o));
        }
        let mut w = self.0.borrow_mut();
        let Some(ResourceState::Image { w: iw, h: ih, data }) = w.resources.get_mut(&obj.id()) else { return None };
        match name {
            "get_width" => ok(Value::Int(*iw as i64)),
            "get_height" => ok(Value::Int(*ih as i64)),
            "get_size" => ok(Value::Vector2i(*iw as i32, *ih as i32)),
            "get_format" => ok(Value::Int(5)),
            "fill" => {
                let c = match a.first() { Some(Value::Color(r, g, b, al)) => [*r, *g, *b, *al], _ => [0.0; 4] };
                let px = [c[0], c[1], c[2], c[3]].map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
                for p in data.chunks_exact_mut(4) {
                    p.copy_from_slice(&px);
                }
                ok(Value::Nil)
            }
            "set_pixel" => {
                let x = a.first().and_then(Value::as_i64).unwrap_or(0);
                let y = a.get(1).and_then(Value::as_i64).unwrap_or(0);
                if let Some(Value::Color(r, g, b, al)) = a.get(2)
                    && x >= 0 && y >= 0 && (x as usize) < *iw && (y as usize) < *ih
                {
                    let i = (y as usize * *iw + x as usize) * 4;
                    data[i..i + 4].copy_from_slice(&[*r, *g, *b, *al].map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8));
                }
                ok(Value::Nil)
            }
            "get_pixel" => {
                let x = a.first().and_then(Value::as_i64).unwrap_or(0);
                let y = a.get(1).and_then(Value::as_i64).unwrap_or(0);
                if x >= 0 && y >= 0 && (x as usize) < *iw && (y as usize) < *ih {
                    let i = (y as usize * *iw + x as usize) * 4;
                    let f = |v: u8| v as f32 / 255.0;
                    ok(Value::Color(f(data[i]), f(data[i + 1]), f(data[i + 2]), f(data[i + 3])))
                } else {
                    ok(Value::Color(0.0, 0.0, 0.0, 0.0))
                }
            }
            "get_data" => ok(Value::array(data.iter().map(|b| Value::Int(*b as i64)).collect())),
            "save_png" => {
                let path = resolve_io(&project, &arg_str(a, 0));
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                let file = match std::fs::File::create(&path) {
                    Ok(f) => f,
                    Err(_) => return ok(Value::Int(12)),
                };
                let mut enc = png::Encoder::new(std::io::BufWriter::new(file), *iw as u32, *ih as u32);
                enc.set_color(png::ColorType::Rgba);
                enc.set_depth(png::BitDepth::Eight);
                let r = enc.write_header().and_then(|mut wr| wr.write_image_data(data)).map(|_| 0).unwrap_or(12);
                ok(Value::Int(r))
            }
            _ => None,
        }
    }

    fn image_texture_method(&mut self, obj: &Object, name: &str, a: &[Value]) -> Option<RResult<Value>> {
        if matches!(obj.0.kind, ObjKind::NativeClass(_)) {
            return match name {
                "create_from_image" => {
                    let Some(Value::Object(img)) = a.first() else { return ok(Value::Nil) };
                    let o = Object::new(ObjKind::Instance, "ImageTexture", None);
                    self.0.borrow_mut().resources.insert(o.id(), ResourceState::ImageTexture { image: img.id() });
                    ok(Value::Object(o))
                }
                _ => None,
            };
        }
        match name {
            "get_width" | "get_height" => {
                let id = match self.0.borrow().resources.get(&obj.id()) {
                    Some(ResourceState::ImageTexture { image }) => *image,
                    _ => return ok(Value::Int(0)),
                };
                let size = self.0.borrow().resources.get(&id).and_then(|r| match r {
                    ResourceState::Image { w, h, .. } => Some((*w, *h)),
                    _ => None,
                });
                ok(Value::Int(match (name, size) { ("get_width", Some((w, _))) => w as i64, ("get_height", Some((_, h))) => h as i64, _ => 0 }))
            }
            "get_image" => {
                let id = match self.0.borrow().resources.get(&obj.id()) {
                    Some(ResourceState::ImageTexture { image }) => *image,
                    _ => return ok(Value::Nil),
                };
                let copy = self.0.borrow().resources.get(&id).and_then(|r| match r {
                    ResourceState::Image { w, h, data } => Some((*w, *h, data.clone())),
                    _ => None,
                });
                let Some((w, h, data)) = copy else { return ok(Value::Nil) };
                let o = Object::new(ObjKind::Instance, "Image", None);
                self.0.borrow_mut().resources.insert(o.id(), ResourceState::Image { w, h, data });
                ok(Value::Object(o))
            }
            "update" | "set_image" => ok(Value::Nil),
            _ => None,
        }
    }

    fn classdb_method(&mut self, it: &mut Interp, name: &str, a: &[Value]) -> Option<RResult<Value>> {
        match name {
            "class_exists" => {
                let n = arg_str(a, 0);
                ok(Value::Bool(neko_core::ClassDb::get().class(&n).is_some() || is_engine_class(&n)))
            }
            "instantiate" => {
                let n = arg_str(a, 0);
                ok(self.instantiate(it, &n).map(Value::Object).unwrap_or(Value::Nil))
            }
            "class_get_parent" => {
                let n = arg_str(a, 0);
                let parent = neko_core::ClassDb::get().class(&n).and_then(|c| c.parent);
                ok(parent.map(|p| Value::StringName(Rc::from(p))).unwrap_or(Value::Nil))
            }
            "class_has_method" | "class_has_signal" | "class_has_property" => ok(Value::Bool(false)),
            _ => None,
        }
    }

    fn multiplayer_method(&mut self, name: &str) -> Option<RResult<Value>> {
        match name {
            "has_multiplayer_peer" => ok(Value::Bool(false)),
            "get_unique_id" => ok(Value::Int(1)),
            "get_peers" => ok(Value::array(Vec::new())),
            "get_peer" | "get_multiplayer_peer" => ok(Value::Nil),
            "is_server" => ok(Value::Bool(true)),
            "get_remote_sender_id" => ok(Value::Int(0)),
            "create_server" | "create_client" | "set_multiplayer_peer" | "poll" | "disconnect_peer" | "set_refuse_new_connections" => ok(Value::Nil),
            _ => None,
        }
    }

    fn tween_method(&mut self, obj: &Object, name: &str, a: &[Value]) -> Option<RResult<Value>> {
        let mut w = self.0.borrow_mut();
        let Some(t) = w.tweens.iter_mut().find(|t| t.obj.id() == obj.id()) else {
            return match name {
                "kill" => ok(Value::Nil),
                "is_running" => ok(Value::Bool(false)),
                "is_valid" => ok(Value::Bool(true)),
                _ => ok(Value::Object(obj.clone())),
            };
        };
        match name {
            "tween_property" => {
                let duration = a.get(3).and_then(Value::as_f64).unwrap_or(1.0);
                t.total += duration;
                ok(Value::Object(obj.clone()))
            }
            "tween_interval" => {
                let duration = a.first().and_then(Value::as_f64).unwrap_or(0.0);
                t.total += duration;
                ok(Value::Object(obj.clone()))
            }
            "tween_callback" => {
                if let Some(c) = a.first() {
                    t.callbacks.push((t.total, c.clone()));
                }
                ok(Value::Object(obj.clone()))
            }
            "tween_method" => {
                let duration = a.get(3).and_then(Value::as_f64).unwrap_or(1.0);
                t.total += duration;
                ok(Value::Object(obj.clone()))
            }
            "kill" => {
                t.done = true;
                ok(Value::Nil)
            }
            "is_running" => ok(Value::Bool(!t.done)),
            "is_valid" => ok(Value::Bool(!t.done)),
            _ => ok(Value::Object(obj.clone())),
        }
    }

    /// Resource and UI-style objects that scripts configure but Neko's
    /// renderer does not draw yet: accept the setters, keep the values.
    /// `.tres`: a script-backed resource with its `[resource]` properties.
    /// `load()` returns an object whose script (if any) is attached by the
    /// interpreter after this host call (see `Interp::flush_pending`).
    fn load_tres(&mut self, it: &mut Interp, path: &str, file: &Path) -> Option<Value> {
        let text = std::fs::read_to_string(file).ok()?;
        let doc = neko_core::godot::text::Document::parse(&text).ok()?;
        let head = doc.sections.iter().find(|s| s.tag == "gd_resource")?;
        let class = head.field_str("script_class").or_else(|| head.field_str("type")).unwrap_or("Resource").to_string();
        let ext: HashMap<String, String> = doc
            .sections
            .iter()
            .filter(|s| s.tag == "ext_resource")
            .filter_map(|s| Some((s.field_str("id")?.to_string(), s.field_str("path")?.to_string())))
            .collect();
        let sub: HashMap<String, Vec<(String, GValue)>> = doc
            .sections
            .iter()
            .filter(|s| s.tag == "sub_resource")
            .filter_map(|s| Some((s.field_str("id")?.to_string(), s.props.clone())))
            .collect();
        let res = doc.sections.iter().find(|s| s.tag == "resource")?;
        let o = Object::new(ObjKind::Instance, &class, None);
        self.0.borrow_mut().resource_cache.insert(path.to_string(), o.clone());
        let mut vars = Vec::new();
        let mut script = None;
        for (k, v) in &res.props {
            if k == "script" {
                script = v.resource_ref().and_then(|(_, id)| ext.get(id).cloned());
                continue;
            }
            vars.push((k.clone(), self.resource_value(it, v, &ext, &sub)));
        }
        {
            let mut w = self.0.borrow_mut();
            let mut objvars = o.0.vars.borrow_mut();
            for (k, v) in vars {
                objvars.insert(k, v);
            }
            drop(objvars);
            if let Some(path) = script {
                w.pending_scripts.push(neko_script::interp::PendingScript { obj: o.clone(), path });
            }
        }
        Some(Value::Object(o))
    }

    fn resource_value(&mut self, it: &mut Interp, v: &GValue, ext: &HashMap<String, String>, sub: &HashMap<String, Vec<(String, GValue)>>) -> Value {
        match v {
            GValue::Call(n, args) if n == "ExtResource" => {
                let path = args.first().and_then(GValue::as_str).and_then(|id| ext.get(id)).cloned();
                match path {
                    Some(p) => self.load(it, &p).unwrap_or(Value::Nil),
                    None => Value::Nil,
                }
            }
            GValue::Call(n, args) if n == "SubResource" => {
                let mut d = Dict::new();
                if let Some(props) = args.first().and_then(GValue::as_str).and_then(|id| sub.get(id)) {
                    for (k, v) in props {
                        d.set(Value::str(k), self.resource_value(it, v, ext, sub));
                    }
                }
                Value::dict(d)
            }
            GValue::Call(n, args) if n.starts_with("Array") => match args.first() {
                Some(GValue::Array(items)) => Value::array(items.iter().map(|v| self.resource_value(it, v, ext, sub)).collect()),
                _ => Value::array(Vec::new()),
            },
            GValue::Array(items) => Value::array(items.iter().map(|v| self.resource_value(it, v, ext, sub)).collect()),
            GValue::Dictionary(pairs) => {
                let mut d = Dict::new();
                for (k, v) in pairs {
                    d.set(self.resource_value(it, k, ext, sub), self.resource_value(it, v, ext, sub));
                }
                Value::dict(d)
            }
            other => from_godot(other),
        }
    }

    fn rng_method(&mut self, obj: &Object, name: &str, a: &[Value]) -> Option<RResult<Value>> {
        let mut vars = obj.0.vars.borrow_mut();
        let mut state = vars.get("state").and_then(Value::as_i64).unwrap_or(0x2545_f491);
        let mut next = || {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407) & 0x7fff_ffff_ffff_ffff;
            (state >> 11) as f64 / (1u64 << 53) as f64
        };
        let r = match name {
            "randf" => Value::Float(next()),
            "randi" => Value::Int((next() * i32::MAX as f64) as i64),
            "randf_range" => {
                let lo = a.first().and_then(Value::as_f64).unwrap_or(0.0);
                let hi = a.get(1).and_then(Value::as_f64).unwrap_or(1.0);
                Value::Float(lo + next() * (hi - lo))
            }
            "randi_range" => {
                let lo = a.first().and_then(Value::as_i64).unwrap_or(0);
                let hi = a.get(1).and_then(Value::as_i64).unwrap_or(i32::MAX as i64);
                Value::Int(lo + (next() * (hi - lo + 1) as f64) as i64)
            }
            "randfn" => Value::Float(next() * 2.0 - 1.0),
            "seed" => {
                state = a.first().and_then(Value::as_i64).unwrap_or(0);
                Value::Nil
            }
            "get_seed" => Value::Int(state),
            "set_state" => Value::Nil,
            "get_state" => Value::Int(state),
            _ => return None,
        };
        vars.insert("state".into(), Value::Int(state));
        Some(Ok(r))
    }

    /// Network peers are accepted but do not connect yet.
    fn network_method(&mut self, name: &str, _a: &[Value]) -> Option<RResult<Value>> {
        match name {
            "bind" | "listen" | "connect_to_host" | "put_packet" | "put_var" | "put_data" | "poll" | "disconnect_from_host"
            | "set_dest_address" | "set_broadcast_enabled" | "close" => ok(Value::Nil),
            "is_bound" | "is_listening" | "is_server" | "is_connected_to_host" => ok(Value::Bool(false)),
            "get_available_packet_count" | "get_available_bytes" => ok(Value::Int(0)),
            "get_packet" | "get_var" | "get_data" => ok(Value::array(Vec::new())),
            "get_packet_ip" | "get_packet_address" | "get_connected_host" => ok(Value::str("")),
            "get_packet_port" | "get_local_port" | "get_status" => ok(Value::Int(0)),
            "get_write_buffer_size" | "get_read_available" => ok(Value::Int(0)),
            _ => ok(Value::Nil),
        }
    }

    fn resource_method(&mut self, obj: &Object, name: &str, a: &[Value]) -> Option<RResult<Value>> {
        let class = obj.0.native_class.as_str();
        if class == "FastNoiseLite" {
            let f = |i: usize| a.get(i).and_then(Value::as_f64).unwrap_or(0.0);
            return match name {
                // Deterministic smooth-ish value noise; enough for terrain previews.
                "get_noise_1d" => ok(Value::Float(noise(f(0), 0.0, 0.0))),
                "get_noise_2d" | "get_noise_2dv" => ok(Value::Float(noise(f(0), f(1), 0.0))),
                "get_noise_3d" | "get_noise_3dv" => ok(Value::Float(noise(f(0), f(1), f(2)))),
                "get_image" | "get_seamless_image" => ok(Value::Nil),
                _ => None,
            };
        }
        if class == "Gradient" {
            let key = "points".to_string();
            return match name {
                "set_color" => {
                    let mut vars = obj.0.vars.borrow_mut();
                    let mut points = match vars.get(&key) { Some(Value::Array(a)) => a.borrow().clone(), _ => Vec::new() };
                    let i = a.first().and_then(Value::as_i64).unwrap_or(0).max(0) as usize;
                    while points.len() <= i {
                        points.push(Value::Color(1.0, 1.0, 1.0, 1.0));
                    }
                    points[i] = a.get(1).cloned().unwrap_or(Value::Color(1.0, 1.0, 1.0, 1.0));
                    vars.insert(key, Value::array(points));
                    ok(Value::Nil)
                }
                "add_point" => {
                    let mut vars = obj.0.vars.borrow_mut();
                    let mut points = match vars.get(&key) { Some(Value::Array(a)) => a.borrow().clone(), _ => Vec::new() };
                    points.push(a.get(1).cloned().unwrap_or(Value::Color(1.0, 1.0, 1.0, 1.0)));
                    vars.insert(key, Value::array(points));
                    ok(Value::Nil)
                }
                "get_point_count" => ok(Value::Int(match obj.0.vars.borrow().get(&key) { Some(Value::Array(a)) => a.borrow().len() as i64, _ => 0 })),
                "get_color" => {
                    let i = a.first().and_then(Value::as_i64).unwrap_or(0).max(0) as usize;
                    ok(match obj.0.vars.borrow().get(&key) { Some(Value::Array(x)) => x.borrow().get(i).cloned().unwrap_or(Value::Color(1.0, 1.0, 1.0, 1.0)), _ => Value::Color(1.0, 1.0, 1.0, 1.0) })
                }
                "sample" | "interpolate" => ok(Value::Color(1.0, 1.0, 1.0, 1.0)),
                "remove_point" | "set_point" | "set_offset" => ok(Value::Nil),
                _ => None,
            };
        }
        if class == "MultiMesh" {
            return match name {
                "set_instance_transform" | "set_instance_color" | "set_instance_custom_data" | "set_instance_count"
                | "set_visible_instance_count" | "set_transform_format" | "set_mesh" => ok(Value::Nil),
                "get_instance_count" => ok(Value::Int(a.len() as i64)),
                "get_mesh" => ok(Value::Nil),
                _ => None,
            };
        }
        if class == "Font" || class == "FontFile" {
            return match name {
                "get_string_size" => {
                    let text = arg_str(a, 0);
                    let size = a.get(3).and_then(Value::as_f64).unwrap_or(16.0) as f32;
                    ok(Value::Vector2(text.chars().count() as f32 * size * 0.55, size))
                }
                "get_height" => ok(Value::Float(a.first().and_then(Value::as_f64).unwrap_or(16.0))),
                "get_ascent" | "get_descent" => ok(Value::Float(a.first().and_then(Value::as_f64).unwrap_or(16.0) * 0.8)),
                _ => None,
            };
        }
        if class == "SurfaceTool" {
            return match name {
                "begin" | "add_vertex" | "add_normal" | "add_uv" | "add_uv2" | "add_color" | "add_index" | "set_normal" | "set_uv" | "set_uv2"
                | "set_color" | "set_smooth_group" | "set_material" | "generate_normals" | "generate_tangents" | "index" | "deindex" | "clear" => ok(Value::Nil),
                "commit" => {
                    let mesh = Object::new(ObjKind::Instance, "ArrayMesh", None);
                    ok(Value::Object(mesh))
                }
                _ => None,
            };
        }
        if class.ends_with("Mesh") {
            return match name {
                "get_mesh_arrays" => {
                    let mut d = Dict::new();
                    let corners = [Value::Vector3(-0.5, -0.5, -0.5), Value::Vector3(0.5, -0.5, -0.5), Value::Vector3(0.5, 0.5, -0.5), Value::Vector3(-0.5, 0.5, -0.5), Value::Vector3(-0.5, -0.5, 0.5), Value::Vector3(0.5, -0.5, 0.5), Value::Vector3(0.5, 0.5, 0.5), Value::Vector3(-0.5, 0.5, 0.5)];
                    d.set(Value::Int(0), Value::array(Vec::from(corners)));
                    d.set(Value::Int(1), Value::array(vec![Value::Vector3(0.0, 1.0, 0.0); 8]));
                    d.set(Value::Int(4), Value::array(vec![Value::Vector2(0.0, 0.0); 8]));
                    d.set(Value::Int(8), Value::array(vec![Value::Color(1.0, 1.0, 1.0, 1.0); 8]));
                    d.set(Value::Int(12), Value::array(Vec::new()));
                    ok(Value::dict(d))
                }
                "get_aabb" => ok(Value::Rect2(0.0, 0.0, 1.0, 1.0)),
                "set_arrays" | "add_surface_from_arrays" | "surface_set_material" | "clear_surfaces" | "reload_from_file" => ok(Value::Nil),
                "get_surface_count" => ok(Value::Int(1)),
                _ => None,
            };
        }
        match name {
            "set_border_width_all" | "set_border_color_all" | "set_corner_radius_all" | "set_content_margin_all"
            | "set_expand_margin_all" | "set_shader_parameter" | "set_texture" | "set_meta" | "add_surface_from_arrays" => {
                let key = match name {
                    "set_shader_parameter" => format!("shader_parameter/{}", arg_str(a, 0)),
                    _ => name.trim_start_matches("set_").to_string(),
                };
                obj.0.vars.borrow_mut().insert(key, a.last().cloned().unwrap_or(Value::Nil));
                ok(Value::Nil)
            }
            "get_shader_parameter" => ok(obj.0.vars.borrow().get(&format!("shader_parameter/{}", arg_str(a, 0))).cloned().unwrap_or(Value::Nil)),
            "duplicate" => {
                let copy = Object::new(ObjKind::Instance, class, None);
                copy.0.vars.borrow_mut().extend(obj.0.vars.borrow().clone());
                ok(Value::Object(copy))
            }
            _ if name.starts_with("set_") || name.starts_with("add_") || name.starts_with("remove_") => ok(Value::Nil),
            _ => None,
        }
    }

    fn event_method(&mut self, obj: &Object, name: &str, a: &[Value]) -> Option<RResult<Value>> {
        let var = |n: &str| obj.0.vars.borrow().get(n).cloned().unwrap_or(Value::Nil);
        let code = var("keycode").as_i64().unwrap_or(-1);
        let pressed = var("pressed").truthy();
        let w = self.0.borrow();
        let in_action = |act: &str| w.input.action_codes(act).contains(&code);
        match name {
            "is_pressed" => ok(Value::Bool(pressed)),
            "is_released" => ok(Value::Bool(!pressed)),
            "is_echo" => ok(Value::Bool(var("echo").truthy())),
            "is_action" => ok(Value::Bool(in_action(&arg_str(a, 0)))),
            "is_action_pressed" => ok(Value::Bool(in_action(&arg_str(a, 0)) && pressed && (!var("echo").truthy() || a.get(1).is_some_and(Value::truthy)))),
            "is_action_released" => ok(Value::Bool(in_action(&arg_str(a, 0)) && !pressed)),
            "get_keycode" | "get_physical_keycode" => ok(Value::Int(code)),
            "as_text" | "as_text_keycode" => ok(s(&format!("{code}"))),
            _ => None,
        }
    }

    fn packed_scene_method(&mut self, it: &mut Interp, obj: &Object, name: &str) -> Option<RResult<Value>> {
        if name != "instantiate" && name != "instance" {
            return match name {
                "can_instantiate" => ok(Value::Bool(true)),
                _ => None,
            };
        }
        let path = obj.0.vars.borrow().get("resource_path").map(|v| v.to_string()).unwrap_or_default();
        let project = self.0.borrow().project_dir.clone();
        let scene = match crate::scene_runner::load_scene(&project, &path) {
            Ok(s) => s,
            Err(e) => return fail(format!("Cannot load scene {path}: {e}")),
        };
        let root_obj = Object::new(ObjKind::Instance, &scene.get(scene.root()).map(|n| n.godot.class.clone().unwrap_or_else(|| n.class.to_string())).unwrap_or_default(), None);
        root_obj.0.native.set(1 << 62);
        let key = root_obj.id();
        // Objects for every node, scripts instantiated (`_init` runs now, `_ready` on add_child).
        let mut objs = vec![(scene.root(), root_obj.clone())];
        for (id, _) in scene.walk().into_iter().skip(1) {
            let class = scene.get(id).map(|n| n.godot.class.clone().unwrap_or_else(|| n.class.to_string())).unwrap_or_default();
            let o = Object::new(ObjKind::Instance, &class, None);
            o.0.native.set(1 << 62);
            objs.push((id, o));
        }
        {
            let mut w = self.0.borrow_mut();
            for (id, o) in &objs {
                w.locs.insert(o.id(), Loc::Orphan(key, *id));
                w.orphan_objs.insert(o.id(), o.clone());
            }
        }
        let scripts: Vec<(Object, Option<String>)> = objs.iter().map(|(id, o)| (o.clone(), World::script_path(&scene, *id))).collect();
        let (scene_root, tail) = (scene.root(), scene.godot.tail.clone());
        {
            let mut w = self.0.borrow_mut();
            w.orphans.insert(key, scene);
            for (o, sp) in scripts {
                if let Some(path) = sp {
                    w.pending_scripts.push(neko_script::interp::PendingScript { obj: o, path });
                }
            }
            let root = Loc::Orphan(key, scene_root);
            for sec in tail.iter().filter(|s| s.tag == "connection") {
                let (Some(signal), Some(from), Some(to), Some(method)) = (sec.field_str("signal"), sec.field_str("from"), sec.field_str("to"), sec.field_str("method")) else { continue };
                let flags = sec.field("flags").and_then(|v| v.as_i64()).unwrap_or(2);
                let binds: Vec<Value> = match sec.field("binds") {
                    Some(GValue::Array(b)) => b.iter().map(from_godot).collect(),
                    _ => Vec::new(),
                };
                let src = w.resolve_path(root, from);
                let dst = w.resolve_path(root, to);
                if let (Some(src), Some(dst)) = (src, dst) {
                    if let (Some(src), Some(dst)) = (w.object_at(src), w.object_at(dst)) {
                        w.pending_connections.push(neko_script::interp::PendingConnection {
                            src,
                            signal: signal.to_string(),
                            dst,
                            method: method.to_string(),
                            binds,
                            flags,
                        });
                    }
                }
            }
        }
        let _ = it;
        ok(Value::Object(root_obj))
    }

    /// Move an orphan subtree under `parent` and queue `_ready`. The parent is
    /// either a node in the running tree or a node of another orphan scene
    /// (autoloads, `PackedScene.instantiate()` before `add_child`).
    fn add_child(&mut self, it: &mut Interp, parent: Loc, child: &Object) -> Option<RResult<Value>> {
        let _ = it;
        let mut w = self.0.borrow_mut();
        let Some(Loc::Orphan(key, _)) = w.loc(child) else { return fail("add_child(): the node already has a parent.") };
        let Some(sub) = w.orphans.remove(&key) else { return fail("add_child(): unknown node.") };
        let sub_root = sub.root();
        match parent {
            Loc::Tree(pid) => {
                let map = match w.scene.graft(pid, &sub) {
                    Ok(m) => m,
                    Err(e) => return fail(format!("add_child(): {e}")),
                };
                // Re-point every object of the subtree to its node in the tree.
                let moved: Vec<(u64, NodeId)> = w.locs.iter().filter_map(|(oid, l)| match l {
                    Loc::Orphan(k, old) if *k == key => map.iter().find(|(o, _)| o == old).map(|(_, new)| (*oid, *new)),
                    _ => None,
                }).collect();
                let mut new_objs = Vec::new();
                for (oid, new) in moved {
                    w.locs.insert(oid, Loc::Tree(new));
                    new_objs.push((oid, new));
                }
                // Keep the Object identities scripts already hold.
                let mut ready = Vec::new();
                for (oid, new) in &new_objs {
                    if let Some(o) = w.orphan_objs.remove(oid) {
                        w.objects.insert(*new, o.clone());
                        ready.push(o);
                    }
                }
                // `_ready` runs children first.
                ready.reverse();
                w.pending_ready.extend(ready);
            }
            Loc::Orphan(pkey, pid) => {
                if pkey == key {
                    return fail("add_child(): a node cannot be its own parent.");
                }
                let map = match w.orphans.get_mut(&pkey).and_then(|sc| sc.graft(pid, &sub).ok()) {
                    Some(m) => m,
                    None => return fail("add_child(): unknown parent node."),
                };
                let moved: Vec<(u64, NodeId)> = w.locs.iter().filter_map(|(oid, l)| match l {
                    Loc::Orphan(k, old) if *k == key => map.iter().find(|(o, _)| o == old).map(|(_, new)| (*oid, *new)),
                    _ => None,
                }).collect();
                let mut ready = None;
                for (oid, new) in moved {
                    w.locs.insert(oid, Loc::Orphan(pkey, new));
                    if map.iter().any(|(old, n)| *old == sub_root && *n == new) {
                        ready = w.orphan_objs.get(&oid).cloned();
                    }
                }
                if let Some(o) = ready {
                    w.pending_ready.push(o);
                }
            }
        }
        ok(Value::Nil)
    }
}

/// Scripts of a running scene: owns the interpreter and drives callbacks in
/// Godot's order.
pub struct Scripts {
    pub interp: Interp,
    pub world: Rc<RefCell<World>>,
    /// Scripted objects in tree order (for `_process`).
    errors: usize,
    physics_acc: f64,
}

impl Scripts {
    pub fn new(world: Rc<RefCell<World>>) -> Self {
        let mut interp = Interp::new();
        let dir = world.borrow().project_dir.clone();
        interp.source = Box::new(move |p| std::fs::read_to_string(crate::scene_runner::resolve(&dir, p)).ok());
        interp.host = Some(Box::new(ScriptHost(world.clone())));
        Self { interp, world, errors: 0, physics_acc: 0.0 }
    }

    fn report(&mut self, e: ScriptError) {
        self.errors += 1;
        if self.errors <= 50 {
            let msg = format!("SCRIPT ERROR: {e}");
            eprintln!("{msg}");
            self.world.borrow_mut().output.push(msg);
        }
    }

    /// Register `class_name` scripts and autoloads, like Godot's global class cache.
    pub fn load_globals(&mut self, project: &Path, settings: &neko_core::godot::config::ConfigFile) {
        // class_name scripts anywhere in the project. Compile bases first:
        // `class_name Cave extends Room` needs Room registered before Cave is
        // built, whatever order the filesystem returned the files in.
        let mut files = Vec::new();
        collect_gd(project, project, &mut files);
        let mut pending: Vec<(String, String, Option<String>, Option<String>)> = Vec::new();
        for (res, path) in files {
            if self.interp.scripts.contains_key(&res) {
                continue;
            }
            let Ok(src) = std::fs::read_to_string(&path) else { continue };
            let declared = declared_class_name(&src);
            if declared.is_none() {
                continue;
            }
            let base = extended_class_name(&src);
            pending.push((res, src, declared, base));
        }
        let by_name: HashMap<String, usize> = pending.iter().enumerate().filter_map(|(i, (_, _, n, _))| n.clone().map(|n| (n, i))).collect();
        let mut done = vec![false; pending.len()];
        let mut order = Vec::new();
        fn visit(i: usize, pending: &[(String, String, Option<String>, Option<String>)], by_name: &HashMap<String, usize>, done: &mut [bool], order: &mut Vec<usize>) {
            if done[i] {
                return;
            }
            done[i] = true;
            if let Some(base) = &pending[i].3
                && let Some(&j) = by_name.get(base)
                && j != i
            {
                visit(j, pending, by_name, done, order);
            }
            order.push(i);
        }
        for i in 0..pending.len() {
            visit(i, &pending, &by_name, &mut done, &mut order);
        }
        for i in order {
            let (res, src, _, _) = &pending[i];
            if let Err(e) = self.interp.compile(res, src) {
                self.report(e);
            }
        }
        // Autoloads: `Name="*res://path.gd"` (a `*` means singleton).
        if let Some((_, keys)) = settings.sections.iter().find(|(s, _)| s == "autoload") {
            for (name, v) in keys.clone() {
                let Some(p) = v.as_str() else { continue };
                let path = p.trim_start_matches('*').to_string();
                if path.ends_with(".gd") {
                    match self.interp.load_script(&path).and_then(|c| {
                        let o = self.instantiate_detached(&c)?;
                        Ok(o)
                    }) {
                        Ok(o) => {
                            self.interp.globals.insert(name.clone(), Value::Object(o.clone()));
                            self.world.borrow_mut().pending_ready.push(o);
                        }
                        Err(e) => self.report(e),
                    }
                }
            }
        }
    }

    fn instantiate_detached(&mut self, c: &Rc<neko_script::ScriptClass>) -> RResult<Object> {
        let nb = c.native_base();
        let o = Object::new(ObjKind::Instance, &nb, None);
        // Autoloads live in their own small scene (Godot puts them under /root);
        // keeping them out of the running scene lets a scene change keep them.
        let class = if ClassDb::get().class(&nb).is_some() { nb.as_str() } else { "Node" };
        if let Ok(scene) = Scene::new(class, c.name.as_deref().unwrap_or("Autoload")) {
            let root = scene.root();
            o.0.native.set(1 << 62);
            let mut w = self.world.borrow_mut();
            w.orphans.insert(o.id(), scene);
            w.locs.insert(o.id(), Loc::Orphan(o.id(), root));
            w.orphan_objs.insert(o.id(), o.clone());
        }
        let o = self.interp.instantiate(c, Some(o))?;
        if c.find_func("_init").is_some() {
            self.interp.call_method(&o, "_init", &[])?;
        }
        Ok(o)
    }

    /// Attach scripts to the scene's nodes, then run `_init` (top-down) and
    /// `_ready` (children first), with `@onready` assigned just before.
    pub fn start(&mut self) {
        let ids: Vec<NodeId> = self.world.borrow().scene.walk().into_iter().map(|(id, _)| id).collect();
        let mut scripted = Vec::new();
        for id in &ids {
            let path = World::script_path(&self.world.borrow().scene, *id);
            let obj = self.world.borrow_mut().object(*id);
            if let Some(p) = path {
                match self.interp.load_script(&p).and_then(|c| self.interp.instantiate(&c, Some(obj.clone()))) {
                    Ok(o) => {
                        if o.script().is_some_and(|c| c.find_func("_init").is_some())
                            && let Err(e) = self.interp.call_method(&o, "_init", &[])
                        {
                            self.report(e);
                        }
                        scripted.push(o);
                    }
                    Err(e) => self.report(e),
                }
            }
        }
        // Signals connected in the editor ([connection] sections).
        let (root, tail) = {
            let w = self.world.borrow();
            (w.scene.root(), w.scene.godot.tail.clone())
        };
        if let Err(e) = connect_scene(&mut self.interp, &self.world, Loc::Tree(root), &tail) {
            self.report(e);
        }
        // Timer nodes with autostart.
        {
            let mut w = self.world.borrow_mut();
            for id in &ids {
                let auto = w.scene.get(*id).filter(|n| n.class == "Timer").and_then(|n| Some((n.get("autostart")?.as_bool()?, n.get("wait_time")?.as_f64()?)));
                if let Some((true, wait)) = auto {
                    w.node_timers.insert(*id, NodeTimer { left: wait, running: true });
                }
            }
        }
        // Autoloads are ready first (they are earlier children of /root).
        let mut ready: Vec<Object> = std::mem::take(&mut self.world.borrow_mut().pending_ready);
        ready.extend(scripted.into_iter().rev());
        for o in ready {
            self.ready(&o);
        }
        self.flush();
    }

    /// `change_scene_to_file`: replace the running scene; autoloads stay.
    pub fn change_scene(&mut self, scene: Scene) {
        {
            let mut w = self.world.borrow_mut();
            for o in w.objects.values() {
                o.0.freed.set(true);
            }
            w.objects.clear();
            w.locs.retain(|_, l| matches!(l, Loc::Orphan(..)));
            w.groups.clear();
            w.timers.clear();
            w.no_process.clear();
            w.scene = scene;
        }
        self.start();
    }

    fn ready(&mut self, o: &Object) {
        if let Err(e) = self.interp.assign_onready(o) {
            self.report(e);
        }
        if o.script().is_some_and(|c| c.find_func("_ready").is_some())
            && let Err(e) = self.interp.call_method(o, "_ready", &[])
        {
            self.report(e);
        }
    }

    /// One frame: timers, `_physics_process` at 60 Hz, `_process`, frees.
    pub fn frame(&mut self, delta: f64) {
        self.world.borrow_mut().time += delta;
        self.world.borrow_mut().input.begin_frame();
        // Timers.
        let fired: Vec<Object> = {
            let mut w = self.world.borrow_mut();
            let mut fired = Vec::new();
            for t in &mut w.timers {
                t.left -= delta;
                if t.left <= 0.0 {
                    fired.push(t.obj.clone());
                }
            }
            w.timers.retain(|t| t.left > 0.0);
            fired
        };
        for t in fired {
            if let Err(e) = self.interp.emit(&t, "timeout", &[]) {
                self.report(e);
            }
        }
        // Timer nodes: emit timeout, restart unless one_shot.
        let fired_nodes: Vec<Object> = {
            let mut w = self.world.borrow_mut();
            let mut out = Vec::new();
            let ids: Vec<NodeId> = w.node_timers.keys().copied().collect();
            for id in ids {
                let (one_shot, wait) = match w.scene.get(id) {
                    Some(n) => (n.get("one_shot").and_then(|v| v.as_bool()).unwrap_or(false), n.get("wait_time").and_then(|v| v.as_f64()).unwrap_or(1.0)),
                    None => {
                        w.node_timers.remove(&id);
                        continue;
                    }
                };
                let t = w.node_timers.get_mut(&id).expect("listed");
                if !t.running {
                    continue;
                }
                t.left -= delta;
                if t.left <= 0.0 {
                    if one_shot {
                        t.running = false;
                    } else {
                        t.left += wait.max(0.001);
                    }
                    out.push(w.object(id));
                }
            }
            out
        };
        for o in fired_nodes {
            if let Err(e) = self.interp.emit(&o, "timeout", &[]) {
                self.report(e);
            }
            self.callback(&o, "_on_timeout_internal", &[]);
        }
        // Tweens: durations drive `await tw.finished`; callbacks fire in order.
        let (callbacks, finished) = {
            let mut w = self.world.borrow_mut();
            let mut callbacks = Vec::new();
            let mut finished = Vec::new();
            for t in &mut w.tweens {
                if t.done {
                    continue;
                }
                t.elapsed += delta;
                while t.callbacks.first().is_some_and(|(at, _)| *at <= t.elapsed) {
                    let (_, cb) = t.callbacks.remove(0);
                    callbacks.push(cb);
                }
                if t.elapsed >= t.total {
                    t.done = true;
                    finished.push(t.obj.clone());
                }
            }
            w.tweens.retain(|t| !t.done);
            (callbacks, finished)
        };
        for cb in callbacks {
            if let Value::Callable(c) = cb
                && let Err(e) = self.interp.call_callable(&c, &[])
            {
                self.report(e);
            }
        }
        for o in finished {
            if let Err(e) = self.interp.emit(&o, "finished", &[]) {
                self.report(e);
            }
        }
        let objs = self.scripted_in_tree();
        self.physics_acc += delta;
        let mut steps = 0;
        while self.physics_acc >= 1.0 / 60.0 && steps < 8 {
            self.physics_acc -= 1.0 / 60.0;
            steps += 1;
            self.world.borrow_mut().move_delta = 1.0 / 60.0;
            for o in &objs {
                self.callback(o, "_physics_process", &[Value::Float(1.0 / 60.0)]);
            }
            // Areas see the final positions of this physics step.
            self.physics_update();
        }
        self.world.borrow_mut().move_delta = delta;
        for o in &objs {
            self.callback(o, "_process", &[Value::Float(delta)]);
        }
        self.flush();
        // Resume coroutines whose await finished this frame (timer, tween,
        // signal or the next-frame awaits).
        self.interp.run_tasks();
        self.world.borrow_mut().input.end_frame();
    }

    /// Area2D.body_entered / body_exited against the physics bodies in the
    /// scene, after `_physics_process` moved them.
    fn physics_update(&mut self) {
        let (areas, bodies) = {
            let w = self.world.borrow();
            (collect_areas(&w), collect_collision_bodies(&w))
        };
        let mut next: HashMap<NodeId, HashSet<NodeId>> = HashMap::new();
        for (area, _layer, mask, aabb) in &areas {
            let mut set = HashSet::new();
            for (body, layer, body_aabb) in &bodies {
                if mask & layer != 0 && aabb.overlaps(body_aabb) {
                    set.insert(*body);
                }
            }
            next.insert(*area, set);
        }
        let mut enter = Vec::new();
        let mut exit = Vec::new();
        {
            let w = self.world.borrow();
            for (area, set) in &next {
                let old = w.area_overlaps.get(area);
                for body in set {
                    if old.is_none_or(|o| !o.contains(body))
                        && let (Some(a), Some(b)) = (w.objects.get(area), w.objects.get(body))
                    {
                        enter.push((a.clone(), b.clone()));
                    }
                }
            }
            for (area, set) in &w.area_overlaps {
                let now = next.get(area);
                for body in set {
                    if now.is_none_or(|n| !n.contains(body))
                        && let (Some(a), Some(b)) = (w.objects.get(area), w.objects.get(body))
                    {
                        exit.push((a.clone(), b.clone()));
                    }
                }
            }
        }
        self.world.borrow_mut().area_overlaps = next;
        for (area, body) in enter {
            if area.is_valid() && body.is_valid()
                && let Err(e) = self.interp.emit(&area, "body_entered", &[Value::Object(body)])
            {
                self.report(e);
            }
        }
        for (area, body) in exit {
            if area.is_valid() && body.is_valid()
                && let Err(e) = self.interp.emit(&area, "body_exited", &[Value::Object(body)])
            {
                self.report(e);
            }
        }
    }

    fn scripted_in_tree(&mut self) -> Vec<Object> {
        let w = self.world.borrow();
        w.scene.walk().into_iter().filter_map(|(id, _)| w.objects.get(&id)).filter(|o| o.script().is_some() && o.is_valid() && !w.no_process.contains(&o.id())).cloned().collect()
    }

    fn callback(&mut self, o: &Object, name: &str, args: &[Value]) {
        if !o.is_valid() || !o.script().is_some_and(|c| c.find_func(name).is_some()) {
            return;
        }
        if let Err(e) = self.interp.call_method(o, name, args) {
            self.report(e);
        }
    }

    /// `_input(event)` for key presses, like Godot's input propagation.
    pub fn key_event(&mut self, code: i64, pressed: bool, echo: bool) {
        {
            let mut w = self.world.borrow_mut();
            if pressed && !echo {
                w.input.keys.insert(code);
                w.input.just_pressed.insert(code);
            } else if !pressed {
                w.input.keys.remove(&code);
                w.input.just_released.insert(code);
            }
        }
        let ev = Object::new(ObjKind::Instance, "InputEventKey", None);
        ev.0.native.set(1 << 58);
        {
            let mut v = ev.0.vars.borrow_mut();
            v.insert("keycode".into(), Value::Int(code));
            v.insert("physical_keycode".into(), Value::Int(code));
            v.insert("pressed".into(), Value::Bool(pressed));
            v.insert("echo".into(), Value::Bool(echo));
        }
        // Godot delivers _input in reverse tree order (deepest/last first).
        let objs: Vec<Object> = self.scripted_in_tree().into_iter().rev().collect();
        for o in &objs {
            self.callback(o, "_input", &[Value::Object(ev.clone())]);
        }
        for o in &objs {
            self.callback(o, "_unhandled_input", &[Value::Object(ev.clone())]);
            self.callback(o, "_unhandled_key_input", &[Value::Object(ev.clone())]);
        }
        self.flush();
    }

    /// Apply queue_free and run `_ready` for nodes added this frame.
    fn flush(&mut self) {
        loop {
            let ready: Vec<Object> = std::mem::take(&mut self.world.borrow_mut().pending_ready);
            if ready.is_empty() {
                break;
            }
            for o in ready {
                self.ready(&o);
            }
        }
        let frees: Vec<Object> = std::mem::take(&mut self.world.borrow_mut().to_free);
        for o in frees {
            if !o.is_valid() {
                continue;
            }
            let id = {
                let w = self.world.borrow();
                match w.loc(&o) {
                    Some(Loc::Tree(id)) => Some(id),
                    _ => None,
                }
            };
            if let Some(id) = id {
                self.callback(&o, "_exit_tree", &[]);
                let mut w = self.world.borrow_mut();
                // Every object in the removed subtree becomes invalid.
                let doomed: Vec<NodeId> = {
                    let mut v = Vec::new();
                    let mut stack = vec![id];
                    while let Some(n) = stack.pop() {
                        v.push(n);
                        if let Some(node) = w.scene.get(n) {
                            stack.extend(node.children());
                        }
                    }
                    v
                };
                let _ = w.scene.remove(id);
                for n in doomed {
                    if let Some(ob) = w.objects.remove(&n) {
                        ob.0.freed.set(true);
                        w.locs.remove(&ob.id());
                    }
                }
            }
        }
    }
}

/// Make the `[connection]` sections of a scene, like `PackedScene::instantiate`
/// does: `from`/`to` are paths relative to the scene root at `root`.
fn connect_scene(it: &mut Interp, world: &Rc<RefCell<World>>, root: Loc, sections: &[neko_core::godot::text::Section]) -> RResult<()> {
    for sec in sections.iter().filter(|s| s.tag == "connection") {
        let (Some(signal), Some(from), Some(to), Some(method)) = (sec.field_str("signal"), sec.field_str("from"), sec.field_str("to"), sec.field_str("method")) else { continue };
        let flags = sec.field("flags").and_then(|v| v.as_i64()).unwrap_or(2);
        let binds: Vec<Value> = match sec.field("binds") {
            Some(GValue::Array(b)) => b.iter().map(from_godot).collect(),
            _ => Vec::new(),
        };
        let (src, dst) = {
            let mut w = world.borrow_mut();
            let src = w.resolve_path(root, from).and_then(|l| w.object_at(l));
            let dst = w.resolve_path(root, to).and_then(|l| w.object_at(l));
            (src, dst)
        };
        let (Some(src), Some(dst)) = (src, dst) else {
            return Err(ScriptError { message: format!("Cannot connect signal '{signal}' from \"{from}\" to \"{to}\": node not found."), file: String::new(), line: 0, func: String::new() });
        };
        let c = neko_script::interp::Callable::Method { obj: dst, name: Rc::from(method), bound: binds };
        it.connect(&src, signal, Rc::new(c), flags);
    }
    Ok(())
}

fn declared_class_name(src: &str) -> Option<String> {
    src.lines().find_map(|l| {
        let l = l.trim_start();
        let rest = l.strip_prefix("class_name ")?;
        let name: String = rest.trim_start().chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        (!name.is_empty()).then_some(name)
    })
}

fn extended_class_name(src: &str) -> Option<String> {
    src.lines().find_map(|l| {
        let l = l.trim_start();
        let rest = l.strip_prefix("extends ")?.trim();
        let first = rest.split('.').next().unwrap_or(rest).trim();
        (!first.is_empty() && !first.starts_with('"') && first.chars().all(|c| c.is_alphanumeric() || c == '_')).then(|| first.to_string())
    })
}

fn collect_gd(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name == "target" || name == "addons" {
            continue;
        }
        if p.is_dir() {
            collect_gd(root, &p, out);
        } else if p.extension().is_some_and(|x| x == "gd")
            && let Ok(rel) = p.strip_prefix(root)
        {
            out.push((format!("res://{}", rel.to_string_lossy().replace('\\', "/")), p));
        }
    }
}

/// Godot's `find_child` pattern syntax (`*` and `?`).
fn wildcard(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    fn go(p: &[char], t: &[char]) -> bool {
        match p.first() {
            None => t.is_empty(),
            Some('*') => go(&p[1..], t) || (!t.is_empty() && go(p, &t[1..])),
            Some('?') => !t.is_empty() && go(&p[1..], &t[1..]),
            Some(c) => t.first() == Some(c) && go(&p[1..], &t[1..]),
        }
    }
    go(&p, &t)
}

/// Cheap deterministic 3D value noise in -1..1.
fn noise(x: f64, y: f64, z: f64) -> f64 {
    let h = |a: f64, b: f64, c: f64| {
        let n = (a * 127.1 + b * 311.7 + c * 74.7).sin() * 43758.5453;
        (n - n.floor()) * 2.0 - 1.0
    };
    let (xi, yi, zi) = (x.floor(), y.floor(), z.floor());
    let (fx, fy, fz) = (x - xi, y - yi, z - zi);
    let s = |t: f64| t * t * (3.0 - 2.0 * t);
    let lerp = |a: f64, b: f64, t: f64| a + (b - a) * t;
    let (fx, fy, fz) = (s(fx), s(fy), s(fz));
    let c = |dx, dy, dz| h(xi + dx, yi + dy, zi + dz);
    let x00 = lerp(c(0.0, 0.0, 0.0), c(1.0, 0.0, 0.0), fx);
    let x10 = lerp(c(0.0, 1.0, 0.0), c(1.0, 1.0, 0.0), fx);
    let x01 = lerp(c(0.0, 0.0, 1.0), c(1.0, 0.0, 1.0), fx);
    let x11 = lerp(c(0.0, 1.0, 1.0), c(1.0, 1.0, 1.0), fx);
    lerp(lerp(x00, x10, fy), lerp(x01, x11, fy), fz)
}

/// `"display/window/size/viewport_width"` -> (`"display"`, `"window/size/viewport_width"`).
fn setting_parts(key: &str) -> (String, String) {
    match key.split_once('/') {
        Some((section, rest)) => (section.to_string(), rest.to_string()),
        None => (String::new(), key.to_string()),
    }
}

/// Human name of a Godot key code, for `OS.get_keycode_string`.
fn keycode_string(code: i64) -> String {
    const SPECIAL: i64 = 1 << 22;
    if code & SPECIAL != 0 {
        return match code & !SPECIAL {
            0x01 => "Escape", 0x02 => "Tab", 0x04 => "Backspace", 0x05 => "Enter", 0x07 => "Insert",
            0x08 => "Delete", 0x0D => "Home", 0x0E => "End", 0x0F => "Left", 0x10 => "Up",
            0x11 => "Right", 0x12 => "Down", 0x13 => "Page Up", 0x14 => "Page Down",
            k if (0x1B..=0x34).contains(&k) => {
                return format!("F{}", k - 0x1B + 1);
            }
            _ => "",
        }
        .to_string();
    }
    match code {
        0x20 => "Space".into(),
        0x1000000 | 0x2000000 => String::new(),
        c if c > 0 => char::from_u32(c as u32).map(|c| c.to_ascii_uppercase().to_string()).unwrap_or_default(),
        _ => String::new(),
    }
}

/// `res://` -> project folder, `user://` -> the project's user data folder.
fn resolve_io(project: &Path, path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("user://") {
        return PathBuf::from(dirs_user(project)).join(rest);
    }
    if Path::new(path).is_absolute() {
        return PathBuf::from(path);
    }
    crate::scene_runner::resolve(project, path)
}

/// `DirAccess.get_files()` / `get_directories()`.
fn dir_entries(path: &Path, only_dirs: bool) -> Value {
    let mut names: Vec<String> = std::fs::read_dir(path)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()) == only_dirs)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    Value::array(names.into_iter().map(|n| s(&n)).collect())
}

/// Engine singletons whose properties scripts may read/write.
fn value_to_json(v: &Value) -> serde_json::Value {
    use serde_json::Value as J;
    match v {
        Value::Nil => J::Null,
        Value::Bool(b) => J::Bool(*b),
        Value::Int(i) => J::from(*i),
        Value::Float(f) => serde_json::Number::from_f64(*f).map(J::Number).unwrap_or(J::Null),
        Value::String(s) | Value::StringName(s) | Value::NodePath(s) => J::String(s.to_string()),
        Value::Array(a) => J::Array(a.borrow().iter().map(value_to_json).collect()),
        Value::Dictionary(d) => {
            let mut m = serde_json::Map::new();
            for (k, x) in d.borrow().iter() {
                m.insert(k.to_string(), value_to_json(x));
            }
            J::Object(m)
        }
        Value::Vector2(x, y) => J::Array(vec![J::from(*x as f64), J::from(*y as f64)]),
        Value::Vector2i(x, y) => J::Array(vec![J::from(*x), J::from(*y)]),
        Value::Vector3(x, y, z) => J::Array(vec![J::from(*x as f64), J::from(*y as f64), J::from(*z as f64)]),
        Value::Color(r, g, b, a) => J::Array(vec![J::from(*r as f64), J::from(*g as f64), J::from(*b as f64), J::from(*a as f64)]),
        _ => J::Null,
    }
}

fn json_to_value(j: &serde_json::Value) -> Value {
    use serde_json::Value as J;
    match j {
        J::Null => Value::Nil,
        J::Bool(b) => Value::Bool(*b),
        J::Number(n) => n.as_i64().map(Value::Int).unwrap_or_else(|| Value::Float(n.as_f64().unwrap_or(0.0))),
        J::String(s) => Value::str(s),
        J::Array(a) => Value::array(a.iter().map(json_to_value).collect()),
        J::Object(m) => {
            let mut d = Dict::new();
            for (k, v) in m {
                d.set(Value::str(k), json_to_value(v));
            }
            Value::dict(d)
        }
    }
}

fn is_singleton_class(name: &str) -> bool {
    matches!(name, "Engine" | "OS" | "Time" | "MultiplayerAPI" | "Tween" | "AudioServer" | "DisplayServer" | "SceneTree" | "Viewport" | "ViewportTexture" | "Window")
}

/// Engine classes Neko exposes to scripts without a scene node.
fn is_engine_class(name: &str) -> bool {
    matches!(
        name,
        "ConfigFile"
            | "DirAccess"
            | "FileAccess"
            | "Image"
            | "ImageTexture"
            | "InputEvent"
            | "InputEventKey"
            | "InputEventMouseButton"
            | "InputEventMouseMotion"
            | "InputEventJoypadButton"
            | "InputEventJoypadMotion"
            | "InputMap"
            | "ProjectSettings"
            | "ClassDB"
            | "ResourceLoader"
            | "AudioServer"
            | "DisplayServer"
            | "PacketPeerUDP"
            | "UDPServer"
            | "StreamPeerTCP"
            | "WebSocketPeer"
            | "RandomNumberGenerator"
            | "FastNoiseLite"
            | "NoiseTexture2D"
            | "Engine"
            | "OS"
            | "Time"
            | "Tween"
            | "Environment"
            | "Sky"
            | "ProceduralSkyMaterial"
            | "ShaderMaterial"
            | "StandardMaterial3D"
            | "SurfaceTool"
            | "ArrayMesh"
            | "BoxMesh"
            | "SphereMesh"
            | "CapsuleMesh"
            | "CylinderMesh"
            | "PlaneMesh"
            | "Resource"
            | "Texture2D"
            | "Theme"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_map_actions_and_axes() {
        let mut input = InputState::default();
        input.actions.insert("jump".into(), vec![32]);
        input.keys.insert(32);
        assert!(input.action_pressed("jump"));
        input.keys.clear();
        input.axis_actions.insert("right".into(), vec![(0, 1.0)]);
        input.axis_actions.insert("left".into(), vec![(0, -1.0)]);
        input.joy_axes.insert(0, 0.75);
        assert!((input.action_strength("right") - 0.75).abs() < 1e-6);
        assert_eq!(input.action_strength("left"), 0.0);
        input.pressed_actions.insert("fire".into());
        assert!(input.action_pressed("fire"));
        input.just_pressed_actions.insert("dash".into());
        assert!(input.action_just("dash", true));
        input.end_frame();
        assert!(!input.action_just("dash", true));
    }

    #[test]
    fn tres_resources_load_their_script() {
        let dir = std::env::temp_dir().join(format!("neko-tres-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("mode_data.gd"), "extends Resource\nvar id := \"\"\nfunc is_gun_game() -> bool:\n\treturn id == \"gun_game\"\n").unwrap();
        std::fs::write(dir.join("gun.tres"), r#"[gd_resource type="Resource" script_class="ModeData" format=3]

[ext_resource type="Script" path="res://mode_data.gd" id="1_script"]

[resource]
script = ExtResource("1_script")
id = "gun_game"
"#).unwrap();
        let world = Rc::new(RefCell::new(World::new(Scene::new("Node", "Root").unwrap(), dir.clone())));
        let mut scripts = Scripts::new(world);
        let value = scripts.interp.load_resource("res://gun.tres").unwrap();
        let Value::Object(o) = value else { panic!("not an object") };
        assert!(matches!(scripts.interp.call_method(&o, "is_gun_game", &[]).unwrap(), Value::Bool(true)));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// `user://` for a project (Godot: ~/.local/share/godot/app_userdata/<name>).
fn dirs_user(project: &Path) -> String {
    let name = project.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "game".into());
    let base = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share"))).unwrap_or_else(|| PathBuf::from("."));
    base.join("neko/app_userdata").join(name).to_string_lossy().into_owned()
}
