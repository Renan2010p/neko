//! Godot's FileDialog (scene/gui/file_dialog.cpp, used by EditorFileDialog),
//! on neko-ui: the same layout, buttons, menus and behavior.
//!
//! ```text
//! [<][>][^] Path: [/home/me/Projects..................] [⟳][★] | [+folder]
//! Favorites: [↑][↓] | Directories & Files:   [👁] | [▦][≡] | [filter][sort]
//! (list)            | (folders first, then files)
//! Recent:           | File: [name.tscn.................] [All Recognized ▾]
//!                         [Cancel]  [Open]
//! ```
//!
//! The owner forwards every UI event to [`FileDialog::handle`] while the
//! dialog is open and acts on [`FdResult::Confirmed`] / [`FdResult::Cancelled`].

use std::path::{Path, PathBuf};

use neko::platform::Key;
use neko_ui::{Color32, Control, Id, MenuItem, Sides, StyleBox, TreeRow, Ui, UiEvent};

use crate::node_icons::Icons;
use crate::ui_kit;

/// Godot's `FileDialog::FileMode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileMode {
    OpenFile,
    /// Godot's multi-select mode; the list selects one file for now.
    #[allow(dead_code)]
    OpenFiles,
    OpenDir,
    OpenAny,
    SaveFile,
}

impl FileMode {
    /// Godot's default titles (`FileDialog::set_file_mode`).
    fn title(self) -> &'static str {
        match self {
            FileMode::OpenFile => "Open a File",
            FileMode::OpenFiles => "Open File(s)",
            FileMode::OpenDir => "Open a Directory",
            FileMode::OpenAny => "Open a File or Directory",
            FileMode::SaveFile => "Save a File",
        }
    }

    fn ok_text(self) -> &'static str {
        match self {
            FileMode::OpenDir => "Select Current Folder",
            FileMode::SaveFile => "Save",
            _ => "Open",
        }
    }
}

/// Godot's `FileSortOption`, in the sort menu's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sort {
    Name,
    NameReverse,
    Type,
    TypeReverse,
    Modified,
    ModifiedReverse,
}

const SORTS: [(Sort, &str); 6] = [
    (Sort::Name, "Sort by Name (Ascending)"),
    (Sort::NameReverse, "Sort by Name (Descending)"),
    (Sort::Type, "Sort by Type (Ascending)"),
    (Sort::TypeReverse, "Sort by Type (Descending)"),
    (Sort::Modified, "Sort by Modified Time (Newest First)"),
    (Sort::ModifiedReverse, "Sort by Modified Time (Oldest First)"),
];

// Popup menu actions (kept apart from the owner's menu ids).
const MENU_SORT: u32 = 9100;
const MENU_COPY_PATH: u32 = 9110;
const MENU_DELETE: u32 = 9111;
const MENU_NEW_FOLDER: u32 = 9112;
const MENU_REFRESH: u32 = 9113;
const MENU_SHOW_IN_FILE_MANAGER: u32 = 9114;

/// What happened with an event.
#[derive(Debug, PartialEq)]
pub enum FdResult {
    /// Not the dialog's event.
    Ignored,
    /// The dialog used it and stays open.
    Handled,
    Cancelled,
    /// The chosen paths (one, except for `OpenFiles`). The dialog is closed.
    Confirmed(Vec<PathBuf>),
}

struct Entry {
    path: PathBuf,
    is_dir: bool,
}

struct W {
    modal: Id,
    close: Id,
    back: Id,
    forward: Id,
    up: Id,
    path: Id,
    refresh: Id,
    favorite: Id,
    make_dir: Id,
    fav_list: Id,
    fav_up: Id,
    fav_down: Id,
    recent_list: Id,
    hidden: Id,
    list_mode: Id,
    show_filter: Id,
    sort: Id,
    files: Id,
    message: Id,
    filter_box: Id,
    filter_edit: Id,
    file_edit: Id,
    filter_kind: Id,
    status: Id,
    ok: Id,
    cancel: Id,
}

/// A dialog on top of the file dialog (New Folder, overwrite, delete).
enum Sub {
    NewFolder { modal: Id, name: Id, ok: Id, cancel: Id, close: Id },
    Overwrite { modal: Id, path: PathBuf, ok: Id, cancel: Id, close: Id },
    Delete { modal: Id, path: PathBuf, ok: Id, cancel: Id, close: Id },
}

/// Favorites and recent folders, kept between sessions (Godot keeps them
/// in the editor settings as `favorite_dirs` / `recent_dirs`).
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Places {
    #[serde(default)]
    favorites: Vec<PathBuf>,
    #[serde(default)]
    recent: Vec<PathBuf>,
}

impl Places {
    fn file() -> Option<PathBuf> {
        dirs::data_dir().map(|d| d.join("neko").join("file_dialog.toml"))
    }

