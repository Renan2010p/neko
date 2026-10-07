//! `engine.sound` — sound loading, playback and volume.

const types: type = @import("../types.zig");
const context: type = @import("../context.zig");
const backend: type = @import("../backend.zig");

/// Loads a sound (WAV/OGG/MP3/FLAC) from `path`.
pub fn load(path: []const u8) ?types.SoundHandle {
    const e: backend.Backend = context.get() orelse return null;
    return e.load_sound(path);
}

/// Plays `snd` on `channel` (-1 = first free), looping `loops` times
/// (-1 = forever). Returns the channel used.
pub fn play(snd: types.SoundHandle, loops: i32, channel: i32) i32 {
    const e: backend.Backend = context.get() orelse return -1;
    return e.play_sound(snd, loops, channel);
}

/// Stops a channel (-1 = all).
pub fn stop_channel(channel: i32) void {
    const e: backend.Backend = context.get() orelse return;
    e.stop_channel(channel);
}

/// Stops every sound.
pub fn stop_all() void {
    const e: backend.Backend = context.get() orelse return;
    e.stop_all_sounds();
}

/// Sets the master volume (0-100).
pub fn master_volume(vol: i32) void {
    const e: backend.Backend = context.get() orelse return;
    e.set_master_volume(vol);
}

/// Sets the sound-effects volume (0-100).
pub fn sfx_volume(vol: i32) void {
    const e: backend.Backend = context.get() orelse return;
    e.set_sfx_volume(vol);
}

/// Sets the music volume (0-100).
pub fn music_volume(vol: i32) void {
    const e: backend.Backend = context.get() orelse return;
    e.set_music_volume(vol);
}
