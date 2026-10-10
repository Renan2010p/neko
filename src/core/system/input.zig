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
const types: type = @import("../base/types.zig");
const context: type = @import("../base/context.zig");
const backend: type = @import("../base/backend.zig");
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

var keys_down: KeySet = KeySet.empty;
var keys_pressed: KeySet = KeySet.empty;
var keys_released: KeySet = KeySet.empty;

var buttons_down: ButtonSet = ButtonSet.empty;
var buttons_pressed: ButtonSet = ButtonSet.empty;
var buttons_released: ButtonSet = ButtonSet.empty;

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
    keys_pressed = KeySet.empty;
    keys_released = KeySet.empty;
    buttons_pressed = ButtonSet.empty;
    buttons_released = ButtonSet.empty;
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

// ── Actions ──────────────────────────────────────────────────────────────────

/// One physical input a named action can be bound to.
pub const Source: type = union(enum) {
    key: Key,
    mouse: MouseButton,
};

/// A type-safe action map for an enum you define. It is a value you own (no
/// global state), so it is easy to test and reuse:
///
/// ```zig
/// const Act = enum { jump, left, right };
/// var acts: neko.input.Actions(Act) = .{};
///
/// acts.bind(.jump, &.{ .{ .key = .space } });
/// acts.bind(.left, &.{ .{ .key = .left }, .{ .key = .a } });
/// acts.bind(.right, &.{ .{ .key = .right }, .{ .key = .d } });
///
/// if (acts.justPressed(.jump)) player.jump();
/// const move: f32 = acts.axis(.left, .right); // -1..1
/// ```
///
/// Raw `key`/`keyDown`/`mouse*` queries stay available for menus and tools.
pub fn Actions(comptime E: type) type {
    return struct {
        const Self: type = @This();
        const max_sources: usize = 4;

        const Slot: type = struct {
            sources: [max_sources]Source = undefined,
            len: usize = 0,
        };

        slots: [std.meta.fields(E).len]Slot = @splat(.{}),

        const Match: type = enum { held, pressed, released };

        /// Binds `action` to `sources`, replacing any previous binding.
        pub fn bind(self: *Self, action: E, sources: []const Source) void {
            const i: usize = @backingInt(action);
            const n: usize = @min(sources.len, max_sources);
            var j: usize = 0;
            while (j < n) : (j += 1) {
                self.slots[i].sources[j] = sources[j];
            }
            self.slots[i].len = n;
        }

        fn matches(self: *const Self, action: E, mode: Match) bool {
            const slot: Slot = self.slots[@backingInt(action)];
            for (slot.sources[0..slot.len]) |source| {
                switch (source) {
                    .key => |k| switch (mode) {
                        .held => if (key(k)) return true,
                        .pressed => if (keyDown(k)) return true,
                        .released => if (keyUp(k)) return true,
                    },
                    .mouse => |b| switch (mode) {
                        .held => if (mouseDown(b)) return true,
                        .pressed => if (mousePressed(b)) return true,
                        .released => if (mouseReleased(b)) return true,
                    },
                }
            }
            return false;
        }

        /// True while any bound source of `action` is held.
        pub fn held(self: *const Self, action: E) bool {
            return self.matches(action, .held);
        }

        /// True only on the frame any bound source of `action` went down.
        pub fn justPressed(self: *const Self, action: E) bool {
            return self.matches(action, .pressed);
        }

        /// True only on the frame any bound source of `action` went up.
        pub fn justReleased(self: *const Self, action: E) bool {
            return self.matches(action, .released);
        }

        /// `positive - negative`, each 0 or 1: an analog-style axis from two
        /// digital actions.
        pub fn axis(self: *const Self, negative: E, positive: E) f32 {
            const lo: f32 = if (self.held(negative)) 1 else 0;
            const hi: f32 = if (self.held(positive)) 1 else 0;
            return hi - lo;
        }
    };
}

// ── Tests ────────────────────────────────────────────────────────────────────

const testing: type = @import("std").testing;

test "Actions binds sources and reports idle state" {
    const Act: type = enum { jump, left, right };

    var acts: Actions(Act) = .{};
    acts.bind(.jump, &.{ .{ .key = .space }, .{ .mouse = .left } });
    acts.bind(.left, &.{.{ .key = .left }});
    acts.bind(.right, &.{.{ .key = .right }});

    // No backend, no events: everything is idle.
    try testing.expect(!acts.held(.jump));
    try testing.expect(!acts.justPressed(.jump));
    try testing.expectEqual(@as(f32, 0), acts.axis(.left, .right));
}
