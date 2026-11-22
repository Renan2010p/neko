//! SDL3 platform backend.
//!
//! SDL3 is loaded at runtime (see [`ffi`]), so this crate builds everywhere
//! and only needs `libSDL3` when the backend is actually selected.
//! Override the library path with `NEKO_SDL3_LIB=/path/to/libSDL3.so`.

use std::ffi::{CStr, CString, c_int, c_void};
use std::num::NonZeroIsize;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use neko_gamepad_sdl3::Gamepads;
use neko_sdl3_sys::*;
use neko_platform::rwh::{self, HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use neko_platform::{
    App, CursorIcon, Event, Flow, Key, Modifiers, MouseButton, Platform, PlatformError, WheelUnit,
    Window, WindowConfig,
};

pub struct Sdl3Platform {
    sdl: Arc<Sdl>,
}

impl Sdl3Platform {
    /// Load `libSDL3`. Fails if the library is not installed.
    pub fn new() -> Result<Self, PlatformError> {
        let sdl = neko_sdl3_sys::shared().map_err(PlatformError::LibraryNotFound)?;
        Ok(Self { sdl })
    }

    /// SDL version as `(major, minor, patch)`.
    pub fn version(&self) -> (i32, i32, i32) {
        let v = unsafe { (self.sdl.SDL_GetVersion)() };
        (v / 1_000_000, (v / 1000) % 1000, v % 1000)
    }
}

fn sdl_error(sdl: &Sdl) -> String {
    unsafe { CStr::from_ptr((sdl.SDL_GetError)()).to_string_lossy().into_owned() }
}

/// Native handles captured when the window is created.
#[derive(Clone, Copy)]
enum Handles {
    Wayland { display: NonNull<c_void>, surface: NonNull<c_void> },
    Xlib { display: Option<NonNull<c_void>>, window: u64, screen: i32 },
    Win32 { hwnd: NonZeroIsize, hinstance: Option<NonZeroIsize> },
    Android { window: NonNull<c_void> },
}

pub struct SdlWindow {
    sdl: Arc<Sdl>,
    raw: *mut SDL_Window,
    handles: Handles,
    redraw: AtomicBool,
    gamepads: Option<Arc<Mutex<Gamepads>>>,
}

// SAFETY: the raw pointer is only dereferenced by SDL; window-mutating calls
// are documented (in `neko_platform::Window`) to happen on the loop thread.
unsafe impl Send for SdlWindow {}
unsafe impl Sync for SdlWindow {}

impl Drop for SdlWindow {
    fn drop(&mut self) {
        unsafe { (self.sdl.SDL_DestroyWindow)(self.raw) };
    }
}

impl SdlWindow {
    fn create(sdl: Arc<Sdl>, cfg: &WindowConfig, gamepads: Option<Arc<Mutex<Gamepads>>>) -> Result<Self, PlatformError> {
        let title = CString::new(cfg.title.as_str()).unwrap_or_default();
        let mut flags = SDL_WINDOW_HIGH_PIXEL_DENSITY | SDL_WINDOW_HIDDEN;
        if cfg.resizable {
            flags |= SDL_WINDOW_RESIZABLE;
        }
        let raw = unsafe { (sdl.SDL_CreateWindow)(title.as_ptr(), cfg.width as c_int, cfg.height as c_int, flags) };
        if raw.is_null() {
            return Err(PlatformError::CreateWindow(sdl_error(&sdl)));
        }
        unsafe { (sdl.SDL_SetWindowMinimumSize)(raw, cfg.min_width as c_int, cfg.min_height as c_int) };
        let handles = match unsafe { native_handles(&sdl, raw) } {
            Ok(h) => h,
            Err(e) => {
                unsafe { (sdl.SDL_DestroyWindow)(raw) };
                return Err(e);
            }
        };
        unsafe {
            (sdl.SDL_ShowWindow)(raw);
            (sdl.SDL_StartTextInput)(raw);
        }
        Ok(Self { sdl, raw, handles, redraw: AtomicBool::new(true), gamepads })
    }

    fn pixel_density(&self) -> f32 {
        let d = unsafe { (self.sdl.SDL_GetWindowPixelDensity)(self.raw) };
        if d > 0.0 { d } else { 1.0 }
    }
}

unsafe fn native_handles(sdl: &Sdl, win: *mut SDL_Window) -> Result<Handles, PlatformError> {
    unsafe {
        let driver = CStr::from_ptr((sdl.SDL_GetCurrentVideoDriver)()).to_string_lossy().into_owned();
        let props = (sdl.SDL_GetWindowProperties)(win);
        let ptr = |name: &CStr| NonNull::new((sdl.SDL_GetPointerProperty)(props, name.as_ptr(), std::ptr::null_mut()));
        let num = |name: &CStr| (sdl.SDL_GetNumberProperty)(props, name.as_ptr(), 0);
        let missing = || PlatformError::Unsupported(format!("SDL video driver '{driver}' did not expose native handles"));
        match driver.as_str() {
            "wayland" => Ok(Handles::Wayland {
                display: ptr(c"SDL.window.wayland.display").ok_or_else(missing)?,
                surface: ptr(c"SDL.window.wayland.surface").ok_or_else(missing)?,
            }),
            "x11" => Ok(Handles::Xlib {
                display: ptr(c"SDL.window.x11.display"),
                window: num(c"SDL.window.x11.window") as u64,
                screen: num(c"SDL.window.x11.screen") as i32,
            }),
            "windows" => Ok(Handles::Win32 {
                hwnd: ptr(c"SDL.window.win32.hwnd").and_then(|p| NonZeroIsize::new(p.as_ptr() as isize)).ok_or_else(missing)?,
                hinstance: ptr(c"SDL.window.win32.instance").and_then(|p| NonZeroIsize::new(p.as_ptr() as isize)),
            }),
            "android" => Ok(Handles::Android { window: ptr(c"SDL.window.android.window").ok_or_else(missing)? }),
            // TODO(macos/ios): create an NSView/UIView via SDL_Metal_CreateView.
            other => Err(PlatformError::Unsupported(format!("SDL video driver '{other}'"))),
        }
    }
}

impl HasWindowHandle for SdlWindow {
    fn window_handle(&self) -> Result<rwh::WindowHandle<'_>, rwh::HandleError> {
        let raw = match self.handles {
            Handles::Wayland { surface, .. } => RawWindowHandle::Wayland(rwh::WaylandWindowHandle::new(surface)),
            Handles::Xlib { window, .. } => RawWindowHandle::Xlib(rwh::XlibWindowHandle::new(window as _)),
            Handles::Win32 { hwnd, hinstance } => {
                let mut h = rwh::Win32WindowHandle::new(hwnd);
                h.hinstance = hinstance;
                RawWindowHandle::Win32(h)
            }
            Handles::Android { window } => RawWindowHandle::AndroidNdk(rwh::AndroidNdkWindowHandle::new(window)),
        };
        // SAFETY: the handle lives as long as `self` (destroyed in Drop).
        Ok(unsafe { rwh::WindowHandle::borrow_raw(raw) })
    }
}

