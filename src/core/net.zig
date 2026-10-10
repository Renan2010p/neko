// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `neko.net` — optional networking/integration namespaces.
//!
//! These only dispatch to the backend; the core never touches a socket itself,
//! so a freestanding target keeps compiling and the calls quietly do nothing.

/// Discord Rich Presence: `neko.net.discord_rich_presence`.
pub const discord_rich_presence: type = @import("net/discord.zig");
