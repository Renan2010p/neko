//! The FileSystem dock, ported from Godot's
//! `editor/docks/filesystem_dock.cpp`: a folder tree with a Favorites list, a
//! file list for the current folder, path history, and the context menu with
//! Create New / Copy Path / Rename / Duplicate / Delete / Favorites.
//!
//! Everything here works on the project folder on disk; opened scenes keep
//! their file paths in sync, and text resources have `res://` references
//! rewritten when a file or folder moves, like Godot's dependency update.

use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use neko::core::{Scene, format};
use neko::project::ProjectConfig;
use neko_ui::{Color32, Control, Id, MenuItem, Pos2, TreeRow};

use super::dialogs::Dialog;
use super::menus::*;
use super::{Doc, Editor, FileEntry, FsView};
use crate::ui_kit;

/// Keys of rows that are not a path from the project scan.
const TREE_FAVORITES: u64 = u64::MAX - 1;
const TREE_ROOT: u64 = u64::MAX;
/// The ".." row at the top of the file list.
const LIST_UP: u64 = 0;
/// Favorites entries in the tree (never collide with file indices).
const FAVORITE_KEY_BASE: u64 = u64::MAX - 1000;

/// Root classes offered by "Create New > Scene..." / base types for scripts.
pub(super) const ROOT_CLASSES: [&str; 4] = ["Node2D", "Node3D", "Control", "Node"];

const TEXT_EXTENSIONS: [&str; 4] = ["tscn", "tres", "gd", "gdextension"];

/// A FileSystem dialog: Godot's DirectoryCreateDialog / SceneCreateDialog /
/// ScriptCreateDialog all reduce to a name, a folder and a base class.
#[derive(Clone, Debug)]
pub(super) enum FileOp {
    NewFolder { dir: PathBuf },
    NewScene { dir: PathBuf },
    NewScript { dir: PathBuf },
    NewTextFile { dir: PathBuf },
    Rename { path: PathBuf },
    Duplicate { path: PathBuf },
}

impl FileOp {
    fn title(&self) -> &'static str {
        match self {
            Self::NewFolder { .. } => "Create Folder",
            Self::NewScene { .. } => "Create New Scene",
            Self::NewScript { .. } => "Create New Script",
            Self::NewTextFile { .. } => "Create New TextFile",
            Self::Rename { .. } => "Rename",
            Self::Duplicate { .. } => "Duplicate",
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Self::NewFolder { .. } => "Folder name:",
            Self::NewScene { .. } => "Scene name:",
            Self::NewScript { .. } => "Script name:",
            Self::NewTextFile { .. } => "File name:",
            Self::Rename { .. } => "New name:",
            Self::Duplicate { .. } => "New name:",
        }
    }

    /// Directory printed before the name field.
    fn parent(&self) -> &Path {
        match self {
            Self::NewFolder { dir } | Self::NewScene { dir } | Self::NewScript { dir } | Self::NewTextFile { dir } => dir,
            Self::Rename { path } | Self::Duplicate { path } => path.parent().unwrap_or(Path::new("")),
        }
    }

    fn default_name(&self) -> String {
        match self {
            Self::NewFolder { .. } => "new folder".into(),
            Self::NewScene { .. } => "new_scene.tscn".into(),
            Self::NewScript { .. } => "new_script.gd".into(),
            Self::NewTextFile { .. } => "new_file.txt".into(),
            Self::Rename { path } | Self::Duplicate { path } => {
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                if matches!(self, Self::Duplicate { .. }) { unique_name(path) } else { name }
            }
        }
    }

    /// "Root Type" for scenes, "Base Type" for scripts.
    fn root_label(&self) -> Option<&'static str> {
        match self {
            Self::NewScene { .. } => Some("Root Type:"),
            Self::NewScript { .. } => Some("Base Type:"),
            _ => None,
        }
    }
}

impl Editor {
    // ------------------------------------------------------------- toolbar

    pub(super) fn fs_focused(&self) -> bool {
        matches!(self.ui.focused(), Some(id) if id == self.w.file_tree || id == self.w.file_list)
    }

    pub(super) fn fs_back(&mut self) {
        if self.fs_history_pos == 0 {
            return;
        }
        self.fs_history_pos -= 1;
        let view = self.fs_history[self.fs_history_pos].clone();
        self.show_view(view);
    }

    pub(super) fn fs_forward(&mut self) {
        if self.fs_history_pos + 1 >= self.fs_history.len() {
            return;
        }
        self.fs_history_pos += 1;
        let view = self.fs_history[self.fs_history_pos].clone();
        self.show_view(view);
    }

    pub(super) fn fs_toggle_split(&mut self) {
        self.fs_show_list = !self.fs_show_list;
        self.ui.set_visible(self.w.file_list_panel, self.fs_show_list);
        self.ui.set_split_offset(self.w.fs_split, if self.fs_show_list { 150.0 } else { 1.0e6 });
    }

    /// The path bar accepts `res://scenes`, an absolute path or "Favorites".
    pub(super) fn fs_go_to_path(&mut self, text: &str) {
        let text = text.trim();
        if text.eq_ignore_ascii_case("favorites") {
            self.navigate(FsView::Favorites, true);
            return;
        }
        let text = text.strip_prefix("res://").unwrap_or(text);
        let target = if Path::new(text).is_absolute() { PathBuf::from(text) } else { self.path.join(text) };
        let target = normalize(&target);
        if !target.starts_with(&self.path) {
            self.notify("That folder is outside the project.");
            self.reset_path_bar();
            return;
        }
        if !target.is_dir() {
            self.notify(format!("No folder at {}", self.res_url(&target)));
            self.reset_path_bar();
            return;
        }
        self.navigate(FsView::Dir(target), true);
    }

    // ---------------------------------------------------------- navigation

    fn show_view(&mut self, view: FsView) {
        match &view {
            FsView::Dir(dir) => {
                if !dir.is_dir() {
                    self.notify(format!("No folder at {}", self.res_url(dir)));
                    return;
                }
                self.fs_selected = Some(dir.clone());
            }
            FsView::Favorites => self.fs_selected = None,
        }
        self.fs_view = view;
        self.fs_path_shown.clear();
    }

    fn navigate(&mut self, view: FsView, push: bool) {
        if push {
            self.fs_history.truncate(self.fs_history_pos + 1);
            if self.fs_history.last() != Some(&view) {
                self.fs_history.push(view.clone());
                if self.fs_history.len() > 100 {
                    self.fs_history.remove(0);
                }
                self.fs_history_pos = self.fs_history.len() - 1;
            }
        }
        self.show_view(view);
    }

    fn current_dir(&self) -> Option<&Path> {
        match &self.fs_view {
            FsView::Dir(dir) => Some(dir),
            FsView::Favorites => None,
        }
    }

    fn reset_path_bar(&mut self) {
        let url = self.view_url();
        self.ui.set_text(self.w.file_path, url.clone());
        self.fs_path_shown = url;
    }

    /// The path bar text, with a trailing slash like Godot's (`res://scenes/`).
    fn view_url(&self) -> String {
        match &self.fs_view {
            FsView::Favorites => "Favorites".to_string(),
            FsView::Dir(d) => {
                let url = self.res_url(d);
                if url != "res://" { format!("{url}/") } else { url }
            }
        }
    }

    /// The folder new files go into: the selected folder, or the current one.
    fn target_dir(&self) -> PathBuf {
        if self.fs_context_item
            && let Some(p) = self.file_context.as_ref().filter(|p| p.is_dir())
        {
            return p.clone();
        }
        self.current_dir().unwrap_or(&self.path).to_path_buf()
    }

