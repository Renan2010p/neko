//! The scene editor, laid out like Godot's (`editor/editor_node.cpp`): menus,
//! workspaces and run buttons on top; Scene and FileSystem docks on the left;
//! scene tabs, viewport toolbar and viewport in the middle with the bottom
//! panel under it; Inspector / Signals / Groups on the right.
//!
//! Everything is drawn with `neko-ui`, Neko's own GUI: the controls are built
//! once and updated from the editor state every frame.

mod attach_script;
mod dialogs;
mod docks;
mod filesystem;
mod inspector;
mod menus;
mod renderer;
mod script_editor;
mod viewport2d;
mod viewport3d;

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use neko::core::{NodeId, Scene, format};
use neko::platform::{Key, Modifiers};
use neko::project::{ProjectConfig, RenderingDriver};
use neko_ui::{Anchors, Control, Id, Sides, StyleBox, Tab, Theme, TreeRow, Ui, UiEvent, UiOutput, Vec2, vec2};
use serde::{Deserialize, Serialize};

use crate::create_node::{CreateNodeDialog, Outcome};
use crate::node_icons::Icons;
use crate::ui_kit;

/// Size of the game window in the 2D viewport (a project setting later).
const GAME_SIZE: Vec2 = vec2(1152.0, 648.0);
const MAX_UNDO: usize = 100;

/// Per-project editor state, stored in `.neko/editor.toml` (git-ignored).
#[derive(Default, Serialize, Deserialize)]
struct Prefs {
    #[serde(default)]
    favorites: Vec<String>,
    #[serde(default)]
    recents: Vec<String>,
    #[serde(default)]
    recent_scenes: Vec<String>,
}

#[derive(Clone)]
struct Snapshot {
    scene: Scene,
    has_root: bool,
    selected: Option<NodeId>,
}

/// One open scene (a tab).
pub(crate) struct Doc {
    /// Absolute path; `None` until first saved.
    file: Option<PathBuf>,
    scene: Scene,
    /// A new scene has no root until the user picks one.
    has_root: bool,
    dirty: bool,
    selected: Option<NodeId>,
    collapsed: HashSet<NodeId>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    last_edit: Option<(String, Instant)>,
    pan: Vec2,
    zoom: f32,
    warnings: Vec<String>,
}

impl Doc {
    fn empty() -> Self {
        let mut d = Self::with_scene(None, Scene::new("Node", "Node").expect("Node is valid"));
        d.has_root = false;
        d.selected = None;
        d
    }

    fn with_scene(file: Option<PathBuf>, scene: Scene) -> Self {
        let zoom = 0.5;
        Self {
            file,
            selected: Some(scene.root()),
            scene,
            has_root: true,
            dirty: false,
            collapsed: HashSet::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            last_edit: None,
            pan: -GAME_SIZE * zoom / 2.0,
            zoom,
            warnings: Vec::new(),
        }
    }

    fn load(file: PathBuf) -> Result<Self, String> {
        let text = std::fs::read_to_string(&file).map_err(|e| e.to_string())?;
        let loaded = format::from_str(&text).map_err(|e| e.to_string())?;
        let mut d = Self::with_scene(Some(file), loaded.scene);
        d.warnings = loaded.warnings;
        Ok(d)
    }

    /// Tab title, Godot style: `[empty]`, the file stem, `(*)` when unsaved.
    fn title(&self) -> String {
        let name = match (&self.file, self.has_root) {
            (_, false) => "[empty]".into(),
            (Some(f), true) => f.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
            (None, true) => self.scene.get(self.scene.root()).map(|n| n.name.clone()).unwrap_or_default(),
        };
        if self.dirty { format!("{name}(*)") } else { name }
    }

    /// Record the current state before a change. Changes with the same `key`
    /// within a second (e.g. dragging a value) become one undo step.
    fn checkpoint(&mut self, key: Option<&str>) {
        if let (Some(k), Some((last, at))) = (key, &self.last_edit)
            && k == last
            && at.elapsed() < Duration::from_secs(1)
        {
            self.last_edit = Some((k.to_string(), Instant::now()));
            self.dirty = true;
            return;
        }
        self.undo.push(Snapshot { scene: self.scene.clone(), has_root: self.has_root, selected: self.selected });
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.last_edit = key.map(|k| (k.to_string(), Instant::now()));
        self.dirty = true;
    }

    fn restore(&mut self, s: Snapshot) -> Snapshot {
        let current = Snapshot { scene: std::mem::replace(&mut self.scene, s.scene), has_root: self.has_root, selected: self.selected };
        self.has_root = s.has_root;
        self.selected = s.selected.filter(|&id| self.scene.contains(id));
        self.dirty = true;
        self.last_edit = None;
        current
    }

    fn undo(&mut self) -> bool {
        let Some(s) = self.undo.pop() else { return false };
        let cur = self.restore(s);
        self.redo.push(cur);
        true
    }

