// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `neko.log` — engine logging, built on `std.log`.
//!
//! Everything is scoped `.neko`, so a game can filter it with `std_options`
//! (`log_level` / `log_scope_levels`). On a **freestanding** target the calls
//! compile away entirely (there is no logger and no OS), so the core stays
//! clean.
//!
//! ```zig
//! const neko = @import("neko");
//! neko.log.info("hello {d}", .{1});
//! ```

const std: type = @import("std");
const builtin: type = @import("builtin");

const logger: type = std.log.scoped(.neko);
const hosted: bool = builtin.os.tag != .freestanding;

/// Verbose, often per-frame diagnostics. Hidden at the default level.
pub fn debug(comptime fmt: []const u8, args: anytype) void {
    if (comptime hosted) logger.debug(fmt, args);
}

/// Notable one-time events: backend start, asset loaded, GPU upload.
pub fn info(comptime fmt: []const u8, args: anytype) void {
    if (comptime hosted) logger.info(fmt, args);
}

/// Something recoverable went wrong (missing asset, degraded feature).
pub fn warn(comptime fmt: []const u8, args: anytype) void {
    if (comptime hosted) logger.warn(fmt, args);
}

/// A hard failure (backend could not start, GPU object creation failed).
pub fn err(comptime fmt: []const u8, args: anytype) void {
    if (comptime hosted) logger.err(fmt, args);
}