impl HasDisplayHandle for SdlWindow {
    fn display_handle(&self) -> Result<rwh::DisplayHandle<'_>, rwh::HandleError> {
        let raw = match self.handles {
            Handles::Wayland { display, .. } => RawDisplayHandle::Wayland(rwh::WaylandDisplayHandle::new(display)),
            Handles::Xlib { display, screen, .. } => RawDisplayHandle::Xlib(rwh::XlibDisplayHandle::new(display, screen)),
            Handles::Win32 { .. } => RawDisplayHandle::Windows(rwh::WindowsDisplayHandle::new()),
            Handles::Android { .. } => RawDisplayHandle::Android(rwh::AndroidDisplayHandle::new()),
        };
        Ok(unsafe { rwh::DisplayHandle::borrow_raw(raw) })
    }
}

impl Window for SdlWindow {
    fn surface(self: Arc<Self>) -> neko_platform::Surface {
        neko_platform::Surface::Native(self)
    }
    fn inner_size(&self) -> (u32, u32) {
        let (mut w, mut h) = (0, 0);
        unsafe { (self.sdl.SDL_GetWindowSizeInPixels)(self.raw, &mut w, &mut h) };
        (w.max(1) as u32, h.max(1) as u32)
    }
    fn scale_factor(&self) -> f32 {
        let s = unsafe { (self.sdl.SDL_GetWindowDisplayScale)(self.raw) };
        if s > 0.0 { s } else { 1.0 }
    }
    fn set_title(&self, title: &str) {
        if let Ok(t) = CString::new(title) {
            unsafe { (self.sdl.SDL_SetWindowTitle)(self.raw, t.as_ptr()) };
        }
    }
    fn request_redraw(&self) {
        self.redraw.store(true, Ordering::Release);
    }
    fn rumble(&self, id: u32, weak: f32, strong: f32, duration_ms: u32) -> bool {
        self.gamepads.as_ref().is_some_and(|g| g.lock().is_ok_and(|g| g.rumble(id, weak, strong, duration_ms)))
    }
    fn set_fullscreen(&self, fullscreen: bool) {
        unsafe { (self.sdl.SDL_SetWindowFullscreen)(self.raw, fullscreen) };
    }
    fn set_text_input(&self, enabled: bool) {
        unsafe {
            if enabled {
                (self.sdl.SDL_StartTextInput)(self.raw);
            } else {
                (self.sdl.SDL_StopTextInput)(self.raw);
            }
        }
    }
    fn set_cursor(&self, cursor: CursorIcon) {
        let id = match cursor {
            CursorIcon::Default => SDL_SYSTEM_CURSOR_DEFAULT,
            CursorIcon::Pointer => SDL_SYSTEM_CURSOR_POINTER,
            CursorIcon::Text => SDL_SYSTEM_CURSOR_TEXT,
            CursorIcon::Move | CursorIcon::Grab | CursorIcon::Grabbing => SDL_SYSTEM_CURSOR_MOVE,
            CursorIcon::NotAllowed => SDL_SYSTEM_CURSOR_NOT_ALLOWED,
            CursorIcon::ResizeHorizontal => SDL_SYSTEM_CURSOR_EW_RESIZE,
            CursorIcon::ResizeVertical => SDL_SYSTEM_CURSOR_NS_RESIZE,
            CursorIcon::Crosshair => SDL_SYSTEM_CURSOR_CROSSHAIR,
        };
        // SDL caches nothing here; system cursors are cheap and few, so we leak them.
        unsafe {
            let c = (self.sdl.SDL_CreateSystemCursor)(id);
            if !c.is_null() {
                (self.sdl.SDL_SetCursor)(c);
            }
        }
    }
    fn set_clipboard(&self, text: &str) {
        if let Ok(t) = CString::new(text) {
            unsafe { (self.sdl.SDL_SetClipboardText)(t.as_ptr()) };
        }
    }
    fn clipboard(&self) -> Option<String> {
        unsafe {
            let p = (self.sdl.SDL_GetClipboardText)();
            if p.is_null() {
                return None;
            }
            let s = CStr::from_ptr(p).to_string_lossy().into_owned();
            (self.sdl.SDL_free)(p.cast());
            Some(s)
        }
    }
}

