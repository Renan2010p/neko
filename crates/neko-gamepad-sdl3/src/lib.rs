//! Gamepad input through SDL3.
//!
//! Godot (4.5+) uses SDL3 only for joypads while drawing windows with its own
//! display servers; Neko does the same. The logic mirrors Godot's
//! `drivers/sdl/joypad_sdl.cpp` (MIT):
//!
//! - init only `SDL_INIT_JOYSTICK | SDL_INIT_GAMEPAD`, with the joystick thread
//!   and no signal handlers;
//! - devices SDL recognizes are opened as gamepads (remapped buttons/axes),
//!   others as raw joysticks;
//! - sticks are normalized to -1..1, triggers to 0..1;
//! - each device gets the smallest free id, so player 1 stays id 0.
//!
//! Two ways to drive it:
//! - [`Gamepads::poll`] when another backend owns the window (winit): it only
//!   takes joystick/gamepad events from SDL's queue;
//! - [`Gamepads::handle`] from the SDL3 window backend's own event loop.

use std::collections::BTreeMap;
use std::ffi::{CStr, CString, c_char};
use std::sync::Arc;

use neko_platform::{Event, GamepadAxis, GamepadButton, GamepadEvent, GamepadInfo};
use neko_sdl3_sys::*;

struct Pad {
    instance: u32,
    gamepad: *mut SDL_Gamepad,
    joystick: *mut SDL_Joystick,
    info: GamepadInfo,
    /// Last hat value of raw joysticks, to turn hat changes into D-pad presses.
    hat: u8,
}

pub struct Gamepads {
    sdl: Arc<Sdl>,
    /// Keyed by Neko id (0, 1, 2...).
    pads: BTreeMap<u32, Pad>,
    standalone: bool,
}

// SAFETY: SDL's joystick API locks internally; pads are only touched through `&mut self`
// or behind the caller's mutex.
unsafe impl Send for Gamepads {}

fn cstr(p: *const c_char) -> String {
    if p.is_null() { String::new() } else { unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned() }
}

/// Godot's axis formula: SDL range to -1..1.
fn stick(v: i16) -> f32 {
    ((v as f32 - SDL_JOYSTICK_AXIS_MIN as f32) / (SDL_JOYSTICK_AXIS_MAX as f32 - SDL_JOYSTICK_AXIS_MIN as f32) - 0.5) * 2.0
}

impl Gamepads {
    /// Start the gamepad subsystem. `standalone` is true when SDL does not own
    /// the window (then input keeps flowing even though SDL sees no focused window).
    pub fn new(sdl: Arc<Sdl>, standalone: bool) -> Result<Self, String> {
        unsafe {
            (sdl.SDL_SetHint)(c"SDL_JOYSTICK_THREAD".as_ptr(), c"1".as_ptr());
            (sdl.SDL_SetHint)(c"SDL_NO_SIGNAL_HANDLERS".as_ptr(), c"1".as_ptr());
            if standalone {
                (sdl.SDL_SetHint)(c"SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS".as_ptr(), c"1".as_ptr());
            }
            if !(sdl.SDL_InitSubSystem)(SDL_INIT_JOYSTICK | SDL_INIT_GAMEPAD) {
                return Err(error(&sdl));
            }
        }
        // Like Godot's built-in database: extra mappings from the user's data folder.
        if let Some(file) = dirs::data_dir().map(|d| d.join("neko").join("gamecontrollerdb.txt"))
            && file.is_file()
            && let Ok(c) = CString::new(file.display().to_string())
        {
            let n = unsafe { (sdl.SDL_AddGamepadMappingsFromFile)(c.as_ptr()) };
            log::info!("loaded {n} gamepad mappings from {}", file.display());
        }
        Ok(Self { sdl, pads: BTreeMap::new(), standalone })
    }

    /// Load SDL and start gamepads, for backends that do not use SDL otherwise.
    pub fn standalone() -> Result<Self, String> {
        Self::new(shared()?, true)
    }

    pub fn is_empty(&self) -> bool {
        self.pads.is_empty()
    }

    /// Connected pads by id.
    pub fn connected(&self) -> Vec<(u32, GamepadInfo)> {
        self.pads.iter().map(|(&id, p)| (id, p.info.clone())).collect()
    }

