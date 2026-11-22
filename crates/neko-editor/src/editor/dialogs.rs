//! The editor's small dialogs, titled as in Godot: "Save Scene As...",
//! "Open a File", "Project Settings", "About Neko" and "Please Confirm...".

use std::path::PathBuf;

use neko::platform::Key;
use neko::project::{AudioBackend, BackendChoice, ProjectConfig, RenderBackend, RenderingDriver, WindowBackend};
use neko_ui::{Control, Id, UiEvent, vec2};

use super::filesystem::FileOp;
use super::{Editor, Pending};
use crate::ui_kit;

pub(super) enum Dialog {
    Settings { modal: Id, close: Id, name: Id, main_scene: Id, windows: Vec<(Id, WindowBackend)>, renders: Vec<(Id, RenderBackend)>, audios: Vec<(Id, AudioBackend)>, ok: Id, cancel: Id },
    About { modal: Id, close: Id, ok: Id },
    Confirm { modal: Id, close: Id, save: Id, discard: Id, cancel: Id, pending: Pending },
    /// DirectoryCreateDialog / SceneCreateDialog / ScriptCreateDialog.
    File { modal: Id, close: Id, field: Id, error: Id, root: Option<Id>, ok: Id, cancel: Id, op: FileOp },
    /// The FileSystem dock's delete confirmation.
    Delete { modal: Id, close: Id, ok: Id, cancel: Id, path: PathBuf },
    /// "Close and save changes?" for a modified script.
    CloseScript { modal: Id, close: Id, save: Id, discard: Id, cancel: Id, index: usize },
    /// Script editor's "Go to Line".
    GoToLine { modal: Id, close: Id, field: Id, ok: Id, cancel: Id },
    /// "Attach Node Script".
    AttachScript(Box<super::attach_script::Attach>),
    /// "Changing the renderer requires restarting the editor."
    Restart { modal: Id, close: Id, ok: Id, cancel: Id, driver: RenderingDriver },
}

impl Dialog {
    fn modal(&self) -> Id {
        match self {
            Dialog::AttachScript(a) => a.modal,
            Dialog::Settings { modal, .. }
            | Dialog::About { modal, .. }
            | Dialog::Confirm { modal, .. }
            | Dialog::File { modal, .. }
            | Dialog::Delete { modal, .. }
            | Dialog::Restart { modal, .. }
            | Dialog::CloseScript { modal, .. }
            | Dialog::GoToLine { modal, .. } => *modal,
        }
    }
}

impl Editor {
    pub(super) fn close_dialog(&mut self) {
        if let Some(d) = self.dialog.take() {
            self.ui.remove(d.modal());
        }
    }

    /// Godot's "Save Scene As..." (an EditorFileDialog in Save mode, inside res://).
    pub(super) fn open_save_as(&mut self, doc: usize, suggested: String) {
        use crate::file_dialog::{FileDialog, FileMode};
        self.close_dialog();
        let rel = suggested.trim_start_matches("res://");
        let wanted = self.path.join(rel);
        let mut start = wanted.parent().map(PathBuf::from).unwrap_or_else(|| self.path.clone());
        while !start.is_dir() && start != self.path {
            start = start.parent().map(PathBuf::from).unwrap_or_else(|| self.path.clone());
        }
        let mut fd = FileDialog::open(&mut self.ui, &self.icons, FileMode::SaveFile, Some("Save Scene As..."), &start, Some(&self.path), &[("*.tscn", "Text Scene"), ("*.scn", "Binary Scene")]);
        fd.set_file_name(&mut self.ui, wanted.file_name().map(|n| n.to_string_lossy().into_owned()).as_deref().unwrap_or("scene.tscn"));
        self.files_dialog = Some((fd, super::FilePurpose::SaveSceneAs(doc)));
    }

