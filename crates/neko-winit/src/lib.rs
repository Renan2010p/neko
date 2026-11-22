//! `winit` platform backend: windowing and input in pure Rust.

use std::sync::Arc;
#[cfg(feature = "gamepad-sdl3")]
use std::sync::Mutex;
#[cfg(feature = "gamepad-sdl3")]
use std::time::Duration;
use std::time::Instant;

#[cfg(feature = "gamepad-sdl3")]
use neko_gamepad_sdl3::Gamepads;

use neko_platform::rwh::{self, HasDisplayHandle, HasWindowHandle};
use neko_platform::{
    App, CursorIcon, Event, Flow, Key, Modifiers, MouseButton, Platform, PlatformError, WheelUnit,
    Window, WindowConfig,
};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::{ElementState, MouseScrollDelta, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key as WKey, NamedKey};
use winit::window::WindowId;

pub struct WinitPlatform;

impl WinitPlatform {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WinitPlatform {
    fn default() -> Self {
        Self::new()
    }
}

impl Platform for WinitPlatform {
    fn name(&self) -> &'static str {
        "winit"
    }
    fn supports_subwindows(&self) -> bool {
        true
    }

    fn run(self: Box<Self>, config: WindowConfig, app: Box<dyn App>) -> Result<(), PlatformError> {
        let event_loop = EventLoop::new().map_err(|e| PlatformError::Init(e.to_string()))?;
        let mut handler = Handler {
            config,
            app,
            window: None,
            subs: std::collections::HashMap::new(),
            modifiers: Modifiers::default(),
            error: None,
            flow: Flow::Wait,
            #[cfg(feature = "gamepad-sdl3")]
            gamepads: None,
        };
        event_loop.run_app(&mut handler).map_err(|e| PlatformError::Init(e.to_string()))?;
        match handler.error {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

/// A winit window exposed through the [`Window`] trait.
pub struct WinitWindow {
    pub window: Arc<winit::window::Window>,
    #[cfg(feature = "gamepad-sdl3")]
    gamepads: Option<Arc<Mutex<Gamepads>>>,
}

impl HasWindowHandle for WinitWindow {
    fn window_handle(&self) -> Result<rwh::WindowHandle<'_>, rwh::HandleError> {
        self.window.window_handle()
    }
}
impl HasDisplayHandle for WinitWindow {
    fn display_handle(&self) -> Result<rwh::DisplayHandle<'_>, rwh::HandleError> {
        self.window.display_handle()
    }
}

impl Window for WinitWindow {
    fn surface(self: Arc<Self>) -> neko_platform::Surface {
        neko_platform::Surface::Native(self)
    }
    fn inner_size(&self) -> (u32, u32) {
        let s = self.window.inner_size();
        (s.width, s.height)
    }
    fn scale_factor(&self) -> f32 {
        self.window.scale_factor() as f32
    }
    fn set_title(&self, title: &str) {
        self.window.set_title(title);
    }
    fn request_redraw(&self) {
        self.window.request_redraw();
    }
    fn set_text_input(&self, enabled: bool) {
        self.window.set_ime_allowed(enabled);
    }
    #[cfg(feature = "gamepad-sdl3")]
    fn rumble(&self, id: u32, weak: f32, strong: f32, duration_ms: u32) -> bool {
        self.gamepads.as_ref().is_some_and(|g| g.lock().is_ok_and(|g| g.rumble(id, weak, strong, duration_ms)))
    }
    fn set_fullscreen(&self, fullscreen: bool) {
        self.window.set_fullscreen(fullscreen.then_some(winit::window::Fullscreen::Borderless(None)));
    }
    fn set_cursor(&self, cursor: CursorIcon) {
        use winit::window::CursorIcon as C;
        self.window.set_cursor(match cursor {
            CursorIcon::Default => C::Default,
            CursorIcon::Pointer => C::Pointer,
            CursorIcon::Text => C::Text,
            CursorIcon::Move => C::Move,
            CursorIcon::Grab => C::Grab,
            CursorIcon::Grabbing => C::Grabbing,
            CursorIcon::NotAllowed => C::NotAllowed,
            CursorIcon::ResizeHorizontal => C::EwResize,
            CursorIcon::ResizeVertical => C::NsResize,
            CursorIcon::Crosshair => C::Crosshair,
        });
    }
}

struct Handler {
    config: WindowConfig,
    app: Box<dyn App>,
    window: Option<Arc<winit::window::Window>>,
    /// Extra windows: app id -> winit window.
    subs: std::collections::HashMap<u64, Arc<winit::window::Window>>,
    modifiers: Modifiers,
    error: Option<PlatformError>,
    /// What the app asked for after its last frame.
    flow: Flow,
    #[cfg(feature = "gamepad-sdl3")]
    gamepads: Option<Arc<Mutex<Gamepads>>>,
}

/// How often gamepads are polled while the app is otherwise idle.
#[cfg(feature = "gamepad-sdl3")]
const GAMEPAD_POLL: Duration = Duration::from_millis(16);

impl Handler {
    fn apply_flow(&mut self, el: &ActiveEventLoop, flow: Flow) {
        self.flow = flow;
        match flow {
            Flow::Exit => el.exit(),
            Flow::Wait => el.set_control_flow(ControlFlow::Wait),
            Flow::WaitUntil(t) => el.set_control_flow(ControlFlow::WaitUntil(t)),
            Flow::Poll => {
                el.set_control_flow(ControlFlow::Poll);
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
        }
    }
}

impl Handler {
    /// Platform events of an extra window (same mapping as the main window).
    fn translate(&mut self, event: WindowEvent) -> Vec<Event> {
        let mut out = Vec::new();
        match event {
            WindowEvent::CloseRequested => out.push(Event::CloseRequested),
            WindowEvent::Resized(PhysicalSize { width, height }) => out.push(Event::Resized { width, height }),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => out.push(Event::ScaleFactorChanged(scale_factor as f32)),
            WindowEvent::Focused(f) => out.push(Event::Focused(f)),
            WindowEvent::CursorMoved { position, .. } => out.push(Event::MouseMoved { x: position.x as f32, y: position.y as f32 }),
            WindowEvent::CursorLeft { .. } => out.push(Event::MouseLeft),
            WindowEvent::MouseInput { state, button, .. } => out.push(Event::MouseButton {
                button: match button {
                    winit::event::MouseButton::Left => MouseButton::Left,
                    winit::event::MouseButton::Right => MouseButton::Right,
                    winit::event::MouseButton::Middle => MouseButton::Middle,
                    winit::event::MouseButton::Back => MouseButton::Back,
                    winit::event::MouseButton::Forward => MouseButton::Forward,
                    winit::event::MouseButton::Other(n) => MouseButton::Other(n),
                },
                pressed: state == ElementState::Pressed,
            }),
            WindowEvent::MouseWheel { delta, .. } => out.push(match delta {
                MouseScrollDelta::LineDelta(x, y) => Event::MouseWheel { delta_x: x, delta_y: y, unit: WheelUnit::Lines },
                MouseScrollDelta::PixelDelta(p) => Event::MouseWheel { delta_x: p.x as f32, delta_y: p.y as f32, unit: WheelUnit::Pixels },
            }),
            WindowEvent::ModifiersChanged(m) => {
                let s = m.state();
                self.modifiers = Modifiers { shift: s.shift_key(), ctrl: s.control_key(), alt: s.alt_key(), logo: s.super_key() };
                out.push(Event::ModifiersChanged(self.modifiers));
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state == ElementState::Pressed;
                out.push(Event::Key { key: map_key(&event.logical_key), pressed, repeat: event.repeat, modifiers: self.modifiers });
                if pressed
                    && !self.modifiers.ctrl
                    && !self.modifiers.logo
                    && let Some(text) = event.text
                    && text.chars().all(|c| !c.is_control())
                {
                    out.push(Event::Text(text.to_string()));
                }
            }
            WindowEvent::Ime(winit::event::Ime::Commit(text)) => out.push(Event::Text(text)),
            WindowEvent::DroppedFile(path) => out.push(Event::DroppedFiles(vec![path])),
            _ => {}
        }
        out
    }

