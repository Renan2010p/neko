//! The class database: every node type Neko knows about.
//!
//! Classes form a single-inheritance tree rooted at `Node`. A node has the
//! properties of its class and of every ancestor class.


#[allow(unused_imports)]
use neko_base::prelude::*;

use crate::variant::{PropType, Variant};

/// Broad group, used for icons, colors and filtering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Node,
    TwoD,
    ThreeD,
    Ui,
    Audio,
}

impl Category {
    pub const ALL: [Self; 5] = [Self::Node, Self::TwoD, Self::ThreeD, Self::Ui, Self::Audio];

    pub fn label(self) -> &'static str {
        match self {
            Self::Node => "Core",
            Self::TwoD => "2D",
            Self::ThreeD => "3D",
            Self::Ui => "UI",
            Self::Audio => "Audio",
        }
    }
}

#[derive(Debug)]
pub struct PropDef {
    pub name: &'static str,
    pub ty: PropType,
    pub default: Variant,
    pub hint: &'static str,
}

#[derive(Debug)]
pub struct ClassInfo {
    pub name: &'static str,
    pub parent: Option<&'static str>,
    pub category: Category,
    /// Cargo feature of the `neko` crate that provides the class.
    pub feature: &'static str,
    pub description: &'static str,
    /// Abstract classes group others and cannot be instanced.
    pub is_abstract: bool,
    /// Properties declared by this class (not inherited ones).
    pub props: &'static [PropDef],
}

/// The class database. Built at compile time (immutable `static` tables):
/// no start-up cost, no locks, safe to read from any thread, and no allocation
/// on targets with very little memory.
pub struct ClassDb {
    classes: &'static [ClassInfo],
}

static DB: ClassDb = ClassDb { classes: BUILTIN };

impl ClassDb {
    /// The built-in class database.
    pub fn get() -> &'static ClassDb {
        &DB
    }

    pub fn class(&self, name: &str) -> Option<&'static ClassInfo> {
        self.classes.iter().find(|c| c.name == name)
    }

    pub fn all(&self) -> &'static [ClassInfo] {
        self.classes
    }

    pub fn root(&self) -> &ClassInfo {
        &self.classes[0]
    }

    /// Direct subclasses, in declaration order.
    pub fn children(&self, name: &str) -> Vec<&'static str> {
        self.classes.iter().filter(|c| c.parent == Some(name)).map(|c| c.name).collect()
    }

    /// `["Node", "Node2D", "Sprite2D"]` for `"Sprite2D"`.
    pub fn ancestry(&self, name: &str) -> Vec<&'static str> {
        let mut out = Vec::new();
        let mut cur = self.class(name);
        while let Some(c) = cur {
            out.push(c.name);
            cur = c.parent.and_then(|p| self.class(p));
        }
        out.reverse();
        out
    }

    pub fn inherits(&self, class: &str, base: &str) -> bool {
        self.ancestry(class).contains(&base)
    }

    /// All properties of a class, base classes first.
    pub fn properties(&self, name: &str) -> Vec<(&'static str, &PropDef)> {
        self.ancestry(name)
            .into_iter()
            .filter_map(|c| self.class(c))
            .flat_map(|c| c.props.iter().map(move |p| (c.name, p)))
            .collect()
    }

    pub fn property(&self, class: &str, prop: &str) -> Option<&PropDef> {
        self.properties(class).into_iter().map(|(_, p)| p).find(|p| p.name == prop)
    }
}

// ---------------------------------------------------------------- built-ins

const fn p(name: &'static str, ty: PropType, default: Variant) -> PropDef {
    PropDef { name, ty, default, hint: "" }
}

const fn ph(name: &'static str, ty: PropType, default: Variant, hint: &'static str) -> PropDef {
    PropDef { name, ty, default, hint }
}

