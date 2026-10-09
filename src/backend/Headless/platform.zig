// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `headless` platform layer: **no operating system**.
//!
//! This backend exists so a game (or a test) can run the engine loop on any
//! target — hosted CI or bare metal — with zero OS calls and no picture. It is
//! the minimal reference for a freestanding backend: the platform layer here is
//! just a deterministic clock.

pub const Clock: type = @import("platform/clock.zig");