    /// Scene > Open Scene... (Ctrl+O): EditorFileDialog in Open mode, inside res://.
    pub(super) fn open_scene_dialog(&mut self) {
        use crate::file_dialog::{FileDialog, FileMode};
        self.close_dialog();
        let start = self.docs.get(self.current).and_then(|d| d.file.as_ref()).and_then(|f| f.parent()).map(PathBuf::from).unwrap_or_else(|| self.path.clone());
        let fd = FileDialog::open(&mut self.ui, &self.icons, FileMode::OpenFile, Some("Open Scene"), &start, Some(&self.path), &[("*.tscn", "Text Scene"), ("*.scn", "Binary Scene")]);
        self.files_dialog = Some((fd, super::FilePurpose::OpenScene));
    }

    /// Events for the file dialog; true when it used the event.
    pub(super) fn handle_files_dialog(&mut self, ev: &UiEvent) -> bool {
        use crate::file_dialog::FdResult;
        let Some((mut fd, purpose)) = self.files_dialog.take() else { return false };
        let r = fd.handle(&mut self.ui, ev);
        if let Some(t) = fd.clipboard.take() {
            self.clipboard = Some(t);
        }
        match r {
            FdResult::Cancelled => fd.close(&mut self.ui),
            FdResult::Confirmed(paths) => {
                fd.close(&mut self.ui);
                match purpose {
                    super::FilePurpose::OpenScene => {
                        if let Some(p) = paths.into_iter().next() {
                            self.open_scene_file(p);
                        }
                    }
                    super::FilePurpose::SaveSceneAs(doc) => {
                        if let Some(p) = paths.into_iter().next()
                            && doc < self.docs.len()
                        {
                            self.docs[doc].file = Some(p);
                            self.save_doc(doc);
                        }
                    }
                    super::FilePurpose::Import(target) => self.import_into(paths, &target),
                    super::FilePurpose::AttachScriptPath => {
                        if let Some(p) = paths.into_iter().next() {
                            self.attach_path_chosen(p);
                        }
                    }
                    super::FilePurpose::OpenScript => {
                        if let Some(p) = paths.into_iter().next() {
                            self.open_script(p);
                        }
                    }
                    super::FilePurpose::SaveScriptAs(i) => {
                        if let Some(p) = paths.into_iter().next() {
                            self.save_script_as(i, p);
                        }
                    }
                }
            }
            FdResult::Handled | FdResult::Ignored => self.files_dialog = Some((fd, purpose)),
        }
        true
    }

    pub(super) fn open_project_settings(&mut self) {
        self.close_dialog();
        let Ok(c) = &self.config else {
            self.notify("project.neko could not be read, so settings cannot be edited here.");
            return;
        };
        let c = c.clone();
        let (modal, body, close) = ui_kit::dialog(&mut self.ui, "Project Settings", 560.0);
        let t = self.ui.theme.clone();
        let field = |ui: &mut neko_ui::Ui, label: &str| {
            let row = ui.add(body, Control::hbox());
            ui.add(row, Control::label(label).min(150.0, 0.0).v_center());
            row
        };
        let r = field(&mut self.ui, "Name");
        let name = self.ui.add(r, Control::line_edit(c.project.name.clone()));
        let r = field(&mut self.ui, "Main Scene");
        let main_scene = self.ui.add(r, Control::line_edit(format!("res://{}", c.project.main_scene)).monospace());
        let r = field(&mut self.ui, "Window & Input");
        let windows = WindowBackend::ALL.iter().map(|&w| (self.ui.add(r, Control::radio(w.label(), w == c.backends.window).disabled(!w.implemented()).tooltip(w.description())), w)).collect();
        let r = field(&mut self.ui, "Renderer");
        let renders = RenderBackend::ALL.iter().map(|&b| (self.ui.add(r, Control::radio(b.label(), b == c.backends.render).disabled(!b.implemented()).tooltip(b.description())), b)).collect();
        let r = field(&mut self.ui, "Audio");
        let audios = AudioBackend::ALL.iter().map(|&b| (self.ui.add(r, Control::radio(b.label(), b == c.backends.audio).tooltip(b.description())), b)).collect();
        self.ui.add(body, Control::label("Backends also need the matching features in Cargo.toml.").font(12.5).color(t.font_muted_color));
        let (ok, cancel) = ui_kit::ok_cancel(&mut self.ui, body, "Save", "Cancel");
        self.dialog = Some(Dialog::Settings { modal, close, name, main_scene, windows, renders, audios, ok, cancel });
    }

