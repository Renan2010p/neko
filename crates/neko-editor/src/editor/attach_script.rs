//! "Attach Node Script" (Godot's ScriptCreateDialog, editor/script/script_create_dialog.cpp)
//! and the Scene dock's Attach / Detach Script tools (SceneTreeDock).

use std::path::PathBuf;

use neko::core::NodeId;
use neko::platform::Key;
use neko_ui::{Color32, Control, Id, UiEvent};

use super::dialogs::Dialog;
use super::{Editor, FilePurpose};
use crate::ui_kit;

/// A built-in script template (modules/gdscript/editor/script_templates).
struct Template {
    inherit: &'static str,
    name: &'static str,
    content: &'static str,
}

const NODE_DEFAULT: &str = "extends _BASE_


# Called when the node enters the scene tree for the first time.
func _ready() -> void:
_TS_pass # Replace with function body.


# Called every frame. 'delta' is the elapsed time since the previous frame.
func _process(delta: float) -> void:
_TS_pass
";

const OBJECT_EMPTY: &str = "extends _BASE_
";

const CHARACTER_2D: &str = "extends _BASE_


const SPEED = 300.0
const JUMP_VELOCITY = -400.0


func _physics_process(delta: float) -> void:
_TS_# Add the gravity.
_TS_if not is_on_floor():
_TS__TS_velocity += get_gravity() * delta

