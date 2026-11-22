//! The project manager built with Neko's own GUI (`neko-ui`), following the
//! structure of Godot's `editor/project_manager/` (top bar with tabs, toolbar,
//! project list items, action column, Create New Project dialog).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use neko::platform::{Key, Modifiers};
use neko::project::scaffold::{self, NewProject, default_projects_dir, slug};
use neko::project::{
    AudioBackend, BackendChoice, BackendSet, ProjectConfig, ProjectEntry, Registry, RenderBackend, Support, TargetPlatform, Template, WindowBackend,
};
use neko_ui::{Align, Anchors, Color32, Control, Id, Sides, StyleBox, Theme, Ui, UiEvent, vec2};

use crate::icons::{self, Icon};
use crate::util::{Action, RuntimeInfo, ago, copy_dir, display_path, engine_checkout, expand_home, open_folder, renderer_bullets};

const SORTS: [&str; 3] = ["Last Edited", "Name", "Path"];

/// Ids of the fixed parts of the screen.
struct Widgets {
    tab_projects: Id,
    tab_assets: Id,
    settings: Id,
    projects_view: Id,
    assets_view: Id,
    create: Id,
    import: Id,
    scan: Id,
    filter: Id,
    sort: Id,
    list: Id,
    list_scroll: Id,
    empty: Id,
    edit: Id,
    run: Id,
    rename: Id,
    duplicate: Id,
    tags: Id,
    open_folder: Id,
    remove: Id,
    remove_missing: Id,
    about: Id,
    status_count: Id,
    status_pads: Id,
    status_runtime: Id,
    toast: Id,
    toast_label: Id,
}

struct Row {
    panel: Id,
    star: Id,
    folder: Id,
    path: PathBuf,
}

enum InputKind {
    Rename(PathBuf),
    Tags(PathBuf),
}

/// What the folder picker does with the chosen folder.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FolderUse {
    ImportProject,
    ScanProjects,
    /// The Browse button of Create New Project.
    NewProjectPath,
}

enum Dialog {
    NewProject(NewDialog),
    Input { modal: Id, kind: InputKind, field: Id, error: Id, ok: Id, cancel: Id, close: Id },
    Confirm { modal: Id, path: PathBuf, ok: Id, cancel: Id, close: Id },
    Info { modal: Id, close: Id, ok: Id, single_window: Option<Id> },
}

struct NewDialog {
    modal: Id,
    close: Id,
    name: Id,
    path: Id,
    browse: Id,
    create_folder: Id,
    message: Id,
    renderers: Vec<(Id, RenderBackend)>,
    bullets: Id,
    windows: Vec<(Id, WindowBackend)>,
    vcs: Id,
    edit_now: Id,
    cancel: Id,
    create: Id,
    parent_dir: String,
    custom_path: String,
}

pub struct NativeProjectManager {
    pub ui: Ui,
    icons: crate::node_icons::Icons,
    registry: Registry,
    configs: HashMap<PathBuf, Result<ProjectConfig, String>>,
    selected: Option<PathBuf>,
    w: Widgets,
    rows: Vec<Row>,
    dialog: Option<Dialog>,
    /// Godot's FileDialog (on top of any dialog) and what it is for.
    files: Option<(crate::file_dialog::FileDialog, FolderUse)>,
    /// Text to put on the system clipboard.
    pub clipboard: Option<String>,
    toast_until: Option<Instant>,
    pub info: RuntimeInfo,
    shown_pads: String,
    pending: Option<Action>,
}

fn theme() -> Theme {
    Theme::neko_dark()
}

/// A button with one of the editor's icons.
fn icon_button(text: &str, icon: Icon) -> Control {
    Control::button(text).align(Align::Start).icon(move |p, r, hovered, _| {
        let t = theme();
        icons::paint_on(p, r, icon, if hovered { t.font_hover_color } else { t.font_color });
    })
}

