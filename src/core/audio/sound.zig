//! `engine.sound` — sound loading, playback and volume.

const types: type = @import("../base/types.zig");
const context: type = @import("../base/context.zig");
const backend: type = @import("../base/backend.zig");
const log: type = @import("../system/log.zig");

/// Loads a sound (WAV/OGG/MP3/FLAC) from `path`.
pub fn load(path: []const u8) ?types.SoundHandle {
    const e: backend.Backend = context.get() orelse return null;
    const handle: ?types.SoundHandle = e.load_sound(path);
    if (handle == null) {
        log.warn("audio: load '{s}' failed", .{path});
    } else {
        log.info("audio: load '{s}'", .{path});
    }
    return handle;
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

// ── Object API ───────────────────────────────────────────────────────────────

/// A loaded sound with a small object API, so you do not juggle handles:
///
/// ```zig
/// const hit: ?neko.sound.Sound = neko.sound.Sound.load("sfx/hit.wav");
/// if (hit) |s| _ = s.play(-1, 0);
/// ```
pub const Sound: type = struct {
    handle: types.SoundHandle,

    /// Loads `path`; null when the backend has no sound support or fails.
    pub fn load(path: []const u8) ?Sound {
        const e: backend.Backend = context.get() orelse return null;
        const handle: types.SoundHandle = e.load_sound(path) orelse return null;
        return .{ .handle = handle };
    }

    /// Plays this sound. `loops` = -1 loops forever, `channel` = -1 picks the
    /// first free one. Returns the channel used.
    pub fn play(self: Sound, loops: i32, channel: i32) i32 {
        const e: backend.Backend = context.get() orelse return -1;
        return e.play_sound(self.handle, loops, channel);
    }
};
