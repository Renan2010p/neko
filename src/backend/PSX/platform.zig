// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! The PSX **platform layer** facade.
//!
//! The renderer (`render/render.zig`) drives the console through `hw.zig`
//! (GPU, DMA, controller, timers) under `platform/`.

pub const hw: type = @import("platform/hw.zig");
