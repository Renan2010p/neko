// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `neko.assets` — a name → loaded-resource registry with **visible errors**.
//!
//! Unlike `neko.sprite` (which silently falls back to a grey placeholder), the
//! loads here return errors, so a missing file is caught at startup:
//!
//! ```zig
//! try neko.assets.load_texture("hero", "art/hero.png");
//! const hero: neko.TextureHandle = neko.assets.texture("hero").?;
//! ```
//!
//! Names are owned by the registry (it copies them); `unload` frees them. The
//! registry only *forgets* handles — the backend owns the GPU/audio object.

const std: type = @import("std");
const types: type = @import("../base/types.zig");
const context: type = @import("../base/context.zig");
const texture_mod: type = @import("../graphics/texture.zig");
const text_mod: type = @import("../graphics/text.zig");
const sound_mod: type = @import("../audio/sound.zig");
const log: type = @import("log.zig");

/// What a load can fail with. `LoadFailed` means the backend refused (missing
/// file, unsupported format, or no such capability).
pub const Error: type = error{LoadFailed};

const TextureMap: type = std.StringHashMapUnmanaged(types.TextureHandle);
const FontMap: type = std.StringHashMapUnmanaged(i64);
const SoundMap: type = std.StringHashMapUnmanaged(types.SoundHandle);

var textures: TextureMap = .empty;
var fonts: FontMap = .empty;
var sounds: SoundMap = .empty;

/// Loads a texture under `name`. Idempotent: a known name returns the existing
/// handle.
pub fn load_texture(name: []const u8, path: []const u8) !types.TextureHandle {
    if (textures.get(name)) |existing| return existing;
    const handle: types.TextureHandle = texture_mod.load(path) orelse {
        log.warn("assets: texture '{s}' <- '{s}' failed", .{ name, path });
        return error.LoadFailed;
    };
    try put(types.TextureHandle, &textures, name, handle);
    log.info("assets: texture '{s}' <- '{s}'", .{ name, path });
    return handle;
}

/// The texture registered under `name`, or null.
pub fn texture(name: []const u8) ?types.TextureHandle {
    return textures.get(name);
}

/// Loads a font under `name` at `pixel_size`. Idempotent.
pub fn load_font(name: []const u8, path: []const u8, pixel_size: u16) !i64 {
    if (fonts.get(name)) |existing| return existing;
    const index: i64 = text_mod.load_font(path, pixel_size);
    if (index < 0) {
        log.warn("assets: font '{s}' <- '{s}' ({d}px) failed", .{ name, path, pixel_size });
        return error.LoadFailed;
    }
    try put(i64, &fonts, name, index);
    log.info("assets: font '{s}' <- '{s}' ({d}px)", .{ name, path, pixel_size });
    return index;
}

/// The font index registered under `name`, or null.
pub fn font(name: []const u8) ?i64 {
    return fonts.get(name);
}

/// Loads a sound under `name`. Idempotent.
pub fn load_sound(name: []const u8, path: []const u8) !types.SoundHandle {
    if (sounds.get(name)) |existing| return existing;
    const handle: types.SoundHandle = sound_mod.load(path) orelse {
        log.warn("assets: sound '{s}' <- '{s}' failed", .{ name, path });
        return error.LoadFailed;
    };
    try put(types.SoundHandle, &sounds, name, handle);
    log.info("assets: sound '{s}' <- '{s}'", .{ name, path });
    return handle;
}

/// The sound registered under `name`, or null.
pub fn sound(name: []const u8) ?types.SoundHandle {
    return sounds.get(name);
}

/// Forgets `name` from every registry (does not destroy the resource).
pub fn unload(name: []const u8) void {
    if (textures.fetchRemove(name)) |entry| context.allocator.free(entry.key);
    if (fonts.fetchRemove(name)) |entry| context.allocator.free(entry.key);
    if (sounds.fetchRemove(name)) |entry| context.allocator.free(entry.key);
}

/// Forgets every name.
pub fn unload_all() void {
    free_keys(types.TextureHandle, &textures);
    free_keys(i64, &fonts);
    free_keys(types.SoundHandle, &sounds);
}

/// Frees the registries. Called by `neko.screen.shutdown`.
pub fn deinit() void {
    unload_all();
    textures.deinit(context.allocator);
    fonts.deinit(context.allocator);
    sounds.deinit(context.allocator);
}

// ── Internal ─────────────────────────────────────────────────────────────────

fn put(comptime V: type, map: anytype, name: []const u8, value: V) !void {
    const key: []u8 = try context.allocator.dupe(u8, name);
    errdefer context.allocator.free(key);
    try map.put(context.allocator, key, value);
    log.debug("assets: alloc {d} bytes for name '{s}'", .{ key.len, name });
}

fn free_keys(comptime V: type, map: anytype) void {
    _ = V;
    var it = map.iterator();
    while (it.next()) |entry| context.allocator.free(entry.key_ptr.*);
    map.clearRetainingCapacity();
}

// ── Tests ────────────────────────────────────────────────────────────────────

const testing: type = @import("std").testing;

test "assets reports a load failure without a backend" {
    try testing.expectError(error.LoadFailed, load_texture("missing", "missing.png"));
    try testing.expect(texture("missing") == null);
}
