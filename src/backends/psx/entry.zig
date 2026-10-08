// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! PS1 executable root — the bridge between the naked startup and the game.
//!
//! On freestanding targets Zig's `start.zig` never calls `main` and a `main` in
//! a non-root module is not a GC root, so this file is used as the executable
//! root instead of the game's `main.zig`. The game is imported under the fixed
//! name `game`, so its source keeps the normal host signature:
//!
//!     pub fn main(init: std.process.Init) !void { … }
//!
//! `_start` is a `naked` function that sets `$sp`/`$gp`, clears `.bss` and hands
//! control to the engine, which builds the `std.process.Init` the game expects.

const neko_backend: type = @import("neko_backend");
const game: type = @import("game");

extern var _bssStart: u8;
extern var _bssEnd: u8;

export fn _start() callconv(.naked) noreturn {
    asm volatile (
        \\lui $sp, 0x801F
        \\ori $sp, $sp, 0xFFF0
        \\la  $gp, _gp
        \\jal _entry
        \\nop
        \\1:
        \\j 1b
        \\nop
    );
}

export fn _entry() callconv(.c) noreturn {
    var p: usize = @intFromPtr(&_bssStart) & ~@as(usize, 3);
    const end: usize = (@intFromPtr(&_bssEnd) + 3) & ~@as(usize, 3);
    while (p < end) : (p += 4) {
        @as(*align(4) u32, @ptrFromInt(p)).* = 0;
    }
    neko_backend.run(game.main);
    while (true) {}
}
