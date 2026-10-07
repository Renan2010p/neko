// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! PS2 executable root — the bridge between crt0 and the game.
//!
//! PS2 builds use this file as the executable root instead of the game's
//! `main.zig`, because on freestanding targets a `main` in a non-root module is
//! not a GC root (Zig's `start.zig` does nothing for `.freestanding`) and
//! `std.process.Init` cannot be built by the runtime. The game is imported
//! under the fixed name `game`, so its source keeps the normal host signature:
//!
//!     pub fn main(init: std.process.Init) !void { … }
//!
//! The game's build wires this up (when `backend == .ps2`):
//!
//!     const entry = neko_dep.path("src/platform/ps2/entry.zig");
//!     const root = b.createModule(.{
//!         .root_source_file = entry, .target = target, .optimize = optimize,
//!         .imports = &.{
//!             .{ .name = "game", .module = game_module },
//!             .{ .name = "neko", .module = neko },
//!         },
//!     });

const neko_backend: type = @import("neko_backend");
const game: type = @import("game");

/// The symbol PS2 crt0 calls. Owned by the engine, not by the game.
export fn main() callconv(.c) void {
    neko_backend.run(game.main);
}
