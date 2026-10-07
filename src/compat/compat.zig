// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `neko_compat` — compatibility layers shaped after other APIs.
//!
//! Each layer is self-contained and built only on Neko's public API, so it
//! never reaches into the engine core or the backends. Today there is one:
//!
//!   - [`pygame`] — a pygame-shaped Surface/draw/transform/display layer.
//!
//! Add another by dropping a directory here and re-exporting it below.

pub const pygame: type = @import("pygame/pygame.zig");
