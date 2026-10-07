//! `engine.texture` — textures, offscreen targets and render targets.

const types: type = @import("../types.zig");
const context: type = @import("../context.zig");
const backend: type = @import("../backend.zig");

/// Loads a texture from `path` (PNG, …).
pub fn load(path: []const u8) ?types.TextureHandle {
    const e: backend.Backend = context.get() orelse return null;
    return e.load_texture(path);
}

/// Creates an offscreen render target of `width` x `height`.
pub fn create_target(width: u32, height: u32) ?types.TextureHandle {
    const e: backend.Backend = context.get() orelse return null;
    return e.create_target(width, height);
}

/// Creates a CPU-writable streaming texture of `width` x `height`.
///
/// Pixels use the backend's 32-bit format as packed ARGB (on little-endian
/// machines the byte order in memory is B, G, R, A). `pitch` is the number of
/// bytes per row and must be at least `width * 4`.
pub fn create(width: u32, height: u32) ?types.TextureHandle {
    const e: backend.Backend = context.get() orelse return null;
    return e.create_texture(width, height, null, width * 4);
}

/// As `create`, but uploads an initial pixel buffer.
pub fn fromPixels(width: u32, height: u32, pixels: []const u8, pitch: u32) ?types.TextureHandle {
    const e: backend.Backend = context.get() orelse return null;
    return e.create_texture(width, height, pixels, pitch);
}

/// Re-uploads `pixels` (packed ARGB, `pitch` bytes per row) into `tex`.
pub fn update(tex: types.TextureHandle, pixels: []const u8, pitch: u32) void {
    const e: backend.Backend = context.get() orelse return;
    e.update_texture(tex, pixels, pitch);
}

/// Draws `tex` into `dst`. `src` selects a sub-rectangle (null = whole
/// texture) and `alpha` overrides opacity (null = opaque).
pub fn draw(tex: types.TextureHandle, dst: types.Rect, src: ?types.Rect, alpha: ?u8) void {
    const e: backend.Backend = context.get() orelse return;
    e.draw_texture(tex, dst, src, alpha);
}

/// Draws `tex` into `dst` rotated by `angle` degrees.
pub fn draw_rotated(tex: types.TextureHandle, dst: types.Rect, angle: f32, alpha: ?u8) void {
    const e: backend.Backend = context.get() orelse return;
    e.draw_texture_rotated(tex, dst, angle, alpha);
}

/// Returns the pixel size of `tex`, or (0, 0) if unknown.
pub fn size(tex: types.TextureHandle) types.Point {
    const e: backend.Backend = context.get() orelse return types.Point{ .x = 0, .y = 0 };
    return e.texture_size(tex);
}

/// Draws a textured mesh: `vertices` carry screen position, UV and color;
/// `indices` form triangles. Use it for warps/composites the flat quad can't do.
pub fn geometry(tex: types.TextureHandle, vertices: []const types.Vertex, indices: []const i32) void {
    const e: backend.Backend = context.get() orelse return;
    e.geometry(tex, vertices, indices);
}

/// Redirects all drawing into `target`. Pass null to draw to the window.
pub fn set_target(target: ?types.TextureHandle) void {
    const e: backend.Backend = context.get() orelse return;
    e.set_render_target(target);
}

/// Stops drawing into an offscreen target.
pub fn reset_target() void {
    const e: backend.Backend = context.get() orelse return;
    e.set_render_target(null);
}
