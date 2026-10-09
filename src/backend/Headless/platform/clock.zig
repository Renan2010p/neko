// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! A virtual monotonic clock: no OS timer, no `std.time`.

/// Advances by `step_ms` on every `tick`, starting at zero. Deterministic, so
/// a headless run is reproducible.
pub const Clock: type = struct {
    /// Milliseconds added per frame.
    step_ms: u64 = 16,
    /// Milliseconds elapsed so far.
    now_ms: u64 = 0,

    /// Advances one frame and returns the new time.
    pub fn tick(self: *Clock) u64 {
        self.now_ms += self.step_ms;
        return self.now_ms;
    }

    /// The current time, without advancing.
    pub fn read(self: *Clock) u64 {
        return self.now_ms;
    }
};
