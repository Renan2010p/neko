//! Engine-agnostic input and window events.
//!
//! The raw platform key code is kept (`code`) because games compare against
//! raw values such as `13`, `'w'` and the arrow-key constants. `key` is a
//! convenience mapping on top.

/// A key the game reacts to. Platform backends map their native key codes
/// onto these names as a convenience; the raw code stays in `KeyEvent.code`.
pub const Key: type = enum {
    unknown,
    up,
    down,
    left,
    right,
    enter,
    escape,
    space,
    tab,
    backspace,
    // Letter keys.
    a,
    b,
    c,
    d,
    e,
    f,
    g,
    h,
    i,
    j,
    k,
    l,
    m,
    n,
    o,
    p,
    q,
    r,
    s,
    t,
    u,
    v,
    w,
    x,
    y,
    z,
    // Number-row keys.
    number_0,
    number_1,
    number_2,
    number_3,
    number_4,
    number_5,
    number_6,
    number_7,
    number_8,
    number_9,
};

/// A mouse button.
pub const MouseButton: type = enum {
    unknown,
    left,
    middle,
    right,
    x1,
    x2,
};

/// One input or window event, drained from the platform queue.
pub const Event: type = union(enum) {
    quit,
    key_down: KeyEvent,
    key_up: KeyEvent,
    mouse_motion: Motion,
    mouse_button_down: Button,
    mouse_button_up: Button,
    mouse_wheel: Wheel,

    /// A key press or release.
    pub const KeyEvent: type = struct {
        /// Raw platform key code (SDL keycode). The game compares this.
        code: i32,
        /// Convenience mapping of `code`.
        key: Key,
        /// Human-readable key name, e.g. "Up", "Return".
        name: []const u8,
        /// Scan-code name.
        scan_name: []const u8,
    };

    /// Absolute mouse position in logical pixels.
    pub const Motion: type = struct {
        x: i32,
        y: i32,
    };

    /// A mouse button press or release, with the cursor position.
    pub const Button: type = struct {
        button: MouseButton,
        x: i32,
        y: i32,
    };

    /// Wheel delta. `x` is horizontal, `y` is vertical.
    pub const Wheel: type = struct {
        x: f32,
        y: f32,
    };
};
