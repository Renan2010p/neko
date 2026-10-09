//! `neko.screen` — the display: startup, window state and presenting.
//!
//! The concrete backend is created through the platform seam
//! (`src/core/platform.zig`), so game code never names the platform:
//!
//!     if (!neko.screen.init(config)) return error.InitFailed;
//!     defer neko.screen.shutdown();
//!     ...
//!     neko.screen.present();

const std: type = @import("std");
const Allocator: type = std.mem.Allocator;
const platform: type = @import("../platform.zig");
const types: type = @import("../base/types.zig");
const context: type = @import("../base/context.zig");
const backend: type = @import("../base/backend.zig");
const sprite: type = @import("../graphics/sprite.zig");
const assets: type = @import("assets.zig");
const log: type = @import("log.zig");

/// Opens the window and prepares everything.
pub fn init(config: types.Config) bool {
    context.allocator = config.allocator;
    context.assets_dir = config.assets_dir;
    log.info("screen.init: backend={s} {d}x{d} assets='{s}'", .{ platform.kind.name, config.width, config.height, config.assets_dir });

    const handle: backend.Backend = platform.create();
    context.attach(handle);
    const ok: bool = handle.init(config);
    if (!ok) log.err("screen.init: backend '{s}' failed to start", .{platform.kind.name});
    return ok;
}

/// Releases every resource and closes the window.
pub fn shutdown() void {
    log.info("screen.shutdown", .{});
    sprite.deinit();
    assets.deinit();

    const e: backend.Backend = context.get() orelse return;
    e.shutdown();
}

/// Shows everything drawn since the last call.
pub fn present() void {
    const e: backend.Backend = context.get() orelse return;
    e.present();
}

/// Sets the window title.
pub fn set_caption(title: []const u8) void {
    const e: backend.Backend = context.get() orelse return;
    e.set_title(title);
}

/// Sets the virtual resolution the backend scales to the real window.
pub fn set_logical_size(width: u32, height: u32) void {
    const e: backend.Backend = context.get() orelse return;
    e.set_logical_size(width, height);
}

/// Switches fullscreen on or off.
pub fn set_fullscreen(on: bool) void {
    const e: backend.Backend = context.get() orelse return;
    e.set_fullscreen(on);
}

/// Switches vertical sync on or off.
pub fn set_vsync(on: bool) void {
    const e: backend.Backend = context.get() orelse return;
    e.set_vsync(on);
}

/// Resizes the real window.
pub fn set_resolution(width: u32, height: u32) void {
    const e: backend.Backend = context.get() orelse return;
    e.set_resolution(width, height);
}

/// The current logical size, in pixels.
pub fn logical_size() types.Point {
    const e: backend.Backend = context.get() orelse return types.Point{ .x = 0, .y = 0 };
    return e.logical_size();
}

/// Lists the display modes the backend can see. The caller owns the result
/// and frees it with the same allocator.
pub fn display_modes(allocator: Allocator) []types.DisplayMode {
    const e: backend.Backend = context.get() orelse return &.{};
    return e.display_modes(allocator);
}

/// True where the platform can composite a curved panorama cheaply. False on
/// systems without it (e.g. the PS2).
pub fn supports_curved_panorama() bool {
    const e: backend.Backend = context.get() orelse return true;
    return e.supports_curved_panorama();
}

/// True where offscreen render targets are cheap. False on the PS2.
pub fn supports_offscreen_targets() bool {
    const e: backend.Backend = context.get() orelse return true;
    return e.supports_offscreen_targets();
}

/// Adds a temporary translation to every logical draw coordinate.
pub fn set_draw_offset(dx: i32, dy: i32) void {
    const e: backend.Backend = context.get() orelse return;
    e.set_draw_offset(dx, dy);
}