    /// Pump SDL and translate pending joystick/gamepad events (standalone mode).
    pub fn poll(&mut self, out: &mut Vec<Event>) {
        let mut ev = RawEvent::zeroed();
        unsafe { (self.sdl.SDL_PumpEvents)() };
        while unsafe { (self.sdl.SDL_PeepEvents)(&mut ev, 1, SDL_GETEVENT, EV_JOYSTICK_FIRST, EV_JOYSTICK_LAST) } > 0 {
            self.handle(&ev, out);
        }
        if self.standalone {
            // Nothing else reads SDL's queue in this mode; drop the rest so it cannot grow.
            unsafe {
                (self.sdl.SDL_FlushEvents)(0, EV_JOYSTICK_FIRST - 1);
                (self.sdl.SDL_FlushEvents)(EV_JOYSTICK_LAST + 1, u32::MAX);
            }
        }
    }

    fn id_of(&self, instance: u32) -> Option<u32> {
        self.pads.iter().find(|(_, p)| p.instance == instance).map(|(&id, _)| id)
    }

    /// Translate one SDL event. Returns true if it was a joystick/gamepad event.
    pub fn handle(&mut self, ev: &RawEvent, out: &mut Vec<Event>) -> bool {
        let kind = ev.kind();
        if !(EV_JOYSTICK_FIRST..=EV_JOYSTICK_LAST).contains(&kind) {
            return false;
        }
        let which = ev.u32(16);
        let sdl = self.sdl.clone();
        let is_gamepad = unsafe { (sdl.SDL_IsGamepad)(which) };
        let mut push = |id: u32, event: GamepadEvent| out.push(Event::Gamepad { id, event });
        match kind {
            EV_JOYSTICK_ADDED => {
                if self.id_of(which).is_some() {
                    return true;
                }
                let (gamepad, joystick, name) = unsafe {
                    if is_gamepad {
                        let g = (sdl.SDL_OpenGamepad)(which);
                        if g.is_null() {
                            log::warn!("could not open gamepad {which}: {}", error(&sdl));
                            return true;
                        }
                        (g, (sdl.SDL_GetGamepadJoystick)(g), cstr((sdl.SDL_GetGamepadName)(g)))
                    } else {
                        let j = (sdl.SDL_OpenJoystick)(which);
                        if j.is_null() {
                            log::warn!("could not open joystick {which}: {}", error(&sdl));
                            return true;
                        }
                        (std::ptr::null_mut(), j, cstr((sdl.SDL_GetJoystickName)(j)))
                    }
                };
                let mut guid = [0 as c_char; 64];
                let info = unsafe {
                    (sdl.SDL_GUIDToString)((sdl.SDL_GetJoystickGUID)(joystick), guid.as_mut_ptr(), guid.len() as i32);
                    GamepadInfo {
                        name: if name.is_empty() { "Unknown controller".into() } else { name },
                        guid: cstr(guid.as_ptr()),
                        vendor_id: (sdl.SDL_GetJoystickVendor)(joystick),
                        product_id: (sdl.SDL_GetJoystickProduct)(joystick),
                        is_gamepad,
                        can_rumble: (sdl.SDL_GetBooleanProperty)((sdl.SDL_GetJoystickProperties)(joystick), c"SDL.joystick.cap.rumble".as_ptr(), false),
                    }
                };
                let id = (0..).find(|i| !self.pads.contains_key(i)).expect("free id");
                log::info!("gamepad {id} connected: {} ({})", info.name, info.guid);
                self.pads.insert(id, Pad { instance: which, gamepad, joystick, info: info.clone(), hat: 0 });
                push(id, GamepadEvent::Connected(info));
            }
            EV_JOYSTICK_REMOVED => {
                if let Some(id) = self.id_of(which) {
                    self.close(id);
                    push(id, GamepadEvent::Disconnected);
                }
            }
            _ => {
                let Some(id) = self.id_of(which) else { return true };
                // Gamepads send both raw joystick and remapped gamepad events; keep the remapped ones.
                let raw = matches!(kind, EV_JOYSTICK_AXIS_MOTION | EV_JOYSTICK_BUTTON_DOWN | EV_JOYSTICK_BUTTON_UP | EV_JOYSTICK_HAT_MOTION);
                if raw && is_gamepad {
                    return true;
                }
                match kind {
                    EV_GAMEPAD_AXIS_MOTION => {
                        let axis = GamepadAxis::from_sdl(ev.u8(20));
                        let v = ev.i16(24);
                        let value = if axis.is_trigger() { v as f32 / SDL_JOYSTICK_AXIS_MAX as f32 } else { stick(v) };
                        push(id, GamepadEvent::Axis { axis, value: value.clamp(-1.0, 1.0) });
                    }
                    EV_GAMEPAD_BUTTON_DOWN | EV_GAMEPAD_BUTTON_UP => {
                        push(id, GamepadEvent::Button { button: GamepadButton::from_sdl(ev.u8(20)), pressed: ev.u8(21) != 0 });
                    }
                    EV_JOYSTICK_AXIS_MOTION => {
                        push(id, GamepadEvent::Axis { axis: GamepadAxis::from_sdl(ev.u8(20)), value: stick(ev.i16(24)).clamp(-1.0, 1.0) });
                    }
                    EV_JOYSTICK_BUTTON_DOWN | EV_JOYSTICK_BUTTON_UP => {
                        // Raw joysticks have no standard layout; report their buttons as Other(n).
                        push(id, GamepadEvent::Button { button: GamepadButton::Other(ev.u8(20)), pressed: ev.u8(21) != 0 });
                    }
                    EV_JOYSTICK_HAT_MOTION => {
                        let value = ev.u8(21);
                        let pad = self.pads.get_mut(&id).expect("id exists");
                        let old = std::mem::replace(&mut pad.hat, value);
                        for (mask, button) in [
                            (SDL_HAT_UP, GamepadButton::DpadUp),
                            (SDL_HAT_RIGHT, GamepadButton::DpadRight),
                            (SDL_HAT_DOWN, GamepadButton::DpadDown),
                            (SDL_HAT_LEFT, GamepadButton::DpadLeft),
                        ] {
                            if (old ^ value) & mask != 0 {
                                push(id, GamepadEvent::Button { button, pressed: value & mask != 0 });
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        true
    }

    /// Vibrate a pad. Strengths 0..1; zero strength or duration stops it.
    pub fn rumble(&self, id: u32, weak: f32, strong: f32, duration_ms: u32) -> bool {
        let Some(p) = self.pads.get(&id) else { return false };
        if !p.info.can_rumble {
            return false;
        }
        let to16 = |v: f32| (v.clamp(0.0, 1.0) * u16::MAX as f32) as u16;
        // SDL calls the weak motor "high frequency"; Godot passes (weak, strong) the same way.
        unsafe { (self.sdl.SDL_RumbleJoystick)(p.joystick, to16(strong), to16(weak), duration_ms) }
    }

    fn close(&mut self, id: u32) {
        if let Some(p) = self.pads.remove(&id) {
            unsafe {
                if p.gamepad.is_null() {
                    (self.sdl.SDL_CloseJoystick)(p.joystick);
                } else {
                    (self.sdl.SDL_CloseGamepad)(p.gamepad);
                }
            }
        }
    }
}

impl Drop for Gamepads {
    fn drop(&mut self) {
        let ids: Vec<u32> = self.pads.keys().copied().collect();
        for id in ids {
            self.close(id);
        }
        unsafe { (self.sdl.SDL_QuitSubSystem)(SDL_INIT_JOYSTICK | SDL_INIT_GAMEPAD) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axis_normalization_matches_godot() {
        assert_eq!(stick(SDL_JOYSTICK_AXIS_MIN), -1.0);
        assert_eq!(stick(SDL_JOYSTICK_AXIS_MAX), 1.0);
        assert!(stick(0).abs() < 1e-4);
        assert_eq!(GamepadButton::from_sdl(0), GamepadButton::South);
        assert_eq!(GamepadButton::from_sdl(14), GamepadButton::DpadRight);
        assert_eq!(GamepadButton::from_sdl(40), GamepadButton::Other(40));
        assert_eq!(GamepadAxis::from_sdl(5), GamepadAxis::RightTrigger);
    }
}
