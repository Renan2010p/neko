// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! SDL2 executable root — the hosted analogue of the PS2/PSX `entry.zig`.
//!
//! On hosted targets the C/Zig runtime already calls `main` with a
//! `std.process.Init`, so there is no crt0 shim here. This module exists so
//! every backend exposes the same `entry.zig`, letting a game use one entry
//! point on every platform. The game is imported under the fixed name `game`
//! and keeps the normal host signature:
//!
//!     pub fn main(init: std.process.Init) !void { … }
//!
//! A game that wants the uniform entry wires it up as its executable root:
//!
//!     const entry = neko_dep.path("src/backend/SDL2/entry.zig");
//!     const root = b.createModule(.{
//!         .root_source_file = entry, .target = target, .optimize = optimize,
//!         .imports = &.{
//!             .{ .name = "game", .module = game_module },
//!             .{ .name = "neko_backend", .module = backend_module },
//!         },
//!     });

const std: type = @import("std");
const process: type = std.process;
const neko_backend: type = @import("neko_backend");
const game: type = @import("game");

pub fn main(init: process.Init) !void {
    try neko_backend.run(init, game.main);
}
