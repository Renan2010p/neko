//! The editor's menus, item for item as Godot 4.7 builds them in
//! `editor/editor_node.cpp` (`_build_file_menu`, `_build_project_menu`,
//! settings and help menus) and `debugger_editor_plugin.cpp` (Debug).
//! Items Neko does not implement yet are shown disabled, never removed.

use neko::project::BackendChoice;
use neko::project::scaffold::ENGINE_VERSION;
use neko_ui::{Menu, MenuItem};

use super::{Doc, Editor};
use crate::util;

pub(super) const NEW_SCENE: u32 = 1;
pub(super) const OPEN_SCENE: u32 = 2;
pub(super) const SAVE_SCENE: u32 = 3;
pub(super) const SAVE_AS: u32 = 4;
pub(super) const SAVE_ALL: u32 = 5;
pub(super) const UNDO: u32 = 6;
pub(super) const REDO: u32 = 7;
pub(super) const RELOAD_SCENE: u32 = 8;
pub(super) const CLOSE_SCENE: u32 = 9;
pub(super) const CLOSE_ALL: u32 = 10;
pub(super) const QUIT: u32 = 11;
pub(super) const PROJECT_SETTINGS: u32 = 20;
pub(super) const OPEN_USER_DATA: u32 = 21;
pub(super) const RELOAD_PROJECT: u32 = 22;
pub(super) const QUIT_TO_LIST: u32 = 23;
pub(super) const VISIBLE_COLLISIONS: u32 = 30;
pub(super) const FULLSCREEN: u32 = 40;
pub(super) const OPEN_DATA_FOLDER: u32 = 41;
pub(super) const ONLINE_DOCS: u32 = 50;
pub(super) const COPY_SYSTEM_INFO: u32 = 51;
pub(super) const ABOUT: u32 = 52;
pub(super) const RECENT_BASE: u32 = 100;
pub(super) const CLEAR_RECENT: u32 = 199;
pub(super) const VIEW_CENTER: u32 = 200;
pub(super) const VIEW_GRID: u32 = 201;
pub(super) const VIEW_COLLISIONS: u32 = 202;
pub(super) const RESET_POSITION: u32 = 210;
pub(super) const RESET_ROTATION: u32 = 211;
pub(super) const RESET_SCALE: u32 = 212;
// Scene tree context menu (SceneTreeDock).
pub(super) const NODE_ADD: u32 = 300;
pub(super) const NODE_RENAME: u32 = 301;
pub(super) const NODE_MOVE_UP: u32 = 302;
pub(super) const NODE_MOVE_DOWN: u32 = 303;
pub(super) const NODE_DUPLICATE: u32 = 304;
pub(super) const NODE_DELETE: u32 = 305;
pub(super) const NODE_ATTACH_SCRIPT: u32 = 306;
pub(super) const NODE_DETACH_SCRIPT: u32 = 307;
// FileSystem context menu (FileSystemDock::_file_option).
pub(super) const FILE_OPEN: u32 = 400;
pub(super) const FILE_SHOW: u32 = 401;
pub(super) const FILE_NEW_SCENE: u32 = 402;
pub(super) const FILE_COPY_PATH: u32 = 403;
pub(super) const FILE_COPY_ABSOLUTE_PATH: u32 = 404;
pub(super) const FILE_RENAME: u32 = 405;
pub(super) const FILE_DUPLICATE: u32 = 406;
pub(super) const FILE_DELETE: u32 = 407;
pub(super) const FILE_NEW_FOLDER: u32 = 408;
pub(super) const FILE_NEW_SCRIPT: u32 = 409;
pub(super) const FILE_SET_MAIN_SCENE: u32 = 410;
pub(super) const FILE_ADD_FAVORITE: u32 = 411;
pub(super) const FILE_REMOVE_FAVORITE: u32 = 412;
pub(super) const FILE_EXPAND_ALL: u32 = 413;
pub(super) const FILE_COLLAPSE_ALL: u32 = 414;
pub(super) const FILE_NEW_TEXTFILE: u32 = 415;
pub(super) const FILE_IMPORT: u32 = 416;
pub(super) const FILE_LAST: u32 = 416;

