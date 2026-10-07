//! `neko.splash` — a boot splash for games made with Neko.
//!
//! Shows the **NEKO** wordmark with a "LOADING" label and a progress bar,
//! Godot-style. It is completely asset-free: the wordmark is drawn with engine
//! primitives and the label uses a tiny 5x7 bitmap font built into the engine,
//! so it renders even before any game asset (or font) exists.
//!
//! `neko.run` and `neko.app.run` show it automatically before the first frame
//! (disable with `.splash = null`). With the explicit `neko.window` API, call
//! `window.splash(.{})`. For real asset loading, drive `draw` yourself:
//!
//!     while (loading) {
//!         loadNextAsset();
//!         neko.splash.draw(loadedCount / total, .{});
//!         neko.screen.present();
//!     }

const std: type = @import("std");
const types: type = @import("../types.zig");
const draw_mod: type = @import("../graphics/draw.zig");
const screen: type = @import("screen.zig");
const input: type = @import("input.zig");
const time: type = @import("time.zig");

/// The default wordmark blue (`Color.hex` cannot be used in a struct default,
/// so it is spelled out).
const default_fg: types.Color = .{ .r = 0x66, .g = 0xcc, .b = 0xff };

/// Tuning for the splash.
pub const Options: type = struct {
    /// Minimum time the splash stays on screen, in milliseconds.
    duration_ms: u32 = 1400,
    /// Any key, mouse button or wheel skips it.
    skippable: bool = true,
    /// Background colour.
    bg: types.Color = types.Color{ .r = 12, .g = 12, .b = 18 },
    /// Wordmark colour (the NEKO blue by default).
    fg: types.Color = default_fg,
    /// Accent colour for the progress fill and the "loading" label.
    accent: types.Color = types.Color{ .r = 240, .g = 244, .b = 255 },
    /// Draw the progress bar at the bottom.
    progress_bar: bool = true,
    /// Label under the wordmark. Set to "" to hide it.
    label: []const u8 = "loading",
};

/// Draws one splash frame for `progress` in `0..1`. The caller presents.
pub fn draw(progress: f32, options: Options) void {
    const size: types.Point = screen.logical_size();
    const w: i32 = size.x;
    const h: i32 = size.y;
    if (w <= 0 or h <= 0) return;

    const p: f32 = clamp01(progress);
    const alpha: f32 = fadeAlpha(p);
    const a: u8 = @intFromFloat(alpha * 255.0);

    const fg: types.Color = options.fg.withAlpha(a);
    const accent: types.Color = options.accent.withAlpha(a);

    draw_mod.clear(options.bg);

    // ── Wordmark ─────────────────────────────────────────────────────────
    const word: *const [4:0]u8 = "NEKO";
    const col_units: i32 = wordColumns(word);
    const word_scale: i32 = @max(2, @divTrunc(@as(i32, @intFromFloat(@as(f32, @floatFromInt(w)) * 0.55)), col_units));
    const word_w: i32 = col_units * word_scale;
    const word_x: i32 = @divTrunc(w - word_w, 2);
    const word_y: i32 = @divTrunc(h, 2) - 7 * word_scale - word_scale;
    drawText(word, word_x + word_scale, word_y + word_scale, word_scale, types.Color{ .r = 0, .g = 0, .b = 0, .a = @divTrunc(a, 3) });
    drawText(word, word_x, word_y, word_scale, fg);

    // ── Label with animated dots ─────────────────────────────────────────
    if (options.label.len > 0) {
        const dots: usize = @as(usize, @intCast(@divTrunc(time.ticks_ms(), 350) % 4));
        const gap: i32 = word_scale;
        const label_scale: i32 = @max(1, @divTrunc(word_scale, 3));

        var buf: [48]u8 = undefined;
        var n: usize = 0;
        for (options.label) |ch| {
            if (n < buf.len) {
                buf[n] = ch;
                n += 1;
            }
        }
        var d: usize = 0;
        while (d < dots and n < buf.len) : (d += 1) {
            buf[n] = '.';
            n += 1;
        }

        const text: []u8 = buf[0..n];
        const label_w: i32 = textWidth(text, label_scale);
        drawText(text, @divTrunc(w - label_w, 2), word_y + 8 * word_scale + gap, label_scale, accent);
    }

    // ── Progress bar ─────────────────────────────────────────────────────
    if (options.progress_bar) {
        const bar_w: i32 = @intFromFloat(@as(f32, @floatFromInt(w)) * 0.6);
        const bar_h: i32 = @max(4, @divTrunc(word_scale, 3));
        const bar_x: i32 = @divTrunc(w - bar_w, 2);
        const bar_y: i32 = h - bar_h - @max(16, @divTrunc(h, 10));

        draw_mod.rect(.{ .x = bar_x, .y = bar_y, .w = bar_w, .h = bar_h }, fg.withAlpha(@divTrunc(a, 5)), true);
        draw_mod.rect(.{ .x = bar_x, .y = bar_y, .w = @intFromFloat(@as(f32, @floatFromInt(bar_w)) * p), .h = bar_h }, accent, true);
        draw_mod.rect(.{ .x = bar_x, .y = bar_y, .w = bar_w, .h = bar_h }, fg.withAlpha(@divTrunc(a, 2)), false);

        // Percentage, right-aligned above the bar.
        var pct_buf: [8]u8 = undefined;
        const pct: []u8 = std.fmt.bufPrint(&pct_buf, "{d}%", .{@as(u32, @intFromFloat(p * 100.0 + 0.5))}) catch return;
        const pct_scale: i32 = @max(1, @divTrunc(word_scale, 4));
        drawText(pct, bar_x + bar_w - textWidth(pct, pct_scale), bar_y - 8 * pct_scale - 4, pct_scale, fg.withAlpha(@divTrunc(a, 2)));
    }
}