    fn load() -> Self {
        Self::file().and_then(|f| std::fs::read_to_string(f).ok()).and_then(|t| toml::from_str(&t).ok()).unwrap_or_default()
    }

    fn save(&self) {
        if let Some(f) = Self::file() {
            if let Some(d) = f.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            if let Ok(t) = toml::to_string(self) {
                let _ = std::fs::write(f, t);
            }
        }
    }
}

/// Checks a candidate path; returns a message and whether OK is allowed.
pub type Validator = Box<dyn Fn(&Path) -> (String, bool)>;

pub struct FileDialog {
    pub mode: FileMode,
    w: W,
    icons: Icons,
    /// `res://` access: the project folder; navigation stays inside it.
    root: Option<PathBuf>,
    dir: PathBuf,
    history: Vec<PathBuf>,
    history_pos: usize,
    /// `(patterns, description)`, e.g. `("*.tscn, *.scn", "Scenes")`.
    filters: Vec<(String, String)>,
    show_hidden: bool,
    sort: Sort,
    entries: Vec<Entry>,
    places: Places,
    sub: Option<Sub>,
    context: Option<PathBuf>,
    validator: Option<Validator>,
    /// Text the owner should copy to the clipboard ("Copy Path").
    pub clipboard: Option<String>,
}

impl FileDialog {
    /// Open the dialog at `start`. `root` limits it to a project (`res://`).
    pub fn open(ui: &mut Ui, icons: &Icons, mode: FileMode, title: Option<&str>, start: &Path, root: Option<&Path>, filters: &[(&str, &str)]) -> Self {
        let t = ui.theme.clone();
        let (modal, body, close) = ui_kit::dialog(ui, title.unwrap_or(mode.title()), 900.0);
        let b = |name: &str, tip: &str| ui_kit::icon_button(icons, name, tip, None);

        // Top toolbar.
        let top = ui.add(body, Control::hbox().separation(2.0));
        let back = ui.add(top, b("Back", "Go to previous folder."));
        let forward = ui.add(top, b("Forward", "Go to next folder."));
        let up = ui.add(top, b("ArrowUp", "Go to parent folder."));
        ui.add(top, Control::label("Path:").v_center());
        let path = ui.add(top, Control::line_edit("").expand_h());
        let refresh = ui.add(top, b("Reload", "Refresh files."));
        let favorite = ui.add(top, ui_kit::icon_button(icons, "Favorites", "(Un)favorite current folder.", Some(false)));
        ui.add(top, Control::v_separator());
        let make_dir = ui.add(top, b("FolderCreate", "Create a new folder."));

        // Favorites/Recent | Directories & Files.
        let split = ui.add(body, Control::h_split(190.0).min(0.0, 380.0));
        let left = ui.add(split, Control::vbox().separation(4.0));
        let fav_head = ui.add(left, Control::hbox().separation(2.0));
        ui.add(fav_head, Control::label("Favorites:").v_center().expand_h());
        let fav_up = ui.add(fav_head, b("MoveUp", "Move the selected favorite up."));
        let fav_down = ui.add(fav_head, b("MoveDown", "Move the selected favorite down."));
        let fav_bg = ui.add(left, Control::panel_container().style(t.tree_panel).expand());
        let fav_list = ui.add(fav_bg, Control::tree());
        ui.add(left, Control::label("Recent:"));
        let recent_bg = ui.add(left, Control::panel_container().style(t.tree_panel).expand());
        let recent_list = ui.add(recent_bg, Control::tree());

        let right = ui.add(split, Control::vbox().separation(4.0));
        let lower = ui.add(right, Control::hbox().separation(2.0));
        ui.add(lower, Control::label("Directories & Files:").v_center().expand_h());
        let hidden = ui.add(lower, ui_kit::icon_button(icons, "GuiVisibilityHidden", "Toggle the visibility of hidden files.", Some(false)));
        ui.add(lower, Control::v_separator());
        ui.add(lower, ui_kit::icon_button(icons, "FileThumbnail", "View items as a grid of thumbnails. Not available yet.", Some(false)).disabled(true));
        let list_mode = ui.add(lower, ui_kit::icon_button(icons, "FileList", "View items as a list.", Some(true)));
        ui.add(lower, Control::v_separator());
        let show_filter = ui.add(lower, ui_kit::icon_button(icons, "FilenameFilter", "Toggle the visibility of the filter for file names.", Some(false)));
        let sort = ui.add(lower, b("Sort", "Sort files"));
        let files_bg = ui.add(right, Control::panel_container().style(t.tree_panel).expand());
        let files = ui.add(files_bg, Control::tree());
        let message = ui.add(right, Control::label("").color(t.font_muted_color).hidden());
        let filter_box = ui.add(right, Control::hbox().separation(4.0).hidden());
        ui.add(filter_box, Control::label("Filter:").v_center());
        let filter_edit = ui.add(filter_box, Control::line_edit("").placeholder("Filter").expand_h());
        let file_box = ui.add(right, Control::hbox().separation(4.0));
        ui.add(file_box, Control::label("File:").v_center());
        let file_edit = ui.add(file_box, Control::line_edit("").expand_h());
        // Godot's filter list: "All Recognized" (if several), each filter, "All Files".
        let mut kinds: Vec<String> = Vec::new();
        if filters.len() > 1 {
            let all: Vec<&str> = filters.iter().map(|(p, _)| *p).collect();
            kinds.push(format!("All Recognized ({})", all.join(", ")));
        }
        for (p, d) in filters {
            kinds.push(format!("{d} ({p})"));
        }
        kinds.push("All Files (*.*)".into());
        let kind_refs: Vec<&str> = kinds.iter().map(String::as_str).collect();
        let filter_kind = ui.add(file_box, Control::option_button(&kind_refs, 0).min(220.0, 0.0));
        let status = ui.add(body, Control::panel_container().style(StyleBox::flat(Color32::TRANSPARENT, 0, Sides::symmetric(2.0, 0.0))));
        let status_label = ui.add(status, Control::label("").font(12.5));
        let (ok, cancel) = ui_kit::ok_cancel(ui, body, mode.ok_text(), "Cancel");

        // Open Directory mode shows no file name field (Godot hides file_box).
        if mode == FileMode::OpenDir {
            ui.set_visible(file_box, false);
        }
        let start = std::fs::canonicalize(start).unwrap_or_else(|_| start.to_path_buf());
        let mut fd = FileDialog {
            mode,
            w: W {
                modal,
                close,
                back,
                forward,
                up,
                path,
                refresh,
                favorite,
                make_dir,
                fav_list,
                fav_up,
                fav_down,
                recent_list,
                hidden,
                list_mode,
                show_filter,
                sort,
                files,
                message,
                filter_box,
                filter_edit,
                file_edit,
                filter_kind,
                status: status_label,
                ok,
                cancel,
            },
            icons: *icons,
            root: root.map(|r| std::fs::canonicalize(r).unwrap_or_else(|_| r.to_path_buf())),
            dir: start.clone(),
            history: vec![start],
            history_pos: 0,
            filters: filters.iter().map(|(p, d)| (p.to_string(), d.to_string())).collect(),
            show_hidden: false,
            sort: Sort::Name,
            entries: Vec::new(),
            places: Places::load(),
            sub: None,
            context: None,
            validator: None,
            clipboard: None,
        };
        fd.refresh(ui);
        fd.refresh_places(ui);
        if mode == FileMode::SaveFile {
            ui.focus(file_edit);
        }
        fd
    }

