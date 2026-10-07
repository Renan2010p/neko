// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `display` — the single canvas presented to the window.

const std: type = @import("std");
const neko: type = @import("neko");
const surface_mod: type = @import("surface.zig");

const Surface: type = surface_mod.Surface;
const Allocator: type = std.mem.Allocator;

/// The canvas currently presented to the window.
var canvas: ?*Surface = null;
var canvas_tex: ?neko.TextureHandle = null;
var logical_ready: bool = false;

/// No-op, for pygame-style `pygame.init()`.
pub fn init() void {}
/// No-op, for pygame-style `pygame.quit()`.
pub fn quit() void {}

fn create_mode(w: u32, h: u32) *Surface {
    const alloc: Allocator = std.heap.c_allocator;
    const s: *Surface = alloc.create(Surface) catch @panic("neko_pygame: out of memory");
    s.* = Surface.init(alloc, w, h) catch @panic("neko_pygame: out of memory");
    canvas = s;
    return s;
}

fn present_flip() void {
    const s: *Surface = canvas orelse return;
    if (!logical_ready) {
        neko.screen.set_logical_size(s.w, s.h);
        canvas_tex = neko.texture.create(s.w, s.h);
        logical_ready = true;
    }
    const tex: neko.TextureHandle = canvas_tex orelse return;
    neko.texture.update(tex, std.mem.sliceAsBytes(s.pixels), s.w * 4);
    neko.texture.draw(tex, neko.Rect.init(0, 0, @intCast(s.w), @intCast(s.h)), null, null);
}

/// Creates the canvas and remembers it; returns it so you can draw right away.
pub fn set_mode(w: u32, h: u32) *Surface {
    return create_mode(w, h);
}

/// The canvas returned by `set_mode`.
pub fn surface() *Surface {
    return canvas orelse @panic("neko_pygame: call set_mode() first");
}

/// Uploads the canvas to the GPU and scales it to the window (pygame `flip`).
pub fn flip() void {
    present_flip();
}

/// `pygame.display` — the same calls under the pygame name.
pub const display: type = struct {
    pub fn set_mode(w: u32, h: u32) *Surface {
        return create_mode(w, h);
    }
    pub fn flip() void {
        present_flip();
    }
    pub fn get_surface() *Surface {
        return canvas orelse @panic("neko_pygame: call set_mode() first");
    }
};
