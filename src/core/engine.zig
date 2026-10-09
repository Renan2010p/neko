// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `neko.Engine` — an **explicit, optional** engine handle.
//!
//! The public `neko.*` namespaces stay process-global: `neko.draw`, `neko.input`
//! and friends dispatch to the active backend stored in `neko.core.context`.
//! `Engine` makes that active handle explicit — it owns a `Config`, starts and
//! stops the backend, and exposes the same capability queries. It is useful for
//! tools, tests and embedding.
//!
//! ```zig
//! var engine = neko.Engine.create(config);
//! if (!engine.init()) return error.InitFailed;
//! defer engine.shutdown();
//!
//! while (engine.keeps_running()) {
//!     neko.input.beginFrame();
//!     // ... draw ...
//!     engine.present();
//! }
//! ```
//!
//! There is one process-wide backend instance today, so `Engine` currently
//! manages the active handle: creating and initialising it also makes it the
//! backend the global namespaces dispatch to. Multiple independent engines are
//! a future extension.

const std: type = @import("std");
const Allocator: type = std.mem.Allocator;
const types: type = @import("base/types.zig");
const backend_mod: type = @import("base/backend.zig");
const context: type = @import("base/context.zig");
const platform: type = @import("platform.zig");

pub const Engine: type = struct {
    /// The bootstrap data this engine was created with.
    config: types.Config,
    /// The backend handle this engine drives.
    handle: backend_mod.Backend,

    /// Builds an engine from `config`, without starting it. The backend handle
    /// is the process-wide instance selected at build time.
    pub fn create(config: types.Config) Engine {
        return .{ .config = config, .handle = platform.create() };
    }

    /// Makes this engine's backend active and starts it (opens the window,
    /// audio, …). Returns `false` when the backend cannot start.
    pub fn init(self: *Engine) bool {
        context.allocator = self.config.allocator;
        context.assets_dir = self.config.assets_dir;
        context.attach(self.handle);
        return self.handle.init(self.config);
    }

    /// Stops the backend and releases its resources.
    pub fn shutdown(self: *Engine) void {
        self.handle.shutdown();
    }

    /// Shows everything drawn since the last call.
    pub fn present(self: *Engine) void {
        self.handle.present();
    }

    /// True while the run loop should keep going.
    pub fn keeps_running(self: *Engine) bool {
        return self.handle.keeps_running();
    }

    /// Asks the backend's run loop to stop after the current frame.
    pub fn request_stop(self: *Engine) void {
        self.handle.request_stop();
    }

    /// The abstract backend handle (advanced).
    pub fn backend(self: *Engine) backend_mod.Backend {
        return self.handle;
    }

    /// The allocator passed in `Config`.
    pub fn allocator(self: *Engine) Allocator {
        return self.config.allocator;
    }

    /// The assets directory passed in `Config`.
    pub fn assets_dir(self: *Engine) []const u8 {
        return self.config.assets_dir;
    }

    /// True when the backend declares `feature`.
    pub fn supports(self: *Engine, feature: backend_mod.Feature) bool {
        return self.handle.supports(feature);
    }
};