_TS_# Handle jump.
_TS_if Input.is_action_just_pressed(\"ui_accept\") and is_on_floor():
_TS__TS_velocity.y = JUMP_VELOCITY

_TS_# Get the input direction and handle the movement/deceleration.
_TS_# As good practice, you should replace UI actions with custom gameplay actions.
_TS_var direction := Input.get_axis(\"ui_left\", \"ui_right\")
_TS_if direction:
_TS__TS_velocity.x = direction * SPEED
_TS_else:
_TS__TS_velocity.x = move_toward(velocity.x, 0, SPEED)

_TS_move_and_slide()
";

const CHARACTER_3D: &str = "extends _BASE_


const SPEED = 5.0
const JUMP_VELOCITY = 4.5


func _physics_process(delta: float) -> void:
_TS_# Add the gravity.
_TS_if not is_on_floor():
_TS__TS_velocity += get_gravity() * delta

_TS_# Handle jump.
_TS_if Input.is_action_just_pressed(\"ui_accept\") and is_on_floor():
_TS__TS_velocity.y = JUMP_VELOCITY

_TS_# Get the input direction and handle the movement/deceleration.
_TS_# As good practice, you should replace UI actions with custom gameplay actions.
_TS_var input_dir := Input.get_vector(\"ui_left\", \"ui_right\", \"ui_up\", \"ui_down\")
_TS_var direction := (transform.basis * Vector3(input_dir.x, 0, input_dir.y)).normalized()
_TS_if direction:
_TS__TS_velocity.x = direction.x * SPEED
_TS__TS_velocity.z = direction.z * SPEED
_TS_else:
_TS__TS_velocity.x = move_toward(velocity.x, 0, SPEED)
_TS__TS_velocity.z = move_toward(velocity.z, 0, SPEED)

_TS_move_and_slide()
";

const TEMPLATES: &[Template] = &[
    Template { inherit: "CharacterBody2D", name: "Basic Movement", content: CHARACTER_2D },
    Template { inherit: "CharacterBody3D", name: "Basic Movement", content: CHARACTER_3D },
    Template { inherit: "Node", name: "Default", content: NODE_DEFAULT },
    Template { inherit: "Object", name: "Empty", content: OBJECT_EMPTY },
];

/// Templates for `class`, closest ancestor first (Godot walks the hierarchy upwards).
fn templates_for(class: &str) -> Vec<&'static Template> {
    let mut chain = neko::core::ClassDb::get().ancestry(class);
    chain.reverse();
    let mut out: Vec<&Template> = Vec::new();
    for c in chain.iter().copied().chain(["Object"]) {
        out.extend(TEMPLATES.iter().filter(|t| t.inherit == c));
    }
    if out.is_empty() {
        out.extend(TEMPLATES.iter().filter(|t| t.inherit == "Object"));
    }
    out
}

/// GDScriptLanguage::make_template: fill in the base class and indentation.
fn process_template(t: &Template, base: &str, class_name: &str) -> String {
    t.content.replace("_BASE_", base).replace("_CLASS_SNAKE_CASE_", &to_snake_case(class_name)).replace("_CLASS_", class_name).replace("_TS_", "\t")
}

/// Godot's `String::to_snake_case` ("PlayerBody2D" -> "player_body_2d").
pub(super) fn to_snake_case(s: &str) -> String {
    let ch: Vec<char> = s.chars().collect();
    let mut out = String::new();
    for (i, &c) in ch.iter().enumerate() {
        let prev = i.checked_sub(1).map(|j| ch[j]);
        let next = ch.get(i + 1).copied();
        if c == ' ' || c == '-' {
            out.push('_');
            continue;
        }
        let boundary = match prev {
            Some(p) if p.is_alphanumeric() => {
                (c.is_uppercase() && (p.is_lowercase() || (p.is_uppercase() && next.is_some_and(char::is_lowercase))))
                    || (c.is_ascii_digit() && p.is_alphabetic())
            }
            _ => false,
        };
        if boundary && !out.ends_with('_') {
            out.push('_');
        }
        out.extend(c.to_lowercase());
    }
    out
}

/// The dialog's controls and target.
pub(super) struct Attach {
    pub modal: Id,
    close: Id,
    inherits: Id,
    template: Id,
    templates: Vec<&'static Template>,
    path: Id,
    browse: Id,
    msg_parent: Id,
    msg_path: Id,
    ok: Id,
    cancel: Id,
    node: NodeId,
}

const OK_COLOR: Color32 = Color32::from_rgb(115, 242, 128);
const ERR_COLOR: Color32 = Color32::from_rgb(255, 120, 107);

impl Editor {
    /// SceneTreeDock::attach_script_to_selected.
    pub(super) fn open_attach_script(&mut self) {
        let doc = self.doc();
        let Some(node) = doc.selected.filter(|&id| doc.has_root && doc.scene.contains(id)) else { return };
        let n = doc.scene.get(node).expect("checked");
        let class = n.godot.class.clone().unwrap_or_else(|| n.class.to_string());
        // Next to the scene file (or in res://), named after the node.
        let dir = doc.file.as_ref().and_then(|f| f.parent()).map(PathBuf::from).unwrap_or_else(|| self.path.clone());
        let default_path = format!("{}/{}.gd", self.rel(&dir).trim_end_matches('/'), to_snake_case(&n.name));
        self.close_dialog();

        let t = self.ui.theme.clone();
        let (modal, body, close) = ui_kit::dialog(&mut self.ui, "Attach Node Script", 520.0);
        let grid = self.ui.add(body, Control::vbox().separation(6.0));
        let row = |ui: &mut neko_ui::Ui, label: &str| {
            let r = ui.add(grid, Control::hbox().separation(8.0));
            ui.add(r, Control::label(label).v_center().min(110.0, 0.0));
            r
        };
        let r = row(&mut self.ui, "Language:");
        self.ui.add(r, Control::option_button(&["NekoScript (GDScript)"], 0).expand_h());
        let r = row(&mut self.ui, "Inherits:");
        let inherits = self.ui.add(r, Control::line_edit(class.clone()));
        let templates = templates_for(&class);
        let labels: Vec<String> = templates.iter().map(|t| format!("{}: {}", t.inherit, t.name)).collect();
        let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
        let r = row(&mut self.ui, "Template:");
        let template = self.ui.add(r, Control::option_button(&label_refs, 0).expand_h());
        let r = row(&mut self.ui, "Built-in Script:");
        self.ui.add(r, Control::check_box("On", false).disabled(true).tooltip("Built-in scripts are not supported yet."));
        let r = row(&mut self.ui, "Path:");
        let path = self.ui.add(r, Control::line_edit(default_path));
        let browse = self.ui.add(r, ui_kit::icon_button(&self.icons, "Folder", "Open Script / Choose Location", None));
        // EditorValidationPanel.
        let panel = self.ui.add(body, Control::panel_container().style(neko_ui::StyleBox::flat(t.background, 4, neko_ui::Sides::all(8.0))));
        let msgs = self.ui.add(panel, Control::vbox().separation(4.0));
        let msg_parent = self.ui.add(msgs, Control::label("").wrap());
        let msg_path = self.ui.add(msgs, Control::label("").wrap());
        let (ok, cancel) = ui_kit::ok_cancel(&mut self.ui, body, "Create", "Cancel");
        self.ui.focus(path);
        self.dialog = Some(Dialog::AttachScript(Box::new(Attach { modal, close, inherits, template, templates, path, browse, msg_parent, msg_path, ok, cancel, node })));
        self.validate_attach();
    }

    fn attach_state(&self) -> Option<&Attach> {
        match &self.dialog {
            Some(Dialog::AttachScript(a)) => Some(a),
            _ => None,
        }
    }

    /// The chosen file on disk, or an error (ScriptCreateDialog::_validate_path).
    fn attach_target(&self, text: &str) -> Result<PathBuf, &'static str> {
        let text = text.trim();
        let Some(rel) = text.strip_prefix("res://") else {
            return Err(if text.is_empty() { "Path is empty." } else { "Path is not local." });
        };
        let p = self.path.join(rel);
        let name = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        if name.is_empty() || rel.ends_with('/') {
            return Err("Filename is empty.");
        }
        if p.extension().is_none_or(|e| e != "gd") {
            return Err("Invalid extension.");
        }
        if !p.parent().is_some_and(|d| d.is_dir()) {
            return Err("Base path is invalid.");
        }
        if p.is_dir() {
            return Err("Invalid path.");
        }
        Ok(p)
    }

    /// Refresh the messages and the Create / Load button (ScriptCreateDialog::_update_dialog).
    fn validate_attach(&mut self) -> bool {
        let Some(a) = self.attach_state() else { return false };
        let (inherits, path, template, msg_parent, msg_path, ok) = (a.inherits, a.path, a.template, a.msg_parent, a.msg_path, a.ok);
        let parent = self.ui.text(inherits).trim().to_string();
        let parent_ok = !parent.is_empty() && (parent.starts_with('"') || parent.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.'));
        let target = self.attach_target(&self.ui.text(path).to_string());
        let exists = target.as_ref().is_ok_and(|p| p.is_file());
        let (pt, pc) = if parent_ok { ("Script path/name is valid.", OK_COLOR) } else { ("Invalid inherited parent name or path.", ERR_COLOR) };
        self.ui.set_text(msg_parent, pt);
        self.ui.set_font_color(msg_parent, Some(pc));
        let (mt, mc) = match &target {
            Ok(_) if exists => ("Will load an existing script file.", OK_COLOR),
            Ok(_) => ("Will create a new script file.", OK_COLOR),
            Err(e) => (*e, ERR_COLOR),
        };
        self.ui.set_text(msg_path, mt);
        self.ui.set_font_color(msg_path, Some(mc));
        self.ui.set_text(ok, if exists { "Load" } else { "Create" });
        // An existing file keeps its own code, so its template does not apply.
        self.ui.set_disabled(template, exists);
        self.ui.set_disabled(inherits, exists);
        let valid = parent_ok && target.is_ok();
        self.ui.set_disabled(ok, !valid);
        valid
    }

    /// Events while "Attach Node Script" is open.
    pub(super) fn attach_event(&mut self, ev: UiEvent) {
        let Some(a) = self.attach_state() else { return };
        let (close, cancel, ok, browse, path, inherits) = (a.close, a.cancel, a.ok, a.browse, a.path, a.inherits);
        match ev {
            UiEvent::Key { key: Key::Escape, .. } => self.close_dialog(),
            UiEvent::Pressed(id) if id == close || id == cancel => self.close_dialog(),
            UiEvent::TextChanged(id) if id == path || id == inherits => {
                self.validate_attach();
            }
            UiEvent::Pressed(id) if id == browse => {
                use crate::file_dialog::{FileDialog, FileMode};
                let current = self.attach_target(&self.ui.text(path).to_string()).ok();
                let dir = current.as_ref().and_then(|p| p.parent()).map(PathBuf::from).unwrap_or_else(|| self.path.clone());
                let mut fd = FileDialog::open(&mut self.ui, &self.icons, FileMode::OpenAny, Some("Open Script / Choose Location"), &dir, Some(&self.path), &[("*.gd", "NekoScript / GDScript")]);
                if let Some(name) = current.as_ref().and_then(|p| p.file_name()) {
                    fd.set_file_name(&mut self.ui, &name.to_string_lossy());
                }
                self.files_dialog = Some((fd, FilePurpose::AttachScriptPath));
            }
            UiEvent::Pressed(id) if id == ok => self.confirm_attach(),
            UiEvent::TextSubmitted(id) if id == path => self.confirm_attach(),
            _ => {}
        }
    }

    /// The file dialog picked a path for the script.
    pub(super) fn attach_path_chosen(&mut self, p: PathBuf) {
        let Some(path) = self.attach_state().map(|a| a.path) else { return };
        let mut p = p;
        if p.is_dir() {
            p = p.join("new_script.gd");
        } else if p.extension().is_none() {
            p.set_extension("gd");
        }
        let text = self.rel(&p);
        self.ui.set_text(path, text);
        self.validate_attach();
    }

    fn confirm_attach(&mut self) {
        if !self.validate_attach() {
            return;
        }
        let Some(a) = self.attach_state() else { return };
        let (node, path, inherits, template) = (a.node, a.path, a.inherits, a.template);
        let tmpl = a.templates.get(self.ui.selected(template)).copied();
        let Ok(file) = self.attach_target(&self.ui.text(path).to_string()) else { return };
        let base = self.ui.text(inherits).trim().to_string();
        if !file.is_file() {
            let stem = file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            let class_name: String = stem.split('_').map(|w| w[..1.min(w.len())].to_uppercase() + &w[1.min(w.len())..]).collect();
            let text = tmpl.map(|t| process_template(t, &base, &class_name)).unwrap_or_else(|| format!("extends {base}\n"));
            if let Err(e) = std::fs::write(&file, text) {
                self.notify(format!("Could not create {}: {e}", self.rel(&file)));
                return;
            }
            // Godot 4.4+ keeps each script's UID in a `.uid` file beside it.
            let uid = neko::core::godot::uid::id_to_text(neko::core::godot::uid::create_id());
            let _ = std::fs::write(PathBuf::from(format!("{}.uid", file.display())), format!("{uid}\n"));
        }
        let uid = std::fs::read_to_string(format!("{}.uid", file.display())).ok().map(|s| s.trim().to_string()).filter(|s| s.starts_with("uid://"));
        let res = self.rel(&file);
        self.close_dialog();
        let doc = self.doc_mut();
        doc.checkpoint(None);
        neko::core::tscn::attach_script(&mut doc.scene, node, &res, uid.as_deref(), &mut neko::core::godot::uid::OsIds);
        self.reload_files();
        self.notify(format!("Attached {res}"));
        // Godot opens the new script in the script editor.
        self.open_script(file);
    }

    /// SceneTreeDock's TOOL_DETACH_SCRIPT.
    pub(super) fn detach_script(&mut self) {
        let doc = self.doc();
        let Some(node) = doc.selected.filter(|&id| doc.has_root && doc.scene.contains(id)) else { return };
        if neko::core::tscn::script_path(&doc.scene, node).is_none() {
            return;
        }
        let doc = self.doc_mut();
        doc.checkpoint(None);
        neko::core::tscn::detach_script(&mut doc.scene, node);
    }

    /// The selected node's script as a file path, if any.
    pub(super) fn selected_script(&self) -> Option<PathBuf> {
        let doc = self.doc();
        let node = doc.selected.filter(|&id| doc.has_root && doc.scene.contains(id))?;
        self.node_script_file(node)
    }

    pub(super) fn node_script_file(&self, node: NodeId) -> Option<PathBuf> {
        let res = neko::core::tscn::script_path(&self.doc().scene, node)?;
        Some(self.path.join(res.strip_prefix("res://")?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snake_case_like_godot() {
        assert_eq!(to_snake_case("Player"), "player");
        assert_eq!(to_snake_case("PlayerBody2D"), "player_body_2d");
        assert_eq!(to_snake_case("HUD"), "hud");
        assert_eq!(to_snake_case("ScoreLabel"), "score_label");
        assert_eq!(to_snake_case("My Node"), "my_node");
    }

    #[test]
    fn templates_closest_first() {
        let t = templates_for("CharacterBody2D");
        assert_eq!(t[0].inherit, "CharacterBody2D");
        assert!(t.iter().any(|t| t.inherit == "Node"));
        assert_eq!(t.last().unwrap().inherit, "Object");
        let s = process_template(t[0], "CharacterBody2D", "Player");
        assert!(s.starts_with("extends CharacterBody2D\n"));
        assert!(s.contains("\tmove_and_slide()"));
        assert!(neko::script::parse(&s).is_ok());
        assert!(neko::script::parse(&process_template(&TEMPLATES[2], "Node2D", "Main")).is_ok());
    }
}