    pub(super) fn open_about(&mut self) {
        self.close_dialog();
        let (modal, body, close) = ui_kit::dialog(&mut self.ui, "About Neko", 460.0);
        let t = self.ui.theme.clone();
        let row = self.ui.add(body, Control::hbox().separation(14.0));
        self.ui.add(row, Control::custom(vec2(64.0, 64.0), |p, r, _, _| neko_ui::splash::draw_logo(p, r)));
        let col = self.ui.add(row, Control::vbox().v_center());
        self.ui.add(col, Control::label("Neko Engine").font(20.0).color(t.font_focus_color));
        self.ui.add(col, Control::label(format!("v{}.dev", neko::project::scaffold::ENGINE_VERSION)).color(t.font_muted_color));
        self.ui.add(body, Control::label("A reimplementation of the Godot editor and engine in Rust, with every window, render and audio backend as a separate crate.").wrap().min(420.0, 0.0));
        self.ui.add(body, Control::label("License: MPL-2.0. Includes Godot's editor icons (MIT).").font(12.5).color(t.font_muted_color));
        let r = self.ui.add(body, Control::hbox());
        self.ui.add(r, Control::spacer());
        let ok = self.ui.add(r, Control::button("OK").min(96.0, 0.0));
        self.ui.add(r, Control::spacer());
        self.dialog = Some(Dialog::About { modal, close, ok });
    }

    pub(super) fn open_confirm(&mut self, pending: Pending) {
        self.close_dialog();
        let what = match pending {
            Pending::CloseTab(i) => format!("Save changes to '{}' before closing?", self.docs.get(i).map(|d| d.title()).unwrap_or_default()),
            _ => "Save changes to the following scene(s) before closing?".to_string(),
        };
        let (modal, body, close) = ui_kit::dialog(&mut self.ui, "Please Confirm...", 420.0);
        self.ui.add(body, Control::label(what).wrap());
        let row = self.ui.add(body, Control::hbox().separation(10.0));
        self.ui.add(row, Control::spacer());
        let cancel = self.ui.add(row, Control::button("Cancel").min(90.0, 0.0));
        let discard = self.ui.add(row, Control::button("Don't Save").min(90.0, 0.0));
        let save = self.ui.add(row, Control::button("Save").primary().min(90.0, 0.0));
        self.ui.add(row, Control::spacer());
        self.dialog = Some(Dialog::Confirm { modal, close, save, discard, cancel, pending });
    }

