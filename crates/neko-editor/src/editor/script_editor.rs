//! The Script workspace: Godot's ScriptEditor (editor/script/script_editor_plugin.cpp)
//! with ScriptTextEditor / CodeTextEditor for NekoScript (`.gd`) files.
//!
//! Layout as in Godot: File / Edit / Search / Go To / Debug menus; on the left
//! the open scripts and the current script's methods, each with a filter; the
//! CodeEdit with a find bar under it and a status bar (error, line : column).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use neko::core::NodeId;

use neko_ui::{CodeHandle, Color32, CompletionKind, CompletionOption, Control, Id, Menu, MenuItem, Sides, StyleBox, TextPos, TreeRow, Ui, UiEvent};

use super::dialogs::Dialog;
use super::{Editor, FilePurpose, Workspace};
use crate::node_icons::Icons;
use crate::{script_completion, script_highlight, ui_kit};

// Menu actions (the file dialog uses 9100+).
const FILE_NEW: u32 = 9600;
const FILE_OPEN: u32 = 9601;
const FILE_SAVE: u32 = 9602;
const FILE_SAVE_AS: u32 = 9603;
const FILE_SAVE_ALL: u32 = 9604;
const FILE_COPY_PATH: u32 = 9605;
const FILE_SHOW_IN_FS: u32 = 9606;
const FILE_CLOSE: u32 = 9607;
const FILE_CLOSE_ALL: u32 = 9608;
const FILE_CLOSE_OTHERS: u32 = 9609;
const FILE_RELOAD: u32 = 9610;
const EDIT_UNDO: u32 = 9620;
const EDIT_REDO: u32 = 9621;
const EDIT_CUT: u32 = 9622;
const EDIT_COPY: u32 = 9623;
const EDIT_SELECT_ALL: u32 = 9624;
const EDIT_INDENT: u32 = 9625;
const EDIT_UNINDENT: u32 = 9626;
const EDIT_TOGGLE_COMMENT: u32 = 9627;
const EDIT_DELETE_LINE: u32 = 9628;
const EDIT_DUPLICATE: u32 = 9629;
const EDIT_MOVE_UP: u32 = 9630;
const EDIT_MOVE_DOWN: u32 = 9631;
const EDIT_COMPLETE: u32 = 9632;
const SEARCH_FIND: u32 = 9640;
const SEARCH_FIND_NEXT: u32 = 9641;
const SEARCH_FIND_PREV: u32 = 9642;
const GOTO_LINE: u32 = 9650;

pub(super) const MENU_FIRST: u32 = 9600;
pub(super) const MENU_LAST: u32 = 9699;

/// How long after the last keystroke the script is parsed for errors.
const CHECK_DELAY: Duration = Duration::from_millis(250);

/// An open script.
pub(super) struct ScriptDoc {
    pub path: PathBuf,
    code: CodeHandle,
    /// The CodeEdit control showing it (one per script, only the current is visible).
    control: Id,
    saved_version: u64,
    checked_version: u64,
    changed_at: Instant,
    /// Parse error: line (0-based) and message.
    error: Option<(usize, String)>,
    /// `func` declarations: name and line.
    methods: Vec<(String, usize)>,
    /// Members, functions, parameters, locals and inferred types.
    symbols: script_completion::Symbols,
    /// Show completion again after the next parse (Godot's 0.3s timer).
    auto_complete: bool,
}

impl ScriptDoc {
    pub fn dirty(&self) -> bool {
        self.code.version() != self.saved_version
    }

    fn name(&self) -> String {
        self.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
    }
}

/// Ids of the Script workspace controls.
pub(super) struct ScriptW {
    pub root: Id,
    menus: Id,
    script_filter: Id,
    script_list: Id,
    method_filter: Id,
    method_list: Id,
    method_title: Id,
    code_area: Id,
    empty: Id,
    find_bar: Id,
    find_field: Id,
    find_count: Id,
    find_case: Id,
    find_prev: Id,
    find_next: Id,
    find_close: Id,
    status_error: Id,
    status_pos: Id,
}