    fn expand(&mut self, path: &Path) {
        self.collapsed_dirs.remove(path);
    }

    fn expand_recursive(&mut self, path: &Path, open: bool) {
        let dirs: Vec<PathBuf> = self.files.iter().filter(|e| e.is_dir && e.path.starts_with(path)).map(|e| e.path.clone()).collect();
        for d in dirs {
            if open {
                self.collapsed_dirs.remove(&d);
            } else {
                self.collapsed_dirs.insert(d);
            }
        }
        if open {
            self.collapsed_dirs.remove(path);
        } else {
            self.collapsed_dirs.insert(path.to_path_buf());
        }
    }

    // --------------------------------------------------------------- input

    pub(super) fn fs_selected(&mut self, tree: Id, key: u64) {
        if tree == self.w.file_tree {
            match key {
                TREE_FAVORITES => self.navigate(FsView::Favorites, true),
                TREE_ROOT => self.navigate(FsView::Dir(self.path.clone()), true),
                _ => {
                    let Some(path) = self.file_keys.get(&key).cloned() else { return };
                    if path.is_dir() {
                        self.navigate(FsView::Dir(path), true);
                    } else {
                        self.fs_selected = Some(path.clone());
                        if let Some(dir) = path.parent() {
                            self.navigate(FsView::Dir(dir.to_path_buf()), true);
                            self.fs_selected = Some(path);
                        }
                    }
                }
            }
        } else {
            if key == LIST_UP {
                if let Some(dir) = self.current_dir().and_then(Path::parent) {
                    self.navigate(FsView::Dir(dir.to_path_buf()), true);
                }
                return;
            }
            if let Some(path) = self.list_keys.get(&key).cloned() {
                self.fs_selected = Some(path);
            }
        }
    }

    pub(super) fn fs_activated(&mut self, tree: Id, key: u64) {
        if tree == self.w.file_tree {
            match key {
                TREE_FAVORITES => self.navigate(FsView::Favorites, true),
                TREE_ROOT => {
                    self.expand(&self.path.clone());
                    self.navigate(FsView::Dir(self.path.clone()), true);
                }
                _ => {
                    if let Some(p) = self.file_keys.get(&key).cloned() {
                        self.open_file(p);
                    }
                }
            }
        } else {
            if key == LIST_UP {
                if let Some(dir) = self.current_dir().and_then(Path::parent) {
                    self.navigate(FsView::Dir(dir.to_path_buf()), true);
                }
                return;
            }
            if let Some(p) = self.list_keys.get(&key).cloned() {
                self.open_file(p);
            }
        }
    }

    pub(super) fn fs_toggled(&mut self, tree: Id, key: u64, open: bool) {
        if tree == self.w.file_tree {
            if key == TREE_FAVORITES {
                self.fs_favorites_open = open;
                return;
            }
            if let Some(p) = self.file_keys.get(&key).cloned() {
                if open {
                    self.collapsed_dirs.remove(&p);
                } else {
                    self.collapsed_dirs.insert(p);
                }
            }
        }
    }

    /// In-place rename finished (Enter or clicking outside the row).
    pub(super) fn fs_edited(&mut self, tree: Id, key: u64, text: &str) {
        let path = if tree == self.w.file_tree { self.file_keys.get(&key).cloned() } else { self.list_keys.get(&key).cloned() };
        let Some(path) = path else { return };
        if let Err(e) = self.rename_to(&path, text) {
            self.notify(e);
        }
        self.reload_files();
    }

    /// Dragging a file or folder onto another row (Ctrl copies).
    pub(super) fn fs_dropped(&mut self, tree: Id, dragged: u64, target: u64) {
        let source = if tree == self.w.file_tree {
            self.file_keys.get(&dragged).cloned()
        } else {
            self.list_keys.get(&dragged).cloned()
        };
        let Some(source) = source else { return };
        if target == TREE_FAVORITES {
            self.add_favorite(&source);
            return;
        }
        let target_path = if tree == self.w.file_tree {
            if target == TREE_ROOT { Some(self.path.clone()) } else { self.file_keys.get(&target).cloned() }
        } else {
            self.list_keys.get(&target).cloned()
        };
        let Some(target_path) = target_path else { return };
        let dir = if target_path.is_dir() { target_path } else { target_path.parent().unwrap_or(&self.path).to_path_buf() };
        let copy = self.ui.modifiers().ctrl || self.ui.modifiers().logo;
        if let Err(e) = self.move_to(&source, &dir, copy) {
            self.notify(e);
        }
    }

    pub(super) fn fs_context(&mut self, tree: Id, key: Option<u64>, pos: Pos2) {
        let current = self.current_dir().unwrap_or(&self.path).to_path_buf();
        self.fs_context_item = key.is_some();
        self.file_context = match (tree == self.w.file_tree, key) {
            (true, Some(TREE_FAVORITES)) | (true, None) => None,
            (true, Some(TREE_ROOT)) => Some(self.path.clone()),
            (true, Some(k)) => self.file_keys.get(&k).cloned(),
            (false, Some(LIST_UP)) | (false, None) => {
                self.fs_context_item = false;
                Some(current)
            }
            (false, Some(k)) => self.list_keys.get(&k).cloned(),
        };
        self.fs_menu(pos);
    }

    // ---------------------------------------------------------------- menu

    fn fs_menu(&mut self, pos: Pos2) {
        let path = self.file_context.clone();
        let item = self.fs_context_item && path.is_some();
        let is_root = path.as_deref() == Some(self.path.as_path());
        let is_scene = path.as_ref().is_some_and(|p| is_scene(p));
        let is_dir = path.as_ref().is_some_and(|p| p.is_dir());
        let favorite = path.as_ref().is_some_and(|p| self.is_favorite(p));
        let main_scene = self.config.as_ref().ok().map(|c| c.project.main_scene.clone()).unwrap_or_default();
        let is_main = path.as_ref().is_some_and(|p| self.rel_path(p) == main_scene);
        let mut items = Vec::new();

        if item {
            if is_dir {
                items.push(MenuItem::item("Open", FILE_OPEN).shortcut("Enter"));
                items.push(MenuItem::item("Expand Hierarchy", FILE_EXPAND_ALL));
                items.push(MenuItem::item("Collapse Hierarchy", FILE_COLLAPSE_ALL));
            } else {
                items.push(MenuItem::item(if is_scene { "Open Scene" } else { "Open" }, FILE_OPEN).shortcut("Enter"));
                if is_scene && !is_main {
                    items.push(MenuItem::item("Set as Main Scene", FILE_SET_MAIN_SCENE));
                }
            }
            items.push(MenuItem::separator());
        }

        // Godot only offers Create New for a single item (or empty space).
        let new_folder = MenuItem::item("Folder...", FILE_NEW_FOLDER);
        let new_scene = MenuItem::item("Scene...", FILE_NEW_SCENE);
        let new_script = MenuItem::item("Script...", FILE_NEW_SCRIPT);
        let new_resource = MenuItem::item("Resource...", 0).disabled();
        let new_text = MenuItem::item("TextFile...", FILE_NEW_TEXTFILE);
        items.push(MenuItem::submenu("Create New", vec![new_folder, new_scene, new_script, new_resource, new_text]));
        items.push(MenuItem::item("Import Files...", FILE_IMPORT));

        if item {
            items.push(MenuItem::separator());
            items.push(MenuItem::item("Copy Path", FILE_COPY_PATH).shortcut("Ctrl+Shift+C"));
            items.push(MenuItem::item("Copy Absolute Path", FILE_COPY_ABSOLUTE_PATH).shortcut("Ctrl+Alt+C"));
            if !is_root {
                items.push(MenuItem::item("Rename...", FILE_RENAME).shortcut("F2"));
                items.push(MenuItem::item("Duplicate...", FILE_DUPLICATE).shortcut("Ctrl+D"));
            }
            items.push(MenuItem::separator());
            if favorite {
                items.push(MenuItem::item("Remove from Favorites", FILE_REMOVE_FAVORITE));
            } else {
                items.push(MenuItem::item("Add to Favorites", FILE_ADD_FAVORITE));
            }
            if !is_root {
                items.push(MenuItem::separator());
                items.push(MenuItem::item("Delete", FILE_DELETE).shortcut("Delete"));
            }
            items.push(MenuItem::separator());
        }

        items.push(MenuItem::item("Open in File Manager", FILE_SHOW).shortcut("Ctrl+Alt+R"));
        self.ui.open_menu(items, pos);
    }