    pub(super) fn handle_dialog(&mut self, ev: UiEvent) {
        if matches!(self.dialog, Some(Dialog::AttachScript(_))) {
            return self.attach_event(ev);
        }
        let Some(dialog) = &self.dialog else { return };
        let esc = matches!(ev, UiEvent::Key { key: Key::Escape, .. });
        match dialog {
            Dialog::Settings { close, name, main_scene, windows, renders, audios, ok, cancel, .. } => {
                if esc || ev == UiEvent::Pressed(*close) || ev == UiEvent::Pressed(*cancel) {
                    self.close_dialog();
                } else if ev == UiEvent::Pressed(*ok) {
                    let (window, render, audio) = (pick(&self.ui, windows), pick(&self.ui, renders), pick(&self.ui, audios));
                    let (name, main) = (self.ui.text(*name).trim().to_string(), self.ui.text(*main_scene).trim().trim_start_matches("res://").to_string());
                    self.close_dialog();
                    match ProjectConfig::load(&self.path) {
                        Ok(mut c) => {
                            c.project.name = name;
                            c.project.main_scene = main;
                            if let Some(w) = window {
                                c.backends.window = w;
                            }
                            if let Some(r) = render {
                                c.backends.render = r;
                            }
                            if let Some(a) = audio {
                                c.backends.audio = a;
                            }
                            c.backends.normalize();
                            match c.save(&self.path) {
                                Ok(()) => {
                                    self.config = Ok(c);
                                    self.notify("Project settings saved to project.neko.");
                                }
                                Err(e) => self.notify(format!("Could not save settings: {e}")),
                            }
                        }
                        Err(e) => self.notify(format!("Could not read project.neko: {e}")),
                    }
                }
            }
            Dialog::About { close, ok, .. } => {
                if esc || ev == UiEvent::Pressed(*close) || ev == UiEvent::Pressed(*ok) {
                    self.close_dialog();
                }
            }
            Dialog::Confirm { close, save, discard, cancel, pending, .. } => {
                let pending = *pending;
                if esc || ev == UiEvent::Pressed(*close) || ev == UiEvent::Pressed(*cancel) {
                    self.close_dialog();
                    return;
                }
                let proceed = if ev == UiEvent::Pressed(*save) {
                    self.close_dialog();
                    match pending {
                        Pending::CloseTab(i) => self.save_doc(i),
                        _ => {
                            self.save_all();
                            !self.is_dirty()
                        }
                    }
                } else if ev == UiEvent::Pressed(*discard) {
                    self.close_dialog();
                    true
                } else {
                    return;
                };
                if proceed {
                    match pending {
                        Pending::Back => {
                            self.stop_project();
                            self.back = true;
                        }
                        Pending::Quit => {
                            self.stop_project();
                            self.quit = true;
                        }
                        Pending::CloseTab(i) => self.close_tab(i, true),
                    }
                }
            }
            Dialog::File { close, field, error, root, ok, cancel, op, .. } => {
                let (field, error, root, ok, cancel) = (*field, *error, *root, *ok, *cancel);
                let op = op.clone();
                if esc || ev == UiEvent::Pressed(*close) || ev == UiEvent::Pressed(cancel) {
                    self.close_dialog();
                } else if ev == UiEvent::Pressed(ok) || ev == UiEvent::TextSubmitted(field) {
                    let name = self.ui.text(field).trim().to_string();
                    let base = root.map_or(0, |r| self.ui.selected(r));
                    match self.apply_file_dialog(&op, &name, base) {
                        Ok(()) => self.close_dialog(),
                        Err(e) => {
                            self.ui.set_text(error, e);
                            self.ui.set_visible(error, true);
                        }
                    }
                }
            }
            // Handled above by attach_event.
            Dialog::AttachScript(_) => {}
            Dialog::CloseScript { close, save, discard, cancel, index, .. } => {
                let index = *index;
                if esc || ev == UiEvent::Pressed(*close) || ev == UiEvent::Pressed(*cancel) {
                    self.close_dialog();
                } else if ev == UiEvent::Pressed(*save) || ev == UiEvent::Pressed(*discard) {
                    let save = ev == UiEvent::Pressed(*save);
                    self.close_dialog();
                    self.close_script_answer(index, save);
                }
            }
            Dialog::GoToLine { close, field, ok, cancel, .. } => {
                let field = *field;
                if esc || ev == UiEvent::Pressed(*close) || ev == UiEvent::Pressed(*cancel) {
                    self.close_dialog();
                } else if ev == UiEvent::Pressed(*ok) || ev == UiEvent::TextSubmitted(field) {
                    let text = self.ui.text(field).to_string();
                    self.close_dialog();
                    self.go_to_line_answer(&text);
                }
            }
            Dialog::Restart { close, ok, cancel, driver, .. } => {
                let driver = *driver;
                if esc || ev == UiEvent::Pressed(*close) || ev == UiEvent::Pressed(*cancel) {
                    self.close_dialog();
                } else if ev == UiEvent::Pressed(*ok) {
                    self.close_dialog();
                    self.save_and_restart(driver);
                }
            }
            Dialog::Delete { close, ok, cancel, path, .. } => {
                let path = path.clone();
                if esc || ev == UiEvent::Pressed(*close) || ev == UiEvent::Pressed(*cancel) {
                    self.close_dialog();
                } else if ev == UiEvent::Pressed(*ok) {
                    self.close_dialog();
                    if let Err(e) = self.delete_path(&path) {
                        self.notify(e);
                    }
                }
            }
        }
    }
}

/// The backend whose radio button is checked.
fn pick<B: Copy>(ui: &neko_ui::Ui, ids: &[(Id, B)]) -> Option<B> {
    ids.iter().find(|(id, _)| ui.is_checked(*id)).map(|(_, b)| *b)
}
