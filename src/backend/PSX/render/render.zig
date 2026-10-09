// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! PlayStation 1 backend (pure Zig, freestanding, no SDK).
//!
//! Rendering is done by the GPU through an ordering table (`hw.zig`): fills,
//! flat quads and 4bpp CLUT-textured quads are accumulated during the frame and
//! streamed to the GPU with a single DMA transfer on `present`. Textures live
//! in VRAM as compressed 4bpp CLUT images, which is what keeps the bandwidth
//! low enough for 60 Hz on the R3000A.

const std: type = @import("std");
const Allocator: type = std.mem.Allocator;
const engine: type = @import("neko");
const types: type = engine; // Color/Rect/Point/... are re-exported by `neko`
const platform: type = @import("neko_psx_platform");
const hw: type = platform.hw;

const W = hw.W;
const H = hw.H;

// ── Texture registry ─────────────────────────────────────────────────────────

const MAX_TEX: usize = 16;

const TexInfo: type = struct {
    vram_x: u32 = 0,
    vram_y: u32 = 0,
    w: u32 = 0,
    h: u32 = 0,
    u_off: u32 = 0,
    tpage: u32 = 0,
    clut: u32 = 0,
    valid: bool = false,
};

var texs: [MAX_TEX]TexInfo = undefined;
var tex_count: usize = 0;
var tex_pixels: [4096]u16 align(16) = undefined;

/// A fixed 16-colour palette (BGR555) used to quantise 4bpp CLUT textures.
var palette: [16]u16 = undefined;
var pal_rgb: [16][3]i32 = undefined;
var z_counter: u32 = 0;

fn bgr555(r: u32, g: u32, b: u32) u16 {
    return @intCast(((b >> 3) & 0x1F) << 10 | ((g >> 3) & 0x1F) << 5 | ((r >> 3) & 0x1F));
}

fn initPalette() void {
    const cols = [16][3]u32{
        .{ 0, 0, 0 },    .{ 48, 48, 56 },   .{ 112, 112, 120 }, .{ 240, 240, 240 },
        .{ 96, 24, 20 }, .{ 200, 48, 40 },  .{ 24, 80, 32 },    .{ 72, 200, 80 },
        .{ 24, 32, 96 }, .{ 56, 88, 224 },  .{ 40, 200, 220 },  .{ 24, 96, 112 },
        .{ 96, 64, 32 }, .{ 232, 152, 48 }, .{ 200, 64, 180 },  .{ 240, 216, 96 },
    };
    for (0..16) |i| {
        palette[i] = bgr555(cols[i][0], cols[i][1], cols[i][2]);
        pal_rgb[i] = .{ @intCast(cols[i][0]), @intCast(cols[i][1]), @intCast(cols[i][2]) };
    }
}

fn nearestPalette(r: u32, g: u32, b: u32) u32 {
    var best: u32 = 0;
    var bd: i32 = 1 << 30;
    for (0..16) |i| {
        const dr = @as(i32, @intCast(r)) - pal_rgb[i][0];
        const dg = @as(i32, @intCast(g)) - pal_rgb[i][1];
        const db = @as(i32, @intCast(b)) - pal_rgb[i][2];
        const d = dr * dr + dg * dg + db * db;
        if (d < bd) {
            bd = d;
            best = @intCast(i);
        }
    }
    return best;
}

fn nextZ() u32 {
    const b = @min(z_counter, hw.OT_LEN - 1);
    z_counter += 1;
    return b;
}

// ── State ────────────────────────────────────────────────────────────────────

const PsxEngine: type = struct {
    allocator: Allocator = undefined,
    assets_dir: []u8 = &.{},
    running: bool = true,
    logical_w: u32 = W,
    logical_h: u32 = H,
    frames: u32 = 0,

    pub fn backend(self: *PsxEngine) engine.Backend {
        return engine.Backend{ .ptr = @ptrCast(self), .vtable = &vtable };
    }
};

pub const Engine: type = PsxEngine;
var instance: PsxEngine = .{};

pub const kind: engine.BackendKind = .{ .name = "psx" };

pub fn create() engine.Backend {
    return instance.backend();
}

fn asSelf(ptr: *anyopaque) *PsxEngine {
    return @ptrCast(@alignCast(ptr));
}

// ── Vtable ───────────────────────────────────────────────────────────────────