    /// Context-menu actions (constants in [`super::menus`]).
    pub(super) fn fs_action(&mut self, action: u32) {
        let path = self.file_context.clone().or_else(|| self.current_dir().map(Path::to_path_buf));
        match action {
            FILE_OPEN => {
                if let Some(p) = path {
                    self.open_file(p);
                }
            }
            FILE_SHOW => {
                let p = path.unwrap_or_else(|| self.path.clone());
                let dir = if p.is_dir() { p } else { p.parent().map(Path::to_path_buf).unwrap_or_else(|| self.path.clone()) };
                crate::util::open_folder(&dir);
            }
            FILE_NEW_FOLDER => self.open_file_dialog(FileOp::NewFolder { dir: self.target_dir() }),
            FILE_NEW_SCENE => self.open_file_dialog(FileOp::NewScene { dir: self.target_dir() }),
            FILE_NEW_SCRIPT => self.open_file_dialog(FileOp::NewScript { dir: self.target_dir() }),
            FILE_NEW_TEXTFILE => self.open_file_dialog(FileOp::NewTextFile { dir: self.target_dir() }),
            FILE_IMPORT => self.open_import_dialog(),
            FILE_RENAME => self.fs_rename_selected(),
            FILE_DUPLICATE => {
                if let Some(p) = path.filter(|p| p != &self.path) {
                    self.open_file_dialog(FileOp::Duplicate { path: p });
                }
            }
            FILE_DELETE => {
                if let Some(p) = path.filter(|p| p != &self.path) {
                    self.open_delete_dialog(p);
                }
            }
            FILE_COPY_PATH => {
                if let Some(p) = path {
                    self.clipboard = Some(self.res_url(&p));
                }
            }
            FILE_COPY_ABSOLUTE_PATH => {
                if let Some(p) = path {
                    self.clipboard = Some(p.display().to_string());
                }
            }
            FILE_SET_MAIN_SCENE => {
                if let Some(p) = path {
                    self.set_main_scene(&p);
                }
            }
            FILE_ADD_FAVORITE => {
                if let Some(p) = path {
                    self.add_favorite(&p);
                }
            }
            FILE_REMOVE_FAVORITE => {
                if let Some(p) = path {
                    self.remove_favorite(&p);
                }
            }
            FILE_EXPAND_ALL => {
                if let Some(p) = path {
                    self.expand_recursive(&p, true);
                }
            }
            FILE_COLLAPSE_ALL => {
                if let Some(p) = path {
                    self.expand_recursive(&p, false);
                }
            }
            _ => {}
        }
        self.file_context = None;
    }

    pub(super) fn fs_rename_selected(&mut self) {
        let Some(path) = self.fs_selected.clone().or_else(|| self.file_context.clone()) else { return };
        if path == self.path {
            return;
        }
        // Prefer editing the row in place, as Godot's Tree::edit_selected does.
        if let Some(&key) = self.file_keys.iter().find(|(_, p)| **p == path).map(|(k, _)| k) {
            self.ui.edit_tree_row(self.w.file_tree, key);
            return;
        }
        if let Some(&key) = self.list_keys.iter().find(|(_, p)| **p == path).map(|(k, _)| k) {
            self.ui.edit_tree_row(self.w.file_list, key);
            return;
        }
        self.open_file_dialog(FileOp::Rename { path });
    }

    pub(super) fn fs_delete_selected(&mut self) {
        if let Some(p) = self.fs_selected.clone().or_else(|| self.file_context.clone()).filter(|p| p != &self.path) {
            self.open_delete_dialog(p);
        }
    }

    pub(super) fn fs_duplicate_selected(&mut self) {
        if let Some(p) = self.fs_selected.clone().or_else(|| self.file_context.clone()).filter(|p| p != &self.path) {
            self.open_file_dialog(FileOp::Duplicate { path: p });
        }
    }

    pub(super) fn fs_copy_path(&mut self) {
        if let Some(p) = self.fs_selected.clone().or_else(|| self.file_context.clone()) {
            self.clipboard = Some(self.res_url(&p));
        }
    }

    pub(super) fn fs_copy_absolute(&mut self) {
        if let Some(p) = self.fs_selected.clone().or_else(|| self.file_context.clone()) {
            self.clipboard = Some(p.display().to_string());
        }
    }

    pub(super) fn fs_show_in_file_manager(&mut self) {
        if let Some(p) = self.fs_selected.clone().or_else(|| self.file_context.clone()) {
            let dir = if p.is_dir() { p } else { p.parent().map(Path::to_path_buf).unwrap_or_else(|| self.path.clone()) };
            crate::util::open_folder(&dir);
        }
    }

    // ------------------------------------------------------------ file ops

    pub(super) fn open_file_dialog(&mut self, op: FileOp) {
        self.close_dialog();
        let (modal, body, close) = ui_kit::dialog(&mut self.ui, op.title(), 480.0);
        let t = self.ui.theme.clone();
        self.ui.add(body, Control::label(op.label()));
        let row = self.ui.add(body, Control::hbox());
        let mut prefix = self.res_url(op.parent());
        if !prefix.ends_with('/') {
            prefix.push('/');
        }
        self.ui.add(row, Control::label(prefix).font(12.5).color(t.font_muted_color).v_center().ellipsis());
        let field = self.ui.add(row, Control::line_edit(op.default_name()).monospace());
        let root = op.root_label().map(|label| {
            let r = self.ui.add(body, Control::hbox());
            self.ui.add(r, Control::label(label).min(170.0, 0.0).v_center());
            self.ui.add(r, Control::option_button(&ROOT_CLASSES, if matches!(op, FileOp::NewScene { .. }) { 0 } else { 3 }))
        });
        let error = self.ui.add(body, Control::label("").color(crate::theme::BAD).wrap().hidden());
        let (ok, cancel) = ui_kit::ok_cancel(&mut self.ui, body, "Create", "Cancel");
        self.ui.focus(field);
        self.dialog = Some(Dialog::File { modal, close, field, error, root, ok, cancel, op });
    }

    /// Create or rename after a FileSystem dialog was accepted.
    pub(super) fn apply_file_dialog(&mut self, op: &FileOp, name: &str, root: usize) -> Result<(), String> {
        let class = ROOT_CLASSES.get(root).copied().unwrap_or("Node");
        match op {
            FileOp::NewFolder { dir } => self.create_folder(dir, name)?,
            FileOp::NewScene { dir } => self.create_scene(dir, name, class)?,
            FileOp::NewScript { dir } => self.create_script(dir, name, class)?,
            FileOp::NewTextFile { dir } => self.create_text_file(dir, name)?,
            FileOp::Rename { path } => self.rename_to(path, name)?,
            FileOp::Duplicate { path } => self.duplicate_to(path, name)?,
        }
        self.reload_files();
        Ok(())
    }