impl NativeProjectManager {
    pub fn new(registry: Registry) -> Self {
        let mut ui = Ui::new(theme());
        ui.set_native_windows(crate::util::native_dialogs());
        let icons = crate::node_icons::Icons::load(&mut ui);
        let t = ui.theme.clone();
        let root = ui.root();
        let main = ui.add(root, Control::vbox().separation(0.0).anchors(Anchors::FULL_RECT));

        // Top bar: logo, centered tabs, settings (like Godot's title bar).
        let top = ui.add(main, Control::panel_container().style(StyleBox::flat(t.background, 0, Sides::symmetric(10.0, 6.0))));
        let top_row = ui.add(top, Control::hbox());
        let accent = t.accent;
        ui.add(top_row, Control::custom(vec2(22.0, 22.0), move |p, r, _, _| icons::cat_logo(p, r, accent, Color32::from_rgb(0x1a, 0x14, 0x05))));
        ui.add(top_row, Control::label("NEKO").font(17.0).color(t.font_focus_color).v_center());
        ui.add(top_row, Control::spacer());
        let tab_projects = ui.add(top_row, icon_button("Projects", Icon::List).flat().toggle(true));
        let tab_assets = ui.add(top_row, icon_button("Asset Library", Icon::Download).flat().toggle(false));
        ui.add(top_row, Control::spacer());
        let settings = ui.add(top_row, icon_button("Settings", Icon::Settings).flat());

        // Projects view.
        let projects_view = ui.add(main, Control::vbox().separation(0.0).expand());
        let toolbar = ui.add(projects_view, Control::panel_container().style(StyleBox::flat(t.panel.bg, 0, Sides::symmetric(8.0, 6.0))));
        let tb = ui.add(toolbar, Control::hbox());
        let create = ui.add(tb, icon_button("Create", Icon::Plus).tooltip("Create a new project (Ctrl+N)"));
        let import = ui.add(tb, icon_button("Import", Icon::Import).tooltip("Add an existing project (Ctrl+I)"));
        let scan = ui.add(tb, icon_button("Scan", Icon::Search).tooltip("Find projects inside a folder"));
        let filter = ui.add(tb, Control::line_edit("").placeholder("Filter Projects"));
        ui.add(tb, Control::label("Sort:").v_center());
        let sort = ui.add(tb, Control::option_button(&SORTS, 0).min(150.0, 0.0));

        let body = ui.add(projects_view, Control::hbox().separation(0.0).expand());
        let list_margin = ui.add(body, Control::panel_container().expand().style(StyleBox::flat(t.panel.bg, 0, Sides::new(8.0, 0.0, 4.0, 8.0))));
        let list_bg = ui.add(list_margin, Control::panel_container().style(t.tree_panel));
        let list_scroll = ui.add(list_bg, Control::scroll());
        let list = ui.add(list_scroll, Control::vbox().separation(2.0));
        let empty = ui.add(list_bg, Control::label("No projects yet. Create a new project or import one you already have.").align(Align::Center).color(t.font_muted_color).anchors(Anchors::CENTER).hidden());

        let actions_bg = ui.add(body, Control::panel_container().style(StyleBox::flat(t.panel.bg, 0, Sides::new(4.0, 0.0, 8.0, 8.0))).min(150.0, 0.0));
        let actions = ui.add(actions_bg, Control::vbox());
        let edit = ui.add(actions, Control::button("Edit").primary().tooltip("Open in the editor (Enter)"));
        let run = ui.add(actions, icon_button("Run", Icon::Play).tooltip("cargo run in the project folder"));
        let rename = ui.add(actions, icon_button("Rename", Icon::Rename));
        let duplicate = ui.add(actions, icon_button("Duplicate", Icon::Duplicate));
        let tags = ui.add(actions, icon_button("Manage Tags", Icon::Tag));
        let open_folder_b = ui.add(actions, icon_button("Open Folder", Icon::Folder));
        let remove = ui.add(actions, icon_button("Remove", Icon::Trash).tooltip("Remove from the list (files are kept)"));
        let remove_missing = ui.add(actions, icon_button("Remove Missing", Icon::Broom));
        ui.add(actions, Control::spacer());
        let about = ui.add(actions, icon_button("About Neko", Icon::Info));

        // Asset Library view.
        let assets_view = ui.add(main, Control::panel_container().expand().style(StyleBox::flat(t.panel.bg, 0, Sides::all(24.0))).hidden());
        let assets_box = ui.add(assets_view, Control::vbox());
        ui.add(assets_box, Control::label("Asset Library").font(20.0).color(t.font_focus_color));
        ui.add(assets_box, Control::label("Browsing community assets needs the asset server, which is not built yet.").color(t.font_muted_color));

        // Status bar.
        let status = ui.add(main, Control::panel_container().style(StyleBox::flat(t.background, 0, Sides::symmetric(12.0, 4.0))));
        let sb = ui.add(status, Control::hbox());
        let status_count = ui.add(sb, Control::label("").font(12.0).color(t.font_muted_color));
        ui.add(sb, Control::v_separator());
        let status_pads = ui.add(sb, Control::label("no gamepads").font(12.0).color(t.font_muted_color).tooltip("Gamepads are read through SDL3, like Godot"));
        ui.add(sb, Control::spacer());
        let status_runtime = ui.add(sb, Control::label("").font(12.0).color(t.font_muted_color));

        // Toast, above everything else.
        let toast = ui.add(root, Control::panel_container().style(t.tooltip.with_padding(Sides::symmetric(14.0, 8.0))).anchors(Anchors::bottom_center(48.0)).hidden());
        let toast_label = ui.add(toast, Control::label(""));

        let w = Widgets {
            tab_projects,
            tab_assets,
            settings,
            projects_view,
            assets_view,
            create,
            import,
            scan,
            filter,
            sort,
            list,
            list_scroll,
            empty,
            edit,
            run,
            rename,
            duplicate,
            tags,
            open_folder: open_folder_b,
            remove,
            remove_missing,
            about,
            status_count,
            status_pads,
            status_runtime,
            toast,
            toast_label,
        };
        let mut pm = Self {
            ui,
            icons,
            registry,
            configs: HashMap::new(),
            selected: None,
            w,
            rows: Vec::new(),
            dialog: None,
            files: None,
            clipboard: None,
            toast_until: None,
            info: RuntimeInfo::default(),
            shown_pads: String::new(),
            pending: None,
        };
        pm.load_configs();
        pm.selected = pm.sorted().first().map(|e| e.path.clone());
        pm.rebuild_list();
        pm
    }

    pub fn take_action(&mut self) -> Option<Action> {
        self.pending.take()
    }

    fn load_configs(&mut self) {
        for e in &self.registry.projects {
            self.configs.entry(e.path.clone()).or_insert_with(|| ProjectConfig::load(&e.path).map_err(|e| e.to_string()));
        }
    }

    fn save_registry(&mut self) {
        if let Err(e) = self.registry.save() {
            self.notify(format!("Could not save the project list: {e}"));
        }
    }

    fn notify(&mut self, msg: impl Into<String>) {
        self.ui.set_text(self.w.toast_label, msg.into());
        self.ui.set_visible(self.w.toast, true);
        self.toast_until = Some(Instant::now() + Duration::from_secs(4));
    }

    fn name_of(&self, e: &ProjectEntry) -> String {
        match self.configs.get(&e.path) {
            Some(Ok(c)) => c.project.name.clone(),
            _ => e.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        }
    }

    fn sorted(&self) -> Vec<ProjectEntry> {
        let term = self.ui.text(self.w.filter).to_lowercase();
        let mut v: Vec<ProjectEntry> = self
            .registry
            .projects
            .iter()
            .filter(|e| {
                let mut hay = format!("{} {}", self.name_of(e), e.path.display());
                if let Some(Ok(c)) = self.configs.get(&e.path) {
                    hay += &format!(" {} {}", c.backends.window.label(), c.backends.render.label());
                }
                hay += &e.tags.join(" ");
                term.is_empty() || hay.to_lowercase().contains(&term)
            })
            .cloned()
            .collect();
        let sort = self.ui.selected(self.w.sort);
        v.sort_by(|a, b| {
            b.favorite.cmp(&a.favorite).then_with(|| match sort {
                1 => self.name_of(a).to_lowercase().cmp(&self.name_of(b).to_lowercase()),
                2 => a.path.cmp(&b.path),
                _ => b.last_opened.cmp(&a.last_opened),
            })
        });
        v
    }