impl Platform for Sdl3Platform {
    fn name(&self) -> &'static str {
        "sdl3"
    }

    fn run(self: Box<Self>, config: WindowConfig, mut app: Box<dyn App>) -> Result<(), PlatformError> {
        let sdl = self.sdl.clone();
        if !unsafe { (sdl.SDL_Init)(SDL_INIT_VIDEO | SDL_INIT_EVENTS) } {
            return Err(PlatformError::Init(sdl_error(&sdl)));
        }
        let result = (|| {
            let gamepads = match Gamepads::new(sdl.clone(), false) {
                Ok(g) => Some(Arc::new(Mutex::new(g))),
                Err(e) => {
                    log::warn!("gamepads unavailable: {e}");
                    None
                }
            };
            let window = Arc::new(SdlWindow::create(sdl.clone(), &config, gamepads.clone())?);
            let driver = unsafe { CStr::from_ptr((sdl.SDL_GetCurrentVideoDriver)()).to_string_lossy().into_owned() };
            log::info!("SDL3 {:?} on video driver '{driver}'", self.version());
            app.resumed(window.clone());
            run_loop(&sdl, &window, app.as_mut());
            app.exiting();
            drop(app);
            drop(window);
            drop(gamepads);
            Ok(())
        })();
        unsafe { (sdl.SDL_Quit)() };
        result
    }
}

