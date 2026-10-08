// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! PlayStation 1 backend (pure Zig, freestanding, no SDK).
//!
//! Implements the Neko `Backend` vtable on the PS1 hardware directly: the GPU
//! is driven through GP0/GP1, the framebuffer is uploaded to VRAM over the GPU
//! DMA channel and double buffered with vblank sync. This backend targets the
//! presenter path (CPU surfaces → screen), which is what the engine and the
//! games use; the engine's own primitives/text/sound are stubbed.

const std: type = @import("std");
const Allocator: type = std.mem.Allocator;
const engine: type = @import("neko");
const types: type = engine; // Color/Rect/Point/... are re-exported by `neko`

// ── Hardware registers ───────────────────────────────────────────────────────

const GP0: *volatile u32 = @ptrFromInt(0x1F801810);
const GP1: *volatile u32 = @ptrFromInt(0x1F801814);
const I_STAT: *volatile u32 = @ptrFromInt(0x1F801070);
const I_MASK: *volatile u32 = @ptrFromInt(0x1F801074);
const DMA_DPCR: *volatile u32 = @ptrFromInt(0x1F8010F0);
const DMA_MADR: *volatile u32 = @ptrFromInt(0x1F8010A0); // channel 2 (GPU)
const DMA_BCR: *volatile u32 = @ptrFromInt(0x1F8010A4);
const DMA_CHCR: *volatile u32 = @ptrFromInt(0x1F8010A8);

const W: u32 = 320;
const H: u32 = 240;

fn waitGpu() void {
    while ((GP1.* & 0x04000000) == 0) {}
}

fn vsync() void {
    while ((I_STAT.* & 1) == 0) {}
    I_STAT.* = 0xFFFFFFFE;
}

fn gpuInit() void {
    GP1.* = 0x00000000;
    GP1.* = 0x06000000 | 0x260 | (0xC60 << 12);
    GP1.* = 0x07000000 | 0x10 | (0x100 << 10);
    GP1.* = 0x08000001; // 320x240 NTSC 15bpp
    GP1.* = 0x05000000;
    GP1.* = 0x03000000; // display on
    waitGpu();
    GP0.* = 0xE1000200;
    GP0.* = 0xE3000000;
    GP0.* = 0xE4000000 | (239 << 10) | 319;
    GP0.* = 0xE5000000;

    DMA_DPCR.* |= 0x0800; // enable channel 2
    DMA_CHCR.* = 0;
    GP1.* = 0x04000000;
    I_MASK.* |= 1;
}

/// Uploads `pixels` (BGR555) to VRAM at (x,y) over DMA2, then flips/display.
fn vramUpload(x: u32, y: u32, w: u32, h: u32, pixels: []const u16) void {
    const words: u32 = (w * h) / 2;
    while ((DMA_CHCR.* & 0x01000000) != 0) {}
    GP1.* = 0x04000000;
    waitGpu();
    GP0.* = 0xA0000000;
    GP0.* = (y << 16) | x;
    GP0.* = (h << 16) | w;
    GP1.* = 0x04000002;
    DMA_MADR.* = @intCast(@intFromPtr(pixels.ptr) & 0x1FFFFFFF);
    DMA_BCR.* = 16 | ((words / 16) << 16);
    DMA_CHCR.* = 0x00000001 | 0x00000200 | 0x01000000;
    while ((DMA_CHCR.* & 0x01000000) != 0) {}
    GP1.* = 0x04000000;
}

fn rgb555(c: types.Color) u16 {
    return @as(u16, (c.b >> 3) & 0x1F) << 10 | @as(u16, (c.g >> 3) & 0x1F) << 5 | @as(u16, (c.r >> 3) & 0x1F);
}

// ── State ────────────────────────────────────────────────────────────────────

const MAX_TEX: usize = 4;

const PsxEngine: type = struct {
    allocator: Allocator = undefined,
    assets_dir: []u8 = &.{},
    running: bool = true,
    logical_w: u32 = W,
    logical_h: u32 = H,
    frames: u32 = 0,

    canvas: [W * H]u16 align(16) = undefined,
    shown: u32 = 0,

    tex_w: u32 = 0,
    tex_h: u32 = 0,
    tex_pixels: [320 * 240]u16 align(16) = undefined,

    pub fn backend(self: *PsxEngine) engine.Backend {
        return engine.Backend{ .ptr = @ptrCast(self), .vtable = &vtable };
    }
};

pub const Engine: type = PsxEngine;
var instance: PsxEngine = .{};

pub const kind: engine.BackendKind = .psx;

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
    // Diagnostic: cyan means screen.init() was reached.
    waitGpu();
    GP0.* = 0x02FFFF00;
    GP0.* = 0x00000000;
    GP0.* = (240 << 16) | 320;

    const self = asSelf(ptr);
    self.allocator = config.allocator;
    self.assets_dir = self.allocator.dupe(u8, config.assets_dir) catch &.{};
    self.logical_w = config.width;
    self.logical_h = config.height;
    gpuInit();
    for (0..W * H) |i| self.canvas[i] = rgb555(types.Color.black);
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
    const back = self.shown ^ 1;
    vramUpload(0, back * H, W, H, &self.canvas);
    vsync();
    GP1.* = 0x05000000 | ((back * H) << 10);
    GP1.* = 0x03000000;
    self.shown = back;
    self.frames += 1;
}