    // ------------------------------------------------------------ list

    fn rebuild_list(&mut self) {
        let t = self.ui.theme.clone();
        self.ui.clear_children(self.w.list);
        self.rows.clear();
        let entries = self.sorted();
        self.ui.set_visible(self.w.empty, entries.is_empty());
        let empty_text = if self.registry.projects.is_empty() {
            "No projects yet. Create a new project or import one you already have.".to_string()
        } else {
            format!("No project matches \"{}\".", self.ui.text(self.w.filter))
        };
        self.ui.set_text(self.w.empty, empty_text);
        for e in entries {
            let exists = e.exists();
            let name = self.name_of(&e);
            let cfg = self.configs.get(&e.path).and_then(|c| c.as_ref().ok()).cloned();
            let selected = self.selected.as_ref() == Some(&e.path);
            let row_style = if selected { t.item_selected.with_padding(Sides::symmetric(8.0, 6.0)) } else { StyleBox::EMPTY.with_padding(Sides::symmetric(8.0, 6.0)) };
            let panel = self.ui.add(self.w.list, Control::panel_container().style(row_style).clickable());
            let hb = self.ui.add(panel, Control::hbox().separation(8.0));

            let fav = e.favorite;
            let star = self.ui.add(
                hb,
                Control::custom(vec2(20.0, 20.0), move |p, r, hovered, _| {
                    let t = theme();
                    let color = if fav { t.accent } else if hovered { t.font_hover_color } else { t.font_muted_color };
                    icons::paint_on(p, r, if fav { Icon::StarFilled } else { Icon::Star }, color);
                })
                .clickable()
                .tooltip("Toggle Favorite"),
            );
            let badge_name = name.clone();
            self.ui.add(
                hb,
                Control::custom(vec2(44.0, 44.0), move |p, r, _, _| {
                    let fill = if exists { crate::theme::badge_color(&badge_name) } else { Color32::from_gray(60) };
                    p.rect_filled(r, 8, fill);
                    let initials: String = badge_name.split_whitespace().take(2).filter_map(|w| w.chars().next()).collect::<String>().to_uppercase();
                    p.text(r.center(), neko_ui::Align2::CENTER_CENTER, &initials, neko_ui::FontId::proportional(17.0), Color32::from_rgb(0x12, 0x13, 0x18));
                }),
            );

            let col = self.ui.add(hb, Control::vbox().separation(2.0).expand_h().v_center());
            let title = self.ui.add(col, Control::hbox().separation(6.0));
            self.ui.add(title, Control::label(&name).font(15.0).color(if exists { t.font_focus_color } else { t.font_disabled_color }));
            if let Some(tag) = cfg.as_ref().and_then(|c| c.project.template.tag()) {
                chip(&mut self.ui, title, &tag.to_uppercase(), crate::theme::tag_color(tag));
            }
            for tag in &e.tags {
                chip(&mut self.ui, title, tag, t.font_muted_color);
            }
            if !exists {
                chip(&mut self.ui, title, "missing", crate::theme::BAD);
            }
            let path_row = self.ui.add(col, Control::hbox().separation(4.0));
            let folder = self.ui.add(
                path_row,
                Control::custom(vec2(18.0, 18.0), move |p, r, hovered, _| {
                    let t = theme();
                    icons::paint_on(p, r.shrink(2.0), Icon::Folder, if hovered { t.font_hover_color } else { t.font_muted_color });
                })
                .clickable()
                .tooltip("Open in file manager"),
            );
            self.ui.add(path_row, Control::label(display_path(&e.path)).font(12.5).color(t.font_muted_color).ellipsis().tooltip(e.path.display().to_string()));
            if let Some(c) = &cfg {
                self.ui.add(path_row, Control::label(format!("{} · {}", c.backends.render.label(), c.backends.window.label())).font(12.0).color(t.font_muted_color));
                self.ui.add(path_row, Control::label(&c.project.engine).font(12.5).color(t.font_color).min(46.0, 0.0).align(Align::End));
            }
            self.ui.add(path_row, Control::label(ago(e.last_opened)).font(12.5).color(t.font_muted_color).min(80.0, 0.0).align(Align::End).tooltip("Last opened"));
            self.rows.push(Row { panel, star, folder, path: e.path.clone() });
        }
        self.update_actions();
    }

    fn select(&mut self, path: Option<PathBuf>) {
        self.selected = path;
        let t = self.ui.theme.clone();
        for r in &self.rows {
            let on = self.selected.as_ref() == Some(&r.path);
            let style = if on { t.item_selected } else { StyleBox::EMPTY };
            self.ui.set_style(r.panel, Some(style.with_padding(Sides::symmetric(8.0, 6.0))));
        }
        self.update_actions();
    }

    fn update_actions(&mut self) {
        let sel = self.selected.clone();
        let exists = sel.as_ref().is_some_and(|p| neko::project::is_project_dir(p));
        for id in [self.w.edit, self.w.run, self.w.rename, self.w.duplicate, self.w.open_folder] {
            self.ui.set_disabled(id, !exists);
        }
        for id in [self.w.tags, self.w.remove] {
            self.ui.set_disabled(id, sel.is_none());
        }
        let missing = self.registry.projects.iter().filter(|p| !p.exists()).count();
        self.ui.set_disabled(self.w.remove_missing, missing == 0);
        self.ui.set_text(self.w.status_count, format!("{} projects", self.registry.projects.len()));
    }

    // ---------------------------------------------------------- per frame