const vtable: engine.Backend.VTable = .{
    .init = vtInit,
    .shutdown = vtShutdown,
    .keeps_running = vtKeepsRunning,
    .request_stop = vtRequestStop,
    .present = vtPresent,
    .poll_event = vtPollEvent,
    .ticks_ms = vtTicksMs,
    .set_title = vtSetTitle,
    .set_logical_size = vtSetLogicalSize,
    .set_fullscreen = vtBool,
    .set_vsync = vtBool,
    .set_resolution = vtSetResolution,
    .logical_size = vtLogicalSize,
    .display_modes = vtDisplayModes,
    .supports_curved_panorama = vtFalse,
    .supports_offscreen_targets = vtFalse,
    .set_draw_offset = vtDrawOffset,
    .clear = vtClear,
    .draw_rect = vtDrawRect,
    .draw_line = vtDrawLine,
    .draw_circle = vtDrawCircle,
    .load_texture = vtLoadTexture,
    .create_target = vtCreateTarget,
    .create_texture = vtCreateTexture,
    .update_texture = vtUpdateTexture,
    .draw_texture = vtDrawTexture,
    .draw_texture_rotated = vtDrawTextureRotated,
    .texture_size = vtTextureSize,
    .geometry = vtGeometry,
    .set_render_target = vtSetRenderTarget,
    .load_font = vtLoadFont,
    .draw_text = vtDrawText,
    .draw_text_rotated = vtDrawTextRotated,
    .text_size = vtTextSize,
    .load_sound = vtLoadSound,
    .play_sound = vtPlaySound,
    .stop_channel = vtStopChannel,
    .stop_all_sounds = vtVoid,
    .set_master_volume = vtVolume,
    .set_sfx_volume = vtVolume,
    .set_music_volume = vtVolume,
    .read_file = vtReadFile,
    .write_file = vtWriteFile,
    .delete_file = vtDeleteFile,
    .file_exists = vtFalseStr,
    .mouse_pos = vtMousePos,
    .update_discord = vtDiscord,
};

fn vtInit(ptr: *anyopaque, config: types.Config) bool {
    const self = asSelf(ptr);
    self.allocator = config.allocator;
    self.assets_dir = self.allocator.dupe(u8, config.assets_dir) catch &.{};
    self.logical_w = config.width;
    self.logical_h = config.height;
    hw.gpuInit();
    hw.dmaInit();
    initPalette();
    hw.otClear();
    hw.fill(0, 0, W, H, 0);
    hw.drawOT();
    engine.attach(self.backend());
    return true;
}

fn vtShutdown(ptr: *anyopaque) void {
    asSelf(ptr).running = false;
    engine.detach();
}

fn vtKeepsRunning(ptr: *anyopaque) bool {
    return asSelf(ptr).running;
}

fn vtRequestStop(ptr: *anyopaque) void {
    asSelf(ptr).running = false;
}

fn vtPresent(ptr: *anyopaque) void {
    const self = asSelf(ptr);
    hw.drawOT();
    hw.vsync();
    self.frames += 1;
}

fn vtPollEvent(_: *anyopaque) ?engine.Event {
    return null;
}

fn vtTicksMs(ptr: *anyopaque) u64 {
    return @as(u64, asSelf(ptr).frames) * 16;
}

fn vtVoid(_: *anyopaque) void {}
fn vtSetTitle(_: *anyopaque, _: []const u8) void {}
fn vtBool(_: *anyopaque, _: bool) void {}
fn vtFalse(_: *anyopaque) bool {
    return false;
}
fn vtDrawOffset(_: *anyopaque, _: i32, _: i32) void {}

fn vtSetLogicalSize(ptr: *anyopaque, w: u32, h: u32) void {
    const self = asSelf(ptr);
    self.logical_w = w;
    self.logical_h = h;
}

fn vtSetResolution(ptr: *anyopaque, w: u32, h: u32) void {
    const self = asSelf(ptr);
    self.logical_w = w;
    self.logical_h = h;
}

fn vtLogicalSize(ptr: *anyopaque) types.Point {
    const self = asSelf(ptr);
    return .{ .x = @intCast(self.logical_w), .y = @intCast(self.logical_h) };
}

fn vtDisplayModes(_: *anyopaque, _: Allocator) []types.DisplayMode {
    return &.{};
}

fn rgb24(c: types.Color) u32 {
    return (@as(u32, c.r) << 16) | (@as(u32, c.g) << 8) | @as(u32, c.b);
}

