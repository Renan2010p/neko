//! The renderer selector (EditorNode's `renderer` OptionButton): which
//! graphics API the project and the editor draw with. Changing it asks to
//! "Save & Restart", as in Godot.

use neko::project::RenderingDriver;
use neko_ui::Color32;

use super::Editor;
use super::dialogs::Dialog;
use crate::ui_kit;

/// Godot's `forward_plus_color` / `gl_compatibility_color` / `mobile_color`.
fn driver_color(d: RenderingDriver) -> Color32 {
    match d {
        RenderingDriver::Vulkan | RenderingDriver::D3d12 | RenderingDriver::Metal => Color32::from_rgb(0x5d, 0x8c, 0x3f),
        RenderingDriver::OpenGl => Color32::from_rgb(0x55, 0x86, 0xa4),
        RenderingDriver::Software => Color32::from_rgb(0xa5, 0x55, 0x7d),
    }
}

impl Editor {
    /// The project's rendering driver (the default one if project.neko is unreadable).
    pub fn project_driver(&self) -> RenderingDriver {
        self.config.as_ref().map(|c| c.rendering_driver()).unwrap_or_else(|_| RenderingDriver::host_default())
    }

    pub fn project_path(&self) -> &std::path::Path {
        &self.path
    }

    /// Tell the editor what it is actually drawing with.
    pub fn set_running_driver(&mut self, d: Option<RenderingDriver>) {
        self.running_driver = d;
        self.refresh_renderer_selector();
    }

    /// Godot lists the renderers when the running one is the project's, and
    /// otherwise only "<running> (Overridden)".
    pub(super) fn refresh_renderer_selector(&mut self) {
        let id = self.w.renderer;
        let project = self.project_driver();
        let choices = RenderingDriver::host_choices();
        match self.running_driver {
            Some(running) if running != project => {
                self.ui.set_items(id, vec![format!("{} (Overridden)", running.label())], 0);
                self.ui.set_disabled(id, true);
                self.ui.set_tooltip(
                    id,
                    Some(format!(
                        "The project uses {}, but the editor is running with {} (command-line option, or {} is not available on this machine).",
                        project.label(),
                        running.label(),
                        project.label()
                    )),
                );
                self.ui.set_font_color(id, Some(driver_color(running)));
            }
            _ => {
                let labels = choices.iter().map(|d| d.label().to_string()).collect();
                let sel = choices.iter().position(|&d| d == project).unwrap_or(0);
                self.ui.set_items(id, labels, sel);
                self.ui.set_disabled(id, false);
                self.ui.set_font_color(id, Some(driver_color(project)));
            }
        }
    }

    /// EditorNode::_renderer_selected: keep the current choice selected and
    /// ask to restart with the new one.
    pub(super) fn renderer_selected(&mut self, index: usize) {
        let Some(&driver) = RenderingDriver::host_choices().get(index) else { return };
        let current = self.project_driver();
        self.refresh_renderer_selector();
        if driver == current {
            return;
        }
        self.close_dialog();
        let (modal, body, close) = ui_kit::dialog(&mut self.ui, "Please Confirm...", 460.0);
        self.ui.add(
            body,
            neko_ui::Control::label(format!(
                "Changing the renderer requires restarting the editor.\n\nChoosing Save & Restart will change the renderer to {}.\n\n{}",
                driver.label(),
                driver.description()
            ))
            .wrap(),
        );
        let (ok, cancel) = ui_kit::ok_cancel(&mut self.ui, body, "Save & Restart", "Cancel");
        self.dialog = Some(Dialog::Restart { modal, close, ok, cancel, driver });
    }

    /// EditorNode::_set_renderer_name_save_and_restart.
    pub(super) fn save_and_restart(&mut self, driver: RenderingDriver) {
        let mut config = match self.config.clone() {
            Ok(c) => c,
            Err(e) => {
                self.notify(format!("Could not read project.neko: {e}"));
                return;
            }
        };
        config.set_rendering_driver(driver);
        if let Err(e) = config.save(&self.path) {
            self.notify(format!("Could not save project.neko: {e}"));
            return;
        }
        self.config = Ok(config);
        self.save_all();
        self.stop_project();
        self.restart = true;
    }
}
