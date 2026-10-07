//! The active backend and process-wide engine state.
//!
//! A backend registers itself here when it starts, so the public `neko.*`
//! namespaces can dispatch without an explicit handle:
//!
//!     neko.draw.rect(...)
//!
//! This is the *only* place the core keeps runtime platform state. Everything
//! that performs OS work goes through the abstract `Backend` stored in
//! `current`; the core itself only remembers an allocator and the assets
//! directory, both plain data.

const std: type = @import("std");
const Allocator: type = std.mem.Allocator;
const backend: type = @import("backend.zig");

/// The backend currently in use, or null before `attach` / after `detach`.
pub var current: ?backend.Backend = null;

/// The allocator passed in `Config`. Valid once the engine is initialized.
pub var allocator: Allocator = undefined;

/// The assets directory passed in `Config`.
pub var assets_dir: []const u8 = "assets";

/// Makes `handle` the active backend.
pub fn attach(handle: backend.Backend) void {
    current = handle;
}

/// Clears the active backend.
pub fn detach() void {
    current = null;
}

/// Returns the active backend, or null if none is attached.
pub fn get() ?backend.Backend {
    return current;
}