/// `class!(name, parent, category, feature, abstract, description, props)`.
/// A macro (not a const fn) so the `&[...]` property lists live in the static.
macro_rules! class {
    ($name:expr, $parent:expr, $category:expr, $feature:expr, $abstract:expr, $desc:expr, $props:expr $(,)?) => {
        ClassInfo { name: $name, parent: $parent, category: $category, feature: $feature, is_abstract: $abstract, description: $desc, props: $props }
    };
}

use Category::*;
use PropType as T;
use Variant as V;

const WHITE: Variant = V::Color([1.0, 1.0, 1.0, 1.0]);
const PHYSICS_LAYERS: &[PropDef] = &[
    ph("collision_layer", T::Int, V::Int(1), "Bit mask of the layers this object is on."),
    ph("collision_mask", T::Int, V::Int(1), "Bit mask of the layers this object detects."),
];
const AUDIO: &[PropDef] = &[
    ph("stream", T::Resource, V::str(""), "Audio file, e.g. res://sfx/jump.ogg"),
    p("volume_db", T::Float, V::Float(0.0)),
    p("autoplay", T::Bool, V::Bool(false)),
    p("bus", T::String, V::str("Master")),
];
const AUDIO_POSITIONAL: &[PropDef] = &[
    ph("stream", T::Resource, V::str(""), "Audio file, e.g. res://sfx/jump.ogg"),
    p("volume_db", T::Float, V::Float(0.0)),
    p("autoplay", T::Bool, V::Bool(false)),
    p("bus", T::String, V::str("Master")),
    p("max_distance", T::Float, V::Float(2000.0)),
];