fn vtPollEvent(_: *anyopaque) ?engine.Event {
    return null; // no pad driver yet
}

fn vtTicksMs(ptr: *anyopaque) u64 {
    return @as(u64, asSelf(ptr).frames) * 16;
}

fn vtVoid(_: *anyopaque) void {}
fn vtSetTitle(_: *anyopaque, _: []const u8) void {}
fn vtBool(_: *anyopaque, _: bool) void {}
fn vtTrue(_: *anyopaque) bool {
    return true;
}
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

fn vtClear(ptr: *anyopaque, color: types.Color) void {
    const self = asSelf(ptr);
    const c = rgb555(color);
    for (0..W * H) |i| self.canvas[i] = c;
}

fn vtDrawRect(ptr: *anyopaque, rect: types.Rect, color: types.Color, _: bool) void {
    const self = asSelf(ptr);
    const c = rgb555(color);
    var y: i32 = rect.y;
    while (y < rect.y + rect.h) : (y += 1) {
        var x: i32 = rect.x;
        while (x < rect.x + rect.w) : (x += 1) setPixel(self, x, y, c);
    }
}

fn vtDrawLine(_: *anyopaque, _: i32, _: i32, _: i32, _: i32, _: types.Color) void {}
fn vtDrawCircle(_: *anyopaque, _: i32, _: i32, _: i32, _: types.Color, _: bool) void {}

fn setPixel(self: *PsxEngine, x: i32, y: i32, c: u16) void {
    if (x < 0 or y < 0 or x >= W or y >= H) return;
    self.canvas[@intCast(y * @as(i32, W) + x)] = c;
}

fn vtLoadTexture(_: *anyopaque, _: []const u8) ?types.TextureHandle {
    return null;
}

fn vtCreateTarget(_: *anyopaque, _: u32, _: u32) ?types.TextureHandle {
    return null;
}

fn vtCreateTexture(ptr: *anyopaque, width: u32, height: u32, _: ?[]const u8, _: u32) ?types.TextureHandle {
    const self = asSelf(ptr);
    self.tex_w = @min(width, 320);
    self.tex_h = @min(height, 240);
    return .{ .id = 1 };
}

fn vtUpdateTexture(ptr: *anyopaque, _: types.TextureHandle, pixels: []const u8, pitch: u32) void {
    const self = asSelf(ptr);
    const tw = self.tex_w;
    const th = self.tex_h;
    var y: u32 = 0;
    while (y < th) : (y += 1) {
        const row = y * pitch;
        var x: u32 = 0;
        while (x < tw) : (x += 1) {
            const o = row + x * 4;
            if (o + 2 >= pixels.len) break;
            const r: u16 = pixels[o + 2];
            const g: u16 = pixels[o + 1];
            const b: u16 = pixels[o];
            self.tex_pixels[y * 320 + x] = ((b >> 3) << 10) | ((g >> 3) << 5) | (r >> 3);
        }
    }
}

fn vtDrawTexture(ptr: *anyopaque, _: types.TextureHandle, _: types.Rect, _: ?types.Rect, _: ?u8) void {
    const self = asSelf(ptr);
    if (self.tex_w == 0 or self.tex_h == 0) return;
    var y: u32 = 0;
    while (y < H) : (y += 1) {
        const sy = y * self.tex_h / H;
        var x: u32 = 0;
        while (x < W) : (x += 1) {
            const sx = x * self.tex_w / W;
            self.canvas[y * W + x] = self.tex_pixels[sy * 320 + sx];
        }
    }
}

fn vtDrawTextureRotated(ptr: *anyopaque, tex: types.TextureHandle, dst: types.Rect, _: f32, alpha: ?u8) void {
    vtDrawTexture(ptr, tex, dst, null, alpha);
}

fn vtTextureSize(ptr: *anyopaque, _: types.TextureHandle) types.Point {
    const self = asSelf(ptr);
    return .{ .x = @intCast(self.tex_w), .y = @intCast(self.tex_h) };
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
//
// The PS1 has no `malloc` and no `std.process.Init`, so the game's `main(init)`
// is called with an `Init` we build over a fixed buffer allocator and a stub
// `Io`. `entry.zig` provides `_start` and calls `run(game.main)`.

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
    // Diagnostic: magenta means we reached the engine entry.
    gpuInit();
    waitGpu();
    GP0.* = 0x02FF00FF;
    GP0.* = 0x00000000;
    GP0.* = (240 << 16) | 320;
    GP1.* = 0x05000000;
    GP1.* = 0x03000000;

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
            waitGpu();
            GP0.* = 0x0200FF00; // green: about to enter the game
            GP0.* = 0x00000000;
            GP0.* = (240 << 16) | 320;
            invoke(main_fn(init));
        } else if (P == std.process.Init.Minimal) {
            invoke(main_fn(init.minimal));
        } else {
            @compileError("neko/psx: main must take std.process.Init, Init.Minimal, or no parameter");
        }
    }
}