    /// Check candidates as the user browses (shown under the lists).
    pub fn set_validator(&mut self, ui: &mut Ui, v: Validator) {
        self.validator = Some(v);
        self.update_status(ui);
    }

    /// Prefill the file name (Save dialogs).
    pub fn set_file_name(&mut self, ui: &mut Ui, name: &str) {
        ui.set_text(self.w.file_edit, name);
    }

    pub fn close(self, ui: &mut Ui) {
        if let Some(s) = self.sub {
            ui.remove(sub_modal(&s));
        }
        ui.remove(self.w.modal);
    }

    // ------------------------------------------------------------ listing

    fn display_dir(&self) -> String {
        match &self.root {
            Some(r) => match self.dir.strip_prefix(r) {
                Ok(rel) if rel.as_os_str().is_empty() => "res://".into(),
                Ok(rel) => format!("res://{}", rel.display()),
                Err(_) => self.dir.display().to_string(),
            },
            None => self.dir.display().to_string(),
        }
    }

    /// Patterns of the selected filter; empty means all files.
    fn patterns(&self, ui: &Ui) -> Vec<String> {
        let sel = ui.selected(self.w.filter_kind);
        let n = self.filters.len();
        let all_index = if n > 1 { n + 1 } else { n };
        let split = |p: &str| p.split(',').map(|x| x.trim().to_lowercase()).filter(|x| !x.is_empty()).collect::<Vec<_>>();
        if sel >= all_index {
            return Vec::new();
        }
        if n > 1 && sel == 0 {
            return self.filters.iter().flat_map(|(p, _)| split(p)).collect();
        }
        let idx = if n > 1 { sel - 1 } else { sel };
        self.filters.get(idx).map(|(p, _)| split(p)).unwrap_or_default()
    }

