// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `discord` capability: Discord Rich Presence over the local IPC socket.
//!
//! Optional — a backend that does not implement it leaves these at their no-op
//! defaults, and `neko.net.discord_rich_presence` quietly does nothing. Only
//! hosted desktop backends (SDL2) implement it.

const types: type = @import("../types.zig");

pub const VTable: type = struct {
    discord_connect: *const fn (ptr: *anyopaque, client_id: []const u8) bool = noopFalse,
    discord_set: *const fn (ptr: *anyopaque, presence: types.DiscordPresence) bool = noopSet,
    discord_clear: *const fn (ptr: *anyopaque) void = noopVoid,
    discord_close: *const fn (ptr: *anyopaque) void = noopVoid,
    discord_connected: *const fn (ptr: *anyopaque) bool = noopFalse1,
};

fn noopFalse(ptr: *anyopaque, client_id: []const u8) bool {
    _ = ptr;
    _ = client_id;
    return false;
}

fn noopFalse1(ptr: *anyopaque) bool {
    _ = ptr;
    return false;
}

fn noopSet(ptr: *anyopaque, presence: types.DiscordPresence) bool {
    _ = ptr;
    _ = presence;
    return false;
}

fn noopVoid(ptr: *anyopaque) void {
    _ = ptr;
}