/// Runs a timed, blocking splash. Requires an initialized window
/// (`neko.screen.init` or `neko.window.create`). Returns early on `.quit`.
pub fn show(options: Options) void {
    const start: u64 = time.ticks_ms();
    const duration: u64 = options.duration_ms;

    while (true) {
        input.beginFrame();

        var skip: bool = false;
        while (input.poll_event()) |ev| {
            switch (ev) {
                .quit => {
                    input.endFrame();
                    return;
                },
                .key_down, .mouse_button_down, .mouse_wheel => if (options.skippable) {
                    skip = true;
                },
                else => {},
            }
        }

        const elapsed: u64 = time.ticks_ms() -% start;
        const p: f32 = if (duration == 0)
            1.0
        else
            @min(1.0, @as(f32, @floatFromInt(elapsed)) / @as(f32, @floatFromInt(duration)));

        draw(p, options);
        screen.present();
        input.endFrame();

        if (skip or elapsed >= duration) break;
    }
}

// ── Internals ────────────────────────────────────────────────────────────────

fn clamp01(v: f32) f32 {
    return @max(0.0, @min(1.0, v));
}

fn fadeAlpha(p: f32) f32 {
    const fade_in: f32 = 0.15;
    const fade_out: f32 = 0.85;
    if (p < fade_in) return p / fade_in;
    if (p > fade_out) return (1.0 - p) / (1.0 - fade_out);
    return 1.0;
}

/// Columns a string occupies at scale 1 (glyph = 5, gap = 1).
fn wordColumns(text: []const u8) i32 {
    if (text.len == 0) return 0;
    return @as(i32, @intCast(text.len)) * 6 - 1;
}

fn textWidth(text: []const u8, scale: i32) i32 {
    if (text.len == 0) return 0;
    return wordColumns(text) * scale;
}

/// Draws `text` with the built-in 5x7 font. Unknown characters are skipped.
fn drawText(text: []const u8, x: i32, y: i32, scale: i32, color: types.Color) void {
    if (scale <= 0) return;
    var pen: i32 = x;
    for (text) |ch| {
        const upper: u8 = std.ascii.toUpper(ch);
        if (glyph(upper)) |rows| {
            for (rows.*, 0..) |row, ry| {
                var col: u3 = 0;
                while (col < 5) : (col += 1) {
                    const bit: u8 = @as(u8, 1) << @intCast(4 - @as(u8, col));
                    if ((row & bit) != 0) {
                        draw_mod.rect(.{
                            .x = pen + @as(i32, col) * scale,
                            .y = y + @as(i32, @intCast(ry)) * scale,
                            .w = scale,
                            .h = scale,
                        }, color, true);
                    }
                }
            }
        }
        pen += 6 * scale;
    }
}

// ── Built-in 5x7 font ────────────────────────────────────────────────────────
// Each glyph is 7 rows; the low 5 bits of a row are columns 0..4 (bit 4 = left).

const G_N: [7]u8 = [7]u8{ 0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001 };
const G_E: [7]u8 = [7]u8{ 0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111 };
const G_K: [7]u8 = [7]u8{ 0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001 };
const G_O: [7]u8 = [7]u8{ 0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110 };
const G_L: [7]u8 = [7]u8{ 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111 };
const G_A: [7]u8 = [7]u8{ 0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001 };
const G_D: [7]u8 = [7]u8{ 0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110 };
const G_I: [7]u8 = [7]u8{ 0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b11111 };
const G_G: [7]u8 = [7]u8{ 0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01110 };
const G_0: [7]u8 = [7]u8{ 0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110 };
const G_1: [7]u8 = [7]u8{ 0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110 };
const G_2: [7]u8 = [7]u8{ 0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111 };
const G_3: [7]u8 = [7]u8{ 0b11111, 0b00010, 0b00100, 0b00010, 0b00001, 0b10001, 0b01110 };
const G_4: [7]u8 = [7]u8{ 0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010 };
const G_5: [7]u8 = [7]u8{ 0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110 };
const G_6: [7]u8 = [7]u8{ 0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110 };
const G_7: [7]u8 = [7]u8{ 0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000 };
const G_8: [7]u8 = [7]u8{ 0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110 };
const G_9: [7]u8 = [7]u8{ 0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100 };
const G_PCT: [7]u8 = [7]u8{ 0b11001, 0b11010, 0b00010, 0b00100, 0b01000, 0b01011, 0b10011 };

fn glyph(c: u8) ?*const [7]u8 {
    return switch (c) {
        'N' => &G_N,
        'E' => &G_E,
        'K' => &G_K,
        'O' => &G_O,
        'L' => &G_L,
        'A' => &G_A,
        'D' => &G_D,
        'I' => &G_I,
        'G' => &G_G,
        '0' => &G_0,
        '1' => &G_1,
        '2' => &G_2,
        '3' => &G_3,
        '4' => &G_4,
        '5' => &G_5,
        '6' => &G_6,
        '7' => &G_7,
        '8' => &G_8,
        '9' => &G_9,
        '%' => &G_PCT,
        else => null,
    };
}