    fn refresh(&mut self, ui: &mut Ui) {
        let patterns = self.patterns(ui);
        let name_filter = ui.text(self.w.filter_edit).trim().to_lowercase();
        let mut dirs = Vec::new();
        let mut files = Vec::new();
        let readable = match std::fs::read_dir(&self.dir) {
            Ok(rd) => {
                for e in rd.flatten() {
                    let name = e.file_name().to_string_lossy().into_owned();
                    if !self.show_hidden && name.starts_with('.') {
                        continue;
                    }
                    if !name_filter.is_empty() && !name.to_lowercase().contains(&name_filter) {
                        continue;
                    }
                    let is_dir = e.path().is_dir();
                    if is_dir {
                        dirs.push(e.path());
                    } else if patterns.is_empty() || patterns.iter().any(|p| glob(p, &name.to_lowercase())) {
                        files.push(e.path());
                    }
                }
                true
            }
            Err(_) => false,
        };
        let name = |p: &PathBuf| p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        let ext = |p: &PathBuf| p.extension().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        let mtime = |p: &PathBuf| p.metadata().and_then(|m| m.modified()).ok();
        let by = |v: &mut Vec<PathBuf>, s: Sort| match s {
            Sort::Name | Sort::NameReverse => v.sort_by(|a, b| natural_cmp(&name(a), &name(b))),
            Sort::Type | Sort::TypeReverse => v.sort_by(|a, b| ext(a).cmp(&ext(b)).then_with(|| natural_cmp(&name(a), &name(b)))),
            Sort::Modified | Sort::ModifiedReverse => v.sort_by(|a, b| mtime(b).cmp(&mtime(a))),
        };
        // Directories sort by name or time; files by the chosen option.
        let dir_sort = match self.sort {
            Sort::Type => Sort::Name,
            Sort::TypeReverse => Sort::NameReverse,
            s => s,
        };
        by(&mut dirs, dir_sort);
        by(&mut files, self.sort);
        if matches!(self.sort, Sort::NameReverse | Sort::TypeReverse | Sort::ModifiedReverse) {
            dirs.reverse();
            files.reverse();
        }
        let folder = self.icons.get("Folder").map(|i| i.tinted(Color32::from_rgb(0x87, 0xb0, 0xf0)));
        let file_icon = self.icons.tinted("File", Color32::from_gray(214));
        let scene_icon = self.icons.get("PackedScene");
        let script_icon = self.icons.get("Script");
        let muted = ui.theme.font_disabled_color;
        let mut rows = Vec::new();
        self.entries.clear();
        for d in dirs {
            let key = self.entries.len() as u64 + 1;
            let mut r = TreeRow::new(key, 0, d.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
            r.icon = folder;
            if neko::project::is_project_dir(&d) {
                r.suffix = "project".into();
            }
            rows.push(r);
            self.entries.push(Entry { path: d, is_dir: true });
        }
        for f in files {
            let key = self.entries.len() as u64 + 1;
            let n = f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let mut r = TreeRow::new(key, 0, n);
            r.icon = match ext(&f).as_str() {
                "tscn" | "scn" => scene_icon,
                "gd" | "rs" => script_icon,
                _ => file_icon,
            };
            // Folder pickers list files greyed out.
            if self.mode == FileMode::OpenDir {
                r.color = Some(muted);
            }
            rows.push(r);
            self.entries.push(Entry { path: f, is_dir: false });
        }
        let empty = rows.is_empty();
        ui.set_rows(self.w.files, rows);
        ui.set_tree_selected(self.w.files, None);
        ui.set_text(self.w.path, self.display_dir());
        // Godot's message label over the list.
        let msg = if !readable {
            Some("You don't have permission to access contents of this folder.")
        } else if empty && !name_filter.is_empty() {
            Some("No files match the filter.")
        } else {
            None
        };
        ui.set_text(self.w.message, msg.unwrap_or(""));
        ui.set_visible(self.w.message, msg.is_some());
        ui.set_disabled(self.w.back, self.history_pos == 0);
        ui.set_disabled(self.w.forward, self.history_pos + 1 >= self.history.len());
        ui.set_disabled(self.w.up, self.parent_of(&self.dir).is_none());
        ui.set_checked(self.w.favorite, self.places.favorites.contains(&self.dir));
        ui.set_checked(self.w.hidden, self.show_hidden);
        self.update_status(ui);
    }

    fn refresh_places(&mut self, ui: &mut Ui) {
        let folder = self.icons.get("Folder").map(|i| i.tinted(Color32::from_rgb(0x87, 0xb0, 0xf0)));
        let rows = |list: &[PathBuf]| -> Vec<TreeRow> {
            list.iter()
                .enumerate()
                .map(|(i, p)| {
                    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string());
                    let mut r = TreeRow::new(i as u64 + 1, 0, name);
                    r.icon = folder;
                    r
                })
                .collect()
        };
        let favs: Vec<PathBuf> = self.places.favorites.iter().filter(|p| self.allowed(p)).cloned().collect();
        let recent: Vec<PathBuf> = self.places.recent.iter().filter(|p| self.allowed(p)).cloned().collect();
        ui.set_rows(self.w.fav_list, rows(&favs));
        ui.set_rows(self.w.recent_list, rows(&recent));
    }