fn run_loop(sdl: &Sdl, window: &SdlWindow, app: &mut dyn App) {
    let mut flow = Flow::Wait;
    let mut ev = RawEvent::zeroed();
    let mut mods = Modifiers::default();
    loop {
        let redraw_pending = window.redraw.load(Ordering::Acquire) || flow == Flow::Poll;
        let timeout_ms: i32 = if redraw_pending {
            0
        } else {
            match flow {
                Flow::WaitUntil(t) => t.saturating_duration_since(Instant::now()).as_millis().min(i32::MAX as u128) as i32,
                _ => -1,
            }
        };
        let mut got = if timeout_ms == 0 {
            unsafe { (sdl.SDL_PollEvent)(&mut ev) }
        } else {
            unsafe { (sdl.SDL_WaitEventTimeout)(&mut ev, timeout_ms) }
        };
        while got {
            if let Some(g) = &window.gamepads {
                let mut pad_events = Vec::new();
                let handled = g.lock().map(|mut g| g.handle(&ev, &mut pad_events)).unwrap_or(false);
                if handled {
                    for e in pad_events {
                        app.event(e);
                    }
                    window.request_redraw();
                    got = unsafe { (sdl.SDL_PollEvent)(&mut ev) };
                    continue;
                }
            }
            match translate(&ev, window, &mut mods) {
                Translated::Quit => return,
                Translated::Events(list) => {
                    for e in list {
                        app.event(e);
                    }
                    window.request_redraw();
                }
                Translated::Redraw => window.request_redraw(),
                Translated::None => {}
            }
            got = unsafe { (sdl.SDL_PollEvent)(&mut ev) };
        }
        if let Flow::WaitUntil(t) = flow
            && Instant::now() >= t
        {
            window.request_redraw();
        }
        if window.redraw.swap(false, Ordering::AcqRel) || flow == Flow::Poll {
            flow = app.frame();
            if flow == Flow::Exit {
                return;
            }
        }
    }
}

enum Translated {
    None,
    Quit,
    Redraw,
    Events(Vec<Event>),
}