/// Every built-in class, parents before children.
static BUILTIN: &[ClassInfo] = &[
        class!("Node", None, Node, "core", false,
            "Base of everything in the scene tree. Has a name, a parent and children, and receives the ready, process and physics_process callbacks.",
            &[ph("process_mode", T::String, V::str("inherit"), "inherit, always, pausable or disabled")]),

        // ----------------------------------------------------------------- 2D
        class!("Node2D", Some("Node"), TwoD, "2d", false,
            "A 2D object with position, rotation, scale and z-index. Parent of everything on the 2D canvas.",
            &[
                p("position", T::Vec2, V::Vec2([0.0, 0.0])),
                ph("rotation_degrees", T::Float, V::Float(0.0), "Clockwise, in degrees."),
                p("scale", T::Vec2, V::Vec2([1.0, 1.0])),
                p("z_index", T::Int, V::Int(0)),
                p("visible", T::Bool, V::Bool(true)),
                p("modulate", T::Color, WHITE),
            ]),
        class!("Marker2D", Some("Node2D"), TwoD, "2d", false,
            "A point in 2D space with an editor gizmo. Use it for spawn points and targets.",
            &[p("gizmo_extents", T::Float, V::Float(10.0))]),
        class!("Sprite2D", Some("Node2D"), TwoD, "2d", false,
            "Draws a texture, or a region of an atlas. Supports flipping, offset and sprite sheets.",
            &[
                ph("texture", T::Resource, V::str(""), "Image file, e.g. res://player.png"),
                p("centered", T::Bool, V::Bool(true)),
                p("offset", T::Vec2, V::Vec2([0.0, 0.0])),
                p("flip_h", T::Bool, V::Bool(false)),
                p("flip_v", T::Bool, V::Bool(false)),
                p("hframes", T::Int, V::Int(1)),
                p("vframes", T::Int, V::Int(1)),
                p("frame", T::Int, V::Int(0)),
            ]),
        class!("AnimatedSprite2D", Some("Node2D"), TwoD, "2d", false,
            "A sprite animated frame by frame from a SpriteFrames resource.",
            &[
                p("sprite_frames", T::Resource, V::str("")),
                p("animation", T::String, V::str("default")),
                p("autoplay", T::Bool, V::Bool(false)),
                p("speed_scale", T::Float, V::Float(1.0)),
            ]),
        class!("Camera2D", Some("Node2D"), TwoD, "2d", false,
            "Sets which part of the 2D world is visible. Supports smoothing, limits and zoom.",
            &[
                p("enabled", T::Bool, V::Bool(true)),
                p("zoom", T::Vec2, V::Vec2([1.0, 1.0])),
                p("position_smoothing", T::Bool, V::Bool(false)),
            ]),
        class!("TileMap", Some("Node2D"), TwoD, "2d", false,
            "A grid of tiles with layers, autotiling and collision generated from a TileSet.",
            &[p("tile_set", T::Resource, V::str("")), p("cell_size", T::Vec2, V::Vec2([16.0, 16.0]))]),
        class!("Line2D", Some("Node2D"), TwoD, "2d", false,
            "A polyline with width, gradient and texture along its length.",
            &[p("width", T::Float, V::Float(4.0)), p("default_color", T::Color, WHITE)]),
        class!("Polygon2D", Some("Node2D"), TwoD, "2d", false,
            "A filled polygon with UVs and bone deformation.",
            &[p("color", T::Color, WHITE), p("texture", T::Resource, V::str(""))]),
        class!("CollisionObject2D", Some("Node2D"), TwoD, "physics2d", true,
            "Abstract base of objects that take part in 2D physics. Collision layers and masks.",
            PHYSICS_LAYERS),
        class!("Area2D", Some("CollisionObject2D"), TwoD, "physics2d", false,
            "Detects overlapping bodies and areas. Can change gravity locally.",
            &[p("monitoring", T::Bool, V::Bool(true)), p("gravity_override", T::Bool, V::Bool(false))]),
        class!("PhysicsBody2D", Some("CollisionObject2D"), TwoD, "physics2d", true,
            "Abstract base of 2D physics bodies.", &[]),
        class!("StaticBody2D", Some("PhysicsBody2D"), TwoD, "physics2d", false,
            "A body the simulation never moves. Walls, floors, platforms.",
            &[p("friction", T::Float, V::Float(1.0)), p("bounce", T::Float, V::Float(0.0))]),
        class!("RigidBody2D", Some("PhysicsBody2D"), TwoD, "physics2d", false,
            "A simulated body with mass, friction, forces and impulses.",
            &[
                p("mass", T::Float, V::Float(1.0)),
                p("gravity_scale", T::Float, V::Float(1.0)),
                p("lock_rotation", T::Bool, V::Bool(false)),
            ]),
        class!("CharacterBody2D", Some("PhysicsBody2D"), TwoD, "physics2d", false,
            "A body moved by code with move_and_slide. Ideal for players and enemies.",
            &[
                p("velocity", T::Vec2, V::Vec2([0.0, 0.0])),
                ph("floor_max_angle", T::Float, V::Float(45.0), "Degrees."),
            ]),
        class!("RayCast2D", Some("Node2D"), TwoD, "physics2d", false,
            "A ray that reports the first collision against physics bodies along its target.",
            &[
                p("enabled", T::Bool, V::Bool(true)),
                p("target_position", T::Vec2, V::Vec2([0.0, 50.0])),
                p("collision_mask", T::Int, V::Int(1)),
                p("exclude_parent", T::Bool, V::Bool(true)),
                p("collide_with_areas", T::Bool, V::Bool(false)),
                p("collide_with_bodies", T::Bool, V::Bool(true)),
                p("hit_from_inside", T::Bool, V::Bool(false)),
            ]),
        class!("CollisionShape2D", Some("Node2D"), TwoD, "physics2d", false,
            "Gives a shape to a CollisionObject2D parent.",
            &[
                ph("shape", T::String, V::str("rectangle"), "rectangle, circle or capsule"),
                ph("size", T::Vec2, V::Vec2([32.0, 32.0]), "Width and height; circles use the width as diameter."),
                p("disabled", T::Bool, V::Bool(false)),
            ]),
        class!("GpuParticles2D", Some("Node2D"), TwoD, "particles", false,
            "2D particle system simulated in a WGSL compute shader.",
            &[p("amount", T::Int, V::Int(32)), p("lifetime", T::Float, V::Float(1.0)), p("emitting", T::Bool, V::Bool(true))]),
        class!("CPUParticles2D", Some("Node2D"), TwoD, "particles", false,
            "2D particle system simulated on the CPU. Godot projects use it for short effects.",
            &[
                p("amount", T::Int, V::Int(8)),
                p("lifetime", T::Float, V::Float(1.0)),
                p("emitting", T::Bool, V::Bool(true)),
                p("one_shot", T::Bool, V::Bool(false)),
                p("explosiveness", T::Float, V::Float(0.0)),
                p("direction", T::Vec2, V::Vec2([0.0, -1.0])),
                p("spread", T::Float, V::Float(45.0)),
                p("gravity", T::Vec2, V::Vec2([0.0, 980.0])),
                p("initial_velocity_min", T::Float, V::Float(0.0)),
                p("initial_velocity_max", T::Float, V::Float(10.0)),
                p("scale_amount_min", T::Float, V::Float(1.0)),
                p("scale_amount_max", T::Float, V::Float(1.0)),
                p("color", T::Color, WHITE),
                p("texture", T::Resource, V::str("")),
            ]),
        class!("PointLight2D", Some("Node2D"), TwoD, "2d", false,
            "A 2D light from a texture, with shadows cast by occluders.",
            &[p("color", T::Color, WHITE), p("energy", T::Float, V::Float(1.0)), p("shadows", T::Bool, V::Bool(false))]),
        class!("Path2D", Some("Node2D"), TwoD, "2d", false,
            "A 2D Bezier curve. Use PathFollow2D to move nodes along it.", &[p("curve", T::Resource, V::str(""))]),
        class!("PathFollow2D", Some("Node2D"), TwoD, "2d", false,
            "Follows the parent Path2D by progress or distance.",
            &[p("progress", T::Float, V::Float(0.0)), p("loop", T::Bool, V::Bool(true))]),
        class!("NavigationAgent2D", Some("Node"), TwoD, "navigation", false,
            "Finds paths over the navigation mesh and avoids obstacles.",
            &[p("max_speed", T::Float, V::Float(200.0)), p("radius", T::Float, V::Float(10.0))]),

        // ----------------------------------------------------------------- 3D
        class!("Node3D", Some("Node"), ThreeD, "3d", false,
            "A 3D object with a Transform3D. Parent of everything in the 3D world.",
            &[
                p("position", T::Vec3, V::Vec3([0.0, 0.0, 0.0])),
                p("rotation_degrees", T::Vec3, V::Vec3([0.0, 0.0, 0.0])),
                p("scale", T::Vec3, V::Vec3([1.0, 1.0, 1.0])),
                p("visible", T::Bool, V::Bool(true)),
                p("modulate", T::Color, WHITE),
            ]),
        class!("Marker3D", Some("Node3D"), ThreeD, "3d", false,
            "A point in 3D space with an editor gizmo. Use it for spawn points and targets.",
            &[p("gizmo_extents", T::Float, V::Float(0.25))]),
        class!("Label3D", Some("Node3D"), ThreeD, "3d", false,
            "Text drawn in the 3D world, optionally facing the camera.",
            &[
                p("text", T::String, V::str("")),
                p("font_size", T::Int, V::Int(32)),
                p("pixel_size", T::Float, V::Float(0.005)),
                p("modulate", T::Color, WHITE),
            ]),
        class!("GeometryInstance3D", Some("Node3D"), ThreeD, "3d", false,
            "Base of nodes that draw geometry. Supports material overrides and shadow casting.",
            &[p("material_override", T::Resource, V::str("")), p("material_overlay", T::Resource, V::str("")), p("cast_shadow", T::Bool, V::Bool(true))]),
        class!("MeshInstance3D", Some("GeometryInstance3D"), ThreeD, "3d", false,
            "Renders a Mesh with PBR materials.",
            &[p("mesh", T::Resource, V::str("")), p("material_override", T::Resource, V::str("")), p("cast_shadow", T::Bool, V::Bool(true))]),
        class!("MultiMeshInstance3D", Some("GeometryInstance3D"), ThreeD, "3d", false,
            "Draws one mesh many times from a MultiMesh (grass, rocks, debris).",
            &[p("multimesh", T::Resource, V::str(""))]),
        class!("Camera3D", Some("Node3D"), ThreeD, "3d", false,
            "Perspective or orthographic camera.",
            &[
                ph("fov", T::Float, V::Float(75.0), "Vertical field of view, degrees."),
                p("near", T::Float, V::Float(0.05)),
                p("far", T::Float, V::Float(4000.0)),
                p("current", T::Bool, V::Bool(false)),
                p("h_offset", T::Float, V::Float(0.0)),
                p("v_offset", T::Float, V::Float(0.0)),
            ]),
        class!("SpringArm3D", Some("Node3D"), ThreeD, "3d", false,
            "Keeps a camera at a distance, pulling in when geometry is in the way.",
            &[p("spring_length", T::Float, V::Float(1.0)), p("margin", T::Float, V::Float(0.01)), p("shape", T::Resource, V::str("")), p("collision_mask", T::Int, V::Int(1))]),
        class!("Light3D", Some("Node3D"), ThreeD, "3d", true,
            "Abstract base of 3D lights.",
            &[p("color", T::Color, WHITE), p("energy", T::Float, V::Float(1.0)), p("shadows", T::Bool, V::Bool(false))]),
        class!("DirectionalLight3D", Some("Light3D"), ThreeD, "3d", false,
            "An infinitely distant light, like the sun. Cascaded shadow maps.", &[]),
        class!("OmniLight3D", Some("Light3D"), ThreeD, "3d", false,
            "A point light that shines in all directions.", &[p("range", T::Float, V::Float(5.0))]),
        class!("SpotLight3D", Some("Light3D"), ThreeD, "3d", false,
            "A cone of light with angle and attenuation.",
            &[p("range", T::Float, V::Float(5.0)), ph("angle", T::Float, V::Float(45.0), "Degrees.")]),
        class!("CollisionObject3D", Some("Node3D"), ThreeD, "physics3d", true,
            "Abstract base of objects that take part in 3D physics.", PHYSICS_LAYERS),
        class!("Area3D", Some("CollisionObject3D"), ThreeD, "physics3d", false,
            "Detects overlaps in 3D and changes gravity and damping.", &[p("monitoring", T::Bool, V::Bool(true))]),
        class!("PhysicsBody3D", Some("CollisionObject3D"), ThreeD, "physics3d", true, "Abstract base of 3D physics bodies.", &[]),
        class!("StaticBody3D", Some("PhysicsBody3D"), ThreeD, "physics3d", false, "A static 3D body for level geometry.",
            &[p("friction", T::Float, V::Float(1.0))]),
        class!("RigidBody3D", Some("PhysicsBody3D"), ThreeD, "physics3d", false, "A simulated 3D body with mass and inertia.",
            &[p("mass", T::Float, V::Float(1.0)), p("gravity_scale", T::Float, V::Float(1.0))]),
        class!("CharacterBody3D", Some("PhysicsBody3D"), ThreeD, "physics3d", false,
            "A 3D body moved by code, with floor detection and step handling.",
            &[p("velocity", T::Vec3, V::Vec3([0.0, 0.0, 0.0]))]),
        class!("CollisionShape3D", Some("Node3D"), ThreeD, "physics3d", false,
            "A 3D collision shape: box, sphere, capsule or convex mesh.",
            &[ph("shape", T::String, V::str("box"), "box, sphere, capsule or convex"), p("size", T::Vec3, V::Vec3([1.0, 1.0, 1.0]))]),
        class!("WorldEnvironment", Some("Node"), ThreeD, "3d", false,
            "Sky, fog, tonemapping, SSAO and glow for the scene.", &[p("environment", T::Resource, V::str(""))]),
        class!("GpuParticles3D", Some("Node3D"), ThreeD, "particles", false, "3D particles simulated on the GPU.",
            &[p("amount", T::Int, V::Int(64)), p("lifetime", T::Float, V::Float(1.0))]),
        class!("CPUParticles3D", Some("Node3D"), ThreeD, "particles", false, "3D particles simulated on the CPU.",
            &[p("amount", T::Int, V::Int(16)), p("lifetime", T::Float, V::Float(1.0)), p("emitting", T::Bool, V::Bool(true)), p("one_shot", T::Bool, V::Bool(false))]),
        class!("Skeleton3D", Some("Node3D"), ThreeD, "animation", false, "A bone hierarchy for skinned meshes.", &[]),
        class!("Decal", Some("Node3D"), ThreeD, "3d", false, "Projects textures onto geometry (marks, dirt, footprints).",
            &[p("texture", T::Resource, V::str("")), p("size", T::Vec3, V::Vec3([2.0, 2.0, 2.0]))]),

        // ----------------------------------------------------------------- UI
        class!("Control", Some("Node"), Ui, "ui", false, "Base of the UI: anchors, offsets, focus and theme.",
            &[
                p("position", T::Vec2, V::Vec2([0.0, 0.0])),
                p("size", T::Vec2, V::Vec2([100.0, 30.0])),
                p("scale", T::Vec2, V::Vec2([1.0, 1.0])),
                p("rotation", T::Float, V::Float(0.0)),
                p("rotation_degrees", T::Float, V::Float(0.0)),
                p("pivot_offset", T::Vec2, V::Vec2([0.0, 0.0])),
                p("modulate", T::Color, WHITE),
                p("self_modulate", T::Color, WHITE),
                p("mouse_filter", T::Int, V::Int(0)),
                p("mouse_default_cursor_shape", T::Int, V::Int(0)),
                p("custom_minimum_size", T::Vec2, V::Vec2([0.0, 0.0])),
                p("size_flags_horizontal", T::Int, V::Int(1)),
                p("size_flags_vertical", T::Int, V::Int(1)),
                p("clip_contents", T::Bool, V::Bool(false)),
                p("layout_mode", T::Int, V::Int(0)),
                p("focus_mode", T::Int, V::Int(0)),
                p("anchor_left", T::Float, V::Float(0.0)),
                p("anchor_top", T::Float, V::Float(0.0)),
                p("anchor_right", T::Float, V::Float(0.0)),
                p("anchor_bottom", T::Float, V::Float(0.0)),
                p("offset_left", T::Float, V::Float(0.0)),
                p("offset_top", T::Float, V::Float(0.0)),
                p("offset_right", T::Float, V::Float(0.0)),
                p("offset_bottom", T::Float, V::Float(0.0)),
                p("grow_horizontal", T::Int, V::Int(1)),
                p("grow_vertical", T::Int, V::Int(1)),
                p("tooltip_text", T::String, V::str("")),
                ph("anchors", T::String, V::str("top_left"), "top_left, center, full_rect, ..."),
                p("visible", T::Bool, V::Bool(true)),
                p("theme", T::Resource, V::str("")),
            ]),
        class!("Label", Some("Control"), Ui, "ui", false, "Plain text with alignment and wrapping.",
            &[p("text", T::String, V::str("")), ph("horizontal_alignment", T::String, V::str("left"), "left, center or right")]),
        class!("RichTextLabel", Some("Control"), Ui, "ui", false, "Text with markup, inline images and effects.",
            &[p("text", T::String, V::str("")), p("bbcode_enabled", T::Bool, V::Bool(true))]),
        class!("Button", Some("Control"), Ui, "ui", false, "A clickable button with text and icon. Emits pressed.",
            &[
                p("text", T::String, V::str("Button")),
                p("icon", T::Resource, V::str("")),
                p("disabled", T::Bool, V::Bool(false)),
                p("toggle_mode", T::Bool, V::Bool(false)),
                p("button_pressed", T::Bool, V::Bool(false)),
            ]),
        class!("CheckBox", Some("Button"), Ui, "ui", false, "A button with a check box. Use it for on/off options.", &[]),
        class!("CheckButton", Some("Button"), Ui, "ui", false, "A button with a switch. Use it for settings that apply at once.", &[]),
        class!("OptionButton", Some("Button"), Ui, "ui", false, "A button that opens a list of choices.",
            &[ph("selected", T::Int, V::Int(-1), "Index of the selected item, -1 for none.")]),
        class!("ColorPickerButton", Some("Button"), Ui, "ui", false, "A button that shows a color and opens a color picker.",
            &[p("color", T::Color, V::Color([0.0, 0.0, 0.0, 1.0])), p("edit_alpha", T::Bool, V::Bool(true))]),
        class!("ColorRect", Some("Control"), Ui, "ui", false, "A rectangle filled with one color.",
            &[p("color", T::Color, WHITE)]),
        class!("LineEdit", Some("Control"), Ui, "ui", false, "A single-line text field with IME support.",
            &[p("text", T::String, V::str("")), p("placeholder", T::String, V::str("")), p("editable", T::Bool, V::Bool(true))]),
        class!("TextureRect", Some("Control"), Ui, "ui", false, "Shows a texture inside the UI layout.",
            &[p("texture", T::Resource, V::str("")), ph("stretch_mode", T::String, V::str("scale"), "scale, tile, keep, keep_aspect")]),
        class!("Range", Some("Control"), Ui, "ui", true, "Abstract base of controls with a value between a minimum and a maximum.",
            &[
                p("min_value", T::Float, V::Float(0.0)),
                p("max_value", T::Float, V::Float(100.0)),
                p("step", T::Float, V::Float(0.01)),
                p("value", T::Float, V::Float(0.0)),
            ]),
        class!("ProgressBar", Some("Range"), Ui, "ui", false, "A progress bar with min and max values.",
            &[p("show_percentage", T::Bool, V::Bool(true))]),
        class!("Slider", Some("Range"), Ui, "ui", true, "Abstract base of sliders.", &[p("editable", T::Bool, V::Bool(true))]),
        class!("HSlider", Some("Slider"), Ui, "ui", false, "A horizontal slider to pick a value.", &[]),
        class!("VSlider", Some("Slider"), Ui, "ui", false, "A vertical slider to pick a value.", &[]),
        class!("Separator", Some("Control"), Ui, "ui", true, "Abstract base of separator lines.", &[]),
        class!("HSeparator", Some("Separator"), Ui, "ui", false, "A horizontal line that separates controls.", &[]),
        class!("VSeparator", Some("Separator"), Ui, "ui", false, "A vertical line that separates controls.", &[]),
        class!("Panel", Some("Control"), Ui, "ui", false, "A background styled by the theme.", &[]),
        class!("Container", Some("Control"), Ui, "ui", false, "Arranges its children automatically.", &[]),
        class!("SubViewportContainer", Some("Container"), Ui, "ui", false, "Embeds a SubViewport in the UI.",
            &[p("stretch", T::Bool, V::Bool(false)), p("stretch_shrink", T::Int, V::Int(1))]),
        class!("SubViewport", Some("Node"), Ui, "ui", false, "A render target with its own scene tree.",
            &[p("size", T::Vec2, V::Vec2([512.0, 512.0])), p("own_world_3d", T::Bool, V::Bool(false)), p("transparent_bg", T::Bool, V::Bool(false))]),
        class!("PanelContainer", Some("Container"), Ui, "ui", false, "A panel background that fits its children.", &[]),
        class!("CenterContainer", Some("Container"), Ui, "ui", false, "Keeps its children centered.",
            &[p("use_top_left", T::Bool, V::Bool(false))]),
        class!("HBoxContainer", Some("Container"), Ui, "ui", false, "Stacks children horizontally.", &[p("separation", T::Int, V::Int(4))]),
        class!("VBoxContainer", Some("Container"), Ui, "ui", false, "Stacks children vertically.", &[p("separation", T::Int, V::Int(4))]),
        class!("GridContainer", Some("Container"), Ui, "ui", false, "Lays children out in columns.", &[p("columns", T::Int, V::Int(2))]),
        class!("MarginContainer", Some("Container"), Ui, "ui", false, "Adds margins around its children.", &[p("margin", T::Int, V::Int(8))]),
        class!("ScrollContainer", Some("Container"), Ui, "ui", false, "A scrollable area for content larger than its size.", &[]),

        // -------------------------------------------------------------- audio
        class!("AudioStreamPlayer", Some("Node"), Audio, "audio", false, "Plays non-positional audio: music and UI sounds.", AUDIO),
        class!("AudioStreamPlayer2D", Some("Node2D"), Audio, "audio", false, "Positional 2D audio that fades with distance.", AUDIO_POSITIONAL),
        class!("AudioStreamPlayer3D", Some("Node3D"), Audio, "audio", false, "Spatial 3D audio with doppler and HRTF.", AUDIO_POSITIONAL),

        // --------------------------------------------------------------- misc
        class!("CanvasLayer", Some("Node"), Node, "2d", false, "A drawing layer independent of the camera. Use it for HUDs.",
            &[p("layer", T::Int, V::Int(1)), p("visible", T::Bool, V::Bool(true))]),
        class!("SubViewport", Some("Node"), Node, "render", false, "Renders a scene into a separate texture (a render target).",
            &[p("size", T::Vec2, V::Vec2([512.0, 512.0]))]),
        class!("AnimationPlayer", Some("Node"), Node, "animation", false, "Animates any property of any node with tracks and keyframes.",
            &[p("autoplay", T::String, V::str("")), p("speed_scale", T::Float, V::Float(1.0))]),
        class!("Timer", Some("Node"), Node, "core", false, "Counts down and emits timeout. One-shot or repeating.",
            &[
                ph("wait_time", T::Float, V::Float(1.0), "Seconds."),
                p("one_shot", T::Bool, V::Bool(false)),
                p("autostart", T::Bool, V::Bool(false)),
            ]),
        class!("HttpRequest", Some("Node"), Node, "net", false, "Asynchronous HTTP requests that never block a frame.",
            &[ph("timeout", T::Float, V::Float(0.0), "Seconds; 0 means no timeout.")]),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_parent_exists_and_root_is_node() {
        let db = ClassDb::get();
        assert_eq!(db.root().name, "Node");
        for c in db.all() {
            if let Some(p) = c.parent {
                assert!(db.class(p).is_some(), "{} has unknown parent {p}", c.name);
            }
            for prop in c.props {
                assert!(prop.default.type_matches(prop.ty), "{}.{} default has the wrong type", c.name, prop.name);
            }
        }
    }

    #[test]
    fn inheritance_and_properties() {
        let db = ClassDb::get();
        assert_eq!(db.ancestry("CharacterBody2D"), ["Node", "Node2D", "CollisionObject2D", "PhysicsBody2D", "CharacterBody2D"]);
        assert!(db.inherits("Sprite2D", "Node2D"));
        assert!(!db.inherits("Label", "Node2D"));
        let names: Vec<_> = db.properties("Sprite2D").into_iter().map(|(_, p)| p.name).collect();
        assert_eq!(names[0], "process_mode");
        assert!(names.contains(&"position") && names.contains(&"texture"));
        assert!(db.children("Node").contains(&"Node2D"));
    }
}
