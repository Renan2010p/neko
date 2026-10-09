//! `engine.texture` — textures, offscreen targets and render targets.

const types: type = @import("../base/types.zig");
const context: type = @import("../base/context.zig");
const backend: type = @import("../base/backend.zig");
const camera: type = @import("camera2d.zig");
const log: type = @import("../system/log.zig");

/// Loads a texture from `path` (PNG, …).
pub fn load(path: []const u8) ?types.TextureHandle {
    const e: backend.Backend = context.get() orelse return null;
    const handle: ?types.TextureHandle = e.load_texture(path);
    if (handle == null) {
        log.warn("gpu: texture load '{s}' failed", .{path});
    } else {
        log.info("gpu: texture load '{s}'", .{path});
    }
    return handle;
}

/// Creates an offscreen render target of `width` x `height`.
pub fn create_target(width: u32, height: u32) ?types.TextureHandle {
    const e: backend.Backend = context.get() orelse return null;
    const handle: ?types.TextureHandle = e.create_target(width, height);
    if (handle == null) {
        log.warn("gpu: render target {d}x{d} failed", .{ width, height });
    } else {
        log.info("gpu: render target {d}x{d}", .{ width, height });
    }
    return handle;
}

/// Creates a CPU-writable streaming texture of `width` x `height`.
///
/// Pixels use the backend's 32-bit format as packed ARGB (on little-endian
/// machines the byte order in memory is B, G, R, A). `pitch` is the number of
/// bytes per row and must be at least `width * 4`.
pub fn create(width: u32, height: u32) ?types.TextureHandle {
    const e: backend.Backend = context.get() orelse return null;
    const handle: ?types.TextureHandle = e.create_texture(width, height, null, width * 4);
    if (handle == null) {
        log.warn("gpu: streaming texture {d}x{d} alloc failed", .{ width, height });
    } else {
        log.info("gpu: streaming texture {d}x{d} alloc ({d} bytes)", .{ width, height, width * height * 4 });
    }
    return handle;
}

/// As `create`, but uploads an initial pixel buffer.
pub fn fromPixels(width: u32, height: u32, pixels: []const u8, pitch: u32) ?types.TextureHandle {
    const e: backend.Backend = context.get() orelse return null;
    const handle: ?types.TextureHandle = e.create_texture(width, height, pixels, pitch);
    if (handle == null) {
        log.warn("gpu: texture upload {d}x{d} (pitch {d}) failed", .{ width, height, pitch });
    } else {
        log.info("gpu: texture upload {d}x{d} (pitch {d}, {d} bytes)", .{ width, height, pitch, pixels.len });
    }
    return handle;
}

/// Re-uploads `pixels` (packed ARGB, `pitch` bytes per row) into `tex`.
pub fn update(tex: types.TextureHandle, pixels: []const u8, pitch: u32) void {
    const e: backend.Backend = context.get() orelse return;
    log.debug("gpu: texture {d} update ({d} bytes, pitch {d})", .{ tex.id, pixels.len, pitch });
    e.update_texture(tex, pixels, pitch);
}

/// Draws `tex` into `dst`. `src` selects a sub-rectangle (null = whole
/// texture) and `alpha` overrides opacity (null = opaque).
pub fn draw(tex: types.TextureHandle, dst: types.Rect, src: ?types.Rect, alpha: ?u8) void {
    const e: backend.Backend = context.get() orelse return;
    e.draw_texture(tex, camera.applyRect(dst), src, alpha);
}

/// Draws `tex` into `dst` rotated by `angle` degrees.
pub fn draw_rotated(tex: types.TextureHandle, dst: types.Rect, angle: f32, alpha: ?u8) void {
    const e: backend.Backend = context.get() orelse return;
    e.draw_texture_rotated(tex, camera.applyRect(dst), angle, alpha);
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