    /// Open and close extra windows to match what the app lists.
    fn sync_subwindows(&mut self, el: &ActiveEventLoop) {
        let wanted = self.app.subwindows();
        let gone: Vec<u64> = self.subs.keys().copied().filter(|k| !wanted.iter().any(|(w, _)| w == k)).collect();
        for k in gone {
            self.subs.remove(&k);
            self.app.subwindow_closed(k);
        }
        for (k, c) in wanted {
            if self.subs.contains_key(&k) {
                continue;
            }
            let attrs = winit::window::Window::default_attributes()
                .with_title(c.title.clone())
                .with_inner_size(LogicalSize::new(c.width, c.height))
                .with_min_inner_size(LogicalSize::new(c.min_width, c.min_height))
                .with_resizable(c.resizable);
            match el.create_window(attrs) {
                Ok(w) => {
                    let w = Arc::new(w);
                    self.subs.insert(k, w.clone());
                    self.app.subwindow_opened(
                        k,
                        Arc::new(WinitWindow {
                            window: w,
                            #[cfg(feature = "gamepad-sdl3")]
                            gamepads: self.gamepads.clone(),
                        }),
                    );
                }
                Err(e) => log::error!("cannot open window \"{}\": {e}", c.title),
            }
        }
    }
}

impl ApplicationHandler for Handler {
    fn new_events(&mut self, _el: &ActiveEventLoop, cause: StartCause) {
        // Wake-ups for gamepad polling do not redraw; only the app's own deadline does.
        if let StartCause::ResumeTimeReached { .. } = cause
            && let Flow::WaitUntil(t) = self.flow
            && Instant::now() >= t
            && let Some(w) = &self.window
        {
            w.request_redraw();
        }
    }

    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let c = &self.config;
        let attrs = winit::window::Window::default_attributes()
            .with_title(c.title.clone())
            .with_inner_size(LogicalSize::new(c.width, c.height))
            .with_min_inner_size(LogicalSize::new(c.min_width, c.min_height))
            .with_resizable(c.resizable);
        match el.create_window(attrs) {
            Ok(w) => {
                let w = Arc::new(w);
                self.window = Some(w.clone());
                #[cfg(feature = "gamepad-sdl3")]
                if self.gamepads.is_none() {
                    match Gamepads::standalone() {
                        Ok(g) => self.gamepads = Some(Arc::new(Mutex::new(g))),
                        Err(e) => log::info!("gamepads unavailable (SDL3 not loaded): {e}"),
                    }
                }
                self.app.resumed(Arc::new(WinitWindow {
                    window: w.clone(),
                    #[cfg(feature = "gamepad-sdl3")]
                    gamepads: self.gamepads.clone(),
                }));
                w.request_redraw();
            }
            Err(e) => {
                self.error = Some(PlatformError::CreateWindow(e.to_string()));
                el.exit();
            }
        }
    }

