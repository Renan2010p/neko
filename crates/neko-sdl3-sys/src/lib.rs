//! The slice of the SDL3 C API Neko uses, resolved with `libloading`.
//!
//! Loading at runtime means building Neko never needs SDL headers, pkg-config
//! or CMake: the backend is available whenever `libSDL3` is on the system.
//! Struct offsets follow SDL 3.2+ `SDL_events.h`.

#![allow(non_snake_case, non_camel_case_types)]

use std::ffi::{c_char, c_int, c_void};
use std::sync::{Arc, OnceLock};

use libloading::Library;

pub type SDL_Window = c_void;
pub type SDL_PropertiesID = u32;

pub const SDL_INIT_VIDEO: u32 = 0x20;
pub const SDL_INIT_EVENTS: u32 = 0x4000;
pub const SDL_INIT_JOYSTICK: u32 = 0x200;
pub const SDL_INIT_GAMEPAD: u32 = 0x2000;

pub type SDL_Gamepad = c_void;
pub type SDL_Joystick = c_void;

/// `SDL_GUID`, passed by value.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SDL_GUID {
    pub data: [u8; 16],
}

pub const SDL_GETEVENT: u32 = 2;

// Joystick and gamepad events (0x600..0x6FF).
pub const EV_JOYSTICK_FIRST: u32 = 0x600;
pub const EV_JOYSTICK_LAST: u32 = 0x6FF;
pub const EV_JOYSTICK_AXIS_MOTION: u32 = 0x600;
pub const EV_JOYSTICK_HAT_MOTION: u32 = 0x602;
pub const EV_JOYSTICK_BUTTON_DOWN: u32 = 0x603;
pub const EV_JOYSTICK_BUTTON_UP: u32 = 0x604;
pub const EV_JOYSTICK_ADDED: u32 = 0x605;
pub const EV_JOYSTICK_REMOVED: u32 = 0x606;
pub const EV_GAMEPAD_AXIS_MOTION: u32 = 0x650;
pub const EV_GAMEPAD_BUTTON_DOWN: u32 = 0x651;
pub const EV_GAMEPAD_BUTTON_UP: u32 = 0x652;

pub const SDL_HAT_UP: u8 = 1;
pub const SDL_HAT_RIGHT: u8 = 2;
pub const SDL_HAT_DOWN: u8 = 4;
pub const SDL_HAT_LEFT: u8 = 8;

pub const SDL_JOYSTICK_AXIS_MAX: i16 = 32767;
pub const SDL_JOYSTICK_AXIS_MIN: i16 = -32768;

pub const SDL_WINDOW_HIDDEN: u64 = 0x8;
pub const SDL_WINDOW_RESIZABLE: u64 = 0x20;
pub const SDL_WINDOW_HIGH_PIXEL_DENSITY: u64 = 0x2000;

pub const EV_QUIT: u32 = 0x100;
pub const EV_WINDOW_EXPOSED: u32 = 516;
pub const EV_WINDOW_RESIZED: u32 = 518;
pub const EV_WINDOW_PIXEL_SIZE_CHANGED: u32 = 519;
pub const EV_WINDOW_MOUSE_LEAVE: u32 = 525;
pub const EV_WINDOW_FOCUS_GAINED: u32 = 526;
pub const EV_WINDOW_FOCUS_LOST: u32 = 527;
pub const EV_WINDOW_CLOSE_REQUESTED: u32 = 528;
pub const EV_WINDOW_DISPLAY_SCALE_CHANGED: u32 = 532;
pub const EV_KEY_DOWN: u32 = 0x300;
pub const EV_KEY_UP: u32 = 0x301;
pub const EV_TEXT_INPUT: u32 = 0x303;
pub const EV_MOUSE_MOTION: u32 = 0x400;
pub const EV_MOUSE_BUTTON_DOWN: u32 = 0x401;
pub const EV_MOUSE_BUTTON_UP: u32 = 0x402;
pub const EV_MOUSE_WHEEL: u32 = 0x403;
pub const EV_DROP_FILE: u32 = 0x1000;

pub const KMOD_SHIFT: u16 = 0x0003;
pub const KMOD_CTRL: u16 = 0x00C0;
pub const KMOD_ALT: u16 = 0x0300;
pub const KMOD_GUI: u16 = 0x0C00;

