//! Dialogs as separate OS windows, like Godot's `Window` nodes when
//! `embed_subwindows` is off (the editor's default; "Single Window Mode"
//! embeds them again).
//!
//! The dialog's controls stay in the same tree. Each native window gets its
//! own input context (screen size, mouse, hover, menus...) that is swapped in
//! while the window's events are handled and while it is painted.

use std::time::Instant;

use neko_platform::Event;
use neko_render::epaint::ClippedShape;

use crate::geom::{Pos2, Rect, Vec2, vec2};
use crate::{Dropdown, Id, Ui, UiEvent, UiOutput, widgets};

/// A dialog that can become a native window.
pub(crate) struct WinReg {
    pub modal: Id,
    pub title: String,
    pub close: Id,
}

/// Per-window input state.
pub(crate) struct Ctx {
    screen: Rect,
    ppp: f32,
    mouse: Option<Pos2>,
    hovered: Option<Id>,
    hover_since: Instant,
    pressed: Option<Id>,
    last_click: Option<(Id, Instant)>,
    dropdown: Option<Dropdown>,
    menus: Vec<widgets::OpenMenu>,
    menubar_open: Option<(Id, usize)>,
    drag: Option<widgets::Drag>,
}

impl Default for Ctx {
    fn default() -> Self {
        Self {
            screen: Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0)),
            ppp: 1.0,
            mouse: None,
            hovered: None,
            hover_since: Instant::now(),
            pressed: None,
            last_click: None,
            dropdown: None,
            menus: Vec::new(),
            menubar_open: None,
            drag: None,
        }
    }
}

/// A native window the app should have open.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowInfo {
    pub id: Id,
    pub title: String,
    /// Minimum content size in points (the dialog's combined minimum size).
    pub min_size: Vec2,
}

impl Ui {
    /// Turn native dialog windows on (Godot's default) or off ("Single
    /// Window Mode"). Set it once, before dialogs are created.
    pub fn set_native_windows(&mut self, on: bool) {
        self.native_windows = on;
    }

    pub fn native_windows_enabled(&self) -> bool {
        self.native_windows
    }

    /// Register a dialog (`modal` with a panel child) as a window: `chrome`
    /// is its in-UI title bar (hidden when it becomes an OS window, whose
    /// decorations show the title) and `close` its close button.
    pub fn register_window(&mut self, modal: Id, title: &str, chrome: Id, close: Id) {
        self.windows.push(WinReg { modal, title: title.to_string(), close });
        if self.native_windows {
            self.set_visible(chrome, false);
        }
    }

    /// Whether `id` is a dialog shown as its own OS window right now.
    pub(crate) fn is_native(&self, id: Id) -> bool {
        self.native_windows && self.windows.iter().any(|w| w.modal == id) && self.is_visible(id)
    }

    pub(crate) fn any_native_open(&self) -> bool {
        self.native_windows && self.windows.iter().any(|w| self.is_visible(w.modal) && self.get(w.modal).is_some())
    }

    /// The native windows that should be open, oldest first.
    pub fn open_windows(&mut self) -> Vec<WindowInfo> {
        let alive: Vec<bool> = self.windows.iter().map(|w| self.slots_contains(w.modal)).collect();
        let mut it = alive.into_iter();
        self.windows.retain(|_| it.next().unwrap_or(false));
        if !self.native_windows {
            return Vec::new();
        }
        let list: Vec<(Id, String)> = self.windows.iter().filter(|w| self.is_visible(w.modal)).map(|w| (w.modal, w.title.clone())).collect();
        list.into_iter()
            .map(|(id, title)| {
                let min = self.panel_of(id).map(|p| self.compute_min(p)).unwrap_or(vec2(320.0, 200.0));
                WindowInfo { id, title, min_size: min }
            })
            .collect()
    }

    fn panel_of(&self, modal: Id) -> Option<Id> {
        self.get(modal)?.children.first().copied()
    }

    fn slots_contains(&self, id: Id) -> bool {
        self.get(id).is_some()
    }

    /// Run `f` with `win`'s input context active.
    pub(crate) fn with_window<R>(&mut self, win: Id, f: impl FnOnce(&mut Ui) -> R) -> R {
        let mut ctx = self.ctxs.remove(&win).unwrap_or_default();
        self.swap_ctx(&mut ctx);
        let prev = self.active_window.replace(win);
        let r = f(self);
        self.active_window = prev;
        self.swap_ctx(&mut ctx);
        self.ctxs.insert(win, ctx);
        r
    }