    /// Inside the project when limited to `res://`.
    fn allowed(&self, p: &Path) -> bool {
        self.root.as_ref().is_none_or(|r| p.starts_with(r))
    }

    fn parent_of(&self, p: &Path) -> Option<PathBuf> {
        let parent = p.parent()?.to_path_buf();
        self.allowed(&parent).then_some(parent)
    }

    fn selected_entry(&self, ui: &Ui) -> Option<&Entry> {
        let k = ui.tree_selected(self.w.files)?;
        self.entries.get(k as usize - 1)
    }

    /// What OK would choose right now.
    fn candidate(&self, ui: &Ui) -> Option<PathBuf> {
        let typed = ui.text(self.w.file_edit).trim().to_string();
        match self.mode {
            FileMode::OpenDir => Some(self.selected_entry(ui).filter(|e| e.is_dir).map(|e| e.path.clone()).unwrap_or_else(|| self.dir.clone())),
            FileMode::OpenAny => Some(self.selected_entry(ui).map(|e| e.path.clone()).unwrap_or_else(|| if typed.is_empty() { self.dir.clone() } else { self.dir.join(&typed) })),
            _ if typed.is_empty() => None,
            _ => Some(self.dir.join(&typed)),
        }
    }

    fn update_status(&mut self, ui: &mut Ui) {
        let cand = self.candidate(ui);
        let (text, ok, color) = match (&self.validator, &cand) {
            (Some(v), Some(p)) => {
                let (m, ok) = v(p);
                (m, ok, if ok { crate::theme::OK } else { crate::theme::BAD })
            }
            (Some(_), None) => (String::new(), false, ui.theme.font_muted_color),
            (None, c) => (String::new(), c.is_some() || self.mode == FileMode::OpenDir, ui.theme.font_muted_color),
        };
        ui.set_text(self.w.status, text);
        ui.set_font_color(self.w.status, Some(color));
        ui.set_disabled(self.w.ok, !ok);
        // Godot: "Select This Folder" when a folder is selected in a folder picker.
        let label = match (self.mode, self.selected_entry(ui)) {
            (FileMode::OpenDir, Some(e)) if e.is_dir => "Select This Folder",
            (FileMode::OpenFile | FileMode::OpenFiles | FileMode::OpenAny, Some(e)) if e.is_dir && self.mode != FileMode::OpenAny => "Open",
            (m, _) => m.ok_text(),
        };
        ui.set_text(self.w.ok, label);
    }

    fn go(&mut self, ui: &mut Ui, to: PathBuf, record: bool) {
        let to = std::fs::canonicalize(&to).unwrap_or(to);
        if !to.is_dir() || !self.allowed(&to) {
            ui.set_text(self.w.status, format!("Can't open {}.", to.display()));
            ui.set_font_color(self.w.status, Some(crate::theme::BAD));
            return;
        }
        if record && to != self.dir {
            self.history.truncate(self.history_pos + 1);
            self.history.push(to.clone());
            self.history_pos = self.history.len() - 1;
        }
        self.dir = to;
        if self.mode != FileMode::SaveFile {
            ui.set_text(self.w.file_edit, "");
        }
        self.refresh(ui);
    }

    /// Path typed in the Path field: absolute, `~/...`, `res://...` or relative.
    fn typed_path(&self, text: &str) -> PathBuf {
        let t = text.trim();
        if let Some(rest) = t.strip_prefix("res://")
            && let Some(r) = &self.root
        {
            return r.join(rest);
        }
        let t = crate::util::expand_home(t);
        let p = PathBuf::from(&t);
        if p.is_absolute() { p } else { self.dir.join(p) }
    }

    // ------------------------------------------------------------ events