pub const SDL_SYSTEM_CURSOR_DEFAULT: c_int = 0;
pub const SDL_SYSTEM_CURSOR_TEXT: c_int = 1;
pub const SDL_SYSTEM_CURSOR_CROSSHAIR: c_int = 3;
pub const SDL_SYSTEM_CURSOR_EW_RESIZE: c_int = 7;
pub const SDL_SYSTEM_CURSOR_NS_RESIZE: c_int = 8;
pub const SDL_SYSTEM_CURSOR_MOVE: c_int = 9;
pub const SDL_SYSTEM_CURSOR_NOT_ALLOWED: c_int = 10;
pub const SDL_SYSTEM_CURSOR_POINTER: c_int = 11;

/// `SDL_Event` is a 128-byte union; we read fields by offset.
#[repr(C, align(8))]
pub struct RawEvent(pub [u8; 128]);

impl RawEvent {
    pub fn zeroed() -> Self {
        Self([0; 128])
    }
    pub fn kind(&self) -> u32 {
        self.u32(0)
    }
    pub fn u32(&self, at: usize) -> u32 {
        u32::from_ne_bytes(self.0[at..at + 4].try_into().unwrap())
    }
    pub fn u16(&self, at: usize) -> u16 {
        u16::from_ne_bytes(self.0[at..at + 2].try_into().unwrap())
    }
    pub fn f32(&self, at: usize) -> f32 {
        f32::from_ne_bytes(self.0[at..at + 4].try_into().unwrap())
    }
    pub fn i16(&self, at: usize) -> i16 {
        i16::from_ne_bytes(self.0[at..at + 2].try_into().unwrap())
    }
    pub fn u8(&self, at: usize) -> u8 {
        self.0[at]
    }
    pub fn ptr(&self, at: usize) -> *const c_char {
        usize::from_ne_bytes(self.0[at..at + size_of::<usize>()].try_into().unwrap()) as *const c_char
    }
}

macro_rules! sdl_api {
    ($( fn $name:ident($($arg:ident: $ty:ty),*) $(-> $ret:ty)?; )*) => {
        pub struct Sdl {
            _lib: Library,
            $( pub $name: unsafe extern "C" fn($($ty),*) $(-> $ret)?, )*
        }

        impl Sdl {
            /// # Safety
            /// `lib` must be a real SDL 3.x library.
            unsafe fn from_library(lib: Library) -> Result<Self, String> {
                unsafe {
                    $(
                        let $name = *lib
                            .get::<unsafe extern "C" fn($($ty),*) $(-> $ret)?>(concat!(stringify!($name), "\0").as_bytes())
                            .map_err(|e| format!("missing symbol {}: {e}", stringify!($name)))?;
                    )*
                    Ok(Self { _lib: lib, $( $name, )* })
                }
            }
        }
    };
}