fn translate(ev: &RawEvent, window: &SdlWindow, mods: &mut Modifiers) -> Translated {
    let density = window.pixel_density();
    let one = |e| Translated::Events(vec![e]);
    match ev.kind() {
        EV_QUIT => Translated::Quit,
        EV_WINDOW_CLOSE_REQUESTED => one(Event::CloseRequested),
        EV_WINDOW_RESIZED | EV_WINDOW_PIXEL_SIZE_CHANGED => {
            let (w, h) = window.inner_size();
            one(Event::Resized { width: w, height: h })
        }
        EV_WINDOW_DISPLAY_SCALE_CHANGED => one(Event::ScaleFactorChanged(window.scale_factor())),
        EV_WINDOW_FOCUS_GAINED => one(Event::Focused(true)),
        EV_WINDOW_FOCUS_LOST => one(Event::Focused(false)),
        EV_WINDOW_MOUSE_LEAVE => one(Event::MouseLeft),
        EV_WINDOW_EXPOSED => Translated::Redraw,
        EV_MOUSE_MOTION => one(Event::MouseMoved { x: ev.f32(28) * density, y: ev.f32(32) * density }),
        EV_MOUSE_BUTTON_DOWN | EV_MOUSE_BUTTON_UP => {
            let button = match ev.u8(24) {
                1 => MouseButton::Left,
                2 => MouseButton::Middle,
                3 => MouseButton::Right,
                4 => MouseButton::Back,
                5 => MouseButton::Forward,
                n => MouseButton::Other(n as u16),
            };
            Translated::Events(vec![
                Event::MouseMoved { x: ev.f32(28) * density, y: ev.f32(32) * density },
                Event::MouseButton { button, pressed: ev.u8(25) != 0 },
            ])
        }
        EV_MOUSE_WHEEL => {
            let flip = if ev.u32(32) == 1 { -1.0 } else { 1.0 };
            one(Event::MouseWheel { delta_x: ev.f32(24) * flip, delta_y: ev.f32(28) * flip, unit: WheelUnit::Lines })
        }
        EV_KEY_DOWN | EV_KEY_UP => {
            let m = ev.u16(32);
            let new = Modifiers {
                shift: m & KMOD_SHIFT != 0,
                ctrl: m & KMOD_CTRL != 0,
                alt: m & KMOD_ALT != 0,
                logo: m & KMOD_GUI != 0,
            };
            let mut out = Vec::with_capacity(2);
            if new != *mods {
                *mods = new;
                out.push(Event::ModifiersChanged(new));
            }
            out.push(Event::Key { key: map_key(ev.u32(28)), pressed: ev.u8(36) != 0, repeat: ev.u8(37) != 0, modifiers: new });
            Translated::Events(out)
        }
        EV_TEXT_INPUT => {
            let align = align_of::<usize>();
            let at = (20 + align - 1) & !(align - 1);
            let p = ev.ptr(at);
            if p.is_null() {
                return Translated::None;
            }
            let text = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
            if text.is_empty() { Translated::None } else { one(Event::Text(text)) }
        }
        EV_DROP_FILE => {
            // SDL_DropEvent: type/reserved/timestamp, windowID, float x, float y,
            // then `source` and `data` pointers (8-aligned on 64-bit).
            let source_at = if align_of::<usize>() == 8 { 32 } else { 28 };
            let p = ev.ptr(source_at + size_of::<usize>());
            if p.is_null() {
                return Translated::None;
            }
            let path = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
            if path.is_empty() { Translated::None } else { one(Event::DroppedFiles(vec![path.into()])) }
        }
        _ => Translated::None,
    }
}

fn map_key(k: u32) -> Key {
    match k {
        0x0D => Key::Enter,
        0x1B => Key::Escape,
        0x08 => Key::Backspace,
        0x09 => Key::Tab,
        0x20 => Key::Space,
        0x7F => Key::Delete,
        0x4000_0049 => Key::Insert,
        0x4000_004A => Key::Home,
        0x4000_004B => Key::PageUp,
        0x4000_004D => Key::End,
        0x4000_004E => Key::PageDown,
        0x4000_004F => Key::ArrowRight,
        0x4000_0050 => Key::ArrowLeft,
        0x4000_0051 => Key::ArrowDown,
        0x4000_0052 => Key::ArrowUp,
        f @ 0x4000_003A..=0x4000_0045 => Key::F((f - 0x4000_003A + 1) as u8),
        c if c < 0x4000_0000 => char::from_u32(c).map(|c| Key::Char(c.to_ascii_lowercase())).unwrap_or(Key::Unknown),
        _ => Key::Unknown,
    }
}