pub(super) fn build(ui: &mut Ui, icons: &Icons, parent: Id) -> ScriptW {
    let t = ui.theme.clone();
    let root = ui.add(parent, Control::vbox().separation(0.0).expand().hidden());
    // Menu row.
    let bar_bg = ui.add(root, Control::panel_container().style(StyleBox::flat(t.panel.bg, 0, Sides::symmetric(4.0, 2.0))));
    let bar = ui.add(bar_bg, Control::hbox().separation(2.0));
    let menus = ui.add(bar, Control::menu_bar(Vec::new()).v_center());
    ui.add(bar, Control::spacer());
    ui.add(bar, ui_kit::icon_text_button(icons, "ExternalLink", "Online Docs").disabled(true).tooltip("Open Godot's online documentation. Not available yet."));
    ui.add(bar, ui_kit::icon_text_button(icons, "HelpSearch", "Search Help").disabled(true).tooltip("Search the class reference documentation. Not available yet."));

    let split = ui.add(root, Control::h_split(220.0).expand());
    // Left: scripts and methods.
    let left = ui.add(split, Control::v_split(300.0).min(120.0, 0.0));
    let scripts_col = ui.add(left, Control::vbox().separation(4.0).min(0.0, 80.0));
    let script_filter = ui.add(scripts_col, Control::line_edit("").placeholder("Filter Scripts"));
    let list_bg = ui.add(scripts_col, Control::panel_container().style(t.tree_panel).expand());
    let script_list = ui.add(list_bg, Control::tree());
    let methods_col = ui.add(left, Control::vbox().separation(4.0).min(0.0, 80.0));
    let method_title = ui.add(methods_col, Control::label("").color(t.font_muted_color).ellipsis());
    let method_filter = ui.add(methods_col, Control::line_edit("").placeholder("Filter Methods"));
    let methods_bg = ui.add(methods_col, Control::panel_container().style(t.tree_panel).expand());
    let method_list = ui.add(methods_bg, Control::tree());

    // Right: code, find bar, status bar.
    let right = ui.add(split, Control::vbox().separation(0.0).min(300.0, 0.0));
    let code_area = ui.add(right, Control::vbox().separation(0.0).expand());
    let empty = ui.add(
        code_area,
        Control::label("No script open.\n\nDouble-click a .gd file in the FileSystem dock, or use File > New Script... / Open...")
            .wrap()
            .align(neko_ui::Align::Center)
            .color(t.font_muted_color)
            .expand(),
    );
    // FindReplaceBar.
    let find_bg = ui.add(right, Control::panel_container().style(StyleBox::flat(t.panel.bg, 0, Sides::symmetric(6.0, 3.0))).hidden());
    let find_row = ui.add(find_bg, Control::hbox().separation(4.0));
    let find_field = ui.add(find_row, Control::line_edit("").placeholder("Find").min(240.0, 0.0));
    let find_count = ui.add(find_row, Control::label("").color(t.font_muted_color).v_center());
    let find_prev = ui.add(find_row, ui_kit::icon_button(icons, "MoveUp", "Previous Match (Shift+F3)", None));
    let find_next = ui.add(find_row, ui_kit::icon_button(icons, "MoveDown", "Next Match (F3)", None));
    let find_case = ui.add(find_row, Control::check_box("Match Case", false));
    ui.add(find_row, Control::spacer());
    let find_close = ui.add(find_row, ui_kit::icon_button(icons, "Close", "Hide (Esc)", None));
    // Status bar.
    let status_bg = ui.add(right, Control::panel_container().style(StyleBox::flat(t.panel.bg, 0, Sides::symmetric(8.0, 3.0))));
    let status = ui.add(status_bg, Control::hbox().separation(8.0));
    let status_error = ui.add(status, Control::label("").color(Color32::from_rgb(255, 120, 107)).ellipsis().expand_h());
    let status_pos = ui.add(status, Control::label("").color(t.font_muted_color));

    ScriptW {
        root,
        menus,
        script_filter,
        script_list,
        method_filter,
        method_list,
        method_title,
        code_area,
        empty,
        find_bar: find_bg,
        find_field,
        find_count,
        find_case,
        find_prev,
        find_next,
        find_close,
        status_error,
        status_pos,
    }
}

fn item(label: &str, action: u32, shortcut: &str) -> MenuItem {
    MenuItem::item(label, action).shortcut(shortcut)
}

fn todo(label: &str) -> MenuItem {
    MenuItem::item(format!("{label} (not available yet)"), 0).disabled()
}

/// `func` declarations and their lines, for the Methods panel.
fn methods_of(symbols: &script_completion::Symbols) -> Vec<(String, usize)> {
    symbols.funcs.iter().map(|f| (f.name.clone(), f.line)).collect()
}

