// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `misc` capability: platform extras that do not fit anywhere else.

pub const VTable: type = struct {
    update_discord: *const fn (ptr: *anyopaque, details: []const u8, state: []const u8) void = noopUpdate,
};

fn noopUpdate(ptr: *anyopaque, details: []const u8, state: []const u8) void {
    _ = ptr;
    _ = details;
    _ = state;
}
