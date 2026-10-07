//! `neko.input` — event queue and live input state.
//!
//! Two ways to read input, pick whichever suits you:
//!
//! **State queries** (easiest). `neko.run` starts a frame for you; then just
//! ask:
//!
//!     if (neko.input.key(.left))       move(-1); // held
//!     if (neko.input.keyDown(.space))  jump();   // pressed this frame
//!     if (neko.input.mousePressed(.left)) shoot();
//!     const p = neko.input.mouse();
//!
//! **Raw events** (`poll_event`) for text, window events or custom handling:
//!
//!     while (neko.input.poll_event()) |ev| { ... }
//!
//! The two work together: `beginFrame` drains the platform queue once and
//! caches it, so `poll_event` replays the same events during the frame. In a
//! hand-written loop call `beginFrame()` at the top of the frame and
//! `endFrame()` at the bottom; `neko.run` / `neko.app` already do.

const std: type = @import("std");
const types: type = @import("../types.zig");
const context: type = @import("../context.zig");
const backend: type = @import("../backend.zig");
const event: type = @import("event.zig");
const lifecycle: type = @import("lifecycle.zig");

pub const Event: type = event.Event;
pub const Key: type = event.Key;
pub const MouseButton: type = event.MouseButton;

const KeySet: type = std.EnumSet(Key);
const ButtonSet: type = std.EnumSet(MouseButton);

/// Events cached for the current frame. If a frame has more, the extra events
/// stay in the platform queue and are drained next frame.
const MAX_FRAME_EVENTS: usize = 64;

var frame_events: [MAX_FRAME_EVENTS]Event = undefined;
var frame_count: usize = 0;
var frame_index: usize = 0;
var in_frame: bool = false;

var keys_down: KeySet = KeySet.initEmpty();
var keys_pressed: KeySet = KeySet.initEmpty();
var keys_released: KeySet = KeySet.initEmpty();

var buttons_down: ButtonSet = ButtonSet.initEmpty();
var buttons_pressed: ButtonSet = ButtonSet.initEmpty();
var buttons_released: ButtonSet = ButtonSet.initEmpty();

var mouse_pos_value: types.Point = .{};
var wheel_value: f32 = 0;

// ── Frame lifecycle ──────────────────────────────────────────────────────────

/// Starts a frame: drains every pending event once, updates the key/mouse
/// state, caches the events for `poll_event`, and requests a stop on `.quit`.
/// `neko.run` and `neko.app` call this for you.
pub fn beginFrame() void {
    frame_count = 0;
    frame_index = 0;
    wheel_value = 0;
    keys_pressed = KeySet.initEmpty();
    keys_released = KeySet.initEmpty();
    buttons_pressed = ButtonSet.initEmpty();
    buttons_released = ButtonSet.initEmpty();
    in_frame = true;

    const e: backend.Backend = context.get() orelse return;
    mouse_pos_value = e.mouse_pos();

    while (frame_count < MAX_FRAME_EVENTS) {
        const ev: Event = e.poll_event() orelse break;
        frame_events[frame_count] = ev;
        frame_count += 1;

        switch (ev) {
            .quit => lifecycle.request_stop(),
            .key_down => |k| {
                keys_down.insert(k.key);
                keys_pressed.insert(k.key);
            },
            .key_up => |k| {
                keys_down.remove(k.key);
                keys_released.insert(k.key);
            },
            .mouse_motion => |m| mouse_pos_value = .{ .x = m.x, .y = m.y },
            .mouse_button_down => |b| {
                buttons_down.insert(b.button);
                buttons_pressed.insert(b.button);
                mouse_pos_value = .{ .x = b.x, .y = b.y };
            },
            .mouse_button_up => |b| {
                buttons_down.remove(b.button);
                buttons_released.insert(b.button);
                mouse_pos_value = .{ .x = b.x, .y = b.y };
            },
            .mouse_wheel => |w| wheel_value += w.y,
        }
    }
}

/// Ends a frame started with `beginFrame`.
pub fn endFrame() void {
    in_frame = false;
}

// ── Events ───────────────────────────────────────────────────────────────────

/// Returns the next event of the current frame. Inside a `neko.run` / `neko.app`
/// frame this replays the cached events; otherwise it polls the platform.
pub fn poll_event() ?Event {
    if (in_frame) {
        if (frame_index < frame_count) {
            const ev: Event = frame_events[frame_index];
            frame_index += 1;
            return ev;
        }
        return null;
    }
    const e: backend.Backend = context.get() orelse return null;
    return e.poll_event();
}

// ── State queries ────────────────────────────────────────────────────────────

/// True while `key` is held down.
pub fn key(k: Key) bool {
    return keys_down.contains(k);
}

/// True only on the frame `key` went down.
pub fn keyDown(k: Key) bool {
    return keys_pressed.contains(k);
}

/// True only on the frame `key` went up.
pub fn keyUp(k: Key) bool {
    return keys_released.contains(k);
}

/// True while `button` is held down.
pub fn mouseDown(button: MouseButton) bool {
    return buttons_down.contains(button);
}

/// True only on the frame `button` went down.
pub fn mousePressed(button: MouseButton) bool {
    return buttons_pressed.contains(button);
}

/// True only on the frame `button` went up.
pub fn mouseReleased(button: MouseButton) bool {
    return buttons_released.contains(button);
}

/// Current mouse position in logical pixels.
pub fn mouse_pos() types.Point {
    if (in_frame) return mouse_pos_value;
    const e: backend.Backend = context.get() orelse return .{};
    return e.mouse_pos();
}

/// The mouse position measured at the start of the frame (same as `mouse_pos`
/// during a frame).
pub fn mouse() types.Point {
    return mouse_pos_value;
}

/// The wheel delta accumulated this frame.
pub fn wheel() f32 {
    return wheel_value;
}
