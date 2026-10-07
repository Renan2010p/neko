//! `neko.window` — an explicit window object for hand-written loops.
//!
//! Everything else in the engine is a process-wide singleton (one window per
//! process), so `Window` is a thin, ergonomic handle over `neko.screen` and
//! `neko.input`. It gives you the classic shape:
//!
//! ```zig
//! var window = try neko.window.create(init, .{
//!     .title = "My Game", .width = 640, .height = 360,
//! });
//! defer window.close();
//!
//! while (window.is_open()) {
//!     window.begin_frame();                 // drains events, computes dt
//!     while (window.poll_event()) |ev| {    // switch on events
//!         switch (ev) {
//!             .quit => window.close(),
//!             .key_down => |key| if (key.key == .escape) window.close(),
//!             else => {},
//!         }
//!     }
//!
//!     neko.draw.clear(neko.Color.hex(0x0c0c12));
//!     // ... draw ...
//!     window.present();
//!     window.end_frame();
//! }
//! ```
//!
//! After `close()` the engine is detached, so any further `neko.draw.*` calls
//! are safe no-ops and the loop's `is_open()` becomes false.
//!
//! Prefer the higher-level `neko.run` / `neko.app.run`? Keep using them — this
//! type is for when you want to own the loop and the `switch (event)` yourself.

const std: type = @import("std");
const types: type = @import("../types.zig");
const context: type = @import("../context.zig");
const screen: type = @import("screen.zig");
const input: type = @import("input.zig");
const lifecycle: type = @import("lifecycle.zig");
const time: type = @import("time.zig");
const event: type = @import("event.zig");
const scene: type = @import("../scene/scene.zig");
const splash_mod: type = @import("splash.zig");

pub const Event: type = event.Event;

/// Startup options. The allocator and I/O handle come from `std.process.Init`.
pub const Options: type = struct {
    title: []const u8 = "Neko",
    width: u32 = 640,
    height: u32 = 360,
    assets_dir: []const u8 = "assets",
    fullscreen: bool = false,
    vsync: bool = true,
};

/// A handle to the (single) engine window.
pub const Window: type = struct {
    open: bool = false,
    last_ms: u64 = 0,
    dt_value: f32 = 0,

    /// True while the window is open and no stop was requested.
    pub fn is_open(self: *const Window) bool {
        return self.open and lifecycle.keeps_running();
    }

    /// Starts a frame: drains events, updates input state and computes `dt`.
    /// Call it first thing in the loop.
    pub fn begin_frame(self: *Window) void {
        input.beginFrame();
        const now_ms: u64 = time.ticks_ms();
        const delta_ms: u64 = now_ms -% self.last_ms;
        self.last_ms = now_ms;
        self.dt_value = @as(f32, @floatFromInt(delta_ms)) / 1000.0;
        time.setDt(self.dt_value);
    }

    /// Seconds since the previous `begin_frame`. Same as `neko.time.dt()`.
    pub fn dt(self: *const Window) f32 {
        return self.dt_value;
    }

    /// Ends a frame started with `begin_frame`.
    pub fn end_frame(self: *const Window) void {
        _ = self;
        input.endFrame();
    }

    /// The next event of this frame, or null when there are none left.
    pub fn poll_event(self: *const Window) ?Event {
        _ = self;
        return input.poll_event();
    }

    /// Shows everything drawn since the last call.
    pub fn present(self: *const Window) void {
        _ = self;
        screen.present();
    }

    /// Closes and releases the window. Safe to call more than once.
    pub fn close(self: *Window) void {
        if (!self.open) return;
        self.open = false;
        scene.deinit();
        screen.shutdown();
    }

    // ── Scenes ───────────────────────────────────────────────────────────────

    /// Makes `root` the current scene, freeing the previous one (Godot's
    /// `change_scene`). Build a scene with `neko.scene.create`.
    pub fn switch_to(self: *const Window, root: *scene.Node) void {
        _ = self;
        scene.switch_to(root);
    }

    /// Pushes an overlay scene on top, keeping the current one alive.
    pub fn push_scene(self: *const Window, root: *scene.Node) void {
        _ = self;
        scene.push(root);
    }

    /// Pops the top overlay scene. Returns false if the stack was empty.
    pub fn pop_scene(self: *const Window) bool {
        _ = self;
        return scene.pop();
    }

    /// Advances the current scene: `ready` on the first frame, then `process`
    /// down the tree. Call once per frame before `draw`.
    pub fn process(self: *const Window, delta: f32) void {
        _ = self;
        scene.process(delta);
    }

    /// Draws the current scene: clears with the scene colour, then draws the
    /// whole tree. Call `present` afterwards.
    pub fn draw(self: *const Window) void {
        _ = self;
        scene.draw();
    }

    /// The current scene root, or null.
    pub fn current_scene(self: *const Window) ?*scene.Node {
        _ = self;
        return scene.current();
    }

    /// The number of scenes on the stack.
    pub fn scene_depth(self: *const Window) usize {
        _ = self;
        return scene.depth();
    }

    /// Colour `draw` clears with (default black).
    pub fn set_clear_color(self: *const Window, color: types.Color) void {
        _ = self;
        scene.set_clear_color(color);
    }

    /// The current logical size, in pixels.
    pub fn size(self: *const Window) types.Point {
        _ = self;
        return screen.logical_size();
    }

    pub fn set_logical_size(self: *const Window, width: u32, height: u32) void {
        _ = self;
        screen.set_logical_size(width, height);
    }

    pub fn set_resolution(self: *const Window, width: u32, height: u32) void {
        _ = self;
        screen.set_resolution(width, height);
    }

    pub fn set_fullscreen(self: *const Window, on: bool) void {
        _ = self;
        screen.set_fullscreen(on);
    }

    pub fn set_vsync(self: *const Window, on: bool) void {
        _ = self;
        screen.set_vsync(on);
    }

    /// Lists the display modes the backend can see (caller owns the result).
    pub fn display_modes(self: *const Window, allocator: std.mem.Allocator) []types.DisplayMode {
        _ = self;
        return screen.display_modes(allocator);
    }

    /// Shows the asset-free NEKO boot splash. Call right after `create`,
    /// before the first game frame.
    pub fn splash(self: *const Window, options: splash_mod.Options) void {
        _ = self;
        splash_mod.show(options);
    }
};

/// Opens the window and returns a handle. Only one window per process is
/// supported; a second `create` returns `error.WindowAlreadyOpen`.
pub fn create(init: std.process.Init, options: Options) !Window {
    if (context.get() != null) return error.WindowAlreadyOpen;

    const config: types.Config = .{
        .allocator = init.gpa,
        .io = init.io,
        .title = options.title,
        .width = options.width,
        .height = options.height,
        .assets_dir = options.assets_dir,
        .fullscreen = options.fullscreen,
        .vsync = options.vsync,
    };

    if (!screen.init(config)) return error.InitFailed;

    // The window owns the scene stack, so games never manage it separately.
    scene.init(init.gpa);

    return .{ .open = true, .last_ms = time.ticks_ms() };
}
