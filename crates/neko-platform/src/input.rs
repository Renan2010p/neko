//! Input types normalized across backends.

use std::path::PathBuf;

/// Input or window event. Positions are in physical pixels.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    CloseRequested,
    Resized { width: u32, height: u32 },
    ScaleFactorChanged(f32),
    Focused(bool),
    MouseMoved { x: f32, y: f32 },
    MouseLeft,
    MouseButton { button: MouseButton, pressed: bool },
    MouseWheel { delta_x: f32, delta_y: f32, unit: WheelUnit },
    Key { key: Key, pressed: bool, repeat: bool, modifiers: Modifiers },
    ModifiersChanged(Modifiers),
    /// Typed text (dead keys / IME already resolved).
    Text(String),
    /// Files or folders dropped from the operating system onto the window
    /// (Godot's `files_dropped`). The app uses the last mouse position to
    /// pick the target folder in the FileSystem dock.
    DroppedFiles(Vec<PathBuf>),
    /// A gamepad or joystick event. `id` is a small stable index (0 = first pad),
    /// reused after a pad is unplugged, like Godot's device ids.
    Gamepad { id: u32, event: GamepadEvent },
}

#[derive(Clone, Debug, PartialEq)]
pub enum GamepadEvent {
    Connected(GamepadInfo),
    Disconnected,
    Button { button: GamepadButton, pressed: bool },
    /// Sticks go from -1.0 to 1.0 (down/right positive); triggers from 0.0 to 1.0.
    Axis { axis: GamepadAxis, value: f32 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GamepadInfo {
    pub name: String,
    /// SDL-style GUID, used by mapping databases.
    pub guid: String,
    pub vendor_id: u16,
    pub product_id: u16,
    /// Has a known button layout (not a raw joystick).
    pub is_gamepad: bool,
    pub can_rumble: bool,
}

/// Gamepad buttons in SDL3 order (Godot's JoyButton uses the same numbers).
/// Face buttons are named by position: South is A on Xbox, Cross on PlayStation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GamepadButton {
    South,
    East,
    West,
    North,
    Back,
    Guide,
    Start,
    LeftStick,
    RightStick,
    LeftShoulder,
    RightShoulder,
    DpadUp,
    DpadDown,
    DpadLeft,
    DpadRight,
    Misc1,
    RightPaddle1,
    LeftPaddle1,
    RightPaddle2,
    LeftPaddle2,
    Touchpad,
    /// Raw joystick buttons beyond the standard layout.
    Other(u8),
}

impl GamepadButton {
    pub fn from_sdl(b: u8) -> Self {
        use GamepadButton::*;
        const ORDER: [GamepadButton; 21] = [
            South, East, West, North, Back, Guide, Start, LeftStick, RightStick, LeftShoulder, RightShoulder, DpadUp, DpadDown,
            DpadLeft, DpadRight, Misc1, RightPaddle1, LeftPaddle1, RightPaddle2, LeftPaddle2, Touchpad,
        ];
        ORDER.get(b as usize).copied().unwrap_or(Other(b))
    }
}

/// Gamepad axes in SDL3 order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GamepadAxis {
    LeftX,
    LeftY,
    RightX,
    RightY,
    LeftTrigger,
    RightTrigger,
    Other(u8),
}

impl GamepadAxis {
    pub fn from_sdl(a: u8) -> Self {
        use GamepadAxis::*;
        [LeftX, LeftY, RightX, RightY, LeftTrigger, RightTrigger].get(a as usize).copied().unwrap_or(Other(a))
    }

    pub fn is_trigger(self) -> bool {
        matches!(self, Self::LeftTrigger | Self::RightTrigger)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
    Other(u16),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WheelUnit {
    /// Scroll in lines (notched mouse wheel).
    Lines,
    /// Scroll in physical pixels (touchpad).
    Pixels,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    /// Super / Windows / Command key.
    pub logo: bool,
}

/// Logical key (depends on the keyboard layout).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    /// Letter, digit or symbol; letters are always lowercase.
    Char(char),
    Escape,
    Enter,
    Tab,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Space,
    F(u8),
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CursorIcon {
    #[default]
    Default,
    Pointer,
    Text,
    Move,
    Grab,
    Grabbing,
    NotAllowed,
    ResizeHorizontal,
    ResizeVertical,
    Crosshair,
}