    pub fn handle(&mut self, ui: &mut Ui, ev: &UiEvent) -> FdResult {
        if let Some(r) = self.handle_sub(ui, ev) {
            return r;
        }
        let w = &self.w;
        match ev {
            UiEvent::Key { key: Key::Escape, .. } => return FdResult::Cancelled,
            UiEvent::Pressed(id) if *id == w.close || *id == w.cancel => return FdResult::Cancelled,
            UiEvent::Pressed(id) if *id == w.ok => return self.confirm(ui),
            UiEvent::Pressed(id) if *id == w.back => {
                if self.history_pos > 0 {
                    self.history_pos -= 1;
                    let d = self.history[self.history_pos].clone();
                    self.go(ui, d, false);
                }
            }
            UiEvent::Pressed(id) if *id == w.forward => {
                if self.history_pos + 1 < self.history.len() {
                    self.history_pos += 1;
                    let d = self.history[self.history_pos].clone();
                    self.go(ui, d, false);
                }
            }
            UiEvent::Pressed(id) if *id == w.up => {
                if let Some(p) = self.parent_of(&self.dir) {
                    self.go(ui, p, true);
                }
            }
            UiEvent::Pressed(id) if *id == w.refresh => self.refresh(ui),
            // Toggle buttons emit Toggled (with the new state) after Pressed.
            UiEvent::Toggled(id, on) if *id == w.favorite => {
                let d = self.dir.clone();
                self.places.favorites.retain(|p| *p != d);
                if *on {
                    self.places.favorites.push(d);
                }
                self.places.save();
                self.refresh_places(ui);
                ui.set_checked(self.w.favorite, self.places.favorites.contains(&self.dir));
            }
            UiEvent::Pressed(id) if *id == w.fav_up || *id == w.fav_down => {
                let up = *id == w.fav_up;
                if let Some(k) = ui.tree_selected(w.fav_list) {
                    let i = k as usize - 1;
                    let j = if up { i.checked_sub(1) } else { (i + 1 < self.places.favorites.len()).then_some(i + 1) };
                    if let Some(j) = j {
                        self.places.favorites.swap(i, j);
                        self.places.save();
                        self.refresh_places(ui);
                        ui.set_tree_selected(self.w.fav_list, Some(j as u64 + 1));
                    }
                }
            }
            UiEvent::TreeSelected(t, k) if *t == w.fav_list || *t == w.recent_list => {
                let list = if *t == w.fav_list { &self.places.favorites } else { &self.places.recent };
                let visible: Vec<PathBuf> = list.iter().filter(|p| self.allowed(p)).cloned().collect();
                if let Some(p) = visible.get(*k as usize - 1).cloned() {
                    self.go(ui, p, true);
                }
            }
            UiEvent::Toggled(id, on) if *id == w.hidden => {
                self.show_hidden = *on;
                self.refresh(ui);
            }
            UiEvent::Toggled(id, _) if *id == w.list_mode => ui.set_checked(w.list_mode, true),
            UiEvent::Toggled(id, on) if *id == w.show_filter => {
                let show = *on;
                ui.set_visible(w.filter_box, show);
                ui.set_checked(w.show_filter, show);
                if show {
                    ui.focus(w.filter_edit);
                } else {
                    ui.set_text(w.filter_edit, "");
                    self.refresh(ui);
                }
            }
            UiEvent::TextChanged(id) if *id == w.filter_edit => self.refresh(ui),
            UiEvent::Pressed(id) if *id == w.sort => {
                let items = SORTS.iter().enumerate().map(|(i, (s, label))| MenuItem::item(*label, MENU_SORT + i as u32).check(*s == self.sort)).collect();
                let at = ui.rect(w.sort).left_bottom();
                ui.open_menu(items, at);
            }
            UiEvent::Menu(a) if (MENU_SORT..MENU_SORT + 6).contains(a) => {
                self.sort = SORTS[(*a - MENU_SORT) as usize].0;
                self.refresh(ui);
            }
            UiEvent::Pressed(id) if *id == w.make_dir => self.open_new_folder(ui),
            UiEvent::ItemSelected(id, _) if *id == w.filter_kind => self.refresh(ui),
            UiEvent::TextSubmitted(id) if *id == w.path => {
                let text = ui.text(*id).to_string();
                let p = self.typed_path(&text);
                if p.is_file() && self.mode != FileMode::OpenDir {
                    // Typing a file path opens it (Godot does the same).
                    if let Some(parent) = p.parent() {
                        self.go(ui, parent.to_path_buf(), true);
                    }
                    ui.set_text(self.w.file_edit, p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
                } else {
                    self.go(ui, p, true);
                }
            }
            UiEvent::TextChanged(id) if *id == w.file_edit => self.update_status(ui),
            UiEvent::TextSubmitted(id) if *id == w.file_edit => return self.confirm(ui),
            UiEvent::TreeSelected(t, _) if *t == w.files => {
                if let Some(e) = self.selected_entry(ui)
                    && !e.is_dir
                    && self.mode != FileMode::OpenDir
                {
                    let n = e.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    ui.set_text(self.w.file_edit, n);
                }
                self.update_status(ui);
            }
            UiEvent::TreeActivated(t, k) if *t == w.files => {
                let Some(e) = self.entries.get(*k as usize - 1) else { return FdResult::Handled };
                if e.is_dir {
                    let p = e.path.clone();
                    self.go(ui, p, true);
                } else if self.mode != FileMode::OpenDir {
                    return self.confirm(ui);
                }
            }
            UiEvent::TreeContext(t, k, pos) if *t == w.files => {
                self.context = k.and_then(|k| self.entries.get(k as usize - 1)).map(|e| e.path.clone());
                let mut items = Vec::new();
                if self.context.is_some() {
                    items.push(MenuItem::item("Copy Path", MENU_COPY_PATH).icon_opt(self.icons.get("ActionCopy")));
                    items.push(MenuItem::item("Delete", MENU_DELETE).icon_opt(self.icons.get("Remove")));
                } else {
                    items.push(MenuItem::item("New Folder...", MENU_NEW_FOLDER).icon_opt(self.icons.get("Folder")));
                    items.push(MenuItem::item("Refresh", MENU_REFRESH).icon_opt(self.icons.get("Reload")));
                }
                items.push(MenuItem::separator());
                items.push(MenuItem::item("Show in File Manager", MENU_SHOW_IN_FILE_MANAGER).icon_opt(self.icons.get("Filesystem")));
                ui.open_menu(items, *pos);
            }
            UiEvent::Menu(MENU_COPY_PATH) => self.clipboard = self.context.as_ref().map(|p| p.display().to_string()),
            UiEvent::Menu(MENU_DELETE) => {
                if let Some(p) = self.context.clone() {
                    self.open_delete(ui, p);
                }
            }
            UiEvent::Menu(MENU_NEW_FOLDER) => self.open_new_folder(ui),
            UiEvent::Menu(MENU_REFRESH) => self.refresh(ui),
            UiEvent::Menu(MENU_SHOW_IN_FILE_MANAGER) => {
                let p = self.context.clone().unwrap_or_else(|| self.dir.clone());
                crate::util::open_folder(if p.is_dir() { &p } else { p.parent().unwrap_or(&p) });
            }
            UiEvent::Pressed(id) if [w.favorite, w.hidden, w.list_mode, w.show_filter].contains(id) => {}
            _ => return FdResult::Ignored,
        }
        FdResult::Handled
    }

    fn confirm(&mut self, ui: &mut Ui) -> FdResult {
        let Some(mut path) = self.candidate(ui) else { return FdResult::Handled };
        match self.mode {
            FileMode::OpenFile | FileMode::OpenFiles => {
                if path.is_dir() {
                    self.go(ui, path, true);
                    return FdResult::Handled;
                }
                if !path.is_file() {
                    ui.set_text(self.w.status, "File not found.");
                    ui.set_font_color(self.w.status, Some(crate::theme::BAD));
                    return FdResult::Handled;
                }
            }
            FileMode::SaveFile => {
                // Add the selected filter's extension when the name has none (Godot does).
                if path.extension().is_none()
                    && let Some(ext) = self.patterns(ui).first().and_then(|p| p.strip_prefix("*.")).map(str::to_string)
                {
                    path.set_extension(ext);
                }
                if path.is_dir() {
                    self.go(ui, path, true);
                    return FdResult::Handled;
                }
                if path.exists() {
                    self.open_overwrite(ui, path);
                    return FdResult::Handled;
                }
            }
            FileMode::OpenDir | FileMode::OpenAny => {}
        }
        if let Some(v) = &self.validator
            && !v(&path).1
        {
            return FdResult::Handled;
        }
        self.finish(path)
    }

    fn finish(&mut self, path: PathBuf) -> FdResult {
        // Remember the folder in Recent (most recent first, like Godot).
        let folder = if path.is_dir() { path.clone() } else { path.parent().map(Path::to_path_buf).unwrap_or_default() };
        self.places.recent.retain(|p| *p != folder);
        self.places.recent.insert(0, folder);
        self.places.recent.truncate(20);
        self.places.save();
        FdResult::Confirmed(vec![path])
    }

    // ------------------------------------------------------------ sub-dialogs

    fn open_new_folder(&mut self, ui: &mut Ui) {
        let (modal, body, close) = ui_kit::dialog(ui, "Create Folder", 320.0);
        ui.add(body, Control::label("Name:"));
        let name = ui.add(body, Control::line_edit("New Folder"));
        let (ok, cancel) = ui_kit::ok_cancel(ui, body, "OK", "Cancel");
        ui.focus(name);
        self.sub = Some(Sub::NewFolder { modal, name, ok, cancel, close });
    }

    fn open_overwrite(&mut self, ui: &mut Ui, path: PathBuf) {
        let (modal, body, close) = ui_kit::dialog(ui, "Please Confirm...", 380.0);
        let f = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        ui.add(body, Control::label(format!("File \"{f}\" already exists.\nDo you want to overwrite it?")).wrap());
        let (ok, cancel) = ui_kit::ok_cancel(ui, body, "OK", "Cancel");
        self.sub = Some(Sub::Overwrite { modal, path, ok, cancel, close });
    }

    fn open_delete(&mut self, ui: &mut Ui, path: PathBuf) {
        let (modal, body, close) = ui_kit::dialog(ui, "Please Confirm...", 420.0);
        ui.add(body, Control::label("Delete the selected file?\nDepending on your filesystem configuration, the files will either be moved to the system trash or deleted permanently.").wrap());
        let (ok, cancel) = ui_kit::ok_cancel(ui, body, "OK", "Cancel");
        self.sub = Some(Sub::Delete { modal, path, ok, cancel, close });
    }

    fn handle_sub(&mut self, ui: &mut Ui, ev: &UiEvent) -> Option<FdResult> {
        let sub = self.sub.as_ref()?;
        let (ok, cancel, close) = match sub {
            Sub::NewFolder { ok, cancel, close, .. } | Sub::Overwrite { ok, cancel, close, .. } | Sub::Delete { ok, cancel, close, .. } => (*ok, *cancel, *close),
        };
        let submit = matches!((sub, ev), (Sub::NewFolder { name, .. }, UiEvent::TextSubmitted(id)) if id == name);
        let dismiss = matches!(ev, UiEvent::Key { key: Key::Escape, .. }) || *ev == UiEvent::Pressed(cancel) || *ev == UiEvent::Pressed(close);
        if dismiss {
            let s = self.sub.take().expect("checked");
            ui.remove(sub_modal(&s));
            return Some(FdResult::Handled);
        }
        if *ev != UiEvent::Pressed(ok) && !submit {
            // Swallow everything while a sub-dialog is open.
            return Some(if matches!(ev, UiEvent::TextChanged(_)) { FdResult::Handled } else { FdResult::Handled });
        }
        let s = self.sub.take().expect("checked");
        ui.remove(sub_modal(&s));
        match s {
            Sub::NewFolder { name, .. } => {
                let n = ui.text(name).trim().to_string();
                let target = self.dir.join(&n);
                if n.is_empty() || n.contains('/') || std::fs::create_dir(&target).is_err() {
                    ui.set_text(self.w.status, "Could not create folder.");
                    ui.set_font_color(self.w.status, Some(crate::theme::BAD));
                } else {
                    self.refresh(ui);
                    if let Some(i) = self.entries.iter().position(|e| e.path == target) {
                        ui.set_tree_selected(self.w.files, Some(i as u64 + 1));
                        self.update_status(ui);
                    }
                }
                Some(FdResult::Handled)
            }
            Sub::Overwrite { path, .. } => Some(self.finish(path)),
            Sub::Delete { path, .. } => {
                let r = if path.is_dir() { std::fs::remove_dir_all(&path) } else { std::fs::remove_file(&path) };
                if r.is_err() {
                    ui.set_text(self.w.status, "Could not delete the file.");
                    ui.set_font_color(self.w.status, Some(crate::theme::BAD));
                }
                self.refresh(ui);
                Some(FdResult::Handled)
            }
        }
    }
}

fn sub_modal(s: &Sub) -> Id {
    match s {
        Sub::NewFolder { modal, .. } | Sub::Overwrite { modal, .. } | Sub::Delete { modal, .. } => *modal,
    }
}

trait IconOpt {
    fn icon_opt(self, i: Option<neko_ui::Image>) -> Self;
}

impl IconOpt for MenuItem {
    fn icon_opt(self, i: Option<neko_ui::Image>) -> Self {
        match i {
            Some(i) => self.icon(i),
            None => self,
        }
    }
}

/// `*.tscn`-style patterns (Godot's `String::matchn`).
fn glob(pattern: &str, name: &str) -> bool {
    fn rec(p: &[u8], n: &[u8]) -> bool {
        match (p.first(), n.first()) {
            (None, None) => true,
            (Some(b'*'), _) => rec(&p[1..], n) || (!n.is_empty() && rec(p, &n[1..])),
            (Some(b'?'), Some(_)) => rec(&p[1..], &n[1..]),
            (Some(a), Some(b)) if a == b => rec(&p[1..], &n[1..]),
            _ => false,
        }
    }
    rec(pattern.as_bytes(), name.as_bytes())
}

/// Godot's `naturalnocasecmp_to`: digits compare as numbers ("file2" < "file10").
fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, _) => return std::cmp::Ordering::Less,
            (_, None) => return std::cmp::Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let mut n1 = String::new();
                while let Some(c) = x.peek().copied().filter(char::is_ascii_digit) {
                    n1.push(c);
                    x.next();
                }
                let mut n2 = String::new();
                while let Some(d) = y.peek().copied().filter(char::is_ascii_digit) {
                    n2.push(d);
                    y.next();
                }
                let (v1, v2) = (n1.parse::<u128>().unwrap_or(0), n2.parse::<u128>().unwrap_or(0));
                if v1 != v2 {
                    return v1.cmp(&v2);
                }
            }
            (Some(c), Some(d)) => {
                if c != d {
                    return c.cmp(&d);
                }
                x.next();
                y.next();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globs_and_natural_order() {
        assert!(glob("*.tscn", "main.tscn"));
        assert!(!glob("*.tscn", "main.tscn.bak"));
        assert!(glob("icon.???", "icon.svg"));
        let mut v = vec!["file10", "file2", "File1", "a"];
        v.sort_by(|a, b| natural_cmp(&a.to_lowercase(), &b.to_lowercase()));
        assert_eq!(v, ["a", "File1", "file2", "file10"]);
    }
}