    fn create_folder(&mut self, dir: &Path, name: &str) -> Result<(), String> {
        validate_name(name)?;
        let path = dir.join(name.trim());
        if path.exists() {
            return Err(format!("{} already exists.", self.res_url(&path)));
        }
        std::fs::create_dir_all(&path).map_err(|e| format!("Could not create {}: {e}", self.res_url(&path)))?;
        self.expand(dir);
        self.fs_selected = Some(path.clone());
        self.notify(format!("Created {}", self.res_url(&path)));
        Ok(())
    }

    fn create_scene(&mut self, dir: &Path, name: &str, class: &str) -> Result<(), String> {
        validate_name(name)?;
        let mut name = name.trim().to_string();
        if !name.ends_with(".tscn") {
            name.push_str(".tscn");
        }
        let path = dir.join(&name);
        if path.exists() {
            return Err(format!("{} already exists.", self.res_url(&path)));
        }
        let stem = Path::new(&name).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "scene".into());
        let mut scene = Scene::new(class, &stem).map_err(|e| e.to_string())?;
        let text = format::to_string(&mut scene);
        std::fs::create_dir_all(dir).map_err(|e| format!("Could not create {}: {e}", self.res_url(dir)))?;
        std::fs::write(&path, text).map_err(|e| format!("Could not write {}: {e}", self.res_url(&path)))?;
        self.expand(dir);
        self.fs_selected = Some(path.clone());
        self.reload_files();
        self.notify(format!("Created {}", self.res_url(&path)));
        self.open_scene_file(path);
        Ok(())
    }

    fn create_script(&mut self, dir: &Path, name: &str, base: &str) -> Result<(), String> {
        validate_name(name)?;
        let mut name = name.trim().to_string();
        if !name.ends_with(".gd") {
            name.push_str(".gd");
        }
        let path = dir.join(&name);
        if path.exists() {
            return Err(format!("{} already exists.", self.res_url(&path)));
        }
        let text = format!("extends {base}\n\n\nfunc _ready() -> void:\n\tpass\n");
        std::fs::create_dir_all(dir).map_err(|e| format!("Could not create {}: {e}", self.res_url(dir)))?;
        std::fs::write(&path, text).map_err(|e| format!("Could not write {}: {e}", self.res_url(&path)))?;
        self.expand(dir);
        self.fs_selected = Some(path.clone());
        self.notify(format!("Created {}", self.res_url(&path)));
        // Godot opens a new script in the script editor.
        self.open_script(path);
        Ok(())
    }

    fn create_text_file(&mut self, dir: &Path, name: &str) -> Result<(), String> {
        validate_name(name)?;
        let path = dir.join(name.trim());
        if path.exists() {
            return Err(format!("{} already exists.", self.res_url(&path)));
        }
        std::fs::create_dir_all(dir).map_err(|e| format!("Could not create {}: {e}", self.res_url(dir)))?;
        std::fs::write(&path, "").map_err(|e| format!("Could not write {}: {e}", self.res_url(&path)))?;
        self.expand(dir);
        self.fs_selected = Some(path.clone());
        self.notify(format!("Created {}", self.res_url(&path)));
        Ok(())
    }

    fn rename_to(&mut self, path: &Path, name: &str) -> Result<(), String> {
        if path == self.path {
            return Err("The project root cannot be renamed.".into());
        }
        validate_name(name)?;
        let name = name.trim();
        if path.file_name().is_some_and(|n| n == name) {
            return Ok(());
        }
        let parent = path.parent().ok_or("This item has no parent folder.")?;
        let new_path = parent.join(name);
        if new_path.exists() {
            return Err(format!("{} already exists.", self.res_url(&new_path)));
        }
        let was_dir = path.is_dir();
        std::fs::rename(path, &new_path).map_err(|e| format!("Could not rename {}: {e}", self.res_url(path)))?;
        if !was_dir {
            move_sidecars(path, &new_path);
        }
        self.after_move(path, &new_path, was_dir);
        self.notify(format!("Renamed to {}", self.res_url(&new_path)));
        Ok(())
    }

    fn duplicate_to(&mut self, path: &Path, name: &str) -> Result<(), String> {
        if path == self.path {
            return Err("The project root cannot be duplicated.".into());
        }
        validate_name(name)?;
        let parent = path.parent().ok_or("This item has no parent folder.")?;
        let new_path = parent.join(name.trim());
        if new_path.exists() {
            return Err(format!("{} already exists.", self.res_url(&new_path)));
        }
        if path.is_dir() {
            crate::util::copy_dir(path, &new_path).map_err(|e| format!("Could not duplicate {}: {e}", self.res_url(path)))?;
        } else {
            std::fs::copy(path, &new_path).map_err(|e| format!("Could not duplicate {}: {e}", self.res_url(path)))?;
            copy_sidecars(path, &new_path);
        }
        self.expand(parent);
        self.fs_selected = Some(new_path.clone());
        self.notify(format!("Duplicated to {}", self.res_url(&new_path)));
        Ok(())
    }

    /// Move (or copy with Ctrl) by dragging in the tree or file list.
    fn move_to(&mut self, source: &Path, dir: &Path, copy: bool) -> Result<(), String> {
        if source == self.path {
            return Err("The project root cannot be moved.".into());
        }
        if !dir.is_dir() {
            return Err("Drop files on a folder.".into());
        }
        if source.is_dir() && (dir == source || dir.starts_with(source)) {
            return Err("A folder cannot be moved inside itself.".into());
        }
        let Some(name) = source.file_name() else { return Ok(()) };
        let new_path = dir.join(name);
        if new_path == source {
            return Ok(());
        }
        if new_path.exists() {
            return Err(format!("{} already exists.", self.res_url(&new_path)));
        }
        if copy {
            if source.is_dir() {
                crate::util::copy_dir(source, &new_path).map_err(|e| format!("Could not copy: {e}"))?;
            } else {
                std::fs::copy(source, &new_path).map_err(|e| format!("Could not copy: {e}"))?;
                copy_sidecars(source, &new_path);
            }
            self.notify(format!("Copied to {}", self.res_url(&new_path)));
        } else {
            let was_dir = source.is_dir();
            std::fs::rename(source, &new_path).map_err(|e| format!("Could not move: {e}"))?;
            if !was_dir {
                move_sidecars(source, &new_path);
            }
            self.after_move(source, &new_path, was_dir);
            self.notify(format!("Moved to {}", self.res_url(&new_path)));
        }
        self.expand(dir);
        self.fs_selected = Some(new_path);
        Ok(())
    }

    pub(super) fn open_delete_dialog(&mut self, path: PathBuf) {
        self.close_dialog();
        let (modal, body, close) = ui_kit::dialog(&mut self.ui, "Delete", 460.0);
        let what = if path.is_dir() { "folder" } else { "file" };
        self.ui.add(body, Control::label(format!("Delete the {what} \"{}\"?", self.res_url(&path))).wrap());
        self.ui.add(body, Control::label("This cannot be undone.").font(12.5).color(self.ui.theme.font_muted_color));
        let (ok, cancel) = ui_kit::ok_cancel(&mut self.ui, body, "Delete", "Cancel");
        self.dialog = Some(Dialog::Delete { modal, close, ok, cancel, path });
    }

    // ---------------------------------------------------------- importing

    /// The folder under the mouse in the FileSystem dock, or the current one.
    /// Godot's `get_folder_path_at_mouse_position` + current directory fallback.
    fn fs_drop_dir(&self, at: Option<Pos2>) -> PathBuf {
        if let Some(at) = at {
            if self.ui.rect(self.w.file_tree).contains(at) {
                if let Some(key) = self.ui.tree_row_at(self.w.file_tree, at) {
                    match key {
                        TREE_FAVORITES | TREE_ROOT => return self.path.clone(),
                        _ => {
                            if let Some(p) = self.file_keys.get(&key) {
                                return if p.is_dir() { p.clone() } else { p.parent().unwrap_or(&self.path).to_path_buf() };
                            }
                        }
                    }
                }
            } else if self.ui.rect(self.w.file_list).contains(at) {
                if let Some(key) = self.ui.tree_row_at(self.w.file_list, at) {
                    if key == LIST_UP {
                        return self.current_dir().and_then(Path::parent).unwrap_or(&self.path).to_path_buf();
                    }
                    if let Some(p) = self.list_keys.get(&key) {
                        return if p.is_dir() { p.clone() } else { p.parent().unwrap_or(&self.path).to_path_buf() };
                    }
                }
            }
        }
        self.current_dir().unwrap_or(&self.path).to_path_buf()
    }

    /// OS drop (or the Import dialog): copy files/folders into `dir`, like
    /// Godot's `EditorNode::_add_dropped_files_recursive`, then rescan.
    pub fn import_files(&mut self, paths: Vec<PathBuf>, at: Option<Pos2>) {
        if paths.is_empty() {
            return;
        }
        let dir = self.fs_drop_dir(at);
        self.import_into(paths, &dir);
    }

    pub(super) fn import_into(&mut self, paths: Vec<PathBuf>, dir: &Path) {
        if !dir.is_dir() {
            self.notify("Import target is not a folder.");
            return;
        }
        let (mut imported, mut skipped) = (0usize, 0usize);
        let mut first = None;
        for src in paths {
            if !src.exists() {
                skipped += 1;
                continue;
            }
            let Some(name) = src.file_name() else {
                skipped += 1;
                continue;
            };
            let dest = dir.join(name);
            if dest == src || (src.is_dir() && dir.starts_with(&src)) {
                skipped += 1;
                continue;
            }
            let result = if src.is_dir() {
                crate::util::copy_dir(&src, &dest)
            } else {
                std::fs::copy(&src, &dest).map(|_| ())
            };
            match result {
                Ok(()) => {
                    if !src.is_dir() {
                        copy_sidecars(&src, &dest);
                    }
                    imported += 1;
                    first.get_or_insert(dest);
                }
                Err(e) => {
                    self.log(format!("Could not import {}: {e}", src.display()));
                    skipped += 1;
                }
            }
        }
        self.reload_files();
        self.expand(dir);
        if let Some(p) = first {
            self.fs_selected = Some(p);
        }
        if imported > 0 {
            let extra = if skipped > 0 { format!(" ({skipped} skipped)") } else { String::new() };
            self.notify(format!("Imported {imported} item(s) into {}{extra}", self.res_url(dir)));
        } else if skipped > 0 {
            self.notify("Nothing was imported. Files already inside the target folder are skipped.");
        }
    }

    /// FileSystem > Import Files...: Godot's file dialog on the whole file
    /// system; the chosen files (or folders) are copied into the target folder.
    pub(super) fn open_import_dialog(&mut self) {
        use crate::file_dialog::{FileDialog, FileMode};
        self.close_dialog();
        let target = self.target_dir();
        let mut into = self.res_url(&target);
        if !into.ends_with('/') {
            into.push('/');
        }
        let title = format!("Import Files into {into}");
        let fd = FileDialog::open(&mut self.ui, &self.icons, FileMode::OpenAny, Some(&title), &home_dir(), None, &[]);
        self.files_dialog = Some((fd, super::FilePurpose::Import(target)));
    }

    pub(super) fn delete_path(&mut self, path: &Path) -> Result<(), String> {
        if path == self.path {
            return Err("The project root cannot be deleted.".into());
        }
        let is_dir = path.is_dir();
        let result = if is_dir { std::fs::remove_dir_all(path) } else { std::fs::remove_file(path) };
        result.map_err(|e| format!("Could not delete {}: {e}", self.res_url(path)))?;
        if !is_dir {
            remove_sidecars(path);
        }

        // Close tabs that pointed into the deleted tree.
        let mut i = 0;
        while i < self.docs.len() {
            if self.docs[i].file.as_ref().is_some_and(|f| f.starts_with(path)) {
                self.docs.remove(i);
            } else {
                i += 1;
            }
        }
        if self.docs.is_empty() {
            self.docs.push(Doc::empty());
        }
        self.current = self.current.min(self.docs.len() - 1);
        self.workspace_for_current();

        let gone: Vec<String> = self.prefs.favorites.iter().filter(|f| self.url_under(f, path)).cloned().collect();
        self.prefs.favorites.retain(|f| !gone.contains(f));
        let gone: Vec<String> = self.prefs.recent_scenes.iter().filter(|r| self.url_under(r, path)).cloned().collect();
        self.prefs.recent_scenes.retain(|r| !gone.contains(r));
        self.collapsed_dirs.retain(|d| !d.starts_with(path));
        self.fs_selected = self.fs_selected.clone().filter(|p| !p.starts_with(path));
        self.fs_history.retain(|v| !matches!(v, FsView::Dir(d) if d.starts_with(path)));
        self.fs_history_pos = self.fs_history_pos.min(self.fs_history.len().saturating_sub(1));
        if let FsView::Dir(d) = &self.fs_view
            && d.starts_with(path)
        {
            self.fs_view = FsView::Dir(self.path.clone());
            self.fs_history = vec![self.fs_view.clone()];
            self.fs_history_pos = 0;
        }
        self.rewrite_project_settings(path, None);
        self.save_prefs();
        self.reload_files();
        self.notify(format!("Deleted {}", self.res_url(path)));
        Ok(())
    }

    fn set_main_scene(&mut self, path: &Path) {
        if !is_scene(path) {
            self.notify("Only scenes can be the main scene.");
            return;
        }
        let rel = self.rel_path(path);
        match ProjectConfig::load(&self.path) {
            Ok(mut c) => {
                c.project.main_scene = rel.clone();
                match c.save(&self.path) {
                    Ok(()) => {
                        self.config = Ok(c);
                        self.notify(format!("Main scene set to res://{rel}"));
                    }
                    Err(e) => self.notify(format!("Could not save project settings: {e}")),
                }
            }
            Err(e) => self.notify(format!("Could not read project.neko: {e}")),
        }
    }

    // ------------------------------------------------------------ favorites

    fn is_favorite(&self, path: &Path) -> bool {
        let url = self.res_url(path);
        self.prefs.favorites.iter().any(|f| *f == url)
    }

    fn add_favorite(&mut self, path: &Path) {
        let url = self.res_url(path);
        if !self.prefs.favorites.contains(&url) {
            self.prefs.favorites.push(url.clone());
            self.prefs.favorites.sort();
            self.save_prefs();
            self.notify(format!("Added {url} to Favorites."));
        }
    }

    fn remove_favorite(&mut self, path: &Path) {
        let url = self.res_url(path);
        let before = self.prefs.favorites.len();
        self.prefs.favorites.retain(|f| *f != url);
        if self.prefs.favorites.len() != before {
            self.save_prefs();
            self.notify(format!("Removed {url} from Favorites."));
        }
    }

    fn favorite_paths(&self) -> Vec<PathBuf> {
        self.prefs.favorites.iter().filter_map(|u| self.abs_url(u)).filter(|p| p.exists()).collect()
    }

    // ------------------------------------------------------- move bookkeeping

    /// After a successful rename/move: rewrite `res://` references on disk,
    /// project settings, favorites, history and open scene paths, then reload
    /// clean scenes so their in-memory resources point at the new files.
    fn after_move(&mut self, old: &Path, new: &Path, was_dir: bool) {
        self.rewrite_resource_paths(old, new, was_dir);

        for d in &mut self.docs {
            if let Some(file) = &d.file
                && let Some(moved) = moved_path(file, old, Some(new))
            {
                d.file = Some(moved);
            }
        }
        self.rewrite_project_settings(old, Some(new));

        let old_url = self.res_url(old);
        let new_url = self.res_url(new);
        for f in &mut self.prefs.favorites {
            *f = replace_url_prefix(f, &old_url, &new_url);
        }
        for r in &mut self.prefs.recent_scenes {
            *r = replace_url_prefix(r, &old_url, &new_url);
        }
        self.save_prefs();

        let mut collapsed = std::collections::HashSet::new();
        for d in self.collapsed_dirs.drain() {
            collapsed.insert(moved_path(&d, old, Some(new)).unwrap_or(d));
        }
        self.collapsed_dirs = collapsed;
        if let Some(p) = &self.fs_selected
            && let Some(moved) = moved_path(p, old, Some(new))
        {
            self.fs_selected = Some(moved);
        }
        for v in &mut self.fs_history {
            if let FsView::Dir(d) = v
                && let Some(moved) = moved_path(d, old, Some(new))
            {
                *d = moved;
            }
        }
        if let FsView::Dir(d) = &mut self.fs_view
            && let Some(moved) = moved_path(d, old, Some(new))
        {
            *d = moved;
        }

        // Clean scenes can be reloaded; dirty ones are left alone with a warning.
        let mut stale = false;
        for i in 0..self.docs.len() {
            if !self.docs[i].has_root || self.docs[i].file.is_none() {
                continue;
            }
            if self.docs[i].dirty {
                stale = true;
            } else if let Some(file) = self.docs[i].file.clone() {
                match Doc::load(file) {
                    Ok(doc) => self.docs[i] = doc,
                    Err(e) => self.log(format!("warning: could not reload a scene after the move: {e}")),
                }
            }
        }
        if stale {
            self.notify("Some unsaved scenes still reference the old path; save them to update it.");
        }
        self.reload_files();
    }

    /// Replace references to the moved file/folder in text resources.
    fn rewrite_resource_paths(&self, old: &Path, new: &Path, was_dir: bool) {
        let old_url = self.res_url(old);
        let new_url = self.res_url(new);
        let mut files = Vec::new();
        walk(&self.path, &mut files);
        for file in files {
            if !file.extension().is_some_and(|e| TEXT_EXTENSIONS.iter().any(|x| e == *x)) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&file) else { continue };
            let updated = replace_refs(&text, &old_url, &new_url, was_dir);
            if updated != text {
                let _ = std::fs::write(&file, updated);
            }
        }
    }

    /// Keep `run/main_scene` valid when the main scene (or its folder) moves
    /// or is deleted (`new` is then `None`).
    fn rewrite_project_settings(&mut self, old: &Path, new: Option<&Path>) {
        let Ok(mut c) = ProjectConfig::load(&self.path) else { return };
        let main = self.path.join(&c.project.main_scene);
        if main != old && !main.starts_with(old) {
            return;
        }
        c.project.main_scene = match new {
            Some(new) => {
                let moved = moved_path(&main, old, Some(new)).unwrap_or_default();
                moved.strip_prefix(&self.path).map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_default()
            }
            None => String::new(),
        };
        if c.save(&self.path).is_ok() {
            self.config = Ok(c);
        }
    }

    // ---------------------------------------------------------------- paths

    fn rel_path(&self, p: &Path) -> String {
        // `res://` always uses forward slashes, also on Windows.
        p.strip_prefix(&self.path).map(|r| r.to_string_lossy().replace('\\', "/")).unwrap_or_else(|_| p.to_string_lossy().replace('\\', "/"))
    }

    fn res_url(&self, p: &Path) -> String {
        let rel = self.rel_path(p);
        if rel.is_empty() { "res://".into() } else { format!("res://{rel}") }
    }

    fn abs_url(&self, url: &str) -> Option<PathBuf> {
        let rel = url.strip_prefix("res://")?;
        Some(self.path.join(rel))
    }

    fn url_under(&self, url: &str, path: &Path) -> bool {
        self.abs_url(url).is_some_and(|p| p.starts_with(path))
    }

    pub(super) fn reload_files(&mut self) {
        self.files_scanned = None;
    }

    // --------------------------------------------------------------- drawing

    pub(super) fn sync_files(&mut self) {
        if self.files_scanned.is_none_or(|t| t.elapsed() > Duration::from_secs(3)) {
            let mut out = Vec::new();
            scan(&self.path, 0, &mut out);
            self.files = out;
            self.files_scanned = Some(Instant::now());
        }
        let filter = self.ui.text(self.w.file_filter).trim().to_lowercase();

        self.sync_file_tree(&filter);
        self.sync_file_list(&filter);

        self.ui.set_visible(self.w.file_list_panel, self.fs_show_list);
        self.ui.set_disabled(self.w.file_back, self.fs_history_pos == 0);
        self.ui.set_disabled(self.w.file_forward, self.fs_history_pos + 1 >= self.fs_history.len());
        self.ui.set_checked(self.w.file_split, self.fs_show_list);
        self.ui.set_checked(self.w.file_sort, self.fs_sort_types);

        let url = self.view_url();
        if self.ui.focused() != Some(self.w.file_path) && self.fs_path_shown != url {
            self.ui.set_text(self.w.file_path, url.clone());
        }
        self.fs_path_shown = url;
    }

    fn sync_file_tree(&mut self, filter: &str) {
        let folder_icon = self.icons.get("Folder").map(|i| i.tinted(Color32::from_rgb(0x87, 0xb0, 0xf0)));
        let mut rows = Vec::new();
        let mut keys = Vec::new();

        let favorites = self.favorite_paths();
        let mut fav = TreeRow::new(TREE_FAVORITES, 0, "Favorites");
        fav.icon = self.icons.get("Favorites");
        fav.has_children = !favorites.is_empty();
        fav.open = self.fs_favorites_open;
        rows.push(fav);
        if self.fs_favorites_open {
            for (i, path) in favorites.iter().enumerate() {
                let key = FAVORITE_KEY_BASE + i as u64;
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string());
                let mut row = TreeRow::new(key, 1, name);
                row.icon = self.file_icon(path, folder_icon);
                row.color = Some(Color32::from_rgb(0xd4, 0xc7, 0x9e));
                rows.push(row);
                keys.push((key, path.clone()));
            }
        }

        let mut root = TreeRow::new(TREE_ROOT, 0, "res://");
        root.icon = folder_icon;
        root.has_children = true;
        root.open = true;
        rows.push(root);

        let show_all_folders = !filter.is_empty();
        let mut hidden_under: Option<&Path> = None;
        for (i, FileEntry { path, depth, is_dir }) in self.files.iter().enumerate() {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if !show_all_folders
                && let Some(h) = hidden_under
            {
                if path.starts_with(h) {
                    continue;
                }
                hidden_under = None;
            }
            if !*is_dir && !filter.is_empty() && !name.to_lowercase().contains(filter) {
                continue;
            }
            let open = show_all_folders || !self.collapsed_dirs.contains(path);
            if *is_dir && !open {
                hidden_under = Some(path);
            }
            let child_depth = self.files.get(i + 1).map_or(0, |e| e.depth);
            let has_children = *is_dir && child_depth > *depth && self.files.get(i + 1).is_some_and(|e| e.path.starts_with(path));
            let mut row = TreeRow::new(i as u64, *depth as u16 + 1, name);
            row.has_children = has_children;
            row.open = open;
            row.icon = self.file_icon(path, folder_icon);
            row.draggable = true;
            rows.push(row);
            keys.push((i as u64, path.clone()));
        }

        self.file_keys = keys.into_iter().collect();
        let selected = if matches!(self.fs_view, FsView::Favorites) {
            Some(TREE_FAVORITES)
        } else if self.fs_selected.as_deref() == Some(self.path.as_path()) {
            Some(TREE_ROOT)
        } else {
            self.fs_selected.as_ref().and_then(|p| key_for(&self.file_keys, p))
        };
        self.ui.set_rows(self.w.file_tree, rows);
        self.ui.set_tree_selected(self.w.file_tree, selected);
    }

    fn sync_file_list(&mut self, filter: &str) {
        let folder_icon = self.icons.get("Folder").map(|i| i.tinted(Color32::from_rgb(0x87, 0xb0, 0xf0)));
        let mut rows = Vec::new();
        let mut keys = Vec::new();

        match &self.fs_view {
            FsView::Favorites => {
                for (i, path) in self.favorite_paths().iter().enumerate() {
                    let key = i as u64 + 1;
                    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string());
                    if !filter.is_empty() && !name.to_lowercase().contains(filter) {
                        continue;
                    }
                    let mut row = TreeRow::new(key, 0, name);
                    row.icon = self.file_icon(path, folder_icon);
                    row.draggable = true;
                    rows.push(row);
                    keys.push((key, path.clone()));
                }
                if rows.is_empty() {
                    let mut row = TreeRow::new(LIST_UP, 0, "No favorites yet. Right-click a file and choose \"Add to Favorites\".");
                    row.color = Some(Color32::from_gray(150));
                    rows.push(row);
                }
            }
            FsView::Dir(dir) => {
                let up = dir != &self.path;
                if up {
                    let mut row = TreeRow::new(LIST_UP, 0, "..");
                    row.icon = folder_icon;
                    rows.push(row);
                }
                let mut entries: Vec<&FileEntry> = self.files.iter().filter(|e| e.path.parent() == Some(dir.as_path())).collect();
                entries.sort_by(|a, b| {
                    let (a_name, b_name) = (a.path.file_name().unwrap_or_default().to_string_lossy().to_lowercase(), b.path.file_name().unwrap_or_default().to_string_lossy().to_lowercase());
                    if a.is_dir != b.is_dir {
                        return b.is_dir.cmp(&a.is_dir);
                    }
                    if self.fs_sort_types {
                        let a_ext = a.path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
                        let b_ext = b.path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
                        return a_ext.cmp(&b_ext).then(a_name.cmp(&b_name));
                    }
                    a_name.cmp(&b_name)
                });
                for (i, e) in entries.into_iter().enumerate() {
                    let name = e.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    if !e.is_dir && !filter.is_empty() && !name.to_lowercase().contains(filter) {
                        continue;
                    }
                    let key = i as u64 + 1;
                    let mut row = TreeRow::new(key, 0, name);
                    row.icon = self.file_icon(&e.path, folder_icon);
                    row.draggable = true;
                    rows.push(row);
                    keys.push((key, e.path.clone()));
                }
            }
        }

        self.list_keys = keys.into_iter().collect();
        let selected = self.fs_selected.as_ref().and_then(|p| key_for(&self.list_keys, p));
        self.ui.set_rows(self.w.file_list, rows);
        self.ui.set_tree_selected(self.w.file_list, selected);
    }

    fn file_icon(&self, path: &Path, folder: Option<neko_ui::Image>) -> Option<neko_ui::Image> {
        if path.is_dir() {
            folder
        } else if is_scene(path) {
            self.icons.get("PackedScene")
        } else if path.extension().is_some_and(|e| e == "gd") {
            self.icons.get("Script")
        } else {
            self.icons.tinted("File", Color32::from_gray(200))
        }
    }

    /// File > Show in FileSystem: open its folder in the dock and select it.
    pub(super) fn show_in_filesystem(&mut self, p: PathBuf) {
        if let Some(dir) = p.parent().map(PathBuf::from) {
            self.expand(&dir);
            self.navigate(FsView::Dir(dir), true);
        }
        self.fs_selected = Some(p);
    }

    /// Double-click in the FileSystem dock: scenes open, folders navigate.
    pub(super) fn open_file(&mut self, p: PathBuf) {
        if p.is_dir() {
            self.expand(&p);
            self.navigate(FsView::Dir(p), true);
        } else if is_scene(&p) {
            self.open_scene_file(p);
        } else if p.extension().is_some_and(|e| e == "gd") {
            // Scripts open in the Script workspace, like Godot.
            self.open_script(p);
        } else {
            // Scripts, images and every other resource open in the OS.
            crate::util::open_folder(&p);
        }
    }
}