    fn swap_ctx(&mut self, c: &mut Ctx) {
        std::mem::swap(&mut self.screen, &mut c.screen);
        std::mem::swap(&mut self.ppp, &mut c.ppp);
        std::mem::swap(&mut self.mouse, &mut c.mouse);
        std::mem::swap(&mut self.hovered, &mut c.hovered);
        std::mem::swap(&mut self.hover_since, &mut c.hover_since);
        std::mem::swap(&mut self.pressed, &mut c.pressed);
        std::mem::swap(&mut self.last_click, &mut c.last_click);
        std::mem::swap(&mut self.dropdown, &mut c.dropdown);
        std::mem::swap(&mut self.menus, &mut c.menus);
        std::mem::swap(&mut self.menubar_open, &mut c.menubar_open);
        std::mem::swap(&mut self.drag, &mut c.drag);
    }

    /// Feed a platform event that happened in native window `win`.
    pub fn window_event(&mut self, win: Id, event: &Event) {
        self.event_window = Some(win);
        if matches!(event, Event::CloseRequested) {
            // The OS close button acts like the dialog's own close button.
            if let Some(close) = self.windows.iter().find(|w| w.modal == win).map(|w| w.close) {
                self.events.push(UiEvent::Pressed(close));
            }
            return;
        }
        self.with_window(win, |ui| ui.on_event(event));
    }

    /// The window that produced the latest events, if it is a native window
    /// (so menus opened in response appear in it).
    pub(crate) fn menu_target(&self) -> Option<Id> {
        match (self.active_window, self.event_window) {
            (None, Some(w)) if self.is_native(w) => Some(w),
            _ => None,
        }
    }

    /// Lay out and paint native window `win` at `size_px`.
    pub fn frame_window(&mut self, win: Id, size_px: (u32, u32), pixels_per_point: f32) -> UiOutput {
        self.with_window(win, |ui| {
            ui.ppp = pixels_per_point.max(0.25);
            ui.screen = Rect::from_min_size(Pos2::ZERO, vec2(size_px.0 as f32, size_px.1 as f32) / ui.ppp);
            let mut shapes: Vec<ClippedShape> = Vec::new();
            if let Some(panel) = ui.panel_of(win) {
                for _ in 0..2 {
                    ui.compute_min(panel);
                    let screen = ui.screen;
                    ui.arrange(panel, screen);
                }
                ui.update_hover();
                // The window background is the dialog panel itself.
                let screen = ui.screen;
                ui.paint(panel, screen, &mut shapes, None);
            }
            ui.finish_frame(shapes, false)
        })
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Control;
    use neko_platform::MouseButton;

    #[test]
    fn dialog_runs_in_its_own_window() {
        let mut ui = Ui::new(crate::Theme::neko_dark());
        ui.set_native_windows(true);
        let root = ui.root();
        let modal = ui.add(root, Control::modal());
        ui.set_visible(modal, true);
        let panel = ui.add(modal, Control::panel_container().min(200.0, 100.0));
        let col = ui.add(panel, Control::vbox());
        let chrome = ui.add(col, Control::label("Title"));
        let ok = ui.add(col, Control::button("OK").min(200.0, 40.0));
        let close = ui.add(col, Control::button("x"));
        ui.register_window(modal, "My Dialog", chrome, close);
        ui.frame((800, 600), 1.0, 2048).textures.clear();
        let wins = ui.open_windows();
        assert_eq!(wins.len(), 1);
        assert_eq!(wins[0].title, "My Dialog");
        assert!(!ui.is_visible(chrome), "the OS title bar replaces the in-UI one");

        // A click in the main window does not reach the dialog.
        ui.on_event(&Event::MouseMoved { x: 400.0, y: 300.0 });
        ui.on_event(&Event::MouseButton { button: MouseButton::Left, pressed: true });
        ui.on_event(&Event::MouseButton { button: MouseButton::Left, pressed: false });
        assert!(ui.take_events().is_empty());

        // In the dialog's window, (10, 10) is on the OK button (it fills the window top).
        ui.frame_window(modal, (300, 200), 1.0).textures.clear();
        ui.window_event(modal, &Event::MouseMoved { x: 10.0, y: 10.0 });
        ui.window_event(modal, &Event::MouseButton { button: MouseButton::Left, pressed: true });
        ui.window_event(modal, &Event::MouseButton { button: MouseButton::Left, pressed: false });
        assert!(ui.take_events().contains(&UiEvent::Pressed(ok)));

        // Closing the OS window presses the dialog's close button.
        ui.window_event(modal, &Event::CloseRequested);
        assert_eq!(ui.take_events(), vec![UiEvent::Pressed(close)]);
        ui.remove(modal);
        assert!(ui.open_windows().is_empty());
    }
}