/// Every node below `id`, with its `A/B` path and whether it is unique in owner.
fn scene_paths(scene: &neko::core::Scene, id: NodeId, prefix: &str, out: &mut Vec<(NodeId, String, bool)>) {
    let Some(node) = scene.get(id) else { return };
    for &child in node.children() {
        let Some(n) = scene.get(child) else { continue };
        let path = if prefix.is_empty() { n.name.clone() } else { format!("{prefix}/{}", n.name) };
        let unique = n.godot.header.iter().chain(n.godot.props.iter()).any(|(k, v)| k == "unique_name_in_owner" && matches!(v, neko::core::godot::Value::Bool(true)));
        out.push((child, path.clone(), unique));
        scene_paths(scene, child, &path, out);
    }
}

impl Editor {
    fn script(&self) -> Option<&ScriptDoc> {
        self.scripts.get(self.current_script)
    }

    /// Open `path` in the Script workspace (or switch to it if already open).
    pub(super) fn open_script(&mut self, path: PathBuf) {
        self.workspace = Workspace::Script;
        if let Some(i) = self.scripts.iter().position(|s| s.path == path) {
            self.select_script(i);
            return;
        }
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                self.notify(format!("Could not open {}: {e}", self.rel(&path)));
                return;
            }
        };
        let code = CodeHandle::new();
        code.set_text(&text);
        code.set_highlighter(Some(script_highlight::gdscript(script_highlight::members_of(&text))));
        let control = self.ui.add(self.w.script.code_area, Control::code_edit(&code).hidden());
        let v = code.version();
        let symbols = script_completion::analyze(&text);
        self.scripts.push(ScriptDoc {
            path,
            code,
            control,
            saved_version: v,
            checked_version: u64::MAX,
            changed_at: Instant::now() - CHECK_DELAY,
            error: None,
            methods: methods_of(&symbols),
            symbols,
            auto_complete: false,
        });
        self.select_script(self.scripts.len() - 1);
    }

    fn select_script(&mut self, i: usize) {
        if i >= self.scripts.len() {
            return;
        }
        self.current_script = i;
        for (j, s) in self.scripts.iter().enumerate() {
            if j != i {
                s.code.cancel_completion();
            }
            self.ui.set_visible(s.control, j == i);
        }
        self.ui.focus(self.scripts[i].control);
    }

    pub(super) fn save_script(&mut self, i: usize) -> bool {
        let Some(s) = self.scripts.get(i) else { return false };
        let text = s.code.text();
        let path = s.path.clone();
        match std::fs::write(&path, text) {
            Ok(()) => {
                let v = self.scripts[i].code.version();
                self.scripts[i].saved_version = v;
                self.notify(format!("Saved {}", self.rel(&path)));
                true
            }
            Err(e) => {
                self.notify(format!("Could not save {}: {e}", self.rel(&path)));
                false
            }
        }
    }

    /// Save every modified script (Godot saves scripts before running the game).
    pub(super) fn save_all_scripts(&mut self) {
        for i in 0..self.scripts.len() {
            if self.scripts[i].dirty() {
                self.save_script(i);
            }
        }
    }

    pub(super) fn scripts_dirty(&self) -> bool {
        self.scripts.iter().any(ScriptDoc::dirty)
    }

    /// Close a script; a modified one asks first, like Godot.
    fn close_script(&mut self, i: usize, force: bool) {
        let Some(s) = self.scripts.get(i) else { return };
        if s.dirty() && !force {
            let name = s.name();
            self.close_dialog();
            let (modal, body, close) = ui_kit::dialog(&mut self.ui, "Save Changes?", 420.0);
            self.ui.add(body, Control::label(format!("Close and save changes?\n\"{name}\"")).wrap());
            let row = self.ui.add(body, Control::hbox().separation(12.0));
            self.ui.add(row, Control::spacer());
            let cancel = self.ui.add(row, Control::button("Cancel").min(96.0, 0.0));
            let discard = self.ui.add(row, Control::button("Discard").min(96.0, 0.0));
            let save = self.ui.add(row, Control::button("Save").primary().min(96.0, 0.0));
            self.ui.add(row, Control::spacer());
            self.dialog = Some(Dialog::CloseScript { modal, close, save, discard, cancel, index: i });
            return;
        }
        let s = self.scripts.remove(i);
        self.ui.remove(s.control);
        if self.current_script >= self.scripts.len() {
            self.current_script = self.scripts.len().saturating_sub(1);
        }
        if !self.scripts.is_empty() {
            self.select_script(self.current_script);
        }
    }

    pub(super) fn close_script_answer(&mut self, index: usize, save: bool) {
        if save && !self.save_script(index) {
            return;
        }
        self.close_script(index, true);
    }

    fn reload_script(&mut self, i: usize) {
        let Some(s) = self.scripts.get(i) else { return };
        match std::fs::read_to_string(&s.path) {
            Ok(text) => {
                s.code.set_text(&text);
                let v = s.code.version();
                self.scripts[i].saved_version = v;
                self.scripts[i].symbols = script_completion::analyze(&text);
                self.scripts[i].checked_version = u64::MAX;
            }
            Err(e) => self.notify(format!("Could not reload: {e}")),
        }
    }

    // ------------------------------------------------------------ menus

    pub(super) fn script_menus(&self) -> Vec<Menu> {
        let has = self.script().is_some();
        let dirty = self.script().is_some_and(ScriptDoc::dirty);
        let file = vec![
            item("New Script...", FILE_NEW, "Ctrl+N"),
            item("Open...", FILE_OPEN, ""),
            MenuItem::separator(),
            item("Save", FILE_SAVE, "Ctrl+Alt+S").enabled(has),
            item("Save As...", FILE_SAVE_AS, "").enabled(has),
            item("Save All", FILE_SAVE_ALL, "Shift+Alt+S").enabled(self.scripts_dirty()),
            MenuItem::separator(),
            item("Reload Script", FILE_RELOAD, "").enabled(has && !dirty),
            item("Copy File Path", FILE_COPY_PATH, "").enabled(has),
            item("Show in FileSystem", FILE_SHOW_IN_FS, "").enabled(has),
            MenuItem::separator(),
            item("Close", FILE_CLOSE, "Ctrl+W").enabled(has),
            item("Close All", FILE_CLOSE_ALL, "").enabled(has),
            item("Close Other Tabs", FILE_CLOSE_OTHERS, "").enabled(self.scripts.len() > 1),
        ];
        let edit = vec![
            item("Undo", EDIT_UNDO, "Ctrl+Z").enabled(has),
            item("Redo", EDIT_REDO, "Ctrl+Y").enabled(has),
            MenuItem::separator(),
            item("Cut", EDIT_CUT, "Ctrl+X").enabled(has),
            item("Copy", EDIT_COPY, "Ctrl+C").enabled(has),
            item("Select All", EDIT_SELECT_ALL, "Ctrl+A").enabled(has),
            MenuItem::separator(),
            item("Move Up", EDIT_MOVE_UP, "Alt+Up").enabled(has),
            item("Move Down", EDIT_MOVE_DOWN, "Alt+Down").enabled(has),
            item("Indent", EDIT_INDENT, "").enabled(has),
            item("Unindent", EDIT_UNINDENT, "Shift+Tab").enabled(has),
            item("Delete Line", EDIT_DELETE_LINE, "Ctrl+Shift+K").enabled(has),
            item("Toggle Comment", EDIT_TOGGLE_COMMENT, "Ctrl+K").enabled(has),
            item("Duplicate Selection", EDIT_DUPLICATE, "Ctrl+Shift+D").enabled(has),
            MenuItem::separator(),
            item("Complete Symbol", EDIT_COMPLETE, "Ctrl+Space").enabled(has),
        ];
        let search = vec![
            item("Find...", SEARCH_FIND, "Ctrl+F").enabled(has),
            item("Find Next", SEARCH_FIND_NEXT, "F3").enabled(has),
            item("Find Previous", SEARCH_FIND_PREV, "Shift+F3").enabled(has),
            todo("Replace..."),
            MenuItem::separator(),
            todo("Find in Files..."),
        ];
        let goto = vec![item("Go to Line...", GOTO_LINE, "Ctrl+G").enabled(has), todo("Go to Function...")];
        let debug = vec![todo("Step Into"), todo("Step Over"), todo("Break"), todo("Continue")];
        let menu = |title: &str, items| Menu { title: title.into(), items };
        vec![menu("File", file), menu("Edit", edit), menu("Search", search), menu("Go To", goto), menu("Debug", debug)]
    }

    pub(super) fn script_menu_action(&mut self, action: u32) {
        let cur = self.current_script;
        let code = self.script().map(|s| s.code.clone());
        let with = |f: fn(&mut neko_ui::CodeState)| {
            if let Some(c) = &code {
                c.with(f);
            }
        };
        match action {
            FILE_NEW => {
                let dir = self.script().and_then(|s| s.path.parent().map(PathBuf::from)).unwrap_or_else(|| self.path.clone());
                self.open_file_dialog(super::filesystem::FileOp::NewScript { dir });
            }
            FILE_OPEN => {
                use crate::file_dialog::{FileDialog, FileMode};
                self.close_dialog();
                let fd = FileDialog::open(&mut self.ui, &self.icons, FileMode::OpenFile, Some("Open File"), &self.path, Some(&self.path), &[("*.gd", "NekoScript / GDScript")]);
                self.files_dialog = Some((fd, FilePurpose::OpenScript));
            }
            FILE_SAVE => {
                self.save_script(cur);
            }
            FILE_SAVE_AS => {
                use crate::file_dialog::{FileDialog, FileMode};
                let Some(s) = self.script() else { return };
                let dir = s.path.parent().map(PathBuf::from).unwrap_or_else(|| self.path.clone());
                let name = s.name();
                self.close_dialog();
                let mut fd = FileDialog::open(&mut self.ui, &self.icons, FileMode::SaveFile, Some("Save Script As..."), &dir, Some(&self.path), &[("*.gd", "NekoScript / GDScript")]);
                fd.set_file_name(&mut self.ui, &name);
                self.files_dialog = Some((fd, FilePurpose::SaveScriptAs(cur)));
            }
            FILE_SAVE_ALL => self.save_all_scripts(),
            FILE_RELOAD => self.reload_script(cur),
            FILE_COPY_PATH => {
                if let Some(s) = self.script() {
                    self.clipboard = Some(self.rel(&s.path));
                }
            }
            FILE_SHOW_IN_FS => {
                if let Some(p) = self.script().map(|s| s.path.clone()) {
                    self.show_in_filesystem(p);
                }
            }
            FILE_CLOSE => self.close_script(cur, false),
            FILE_CLOSE_ALL => {
                for i in (0..self.scripts.len()).rev() {
                    if !self.scripts[i].dirty() {
                        self.close_script(i, true);
                    }
                }
                if !self.scripts.is_empty() {
                    self.close_script(self.current_script.min(self.scripts.len() - 1), false);
                }
            }
            FILE_CLOSE_OTHERS => {
                for i in (0..self.scripts.len()).rev() {
                    if i != cur && !self.scripts[i].dirty() {
                        self.close_script(i, true);
                    }
                }
            }
            EDIT_UNDO => with(|s| {
                s.undo();
            }),
            EDIT_REDO => with(|s| {
                s.redo();
            }),
            EDIT_CUT => with(neko_ui::CodeState::cut),
            EDIT_COPY => with(neko_ui::CodeState::copy),
            EDIT_SELECT_ALL => with(neko_ui::CodeState::select_all),
            EDIT_INDENT => with(neko_ui::CodeState::indent),
            EDIT_UNINDENT => with(neko_ui::CodeState::unindent),
            EDIT_TOGGLE_COMMENT => with(neko_ui::CodeState::toggle_comment),
            EDIT_DELETE_LINE => with(neko_ui::CodeState::delete_lines),
            EDIT_DUPLICATE => with(neko_ui::CodeState::duplicate),
            EDIT_MOVE_UP => with(|s| s.move_lines(false)),
            EDIT_MOVE_DOWN => with(|s| s.move_lines(true)),
            EDIT_COMPLETE => self.update_script_completion(cur, true),
            SEARCH_FIND => self.show_find(),
            SEARCH_FIND_NEXT => self.find_step(true),
            SEARCH_FIND_PREV => self.find_step(false),
            GOTO_LINE => self.open_goto_line(),
            _ => {}
        }
        if let Some(s) = self.script() {
            self.ui.focus(s.control);
        }
    }

    // ------------------------------------------------------------ find / go to

    fn show_find(&mut self) {
        let w = &self.w.script;
        let (bar, field) = (w.find_bar, w.find_field);
        // Start with the selected text, like Godot.
        if let Some(sel) = self.script().and_then(|s| s.code.with(|c| c.selected_text())).filter(|t| !t.contains('\n')) {
            self.ui.set_text(field, sel);
        }
        self.ui.set_visible(bar, true);
        self.ui.focus(field);
    }

    fn find_step(&mut self, forward: bool) {
        let needle = self.ui.text(self.w.script.find_field).to_string();
        if needle.is_empty() {
            self.show_find();
            return;
        }
        let case = self.ui.is_checked(self.w.script.find_case);
        if let Some(s) = self.script() {
            let found = s.code.with(|c| c.find(&needle, forward, case));
            if !found {
                self.notify(format!("No match for \"{needle}\"."));
            }
        }
    }

    fn open_goto_line(&mut self) {
        let Some(s) = self.script() else { return };
        let total = s.code.line_count();
        let current = s.code.caret().line + 1;
        self.close_dialog();
        let (modal, body, close) = ui_kit::dialog(&mut self.ui, "Go to Line", 320.0);
        self.ui.add(body, Control::label(format!("Line Number (1 - {total}):")));
        let field = self.ui.add(body, Control::line_edit(current.to_string()));
        self.ui.focus(field);
        let (ok, cancel) = ui_kit::ok_cancel(&mut self.ui, body, "OK", "Cancel");
        self.dialog = Some(Dialog::GoToLine { modal, close, field, ok, cancel });
    }

    pub(super) fn go_to_line_answer(&mut self, text: &str) {
        if let (Some(s), Ok(n)) = (self.script(), text.trim().parse::<usize>()) {
            s.code.go_to_line(n.saturating_sub(1));
            let id = s.control;
            self.ui.focus(id);
        }
    }

    // ------------------------------------------------------- code completion

    /// Ask [`script_completion`] for the caret context and give the options to
    /// the CodeEdit, which filters and draws the popup. `forced` is Ctrl+Space.
    pub(super) fn update_script_completion(&mut self, i: usize, forced: bool) {
        let Some(s) = self.scripts.get(i) else { return };
        let text = s.code.text();
        let caret = s.code.caret();
        let line = s.code.line_text(caret.line);
        let wants_files = line.contains("preload(") || line.contains("load(") || line.trim_start().starts_with("extends \"");
        let wants_nodes = line.contains('$') || line.contains('%');
        let (nodes, unique) = if wants_nodes { self.scene_completion_paths(&s.path) } else { (Vec::new(), Vec::new()) };
        let files: Vec<String> = if wants_files {
            self.files
                .iter()
                .filter(|f| {
                    f.is_dir
                        || matches!(f.path.extension().and_then(|e| e.to_str()), Some("gd" | "tscn" | "tres" | "png" | "svg" | "jpg" | "wav" | "ogg" | "mp3" | "json" | "txt" | "gdshader" | "res" | "scn"))
                })
                .map(|f| {
                    let mut url = self.rel(&f.path);
                    if f.is_dir && !url.ends_with('/') {
                        url.push('/');
                    }
                    url
                })
                .collect()
        } else {
            Vec::new()
        };
        let sources = script_completion::Sources { files: &files, nodes: &nodes, unique: &unique };
        // Analyze the text as it is now: Ctrl+Space may come before the idle
        // parse check picked up the last declaration.
        let symbols = script_completion::analyze(&text);
        let (options, base) = script_completion::options_with(&symbols, &text, caret, &sources, forced);
        if options.is_empty() {
            s.code.cancel_completion();
            return;
        }
        let options = options.into_iter().map(|o| self.completion_icon(o)).collect();
        s.code.update_completion(options, base, forced);
    }

    /// `$Path`: children of the node the script is attached to, and the
    /// scene's `%UniqueName`s (as GDScript's `get_node` sees them).
    fn scene_completion_paths(&self, script: &Path) -> (Vec<String>, Vec<String>) {
        let doc = self.doc();
        let scene = &doc.scene;
        let mut all = Vec::new();
        scene_paths(scene, scene.root(), "", &mut all);
        let want = self.rel(script);
        let origin = all.iter().find(|(id, _, _)| neko::core::tscn::script_path(scene, *id).as_deref() == Some(want.as_str())).map(|(_, p, _)| p.clone()).unwrap_or_default();
        let prefix = if origin.is_empty() { String::new() } else { format!("{origin}/") };
        let nodes = all
            .iter()
            .filter_map(|(_, p, _)| if origin.is_empty() { Some(p.clone()) } else { p.strip_prefix(&prefix).map(str::to_string) })
            .collect();
        let unique = all.iter().filter(|(_, _, u)| *u).filter_map(|(_, p, _)| p.rsplit('/').next().map(str::to_string)).collect();
        (nodes, unique)
    }

    /// Godot's `CodeTextEditor::_get_completion_icon`.
    fn completion_icon(&self, mut option: CompletionOption) -> CompletionOption {
        option.icon = match option.kind {
            CompletionKind::Class => self.icons.class(&option.display),
            CompletionKind::Enum => self.icons.get("Enum"),
            CompletionKind::Function => self.icons.get("MemberMethod"),
            CompletionKind::Member => self.icons.get("MemberProperty"),
            CompletionKind::Variable => self.icons.get("LocalVariable"),
            CompletionKind::Constant => self.icons.get("MemberConstant"),
            CompletionKind::Signal => self.icons.get("MemberSignal"),
            CompletionKind::Keyword => self.icons.get("Keyword"),
            CompletionKind::Annotation => self.icons.get("MemberAnnotation"),
            CompletionKind::FilePath => self.icons.get("File"),
            CompletionKind::NodePath => self.icons.get("NodePath"),
            CompletionKind::Plain => None,
        };
        option
    }

    // ------------------------------------------------------------ events and sync

    /// Events of the Script workspace controls; true when used.
    pub(super) fn script_event(&mut self, ev: &UiEvent) -> bool {
        let w = &self.w.script;
        let (list, methods, field, case, prev, next, close_find, sfilter, mfilter, find_bar) =
            (w.script_list, w.method_list, w.find_field, w.find_case, w.find_prev, w.find_next, w.find_close, w.script_filter, w.method_filter, w.find_bar);
        match *ev {
            UiEvent::TreeSelected(t, key) if t == list => self.select_script(key as usize),
            UiEvent::TreeSelected(t, key) | UiEvent::TreeActivated(t, key) if t == methods => {
                if let Some(s) = self.script() {
                    s.code.go_to_line(key as usize);
                    let id = s.control;
                    self.ui.focus(id);
                }
            }
            UiEvent::TextChanged(id) if id == field => {
                // Godot's FindReplaceBar searches while typing.
                let needle = self.ui.text(field).to_string();
                if let Some(s) = self.script() {
                    let case = self.ui.is_checked(case);
                    s.code.with(|c| {
                        if let Some((a, _)) = c.selection() {
                            c.select(a, a);
                        }
                        c.find(&needle, true, case)
                    });
                }
            }
            UiEvent::TextSubmitted(id) if id == field => self.find_step(true),
            UiEvent::Pressed(id) if id == next => self.find_step(true),
            UiEvent::Pressed(id) if id == prev => self.find_step(false),
            UiEvent::Pressed(id) if id == close_find => {
                self.ui.set_visible(find_bar, false);
                if let Some(s) = self.script() {
                    let c = s.control;
                    self.ui.focus(c);
                }
            }
            UiEvent::TextChanged(id) if id == sfilter || id == mfilter => {}
            UiEvent::TextChanged(id) if self.scripts.iter().any(|s| s.control == id) => {
                if let Some(s) = self.scripts.iter_mut().find(|s| s.control == id) {
                    s.changed_at = Instant::now();
                    // Godot only opens completion after insertions.
                    s.auto_complete = s.code.last_was_insert();
                }
            }
            _ => return false,
        }
        true
    }

    /// Script workspace shortcuts (keys the CodeEdit did not use).
    pub(super) fn script_shortcut(&mut self, key: neko::platform::Key, m: neko::platform::Modifiers) -> bool {
        use neko::platform::Key;
        if self.workspace != Workspace::Script {
            return false;
        }
        let cmd = m.ctrl || m.logo;
        let action = match key {
            Key::Char('s') if cmd && m.alt => FILE_SAVE,
            Key::Char('s') if m.shift && m.alt => FILE_SAVE_ALL,
            // Ctrl+S saves the current script while editing one (and the scenes, as usual).
            Key::Char('s') if cmd && !m.shift && !self.scripts.is_empty() => {
                self.save_script(self.current_script);
                return false;
            }
            Key::Space if cmd => EDIT_COMPLETE,
            Key::Char('f') if cmd => SEARCH_FIND,
            Key::F(3) if m.shift => SEARCH_FIND_PREV,
            Key::F(3) => SEARCH_FIND_NEXT,
            Key::Char('g') if cmd => GOTO_LINE,
            Key::Char('w') if cmd && !self.scripts.is_empty() => FILE_CLOSE,
            Key::Char('n') if cmd && !m.shift => FILE_NEW,
            Key::Escape if self.ui.is_visible(self.w.script.find_bar) => {
                self.ui.set_visible(self.w.script.find_bar, false);
                if let Some(s) = self.script() {
                    let c = s.control;
                    self.ui.focus(c);
                }
                return true;
            }
            _ => return false,
        };
        self.script_menu_action(action);
        true
    }

    /// Parse modified scripts (after a short pause) and refresh the panels.
    pub(super) fn sync_scripts(&mut self) {
        let active = self.workspace == Workspace::Script;
        self.ui.set_visible(self.w.script.root, active);
        if !active {
            return;
        }
        // Clipboard from Copy/Cut.
        for s in &self.scripts {
            if let Some(t) = s.code.take_clipboard() {
                self.clipboard = Some(t);
            }
        }
        let now = Instant::now();
        let mut complete = Vec::new();
        for (i, s) in self.scripts.iter_mut().enumerate() {
            let v = s.code.version();
            if v != s.checked_version && now - s.changed_at >= CHECK_DELAY {
                s.checked_version = v;
                let text = s.code.text();
                s.symbols = script_completion::analyze(&text);
                s.methods = methods_of(&s.symbols);
                s.error = match neko::script::parse(&text) {
                    Ok(_) => None,
                    Err(e) => Some(((e.line as usize).saturating_sub(1), e.message)),
                };
                s.code.set_marked_line(s.error.as_ref().map(|(l, _)| *l));
                s.code.set_highlighter(Some(script_highlight::gdscript(script_highlight::members_of(&text))));
                if s.auto_complete {
                    s.auto_complete = false;
                    complete.push(i);
                }
            }
        }
        for i in complete {
            if i == self.current_script {
                self.update_script_completion(i, false);
            }
        }
        let w = &self.w.script;
        let (menus, list, methods, title, empty, status_error, status_pos, sfilter, mfilter) =
            (w.menus, w.script_list, w.method_list, w.method_title, w.empty, w.status_error, w.status_pos, w.script_filter, w.method_filter);
        let m = self.script_menus();
        self.ui.set_menus(menus, m);
        self.ui.set_visible(empty, self.scripts.is_empty());

        let sf = self.ui.text(sfilter).to_lowercase();
        let icon = self.icons.get("Script");
        let rows: Vec<TreeRow> = self
            .scripts
            .iter()
            .enumerate()
            .filter(|(_, s)| sf.is_empty() || s.name().to_lowercase().contains(&sf))
            .map(|(i, s)| {
                let mut r = TreeRow::new(i as u64, 0, if s.dirty() { format!("{}(*)", s.name()) } else { s.name() });
                r.icon = icon;
                r
            })
            .collect();
        self.ui.set_rows(list, rows);
        self.ui.set_tree_selected(list, (!self.scripts.is_empty()).then_some(self.current_script as u64));

        let mf = self.ui.text(mfilter).to_lowercase();
        let (mrows, name, err, pos) = match self.script() {
            Some(s) => {
                let func_icon = self.icons.get("MemberMethod");
                let rows = s
                    .methods
                    .iter()
                    .filter(|(n, _)| mf.is_empty() || n.to_lowercase().contains(&mf))
                    .map(|(n, line)| {
                        let mut r = TreeRow::new(*line as u64, 0, n.clone());
                        r.icon = func_icon;
                        r
                    })
                    .collect();
                let TextPos { line, col } = s.code.caret();
                let err = s.error.as_ref().map(|(l, m)| format!("Line {}: {m}", l + 1)).unwrap_or_default();
                (rows, s.name(), err, format!("{:>4} : {:<3}", line + 1, col + 1))
            }
            None => (Vec::new(), String::new(), String::new(), String::new()),
        };
        self.ui.set_rows(methods, mrows);
        // FindReplaceBar's match count.
        let (bar, field, count, case) = (self.w.script.find_bar, self.w.script.find_field, self.w.script.find_count, self.w.script.find_case);
        if self.ui.is_visible(bar) {
            let needle = self.ui.text(field).to_string();
            let n = self.script().map_or(0, |s| s.code.with(|c| c.count(&needle, self.ui.is_checked(case))));
            let text = match (needle.is_empty(), n) {
                (true, _) => String::new(),
                (false, 0) => "No match".to_string(),
                (false, 1) => "1 match".to_string(),
                (false, n) => format!("{n} matches"),
            };
            self.ui.set_text(count, text);
        }
        self.ui.set_text(title, name);
        self.ui.set_text(status_error, err);
        self.ui.set_text(status_pos, pos);
    }

    /// Whether a script still needs to be parsed (keep redrawing until then).
    pub(super) fn scripts_pending(&self) -> bool {
        self.workspace == Workspace::Script && self.scripts.iter().any(|s| s.code.version() != s.checked_version)
    }

    /// Save As for a script: write it to `path` and keep editing there.
    pub(super) fn save_script_as(&mut self, i: usize, path: PathBuf) {
        if i >= self.scripts.len() {
            return;
        }
        self.scripts[i].path = path;
        self.save_script(i);
        self.reload_files();
    }
}