// ------------------------------------------------------------------ helpers

fn is_scene(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "tscn" || e == "scn")
}

/// Where the Import dialog starts browsing.
fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_default()
}

fn validate_name(name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Enter a name.".into());
    }
    if name == "." || name == ".." || name.contains('/') || name.contains('\\') {
        return Err("A name cannot contain path separators.".into());
    }
    if name.chars().any(|c| c == '\0') {
        return Err("That name is not valid.".into());
    }
    Ok(())
}

fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn key_for(map: &std::collections::HashMap<u64, PathBuf>, path: &Path) -> Option<u64> {
    map.iter().find(|(_, p)| p.as_path() == path).map(|(k, _)| *k)
}

fn moved_path(path: &Path, old: &Path, new: Option<&Path>) -> Option<PathBuf> {
    if path == old {
        return new.map(Path::to_path_buf);
    }
    let rest = path.strip_prefix(old).ok()?;
    new.map(|n| n.join(rest))
}

/// `foo.png.import` / `foo.gd.uid`: Godot's sidecars follow their resource.
fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

fn move_sidecars(old: &Path, new: &Path) {
    for suffix in [".uid", ".import"] {
        let from = sidecar(old, suffix);
        if from.is_file() {
            let _ = std::fs::rename(&from, sidecar(new, suffix));
        }
    }
}

