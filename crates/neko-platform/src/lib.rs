//! # neko-platform
//!
//! Platform abstraction layer for Neko. Nothing in here knows about SDL3,
//! winit or any operating system: it only defines the contracts every
//! backend implements.
//!
//! - [`Platform`]: owns the event loop, creates the window and drives the [`App`].
//! - [`Window`]: the screen the game draws to. It hands GPU backends a
//!   [`Surface`]: `raw-window-handle` handles on desktop and mobile, or an
//!   opaque platform object on consoles and other targets without windows.
//! - [`Event`]: normalized input (coordinates in physical pixels).
//!
//! This is Godot's `DisplayServer`; the [`Surface`] plays the role of the
//! per-platform data Godot's display servers pass to the rendering context.

use std::any::Any;
use std::sync::Arc;
use std::time::Instant;

pub use raw_window_handle as rwh;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

/// A native window that exposes `raw-window-handle` handles.
pub trait NativeHandles: HasWindowHandle + HasDisplayHandle + Send + Sync {}
impl<T: HasWindowHandle + HasDisplayHandle + Send + Sync + ?Sized> NativeHandles for T {}

/// What a GPU backend presents to.
///
/// Both variants keep the window alive for as long as the surface exists.
#[derive(Clone)]
pub enum Surface {
    /// A desktop or mobile window (Wayland, X11, Win32, Cocoa, Android...),
    /// usable by any `raw-window-handle` consumer such as wgpu or softbuffer.
    Native(Arc<dyn NativeHandles>),
    /// A platform-specific display object (a console's video output, a
    /// headless target...). Only the GPU backend written for that platform
    /// knows its concrete type and downcasts it.
    Platform(Arc<dyn Any + Send + Sync>),
}

impl Surface {
    /// The native handles, or `None` for platform-specific surfaces.
    pub fn native(&self) -> Option<Arc<dyn NativeHandles>> {
        match self {
            Self::Native(h) => Some(h.clone()),
            Self::Platform(_) => None,
        }
    }
}

mod input;
pub use input::*;

/// Main window configuration.
#[derive(Clone, Debug)]
pub struct WindowConfig {
    pub title: String,
    /// Initial size in logical points.
    pub width: u32,
    pub height: u32,
    pub min_width: u32,
    pub min_height: u32,
    pub resizable: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self { title: "Neko".into(), width: 1280, height: 800, min_width: 640, min_height: 400, resizable: true }
    }
}

/// A native window created by a platform backend.
///
/// `Send + Sync` because renderers may keep an `Arc` of it next to their
/// surface. Methods that touch the window should be called from the
/// event-loop thread.
pub trait Window: Send + Sync {
    /// The surface GPU backends draw to.
    fn surface(self: Arc<Self>) -> Surface;
    /// Drawable area size in physical pixels.
    fn inner_size(&self) -> (u32, u32);
    /// Physical pixels per logical point (DPI scale).
    fn scale_factor(&self) -> f32;
    fn set_title(&self, title: &str);
    /// Ask for a new [`App::frame`] as soon as possible.
    fn request_redraw(&self);
    /// Enable or disable text input (IME / on-screen keyboard).
    fn set_text_input(&self, enabled: bool);
    /// Switch between windowed and borderless fullscreen.
    fn set_fullscreen(&self, _fullscreen: bool) {}
    /// Vibrate gamepad `id`. Strengths go from 0.0 to 1.0. Returns false when
    /// there is no such pad, it cannot rumble, or no gamepad backend is active.
    fn rumble(&self, _id: u32, _weak: f32, _strong: f32, _duration_ms: u32) -> bool {
        false
    }
    /// Set the mouse cursor shape.
    fn set_cursor(&self, _cursor: CursorIcon) {}
    /// Copy text to the clipboard, if the backend supports it.
    fn set_clipboard(&self, _text: &str) {}
    /// Read text from the clipboard, if the backend supports it.
    fn clipboard(&self) -> Option<String> {
        None
    }
}

/// What the event loop should do after a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    /// Sleep until an event arrives.
    Wait,
    /// Sleep until an event arrives or the given instant is reached.
    WaitUntil(Instant),
    /// Redraw continuously (games).
    Poll,
    /// Stop the loop.
    Exit,
}

/// An application driven by a [`Platform`].
pub trait App {
    /// The window exists and can receive a render surface.
    /// On Android this can happen more than once (the surface is recreated).
    fn resumed(&mut self, window: Arc<dyn Window>);
    /// The native surface is about to go away (Android going to background).
    fn suspended(&mut self) {}
    fn event(&mut self, event: Event);
    /// Draw a frame and say when the next one is wanted.
    fn frame(&mut self) -> Flow;
    fn exiting(&mut self) {}

    // ---- extra windows (Godot's sub-windows when they are not embedded)

    /// The extra windows the app wants open right now, by its own ids.
    /// Polled after every event; ids that disappear are closed.
    fn subwindows(&mut self) -> Vec<(u64, WindowConfig)> {
        Vec::new()
    }
    /// An extra window was created.
    fn subwindow_opened(&mut self, _id: u64, _window: Arc<dyn Window>) {}
    /// An extra window was closed by the app (its id stopped being listed).
    fn subwindow_closed(&mut self, _id: u64) {}
    /// An event in an extra window. The main window redraws afterwards and
    /// [`App::frame`] draws every window.
    fn subwindow_event(&mut self, _id: u64, _event: Event) {}
}

/// A platform backend: owns the event loop and the window.
pub trait Platform {
    /// Short name, e.g. `"sdl3"`, `"winit"`.
    fn name(&self) -> &'static str;
    /// Create the window and run the loop until the [`App`] returns
    /// [`Flow::Exit`] or the user closes the window.
    fn run(self: Box<Self>, config: WindowConfig, app: Box<dyn App>) -> Result<(), PlatformError>;
    /// Whether [`App::subwindows`] are honored (else the app embeds them).
    fn supports_subwindows(&self) -> bool {
        false
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    #[error("backend library not found: {0}")]
    LibraryNotFound(String),
    #[error("failed to initialize the backend: {0}")]
    Init(String),
    #[error("failed to create the window: {0}")]
    CreateWindow(String),
    #[error("windowing system not supported by this backend: {0}")]
    Unsupported(String),
}
