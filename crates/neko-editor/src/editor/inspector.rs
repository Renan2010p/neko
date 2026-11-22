//! The Inspector dock (Godot's EditorInspector): one category per class,
//! most derived first, properties sorted into foldable groups (sections)
//! exactly where Godot's `ADD_GROUP` puts them; label on the left, editor on
//! the right, and a revert button when a property differs from its default.

use std::cell::Cell;
use std::collections::HashSet;
use std::rc::Rc;

use neko::core::{ClassDb, NodeId, PropDef, PropType, Variant};
use neko_ui::{Align2, Color32, Control, FontId, Id, Shape, Sides, Stroke, StyleBox, UiEvent, bold, pos2, vec2};

use super::Editor;
use crate::icons::{self, Icon};

struct Row {
    prop: &'static str,
    ty: PropType,
    label: Id,
    editors: Vec<Id>,
    revert: Id,
}

/// A foldable group (Godot's EditorInspectorSection).
struct Section {
    key: String,
    header: Id,
    body: Id,
    folded: Rc<Cell<bool>>,
    /// Count of properties in the section that differ from their default.
    changes: Rc<Cell<usize>>,
    props: Vec<&'static str>,
}

#[derive(Default)]
pub(super) struct State {
    built_for: Option<(usize, NodeId, String)>,
    rows: Vec<Row>,
    sections: Vec<Section>,
    /// Unfolded sections ("Category/Group"); Godot folds every section by default.
    unfolded: HashSet<String>,
    color_popup: Option<(Id, &'static str, Vec<Id>)>,
    /// The Script slot's button (opens the script).
    pub script_button: Option<Id>,
}

/// Binding order of groups that share a category (CanvasItem binds
/// Visibility before Ordering, Texture and Material).
fn group_rank(group: &str) -> usize {
    ["Layout", "Transform", "Visibility", "Ordering", "Texture", "Material", "Process", "Theme"].iter().position(|g| *g == group).unwrap_or(1)
}

/// Where Godot shows a property: its inspector category (the class that binds
/// it; `CanvasItem` owns visibility and ordering) and its `ADD_GROUP`.
/// Mirrors `_bind_methods` in scene/main/node.cpp, canvas_item.cpp,
/// 2d/node_2d.cpp, 3d/node_3d.cpp, gui/control.cpp and friends.
fn placement(class: &'static str, prop: &str) -> (&'static str, Option<&'static str>) {
    match (class, prop) {
        ("Node", "process_mode") => ("Node", Some("Process")),
        ("Node2D" | "Control", "visible" | "modulate") => ("CanvasItem", Some("Visibility")),
        ("Node2D", "z_index") => ("CanvasItem", Some("Ordering")),
        ("Node2D" | "Node3D", "position" | "rotation_degrees" | "scale") => (class, Some("Transform")),
        ("Node3D", "visible") => (class, Some("Visibility")),
        ("Control", "position" | "size" | "anchors") => (class, Some("Layout")),
        ("Control", "theme") => (class, Some("Theme")),
        ("Sprite2D", "centered" | "offset" | "flip_h" | "flip_v") => (class, Some("Offset")),
        ("Sprite2D", "hframes" | "vframes" | "frame") => (class, Some("Animation")),
        ("Camera2D", "position_smoothing") => (class, Some("Position Smoothing")),
        ("CollisionObject2D" | "CollisionObject3D", "collision_layer" | "collision_mask") => (class, Some("Collision")),
        ("MeshInstance3D", "material_override" | "cast_shadow") => ("GeometryInstance3D", Some("Geometry")),
        ("Light3D", "color" | "energy") => (class, Some("Light")),
        ("Light3D", "shadows") => (class, Some("Shadow")),
        ("RigidBody2D" | "RigidBody3D", "mass" | "gravity_scale") => (class, Some("Mass")),
        ("CharacterBody2D", "floor_max_angle") => (class, Some("Floor")),
        ("Line2D", "default_color") => (class, Some("Fill")),
        _ => (class, None),
    }
}

/// `"rotation_degrees"` -> `"Rotation Degrees"`, like Godot's inspector.
fn pretty(name: &str) -> String {
    name.split('_')
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn rgba(c: [f32; 4]) -> Color32 {
    let f = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgba_unmultiplied(f(c[0]), f(c[1]), f(c[2]), f(c[3]))
}

impl Editor {
    pub(super) fn sync_inspector(&mut self) {
        let filter = self.ui.text(self.w.insp_filter).trim().to_lowercase();
        let doc = &self.docs[self.current];
        let target = doc.selected.filter(|&id| doc.has_root && doc.scene.contains(id));
        let key = target.map(|id| (self.current, id, filter.clone()));
        if key != self.inspector.built_for {
            self.rebuild_inspector(target, &filter);
            self.inspector.built_for = key;
            return;
        }
        let Some(id) = target else { return };
        let t = self.ui.theme.clone();
        let focused = self.ui.focused();
        let node = self.docs[self.current].scene.get(id).expect("checked").clone();
        for row in &self.inspector.rows {
            let value = node.get(row.prop);
            let overridden = node.is_overridden(row.prop);
            self.ui.set_font_color(row.label, Some(if overridden { t.font_focus_color } else { t.font_color }));
            self.ui.set_visible(row.revert, overridden);
            for (i, &e) in row.editors.iter().enumerate() {
                if focused == Some(e) {
                    continue;
                }
                match (&value, row.ty) {
                    (Some(Variant::Bool(b)), _) => self.ui.set_checked(e, *b),
                    (Some(Variant::Int(v)), _) => self.ui.set_value(e, *v as f64),
                    (Some(Variant::Float(v)), _) => self.ui.set_value(e, *v),
                    (Some(Variant::Str(s)), _) => {
                        if self.ui.text(e) != s.as_ref() {
                            self.ui.set_text(e, s.to_string());
                        }
                    }
                    (Some(Variant::Vec2(v)), _) => self.ui.set_value(e, v[i] as f64),
                    (Some(Variant::Vec3(v)), _) => self.ui.set_value(e, v[i] as f64),
                    (Some(Variant::Color(c)), _) => self.ui.set_color(e, rgba(*c)),
                    _ => {}
                }
            }
        }
        self.update_sections();
    }

    fn rebuild_inspector(&mut self, target: Option<NodeId>, filter: &str) {
        let body = self.w.insp_body;
        self.ui.clear_children(body);
        self.inspector.rows.clear();
        let t = self.ui.theme.clone();
        let Some(id) = target else {
            self.ui.set_text(self.w.insp_name, "");
            self.ui.set_text(self.w.insp_class, "");
            self.ui.add(body, Control::label("Select a node to edit its properties.").wrap().color(t.font_muted_color));
            return;
        };
        let node = self.docs[self.current].scene.get(id).expect("checked").clone();
        self.ui.set_text(self.w.insp_name, node.name.clone());
        self.ui.set_text(self.w.insp_class, node.class);
        if let Some(img) = self.icons.class(node.class) {
            self.ui.set_image(self.w.insp_icon, img);
        }
        let db = ClassDb::get();
        // Categories in Godot's order: most derived class first, CanvasItem
        // right above Node. Each holds ungrouped properties, then groups.
        let mut cats: Vec<&'static str> = db.ancestry(node.class).into_iter().rev().collect();
        if db.inherits(node.class, "Node2D") || db.inherits(node.class, "Control") {
            cats.insert(cats.len() - 1, "CanvasItem");
        }
        if node.class == "MeshInstance3D" {
            cats.insert(1, "GeometryInstance3D");
        }
        type Group = (Option<&'static str>, Vec<&'static PropDef>);
        let mut placed: Vec<(&'static str, Vec<Group>)> = cats.iter().map(|&c| (c, Vec::new())).collect();
        for cls in db.ancestry(node.class) {
            let Some(info) = db.class(cls) else { continue };
            for def in info.props {
                if !(filter.is_empty() || def.name.contains(filter) || pretty(def.name).to_lowercase().contains(filter)) {
                    continue;
                }
                let (cat, group) = placement(cls, def.name);
                let Some((_, groups)) = placed.iter_mut().find(|(c, _)| *c == cat) else { continue };
                match groups.iter_mut().find(|(g, _)| *g == group) {
                    Some((_, list)) => list.push(def),
                    None => groups.push((group, vec![def])),
                }
            }
        }
        self.inspector.sections.clear();
        for (cat, mut groups) in placed {
            if groups.is_empty() {
                continue;
            }
            // Ungrouped properties come first, then groups in binding order.
            groups.sort_by_key(|(g, _)| g.map(group_rank));
            // Category header: class icon and name, centered (EditorInspectorCategory).
            let head = self.ui.add(body, Control::panel_container().style(StyleBox::flat(t.button.normal.bg, t.corner_radius, Sides::symmetric(6.0, 4.0))));
            let hrow = self.ui.add(head, Control::hbox());
            self.ui.add(hrow, Control::spacer());
            if let Some(img) = self.icons.get(cat).or_else(|| self.icons.class(cat)) {
                self.ui.add(hrow, Control::image(img));
            }
            self.ui.add(hrow, Control::label(cat).color(t.font_focus_color).v_center());
            self.ui.add(hrow, Control::spacer());
            for (group, defs) in groups {
                let parent = match group {
                    None => body,
                    Some(g) => self.add_section(body, cat, g, !filter.is_empty()),
                };
                for def in &defs {
                    self.add_property_row(parent, &node, def);
                }
                if let Some(sec) = self.inspector.sections.last_mut()
                    && group.is_some()
                {
                    sec.props = defs.iter().map(|d| d.name).collect();
                }
            }
        }
        // Godot ends every Object with its script slot and the metadata button.
        if filter.is_empty() || "script".contains(filter) {
            let row = self.ui.add(body, Control::hbox().separation(4.0));
            self.ui.add(row, Control::label("Script").v_center().ratio(1.0));
            // EditorResourcePicker: the script's file name; clicking opens it.
            let script = target.and_then(|id| neko::core::tscn::script_path(&self.docs[self.current].scene, id));
            let (text, tip) = match &script {
                Some(p) => (p.rsplit('/').next().unwrap_or(p).to_string(), format!("{p}\nClick to edit.")),
                None => ("<empty>".to_string(), "Use Attach Script in the Scene dock to add one.".to_string()),
            };
            let b = self.ui.add(row, Control::button(text).disabled(script.is_none()).tooltip(tip).expand_h().ratio(1.3));
            self.inspector.script_button = Some(b);
            self.ui.add(row, Control::control().min(20.0, 20.0));
        }
        let meta = self.ui.add(body, Control::hbox());
        self.ui.add(meta, Control::spacer());
        self.ui.add(meta, crate::ui_kit::icon_text_button(&self.icons, "Add", "Add Metadata").disabled(true));
        self.ui.add(meta, Control::spacer());
        self.update_sections();
    }

    /// A foldable section header plus its (hidden while folded) body.
    fn add_section(&mut self, parent: Id, cat: &str, group: &'static str, force_open: bool) -> Id {
        let t = self.ui.theme.clone();
        let key = format!("{cat}/{group}");
        let folded = Rc::new(Cell::new(!force_open && !self.inspector.unfolded.contains(&key)));
        let changes = Rc::new(Cell::new(0usize));
        let (f, n) = (folded.clone(), changes.clone());
        let (right, down) = (self.icons.get("GuiTreeArrowRight"), self.icons.get("GuiTreeArrowDown"));
        let (font_color, disabled, fs) = (t.font_color, t.font_disabled_color, t.font_size);
        // EditorInspectorSection::_notification(NOTIFICATION_DRAW): arrow, bold
        // title, and "(N changes)" in the disabled color while folded.
        let header = self.ui.add(
            parent,
            Control::custom(vec2(0.0, fs + 8.0), move |p, r, hovered, pressed| {
                if hovered {
                    let a = if pressed { 8 } else { 18 };
                    p.rect_filled(r, 3, Color32::from_white_alpha(a));
                }
                let icon = if f.get() { right } else { down };
                let ar = neko_ui::Rect::from_center_size(pos2(r.left() + 10.0, r.center().y), vec2(14.0, 14.0));
                match icon {
                    Some(img) => p.image(img.tinted(Color32::from_white_alpha(if hovered { 255 } else { 217 })), ar),
                    None => {
                        let c = ar.center();
                        let pts = if f.get() { vec![c + vec2(-3.0, -5.0), c + vec2(4.0, 0.0), c + vec2(-3.0, 5.0)] } else { vec![c + vec2(-5.0, -3.0), c + vec2(5.0, -3.0), c + vec2(0.0, 4.0)] };
                        p.add(Shape::convex_polygon(pts, font_color, Stroke::NONE));
                    }
                }
                let title_color = if hovered { Color32::WHITE } else { font_color };
                p.text(pos2(ar.right() + 4.0, r.center().y), Align2::LEFT_CENTER, group, FontId::new(fs, bold()), title_color);
                if f.get() && n.get() > 0 {
                    let text = if n.get() == 1 { "(1 change)".to_string() } else { format!("({} changes)", n.get()) };
                    p.text(pos2(r.right() - 4.0, r.center().y), Align2::RIGHT_CENTER, &text, FontId::proportional(fs - 1.0), disabled);
                }
            })
            .clickable()
            .expand_h(),
        );
        let body = self.ui.add(parent, Control::margin(Sides { left: 8.0, top: 0.0, right: 0.0, bottom: 0.0 }).hidden());
        let col = self.ui.add(body, Control::vbox().separation(3.0));
        self.ui.set_visible(body, !folded.get());
        self.inspector.sections.push(Section { key, header, body, folded, changes, props: Vec::new() });
        col
    }

    /// Refresh the "(N changes)" counters of folded sections.
    fn update_sections(&mut self) {
        let doc = &self.docs[self.current];
        let Some(node) = doc.selected.and_then(|id| doc.scene.get(id)) else { return };
        for sec in &self.inspector.sections {
            sec.changes.set(sec.props.iter().filter(|p| node.is_overridden(p)).count());
        }
    }

    fn add_property_row(&mut self, parent: Id, node: &neko::core::Node, def: &'static PropDef) {
        let t = self.ui.theme.clone();
        let row = self.ui.add(parent, Control::hbox().separation(4.0));
        let label = self.ui.add(row, Control::label(pretty(def.name)).ellipsis().v_center().ratio(1.0).tooltip(if def.hint.is_empty() { def.name.to_string() } else { format!("{}\n{}", def.name, def.hint) }));
        let area = self.ui.add(row, Control::hbox().separation(3.0).expand_h().ratio(1.3));
        let value = node.get(def.name).unwrap_or_else(|| def.default.clone());
        let mut editors = Vec::new();
        match (def.ty, &value) {
            (PropType::Bool, Variant::Bool(b)) => editors.push(self.ui.add(area, Control::check_box("On", *b))),
            (PropType::Int, Variant::Int(v)) => editors.push(self.ui.add(area, Control::spin_box(*v as f64, 1.0).decimals(0))),
            (PropType::Float, Variant::Float(v)) => editors.push(self.ui.add(area, Control::spin_box(*v, 0.1).decimals(3))),
            (PropType::String | PropType::Resource, Variant::Str(s)) => {
                let ph = if def.ty == PropType::Resource { "<empty>" } else { "" };
                editors.push(self.ui.add(area, Control::line_edit(s.to_string()).placeholder(ph)));
            }
            (PropType::Vec2, Variant::Vec2(v)) => {
                for (i, p) in ["x", "y"].into_iter().enumerate() {
                    editors.push(self.ui.add(area, Control::spin_box(v[i] as f64, 0.5).decimals(2).prefix(p)));
                }
            }
            (PropType::Vec3, Variant::Vec3(v)) => {
                for (i, p) in ["x", "y", "z"].into_iter().enumerate() {
                    editors.push(self.ui.add(area, Control::spin_box(v[i] as f64, 0.05).decimals(2).prefix(p)));
                }
            }
            (PropType::Color, Variant::Color(c)) => editors.push(self.ui.add(area, Control::color_button(rgba(*c)).expand_h())),
            (_, v) => {
                self.ui.add(area, Control::label(v.to_string()).color(t.font_muted_color));
            }
        }
        let revert = self.ui.add(
            row,
            Control::custom(vec2(20.0, 20.0), |p, r, hovered, _| {
                let c = if hovered { Color32::WHITE } else { crate::theme::ACCENT };
                icons::paint_on(p, r.shrink(3.0), Icon::Rename, c);
            })
            .clickable()
            .tooltip(format!("Revert to {}", def.default))
            .hidden(),
        );
        self.inspector.rows.push(Row { prop: def.name, ty: def.ty, label, editors, revert });
    }

    /// Inspector events. Returns true if the event was the inspector's.
    pub(super) fn inspector_event(&mut self, ev: &UiEvent) -> bool {
        if let UiEvent::Pressed(id) = ev
            && let Some(sec) = self.inspector.sections.iter().find(|s| s.header == *id)
        {
            let open = sec.folded.get();
            sec.folded.set(!open);
            let (body, key) = (sec.body, sec.key.clone());
            self.ui.set_visible(body, open);
            if open {
                self.inspector.unfolded.insert(key);
            } else {
                self.inspector.unfolded.remove(&key);
            }
            return true;
        }
        if let UiEvent::TextChanged(id) = ev
            && *id == self.w.insp_filter
        {
            self.inspector.built_for = None;
            return true;
        }
        if let Some((popup, prop, spins)) = &self.inspector.color_popup {
            let (popup, prop, spins) = (*popup, *prop, spins.clone());
            match ev {
                UiEvent::PopupClosed(id) if *id == popup => {
                    self.ui.remove(popup);
                    self.inspector.color_popup = None;
                    return true;
                }
                UiEvent::ValueChanged(id, _) if spins.contains(id) => {
                    let c = [0, 1, 2, 3].map(|i| self.ui.value(spins[i]) as f32);
                    self.set_prop(prop, Variant::Color(c));
                    return true;
                }
                _ => {}
            }
        }
        let Some(id) = self.docs[self.current].selected else { return false };
        let find = |rows: &[Row], w: Id| rows.iter().enumerate().find_map(|(r, row)| row.editors.iter().position(|&e| e == w).map(|i| (r, i)));
        match ev {
            UiEvent::ValueChanged(w, v) => {
                let Some((r, i)) = find(&self.inspector.rows, *w) else { return false };
                let (prop, ty) = (self.inspector.rows[r].prop, self.inspector.rows[r].ty);
                let current = self.docs[self.current].scene.get(id).and_then(|n| n.get(prop));
                let value = match (ty, current) {
                    (PropType::Int, _) => Variant::Int(v.round() as i64),
                    (PropType::Float, _) => Variant::Float(*v),
                    (PropType::Vec2, Some(Variant::Vec2(mut a))) => {
                        a[i] = *v as f32;
                        Variant::Vec2(a)
                    }
                    (PropType::Vec3, Some(Variant::Vec3(mut a))) => {
                        a[i] = *v as f32;
                        Variant::Vec3(a)
                    }
                    _ => return true,
                };
                self.set_prop(prop, value);
                true
            }
            UiEvent::Toggled(w, on) => {
                let Some((r, _)) = find(&self.inspector.rows, *w) else { return false };
                let prop = self.inspector.rows[r].prop;
                self.set_prop(prop, Variant::Bool(*on));
                true
            }
            UiEvent::TextChanged(w) => {
                let Some((r, _)) = find(&self.inspector.rows, *w) else { return false };
                let prop = self.inspector.rows[r].prop;
                let text = self.ui.text(*w).to_string();
                self.set_prop(prop, Variant::Str(text.into()));
                true
            }
            UiEvent::Pressed(w) => {
                if let Some(row) = self.inspector.rows.iter().find(|r| r.revert == *w) {
                    let prop = row.prop;
                    let doc = &mut self.docs[self.current];
                    doc.checkpoint(None);
                    if let Some(n) = doc.scene.get_mut(id) {
                        n.reset(prop);
                    }
                    return true;
                }
                if let Some((r, _)) = find(&self.inspector.rows, *w)
                    && self.inspector.rows[r].ty == PropType::Color
                {
                    let prop = self.inspector.rows[r].prop;
                    self.open_color_popup(prop, *w);
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    fn set_prop(&mut self, prop: &'static str, value: Variant) {
        let doc = &mut self.docs[self.current];
        let Some(id) = doc.selected else { return };
        doc.checkpoint(Some(&format!("{id:?}.{prop}")));
        if let Some(n) = doc.scene.get_mut(id) {
            let _ = n.set(prop, value);
        }
    }

    /// A small RGBA picker under the color button (Godot opens a ColorPicker popup).
    fn open_color_popup(&mut self, prop: &'static str, button: Id) {
        let doc = &self.docs[self.current];
        let Some(Variant::Color(c)) = doc.selected.and_then(|id| doc.scene.get(id)).and_then(|n| n.get(prop)) else { return };
        let t = self.ui.theme.clone();
        let root = self.ui.root();
        let popup = self.ui.add(root, Control::popup());
        let panel = self.ui.add(popup, Control::panel_container().style(t.popup_menu.with_padding(Sides::all(10.0))).min(240.0, 0.0));
        let col = self.ui.add(panel, Control::vbox());
        self.ui.add(col, Control::label(pretty(prop)).color(t.font_focus_color));
        let mut spins = Vec::new();
        for (i, ch) in ["R", "G", "B", "A"].into_iter().enumerate() {
            let row = self.ui.add(col, Control::hbox());
            self.ui.add(row, Control::label(ch).min(18.0, 0.0).v_center());
            spins.push(self.ui.add(row, Control::spin_box(c[i] as f64, 0.01).range(0.0, 1.0).decimals(3)));
        }
        let at = self.ui.rect(button).left_bottom() + vec2(0.0, 4.0);
        self.ui.open_popup(popup, at);
        self.inspector.color_popup = Some((popup, prop, spins));
    }
}