    fn suspended(&mut self, _el: &ActiveEventLoop) {
        self.app.suspended();
    }

    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        // Events of extra windows go to the app tagged with its id.
        let sub = self.subs.iter().find(|(_, w)| w.id() == id).map(|(k, _)| *k);
        if let Some(sid) = sub {
            if let WindowEvent::RedrawRequested = event {
                // Every window is drawn by the main frame.
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
                return;
            }
            for e in self.translate(event) {
                self.app.subwindow_event(sid, e);
            }
            self.sync_subwindows(el);
            if let Some(w) = &self.window {
                w.request_redraw();
            }
            return;
        }
        let out = match event {
            WindowEvent::RedrawRequested => {
                let flow = self.app.frame();
                self.apply_flow(el, flow);
                return;
            }
            WindowEvent::CloseRequested => Some(Event::CloseRequested),
            WindowEvent::Resized(PhysicalSize { width, height }) => Some(Event::Resized { width, height }),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => Some(Event::ScaleFactorChanged(scale_factor as f32)),
            WindowEvent::Focused(f) => Some(Event::Focused(f)),
            WindowEvent::CursorMoved { position, .. } => Some(Event::MouseMoved { x: position.x as f32, y: position.y as f32 }),
            WindowEvent::CursorLeft { .. } => Some(Event::MouseLeft),
            WindowEvent::MouseInput { state, button, .. } => Some(Event::MouseButton {
                button: match button {
                    winit::event::MouseButton::Left => MouseButton::Left,
                    winit::event::MouseButton::Right => MouseButton::Right,
                    winit::event::MouseButton::Middle => MouseButton::Middle,
                    winit::event::MouseButton::Back => MouseButton::Back,
                    winit::event::MouseButton::Forward => MouseButton::Forward,
                    winit::event::MouseButton::Other(n) => MouseButton::Other(n),
                },
                pressed: state == ElementState::Pressed,
            }),
            WindowEvent::MouseWheel { delta, .. } => Some(match delta {
                MouseScrollDelta::LineDelta(x, y) => Event::MouseWheel { delta_x: x, delta_y: y, unit: WheelUnit::Lines },
                MouseScrollDelta::PixelDelta(p) => {
                    Event::MouseWheel { delta_x: p.x as f32, delta_y: p.y as f32, unit: WheelUnit::Pixels }
                }
            }),
            WindowEvent::ModifiersChanged(m) => {
                let s = m.state();
                self.modifiers = Modifiers { shift: s.shift_key(), ctrl: s.control_key(), alt: s.alt_key(), logo: s.super_key() };
                Some(Event::ModifiersChanged(self.modifiers))
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state == ElementState::Pressed;
                let key = map_key(&event.logical_key);
                self.app.event(Event::Key { key, pressed, repeat: event.repeat, modifiers: self.modifiers });
                // Only emit text when it is not a shortcut (Ctrl/Super + key).
                if pressed && !self.modifiers.ctrl && !self.modifiers.logo
                    && let Some(text) = event.text
                    && text.chars().all(|c| !c.is_control())
                {
                    self.app.event(Event::Text(text.to_string()));
                }
                None
            }
            WindowEvent::Ime(winit::event::Ime::Commit(text)) => Some(Event::Text(text)),
            WindowEvent::DroppedFile(path) => Some(Event::DroppedFiles(vec![path])),
            _ => None,
        };
        if let Some(e) = out {
            self.app.event(e);
        }
        self.sync_subwindows(el);
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        #[cfg(feature = "gamepad-sdl3")]
        if let Some(g) = &self.gamepads {
            let mut events = Vec::new();
            if let Ok(mut g) = g.lock() {
                g.poll(&mut events);
            }
            let any = !events.is_empty();
            for e in events {
                self.app.event(e);
            }
            if any && let Some(w) = &self.window {
                w.request_redraw();
            }
            // Keep polling while idle, without redrawing (see new_events).
            let poll_at = Instant::now() + GAMEPAD_POLL;
            match self.flow {
                Flow::Wait => el.set_control_flow(ControlFlow::WaitUntil(poll_at)),
                Flow::WaitUntil(t) => el.set_control_flow(ControlFlow::WaitUntil(t.min(poll_at))),
                Flow::Poll | Flow::Exit => {}
            }
        }
        let _ = el;
    }