fn copy_sidecars(old: &Path, new: &Path) {
    for suffix in [".uid", ".import"] {
        let from = sidecar(old, suffix);
        if from.is_file() {
            let _ = std::fs::copy(&from, sidecar(new, suffix));
        }
    }
}

fn remove_sidecars(path: &Path) {
    for suffix in [".uid", ".import"] {
        let file = sidecar(path, suffix);
        if file.is_file() {
            let _ = std::fs::remove_file(file);
        }
    }
}

/// `Player.tscn` -> `Player_copy.tscn`, `Player_copy2.tscn`, ...
fn unique_name(path: &Path) -> String {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "copy".into());
    let parent = path.parent().unwrap_or(Path::new("."));
    let (stem, ext) = match path.extension() {
        Some(e) if !path.is_dir() => (path.file_stem().unwrap_or_default().to_string_lossy().into_owned(), format!(".{}", e.to_string_lossy())),
        _ => (name.clone(), String::new()),
    };
    for i in 1..10_000 {
        let suffix = if i == 1 { "_copy".to_string() } else { format!("_copy{i}") };
        let candidate = format!("{stem}{suffix}{ext}");
        if !parent.join(&candidate).exists() {
            return candidate;
        }
    }
    format!("{stem}_copy{ext}")
}

/// Rewrite `res://old` inside text, only when the next character really ends
/// the path (`"`, `/`, whitespace...), so `res://a` never hits `res://abc`.
fn replace_refs(text: &str, old: &str, new: &str, was_dir: bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find(old) {
        let after = &rest[i + old.len()..];
        let boundary = if was_dir {
            after.starts_with('/')
        } else {
            matches!(after.chars().next(), None | Some('"' | '\'' | '/' | ')' | ']' | '}' | ',' | ';' | ' ' | '\n' | '\r' | '\t'))
        };
        out.push_str(&rest[..i]);
        if boundary {
            out.push_str(new);
        } else {
            out.push_str(old);
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

fn replace_url_prefix(url: &str, old: &str, new: &str) -> String {
    if url == old {
        new.to_string()
    } else if let Some(rest) = url.strip_prefix(&format!("{old}/")) {
        format!("{new}/{rest}")
    } else {
        url.to_string()
    }
}

/// Project files, folders first, skipping build output and metadata.
fn scan(dir: &Path, depth: usize, out: &mut Vec<FileEntry>) {
    if depth > 16 || out.len() > 50_000 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<(bool, PathBuf)> = rd
        .flatten()
        .filter(|e| {
            let n = e.file_name();
            let n = n.to_string_lossy();
            // Godot's FileSystem dock hides import metadata, UID sidecars and
            // its own hidden folders. Neko also hides Cargo's build output.
            !n.starts_with('.')
                && n != "target"
                && n != "node_modules"
                && !n.ends_with(".import")
                && !n.ends_with(".uid")
        })
        .map(|e| (e.file_type().is_ok_and(|t| t.is_dir()), e.path()))
        .collect();
    entries.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    for (is_dir, path) in entries {
        out.push(FileEntry { path: path.clone(), depth, is_dir });
        if is_dir {
            scan(&path, depth + 1, out);
        }
    }
}

/// Every non-hidden project file (for dependency rewrites).
fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let path = e.path();
        let name = e.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || name == "target" || name == "node_modules" {
            continue;
        }
        if e.file_type().is_ok_and(|t| t.is_dir()) {
            walk(&path, out);
        } else {
            out.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_project(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("neko-fs-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(dir.join("scenes")).unwrap();
        std::fs::write(
            dir.join("project.neko"),
            "config_version=5\n\n[application]\n\nconfig/name=\"FS Test\"\nrun/main_scene=\"res://scenes/main.tscn\"\n\n[neko]\n\ntemplate=\"2d\"\nengine=\"0.1.0\"\nbackends/window=\"winit\"\nbackends/render=\"wgpu\"\nbackends/audio=\"none\"\ntargets/platforms=PackedStringArray(\"linux\")\n",
        )
        .unwrap();
        let mut scene = Scene::new("Node2D", "Main").unwrap();
        std::fs::write(dir.join("scenes/main.tscn"), format::to_string(&mut scene)).unwrap();
        dir
    }

    #[test]
    fn reference_rewrites_respect_path_boundaries() {
        let text = r#"path="res://art/cat.png" other="res://art/cat.png.import" keep="res://art/cat2.png""#;
        let out = replace_refs(text, "res://art/cat.png", "res://art/dog.png", false);
        assert_eq!(out, r#"path="res://art/dog.png" other="res://art/cat.png.import" keep="res://art/cat2.png""#);
        let folder = replace_refs("a=res://art/cat.png b=res://art/cat2.png", "res://art", "res://images", true);
        assert_eq!(folder, "a=res://images/cat.png b=res://images/cat2.png");
    }

    #[test]
    fn name_validation_and_normalize() {
        assert!(validate_name("player.tscn").is_ok());
        assert!(validate_name("").is_err());
        assert!(validate_name("a/b").is_err());
        assert!(validate_name("..").is_err());
        assert_eq!(normalize(Path::new("/a/b/../c/./d")), PathBuf::from("/a/c/d"));
    }

    #[test]
    fn duplicate_names_start_with_copy() {
        let dir = temp_project("names");
        std::fs::write(dir.join("player.tscn"), "").unwrap();
        assert_eq!(unique_name(&dir.join("player.tscn")), "player_copy.tscn");
        std::fs::write(dir.join("player_copy.tscn"), "").unwrap();
        assert_eq!(unique_name(&dir.join("player.tscn")), "player_copy2.tscn");
        std::fs::create_dir(dir.join("levels")).unwrap();
        assert_eq!(unique_name(&dir.join("levels")), "levels_copy");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn creates_renames_duplicates_and_deletes() {
        let dir = temp_project("ops");
        let mut ed = Editor::open(dir.clone());

        ed.create_folder(&dir, "art").unwrap();
        assert!(dir.join("art").is_dir());

        ed.create_scene(&dir, "level", "Node2D").unwrap();
        let scene = dir.join("level.tscn");
        assert!(scene.is_file());
        assert!(ed.docs.iter().any(|d| d.file.as_deref() == Some(scene.as_path())));

        // A sidecar follows the resource, and references in other scenes move.
        std::fs::write(dir.join("level.tscn.uid"), "uid://level").unwrap();
        std::fs::write(dir.join("scenes/main.tscn"), format!("[gd_scene format=3]\n\n[ext_resource type=\"PackedScene\" path=\"res://level.tscn\" id=\"1\"]\n")).unwrap();
        ed.rename_to(&scene, "renamed.tscn").unwrap();
        let renamed = dir.join("renamed.tscn");
        assert!(renamed.is_file() && !scene.exists());
        assert!(dir.join("renamed.tscn.uid").is_file());
        let main = std::fs::read_to_string(dir.join("scenes/main.tscn")).unwrap();
        assert!(main.contains("res://renamed.tscn"), "{main}");

        ed.duplicate_to(&renamed, "copy.tscn").unwrap();
        assert!(dir.join("copy.tscn").is_file());
        assert!(dir.join("copy.tscn.uid").is_file());

        ed.delete_path(&dir.join("copy.tscn")).unwrap();
        assert!(!dir.join("copy.tscn").exists());
        assert!(!dir.join("copy.tscn.uid").exists());

        ed.set_main_scene(&renamed);
        let config = ProjectConfig::load(&dir).unwrap();
        assert_eq!(config.project.main_scene, "renamed.tscn");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn imports_files_and_folders_into_the_project() {
        let dir = temp_project("import");
        let outside = std::env::temp_dir().join(format!("neko-outside-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(outside.join("pack")).unwrap();
        std::fs::write(outside.join("cat.png"), "png").unwrap();
        std::fs::write(outside.join("cat.png.import"), "[remap]").unwrap();
        std::fs::write(outside.join("pack/level.tscn"), "[gd_scene format=3]\n").unwrap();

        let mut ed = Editor::open(dir.clone());
        ed.import_into(vec![outside.join("cat.png"), outside.join("pack")], &dir);
        assert!(dir.join("cat.png").is_file());
        assert!(dir.join("cat.png.import").is_file(), "sidecars follow imported files");
        assert!(dir.join("pack/level.tscn").is_file());

        // Dropping again overwrites, like Godot's DirAccess::copy.
        std::fs::write(outside.join("cat.png"), "new").unwrap();
        ed.import_into(vec![outside.join("cat.png")], &dir);
        assert_eq!(std::fs::read_to_string(dir.join("cat.png")).unwrap(), "new");

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    fn favorites_survive_moves_and_deletes() {
        let dir = temp_project("favorites");
        let mut ed = Editor::open(dir.clone());
        std::fs::write(dir.join("art.png"), "").unwrap();
        ed.add_favorite(&dir.join("art.png"));
        assert!(ed.is_favorite(&dir.join("art.png")));
        ed.rename_to(&dir.join("art.png"), "cat.png").unwrap();
        assert!(ed.is_favorite(&dir.join("cat.png")));
        ed.delete_path(&dir.join("cat.png")).unwrap();
        assert!(!ed.is_favorite(&dir.join("cat.png")));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