fn vtClear(_: *anyopaque, color: types.Color) void {
    // A clear starts a new frame: reset the display list, then fill the screen.
    hw.otClear();
    z_counter = 0;
    hw.fill(0, 0, W, H, rgb24(color));
}

fn vtDrawRect(_: *anyopaque, rect: types.Rect, color: types.Color, _: bool) void {
    hw.addFlatQuad(nextZ(), rect.x, rect.y, rect.w, rect.h, rgb24(color));
}

fn vtDrawLine(_: *anyopaque, _: i32, _: i32, _: i32, _: i32, _: types.Color) void {}
fn vtDrawCircle(_: *anyopaque, _: i32, _: i32, _: i32, _: types.Color, _: bool) void {}

fn vtLoadTexture(_: *anyopaque, _: []const u8) ?types.TextureHandle {
    return null;
}

fn vtCreateTarget(_: *anyopaque, _: u32, _: u32) ?types.TextureHandle {
    return null;
}

fn vtCreateTexture(_: *anyopaque, width: u32, height: u32, _: ?[]const u8, _: u32) ?types.TextureHandle {
    if (tex_count >= MAX_TEX or width == 0 or height == 0 or width > 64 or height > 256) return null;
    const i = tex_count;
    tex_count += 1;
    // Four 64px textures pack into one 64-word (256px) texture page.
    const page = 640 + (i / 4) * 64;
    const u_off = @as(u32, @intCast((i % 4) * 64));
    const clut_x = 640 + @as(u32, @intCast(i)) * 16;
    texs[i] = .{
        .vram_x = page + (u_off >> 2),
        .vram_y = 0,
        .w = width,
        .h = height,
        .u_off = u_off,
        .tpage = (page >> 6) & 0xF,
        .clut = (256 << 6) | ((clut_x >> 4) & 0x3F),
        .valid = true,
    };
    hw.uploadWords(clut_x, 256, 16, 1, &palette);
    return .{ .id = i + 1 };
}

fn vtUpdateTexture(_: *anyopaque, handle: types.TextureHandle, pixels: []const u8, pitch: u32) void {
    const i = handle.id - 1;
    if (i >= tex_count or !texs[i].valid) return;
    const t = texs[i];
    const words_per_row = (t.w + 3) / 4;
    var y: u32 = 0;
    while (y < t.h) : (y += 1) {
        var x: u32 = 0;
        while (x < t.w) : (x += 4) {
            var word: u16 = 0;
            var k: u32 = 0;
            while (k < 4) : (k += 1) {
                const px = x + k;
                var idx: u32 = 0;
                if (px < t.w) {
                    const o = y * pitch + px * 4;
                    if (o + 2 < pixels.len) {
                        idx = nearestPalette(pixels[o + 2], pixels[o + 1], pixels[o]);
                    }
                }
                word |= @as(u16, @intCast(idx & 0xF)) << @intCast(k * 4);
            }
            tex_pixels[y * words_per_row + (x >> 2)] = word;
        }
    }
    hw.uploadWords(t.vram_x, t.vram_y, words_per_row, t.h, tex_pixels[0 .. words_per_row * t.h]);
}

fn vtDrawTexture(
    _: *anyopaque,
    handle: types.TextureHandle,
    dst: types.Rect,
    src: ?types.Rect,
    _: ?u8,
) void {
    const i = handle.id - 1;
    if (i >= tex_count or !texs[i].valid) return;
    const t = texs[i];
    const sx: i32 = if (src) |s| s.x else 0;
    const sy: i32 = if (src) |s| s.y else 0;
    const sw: i32 = if (src) |s| s.w else @intCast(t.w);
    const sh: i32 = if (src) |s| s.h else @intCast(t.h);
    const tx0: u32 = t.u_off + @as(u32, @intCast(@max(0, sx)));
    const ty0: u32 = @intCast(@max(0, sy));
    const tx1: u32 = tx0 + @as(u32, @intCast(@max(0, sw)));
    const ty1: u32 = ty0 + @as(u32, @intCast(@max(0, sh)));
    hw.addTexQuad(
        nextZ(),
        .{ .{ dst.x, dst.y }, .{ dst.x + dst.w, dst.y }, .{ dst.x, dst.y + dst.h }, .{ dst.x + dst.w, dst.y + dst.h } },
        .{ .{ tx0, ty0 }, .{ tx1, ty0 }, .{ tx0, ty1 }, .{ tx1, ty1 } },
        t.clut,
        t.tpage,
    );
}

