// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `headless` backend — the engine loop with **no OS**.
//!
//! It implements only the `core` capability: a deterministic clock and a run
//! flag. Drawing, text, audio, files and input all fall back to their no-op
//! defaults, so a game runs but produces no output. Use it for tests, servers
//! and as the template for a bare-metal backend.
//!
//! Select it with `-Dbackend=headless`. It needs no libc and no window.

const std: type = @import("std");
const engine: type = @import("neko");
const platform: type = @import("neko_headless_platform");

const HeadlessEngine: type = struct {
    allocator: std.mem.Allocator = undefined,
    assets_dir: []u8 = &.{},
    clock: platform.Clock = .{},
    running: bool = false,
    frames: u64 = 0,

    pub fn backend(self: *HeadlessEngine) engine.Backend {
        return .{
            .ptr = @ptrCast(self),
            .vtable = &vtable,
            .caps = caps_decl,
        };
    }
};

pub const Engine: type = HeadlessEngine;
var instance: HeadlessEngine = .{};

/// Which backend this module implements.
pub const kind: engine.BackendKind = .{ .name = "headless" };

/// The capabilities this backend declares. It provides only the loop.
const caps_decl: engine.Capabilities = engine.Capabilities.empty;

/// Returns the backend as an abstract handle. Called by `src/core/platform.zig`.
pub fn create() engine.Backend {
    return instance.backend();
}

/// Runs a game from its `main` (the headless `entry.zig` calls this).
pub fn run(init: std.process.Init, main_fn: anytype) !void {
    return main_fn(init);
}

fn as_self(ptr: *anyopaque) *HeadlessEngine {
    return @ptrCast(@alignCast(ptr));
}

const vtable: engine.Backend.VTable = .{
    .init = vt_init,
    .shutdown = vt_shutdown,
    .keeps_running = vt_keeps_running,
    .request_stop = vt_request_stop,
    .present = vt_present,
    .ticks_ms = vt_ticks_ms,
};

fn vt_init(ptr: *anyopaque, config: engine.Config) bool {
    const self: *HeadlessEngine = as_self(ptr);
    self.allocator = config.allocator;
    self.assets_dir = self.allocator.dupe(u8, config.assets_dir) catch &.{};
    self.clock = .{};
    self.frames = 0;
    self.running = true;
    engine.log.info("headless: ready (no OS)", .{});
    engine.attach(self.backend());
    return true;
}

fn vt_shutdown(ptr: *anyopaque) void {
    const self: *HeadlessEngine = as_self(ptr);
    if (self.assets_dir.len > 0) {
        self.allocator.free(self.assets_dir);
        self.assets_dir = &.{};
    }
    self.running = false;
    engine.detach();
}

fn vt_keeps_running(ptr: *anyopaque) bool {
    return as_self(ptr).running;
}

fn vt_request_stop(ptr: *anyopaque) void {
    as_self(ptr).running = false;
}

fn vt_present(ptr: *anyopaque) void {
    const self: *HeadlessEngine = as_self(ptr);
    _ = self.clock.tick();
    self.frames += 1;
}

fn vt_ticks_ms(ptr: *anyopaque) u64 {
    return as_self(ptr).clock.read();
}
