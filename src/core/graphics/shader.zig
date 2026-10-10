// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `neko.shader` — runtime shader programs (capability-gated).
//!
//! Not every backend can run custom shaders: the default SDL2 (SDL_Renderer)
//! backend cannot, while the OpenGL backend compiles GLSL at runtime. Query
//! `supported()` before relying on it, and always keep a non-shader fallback.
//!
//! A program is drawn as one full-screen quad. Its vertex shader receives
//! `a_pos` (vec2, 0..1, top-left origin) and `a_uv` (vec2, 0..1); its fragment
//! shader receives `u_tex` (sampler2D, bound to `tex`) and `u_params` (vec4,
//! the `params` passed to `draw`).

const std: type = @import("std");
const types: type = @import("../base/types.zig");
const context: type = @import("../base/context.zig");
const save: type = @import("../system/save.zig");

const Allocator: type = std.mem.Allocator;

/// A compiled shader program.
pub const Program: type = struct {
    handle: types.ShaderHandle,

    /// Draws the program over the whole screen, sampling `tex` and passing
    /// `params` as `u_params`.
    pub fn draw(self: Program, tex: types.TextureHandle, params: [4]f32) void {
        const e = context.get() orelse return;
        e.shader_draw(self.handle, tex, params);
    }

    /// Frees the program.
    pub fn deinit(self: Program) void {
        const e = context.get() orelse return;
        e.shader_free(self.handle);
    }
};

/// True when the active backend can compile and run shaders.
pub fn supported() bool {
    const e = context.get() orelse return false;
    return e.supports(.shader);
}

/// Compiles `vertex_src` + `fragment_src` (GLSL). Returns null when the backend
/// has no shader support or compilation fails.
pub fn load(vertex_src: []const u8, fragment_src: []const u8) ?Program {
    const e = context.get() orelse return null;
    if (!e.supports(.shader)) return null;
    const handle = e.shader_load(vertex_src, fragment_src) orelse return null;
    return Program{ .handle = handle };
}

/// Compiles a shader whose GLSL lives in the game's assets, e.g.
/// `load_files(allocator, "fnwf/fnwf1/assets/shaders", "crt.vert", "crt.frag")`.
/// This is how a game adds its own shaders without touching the engine.
pub fn load_files(allocator: Allocator, dir: []const u8, vertex_file: []const u8, fragment_file: []const u8) ?Program {
    const e = context.get() orelse return null;
    if (!e.supports(.shader)) return null;

    const max: usize = 1 << 16;
    const vs = save.read_file(allocator, dir, vertex_file, max) orelse return null;
    defer allocator.free(vs);
    const fs = save.read_file(allocator, dir, fragment_file, max) orelse return null;
    defer allocator.free(fs);

    return load(vs, fs);
}

/// Loads a shader the backend ships itself (e.g. "cylinder"). Returns null when
/// unsupported.
pub fn load_builtin(name: []const u8) ?Program {
    const e = context.get() orelse return null;
    if (!e.supports(.shader)) return null;
    const handle = e.shader_load_builtin(name) orelse return null;
    return Program{ .handle = handle };
}