    /// Apply events and timers. Call before `ui.frame()`.
    pub fn update(&mut self) {
        if self.toast_until.is_some_and(|t| Instant::now() > t) {
            self.toast_until = None;
            self.ui.set_visible(self.w.toast, false);
        }
        let pads = if self.info.gamepads.is_empty() { "no gamepads".to_string() } else { format!("gamepads: {}", self.info.gamepads.values().cloned().collect::<Vec<_>>().join(", ")) };
        if pads != self.shown_pads {
            self.ui.set_text(self.w.status_pads, pads.clone());
            self.shown_pads = pads;
        }
        let runtime = format!("v{}.dev  [{} · {}]  ·  neko-ui", scaffold::ENGINE_VERSION, self.info.window, self.info.renderer);
        if self.ui.text(self.w.status_runtime) != runtime {
            self.ui.set_text(self.w.status_runtime, runtime);
        }
        for ev in self.ui.take_events() {
            self.handle(ev);
        }
    }

    /// When the screen wants to be redrawn without input.
    pub fn repaint_after(&self) -> Option<Duration> {
        self.toast_until.map(|t| t.saturating_duration_since(Instant::now()))
    }

    fn handle(&mut self, ev: UiEvent) {
        // The file dialog sits on top of everything (even Create New Project).
        if self.files.is_some() {
            self.handle_files(ev);
            return;
        }
        if self.dialog.is_some() {
            self.handle_dialog(ev);
            return;
        }
        let w = &self.w;
        match ev {
            UiEvent::Pressed(id) if id == w.tab_projects || id == w.tab_assets => {
                let projects = id == w.tab_projects;
                self.ui.set_checked(self.w.tab_projects, projects);
                self.ui.set_checked(self.w.tab_assets, !projects);
                self.ui.set_visible(self.w.projects_view, projects);
                self.ui.set_visible(self.w.assets_view, !projects);
            }
            UiEvent::Pressed(id) if id == w.settings => self.open_info(true),
            UiEvent::Pressed(id) if id == w.about => self.open_info(false),
            UiEvent::Pressed(id) if id == w.create => self.open_new_project(),
            UiEvent::Pressed(id) if id == w.import => self.open_folder_picker(FolderUse::ImportProject),
            UiEvent::Pressed(id) if id == w.scan => self.open_folder_picker(FolderUse::ScanProjects),
            UiEvent::TextChanged(id) if id == w.filter => {
                self.ui.scroll_to_top(self.w.list_scroll);
                self.rebuild_list();
            }
            UiEvent::ItemSelected(id, _) if id == w.sort => self.rebuild_list(),
            UiEvent::Pressed(id) if id == w.edit => self.edit_selected(),
            UiEvent::Pressed(id) if id == w.run => {
                if let Some(p) = self.selected.clone() {
                    match std::process::Command::new("cargo").arg("run").current_dir(&p).spawn() {
                        Ok(_) => self.notify(format!("Running cargo run in {}", display_path(&p))),
                        Err(e) => self.notify(format!("Could not start cargo: {e}")),
                    }
                }
            }
            UiEvent::Pressed(id) if id == w.rename => {
                if let Some(p) = self.selected.clone() {
                    let name = self.configs.get(&p).and_then(|c| c.as_ref().ok()).map(|c| c.project.name.clone()).unwrap_or_default();
                    self.open_input(InputKind::Rename(p), &name);
                }
            }
            UiEvent::Pressed(id) if id == w.duplicate => {
                if let Some(p) = self.selected.clone() {
                    self.duplicate(&p);
                }
            }
            UiEvent::Pressed(id) if id == w.tags => {
                if let Some(p) = self.selected.clone()
                    && let Some(i) = self.registry.find(&p)
                {
                    let tags = self.registry.projects[i].tags.join(", ");
                    self.open_input(InputKind::Tags(p), &tags);
                }
            }
            UiEvent::Pressed(id) if id == w.open_folder => {
                if let Some(p) = &self.selected {
                    open_folder(p);
                }
            }
            UiEvent::Pressed(id) if id == w.remove => {
                if let Some(p) = self.selected.clone() {
                    self.open_confirm(p);
                }
            }
            UiEvent::Pressed(id) if id == w.remove_missing => {
                let n = self.registry.remove_missing();
                self.save_registry();
                self.selected = self.sorted().first().map(|e| e.path.clone());
                self.rebuild_list();
                self.notify(format!("Removed {n} missing project(s)."));
            }
            UiEvent::Pressed(id) => {
                if let Some(r) = self.rows.iter().find(|r| r.star == id) {
                    let path = r.path.clone();
                    if let Some(i) = self.registry.find(&path) {
                        self.registry.projects[i].favorite ^= true;
                        self.save_registry();
                        self.rebuild_list();
                    }
                } else if let Some(r) = self.rows.iter().find(|r| r.folder == id) {
                    open_folder(&r.path.clone());
                } else if let Some(r) = self.rows.iter().find(|r| r.panel == id) {
                    let p = r.path.clone();
                    self.select(Some(p));
                }
            }
            UiEvent::DoubleClicked(id) => {
                if let Some(r) = self.rows.iter().find(|r| r.panel == id) {
                    let p = r.path.clone();
                    self.select(Some(p));
                    self.edit_selected();
                }
            }
            UiEvent::Key { key, modifiers } => self.shortcut(key, modifiers),
            _ => {}
        }
    }

    fn shortcut(&mut self, key: Key, m: Modifiers) {
        let cmd = m.ctrl || m.logo;
        match key {
            Key::Char('n') if cmd => self.open_new_project(),
            Key::Char('i') if cmd => self.open_folder_picker(FolderUse::ImportProject),
            Key::Char('f') if cmd => self.ui.focus(self.w.filter),
            Key::ArrowDown | Key::ArrowUp => {
                let list: Vec<PathBuf> = self.rows.iter().map(|r| r.path.clone()).collect();
                if list.is_empty() {
                    return;
                }
                let i = self.selected.as_ref().and_then(|s| list.iter().position(|p| p == s));
                let j = match (i, key) {
                    (Some(i), Key::ArrowDown) => (i + 1).min(list.len() - 1),
                    (Some(i), _) => i.saturating_sub(1),
                    (None, _) => 0,
                };
                self.select(Some(list[j].clone()));
            }
            Key::Enter => self.edit_selected(),
            Key::Delete => {
                if let Some(p) = self.selected.clone() {
                    self.open_confirm(p);
                }
            }
            _ => {}
        }
    }

