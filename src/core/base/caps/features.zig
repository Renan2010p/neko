// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! Capability flags a backend can declare.
//!
//! A backend fills `Backend.caps` with the features it provides. Game code
//! queries them with `neko.Backend.supports(.graphics3d)` (or the `neko.screen`
//! helpers) instead of testing for a concrete backend kind. New features are
//! added here, so the API grows without breaking existing backends.

const std: type = @import("std");

/// A capability or optional feature a backend may provide.
///
/// These are *feature* flags, not implementations: a backend may advertise
/// `graphics3d` while a 2D-only backend leaves it out and the `neko.render3d`
/// calls become no-ops.
pub const Feature: type = enum {
    /// 2D drawing primitives and textures.
    graphics2d,
    /// Font loading and text drawing.
    text,
    /// Sound loading and playback.
    audio,
    /// File reads/writes (save files, assets).
    files,
    /// Event polling and live mouse state.
    input,
    /// A depth-tested 3D pipeline (`neko.render3d`).
    graphics3d,
    /// CPU-writable streaming textures (`create_texture`/`update_texture`).
    streaming_textures,
    /// Offscreen render targets (`create_target`/`set_render_target`).
    offscreen_targets,
    /// Arbitrary textured geometry (`geometry`).
    geometry,
    /// Enumerating display modes.
    display_modes,
    /// The curved-panorama composite (`neko.render`).
    curved_panorama,
    /// Discord presence updates.
    discord,
};

/// A set of declared capabilities. Empty when a backend does not declare any.
pub const Capabilities: type = std.EnumSet(Feature);
