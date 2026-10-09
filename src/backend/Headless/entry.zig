// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `headless` executable bootstrap.
//!
//! On a hosted target the runtime calls `main`; on bare metal a crt0 shim would
//! call `neko_backend.run`. The game is imported as `game` so it keeps the
//! normal `main(init: std.process.Init)` signature.

const std: type = @import("std");
const neko_backend: type = @import("neko_backend");
const game: type = @import("game");

pub fn main(init: std.process.Init) !void {
    try neko_backend.run(init, game.main);
}
