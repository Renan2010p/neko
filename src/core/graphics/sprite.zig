//! `neko.sprite` — named sprites, cached fonts, text and simple widgets.
//!
//! This is the port of the C++ `DrawUtils`. Sprites are loaded by name from
//! `<assets_dir>/<name>.png` and cached. Text picks a font by size and caches
//! it, so drawing text every frame is cheap.

const std: type = @import("std");
const mem: type = std.mem;
const ascii: type = std.ascii;
const fmt: type = std.fmt;

const types: type = @import("../base/types.zig");
const context: type = @import("../base/context.zig");
const draw_mod: type = @import("draw.zig");
const texture: type = @import("texture.zig");
const text_mod: type = @import("text.zig");

// ── State ────────────────────────────────────────────────────────────────

const SpriteMap: type = std.StringHashMapUnmanaged(types.TextureHandle);
const FontMap: type = std.AutoHashMapUnmanaged(u32, i64);

var sprites: SpriteMap = .empty;
var fonts: FontMap = .empty;
var main_font: i64 = -1;
var render_quality: []const u8 = "high";

// ── Sprites ──────────────────────────────────────────────────────────────

/// Loads `<assets_dir>/<name>.png` into the sprite cache. No-op if already
/// loaded.
pub fn load(name: []const u8) void {
    if (sprites.contains(name)) return;

    var buf: [512]u8 = undefined;
    const path: []u8 = fmt.bufPrint(&buf, "{s}/{s}.png", .{ context.assets_dir, name }) catch return;

    const handle: types.TextureHandle = texture.load(path) orelse return;
    const key: []u8 = context.allocator.dupe(u8, name) catch return;
    sprites.put(context.allocator, key, handle) catch {
        context.allocator.free(key);
    };
}

/// Returns a cached sprite, or null if it was never loaded.
pub fn get(name: []const u8) ?types.TextureHandle {
    return sprites.get(name);
}

/// Draws a cached sprite. If it is missing, draws a grey placeholder with the
/// uppercased name.
pub fn draw(name: []const u8, x: i32, y: i32, w: i32, h: i32) void {
    if (get(name)) |tex| {
        texture.draw(tex, types.Rect{ .x = x, .y = y, .w = w, .h = h }, null, null);
        return;
    }

    draw_mod.rect(types.Rect{ .x = x, .y = y, .w = w, .h = h }, types.Color{ .r = 100, .g = 100, .b = 100 }, true);

    var upper: [128]u8 = undefined;
    const n: usize = @min(name.len, upper.len);
    for (name[0..n], 0..) |ch, i| {
        upper[i] = ascii.toUpper(ch);
    }
    text(upper[0..n], x + @divTrunc(w, 2), y + @divTrunc(h, 2), Options{ .size = 14, .center = true });
}

/// Draws a cached sprite only if it exists (no placeholder).
pub fn face(name: []const u8, x: i32, y: i32, w: i32, h: i32) void {
    if (get(name)) |tex| {
        texture.draw(tex, types.Rect{ .x = x, .y = y, .w = w, .h = h }, null, null);
    }
}

// ── Text ─────────────────────────────────────────────────────────────────

/// Optional parameters for the text functions. This is the same type as
/// `neko.text.Options`, so the two APIs stay interchangeable.
pub const Options: type = text_mod.Options;

/// Loads the shared default font (size 16) so text has a fallback.
pub fn load_default_font() void {
    var buf: [512]u8 = undefined;
    const path: []u8 = fmt.bufPrint(&buf, "{s}/font/font.ttf", .{context.assets_dir}) catch return;

    main_font = text_mod.load_font(path, 16);
    if (main_font == -1) main_font = 0;
    fonts.put(context.allocator, 16, main_font) catch {};
}

/// Returns (loading if needed) a font at `pixel_size`.
fn font_by_size(pixel_size: u32) i64 {
    if (fonts.get(pixel_size)) |idx| return idx;

    var buf: [512]u8 = undefined;
    const path: []u8 = fmt.bufPrint(&buf, "{s}/font/font.ttf", .{context.assets_dir}) catch return main_font;

    const idx: i64 = text_mod.load_font(path, @intCast(pixel_size));
    if (idx == -1) return main_font;
    fonts.put(context.allocator, pixel_size, idx) catch {};
    return idx;
}

/// Draws text, resolving the font by size (and caching it).
pub fn text(str: []const u8, x: i32, y: i32, options: Options) void {
    const font_idx: i64 = if (options.font >= 0) options.font else font_by_size(options.size);
    text_mod.draw(str, x, y, text_mod.Options{
        .size = options.size,
        .color = options.color,
        .center = options.center,
        .font = font_idx,
    });
}

/// Draws text rotated by `angle` degrees.
pub fn text_rotated(str: []const u8, x: i32, y: i32, angle: f32, options: Options) void {
    const font_idx: i64 = if (options.font >= 0) options.font else font_by_size(options.size);
    text_mod.draw_rotated(str, x, y, angle, text_mod.Options{
        .size = options.size,
        .color = options.color,
        .center = options.center,
        .font = font_idx,
    });
}

// ── Widgets ──────────────────────────────────────────────────────────────

/// A labelled box that is tinted when `active`.
pub fn button(x: i32, y: i32, w: i32, h: i32, label: []const u8, active: bool, on_color: types.Color, off_color: types.Color) void {
    const color: types.Color = if (active) on_color else off_color;
    const box: types.Rect = types.Rect{ .x = x, .y = y, .w = w, .h = h };

    draw_mod.rect(box, color, true);
    draw_mod.rect(box, types.Color{ .r = 255, .g = 255, .b = 255 }, false);
    text(label, x + @divTrunc(w, 2), y + @divTrunc(h, 2), Options{ .size = 14, .center = true });
}

// ── Render quality ───────────────────────────────────────────────────────

/// Returns "high" or "low".
pub fn quality() []const u8 {
    return render_quality;
}

/// True when the render quality is "high".
pub fn is_high_quality() bool {
    return mem.eql(u8, render_quality, "high");
}

/// Sets the render quality; anything other than "high" becomes "low".
pub fn set_quality(level: []const u8) void {
    render_quality = if (mem.eql(u8, level, "high")) "high" else "low";
}

// ── Cache lifetime ───────────────────────────────────────────────────────

/// Drops cached sprites and fonts. Called by the backend when textures are
/// invalidated (e.g. a vsync change recreates the renderer).
pub fn clear_cache() void {
    var it: SpriteMap.Iterator = sprites.iterator();
    while (it.next()) |entry| {
        context.allocator.free(entry.key_ptr.*);
    }
    sprites.clearRetainingCapacity();
    fonts.clearRetainingCapacity();
}

/// Frees everything. Called by `neko.screen.shutdown`.
pub fn deinit() void {
    clear_cache();
    sprites.deinit(context.allocator);
    fonts.deinit(context.allocator);
}
