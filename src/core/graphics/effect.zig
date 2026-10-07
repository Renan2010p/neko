//! `neko.effect` — full-screen visual effects.
//!
//! The port of the effect half of the C++ `DrawUtils`: camera noise, scanlines,
//! vignette, a solid tone overlay and a VHS-style OSD.

const types: type = @import("../base/types.zig");
const draw_mod: type = @import("draw.zig");
const sprite: type = @import("sprite.zig");
const screen: type = @import("../system/screen.zig");
const random: type = @import("../system/random.zig");

/// A faint grey flicker over the given rectangle. Skipped most frames, so it
/// reads as TV static. Lower quality halves the strength.
pub fn noise(x: i32, y: i32, w: i32, h: i32, intensity: f32) void {
    var chance: f32 = intensity;
    if (!sprite.is_high_quality()) {
        chance *= 0.55;
    }
    if (random.probability(chance)) {
        draw_mod.rect(types.Rect{ .x = x, .y = y, .w = w, .h = h }, types.Color{ .r = 100, .g = 100, .b = 100, .a = 20 }, true);
    }
}

/// Horizontal scanlines over the given rectangle.
pub fn scanlines(x: i32, y: i32, w: i32, h: i32, alpha: u8) void {
    var a: u8 = alpha;
    var step: i32 = 4;
    if (!sprite.is_high_quality()) {
        a = @intFromFloat(@as(f32, @floatFromInt(a)) * 0.6);
        step = 6;
    } else if (alpha < 20) {
        step = 5;
    }

    const color: types.Color = types.Color{ .r = 0, .g = 0, .b = 0, .a = a };
    var py: i32 = y;
    while (py <= y + h) : (py += step) {
        draw_mod.rect(types.Rect{ .x = x, .y = py, .w = w, .h = 1 }, color, true);
    }
}

/// Outlines fading toward the screen edges.
pub fn vignette(intensity_in: f32, color: types.Color) void {
    const intensity: f32 = @max(0.0, @min(1.0, intensity_in));
    const layers: i32 = if (sprite.is_high_quality()) 22 else 10;
    const max_a: u8 = @intFromFloat(150.0 * intensity);

    const size: types.Point = screen.logical_size();
    var i: i32 = 1;
    while (i <= layers) : (i += 1) {
        const t: f32 = @as(f32, @floatFromInt(i)) / @as(f32, @floatFromInt(layers));
        const inset: i32 = @intFromFloat(@floor(t * 64.0));
        const a: u8 = @intFromFloat((1.0 - t) * @as(f32, @floatFromInt(max_a)));
        draw_mod.rect(
            types.Rect{ .x = inset, .y = inset, .w = size.x - inset * 2, .h = size.y - inset * 2 },
            types.Color{ .r = color.r, .g = color.g, .b = color.b, .a = a },
            false,
        );
    }
}

/// A solid tint over the whole screen. No-op when fully transparent.
pub fn tone(color: types.Color) void {
    if (color.a == 0) return;
    const size: types.Point = screen.logical_size();
    draw_mod.rect(types.Rect{ .x = 0, .y = 0, .w = size.x, .h = size.y }, color, true);
}

/// The classic VHS on-screen text: a dark copy behind a colored one.
pub fn vhs(str: []const u8, x: i32, y: i32, color: types.Color, scale: u32) void {
    sprite.text(str, x + 1, y + 1, sprite.Options{
        .size = scale,
        .color = types.Color{ .r = 0, .g = 0, .b = 0 },
    });
    sprite.text(str, x, y, sprite.Options{ .size = scale, .color = color });
}

/// The security-camera look: scanlines plus noise over the whole screen.
pub fn camera(noise_intensity: f32, scanline_alpha: u8, scanline_spacing: u32) void {
    _ = scanline_spacing;

    const size: types.Point = screen.logical_size();
    scanlines(0, 0, size.x, size.y, scanline_alpha);
    noise(0, 0, size.x, size.y, noise_intensity);
}
