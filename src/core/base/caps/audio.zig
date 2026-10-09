// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `audio` capability: sound loading, playback and volume.

const types: type = @import("../types.zig");

pub const VTable: type = struct {
    load_sound: *const fn (ptr: *anyopaque, path: []const u8) ?types.SoundHandle = noopNullSound,
    play_sound: *const fn (ptr: *anyopaque, snd: types.SoundHandle, loops: i32, channel: i32) i32 = noopPlay,
    stop_channel: *const fn (ptr: *anyopaque, channel: i32) void = noopChannel,
    stop_all_sounds: *const fn (ptr: *anyopaque) void = noopVoid,
    set_master_volume: *const fn (ptr: *anyopaque, vol: i32) void = noopVolume,
    set_sfx_volume: *const fn (ptr: *anyopaque, vol: i32) void = noopVolume,
    set_music_volume: *const fn (ptr: *anyopaque, vol: i32) void = noopVolume,
};

fn noopNullSound(ptr: *anyopaque, path: []const u8) ?types.SoundHandle {
    _ = ptr;
    _ = path;
    return null;
}

fn noopPlay(ptr: *anyopaque, snd: types.SoundHandle, loops: i32, channel: i32) i32 {
    _ = ptr;
    _ = snd;
    _ = loops;
    _ = channel;
    return -1;
}

fn noopChannel(ptr: *anyopaque, channel: i32) void {
    _ = ptr;
    _ = channel;
}

fn noopVoid(ptr: *anyopaque) void {
    _ = ptr;
}

fn noopVolume(ptr: *anyopaque, vol: i32) void {
    _ = ptr;
    _ = vol;
}
