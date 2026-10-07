//! `neko.app` — runners that own the game loop.
//!
//! There are two styles; both drive the same engine.
//!
//! ## The easy one: a frame callback
//!
//! ```zig
//! const neko = @import("neko");
//!
//! pub fn main(init: std.process.Init) !void {
//!     try neko.run(init, .{ .title = "My Game", .width = 640, .height = 360 }, update);
//! }
//!
//! fn update(dt: f32) void {
//!     if (neko.input.key(.left)) self_x -= 120 * dt;
//!     if (neko.input.keyDown(.escape)) neko.quit();
//!     neko.draw.clear(neko.Color.hex(0x0c0c12));
//!     neko.draw.circle(@intFromFloat(self_x), 180, 24, neko.Color.red, true);
//! }
//! ```
//!
//! The runner clears the screen, computes a clamped `dt`, calls `update`, then
//! presents. `neko.input` holds the frame's key/mouse state and `.quit`
//! (window close) stops the loop automatically.
//!
//! ## The structured one: a game type
//!
//! If you prefer methods over globals, declare a struct with any of `start`,
//! `event`, `update(self, dt)`, `draw` and call `run` / `runOptions`. See the
//! `examples/app.zig` example.

const std: type = @import("std");
const types: type = @import("types.zig");
const screen: type = @import("system/screen.zig");
const lifecycle: type = @import("system/lifecycle.zig");
const input: type = @import("system/input.zig");
const time: type = @import("system/time.zig");
const debug: type = @import("system/debug.zig");
const splash: type = @import("system/splash.zig");
const draw: type = @import("graphics/draw.zig");
const sprite: type = @import("graphics/sprite.zig");

/// Tuning knobs for the structured runner (`run` / `runOptions`).
pub const Options: type = struct {
    /// Load `<assets_dir>/font/font.ttf` before the first frame so text works
    /// without an explicit `neko.text.load_font` call. Default `true`.
    load_default_font: bool = true,
    /// Clear the screen to this colour before `Game.draw`. `null` leaves
    /// clearing to the game. Default: near-black.
    clear_color: ?types.Color = types.Color{ .r = 12, .g = 12, .b = 18 },
    /// Upper bound for a single frame's delta time, in seconds. Protects
    /// physics from a long stall (e.g. dragging the window). 0 disables it.
    max_dt: f32 = 0.05,
    /// Boot splash before the first frame. `null` disables it.
    splash: ?splash.Options = .{},
};

/// Everything `frames` needs, with sensible defaults. The allocator and I/O
/// handle are filled in from `std.process.Init` automatically.
pub const Settings: type = struct {
    title: []const u8 = "Neko",
    width: u32 = 640,
    height: u32 = 360,
    assets_dir: []const u8 = "assets",
    fullscreen: bool = false,
    vsync: bool = true,
    /// Load the default font before the first frame. Default `true`.
    load_default_font: bool = true,
    /// Clear colour before the frame callback. `null` = don't clear.
    clear_color: ?types.Color = types.Color{ .r = 12, .g = 12, .b = 18 },
    /// Clamp a frame's delta time to this many seconds. 0 disables it.
    max_dt: f32 = 0.05,
    /// Boot splash before the first frame. `null` disables it.
    splash: ?splash.Options = .{},
};

// ── Easy runner: a single frame callback ─────────────────────────────────────

/// Runs `frame(f32)` every frame until the window closes or `neko.quit()`.
///
/// `frame` receives the seconds since the previous frame. Call
/// `neko.input.*` for input; raw events remain available through
/// `neko.input.poll_event` (they are the same events the runner saw).
pub fn frames(init: std.process.Init, settings: Settings, frame: *const fn (dt: f32) void) !void {
    const config: types.Config = .{
        .allocator = init.gpa,
        .io = init.io,
        .title = settings.title,
        .width = settings.width,
        .height = settings.height,
        .assets_dir = settings.assets_dir,
        .fullscreen = settings.fullscreen,
        .vsync = settings.vsync,
    };

    if (!screen.init(config)) return error.InitFailed;
    defer screen.shutdown();

    if (settings.load_default_font) sprite.load_default_font();
    if (settings.splash) |s| splash.show(s);

    var prev_ms: u64 = time.ticks_ms();
    while (lifecycle.keeps_running()) {
        input.beginFrame();

        const now_ms: u64 = time.ticks_ms();
        var dt: f32 = @as(f32, @floatFromInt(now_ms -% prev_ms)) / 1000.0;
        prev_ms = now_ms;
        if (settings.max_dt > 0 and dt > settings.max_dt) dt = settings.max_dt;
        time.setDt(dt);
        debug.tick();

        if (settings.clear_color) |color| draw.clear(color);
        frame(dt);

        screen.present();
        input.endFrame();
    }
}

// ── Structured runner: a game type ───────────────────────────────────────────

/// Runs `game` with the default `Options`.
pub fn run(comptime Game: type, game: *Game, config: types.Config) !void {
    return runOptions(Game, game, config, .{});
}

/// Runs `game` until the window closes or `neko.lifecycle.request_stop()`.
///
/// It calls the game's optional `start`, `event`, `update` and `draw` methods
/// each frame. Returns `error.InitFailed` when the backend cannot open.
pub fn runOptions(comptime Game: type, game: *Game, config: types.Config, options: Options) !void {
    if (!screen.init(config)) return error.InitFailed;
    defer screen.shutdown();

    if (options.load_default_font) sprite.load_default_font();
    if (options.splash) |s| splash.show(s);

    if (@hasDecl(Game, "start")) game.start();

    var prev_ms: u64 = time.ticks_ms();
    while (lifecycle.keeps_running()) {
        // 1. Drain the event queue into the input state and replay it.
        input.beginFrame();
        if (@hasDecl(Game, "event")) {
            while (input.poll_event()) |ev| game.event(ev);
        }

        // 2. Delta time (clamped).
        const now_ms: u64 = time.ticks_ms();
        var dt: f32 = @as(f32, @floatFromInt(now_ms -% prev_ms)) / 1000.0;
        prev_ms = now_ms;
        if (options.max_dt > 0 and dt > options.max_dt) dt = options.max_dt;
        time.setDt(dt);

        // 3. Simulate and draw.
        if (@hasDecl(Game, "update")) game.update(dt);
        debug.tick();

        if (options.clear_color) |color| draw.clear(color);
        if (@hasDecl(Game, "draw")) game.draw();

        screen.present();
        input.endFrame();
    }

    if (@hasDecl(Game, "stop")) game.stop();
}