/// Not available yet: shown, disabled.
fn todo(label: &str, shortcut: &str) -> MenuItem {
    MenuItem::item(label, 0).shortcut(shortcut).disabled()
}

fn item(label: &str, action: u32, shortcut: &str) -> MenuItem {
    MenuItem::item(label, action).shortcut(shortcut)
}

impl Editor {
    pub(super) fn build_menus(&self) -> Vec<Menu> {
        let doc = self.doc();
        let mut recent: Vec<MenuItem> = self.prefs.recent_scenes.iter().enumerate().map(|(i, r)| MenuItem::item(r.clone(), RECENT_BASE + i as u32)).collect();
        if !recent.is_empty() {
            recent.push(MenuItem::separator());
        }
        recent.push(MenuItem::item("Clear Recent Scenes", CLEAR_RECENT).enabled(!self.prefs.recent_scenes.is_empty()));

        let scene = vec![
            item("New Scene", NEW_SCENE, "Ctrl+N"),
            todo("New Inherited Scene...", "Ctrl+Shift+N"),
            item("Open Scene...", OPEN_SCENE, "Ctrl+O"),
            todo("Reopen Closed Scene", "Ctrl+Shift+T"),
            MenuItem::submenu("Open Recent", recent),
            MenuItem::separator(),
            item("Save Scene", SAVE_SCENE, "Ctrl+S"),
            item("Save Scene As...", SAVE_AS, "Ctrl+Shift+S").enabled(doc.has_root),
            item("Save All Scenes", SAVE_ALL, "Ctrl+Shift+Alt+S"),
            MenuItem::separator(),
            todo("Quick Open...", "Shift+Alt+O"),
            todo("Quick Open Scene...", "Ctrl+Shift+O"),
            todo("Quick Open Script...", "Ctrl+Alt+O"),
            MenuItem::separator(),
            MenuItem::submenu("Export As...", vec![todo("MeshLibrary...", "")]),
            MenuItem::separator(),
            item("Undo", UNDO, "Ctrl+Z").enabled(!doc.undo.is_empty()),
            item("Redo", REDO, "Ctrl+Shift+Z").enabled(!doc.redo.is_empty()),
            MenuItem::separator(),
            item("Reload Saved Scene", RELOAD_SCENE, "").enabled(doc.file.as_ref().is_some_and(|f| f.is_file())),
            item("Close Scene", CLOSE_SCENE, "Ctrl+Shift+W"),
            item("Close All Scenes", CLOSE_ALL, ""),
            MenuItem::separator(),
            item("Quit", QUIT, "Ctrl+Q"),
        ];
        let project = vec![
            item("Project Settings...", PROJECT_SETTINGS, ""),
            todo("Find in Files...", "Ctrl+Shift+F"),
            MenuItem::separator(),
            MenuItem::submenu("Version Control", vec![todo("Create/Override Version Control Metadata...", ""), todo("Version Control Settings...", "")]),
            MenuItem::separator(),
            todo("Export...", ""),
            todo("Pack Project as ZIP...", ""),
            todo("Install Android Build Template...", ""),
            item("Open User Data Folder", OPEN_USER_DATA, ""),
            MenuItem::separator(),
            MenuItem::submenu("Tools", vec![todo("Orphan Resource Explorer...", ""), todo("Engine Compilation Configuration Editor...", ""), todo("Upgrade Project Files...", "")]),
            MenuItem::separator(),
            item("Reload Current Project", RELOAD_PROJECT, ""),
            item("Quit to Project List", QUIT_TO_LIST, "Ctrl+Shift+Q"),
        ];
        let check_todo = |l: &str| MenuItem::item(l, 0).check(false).disabled();
        let debug = vec![
            check_todo("Deploy with Remote Debug"),
            check_todo("Small Deploy with Network Filesystem"),
            MenuItem::separator(),
            MenuItem::item("Visible Collision Shapes", VISIBLE_COLLISIONS).check(self.debug_collisions),
            check_todo("Visible Paths"),
            check_todo("Visible Navigation"),
            check_todo("Visible Avoidance"),
            MenuItem::separator(),
            check_todo("Debug CanvasItem Redraws"),
            MenuItem::separator(),
            check_todo("Synchronize Scene Changes"),
            check_todo("Synchronize Script Changes"),
            check_todo("Keep Debug Server Open"),
            MenuItem::separator(),
            todo("Customize Run Instances...", ""),
        ];
        let docks = ["Scene", "Import", "FileSystem", "Inspector", "Signals", "Groups", "Output", "Debugger", "Audio", "Animation", "Shader Editor"]
            .into_iter()
            .map(|d| MenuItem::item(d, 0).check(true).disabled())
            .collect();
        let editor = vec![
            todo("Editor Settings...", ""),
            todo("Command Palette...", "Ctrl+Shift+P"),
            MenuItem::separator(),
            MenuItem::submenu("Editor Docks", docks),
            MenuItem::submenu("Editor Layout", vec![todo("Save Layout...", ""), todo("Delete Layout...", ""), MenuItem::separator(), todo("Default", "")]),
            MenuItem::separator(),
            todo("Take Screenshot", "Ctrl+F12"),
            item("Toggle Fullscreen", FULLSCREEN, "Shift+F11"),
            MenuItem::separator(),
            item("Open Editor Data Folder", OPEN_DATA_FOLDER, ""),
            todo("Open Editor Settings Folder", ""),
            MenuItem::separator(),
            todo("Manage Editor Features...", ""),
            todo("Manage Export Templates...", ""),
            todo("Configure FBX Importer...", ""),
        ];
        let help = vec![
            todo("Search Help...", "F1"),
            MenuItem::separator(),
            item("Online Documentation", ONLINE_DOCS, ""),
            todo("Forum", ""),
            todo("Community", ""),
            MenuItem::separator(),
            item("Copy System Info", COPY_SYSTEM_INFO, ""),
            todo("Report a Bug", ""),
            todo("Suggest a Feature", ""),
            todo("Send Docs Feedback", ""),
            MenuItem::separator(),
            item("About Neko...", ABOUT, ""),
            todo("Support Neko Development", ""),
        ];
        vec![
            Menu { title: "Scene".into(), items: scene },
            Menu { title: "Project".into(), items: project },
            Menu { title: "Debug".into(), items: debug },
            Menu { title: "Editor".into(), items: editor },
            Menu { title: "Help".into(), items: help },
        ]
    }