    fn redo(&mut self) -> bool {
        let Some(s) = self.redo.pop() else { return false };
        let cur = self.restore(s);
        self.undo.push(cur);
        true
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Workspace {
    TwoD,
    ThreeD,
    Script,
    Game,
    AssetLib,
}

const WORKSPACES: [(Workspace, &str, &str); 5] = [
    (Workspace::TwoD, "2D", "2D"),
    (Workspace::ThreeD, "3D", "3D"),
    (Workspace::Script, "Script", "Script"),
    (Workspace::Game, "Game", "Game"),
    (Workspace::AssetLib, "AssetLib", "AssetStore"),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum BottomTab {
    Output,
    Debugger,
    Audio,
    Animation,
    Shader,
}

const BOTTOM_TABS: [(BottomTab, &str); 5] =
    [(BottomTab::Output, "Output"), (BottomTab::Debugger, "Debugger"), (BottomTab::Audio, "Audio"), (BottomTab::Animation, "Animation"), (BottomTab::Shader, "Shader Editor")];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool {
    Select,
    Move,
    Rotate,
    Scale,
}

const TOOLS: [(Tool, &str, &str); 4] = [
    (Tool::Select, "ToolSelect", "Select Mode (Q)"),
    (Tool::Move, "ToolMove", "Move Mode (W)"),
    (Tool::Rotate, "ToolRotate", "Rotate Mode (E)"),
    (Tool::Scale, "ToolScale", "Scale Mode (S)"),
];

/// What the open file dialog is for.
pub(super) enum FilePurpose {
    OpenScene,
    SaveSceneAs(usize),
    /// Copy the chosen paths into this project folder.
    Import(PathBuf),
    /// Script editor: File > Open...
    OpenScript,
    /// "Attach Node Script": the Path field's browse button.
    AttachScriptPath,
    /// Script editor: File > Save As... for this script.
    SaveScriptAs(usize),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pending {
    Back,
    Quit,
    CloseTab(usize),
}

/// Who the Create New Node dialog is for.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CreateFor {
    Child,
    Root,
}

/// Orbit camera of the 3D viewport.
#[derive(Clone, Copy)]
struct Orbit {
    target: [f32; 3],
    yaw: f32,
    pitch: f32,
    distance: f32,
}

impl Default for Orbit {
    fn default() -> Self {
        Self { target: [0.0; 3], yaw: 0.75, pitch: 0.45, distance: 9.0 }
    }
}

struct FileEntry {
    path: PathBuf,
    depth: usize,
    is_dir: bool,
}

/// What the FileSystem dock is showing: a project folder, or the favorites
/// list that Godot puts above the project tree.
#[derive(Clone, PartialEq, Eq)]
enum FsView {
    Favorites,
    Dir(PathBuf),
}

/// Ids of the editor's fixed controls.
struct W {
    menubar: Id,
    attach_script: Id,
    detach_script: Id,
    /// The Script workspace.
    script: script_editor::ScriptW,
    /// The renderer selector at the right of the title bar.
    renderer: Id,
    workspaces: Vec<Id>,
    play: Id,
    play_scene: Id,
    stop: Id,
    left_tabs: Id,
    scene_panel: Id,
    import_panel: Id,
    add_node: Id,
    node_filter: Id,
    create_root: Id,
    root_buttons: Vec<(Id, Option<&'static str>)>,
    scene_tree: Id,
    file_back: Id,
    file_forward: Id,
    file_path: Id,
    file_split: Id,
    fs_import: Id,
    file_filter: Id,
    file_sort: Id,
    file_tree: Id,
    file_list: Id,
    fs_split: Id,
    file_list_panel: Id,
    scene_tabs: Id,
    view_box: Id,
    tools: Vec<Id>,
    snap: Id,
    grid: Id,
    view_menus: Id,
    viewport: Id,
    placeholder: Id,
    bottom_panel: Id,
    output_tree: Id,
    bottom_note: Id,
    bottom_tabs: Vec<Id>,
    running: Id,
    right_tabs: Id,
    inspector_panel: Id,
    signals_panel: Id,
    groups_panel: Id,
    insp_filter: Id,
    insp_icon: Id,
    insp_name: Id,
    insp_class: Id,
    insp_body: Id,
    toast: Id,
    toast_label: Id,
}

pub struct Editor {
    pub ui: Ui,
    icons: Icons,
    w: W,
    pub path: PathBuf,
    config: Result<ProjectConfig, String>,
    docs: Vec<Doc>,
    current: usize,
    workspace: Workspace,
    bottom: Option<BottomTab>,
    tool: Tool,
    snap: bool,
    show_grid: bool,
    show_collisions: bool,
    /// Debug > Visible Collision Shapes: passes `--debug-collisions` to the game.
    debug_collisions: bool,
    /// What the running game was started with (main scene or the current one).
    run_current: bool,
    files: Vec<FileEntry>,
    files_scanned: Option<Instant>,
    collapsed_dirs: HashSet<PathBuf>,
    file_keys: HashMap<u64, PathBuf>,
    list_keys: HashMap<u64, PathBuf>,
    /// The folder (or Favorites) the dock is showing, with Godot's history.
    fs_view: FsView,
    fs_history: Vec<FsView>,
    fs_history_pos: usize,
    fs_show_list: bool,
    fs_favorites_open: bool,
    fs_sort_types: bool,
    /// The selected file or folder, kept across rescans.
    fs_selected: Option<PathBuf>,
    /// Last path written into the path bar, so typing isn't overwritten.
    fs_path_shown: String,
    create: Option<(CreateNodeDialog, CreateFor)>,
    dialog: Option<dialogs::Dialog>,
    /// Godot's FileDialog (Open Scene, Save Scene As, Import Files) and its purpose.
    files_dialog: Option<(crate::file_dialog::FileDialog, FilePurpose)>,
    prefs: Prefs,
    dragging_node: bool,
    drag_moved: bool,
    drag_acc: Vec2,
    orbit: Orbit,
    hits2d: Vec<(NodeId, neko_ui::Rect)>,
    hits3d: Vec<(NodeId, neko_ui::Pos2)>,
    inspector: inspector::State,
    output: Arc<Mutex<Vec<String>>>,
    output_shown: usize,
    toast_until: Option<Instant>,
    run: Option<Child>,
    fullscreen: bool,
    /// Text the app should put on the system clipboard.
    pub clipboard: Option<String>,
    /// File the FileSystem context menu was opened on.
    file_context: Option<PathBuf>,
    /// Whether the context menu was opened on a row (not on empty space).
    fs_context_item: bool,
    /// Window requests the app applies after the frame.
    pub fullscreen_request: Option<bool>,
    pub back: bool,
    pub quit: bool,
    /// "Save & Restart" after a renderer change: the app restarts the editor.
    pub restart: bool,
    /// Scripts open in the Script workspace.
    scripts: Vec<script_editor::ScriptDoc>,
    current_script: usize,
    /// What the editor is drawing with (may differ from the project's choice
    /// after a command-line override or a fallback).
    running_driver: Option<RenderingDriver>,
}

impl Editor {
    pub fn open(path: PathBuf) -> Self {
        // Godot projects and Neko 0.1 projects are converted on open.
        let migrated = neko::project::migrate::prepare_project(&path, crate::util::engine_checkout().as_deref());
        let config = ProjectConfig::load(&path).map_err(|e| e.to_string());
        let prefs: Prefs = std::fs::read_to_string(path.join(".neko/editor.toml")).ok().and_then(|t| toml::from_str(&t).ok()).unwrap_or_default();
        let mut ui = Ui::new(Theme::neko_dark());
        ui.set_native_windows(crate::util::native_dialogs());
        let icons = Icons::load(&mut ui);
        let w = build(&mut ui, &icons);
        let mut ed = Self {
            ui,
            icons,
            w,
            path: path.clone(),
            config,
            docs: Vec::new(),
            current: 0,
            workspace: Workspace::TwoD,
            bottom: None,
            tool: Tool::Select,
            snap: false,
            show_grid: true,
            show_collisions: true,
            debug_collisions: false,
            run_current: false,
            files: Vec::new(),
            files_scanned: None,
            collapsed_dirs: HashSet::new(),
            file_keys: HashMap::new(),
            list_keys: HashMap::new(),
            fs_view: FsView::Dir(path.clone()),
            fs_history: vec![FsView::Dir(path.clone())],
            fs_history_pos: 0,
            fs_show_list: true,
            fs_favorites_open: true,
            fs_sort_types: false,
            fs_selected: Some(path.clone()),
            fs_path_shown: String::new(),
            create: None,
            dialog: None,
            files_dialog: None,
            prefs,
            dragging_node: false,
            drag_moved: false,
            drag_acc: Vec2::ZERO,
            orbit: Orbit::default(),
            hits2d: Vec::new(),
            hits3d: Vec::new(),
            inspector: inspector::State::default(),
            output: Arc::new(Mutex::new(Vec::new())),
            output_shown: usize::MAX,
            toast_until: None,
            run: None,
            fullscreen: false,
            clipboard: None,
            file_context: None,
            fs_context_item: false,
            fullscreen_request: None,
            back: false,
            quit: false,
            restart: false,
            scripts: Vec::new(),
            current_script: 0,
            running_driver: None,
        };
        ed.refresh_renderer_selector();
        match migrated {
            Ok(lines) if !lines.is_empty() => {
                ed.log("Project converted to Neko's Godot-compatible formats:".into());
                for l in lines {
                    ed.log(format!("  {l}"));
                }
                ed.notify("Project converted: see the Output panel.");
            }
            Ok(_) => {}
            Err(e) => ed.notify(format!("Could not convert the project: {e}")),
        }
        let main = ed.config.as_ref().ok().map(|c| path.join(&c.project.main_scene));
        match main {
            Some(file) if file.is_file() => ed.open_scene_file(file),
            // The main scene does not exist yet: start empty, saving creates it.
            other => ed.docs.push(Doc { file: other, ..Doc::empty() }),
        }
        if ed.docs.is_empty() {
            ed.docs.push(Doc::empty());
        }
        ed.log(format!("Opened project {}", path.display()));
        ed
    }

    // ------------------------------------------------------------ accessors

    fn doc(&self) -> &Doc {
        &self.docs[self.current]
    }

    fn doc_mut(&mut self) -> &mut Doc {
        &mut self.docs[self.current]
    }

    pub fn title(&self) -> String {
        let name = self.config.as_ref().map(|c| c.project.name.as_str()).unwrap_or("Project");
        format!("{} - {name} - Neko", self.doc().title())
    }

    pub fn is_dirty(&self) -> bool {
        self.docs.iter().any(|d| d.dirty && (d.has_root || d.file.is_some())) || self.scripts_dirty()
    }

    fn rel(&self, p: &Path) -> String {
        p.strip_prefix(&self.path)
            .map(|r| format!("res://{}", r.to_string_lossy().replace('\\', "/")))
            .unwrap_or_else(|_| p.to_string_lossy().replace('\\', "/"))
    }

    fn notify(&mut self, msg: impl Into<String>) {
        let msg = msg.into();
        self.log(msg.clone());
        self.ui.set_text(self.w.toast_label, msg);
        self.ui.set_visible(self.w.toast, true);
        self.toast_until = Some(Instant::now() + Duration::from_secs(4));
    }

    /// Add a line to the Output panel.
    pub fn log_line(&self, line: String) {
        self.log(line);
    }

    fn log(&self, line: String) {
        if let Ok(mut o) = self.output.lock() {
            o.push(line);
            let len = o.len();
            if len > 2000 {
                o.drain(..len - 2000);
            }
        }
    }

    fn save_prefs(&self) {
        let dir = self.path.join(".neko");
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(text) = toml::to_string(&self.prefs) {
            let _ = std::fs::write(dir.join("editor.toml"), text);
        }
    }

    // --------------------------------------------------------------- scenes

    fn open_scene_file(&mut self, file: PathBuf) {
        if let Some(i) = self.docs.iter().position(|d| d.file.as_deref() == Some(file.as_path())) {
            self.current = i;
            return;
        }
        match Doc::load(file.clone()) {
            Ok(doc) => {
                for w in &doc.warnings {
                    self.log(format!("warning: {w}"));
                }
                // Replace a single untouched empty tab, like Godot does.
                if self.docs.len() == 1 && !self.docs[0].has_root && !self.docs[0].dirty {
                    self.docs[0] = doc;
                    self.current = 0;
                } else {
                    self.docs.push(doc);
                    self.current = self.docs.len() - 1;
                }
                let rel = self.rel(&file);
                self.prefs.recent_scenes.retain(|r| r != &rel);
                self.prefs.recent_scenes.insert(0, rel);
                self.prefs.recent_scenes.truncate(10);
                self.save_prefs();
                self.workspace_for_current();
            }
            Err(e) => self.notify(format!("Could not open {}: {e}", self.rel(&file))),
        }
    }

    /// Switch to 3D when the scene's root is 3D, to 2D otherwise.
    fn workspace_for_current(&mut self) {
        if !matches!(self.workspace, Workspace::TwoD | Workspace::ThreeD) || !self.doc().has_root {
            return;
        }
        let d = self.doc();
        let class = d.scene.get(d.scene.root()).map_or("Node", |n| n.class);
        self.workspace = if neko::core::ClassDb::get().inherits(class, "Node3D") { Workspace::ThreeD } else { Workspace::TwoD };
    }

    fn new_scene(&mut self) {
        self.docs.push(Doc::empty());
        self.current = self.docs.len() - 1;
    }

    fn set_root(&mut self, class: &'static str) {
        let doc = self.doc_mut();
        doc.checkpoint(None);
        doc.scene = Scene::new(class, class).expect("instanceable class");
        doc.has_root = true;
        doc.selected = Some(doc.scene.root());
        self.workspace_for_current();
    }

    /// Save the current scene; untitled scenes ask for a file name.
    pub fn save(&mut self) -> bool {
        self.save_doc(self.current)
    }

    fn save_doc(&mut self, i: usize) -> bool {
        if !self.docs[i].has_root {
            self.notify("Add a root node before saving the scene.");
            return false;
        }
        let Some(file) = self.docs[i].file.clone() else {
            let root = &self.docs[i].scene;
            let name = root.get(root.root()).map(|n| snake(&n.name)).unwrap_or_else(|| "scene".into());
            self.open_save_as(i, format!("scenes/{name}.tscn"));
            return false;
        };
        let text = format::to_string(&mut self.docs[i].scene);
        let result = file.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|_| std::fs::write(&file, text));
        match result {
            Ok(()) => {
                self.docs[i].dirty = false;
                self.files_scanned = None;
                self.notify(format!("Saved {}", self.rel(&file)));
                true
            }
            Err(e) => {
                self.notify(format!("Could not save {}: {e}", self.rel(&file)));
                false
            }
        }
    }

    fn save_all(&mut self) {
        self.save_all_scripts();
        for i in 0..self.docs.len() {
            if self.docs[i].dirty && self.docs[i].has_root && self.docs[i].file.is_some() {
                self.save_doc(i);
            }
        }
    }

    fn close_tab(&mut self, i: usize, force: bool) {
        let d = &self.docs[i];
        if !force && d.dirty && d.has_root {
            self.current = i;
            self.open_confirm(Pending::CloseTab(i));
            return;
        }
        self.docs.remove(i);
        if self.docs.is_empty() {
            self.docs.push(Doc::empty());
        }
        self.current = self.current.min(self.docs.len() - 1);
        self.workspace_for_current();
    }

    pub fn request_quit(&mut self) {
        if self.is_dirty() {
            self.open_confirm(Pending::Quit);
        } else {
            self.stop_project();
            self.quit = true;
        }
    }

    fn request_back(&mut self) {
        if self.is_dirty() {
            self.open_confirm(Pending::Back);
        } else {
            self.stop_project();
            self.back = true;
        }
    }

    // ---------------------------------------------------------------- nodes

    pub fn open_create_dialog(&mut self) {
        if self.create.is_some() || self.dialog.is_some() {
            return;
        }
        let (class, target) = if self.doc().has_root {
            let d = self.doc();
            let id = d.selected.filter(|&id| d.scene.contains(id)).unwrap_or(d.scene.root());
            (d.scene.get(id).map_or("Node", |n| n.class), CreateFor::Child)
        } else {
            ("Node", CreateFor::Root)
        };
        let dialog = CreateNodeDialog::open(&mut self.ui, &self.icons, class, &self.prefs.favorites, &self.prefs.recents);
        self.create = Some((dialog, target));
    }

    fn add_node(&mut self, class: &'static str) {
        let doc = &mut self.docs[self.current];
        let parent = doc.selected.filter(|&id| doc.scene.contains(id)).unwrap_or(doc.scene.root());
        doc.checkpoint(None);
        match doc.scene.add_child(parent, class, None) {
            Ok(id) => {
                doc.collapsed.remove(&parent);
                doc.selected = Some(id);
            }
            Err(e) => {
                doc.undo.pop();
                self.notify(e.to_string());
                return;
            }
        }
        self.remember_recent(class);
    }

    fn remember_recent(&mut self, class: &str) {
        self.prefs.recents.retain(|r| r != class);
        self.prefs.recents.insert(0, class.to_string());
        self.prefs.recents.truncate(10);
        self.save_prefs();
    }

    fn delete_selected(&mut self) {
        let doc = self.doc_mut();
        let Some(id) = doc.selected else { return };
        if id == doc.scene.root() {
            // Deleting the root empties the scene, as in Godot.
            doc.checkpoint(None);
            let file = doc.file.take();
            *doc = Doc { file, dirty: true, undo: std::mem::take(&mut doc.undo), ..Doc::empty() };
            return;
        }
        let parent = doc.scene.get(id).and_then(|n| n.parent());
        doc.checkpoint(None);
        if doc.scene.remove(id).is_ok() {
            doc.selected = parent;
        }
    }

    fn duplicate_selected(&mut self) {
        let doc = self.doc_mut();
        let Some(id) = doc.selected else { return };
        if id == doc.scene.root() {
            return;
        }
        doc.checkpoint(None);
        if let Ok(copy) = doc.scene.duplicate(id) {
            doc.selected = Some(copy);
        }
    }

    fn move_selected(&mut self, delta: i32) {
        let doc = self.doc_mut();
        let Some(id) = doc.selected else { return };
        if id == doc.scene.root() {
            return;
        }
        doc.checkpoint(None);
        let _ = doc.scene.move_sibling(id, delta);
    }

    fn rename_selected(&mut self) {
        if let Some(id) = self.doc().selected {
            self.ui.edit_tree_row(self.w.scene_tree, id.to_bits());
        }
    }

    // -------------------------------------------------------------- running

    /// F5: run the project's main scene (EditorRunBar::play_main_scene).
    /// Like Godot, when no main scene exists yet, the current scene becomes it.
    fn run_project(&mut self) {
        let main = self.config.as_ref().ok().map(|c| c.project.main_scene.clone()).unwrap_or_default();
        if main.is_empty() || !self.path.join(&main).is_file() {
            self.save_all();
            let Some(file) = self.docs[self.current].file.clone() else {
                self.notify("No main scene has been defined. Save the current scene first, then press Play again.");
                return;
            };
            let rel = self.rel(&file).trim_start_matches("res://").to_string();
            match ProjectConfig::load(&self.path) {
                Ok(mut c) => {
                    c.project.main_scene = rel.clone();
                    if let Err(e) = c.save(&self.path) {
                        self.notify(format!("Could not set the main scene: {e}"));
                        return;
                    }
                    self.config = Ok(c);
                    self.notify(format!("Main scene set to res://{rel}"));
                }
                Err(e) => {
                    self.notify(format!("Could not read project.neko: {e}"));
                    return;
                }
            }
        }
        self.start_game(None);
    }

    /// F6: run the scene being edited (EditorRunBar::play_current_scene).
    fn run_current_scene(&mut self) {
        if !self.docs[self.current].has_root {
            self.notify("There is no defined scene to run.");
            return;
        }
        if self.docs[self.current].file.is_none() || self.docs[self.current].dirty {
            // Godot saves before running; an unsaved new scene asks for a path first.
            if !self.save_doc(self.current) {
                return;
            }
        }
        let Some(file) = self.docs[self.current].file.clone() else { return };
        let scene = self.rel(&file);
        self.start_game(Some(scene));
    }

    /// Build and launch the game with `cargo run`, passing the scene and debug
    /// flags the way Godot's editor passes them to the game process.
    fn start_game(&mut self, scene: Option<String>) {
        self.stop_project();
        self.save_all();
        let out = self.output.clone();
        let mut args = vec!["run".to_string(), "--".to_string()];
        self.run_current = scene.is_some();
        if let Some(s) = scene {
            args.push(s);
        }
        if self.debug_collisions {
            args.push("--debug-collisions".into());
        }
        self.log(format!("> cargo {}  ({})", args.join(" "), self.path.display()));
        let spawned = Command::new("cargo").args(&args).current_dir(&self.path).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn();
        match spawned {
            Ok(mut child) => {
                let pipes: [Option<Box<dyn std::io::Read + Send>>; 2] = [
                    child.stdout.take().map(|p| Box::new(p) as Box<dyn std::io::Read + Send>),
                    child.stderr.take().map(|p| Box::new(p) as Box<dyn std::io::Read + Send>),
                ];
                for pipe in pipes.into_iter().flatten() {
                    let out = out.clone();
                    std::thread::spawn(move || {
                        for line in BufReader::new(pipe).lines().map_while(Result::ok) {
                            if let Ok(mut o) = out.lock() {
                                o.push(line);
                            }
                        }
                    });
                }
                self.run = Some(child);
                self.bottom = Some(BottomTab::Output);
            }
            Err(e) => self.notify(format!("Could not start cargo: {e}")),
        }
    }

    fn stop_project(&mut self) {
        if let Some(mut c) = self.run.take() {
            let _ = c.kill();
            let _ = c.wait();
            self.log("Stopped.".into());
        }
    }

    fn poll_run(&mut self) {
        let Some(child) = &mut self.run else { return };
        match child.try_wait() {
            Ok(Some(status)) => {
                self.run = None;
                self.log(format!("Process exited ({status})."));
            }
            Ok(None) => {}
            Err(_) => self.run = None,
        }
    }

    fn toggle_fullscreen(&mut self) {
        self.fullscreen = !self.fullscreen;
        self.fullscreen_request = Some(self.fullscreen);
    }

    // ---------------------------------------------------------------- frame

    /// Handle input, update the controls from the editor state and draw.
    pub fn frame(&mut self, size_px: (u32, u32), pixels_per_point: f32, max_texture_side: usize) -> UiOutput {
        self.poll_run();
        for ev in self.ui.take_events() {
            self.handle(ev);
        }
        self.sync();
        self.paint_viewport();
        self.ui.frame(size_px, pixels_per_point, max_texture_side)
    }

    /// When the editor wants to be drawn again without input.
    pub fn wake_after(&self) -> Option<Duration> {
        let toast = self.toast_until.map(|t| t.saturating_duration_since(Instant::now()));
        let run = self.run.as_ref().map(|_| Duration::from_millis(300));
        // Parse edited scripts once typing pauses.
        let check = self.scripts_pending().then_some(Duration::from_millis(260));
        [toast, run, check].into_iter().flatten().min()
    }

    fn handle(&mut self, ev: UiEvent) {
        // Godot's file dialog sits above everything else.
        if self.handle_files_dialog(&ev) {
            return;
        }
        // Modal dialogs take every event while open.
        if let Some((dialog, target)) = &mut self.create {
            let target = *target;
            match dialog.handle(&mut self.ui, &self.icons, &ev, &mut self.prefs.favorites, &self.prefs.recents) {
                Outcome::Open => {}
                Outcome::Cancel => {
                    if let Some((d, _)) = self.create.take() {
                        d.close(&mut self.ui);
                    }
                    self.save_prefs();
                }
                Outcome::Create(class) => {
                    if let Some((d, _)) = self.create.take() {
                        d.close(&mut self.ui);
                    }
                    match target {
                        CreateFor::Root => {
                            self.set_root(class);
                            self.remember_recent(class);
                        }
                        CreateFor::Child => self.add_node(class),
                    }
                }
            }
            return;
        }
        if self.dialog.is_some() {
            self.handle_dialog(ev);
            return;
        }
        if self.inspector_event(&ev) {
            return;
        }
        if self.script_event(&ev) {
            return;
        }
        let w = &self.w;
        match ev {
            UiEvent::Menu(action) if (script_editor::MENU_FIRST..=script_editor::MENU_LAST).contains(&action) => self.script_menu_action(action),
            UiEvent::Menu(action) => self.menu_action(action),
            UiEvent::Key { key, modifiers } => self.shortcut(key, modifiers),
            UiEvent::ItemSelected(id, i) if id == w.renderer => self.renderer_selected(i),
            UiEvent::Pressed(id) if id == w.play => self.run_project(),
            UiEvent::Pressed(id) if id == w.play_scene => self.run_current_scene(),
            UiEvent::Pressed(id) if id == w.stop => self.stop_project(),
            UiEvent::Pressed(id) if id == w.add_node => self.open_create_dialog(),
            UiEvent::Pressed(id) if id == w.attach_script => self.open_attach_script(),
            UiEvent::Pressed(id) if id == w.detach_script => self.detach_script(),
            UiEvent::Pressed(id) if Some(id) == self.inspector.script_button => {
                if let Some(p) = self.selected_script() {
                    self.open_script(p);
                }
            }
            UiEvent::Pressed(id) if id == w.file_back => self.fs_back(),
            UiEvent::Pressed(id) if id == w.file_forward => self.fs_forward(),
            UiEvent::Pressed(id) if id == w.file_split => self.fs_toggle_split(),
            UiEvent::Pressed(id) if id == w.fs_import => self.open_import_dialog(),
            UiEvent::Pressed(id) if id == w.file_sort => {
                self.fs_sort_types = !self.fs_sort_types;
            }
            UiEvent::TextSubmitted(id) if id == w.file_path => {
                let text = self.ui.text(id).to_string();
                self.fs_go_to_path(&text);
            }
            UiEvent::Pressed(id) if w.workspaces.contains(&id) => {
                let i = w.workspaces.iter().position(|&x| x == id).expect("listed");
                self.workspace = WORKSPACES[i].0;
            }
            UiEvent::Pressed(id) if w.tools.contains(&id) => {
                let i = w.tools.iter().position(|&x| x == id).expect("listed");
                self.tool = TOOLS[i].0;
            }
            UiEvent::Pressed(id) if id == w.snap => self.snap = !self.snap,
            UiEvent::Pressed(id) if id == w.grid => self.show_grid = !self.show_grid,
            UiEvent::Pressed(id) if w.bottom_tabs.contains(&id) => {
                let i = w.bottom_tabs.iter().position(|&x| x == id).expect("listed");
                let tab = BOTTOM_TABS[i].0;
                self.bottom = if self.bottom == Some(tab) { None } else { Some(tab) };
            }
            UiEvent::Pressed(id) if w.root_buttons.iter().any(|(b, _)| *b == id) => {
                let class = w.root_buttons.iter().find(|(b, _)| *b == id).and_then(|(_, c)| *c);
                match class {
                    Some(c) => self.set_root(c),
                    None => self.open_create_dialog(),
                }
            }
            UiEvent::TabSelected(id, i) if id == w.scene_tabs => {
                self.current = i.min(self.docs.len() - 1);
                self.workspace_for_current();
            }
            UiEvent::TabClosed(id, i) if id == w.scene_tabs => self.close_tab(i, false),
            UiEvent::TabAdded(id) if id == w.scene_tabs => self.new_scene(),
            UiEvent::TabSelected(id, i) if id == w.left_tabs => {
                self.ui.set_visible(self.w.scene_panel, i == 0);
                self.ui.set_visible(self.w.import_panel, i == 1);
            }
            UiEvent::TabSelected(id, i) if id == w.right_tabs => {
                self.ui.set_visible(self.w.inspector_panel, i == 0);
                self.ui.set_visible(self.w.signals_panel, i == 1);
                self.ui.set_visible(self.w.groups_panel, i == 2);
            }
            UiEvent::Canvas(id, input) if id == w.viewport => match self.workspace {
                Workspace::ThreeD => self.viewport3d_input(input),
                _ => self.viewport2d_input(input),
            },
            ev @ (UiEvent::TreeSelected(..)
            | UiEvent::TreeActivated(..)
            | UiEvent::TreeToggled(..)
            | UiEvent::TreeButton(..)
            | UiEvent::TreeContext(..)
            | UiEvent::TreeDropped { .. }
            | UiEvent::TreeEdited(..)) => self.tree_event(ev),
            _ => {}
        }
    }

    /// Editor shortcuts, as registered by Godot's EditorNode / SceneTreeDock.
    fn shortcut(&mut self, key: Key, m: Modifiers) {
        if self.script_shortcut(key, m) {
            return;
        }
        let cmd = m.ctrl || m.logo;
        // FileSystemDock shortcuts only apply while its tree/list has focus.
        if self.fs_focused() {
            match key {
                Key::F(2) => return self.fs_rename_selected(),
                Key::Delete => return self.fs_delete_selected(),
                Key::Char('d') if cmd && !m.shift => return self.fs_duplicate_selected(),
                Key::Char('c') if cmd && m.shift && m.alt => return self.fs_copy_absolute(),
                Key::Char('c') if cmd && m.shift => return self.fs_copy_path(),
                Key::Char('r') if cmd && m.alt => return self.fs_show_in_file_manager(),
                _ => {}
            }
        }
        if cmd && key == Key::Char('l') {
            self.ui.focus(self.w.file_path);
            return;
        }
        match key {
            Key::Char('n') if cmd => self.new_scene(),
            Key::Char('o') if cmd && !m.shift => self.open_scene_dialog(),
            Key::Char('s') if cmd && m.shift && m.alt => self.save_all(),
            Key::Char('s') if cmd && m.shift => self.menu_action(menus::SAVE_AS),
            Key::Char('s') if cmd => {
                self.save();
            }
            Key::Char('w') if cmd && m.shift => self.close_tab(self.current, false),
            Key::Char('q') if cmd && m.shift => self.request_back(),
            Key::Char('q') if cmd => self.request_quit(),
            Key::Char('z') if cmd && m.shift => {
                self.doc_mut().redo();
            }
            Key::Char('y') if cmd => {
                self.doc_mut().redo();
            }
            Key::Char('z') if cmd => {
                self.doc_mut().undo();
            }
            Key::Char('a') if cmd => self.open_create_dialog(),
            Key::Char('d') if cmd => self.duplicate_selected(),
            Key::ArrowUp if cmd => self.move_selected(-1),
            Key::ArrowDown if cmd => self.move_selected(1),
            Key::Delete => self.delete_selected(),
            Key::F(2) => self.rename_selected(),
            Key::F(5) => self.run_project(),
            Key::F(6) => self.run_current_scene(),
            Key::F(8) => self.stop_project(),
            Key::F(11) if m.shift => self.toggle_fullscreen(),
            Key::Char('q') if !cmd => self.tool = Tool::Select,
            Key::Char('w') if !cmd => self.tool = Tool::Move,
            Key::Char('e') if !cmd => self.tool = Tool::Rotate,
            Key::Char('s') if !cmd => self.tool = Tool::Scale,
            _ => {}
        }
    }

    /// Bring every control in line with the editor state.
    fn sync(&mut self) {
        let menus = self.build_menus();
        self.ui.set_menus(self.w.menubar, menus);
        let view = self.view_menus();
        self.ui.set_menus(self.w.view_menus, view);
        for (i, &id) in self.w.workspaces.clone().iter().enumerate() {
            self.ui.set_checked(id, self.workspace == WORKSPACES[i].0);
        }
        let running = self.run.is_some();
        self.ui.set_checked(self.w.play, running && !self.run_current);
        self.ui.set_checked(self.w.play_scene, running && self.run_current);
        self.ui.set_disabled(self.w.stop, !running);
        self.ui.set_visible(self.w.running, running);

        let tabs: Vec<Tab> = self.docs.iter().map(|d| Tab { title: d.title(), icon: self.icons.tinted("PackedScene", neko_ui::Color32::from_gray(214)) }).collect();
        self.ui.set_tabs(self.w.scene_tabs, tabs, self.current);

        let in_viewport = matches!(self.workspace, Workspace::TwoD | Workspace::ThreeD);
        let placeholder = matches!(self.workspace, Workspace::Game | Workspace::AssetLib);
        self.ui.set_visible(self.w.view_box, in_viewport);
        self.ui.set_visible(self.w.placeholder, placeholder);
        self.sync_scripts();
        if placeholder {
            let text = match self.workspace {
                Workspace::Game => "Game view\n\nPress Run Project (F5) to start the game in its own window. Embedding the running game here comes later.",
                _ => "Asset Library\n\nBrowsing community assets needs the asset server, which is not built yet.",
            };
            self.ui.set_text(self.w.placeholder, text);
        }
        for (i, &id) in self.w.tools.clone().iter().enumerate() {
            self.ui.set_checked(id, self.tool == TOOLS[i].0);
        }
        self.ui.set_checked(self.w.snap, self.snap);
        self.ui.set_checked(self.w.grid, self.show_grid);

        let has_root = self.doc().has_root;
        self.ui.set_visible(self.w.create_root, !has_root);
        self.ui.set_visible(self.w.scene_tree, has_root);
        let selected = self.doc().selected.filter(|_| has_root);
        let has_script = self.selected_script().is_some();
        self.ui.set_visible(self.w.attach_script, !has_script);
        self.ui.set_disabled(self.w.attach_script, selected.is_none());
        self.ui.set_visible(self.w.detach_script, has_script);
        self.sync_scene_tree();
        self.sync_files();
        self.sync_inspector();
        self.sync_bottom();

        if self.toast_until.is_some_and(|t| Instant::now() > t) {
            self.toast_until = None;
            self.ui.set_visible(self.w.toast, false);
        }
    }

    fn sync_bottom(&mut self) {
        for (i, &id) in self.w.bottom_tabs.clone().iter().enumerate() {
            self.ui.set_checked(id, self.bottom == Some(BOTTOM_TABS[i].0));
        }
        self.ui.set_visible(self.w.bottom_panel, self.bottom.is_some());
        let output = self.bottom == Some(BottomTab::Output);
        self.ui.set_visible(self.w.output_tree, output);
        self.ui.set_visible(self.w.bottom_note, !output);
        if let Some(tab) = self.bottom
            && !output
        {
            let text = match tab {
                BottomTab::Debugger => "The debugger (breakpoints, errors, profiler) is not available yet. Run output appears in Output.",
                BottomTab::Audio => "Audio buses are not available yet. The project's audio backend is set in Project Settings.",
                BottomTab::Animation => "Select an AnimationPlayer to edit its tracks. The animation editor is not available yet.",
                _ => "Shaders are written in WGSL. The shader editor is not available yet.",
            };
            self.ui.set_text(self.w.bottom_note, text);
        }
        let lines: Vec<String> = self.output.lock().map(|o| o.clone()).unwrap_or_default();
        if lines.len() != self.output_shown {
            self.output_shown = lines.len();
            let t = self.ui.theme.clone();
            let rows = lines
                .iter()
                .enumerate()
                .map(|(i, l)| {
                    let mut r = TreeRow::new(i as u64, 0, l.clone());
                    if l.contains("error") {
                        r.color = Some(crate::theme::BAD);
                    } else if l.contains("warning") {
                        r.color = Some(crate::theme::WARN);
                    } else {
                        r.color = Some(t.font_color);
                    }
                    r
                })
                .collect();
            self.ui.set_rows(self.w.output_tree, rows);
        }
    }

    fn paint_viewport(&mut self) {
        match self.workspace {
            Workspace::TwoD => self.paint_viewport2d(),
            Workspace::ThreeD => self.paint_viewport3d(),
            _ => {}
        }
    }
}

impl Drop for Editor {
    fn drop(&mut self) {
        self.stop_project();
    }
}

/// Build the editor's controls once.
/// A dock's tab bar with the dock options menu button on the right
/// (EditorDockManager's "⋮" with Dock Position / Make Floating / Close).
fn dock_tabs(ui: &mut Ui, icons: &Icons, parent: Id, tabs: Vec<Tab>) -> Id {
    let row = ui.add(parent, Control::hbox().separation(0.0));
    let bar = ui.add(row, Control::tab_bar(tabs, 0).expand_h());
    ui.add(row, ui_kit::icon_button(icons, "GuiTabMenuHl", "Dock options. Not available yet.", None).disabled(true));
    bar
}

fn build(ui: &mut Ui, icons: &Icons) -> W {
    let t = ui.theme.clone();
    let root = ui.root();
    let main = ui.add(root, Control::vbox().separation(0.0).anchors(Anchors::FULL_RECT));

    // Title bar: MenuBar, workspaces centered, run buttons right (Godot's title bar).
    let top = ui.add(main, Control::panel_container().style(StyleBox::flat(t.background, 0, Sides::symmetric(4.0, 2.0))));
    let bar = ui.add(top, Control::hbox().separation(2.0));
    let menubar = ui.add(bar, Control::menu_bar(Vec::new()).v_center());
    ui.add(bar, Control::spacer());
    let workspaces = WORKSPACES.iter().map(|(_, label, icon)| ui.add(bar, ui_kit::icon_text_button(icons, icon, label).toggle(false))).collect();
    ui.add(bar, Control::spacer());
    let play = ui.add(bar, ui_kit::icon_button(icons, "Play", "Run Project (F5)", Some(false)));
    ui.add(bar, ui_kit::icon_button(icons, "Pause", "Pause the running project's execution for debugging.", None).disabled(true));
    let stop = ui.add(bar, ui_kit::icon_button(icons, "Stop", "Stop the currently running project. (F8)", None));
    let play_scene = ui.add(bar, ui_kit::icon_button(icons, "PlayScene", "Run Current Scene (F6)", Some(false)));
    ui.add(bar, ui_kit::icon_button(icons, "PlayScene", "Run Specific Scene (Ctrl+Shift+F5). Not available yet.", None).disabled(true));
    ui.add(bar, ui_kit::icon_button(icons, "MainMovieWrite", "Enable Movie Maker mode. Not available yet.", None).disabled(true));
    // EditorNode's renderer OptionButton ("TopBarOptionButton": flat, bold),
    // listing the graphics APIs this machine can use.
    let labels: Vec<&str> = RenderingDriver::host_choices().iter().map(|d| d.label()).collect();
    let renderer = ui.add(
        bar,
        Control::option_button(&labels, 0)
            .button_style(t.button_flat)
            .bold()
            .v_center()
            .tooltip("Choose a renderer.\n\nVulkan, Direct3D 12 and Metal are the modern GPU APIs; OpenGL is the compatibility renderer for older hardware; Software draws on the CPU."),
    );

    let body = ui.add(main, Control::h_split(270.0));
    // Left docks.
    let left = ui.add(body, Control::v_split(380.0).min(200.0, 0.0));
    let scene_dock = ui.add(left, Control::vbox().separation(0.0).min(0.0, 140.0));
    let left_tabs = dock_tabs(ui, icons, scene_dock, vec![Tab::new("Scene"), Tab::new("Import")]);
    let scene_panel = ui.add(scene_dock, Control::panel_container().expand().style(StyleBox::flat(t.panel.bg, 0, Sides::all(4.0))));
    let scene_col = ui.add(scene_panel, Control::vbox().separation(4.0));
    let tools_row = ui.add(scene_col, Control::hbox().separation(2.0));
    let add_node = ui.add(tools_row, ui_kit::icon_button(icons, "Add", "Add Child Node (Ctrl+A)", None));
    ui.add(tools_row, ui_kit::icon_button(icons, "Instance", "Instantiate a scene file as a Node. Creates an inherited scene if no root node exists. Not available yet.", None).disabled(true));
    let node_filter = ui.add(tools_row, Control::line_edit("").placeholder("Filter Nodes"));
    // SceneTreeDock's button_create_script / button_detach_script: one shows at a time.
    let attach_script = ui.add(tools_row, ui_kit::icon_button(icons, "ScriptCreate", "Attach a new or existing script to the selected node.", None));
    let detach_script = ui.add(tools_row, ui_kit::icon_button(icons, "ScriptRemove", "Detach the script from the selected node.", None).hidden());
    ui.add(tools_row, ui_kit::icon_button(icons, "GuiTabMenuHl", "Extra scene options.", None).disabled(true));
    let create_root = ui.add(scene_col, Control::vbox().separation(4.0).hidden());
    ui.add(create_root, Control::label("Create Root Node:").color(t.font_focus_color));
    let mut root_buttons = Vec::new();
    for (label, class, icon) in [("2D Scene", Some("Node2D"), "Node2D"), ("3D Scene", Some("Node3D"), "Node3D"), ("User Interface", Some("Control"), "Control"), ("Other Node", None, "Add")] {
        let img = icons.get(icon);
        let b = ui.add(create_root, Control::button(label).icon(move |p, r, _, _| {
            if let Some(i) = img {
                p.image(i, neko_ui::Rect::from_center_size(r.center(), vec2(16.0, 16.0)));
            }
        }));
        root_buttons.push((b, class));
    }
    let scene_tree_bg = ui.add(scene_col, Control::panel_container().style(t.tree_panel).expand());
    let scene_tree = ui.add(scene_tree_bg, Control::tree());
    let import_panel = ui.add(scene_dock, Control::panel_container().expand().style(StyleBox::flat(t.panel.bg, 0, Sides::all(8.0))).hidden());
    ui.add(import_panel, Control::label("Select a resource file in the filesystem or in the inspector to adjust import settings.").wrap().color(t.font_muted_color));

    let fs_dock = ui.add(left, Control::vbox().separation(0.0).min(0.0, 140.0));
    dock_tabs(ui, icons, fs_dock, vec![Tab::new("FileSystem")]);
    let fs_panel = ui.add(fs_dock, Control::panel_container().expand().style(StyleBox::flat(t.panel.bg, 0, Sides::all(4.0))));
    let fs_col = ui.add(fs_panel, Control::vbox().separation(4.0));
    // FileSystemDock (editor/docks/filesystem_dock.cpp): history arrows, the
    // current path, the split-mode toggle; then the filter and sort. The tree
    // shows the project folders and the file list shows the current one.
    let nav = ui.add(fs_col, Control::hbox().separation(2.0));
    let file_back = ui.add(nav, ui_kit::icon_button(icons, "Back", "Go to previous selected folder/file.", None));
    let file_forward = ui.add(nav, ui_kit::icon_button(icons, "Forward", "Go to next selected folder/file.", None));
    let file_path = ui.add(nav, Control::line_edit("res://").monospace());
    let file_split = ui.add(nav, ui_kit::icon_button(icons, "GuiTabMenuHl", "Change Split Mode", Some(true)));
    let filter_row = ui.add(fs_col, Control::hbox().separation(2.0));
    let file_filter = ui.add(filter_row, Control::line_edit("").placeholder("Filter Files"));
    let fs_import = ui.add(filter_row, ui_kit::icon_button(icons, "Load", "Import Files...", None));
    let file_sort = ui.add(filter_row, ui_kit::icon_button(icons, "Sort", "Sort Files", Some(false)));
    let fs_split = ui.add(fs_col, Control::v_split(150.0).expand());
    let fs_tree_bg = ui.add(fs_split, Control::panel_container().style(t.tree_panel));
    let file_tree = ui.add(fs_tree_bg, Control::tree());
    let file_list_panel = ui.add(fs_split, Control::panel_container().style(t.tree_panel));
    let file_list = ui.add(file_list_panel, Control::tree());

    // Center and right dock.
    let right_split = ui.add(body, Control::h_split(f32::MAX));
    let center = ui.add(right_split, Control::vbox().separation(0.0).min(300.0, 0.0));
    let tabs_bg = ui.add(center, Control::panel_container().style(StyleBox::flat(t.background, 0, Sides::new(4.0, 4.0, 4.0, 0.0))));
    let tabs_row = ui.add(tabs_bg, Control::hbox().separation(0.0));
    let scene_tabs = ui.add(tabs_row, Control::tab_bar(Vec::new(), 0).closable().add_button().expand_h());
    ui.add(tabs_row, ui_kit::icon_button(icons, "GuiTabMenuHl", "Scene tabs. Not available yet.", None).disabled(true));
    ui.add(tabs_row, ui_kit::icon_button(icons, "DistractionFree", "Toggle Distraction-Free Mode. Not available yet.", None).disabled(true));
    let view_box = ui.add(center, Control::vbox().separation(0.0).expand());
    let tb_bg = ui.add(view_box, Control::panel_container().style(StyleBox::flat(t.panel.bg, 0, Sides::symmetric(4.0, 2.0))));
    let tb = ui.add(tb_bg, Control::hbox().separation(2.0));
    let tools = TOOLS.iter().map(|(tool, icon, tip)| ui.add(tb, ui_kit::icon_button(icons, icon, tip, Some(*tool == Tool::Select)))).collect();
    ui.add(tb, Control::v_separator());
    let snap = ui.add(tb, ui_kit::icon_button(icons, "SnapGrid", "Use Grid Snap", Some(false)));
    let grid = ui.add(tb, ui_kit::icon_button(icons, "GridToggle", "Toggle Grid", Some(true)));
    ui.add(tb, Control::v_separator());
    let view_menus = ui.add(tb, Control::menu_bar(Vec::new()).v_center());
    let viewport = ui.add(view_box, Control::canvas());
    let placeholder = ui.add(center, Control::label("").wrap().align(neko_ui::Align::Center).color(t.font_muted_color).expand().hidden());
    let script = script_editor::build(ui, icons, center);

    let bottom_panel = ui.add(center, Control::panel_container().style(StyleBox::flat(t.panel.bg, 0, Sides::all(4.0))).min(0.0, 190.0).hidden());
    let bottom_col = ui.add(bottom_panel, Control::vbox());
    let output_bg = ui.add(bottom_col, Control::panel_container().style(t.tree_panel).expand());
    let output_tree = ui.add(output_bg, Control::tree());
    let bottom_note = ui.add(bottom_col, Control::label("").wrap().color(t.font_muted_color).hidden());
    let bbar_bg = ui.add(center, Control::panel_container().style(StyleBox::flat(t.background, 0, Sides::symmetric(4.0, 2.0))));
    let bbar = ui.add(bbar_bg, Control::hbox().separation(2.0));
    let bottom_tabs = BOTTOM_TABS.iter().map(|(_, label)| ui.add(bbar, Control::button(*label).flat().toggle(false))).collect();
    ui.add(bbar, Control::spacer());
    let running = ui.add(bbar, Control::label("running").font(12.5).color(crate::theme::OK).v_center().hidden());
    ui.add(bbar, Control::v_separator());
    ui.add(bbar, Control::label(format!("{}.dev", neko::project::scaffold::ENGINE_VERSION)).font(12.5).color(t.font_muted_color).v_center());
    ui.add(bbar, ui_kit::icon_button(icons, "Pin", "Pin Bottom Panel Switching. Not available yet.", None).disabled(true));

    let right = ui.add(right_split, Control::vbox().separation(0.0).min(300.0, 0.0));
    let right_tabs = dock_tabs(ui, icons, right, vec![Tab::new("Inspector"), Tab::new("Signals"), Tab::new("Groups")]);
    let inspector_panel = ui.add(right, Control::panel_container().expand().style(StyleBox::flat(t.panel.bg, 0, Sides::all(4.0))));
    let insp_col = ui.add(inspector_panel, Control::vbox().separation(4.0));
    // InspectorDock (editor/docks/inspector_dock.cpp): resource toolbar with history,
    // the edited object selector, then the property filter.
    let res_bar = ui.add(insp_col, Control::hbox().separation(0.0));
    ui.add(res_bar, ui_kit::icon_button(icons, "New", "Create a new resource in memory and edit it.", None).disabled(true));
    ui.add(res_bar, ui_kit::icon_button(icons, "Load", "Load an existing resource from disk and edit it.", None).disabled(true));
    ui.add(res_bar, ui_kit::icon_button(icons, "Save", "Save the currently edited resource.", None).disabled(true));
    ui.add(res_bar, ui_kit::icon_button(icons, "GuiTabMenuHl", "Extra resource options.", None).disabled(true));
    ui.add(res_bar, Control::spacer());
    ui.add(res_bar, ui_kit::icon_button(icons, "Back", "Go to previous edited object in history.", None).disabled(true));
    ui.add(res_bar, ui_kit::icon_button(icons, "Forward", "Go to next edited object in history.", None).disabled(true));
    ui.add(res_bar, ui_kit::icon_button(icons, "History", "History of recently edited objects.", None).disabled(true));
    let head = ui.add(insp_col, Control::panel_container().style(StyleBox::flat(t.button.normal.bg, t.corner_radius, Sides::symmetric(8.0, 5.0))));
    let head_row = ui.add(head, Control::hbox());
    let insp_icon = ui.add(head_row, ui_kit::class_image(icons, "Node"));
    let insp_name = ui.add(head_row, Control::label("").color(t.font_focus_color).ellipsis());
    let insp_class = ui.add(head_row, Control::label("").font(12.5).color(t.font_muted_color).v_center());
    let filter_row = ui.add(insp_col, Control::hbox().separation(2.0));
    let insp_filter = ui.add(filter_row, Control::line_edit("").placeholder("Filter Properties"));
    ui.add(filter_row, ui_kit::icon_button(icons, "Tools", "Manage object properties.", None).disabled(true));
    let insp_scroll = ui.add(insp_col, Control::scroll());
    let insp_body = ui.add(insp_scroll, Control::vbox().separation(3.0));
    let signals_panel = ui.add(right, Control::panel_container().expand().style(StyleBox::flat(t.panel.bg, 0, Sides::all(10.0))).hidden());
    ui.add(signals_panel, Control::label("Signals connect events (pressed, body_entered, timeout...) to your code. Not available yet.").wrap().color(t.font_muted_color));
    let groups_panel = ui.add(right, Control::panel_container().expand().style(StyleBox::flat(t.panel.bg, 0, Sides::all(10.0))).hidden());
    ui.add(groups_panel, Control::label("Groups tag nodes so code can find them together (\"enemies\", \"coins\"). Not available yet.").wrap().color(t.font_muted_color));

    let toast = ui.add(root, Control::panel_container().style(t.tooltip.with_padding(Sides::symmetric(14.0, 8.0))).anchors(Anchors::bottom_center(56.0)).hidden());
    let toast_label = ui.add(toast, Control::label(""));

    W {
        menubar,
        attach_script,
        detach_script,
        script,
        renderer,
        workspaces,
        play,
        play_scene,
        stop,
        left_tabs,
        scene_panel,
        import_panel,
        add_node,
        node_filter,
        create_root,
        root_buttons,
        scene_tree,
        file_back,
        file_forward,
        file_path,
        file_split,
        fs_import,
        file_filter,
        file_sort,
        file_tree,
        file_list,
        fs_split,
        file_list_panel,
        scene_tabs,
        view_box,
        tools,
        snap,
        grid,
        view_menus,
        viewport,
        placeholder,
        bottom_panel,
        output_tree,
        bottom_note,
        bottom_tabs,
        running,
        right_tabs,
        inspector_panel,
        signals_panel,
        groups_panel,
        insp_filter,
        insp_icon,
        insp_name,
        insp_class,
        insp_body,
        toast,
        toast_label,
    }
}

/// `"PlayerHUD"` -> `"player_hud"`.
fn snake(name: &str) -> String {
    let mut out = String::new();
    let mut prev_lower = false;
    for c in name.chars() {
        if c.is_ascii_uppercase() {
            if prev_lower {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
            prev_lower = false;
        } else if c.is_ascii_alphanumeric() {
            out.push(c);
            prev_lower = c.is_ascii_lowercase() || c.is_ascii_digit();
        } else if !out.ends_with('_') && !out.is_empty() {
            out.push('_');
            prev_lower = false;
        }
    }
    let s = out.trim_matches('_').to_string();
    if s.is_empty() { "scene".into() } else { s }
}
