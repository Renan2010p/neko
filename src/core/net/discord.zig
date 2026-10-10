// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `neko.net.discord_rich_presence` — show "Playing RENGEAR" on Discord.
//!
//! ```zig
//! _ = neko.net.discord_rich_presence.connect("123456789012345678");
//! _ = neko.net.discord_rich_presence.set(.{
//!     .details = "Race · NARA COAST",
//!     .state = "Lap 1/3",
//!     .large_image = "rengear",
//!     .large_text = "RENGEAR",
//! });
//! ```
//!
//! It talks to the Discord desktop client over its local IPC socket. When
//! Discord is not running (or the backend has no support) every call is a safe
//! no-op returning `false`/`void`.

const types: type = @import("../base/types.zig");
const context: type = @import("../base/context.zig");

/// The presence payload (`neko.DiscordPresence`).
pub const Presence: type = types.DiscordPresence;

/// Connects to the local Discord client using `client_id` (your Discord
/// application id). Returns false when Discord is not running.
pub fn connect(client_id: []const u8) bool {
    const engine = context.get() orelse return false;
    return engine.discord_connect(client_id);
}

/// Sets (or updates) the presence. Returns false when not connected.
pub fn set(presence: Presence) bool {
    const engine = context.get() orelse return false;
    return engine.discord_set(presence);
}

/// Clears the presence (leaves the connection open).
pub fn clear() void {
    const engine = context.get() orelse return;
    engine.discord_clear();
}

/// Closes the Discord connection.
pub fn close() void {
    const engine = context.get() orelse return;
    engine.discord_close();
}

/// True while connected to Discord.
pub fn connected() bool {
    const engine = context.get() orelse return false;
    return engine.discord_connected();
}