sdl_api! {
    fn SDL_Init(flags: u32) -> bool;
    fn SDL_Quit();
    fn SDL_InitSubSystem(flags: u32) -> bool;
    fn SDL_QuitSubSystem(flags: u32);
    fn SDL_SetHint(name: *const c_char, value: *const c_char) -> bool;
    fn SDL_PumpEvents();
    fn SDL_PeepEvents(events: *mut RawEvent, numevents: c_int, action: u32, min_type: u32, max_type: u32) -> c_int;
    fn SDL_FlushEvents(min_type: u32, max_type: u32);
    fn SDL_IsGamepad(id: u32) -> bool;
    fn SDL_OpenGamepad(id: u32) -> *mut SDL_Gamepad;
    fn SDL_CloseGamepad(gamepad: *mut SDL_Gamepad);
    fn SDL_GetGamepadName(gamepad: *mut SDL_Gamepad) -> *const c_char;
    fn SDL_GetGamepadJoystick(gamepad: *mut SDL_Gamepad) -> *mut SDL_Joystick;
    fn SDL_AddGamepadMappingsFromFile(file: *const c_char) -> c_int;
    fn SDL_OpenJoystick(id: u32) -> *mut SDL_Joystick;
    fn SDL_CloseJoystick(joystick: *mut SDL_Joystick);
    fn SDL_GetJoystickName(joystick: *mut SDL_Joystick) -> *const c_char;
    fn SDL_GetJoystickVendor(joystick: *mut SDL_Joystick) -> u16;
    fn SDL_GetJoystickProduct(joystick: *mut SDL_Joystick) -> u16;
    fn SDL_GetJoystickGUID(joystick: *mut SDL_Joystick) -> SDL_GUID;
    fn SDL_GUIDToString(guid: SDL_GUID, out: *mut c_char, cb: c_int);
    fn SDL_GetJoystickProperties(joystick: *mut SDL_Joystick) -> SDL_PropertiesID;
    fn SDL_GetBooleanProperty(props: SDL_PropertiesID, name: *const c_char, default: bool) -> bool;
    fn SDL_RumbleJoystick(joystick: *mut SDL_Joystick, low: u16, high: u16, duration_ms: u32) -> bool;
    fn SDL_GetError() -> *const c_char;
    fn SDL_GetVersion() -> c_int;
    fn SDL_GetCurrentVideoDriver() -> *const c_char;
    fn SDL_CreateWindow(title: *const c_char, w: c_int, h: c_int, flags: u64) -> *mut SDL_Window;
    fn SDL_DestroyWindow(window: *mut SDL_Window);
    fn SDL_ShowWindow(window: *mut SDL_Window) -> bool;
    fn SDL_SetWindowTitle(window: *mut SDL_Window, title: *const c_char) -> bool;
    fn SDL_SetWindowFullscreen(window: *mut SDL_Window, fullscreen: bool) -> bool;
    fn SDL_SetWindowMinimumSize(window: *mut SDL_Window, w: c_int, h: c_int) -> bool;
    fn SDL_GetWindowSizeInPixels(window: *mut SDL_Window, w: *mut c_int, h: *mut c_int) -> bool;
    fn SDL_GetWindowPixelDensity(window: *mut SDL_Window) -> f32;
    fn SDL_GetWindowDisplayScale(window: *mut SDL_Window) -> f32;
    fn SDL_GetWindowProperties(window: *mut SDL_Window) -> SDL_PropertiesID;
    fn SDL_GetPointerProperty(props: SDL_PropertiesID, name: *const c_char, default: *mut c_void) -> *mut c_void;
    fn SDL_GetNumberProperty(props: SDL_PropertiesID, name: *const c_char, default: i64) -> i64;
    fn SDL_StartTextInput(window: *mut SDL_Window) -> bool;
    fn SDL_StopTextInput(window: *mut SDL_Window) -> bool;
    fn SDL_PollEvent(event: *mut RawEvent) -> bool;
    fn SDL_WaitEventTimeout(event: *mut RawEvent, timeout_ms: i32) -> bool;
    fn SDL_CreateSystemCursor(id: c_int) -> *mut c_void;
    fn SDL_SetCursor(cursor: *mut c_void) -> bool;
    fn SDL_SetClipboardText(text: *const c_char) -> bool;
    fn SDL_GetClipboardText() -> *mut c_char;
    fn SDL_free(ptr: *mut c_void);
}

/// Library names tried in order. `NEKO_SDL3_LIB` overrides them.
const CANDIDATES: &[&str] = &[
    #[cfg(target_os = "windows")]
    "SDL3.dll",
    #[cfg(target_vendor = "apple")]
    "libSDL3.0.dylib",
    #[cfg(target_vendor = "apple")]
    "libSDL3.dylib",
    #[cfg(target_vendor = "apple")]
    "SDL3.framework/SDL3",
    #[cfg(not(any(target_os = "windows", target_vendor = "apple")))]
    "libSDL3.so.0",
    #[cfg(not(any(target_os = "windows", target_vendor = "apple")))]
    "libSDL3.so",
];

/// The process-wide SDL library, loaded once and shared by the window and
/// gamepad backends (like Godot, which keeps a single SDL instance).
pub fn shared() -> Result<Arc<Sdl>, String> {
    static SDL: OnceLock<Result<Arc<Sdl>, String>> = OnceLock::new();
    SDL.get_or_init(|| load().map(Arc::new)).clone()
}

/// Last SDL error message.
pub fn error(sdl: &Sdl) -> String {
    unsafe { std::ffi::CStr::from_ptr((sdl.SDL_GetError)()).to_string_lossy().into_owned() }
}

// SAFETY: SDL function pointers may be called from any thread; SDL documents
// which calls must stay on the main thread and the backends respect that.
unsafe impl Send for Sdl {}
unsafe impl Sync for Sdl {}

pub fn load() -> Result<Sdl, String> {
    let mut tried = Vec::new();
    let names: Vec<String> = match std::env::var("NEKO_SDL3_LIB") {
        Ok(p) => vec![p],
        Err(_) => CANDIDATES.iter().map(|s| s.to_string()).collect(),
    };
    for name in names {
        // SAFETY: loading SDL runs its library constructors, which are benign.
        match unsafe { Library::new(&name) } {
            Ok(lib) => return unsafe { Sdl::from_library(lib) },
            Err(e) => tried.push(format!("{name}: {e}")),
        }
    }
    Err(tried.join("; "))
}