fn vtDrawTextureRotated(ptr: *anyopaque, tex: types.TextureHandle, dst: types.Rect, _: f32, alpha: ?u8) void {
    vtDrawTexture(ptr, tex, dst, null, alpha);
}

fn vtTextureSize(_: *anyopaque, handle: types.TextureHandle) types.Point {
    const i = handle.id - 1;
    if (i < tex_count and texs[i].valid) {
        return .{ .x = @intCast(texs[i].w), .y = @intCast(texs[i].h) };
    }
    return .{ .x = 0, .y = 0 };
}

fn vtGeometry(_: *anyopaque, _: types.TextureHandle, _: []const types.Vertex, _: []const i32) void {}
fn vtSetRenderTarget(_: *anyopaque, _: ?types.TextureHandle) void {}

fn vtLoadFont(_: *anyopaque, _: []const u8, _: u16) i64 {
    return -1;
}
fn vtDrawText(_: *anyopaque, _: []const u8, _: i32, _: i32, _: u32, _: types.Color, _: bool, _: i64) bool {
    return false;
}
fn vtDrawTextRotated(_: *anyopaque, _: []const u8, _: i32, _: i32, _: u32, _: f32, _: types.Color, _: bool, _: i64) bool {
    return false;
}
fn vtTextSize(_: *anyopaque, _: []const u8, _: u32) ?types.Point {
    return null;
}

fn vtLoadSound(_: *anyopaque, _: []const u8) ?types.SoundHandle {
    return null;
}
fn vtPlaySound(_: *anyopaque, _: types.SoundHandle, _: i32, _: i32) i32 {
    return -1;
}
fn vtStopChannel(_: *anyopaque, _: i32) void {}
fn vtVolume(_: *anyopaque, _: i32) void {}

fn vtReadFile(_: *anyopaque, _: Allocator, _: []const u8, _: []const u8, _: usize) ?[]u8 {
    return null;
}
fn vtWriteFile(_: *anyopaque, _: []const u8, _: []const u8, _: []const u8) bool {
    return false;
}
fn vtDeleteFile(_: *anyopaque, _: []const u8, _: []const u8) void {}
fn vtFalseStr(_: *anyopaque, _: []const u8, _: []const u8) bool {
    return false;
}

fn vtMousePos(_: *anyopaque) types.Point {
    return .{ .x = 0, .y = 0 };
}
fn vtDiscord(_: *anyopaque, _: []const u8, _: []const u8) void {}

// ── Entry support (freestanding) ─────────────────────────────────────────────

var gpa_buffer: [96 * 1024]u8 = undefined;

var io_stub_vtable: std.Io.VTable = undefined;

fn ioStubCrash(_: ?*anyopaque) void {
    while (true) {}
}

fn invoke(result: anytype) void {
    if (@typeInfo(@TypeOf(result)) == .error_union) result catch {};
}

/// Builds the `Init` the game expects and calls `main_fn`.
pub fn run(main_fn: anytype) void {
    var fba: std.heap.FixedBufferAllocator = std.heap.FixedBufferAllocator.init(&gpa_buffer);
    const gpa: Allocator = fba.allocator();
    var arena: std.heap.ArenaAllocator = std.heap.ArenaAllocator.init(gpa);
    var environ_map: std.process.Environ.Map = .{ .array_hash_map = .empty, .allocator = gpa };
    io_stub_vtable.crashHandler = ioStubCrash;

    const init: std.process.Init = .{
        .minimal = .{ .args = .{ .vector = {} }, .environ = std.process.Environ.empty },
        .arena = &arena,
        .gpa = gpa,
        .io = .{ .userdata = null, .vtable = &io_stub_vtable },
        .environ_map = &environ_map,
        .preopens = undefined,
    };

    const info: std.builtin.Type.Fn = @typeInfo(@TypeOf(main_fn)).@"fn";
    if (info.params.len == 0) {
        invoke(main_fn());
    } else {
        const P: type = info.params[0].type.?;
        if (P == std.process.Init) {
            invoke(main_fn(init));
        } else if (P == std.process.Init.Minimal) {
            invoke(main_fn(init.minimal));
        } else {
            @compileError("neko/psx: main must take std.process.Init, Init.Minimal, or no parameter");
        }
    }
}
