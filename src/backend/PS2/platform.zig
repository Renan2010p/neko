// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! The PS2 **platform layer** facade.
//!
//! The renderer (`render/render.zig`) talks to the console only through this
//! module: the PS2SDK bindings (`ps2sdk.zig`) and the gsKit graphics library
//! (`gskit.zig`), both under `platform/`.

pub const ps2sdk: type = @import("platform/ps2sdk.zig");
pub const gskit: type = @import("platform/gskit.zig");