    /// The viewport toolbar's "Transform" and "View" menus.
    pub(super) fn view_menus(&self) -> Vec<Menu> {
        let has_sel = self.doc().selected.is_some() && self.doc().has_root;
        vec![
            Menu {
                title: "Transform".into(),
                items: vec![
                    item("Reset Position", RESET_POSITION, "").enabled(has_sel),
                    item("Reset Rotation", RESET_ROTATION, "").enabled(has_sel),
                    item("Reset Scale", RESET_SCALE, "").enabled(has_sel),
                ],
            },
            Menu {
                title: "View".into(),
                items: vec![
                    MenuItem::item("Grid", VIEW_GRID).check(self.show_grid),
                    MenuItem::item("Collision Shapes", VIEW_COLLISIONS).check(self.show_collisions),
                    MenuItem::separator(),
                    item("Center View", VIEW_CENTER, ""),
                ],
            },
        ]
    }

    pub(super) fn menu_action(&mut self, action: u32) {
        match action {
            NEW_SCENE => self.new_scene(),
            OPEN_SCENE => self.open_scene_dialog(),
            SAVE_SCENE => {
                self.save();
            }
            SAVE_AS => {
                if self.doc().has_root {
                    let cur = self.doc().file.as_ref().map(|f| self.rel(f).trim_start_matches("res://").to_string()).unwrap_or_else(|| "scenes/scene.tscn".into());
                    self.open_save_as(self.current, cur);
                }
            }
            SAVE_ALL => self.save_all(),
            UNDO => {
                self.doc_mut().undo();
            }
            REDO => {
                self.doc_mut().redo();
            }
            RELOAD_SCENE => {
                if let Some(f) = self.doc().file.clone() {
                    match Doc::load(f) {
                        Ok(d) => self.docs[self.current] = d,
                        Err(e) => self.notify(format!("Could not reload: {e}")),
                    }
                }
            }
            CLOSE_SCENE => self.close_tab(self.current, false),
            CLOSE_ALL => {
                if self.is_dirty() {
                    self.notify("Save or close the modified scenes first.");
                } else {
                    self.docs.clear();
                    self.docs.push(Doc::empty());
                    self.current = 0;
                }
            }
            QUIT => self.request_quit(),
            PROJECT_SETTINGS => self.open_project_settings(),
            OPEN_USER_DATA | OPEN_DATA_FOLDER => {
                let dir = self.path.join(".neko");
                let _ = std::fs::create_dir_all(&dir);
                util::open_folder(&dir);
            }
            RELOAD_PROJECT => {
                if self.is_dirty() {
                    self.notify("Save or discard your changes before reloading.");
                } else {
                    let path = self.path.clone();
                    *self = Editor::open(path);
                }
            }
            QUIT_TO_LIST => self.request_back(),
            VISIBLE_COLLISIONS => self.debug_collisions = !self.debug_collisions,
            VIEW_COLLISIONS => self.show_collisions = !self.show_collisions,
            VIEW_GRID => self.show_grid = !self.show_grid,
            VIEW_CENTER => {
                let doc = self.doc_mut();
                doc.zoom = 0.5;
                doc.pan = -super::GAME_SIZE * doc.zoom / 2.0;
                self.orbit = super::Orbit::default();
            }
            RESET_POSITION => self.reset_selected("position"),
            RESET_ROTATION => self.reset_selected("rotation_degrees"),
            RESET_SCALE => self.reset_selected("scale"),
            FULLSCREEN => self.toggle_fullscreen(),
            ONLINE_DOCS => match util::engine_checkout() {
                Some(root) => util::open_folder(&root.join("README.md")),
                None => self.notify("Documentation is not installed with this build."),
            },
            COPY_SYSTEM_INFO => {
                let backends = self.config.as_ref().map(|c| format!("{} / {} / {}", c.backends.window.label(), c.backends.render.label(), c.backends.audio.label())).unwrap_or_default();
                // The app puts this on the system clipboard.
                self.clipboard = Some(format!("Neko v{ENGINE_VERSION}.dev - {} {} - {backends}", std::env::consts::OS, std::env::consts::ARCH));
                self.notify("System info copied.");
            }
            ABOUT => self.open_about(),
            CLEAR_RECENT => {
                self.prefs.recent_scenes.clear();
                self.save_prefs();
            }
            a if (RECENT_BASE..CLEAR_RECENT).contains(&a) => {
                if let Some(r) = self.prefs.recent_scenes.get((a - RECENT_BASE) as usize).cloned() {
                    let p = self.path.join(r.trim_start_matches("res://"));
                    self.open_scene_file(p);
                }
            }
            NODE_ADD => self.open_create_dialog(),
            NODE_ATTACH_SCRIPT => self.open_attach_script(),
            NODE_DETACH_SCRIPT => self.detach_script(),
            NODE_RENAME => self.rename_selected(),
            NODE_MOVE_UP => self.move_selected(-1),
            NODE_MOVE_DOWN => self.move_selected(1),
            NODE_DUPLICATE => self.duplicate_selected(),
            NODE_DELETE => self.delete_selected(),
            a if (FILE_OPEN..=FILE_LAST).contains(&a) => self.fs_action(a),
            _ => {}
        }
    }

    fn reset_selected(&mut self, prop: &str) {
        let doc = self.doc_mut();
        let Some(id) = doc.selected else { return };
        if doc.scene.get(id).is_some_and(|n| n.is_overridden(prop)) {
            doc.checkpoint(None);
            if let Some(n) = doc.scene.get_mut(id) {
                n.reset(prop);
            }
        }
    }
}