    fn edit_selected(&mut self) {
        let Some(p) = self.selected.clone() else { return };
        if !neko::project::is_project_dir(&p) {
            return;
        }
        if let Some(i) = self.registry.find(&p) {
            self.registry.projects[i].touch();
            self.save_registry();
        }
        self.pending = Some(Action::OpenEditor(p));
    }

    fn duplicate(&mut self, src: &Path) {
        let base = src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "project".into());
        let parent = src.parent().unwrap_or(Path::new("."));
        let dest = (1..).map(|i| parent.join(if i == 1 { format!("{base}-copy") } else { format!("{base}-copy{i}") })).find(|p| !p.exists()).expect("a free name");
        match copy_dir(src, &dest) {
            Ok(()) => {
                if let Ok(mut c) = ProjectConfig::load(&dest) {
                    c.project.name = format!("{} (Copy)", c.project.name);
                    let _ = c.save(&dest);
                }
                let _ = self.registry.add(dest.clone());
                self.save_registry();
                self.configs.remove(&dest);
                self.load_configs();
                self.selected = Some(dest.clone());
                self.rebuild_list();
                self.notify(format!("Duplicated to {}.", display_path(&dest)));
            }
            Err(e) => self.notify(format!("Could not duplicate: {e}")),
        }
    }

    // ------------------------------------------------------------- dialogs

    /// A modal with a title bar; returns (modal, body, close button).
    fn dialog_frame(&mut self, title: &str, width: f32) -> (Id, Id, Id) {
        let t = self.ui.theme.clone();
        let root = self.ui.root();
        let modal = self.ui.add(root, Control::modal());
        self.ui.set_visible(modal, true);
        let panel = self.ui.add(modal, Control::panel_container().style(t.dialog).min(width, 0.0));
        let body = self.ui.add(panel, Control::vbox().separation(8.0));
        let bar = self.ui.add(body, Control::hbox());
        self.ui.add(bar, Control::spacer());
        self.ui.add(bar, Control::label(title).font(16.0).color(t.font_focus_color));
        self.ui.add(bar, Control::spacer());
        let close = self.ui.add(
            bar,
            Control::custom(vec2(22.0, 22.0), |p, r, hovered, _| {
                let t = theme();
                icons::paint_on(p, r.shrink(4.0), Icon::Close, if hovered { t.font_hover_color } else { t.font_muted_color });
            })
            .clickable()
            .tooltip("Close (Esc)"),
        );
        self.ui.register_window(modal, title, bar, close);
        (modal, body, close)
    }

    fn close_dialog(&mut self) {
        let modal = match &self.dialog {
            Some(Dialog::NewProject(d)) => d.modal,
            Some(Dialog::Input { modal, .. } | Dialog::Confirm { modal, .. } | Dialog::Info { modal, .. }) => *modal,
            None => return,
        };
        self.ui.remove(modal);
        self.dialog = None;
    }

    fn buttons_row(&mut self, body: Id, ok_text: &str) -> (Id, Id) {
        let row = self.ui.add(body, Control::hbox().separation(12.0));
        self.ui.add(row, Control::spacer());
        let cancel = self.ui.add(row, Control::button("Cancel").min(96.0, 0.0));
        let ok = self.ui.add(row, Control::button(ok_text).primary().min(96.0, 0.0));
        self.ui.add(row, Control::spacer());
        (ok, cancel)
    }

    fn open_info(&mut self, settings: bool) {
        let t = self.ui.theme.clone();
        let (modal, body, close) = self.dialog_frame(if settings { "Settings" } else { "About Neko" }, 440.0);
        let mut single_window = None;
        if settings {
            let rows = [
                ("Language", "English".to_string()),
                ("New projects folder", display_path(&default_projects_dir())),
                ("Project list file", self.registry.file().map(display_path).unwrap_or_else(|| "not saved".into())),
                ("Window backend", self.info.window.clone()),
                ("Renderer", self.info.renderer.clone()),
                ("Interface", "neko-ui (Neko's own GUI)".into()),
            ];
            for (k, v) in rows {
                let r = self.ui.add(body, Control::hbox());
                self.ui.add(r, Control::label(k).color(t.font_muted_color).min(170.0, 0.0));
                self.ui.add(r, Control::label(v).ellipsis());
            }
            self.ui.add(body, Control::label("Change backends with --window and --render when starting neko.").font(12.5).color(t.font_muted_color));
            // interface/editor/display/single_window_mode (restart to apply, like Godot).
            let r = self.ui.add(body, Control::hbox());
            self.ui.add(r, Control::label("Single Window Mode").color(t.font_muted_color).min(170.0, 0.0).tooltip("Show dialogs inside the main window instead of as separate windows.\nRequires restarting Neko."));
            let on = crate::util::EditorSettings::load().single_window_mode;
            single_window = Some(self.ui.add(r, Control::check_box("On", on)));
        } else {
            let r = self.ui.add(body, Control::hbox().separation(12.0));
            let accent = t.accent;
            self.ui.add(r, Control::custom(vec2(48.0, 48.0), move |p, rect, _, _| icons::cat_logo(p, rect, accent, Color32::from_rgb(0x1a, 0x14, 0x05))));
            let col = self.ui.add(r, Control::vbox().v_center());
            self.ui.add(col, Control::label("Neko Engine").font(20.0).color(t.font_focus_color));
            self.ui.add(col, Control::label(format!("v{}.dev", scaffold::ENGINE_VERSION)).color(t.font_muted_color));
            self.ui.add(body, Control::label("A modular game engine written in Rust. Every window, render and audio backend is a separate crate you switch with Cargo features. This screen is drawn by neko-ui, Neko's own GUI.").wrap().min(400.0, 0.0));
            self.ui.add(body, Control::label("License: MPL-2.0").font(12.5).color(t.font_muted_color));
        }
        let row = self.ui.add(body, Control::hbox());
        self.ui.add(row, Control::spacer());
        let ok = self.ui.add(row, Control::button("OK").min(96.0, 0.0));
        self.ui.add(row, Control::spacer());
        self.dialog = Some(Dialog::Info { modal, close, ok, single_window });
    }

    fn open_confirm(&mut self, path: PathBuf) {
        let name = self.configs.get(&path).and_then(|c| c.as_ref().ok()).map(|c| c.project.name.clone()).unwrap_or_else(|| display_path(&path));
        let (modal, body, close) = self.dialog_frame("Please Confirm...", 380.0);
        self.ui.add(body, Control::label(format!("Remove \"{name}\" from the list?")));
        let muted = self.ui.theme.font_muted_color;
        self.ui.add(body, Control::label("The project files on disk are not deleted.").color(muted));
        let (ok, cancel) = self.buttons_row(body, "Remove");
        self.dialog = Some(Dialog::Confirm { modal, path, ok, cancel, close });
    }

    fn open_input(&mut self, kind: InputKind, value: &str) {
        let (title, prompt, ok_text) = match &kind {
            InputKind::Rename(_) => ("Rename Project", "Project name:", "Rename"),
            InputKind::Tags(_) => ("Manage Project Tags", "Tags, separated by commas:", "Save"),
        };
        let (modal, body, close) = self.dialog_frame(title, 460.0);
        self.ui.add(body, Control::label(prompt));
        let field = self.ui.add(body, Control::line_edit(value));
        let bad = crate::theme::BAD;
        let error = self.ui.add(body, Control::label("").color(bad).hidden());
        let (ok, cancel) = self.buttons_row(body, ok_text);
        self.ui.focus(field);
        self.dialog = Some(Dialog::Input { modal, kind, field, error, ok, cancel, close });
    }

    /// Godot's FileDialog in "Open a Directory" mode, for Import, Scan and Browse.
    fn open_folder_picker(&mut self, use_for: FolderUse) {
        use crate::file_dialog::{FileDialog, FileMode};
        let (title, start) = match use_for {
            FolderUse::ImportProject => ("Select a Project Folder", default_projects_dir()),
            FolderUse::ScanProjects => ("Select a Folder to Scan", default_projects_dir()),
            FolderUse::NewProjectPath => {
                let typed = match &self.dialog {
                    Some(Dialog::NewProject(d)) => PathBuf::from(expand_home(self.ui.text(d.path).trim())),
                    _ => default_projects_dir(),
                };
                let mut start = typed;
                while !start.is_dir() {
                    match start.parent() {
                        Some(p) => start = p.to_path_buf(),
                        None => break,
                    }
                }
                ("Select a Folder", start)
            }
        };
        let mut fd = FileDialog::open(&mut self.ui, &self.icons, FileMode::OpenDir, Some(title), &start, None, &[]);
        if use_for == FolderUse::ImportProject {
            // Like Godot's ProjectDialog: only folders holding a project can be imported.
            fd.set_validator(
                &mut self.ui,
                Box::new(|p: &Path| {
                    if neko::project::is_project_dir(p) {
                        let name = ProjectConfig::load(p).map(|c| c.project.name).or_else(|_| {
                            std::fs::read_to_string(p.join("project.godot")).ok().and_then(|t| neko::project::config_from_str(&t).ok()).map(|c| c.project.name).ok_or(())
                        });
                        (format!("Project found: {}", name.unwrap_or_else(|_| "unnamed".into())), true)
                    } else {
                        ("Please choose a folder containing a project.godot or project.neko file.".into(), false)
                    }
                }),
            );
        }
        self.files = Some((fd, use_for));
    }

    fn handle_files(&mut self, ev: UiEvent) {
        use crate::file_dialog::FdResult;
        let Some((mut fd, use_for)) = self.files.take() else { return };
        let r = fd.handle(&mut self.ui, &ev);
        if let Some(text) = fd.clipboard.take() {
            self.clipboard = Some(text);
        }
        match r {
            FdResult::Cancelled => fd.close(&mut self.ui),
            FdResult::Confirmed(paths) => {
                fd.close(&mut self.ui);
                if let Some(p) = paths.into_iter().next() {
                    self.folder_dialog_confirm(use_for, p);
                }
            }
            FdResult::Handled | FdResult::Ignored => self.files = Some((fd, use_for)),
        }
    }

    fn folder_dialog_confirm(&mut self, use_for: FolderUse, path: PathBuf) {
        match use_for {
            FolderUse::ImportProject => match self.registry.add(path.clone()) {
                Ok(_) => {
                    self.save_registry();
                    self.load_configs();
                    self.selected = Some(path.clone());
                    self.rebuild_list();
                    self.notify(format!("Imported {}.", display_path(&path)));
                }
                Err(e) => self.notify(e.to_string()),
            },
            FolderUse::ScanProjects => {
                let n = self.registry.scan(&path, 3);
                self.save_registry();
                self.load_configs();
                self.selected = self.sorted().first().map(|e| e.path.clone());
                self.rebuild_list();
                self.notify(format!("Found {n} new project(s) in {}.", display_path(&path)));
            }
            FolderUse::NewProjectPath => {
                if let Some(Dialog::NewProject(d)) = &self.dialog {
                    let field = d.path;
                    self.ui.set_text(field, display_path(&path));
                    self.refresh_new_project();
                }
            }
        }
    }

    fn open_new_project(&mut self) {
        let t = self.ui.theme.clone();
        let (modal, body, close) = self.dialog_frame("Create New Project", 480.0);
        let parent_dir = default_projects_dir().display().to_string();
        self.ui.add(body, Control::label("Project Name:"));
        let name = self.ui.add(body, Control::line_edit("New Game Project"));
        let path_head = self.ui.add(body, Control::hbox());
        self.ui.add(path_head, Control::label("Project Path:").v_center());
        self.ui.add(path_head, Control::spacer());
        self.ui.add(path_head, Control::label("Create Folder").v_center());
        let create_folder = self.ui.add(path_head, Control::switch(true).v_center());
        // Godot's ProjectDialog: the path field with a Browse button.
        let path_row = self.ui.add(body, Control::hbox().separation(4.0));
        let path = self.ui.add(path_row, Control::line_edit(parent_dir.clone()).monospace().expand_h());
        let browse = self.ui.add(path_row, crate::ui_kit::icon_text_button(&self.icons, "Folder", "Browse"));
        let message = self.ui.add(body, Control::label("").align(Align::Center).font(13.0));

        self.ui.add(body, Control::label("Renderer:"));
        let rr = self.ui.add(body, Control::hbox().separation(10.0));
        let radios = self.ui.add(rr, Control::vbox().min(160.0, 0.0));
        let mut renderers = Vec::new();
        for &r in RenderBackend::ALL {
            let label = if r.implemented() { r.label().to_string() } else { format!("{} (planned)", r.label()) };
            let id = self.ui.add(radios, Control::radio(label, r == RenderBackend::Wgpu).disabled(!r.implemented()).tooltip(r.description()));
            renderers.push((id, r));
        }
        self.ui.add(rr, Control::v_separator());
        let bullets = self.ui.add(rr, Control::vbox().separation(4.0).expand_h());
        self.ui.add(body, Control::label("The renderer can be changed later in Project Settings.").font(12.5).color(t.font_muted_color));

        let wr = self.ui.add(body, Control::hbox().separation(12.0));
        self.ui.add(wr, Control::label("Window & Input:").v_center());
        let mut windows = Vec::new();
        for &w in WindowBackend::ALL {
            let id = self.ui.add(wr, Control::radio(w.label(), w == WindowBackend::Winit).disabled(!w.implemented()).tooltip(w.description()));
            windows.push((id, w));
        }
        let vr = self.ui.add(body, Control::hbox());
        self.ui.add(vr, Control::label("Version Control Metadata:").v_center());
        let vcs = self.ui.add(vr, Control::option_button(&["Git", "None"], 0).min(110.0, 0.0));
        let er = self.ui.add(body, Control::hbox());
        self.ui.add(er, Control::spacer());
        let edit_now = self.ui.add(er, Control::check_box("Edit Now", true));
        self.ui.add(er, Control::spacer());
        let (create, cancel) = self.buttons_row(body, "Create");
        self.ui.focus(name);
        self.dialog = Some(Dialog::NewProject(NewDialog {
            modal,
            close,
            name,
            path,
            browse,
            create_folder,
            message,
            renderers,
            bullets,
            windows,
            vcs,
            edit_now,
            cancel,
            create,
            parent_dir,
            custom_path: String::new(),
        }));
        self.refresh_new_project();
    }

    fn new_project_data(&self) -> Option<NewProject> {
        let Some(Dialog::NewProject(d)) = &self.dialog else { return None };
        let ui = &self.ui;
        let name = ui.text(d.name).to_string();
        let path = if ui.is_checked(d.create_folder) {
            Path::new(&expand_home(ui.text(d.path).trim())).join(slug(&name))
        } else {
            PathBuf::from(expand_home(ui.text(d.path).trim()))
        };
        let render = d.renderers.iter().find(|(id, _)| ui.is_checked(*id)).map_or(RenderBackend::Wgpu, |(_, r)| *r);
        let window = d.windows.iter().find(|(id, _)| ui.is_checked(*id)).map_or(WindowBackend::Winit, |(_, w)| *w);
        let mut backends = BackendSet { window, render, audio: AudioBackend::None };
        backends.normalize();
        let targets = TargetPlatform::ALL
            .into_iter()
            .filter(|&p| backends.support(p) == Support::Yes && matches!(p, TargetPlatform::Windows | TargetPlatform::Macos | TargetPlatform::Linux))
            .collect();
        Some(NewProject { name, path, template: Template::Empty, backends, targets, git: ui.selected(d.vcs) == 0, engine_path: engine_checkout() })
    }

    /// Re-validate and update the message, bullets and Create button.
    fn refresh_new_project(&mut self) {
        let Some(np) = self.new_project_data() else { return };
        let Some(Dialog::NewProject(d)) = &self.dialog else { return };
        let (message, create, bullets, create_folder) = (d.message, d.create, d.bullets, d.create_folder);
        let problem = np.validate().err();
        let (text, color) = match &problem {
            Some(p) => (p.clone(), crate::theme::BAD),
            None if self.ui.is_checked(create_folder) => (format!("The project folder will be created automatically: {}", display_path(&np.path)), crate::theme::OK),
            None => ("The project will be created in this folder.".to_string(), crate::theme::OK),
        };
        self.ui.set_text(message, text);
        self.ui.set_font_color(message, Some(color));
        self.ui.set_disabled(create, problem.is_some());
        self.ui.clear_children(bullets);
        let muted = self.ui.theme.font_muted_color;
        for line in renderer_bullets(np.backends.render) {
            self.ui.add(bullets, Control::label(format!("•  {line}")).font(13.0).color(muted));
        }
    }

    fn handle_dialog(&mut self, ev: UiEvent) {
        let Some(dialog) = &self.dialog else { return };
        let is_escape = matches!(ev, UiEvent::Key { key: Key::Escape, .. });
        match dialog {
            Dialog::Info { close, ok, single_window, .. } => {
                if let (Some(sw), UiEvent::Toggled(id, on)) = (*single_window, &ev)
                    && *id == sw
                {
                    let mut st = crate::util::EditorSettings::load();
                    st.single_window_mode = *on;
                    st.save();
                    self.notify("Single Window Mode changes after restarting Neko.");
                } else if is_escape || ev == UiEvent::Pressed(*close) || ev == UiEvent::Pressed(*ok) {
                    self.close_dialog();
                }
            }
            Dialog::Confirm { ok, cancel, close, path, .. } => {
                let path = path.clone();
                if ev == UiEvent::Pressed(*ok) || matches!(ev, UiEvent::Key { key: Key::Enter, .. }) {
                    if let Some(i) = self.registry.find(&path) {
                        self.registry.remove(i);
                        self.save_registry();
                    }
                    self.close_dialog();
                    self.selected = self.sorted().first().map(|e| e.path.clone());
                    self.rebuild_list();
                    self.notify("Removed from the list.");
                } else if is_escape || ev == UiEvent::Pressed(*cancel) || ev == UiEvent::Pressed(*close) {
                    self.close_dialog();
                }
            }
            Dialog::Input { field, ok, cancel, close, error, .. } => {
                let (field, error) = (*field, *error);
                if ev == UiEvent::Pressed(*ok) || ev == UiEvent::TextSubmitted(field) {
                    let value = self.ui.text(field).trim().to_string();
                    if let Err(msg) = self.submit_input(&value) {
                        self.ui.set_text(error, msg);
                        self.ui.set_visible(error, true);
                    }
                } else if is_escape || ev == UiEvent::Pressed(*cancel) || ev == UiEvent::Pressed(*close) {
                    self.close_dialog();
                }
            }
            Dialog::NewProject(d) => {
                let (close, cancel, create, name, path, create_folder, vcs, browse) = (d.close, d.cancel, d.create, d.name, d.path, d.create_folder, d.vcs, d.browse);
                let radios: Vec<Id> = d.renderers.iter().map(|(i, _)| *i).chain(d.windows.iter().map(|(i, _)| *i)).collect();
                match ev {
                    _ if is_escape => self.close_dialog(),
                    UiEvent::Pressed(id) if id == close || id == cancel => self.close_dialog(),
                    UiEvent::Pressed(id) if id == create => self.create_project(),
                    UiEvent::Pressed(id) if id == browse => self.open_folder_picker(FolderUse::NewProjectPath),
                    UiEvent::TextSubmitted(_) => {
                        if !self.ui.get(create).is_some_and(|c| c.disabled) {
                            self.create_project();
                        }
                    }
                    UiEvent::Toggled(id, on) if id == create_folder => {
                        // Like Godot: the field shows the parent folder when "Create Folder" is on.
                        let current = self.ui.text(path).to_string();
                        let next = if let Some(Dialog::NewProject(d)) = &mut self.dialog {
                            if on {
                                d.custom_path = current;
                                d.parent_dir.clone()
                            } else {
                                d.parent_dir = current.clone();
                                if d.custom_path.is_empty() { Path::new(&current).join(slug(self.ui.text(name))).display().to_string() } else { d.custom_path.clone() }
                            }
                        } else {
                            current
                        };
                        self.ui.set_text(path, next);
                        self.refresh_new_project();
                    }
                    UiEvent::TextChanged(id) if id == name || id == path => self.refresh_new_project(),
                    UiEvent::Toggled(id, _) if radios.contains(&id) => self.refresh_new_project(),
                    UiEvent::ItemSelected(id, _) if id == vcs => self.refresh_new_project(),
                    _ => {}
                }
            }
        }
    }

    fn submit_input(&mut self, value: &str) -> Result<(), String> {
        let Some(Dialog::Input { kind, .. }) = &self.dialog else { return Ok(()) };
        match kind {
            InputKind::Rename(path) => {
                let path = path.clone();
                if value.is_empty() {
                    return Err("The name cannot be empty.".into());
                }
                let mut c = ProjectConfig::load(&path).map_err(|e| e.to_string())?;
                c.project.name = value.to_string();
                c.save(&path).map_err(|e| e.to_string())?;
                self.configs.remove(&path);
                self.load_configs();
                self.close_dialog();
                self.rebuild_list();
                self.notify(format!("Renamed to {value}."));
            }
            InputKind::Tags(path) => {
                let path = path.clone();
                let tags: Vec<String> = value.split(',').map(|t| t.trim().to_lowercase()).filter(|t| !t.is_empty()).collect();
                if let Some(i) = self.registry.find(&path) {
                    self.registry.projects[i].tags = tags;
                    self.save_registry();
                }
                self.close_dialog();
                self.rebuild_list();
            }
        }
        Ok(())
    }

    fn create_project(&mut self) {
        let Some(np) = self.new_project_data() else { return };
        if np.validate().is_err() {
            return;
        }
        let edit = match &self.dialog {
            Some(Dialog::NewProject(d)) => self.ui.is_checked(d.edit_now),
            _ => false,
        };
        match scaffold::create_project(&np) {
            Ok(path) => {
                let _ = self.registry.add(path.clone());
                self.save_registry();
                self.load_configs();
                self.close_dialog();
                self.selected = Some(path.clone());
                self.rebuild_list();
                self.notify(format!("Created {}.", np.name.trim()));
                if edit {
                    self.pending = Some(Action::OpenEditor(path));
                }
            }
            Err(e) => self.notify(format!("Could not create the project: {e}")),
        }
    }

    /// Folders dropped on the project manager: find projects inside them,
    /// like Godot's ProjectManager::_files_dropped.
    pub fn import_paths(&mut self, paths: &[std::path::PathBuf]) {
        let mut folders = Vec::new();
        for p in paths {
            let dir = if p.is_dir() { p.clone() } else { p.parent().map(Path::to_path_buf).unwrap_or_else(|| p.clone()) };
            if !folders.contains(&dir) {
                folders.push(dir);
            }
        }
        let mut found = 0;
        for dir in folders {
            found += self.registry.scan(&dir, 3);
        }
        if found > 0 {
            self.save_registry();
            self.load_configs();
            self.selected = self.sorted().first().map(|e| e.path.clone());
            self.rebuild_list();
            self.notify(format!("Found {found} project(s)."));
        } else {
            self.notify("No Neko or Godot projects in the dropped folders.");
        }
    }

    /// Re-read project configs (after returning from the editor).
    pub fn reload(&mut self) {
        self.configs.clear();
        self.load_configs();
        self.rebuild_list();
    }

    /// Open the Create New Project dialog (for `--new-dialog`).
    pub fn open_new_dialog(&mut self) {
        self.open_new_project();
    }
}

/// A small rounded tag next to a project name.
fn chip(ui: &mut Ui, parent: Id, text: &str, color: Color32) {
    let bg = Color32::from_white_alpha(14);
    let c = ui.add(parent, Control::panel_container().style(StyleBox::flat(bg, 8, Sides::symmetric(7.0, 1.0))).v_center());
    ui.add(c, Control::label(text).font(11.5).color(color));
}