    fn exiting(&mut self, _el: &ActiveEventLoop) {
        self.app.exiting();
    }
}

fn map_key(k: &WKey) -> Key {
    match k {
        WKey::Named(n) => match n {
            NamedKey::Escape => Key::Escape,
            NamedKey::Enter => Key::Enter,
            NamedKey::Tab => Key::Tab,
            NamedKey::Backspace => Key::Backspace,
            NamedKey::Delete => Key::Delete,
            NamedKey::Insert => Key::Insert,
            NamedKey::Home => Key::Home,
            NamedKey::End => Key::End,
            NamedKey::PageUp => Key::PageUp,
            NamedKey::PageDown => Key::PageDown,
            NamedKey::ArrowUp => Key::ArrowUp,
            NamedKey::ArrowDown => Key::ArrowDown,
            NamedKey::ArrowLeft => Key::ArrowLeft,
            NamedKey::ArrowRight => Key::ArrowRight,
            NamedKey::Space => Key::Space,
            NamedKey::F1 => Key::F(1),
            NamedKey::F2 => Key::F(2),
            NamedKey::F3 => Key::F(3),
            NamedKey::F4 => Key::F(4),
            NamedKey::F5 => Key::F(5),
            NamedKey::F6 => Key::F(6),
            NamedKey::F7 => Key::F(7),
            NamedKey::F8 => Key::F(8),
            NamedKey::F9 => Key::F(9),
            NamedKey::F10 => Key::F(10),
            NamedKey::F11 => Key::F(11),
            NamedKey::F12 => Key::F(12),
            _ => Key::Unknown,
        },
        WKey::Character(s) => s.chars().next().map(|c| Key::Char(c.to_ascii_lowercase())).unwrap_or(Key::Unknown),
        _ => Key::Unknown,
    }
}
