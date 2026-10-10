// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! PlayStation 2 backend — implements `neko.Backend` on top of gsKit + the
//! pad, using the hand-written bindings in `ps2sdk.zig` / `gskit.zig`.
//!
//! Game code never changes: `neko.screen.init`, `neko.draw.*`, `neko.text.*`,
//! `neko.input.*` and `neko.screen.present` all dispatch here. The interesting
//! part is `poll_event`, which maps the DualShock buttons onto the SAME raw key
//! codes SDL reports on the desktop (Up = 1073741906, Return = 13, 'w' = 119,
//! …), so games that compare `KeyEvent.code` keep working untouched.
//!
//! Rendering is one-shot: draw calls only enqueue gsKit primitives; `present`
//! runs the queue and flips (waiting for vblank by polling GS_CSR, so no EE
//! interrupt is needed).

const std: type = @import("std");
const engine: type = @import("neko");
const platform: type = @import("neko_ps2_platform");
const ps2: type = platform.ps2sdk;
const gskit: type = platform.gskit;

const Allocator: type = std.mem.Allocator;

/// Gamepad scratch area (256 bytes, 64-byte aligned, PS2SDK requirement).
var pad_area: [256]u8 align(64) = undefined;
var pad_status: ps2.pad.ButtonStatus = undefined;

/// Textures are stored in a fixed pool: no allocator needed for the GPU pool
/// and no failure modes at runtime. Bump this if a game needs more.
const MAX_TEXTURES: usize = 128;
const MAX_EVENTS: usize = 32;

const TextureEntry: type = struct {
    used: bool = false,
    id: u32 = 0,
    tex: gskit.GSTEXTURE = undefined,
    width: u32 = 0,
    height: u32 = 0,
};

/// Maps one pad button onto an SDL-compatible key code (so `KeyEvent.code`
/// matches what games already compare against on the desktop).
const ButtonMap: type = struct {
    bit: u16,
    code: i32,
    key: engine.Key,
    name: []const u8,
    scan: []const u8,
};

const button_map: [15]ButtonMap = [_]ButtonMap{
    .{ .bit = ps2.pad.BTN_UP, .code = 1073741906, .key = .up, .name = "Up", .scan = "Up" },
    .{ .bit = ps2.pad.BTN_DOWN, .code = 1073741905, .key = .down, .name = "Down", .scan = "Down" },
    .{ .bit = ps2.pad.BTN_LEFT, .code = 1073741904, .key = .left, .name = "Left", .scan = "Left" },
    .{ .bit = ps2.pad.BTN_RIGHT, .code = 1073741903, .key = .right, .name = "Right", .scan = "Right" },
    .{ .bit = ps2.pad.BTN_CROSS, .code = 13, .key = .enter, .name = "Return", .scan = "Return" },
    .{ .bit = ps2.pad.BTN_CIRCLE, .code = 27, .key = .escape, .name = "Escape", .scan = "Escape" },
    .{ .bit = ps2.pad.BTN_START, .code = 32, .key = .space, .name = "Space", .scan = "Space" },
    .{ .bit = ps2.pad.BTN_SELECT, .code = 9, .key = .tab, .name = "Tab", .scan = "Tab" },
    .{ .bit = ps2.pad.BTN_SQUARE, .code = 97, .key = .a, .name = "A", .scan = "A" },
    .{ .bit = ps2.pad.BTN_TRIANGLE, .code = 100, .key = .d, .name = "D", .scan = "D" },
    .{ .bit = ps2.pad.BTN_L1, .code = 113, .key = .q, .name = "Q", .scan = "Q" },
    .{ .bit = ps2.pad.BTN_R1, .code = 101, .key = .e, .name = "E", .scan = "E" },
    .{ .bit = ps2.pad.BTN_L2, .code = 108, .key = .l, .name = "L", .scan = "L" },
    .{ .bit = ps2.pad.BTN_R2, .code = 112, .key = .p, .name = "P", .scan = "P" },
    .{ .bit = ps2.pad.BTN_R3, .code = 119, .key = .w, .name = "W", .scan = "W" },
};

// ── FreeType text (font_shim.c) ─────────────────────────────────────────────
// The shim opens the game's TTF and rasterises one glyph at a time; we bake
// them into a T8 atlas and draw text as sprites. This gives the game's own
// typography and avoids gsKit's per-glyph clamp/test/primalpha churn.

const NekoGlyph: type = extern struct {
    width: c_int,
    height: c_int,
    left: c_int,
    top: c_int,
    advance: c_int,
    pitch: c_int,
    coverage: ?[*]const u8,
};

extern fn neko_font_open(path: [*c]const u8, pixel_size: c_int) c_int;
extern fn neko_font_close(slot: c_int) void;
extern fn neko_font_ascender(slot: c_int) c_int;
extern fn neko_font_descender(slot: c_int) c_int;
extern fn neko_font_glyph(slot: c_int, codepoint: c_uint, out: *NekoGlyph) c_int;

/// Printable ASCII range baked into each font atlas.
const ASCII_FIRST: u32 = 32;
const ASCII_LAST: u32 = 126;
const ASCII_COUNT: usize = ASCII_LAST - ASCII_FIRST + 1;

const MAX_FONTS: usize = 16;

/// Audio: OGGs are converted to WAV at build time; we load PCM and mix.
const MAX_SOUNDS: usize = 32;
const MAX_CHANNELS: usize = 16;
const MIX_RATE: u32 = 22050;
const MIX_SAMPLES: usize = 368; // ~one frame at 60 Hz (22050/60)

/// One glyph inside an atlas (pixel-space UV rect + metrics).
const Glyph: type = struct {
    u: u32 = 0,
    v: u32 = 0,
    w: u32 = 0,
    h: u32 = 0,
    left: i32 = 0,
    top: i32 = 0,
    advance: i32 = 0,
};

/// A loaded game font: a FreeType face plus its VRAM atlas (T8 + greyscale CLUT).
const FontSlot: type = struct {
    used: bool = false,
    shim: c_int = -1,
    pixel_size: u32 = 0,
    ascender: i32 = 0,
    descender: i32 = 0,
    atlas_w: u32 = 0,
    atlas_h: u32 = 0,
    atlas: ?[*]u8 = null,
    clut: ?[*]u32 = null,
    tex: gskit.GSTEXTURE = undefined,
    glyphs: [ASCII_COUNT]Glyph = undefined,
};

/// One loaded sound: 16-bit mono PCM decoded from a build-time WAV.
const SoundEntry: type = struct {
    used: bool = false,
    id: u32 = 0,
    pcm: ?[*]const i16 = null,
    frames: usize = 0,
};

/// One playing voice of the software mixer.
const PlayChannel: type = struct {
    active: bool = false,
    sound: u32 = 0,
    pos: usize = 0,
    volume: i32 = 100,
    loops: bool = false,
};

/// Concrete PS2 backend state. One instance per process.
pub const Ps2Engine: type = struct {
    gs: *gskit.GSGLOBAL = undefined,
    font: *gskit.GSFONTM = undefined,
    has_font: bool = false,

    assets_dir: []const u8 = "assets",
    running: bool = true,
    vsync: bool = true,

    logical_w: u32 = 640,
    logical_h: u32 = 448,
    draw_dx: i32 = 0,
    draw_dy: i32 = 0,

    /// Physical framebuffer size (read back from gsKit after init) and the
    /// logical→physical transform. Games draw in `logical` pixels; we scale to
    /// the real framebuffer preserving aspect (letterbox), exactly like
    /// `SDL_RenderSetLogicalSize` does.
    phys_w: u32 = 640,
    phys_h: u32 = 448,
    scale: f32 = 1.0,
    off_x: f32 = 0,
    off_y: f32 = 0,

    tick_base: u64 = 0,

    textures: [MAX_TEXTURES]TextureEntry = @splat(.{}),
    next_id: u32 = 1,

    fonts: [MAX_FONTS]FontSlot = @splat(.{}),

    sounds: [MAX_SOUNDS]SoundEntry = @splat(.{}),
    channels: [MAX_CHANNELS]PlayChannel = @splat(.{}),
    next_sound_id: u32 = 1,
    audio_ok: bool = false,
    audio_master: i32 = 80,
    audio_sfx: i32 = 100,
    audio_music: i32 = 70,
    mix_buf: [MIX_SAMPLES]i16 = undefined,

    pad_ok: bool = false,
    pad_prev: u16 = 0,
    pad_seen: bool = false,
    sample_input: bool = true,

    queue: [MAX_EVENTS]engine.Event = undefined,
    q_head: usize = 0,
    q_len: usize = 0,

    /// Wraps this backend into the type-erased `engine.Backend` handle.
    pub fn backend(self: *Ps2Engine) engine.Backend {
        return engine.Backend{ .ptr = @ptrCast(self), .vtable = &vtable };
    }
};

/// The concrete backend type, under the stable name `neko_backend.Engine`.
pub const Engine: type = Ps2Engine;

/// Process-wide backend instance, with a stable address for the handle.
var instance: Ps2Engine = .{};

/// Returns the backend as an abstract handle. Called by `src/core/platform.zig`,
/// the single core seam that knows this module.
pub fn create() engine.Backend {
    return instance.backend();
}

/// Which backend this module implements.
pub const kind: engine.BackendKind = .{ .name = "ps2" };

const vtable: engine.Backend.VTable = engine.Backend.VTable{
    .init = vt_init,
    .shutdown = vt_shutdown,
    .keeps_running = vt_keeps_running,
    .request_stop = vt_request_stop,
    .present = vt_present,
    .poll_event = vt_poll_event,
    .ticks_ms = vt_ticks_ms,
    .set_logical_size = vt_set_logical_size,
    .set_title = vt_set_title,
    .set_fullscreen = vt_set_fullscreen,
    .set_vsync = vt_set_vsync,
    .set_resolution = vt_set_resolution,
    .logical_size = vt_logical_size,
    .display_modes = vt_display_modes,
    .supports_curved_panorama = vt_supports_curved_panorama,
    .supports_offscreen_targets = vt_supports_offscreen_targets,
    .set_draw_offset = vt_set_draw_offset,
    .clear = vt_clear,
    .draw_rect = vt_draw_rect,
    .draw_line = vt_draw_line,
    .draw_circle = vt_draw_circle,
    .load_texture = vt_load_texture,
    .create_target = vt_create_target,
    .create_texture = vt_create_texture,
    .update_texture = vt_update_texture,
    .draw_texture = vt_draw_texture,
    .draw_texture_rotated = vt_draw_texture_rotated,
    .texture_size = vt_texture_size,
    .geometry = vt_geometry,
    .set_render_target = vt_set_render_target,
    .load_font = vt_load_font,
    .draw_text = vt_draw_text,
    .draw_text_rotated = vt_draw_text_rotated,
    .text_size = vt_text_size,
    .load_sound = vt_load_sound,
    .play_sound = vt_play_sound,
    .stop_channel = vt_stop_channel,
    .stop_all_sounds = vt_stop_all_sounds,
    .set_master_volume = vt_set_master_volume,
    .set_sfx_volume = vt_set_sfx_volume,
    .set_music_volume = vt_set_music_volume,
    .read_file = vt_read_file,
    .write_file = vt_write_file,
    .delete_file = vt_delete_file,
    .file_exists = vt_file_exists,
    .mouse_pos = vt_mouse_pos,
    .update_discord = vt_update_discord,
};

fn as_self(ptr: *anyopaque) *Ps2Engine {
    return @ptrCast(@alignCast(ptr));
}

/// Recomputes the logical→physical transform (uniform scale + centering).
fn updateTransform(self: *Ps2Engine) void {
    const lw: f32 = @floatFromInt(if (self.logical_w == 0) 640 else self.logical_w);
    const lh: f32 = @floatFromInt(if (self.logical_h == 0) 448 else self.logical_h);
    const pw: f32 = @floatFromInt(self.phys_w);
    const ph: f32 = @floatFromInt(self.phys_h);
    self.scale = @min(pw / lw, ph / lh);
    self.off_x = (pw - lw * self.scale) / 2.0;
    self.off_y = (ph - lh * self.scale) / 2.0;
}

/// Logical x (plus the game's draw offset) → physical framebuffer x.
fn sx(self: *Ps2Engine, x: i32) f32 {
    return sx_f(self, @floatFromInt(x + self.draw_dx));
}

/// Logical y → physical framebuffer y.
fn sy(self: *Ps2Engine, y: i32) f32 {
    return sy_f(self, @floatFromInt(y + self.draw_dy));
}

/// Float versions, for glyph positions already in logical space.
fn sx_f(self: *Ps2Engine, x: f32) f32 {
    return self.off_x + x * self.scale;
}

fn sy_f(self: *Ps2Engine, y: f32) f32 {
    return self.off_y + y * self.scale;
}

/// Builds a NUL-terminated device path for PS2 file access. PCSX2 serves
/// assets through `host:` (rooted at the ELF's directory); a real console uses
/// `cdrom0:`/`mass:`. Paths that already carry a device are left alone.
fn resolvePath(path: []const u8, buf: []u8) ?[]u8 {
    const prefix: []const u8 = if (std.mem.indexOfScalar(u8, path, ':') == null) "host:" else "";
    const total: usize = prefix.len + path.len;
    if (total >= buf.len) return null;
    @memcpy(buf[0..prefix.len], prefix);
    @memcpy(buf[prefix.len..total], path);
    buf[total] = 0;
    return buf[0..total];
}

fn nextPow2(n: u32) u32 {
    var p: u32 = 1;
    while (p < n) p <<= 1;
    return p;
}

// ── Colour helpers ────────────────────────────────────────────────────────

/// neko alpha is 0-255; the GS blend multiplies by alpha/128, so map 255 -> 128
/// (fully opaque) and 128 -> 64 (half). `gsKit_clear` also draws a sprite with
/// this alpha, so an opaque clear colour is essential.
fn ga(a: u8) u32 {
    return (@as(u32, a) * 128 + 127) / 255;
}

/// neko colour component (0-255) -> GS texture modulation factor (0-128).
/// Textured primitives multiply by `C/128`, so 255 would come out ~2x too
/// bright ("blown out"); 128 is neutral.
fn gc(c: u8) u32 {
    return (@as(u32, c) * 128 + 127) / 255;
}

fn color64(c: engine.Color) u64 {
    return gskit.reg.rgbaq(c.r, c.g, c.b, ga(c.a), 0);
}

fn color32(c: engine.Color) c_ulong {
    return gskit.reg.font(gc(c.r), gc(c.g), gc(c.b), ga(c.a));
}

// ── Text metrics ──────────────────────────────────────────────────────────

fn text_scale(size: u32) f32 {
    const s: f32 = @floatFromInt(size);
    return s / gskit.fontm.GLYPH;
}

fn text_width(engine_self: *Ps2Engine, text: []const u8, size: u32) f32 {
    _ = engine_self;
    const count: f32 = @floatFromInt(text.len);
    return gskit.fontm.GLYPH * count * text_scale(size);
}

// ── Lifecycle ─────────────────────────────────────────────────────────────

fn vt_init(ptr: *anyopaque, config: engine.Config) bool {
    const self: *Ps2Engine = as_self(ptr);
    self.assets_dir = config.assets_dir;

    // IOP services: SIO2MAN + PADMAN must be running before padInit.
    _ = ps2.sif.sceSifInitRpc(0);
    _ = ps2.loader.SifLoadModule(ps2.loader.SIO2MAN, 0, "");
    _ = ps2.loader.SifLoadModule(ps2.loader.PADMAN, 0, "");
    _ = ps2.pad.padInit(0);
    if (ps2.pad.padPortOpen(0, 0, &pad_area) != 0) self.pad_ok = true;

    // gsKit bring-up.
    self.gs = gskit.core.initGlobal();
    self.font = gskit.fontm.init();
    _ = gskit.dma.init();
    // A 2D game never needs the Z-buffer; freeing it gives the texture manager
    // ~0.5 MB more VRAM.
    gskit.global.setZBuffering(self.gs, false);
    gskit.core.initScreen(self.gs);
    self.has_font = gskit.fontm.upload(self.gs, self.font) == 0;
    gskit.core.modeSwitch(self.gs, gskit.ONESHOT);

    // Per-primitive alpha blending (text alpha included) is OFF by default in
    // gsKit; turn it on and set the standard src-over mode.
    gskit.global.setPrimAlphaEnable(self.gs, true);
    gskit.core.gsKit_set_primalpha(self.gs, gskit.reg.alpha(0, 1, 0, 1, 0), 0);

    // Read back the real framebuffer size gsKit picked.
    const pw: i32 = gskit.global.width(self.gs);
    const ph: i32 = gskit.global.height(self.gs);
    if (pw > 0) self.phys_w = @intCast(pw);
    if (ph > 0) self.phys_h = @intCast(ph);

    self.logical_w = config.width;
    self.logical_h = config.height;
    updateTransform(self);
    self.tick_base = ps2.timer.GetTimerSystemTime();
    self.running = true;
    self.vsync = config.vsync;

    // Wait for the pad to settle (never SleepThread — it never returns here).
    var tries: u32 = 0;
    while (tries < 400) : (tries += 1) {
        const state: c_int = ps2.pad.padGetState(0, 0);
        if (state == ps2.pad.STATE_STABLE or state == ps2.pad.STATE_FINDCTP1) break;
        const start: u64 = ps2.timer.GetTimerSystemTime();
        while (ps2.timer.GetTimerSystemTime() -% start < ps2.timer.ms(4)) {}
    }

    engine.attach(self.backend());
    return true;
}

fn vt_shutdown(ptr: *anyopaque) void {
    const self: *Ps2Engine = as_self(ptr);
    self.running = false;
    engine.detach();
}

fn vt_keeps_running(ptr: *anyopaque) bool {
    return as_self(ptr).running;
}

fn vt_request_stop(ptr: *anyopaque) void {
    as_self(ptr).running = false;
}

fn vt_present(ptr: *anyopaque) void {
    const self: *Ps2Engine = as_self(ptr);
    gskit.core.queueExec(self.gs);
    gskit.core.syncFlip(self.gs);
    gskit.texture.gsKit_TexManager_nextFrame(self.gs);
    mixAudio(self);
    self.sample_input = true;
}

// ── Events and timing ─────────────────────────────────────────────────────

fn push_event(self: *Ps2Engine, ev: engine.Event) void {
    if (self.q_len >= MAX_EVENTS) return;
    const idx: usize = (self.q_head + self.q_len) % MAX_EVENTS;
    self.queue[idx] = ev;
    self.q_len += 1;
}

fn sample_pad(self: *Ps2Engine) void {
    if (!self.pad_ok) return;
    if (ps2.pad.padRead(0, 0, &pad_status) == 0) return;

    var held: u16 = ps2.pad.held(&pad_status);
    // Left analog stick doubles as the D-pad.
    if (pad_status.ljoy_h < 64) held |= ps2.pad.BTN_LEFT else if (pad_status.ljoy_h > 192) held |= ps2.pad.BTN_RIGHT;
    if (pad_status.ljoy_v < 64) held |= ps2.pad.BTN_UP else if (pad_status.ljoy_v > 192) held |= ps2.pad.BTN_DOWN;

    // The very first sample only seeds the baseline. Otherwise every button
    // held (or a stick resting off-centre) while the game boots fires a
    // key_down, which skips the splash and jumps through menus.
    if (!self.pad_seen) {
        self.pad_seen = true;
        self.pad_prev = held;
        return;
    }

    const pressed: u16 = held & ~self.pad_prev;
    const released: u16 = ~held & self.pad_prev;
    self.pad_prev = held;

    inline for (button_map) |m| {
        if ((pressed & m.bit) != 0) {
            push_event(self, engine.Event{ .key_down = .{
                .code = m.code,
                .key = m.key,
                .name = m.name,
                .scan_name = m.scan,
            } });
        }
        if ((released & m.bit) != 0) {
            push_event(self, engine.Event{ .key_up = .{
                .code = m.code,
                .key = m.key,
                .name = m.name,
                .scan_name = m.scan,
            } });
        }
    }
}

fn vt_poll_event(ptr: *anyopaque) ?engine.Event {
    const self: *Ps2Engine = as_self(ptr);
    if (self.q_len == 0 and self.sample_input) {
        sample_pad(self);
        self.sample_input = false;
    }
    if (self.q_len == 0) return null;

    const ev: engine.Event = self.queue[self.q_head];
    self.q_head = (self.q_head + 1) % MAX_EVENTS;
    self.q_len -= 1;
    return ev;
}

fn vt_ticks_ms(ptr: *anyopaque) u64 {
    const self: *Ps2Engine = as_self(ptr);
    return ps2.timer.toUSec(ps2.timer.GetTimerSystemTime() -% self.tick_base) / 1000;
}

// ── Window ────────────────────────────────────────────────────────────────

fn vt_set_title(_: *anyopaque, _: []const u8) void {}

fn vt_set_logical_size(ptr: *anyopaque, width: u32, height: u32) void {
    const self: *Ps2Engine = as_self(ptr);
    self.logical_w = width;
    self.logical_h = height;
    updateTransform(self);
}

fn vt_set_fullscreen(ptr: *anyopaque, on: bool) void {
    _ = ptr;
    _ = on;
}

fn vt_set_vsync(ptr: *anyopaque, on: bool) void {
    as_self(ptr).vsync = on;
}

fn vt_set_resolution(ptr: *anyopaque, width: u32, height: u32) void {
    const self: *Ps2Engine = as_self(ptr);
    self.logical_w = width;
    self.logical_h = height;
    updateTransform(self);
}

fn vt_logical_size(ptr: *anyopaque) engine.Point {
    const self: *Ps2Engine = as_self(ptr);
    return engine.Point{ .x = @intCast(self.logical_w), .y = @intCast(self.logical_h) };
}

fn vt_display_modes(ptr: *anyopaque, allocator: Allocator) []engine.DisplayMode {
    _ = ptr;
    _ = allocator;
    return &.{};
}

fn vt_supports_curved_panorama(ptr: *anyopaque) bool {
    _ = ptr;
    return false;
}

fn vt_supports_offscreen_targets(ptr: *anyopaque) bool {
    _ = ptr;
    return false;
}

fn vt_set_draw_offset(ptr: *anyopaque, dx: i32, dy: i32) void {
    const self: *Ps2Engine = as_self(ptr);
    self.draw_dx = dx;
    self.draw_dy = dy;
}

// ── Drawing primitives ────────────────────────────────────────────────────

const Z: c_int = 1;

fn vt_clear(ptr: *anyopaque, color: engine.Color) void {
    const self: *Ps2Engine = as_self(ptr);
    // gsKit_clear paints a full-screen sprite with this colour, so it must be
    // opaque (alpha 128 in the GS /128 convention) or nothing gets cleared.
    gskit.core.clear(self.gs, gskit.reg.rgbaq(color.r, color.g, color.b, ga(color.a), 0));
}

fn vt_draw_rect(ptr: *anyopaque, rect: engine.Rect, color: engine.Color, filled: bool) void {
    const self: *Ps2Engine = as_self(ptr);
    const x: f32 = sx(self, rect.x);
    const y: f32 = sy(self, rect.y);
    const x2: f32 = sx(self, rect.x + rect.w);
    const y2: f32 = sy(self, rect.y + rect.h);
    const col: u64 = color64(color);

    if (filled) {
        gskit.prim.sprite(self.gs, x, y, x2, y2, Z, col);
    } else {
        gskit.prim.line(self.gs, x, y, x2, y, Z, col);
        gskit.prim.line(self.gs, x2, y, x2, y2, Z, col);
        gskit.prim.line(self.gs, x2, y2, x, y2, Z, col);
        gskit.prim.line(self.gs, x, y2, x, y, Z, col);
    }
}

fn vt_draw_line(ptr: *anyopaque, x1: i32, y1: i32, x2: i32, y2: i32, color: engine.Color) void {
    const self: *Ps2Engine = as_self(ptr);
    gskit.prim.line(
        self.gs,
        sx(self, x1),
        sy(self, y1),
        sx(self, x2),
        sy(self, y2),
        Z,
        color64(color),
    );
}

fn vt_draw_circle(ptr: *anyopaque, cx: i32, cy: i32, radius: i32, color: engine.Color, filled: bool) void {
    const self: *Ps2Engine = as_self(ptr);
    if (radius <= 0) return;

    const segments: usize = 24;
    var points: [2 * (segments + 2)]f32 = undefined;
    const cx_f: f32 = sx(self, cx);
    const cy_f: f32 = sy(self, cy);
    const r_f: f32 = @as(f32, @floatFromInt(radius)) * self.scale;
    const col: u64 = color64(color);

    var i: usize = 0;
    while (i <= segments) : (i += 1) {
        const t: f32 = @as(f32, @floatFromInt(i)) / @as(f32, @floatFromInt(segments)) * 6.2831853;
        points[2 * i] = cx_f + @cos(t) * r_f;
        points[2 * i + 1] = cy_f + @sin(t) * r_f;
    }

    if (filled) {
        // Fan: centre first, then the ring (the last point repeats the first).
        var fan: [2 * (segments + 2)]f32 = undefined;
        fan[0] = cx_f;
        fan[1] = cy_f;
        var k: usize = 0;
        while (k <= segments) : (k += 1) {
            fan[2 * (k + 1)] = points[2 * k];
            fan[2 * (k + 1) + 1] = points[2 * k + 1];
        }
        gskit.prim.gsKit_prim_triangle_fan(self.gs, &fan, @intCast(segments + 2), Z, col);
    } else {
        gskit.prim.gsKit_prim_line_strip(self.gs, &points, @intCast(segments), Z, col);
    }
}

// ── Textures ──────────────────────────────────────────────────────────────

fn find_texture(self: *Ps2Engine, id: u32) ?*TextureEntry {
    for (&self.textures) |*entry| {
        if (entry.used and entry.id == id) return entry;
    }
    return null;
}

fn free_slot(self: *Ps2Engine) ?*TextureEntry {
    for (&self.textures) |*entry| {
        if (!entry.used) return entry;
    }
    return null;
}

/// gsKit's `to_psm16` only handles CT24, so downconvert CT32 to 16-bit
/// ourselves: halves VRAM and texture bandwidth. GS 16-bit layout is
/// A1 B5 G5 R5.
fn downconvertTo16(tex: *gskit.GSTEXTURE) void {
    if (tex.PSM != gskit.PSM_CT32) return;
    const src_mem: [*]u32 = tex.Mem orelse return;
    const count: usize = @as(usize, tex.Width) * @as(usize, tex.Height);

    const dst_raw: *anyopaque = memalign(128, count * 2) orelse return;
    const dst: [*]u16 = @ptrCast(@alignCast(dst_raw));
    var i: usize = 0;
    while (i < count) : (i += 1) {
        const p: u32 = src_mem[i];
        const r: u16 = @intCast(p & 0xFF);
        const g: u16 = @intCast((p >> 8) & 0xFF);
        const b: u16 = @intCast((p >> 16) & 0xFF);
        const a: u32 = (p >> 24) & 0xFF;
        const a1: u16 = if (a >= 96) 1 else 0;
        dst[i] = (a1 << 15) | ((b >> 3) << 10) | ((g >> 3) << 5) | (r >> 3);
    }

    free(@ptrCast(src_mem));
    tex.Mem = @ptrCast(@alignCast(dst));
    tex.PSM = gskit.PSM_CT16S;
}

/// Font atlas sizes actually baked. The game asks for many sizes; keeping one
/// atlas per size would eat the GS' 4 MB, so sizes are bucketed and glyphs are
/// scaled. Keeps VRAM for sprites.
fn bakeSize(size: u32) u32 {
    if (size <= 20) return 16;
    if (size <= 34) return 24;
    return 40;
}

fn vt_load_texture(ptr: *anyopaque, path: []const u8) ?engine.TextureHandle {
    const self: *Ps2Engine = as_self(ptr);
    const slot: *TextureEntry = free_slot(self) orelse return null;

    slot.tex = gskit.GSTEXTURE{
        .Width = 0,
        .Height = 0,
        .PSM = 0,
        .ClutPSM = 0,
        .TBW = 0,
        .Mem = null,
        .Clut = null,
        .Vram = 0,
        .VramClut = 0,
        .Filter = gskit.FILTER_NEAREST,
        .ClutStorageMode = 0,
        // Delayed: gsKit keeps the decoded pixels in EE RAM and uploads /
        // evicts them from VRAM through the texture manager as needed. This is
        // what makes large textures fit inside the GS' 4 MB.
        .Delayed = 1,
    };

    // The engine hands plain slices; build a C string for gsKit (with a device
    // prefix so PCSX2/the console can find the file).
    var buf: [512]u8 = undefined;
    _ = resolvePath(path, &buf) orelse return null;

    const ok: bool = if (has_suffix(path, ".bmp"))
        gskit.texture.gsKit_texture_bmp(self.gs, &slot.tex, &buf) == 0
    else
        gskit.texture.gsKit_texture_png(self.gs, &slot.tex, &buf) == 0;
    if (!ok) return null;

    // gsKit's PNG loader stores CT32 alpha as `128 - a*128/255`, the opposite
    // of the GS blend convention its own font/text paths use (0x80 = opaque).
    // Left as-is, RGBA sprites come out invisible; flip it back.
    if (slot.tex.PSM == gskit.PSM_CT32) {
        if (slot.tex.Mem) |mem| {
            const count: usize = @as(usize, slot.tex.Width) * @as(usize, slot.tex.Height);
            var i: usize = 0;
            while (i < count) : (i += 1) {
                const alpha: u32 = (mem[i] >> 24) & 0xFF;
                mem[i] = (mem[i] & 0x00FF_FFFF) | ((128 - alpha) << 24);
            }
        }
    }

    // Halve the VRAM footprint (sprites are big; the GS only has 4 MB).
    downconvertTo16(&slot.tex);

    slot.used = true;
    slot.id = self.next_id;
    slot.width = slot.tex.Width;
    slot.height = slot.tex.Height;
    self.next_id += 1;
    return engine.TextureHandle{ .id = slot.id };
}

fn has_suffix(path: []const u8, suffix: []const u8) bool {
    if (path.len < suffix.len) return false;
    return std.mem.eql(u8, path[path.len - suffix.len ..], suffix);
}

fn vt_create_target(ptr: *anyopaque, width: u32, height: u32) ?engine.TextureHandle {
    _ = ptr;
    _ = width;
    _ = height;
    return null; // offscreen targets unsupported (supports_offscreen_targets = false)
}

fn vt_create_texture(ptr: *anyopaque, width: u32, height: u32, pixels: ?[]const u8, pitch: u32) ?engine.TextureHandle {
    _ = ptr;
    _ = width;
    _ = height;
    _ = pixels;
    _ = pitch;
    return null; // CPU-writable textures unsupported on PS2
}

fn vt_update_texture(ptr: *anyopaque, tex: engine.TextureHandle, pixels: []const u8, pitch: u32) void {
    _ = ptr;
    _ = tex;
    _ = pixels;
    _ = pitch;
}

fn vt_draw_texture(ptr: *anyopaque, tex: engine.TextureHandle, dst: engine.Rect, src: ?engine.Rect, alpha: ?u8) void {
    const self: *Ps2Engine = as_self(ptr);
    const entry: *TextureEntry = find_texture(self, tex.id) orelse return;
    // Delayed textures are resident in EE RAM until bound; the manager uploads
    // (or evicts another texture) and sets `Vram`.
    _ = gskit.texture.gsKit_TexManager_bind(self.gs, &entry.tex);

    var ua: f32 = 0;
    var va: f32 = 0;
    var ub: f32 = @floatFromInt(entry.width);
    var vb: f32 = @floatFromInt(entry.height);
    if (src) |s| {
        ua = @floatFromInt(s.x);
        va = @floatFromInt(s.y);
        ub = @floatFromInt(s.x + s.w);
        vb = @floatFromInt(s.y + s.h);
    }

    // Texture modulation: GS multiplies by Cv/128, so 128 is neutral and 255
    // doubles ("blown"). We use the neutral RGB *and* full alpha here — with
    // alpha 0x80 this HLSL-style path dropped the sprite entirely.
    _ = alpha;
    const col: u64 = gskit.reg.rgbaq(gc(255), gc(255), gc(255), 255, 0);
    gskit.texture.sprite(
        self.gs,
        &entry.tex,
        sx(self, dst.x),
        sy(self, dst.y),
        ua,
        va,
        sx(self, dst.x + dst.w),
        sy(self, dst.y + dst.h),
        ub,
        vb,
        Z,
        col,
    );
}

fn vt_draw_texture_rotated(ptr: *anyopaque, tex: engine.TextureHandle, dst: engine.Rect, angle: f32, alpha: ?u8) void {
    const self: *Ps2Engine = as_self(ptr);
    const entry: *TextureEntry = find_texture(self, tex.id) orelse return;
    _ = gskit.texture.gsKit_TexManager_bind(self.gs, &entry.tex);

    const x0: f32 = sx(self, dst.x);
    const x1: f32 = sx(self, dst.x + dst.w);
    const y0: f32 = sy(self, dst.y);
    const y1: f32 = sy(self, dst.y + dst.h);
    const cxf: f32 = (x0 + x1) / 2;
    const cyf: f32 = (y0 + y1) / 2;
    const hw: f32 = (x1 - x0) / 2;
    const hh: f32 = (y1 - y0) / 2;
    const rad: f32 = angle * 0.017453292;
    const ca: f32 = @cos(rad);
    const sa: f32 = @sin(rad);

    // Texture modulation: GS multiplies by Cv/128, so 128 is neutral and 255
    // doubles ("blown"). We use the neutral RGB *and* full alpha here — with
    // alpha 0x80 this HLSL-style path dropped the sprite entirely.
    _ = alpha;
    const col: u64 = gskit.reg.rgbaq(gc(255), gc(255), gc(255), 255, 0);
    const u_max: f32 = @floatFromInt(entry.width);
    const v_max: f32 = @floatFromInt(entry.height);

    gskit.texture.gsKit_prim_quad_texture_3d(
        self.gs,
        &entry.tex,
        cxf + (-hw * ca - -hh * sa),
        cyf + (-hw * sa + -hh * ca),
        Z,
        0,
        0,
        cxf + (hw * ca - -hh * sa),
        cyf + (hw * sa + -hh * ca),
        Z,
        u_max,
        0,
        cxf + (hw * ca - hh * sa),
        cyf + (hw * sa + hh * ca),
        Z,
        u_max,
        v_max,
        cxf + (-hw * ca - hh * sa),
        cyf + (-hw * sa + hh * ca),
        Z,
        0,
        v_max,
        col,
    );
}

fn vt_texture_size(ptr: *anyopaque, tex: engine.TextureHandle) engine.Point {
    const self: *Ps2Engine = as_self(ptr);
    const entry: *TextureEntry = find_texture(self, tex.id) orelse return engine.Point{ .x = 0, .y = 0 };
    return engine.Point{ .x = @intCast(entry.width), .y = @intCast(entry.height) };
}

fn vt_geometry(ptr: *anyopaque, tex: engine.TextureHandle, vertices: []const engine.Vertex, indices: []const i32) void {
    _ = ptr;
    _ = tex;
    _ = vertices;
    _ = indices; // textured meshes unsupported
}

fn vt_set_render_target(ptr: *anyopaque, target: ?engine.TextureHandle) void {
    _ = ptr;
    _ = target;
}

// ── Text ──────────────────────────────────────────────────────────────────
//
// Text uses the game's own TTF (via `font_shim.c`/FreeType), rasterised into a
// T8 glyph atlas and drawn as sprites. When a game never loads a font we fall
// back to the BIOS ROM font, so `neko.text.draw` always works.

fn loadFontSlot(self: *Ps2Engine, path: []const u8, size: u32) i64 {
    const baked: u32 = bakeSize(size);
    // One atlas per bucket; reuse when already baked.
    for (&self.fonts, 0..) |*f, i| {
        if (f.used and f.pixel_size == baked) return @intCast(i);
    }

    var chosen: usize = MAX_FONTS;
    for (&self.fonts, 0..) |*f, i| {
        if (!f.used) {
            chosen = i;
            break;
        }
    }
    if (chosen == MAX_FONTS) return -1;
    const slot: *FontSlot = &self.fonts[chosen];

    var buf: [512]u8 = undefined;
    const resolved: []u8 = resolvePath(path, &buf) orelse return -1;
    const shim: c_int = neko_font_open(resolved.ptr, @intCast(baked));
    if (shim < 0) return -1;

    const cell: usize = baked + 6;
    const cols: usize = 16;
    const rows: usize = (ASCII_COUNT + cols - 1) / cols;
    const atlas_w: u32 = nextPow2(@intCast(cols * cell));
    const atlas_h: u32 = nextPow2(@intCast(rows * cell));
    const area: usize = @as(usize, atlas_w) * @as(usize, atlas_h);

    const atlas_raw: *anyopaque = memalign(128, area) orelse {
        neko_font_close(shim);
        return -1;
    };
    const atlas: [*]u8 = @ptrCast(atlas_raw);
    @memset(atlas[0..area], 0);

    const clut_raw: *anyopaque = memalign(128, 256 * 4) orelse {
        free(atlas_raw);
        neko_font_close(shim);
        return -1;
    };
    const clut: [*]u32 = @ptrCast(@alignCast(clut_raw));
    var k: u32 = 0;
    while (k < 256) : (k += 1) {
        const alpha: u32 = k / 2; // GS alpha: 0x80 = opaque
        clut[k] = (alpha << 24) | 0x00FF_FFFF;
    }

    slot.* = FontSlot{
        .used = true,
        .shim = shim,
        .pixel_size = baked,
        .ascender = neko_font_ascender(shim),
        .descender = neko_font_descender(shim),
        .atlas_w = atlas_w,
        .atlas_h = atlas_h,
        .atlas = atlas,
        .clut = clut,
    };

    var i: usize = 0;
    while (i < ASCII_COUNT) : (i += 1) {
        var g: NekoGlyph = undefined;
        if (neko_font_glyph(shim, ASCII_FIRST + @as(u32, @intCast(i)), &g) != 0) continue;
        const gw: usize = @intCast(@min(@as(c_int, @intCast(cell)), @max(g.width, 0)));
        const gh: usize = @intCast(@min(@as(c_int, @intCast(cell)), @max(g.height, 0)));
        const ox: usize = (i % cols) * cell;
        const oy: usize = (i / cols) * cell;
        if (g.coverage) |src| {
            const pitch: usize = @intCast(@max(g.pitch, 0));
            var y: usize = 0;
            while (y < gh) : (y += 1) {
                const dst: usize = (oy + y) * @as(usize, atlas_w) + ox;
                const src_off: usize = y * pitch;
                @memcpy(atlas[dst .. dst + gw], src[src_off .. src_off + gw]);
            }
        }
        slot.glyphs[i] = .{
            .u = @intCast(ox),
            .v = @intCast(oy),
            .w = @intCast(gw),
            .h = @intCast(gh),
            .left = g.left,
            .top = g.top,
            .advance = g.advance,
        };
    }

    slot.tex = gskit.GSTEXTURE{
        .Width = atlas_w,
        .Height = atlas_h,
        .PSM = gskit.PSM_T8,
        .ClutPSM = gskit.PSM_CT32,
        .TBW = 0,
        .Mem = @ptrCast(@alignCast(atlas)),
        .Clut = clut,
        .Vram = 0,
        .VramClut = 0,
        .Filter = gskit.FILTER_LINEAR,
        .ClutStorageMode = 0,
        .Delayed = 1,
    };

    return @intCast(chosen);
}

fn resolveFont(self: *Ps2Engine, font_idx: i64, size: u32) ?*FontSlot {
    if (font_idx >= 0 and font_idx < MAX_FONTS) {
        const f: *FontSlot = &self.fonts[@intCast(font_idx)];
        if (f.used) return f;
    }
    for (&self.fonts) |*f| {
        if (f.used and f.pixel_size == size) return f;
    }
    return null;
}

fn atlasWidth(f: *const FontSlot, text: []const u8, size: u32) f32 {
    const scale: f32 = @as(f32, @floatFromInt(size)) / @as(f32, @floatFromInt(f.pixel_size));
    var w: f32 = 0;
    for (text) |ch| {
        if (ch >= ASCII_FIRST and ch <= ASCII_LAST) {
            w += @as(f32, @floatFromInt(f.glyphs[ch - ASCII_FIRST].advance)) * scale;
        }
    }
    return w;
}

fn drawAtlasText(self: *Ps2Engine, f: *FontSlot, text: []const u8, x: i32, y: i32, size: u32, color: engine.Color, center: bool) bool {
    _ = gskit.texture.gsKit_TexManager_bind(self.gs, &f.tex);
    const scale: f32 = @as(f32, @floatFromInt(size)) / @as(f32, @floatFromInt(f.pixel_size));
    var pen: f32 = @as(f32, @floatFromInt(x + self.draw_dx));
    const baseline: f32 = @as(f32, @floatFromInt(y + self.draw_dy)) + @as(f32, @floatFromInt(f.ascender)) * scale;
    if (center) pen -= atlasWidth(f, text, size) / 2.0;
    const col: u64 = gskit.reg.rgbaq(gc(color.r), gc(color.g), gc(color.b), ga(color.a), 0);

    for (text) |ch| {
        if (ch < ASCII_FIRST or ch > ASCII_LAST) continue;
        const g: Glyph = f.glyphs[ch - ASCII_FIRST];
        if (g.w > 0 and g.h > 0) {
            const gx: f32 = pen + @as(f32, @floatFromInt(g.left)) * scale;
            const gy: f32 = baseline - @as(f32, @floatFromInt(g.top)) * scale;
            const gw: f32 = @as(f32, @floatFromInt(g.w)) * scale;
            const gh: f32 = @as(f32, @floatFromInt(g.h)) * scale;
            gskit.texture.sprite(
                self.gs,
                &f.tex,
                sx_f(self, gx),
                sy_f(self, gy),
                @floatFromInt(g.u),
                @floatFromInt(g.v),
                sx_f(self, gx + gw),
                sy_f(self, gy + gh),
                @floatFromInt(g.u + g.w),
                @floatFromInt(g.v + g.h),
                Z,
                col,
            );
        }
        pen += @as(f32, @floatFromInt(g.advance)) * scale;
    }
    return true;
}

fn drawRomText(self: *Ps2Engine, text: []const u8, x: i32, y: i32, size: u32, color: engine.Color, center: bool) bool {
    if (!self.has_font) return false;

    var buf: [1024]u8 = undefined;
    if (text.len >= buf.len) return true;
    @memcpy(buf[0..text.len], text);
    buf[text.len] = 0;

    const scale: f32 = text_scale(size) * self.scale;
    var px: f32 = sx(self, x);
    const py: f32 = sy(self, y);
    if (center) px -= text_width(self, text, size) * self.scale / 2.0;

    self.font.Align = gskit.fontm.ALIGN_LEFT;
    gskit.fontm.printScaled(self.gs, self.font, px, py, Z, scale, color32(color), &buf);
    return true;
}

fn vt_load_font(ptr: *anyopaque, path: []const u8, size: u16) i64 {
    const self: *Ps2Engine = as_self(ptr);
    return loadFontSlot(self, path, size);
}

fn vt_draw_text(ptr: *anyopaque, text: []const u8, x: i32, y: i32, size: u32, color: engine.Color, center: bool, font_idx: i64) bool {
    const self: *Ps2Engine = as_self(ptr);
    if (text.len == 0) return true;
    if (resolveFont(self, font_idx, size)) |f| {
        return drawAtlasText(self, f, text, x, y, size, color, center);
    }
    return drawRomText(self, text, x, y, size, color, center);
}

fn vt_draw_text_rotated(ptr: *anyopaque, text: []const u8, x: i32, y: i32, size: u32, angle: f32, color: engine.Color, center: bool, font_idx: i64) bool {
    // Per-glyph rotation is not implemented yet; draw upright.
    _ = angle;
    return vt_draw_text(ptr, text, x, y, size, color, center, font_idx);
}

fn vt_text_size(ptr: *anyopaque, text: []const u8, font_idx: u32) ?engine.Point {
    const self: *Ps2Engine = as_self(ptr);
    const f: *FontSlot = resolveFont(self, @intCast(font_idx), 0) orelse return null;
    const w: f32 = atlasWidth(f, text, f.pixel_size);
    return engine.Point{ .x = @intFromFloat(w), .y = f.ascender + f.descender };
}

// ── Sound (audsrv + software mixer) ───────────────────────────────────────
//
// The game ships OGG; the build converts them to 16-bit mono WAV. We parse the
// WAV here (pure Zig) and mix the active voices each frame, streaming the
// result to audsrv. No audio codec runs on the EE.

extern fn fopen(path: [*c]const u8, mode: [*c]const u8) ?*anyopaque;
extern fn fread(ptr: [*]u8, size: usize, count: usize, stream: ?*anyopaque) usize;
extern fn fclose(stream: ?*anyopaque) c_int;
extern fn fseek(stream: ?*anyopaque, offset: c_long, whence: c_int) c_int;
extern fn ftell(stream: ?*anyopaque) c_long;

fn le16(b: []const u8) u16 {
    return @as(u16, b[0]) | (@as(u16, b[1]) << 8);
}

fn le32(b: []const u8) u32 {
    return @as(u32, b[0]) | (@as(u32, b[1]) << 8) | (@as(u32, b[2]) << 16) | (@as(u32, b[3]) << 24);
}

/// Loads the IOP modules and starts audsrv on first use.
fn ensureAudio(self: *Ps2Engine) void {
    if (self.audio_ok) return;
    _ = ps2.loader.SifLoadModule("host:libsd.irx", 0, "");
    _ = ps2.loader.SifLoadModule("host:audsrv.irx", 0, "");
    if (ps2.audsrv.audsrv_init() != 0) return;
    var fmt: ps2.audsrv.Format = ps2.audsrv.Format{ .freq = @intCast(MIX_RATE), .bits = 16, .channels = 1 };
    _ = ps2.audsrv.audsrv_set_format(&fmt);
    _ = ps2.audsrv.audsrv_set_volume(100);
    self.audio_ok = true;
}

fn findSound(self: *Ps2Engine, id: u32) ?*SoundEntry {
    for (&self.sounds) |*s| {
        if (s.used and s.id == id) return s;
    }
    return null;
}

/// `<assets>/audio/foo.ogg` -> `host:.../foo.wav` (the build's conversion).
fn soundPath(path: []const u8, buf: []u8) ?[]u8 {
    var tmp: [512]u8 = undefined;
    if (path.len >= tmp.len) return null;
    @memcpy(tmp[0..path.len], path);
    const n: usize = path.len;
    if (std.mem.endsWith(u8, tmp[0..n], ".ogg")) {
        tmp[n - 3] = 'w';
        tmp[n - 2] = 'a';
        tmp[n - 1] = 'v';
    }
    return resolvePath(tmp[0..n], buf);
}

fn vt_load_sound(ptr: *anyopaque, path: []const u8) ?engine.SoundHandle {
    const self: *Ps2Engine = as_self(ptr);
    var slot: *SoundEntry = undefined;
    var have: bool = false;
    for (&self.sounds) |*s| {
        if (!s.used) {
            slot = s;
            have = true;
            break;
        }
    }
    if (!have) return null;

    var pbuf: [512]u8 = undefined;
    const resolved: []u8 = soundPath(path, &pbuf) orelse return null;

    const fp: *anyopaque = fopen(resolved.ptr, "rb") orelse return null;
    defer _ = fclose(fp);
    _ = fseek(fp, 0, 2); // SEEK_END
    const size_signed: c_long = ftell(fp);
    _ = fseek(fp, 0, 0); // SEEK_SET
    if (size_signed <= 44) return null;
    const size: usize = @intCast(size_signed);

    const raw: *anyopaque = malloc(size) orelse return null;
    const bytes: [*]u8 = @ptrCast(raw);
    if (fread(bytes, 1, size, fp) != size) {
        free(raw);
        return null;
    }

    if (!(std.mem.eql(u8, bytes[0..4], "RIFF") and std.mem.eql(u8, bytes[8..12], "WAVE"))) {
        free(raw);
        return null;
    }

    var off: usize = 12;
    var channels: u16 = 0;
    var bits: u16 = 0;
    var data_off: usize = 0;
    var data_len: usize = 0;
    while (off + 8 <= size) {
        const clen: usize = le32(bytes[off + 4 .. off + 8]);
        if (std.mem.eql(u8, bytes[off .. off + 4], "fmt ")) {
            channels = le16(bytes[off + 10 .. off + 12]);
            bits = le16(bytes[off + 22 .. off + 24]);
        } else if (std.mem.eql(u8, bytes[off .. off + 4], "data")) {
            data_off = off + 8;
            data_len = @min(clen, size - data_off);
            break;
        }
        off += 8 + clen + (clen & 1);
    }
    if (channels != 1 or bits != 16 or data_len < 2) {
        free(raw);
        return null;
    }

    const frames: usize = data_len / 2;
    const pcm_raw: *anyopaque = memalign(128, frames * 2) orelse {
        free(raw);
        return null;
    };
    const pcm_bytes: [*]u8 = @ptrCast(pcm_raw);
    @memcpy(pcm_bytes[0..data_len], bytes[data_off .. data_off + data_len]);
    free(raw);

    slot.* = SoundEntry{
        .used = true,
        .id = self.next_sound_id,
        .pcm = @ptrCast(@alignCast(pcm_raw)),
        .frames = frames,
    };
    self.next_sound_id += 1;
    return engine.SoundHandle{ .id = slot.id };
}

fn vt_play_sound(ptr: *anyopaque, snd: engine.SoundHandle, loops: i32, channel: i32) i32 {
    const self: *Ps2Engine = as_self(ptr);
    ensureAudio(self);
    _ = findSound(self, snd.id) orelse return channel;

    var idx: usize = 0;
    if (channel >= 0 and channel < MAX_CHANNELS and !self.channels[@intCast(channel)].active) {
        idx = @intCast(channel);
    } else {
        var found: bool = false;
        for (&self.channels, 0..) |*c, i| {
            if (!c.active) {
                idx = i;
                found = true;
                break;
            }
        }
        if (!found) return channel;
    }

    self.channels[idx] = .{
        .active = true,
        .sound = snd.id,
        .pos = 0,
        .volume = 100,
        .loops = loops != 0,
    };
    return @intCast(idx);
}

fn vt_stop_channel(ptr: *anyopaque, channel: i32) void {
    const self: *Ps2Engine = as_self(ptr);
    if (channel >= 0 and channel < MAX_CHANNELS) {
        self.channels[@intCast(channel)].active = false;
    }
}

fn vt_stop_all_sounds(ptr: *anyopaque) void {
    const self: *Ps2Engine = as_self(ptr);
    for (&self.channels) |*c| c.active = false;
}

fn vt_set_master_volume(ptr: *anyopaque, vol: i32) void {
    const self: *Ps2Engine = as_self(ptr);
    self.audio_master = vol;
    if (self.audio_ok) _ = ps2.audsrv.audsrv_set_volume(@intCast(@max(0, @min(100, vol))));
}

fn vt_set_sfx_volume(ptr: *anyopaque, vol: i32) void {
    as_self(ptr).audio_sfx = vol;
}

fn vt_set_music_volume(ptr: *anyopaque, vol: i32) void {
    as_self(ptr).audio_music = vol;
}

/// Mixes one block of the active voices and streams it to audsrv.
fn mixAudio(self: *Ps2Engine) void {
    if (!self.audio_ok) return;

    var i: usize = 0;
    while (i < MIX_SAMPLES) : (i += 1) self.mix_buf[i] = 0;

    const vol: i32 = @max(0, @min(100, @divTrunc(self.audio_master * self.audio_sfx, 100)));

    for (&self.channels) |*c| {
        if (!c.active) continue;
        const snd: *SoundEntry = findSound(self, c.sound) orelse {
            c.active = false;
            continue;
        };
        const pcm: [*]const i16 = snd.pcm orelse {
            c.active = false;
            continue;
        };
        var k: usize = 0;
        while (k < MIX_SAMPLES) : (k += 1) {
            if (c.pos >= snd.frames) {
                if (c.loops) {
                    c.pos = 0;
                } else {
                    c.active = false;
                    break;
                }
            }
            const s: i32 = pcm[c.pos];
            const mixed: i32 = @as(i32, self.mix_buf[k]) + @divTrunc(s * vol * c.volume, 10000);
            self.mix_buf[k] = @intCast(std.math.clamp(mixed, -32768, 32767));
            c.pos += 1;
        }
    }

    _ = ps2.audsrv.audsrv_wait_audio(@intCast(MIX_SAMPLES * 2));
    _ = ps2.audsrv.audsrv_play_audio(@ptrCast(&self.mix_buf), @intCast(MIX_SAMPLES * 2));
}

// ── Files ─────────────────────────────────────────────────────────────────
//
// There is no host filesystem on the PS2 yet, so every file operation degrades
// exactly as if the file were missing. The core stays happy: `neko.save`
// simply never finds a save file.

fn vt_read_file(ptr: *anyopaque, allocator: Allocator, dir_path: []const u8, file_name: []const u8, max: usize) ?[]u8 {
    _ = ptr;
    _ = allocator;
    _ = dir_path;
    _ = file_name;
    _ = max;
    return null;
}

fn vt_write_file(ptr: *anyopaque, dir_path: []const u8, file_name: []const u8, data: []const u8) bool {
    _ = ptr;
    _ = dir_path;
    _ = file_name;
    _ = data;
    return false;
}

fn vt_delete_file(ptr: *anyopaque, dir_path: []const u8, file_name: []const u8) void {
    _ = ptr;
    _ = dir_path;
    _ = file_name;
}

fn vt_file_exists(ptr: *anyopaque, dir_path: []const u8, file_name: []const u8) bool {
    _ = ptr;
    _ = dir_path;
    _ = file_name;
    return false;
}

// ── Input and misc ────────────────────────────────────────────────────────

fn vt_mouse_pos(ptr: *anyopaque) engine.Point {
    const self: *Ps2Engine = as_self(ptr);
    return engine.Point{ .x = @intCast(self.logical_w / 2), .y = @intCast(self.logical_h / 2) };
}

fn vt_update_discord(ptr: *anyopaque, details: []const u8, state: []const u8) void {
    _ = ptr;
    _ = details;
    _ = state;
}

// ── Entry point ───────────────────────────────────────────────────────────
//
// On freestanding targets Zig's `start.zig` never calls `main`, and a `main`
// in a non-root module is not a GC root. `entry.zig` (same directory) is used
// as the executable root by PS2 builds: it imports the game module as `game`
// and calls `run(game.main)`, so the game keeps its normal host signature
// `pub fn main(init: std.process.Init)`. We build an `Init` by hand: a
// malloc-backed gpa, an arena, an empty environment map and a stub `Io`.
// `neko.save` never dereferences the stub on PS2, so persistence simply
// degrades to "no save file".

extern fn malloc(size: usize) ?*anyopaque;
extern fn memalign(alignment: usize, size: usize) ?*anyopaque;
extern fn free(ptr: ?*anyopaque) void;

fn cAlloc(_: *anyopaque, len: usize, alignment: std.mem.Alignment, _: usize) ?[*]u8 {
    const bytes: usize = alignment.toByteUnits();
    const raw: ?*anyopaque = if (bytes <= 8) malloc(len) else memalign(bytes, len);
    return @ptrCast(raw);
}

fn cResize(_: *anyopaque, memory: []u8, _: std.mem.Alignment, _: usize, _: usize) bool {
    _ = memory;
    return false;
}

fn cRemap(_: *anyopaque, memory: []u8, _: std.mem.Alignment, _: usize, _: usize) ?[*]u8 {
    _ = memory;
    return null;
}

fn cFree(_: *anyopaque, memory: []u8, _: std.mem.Alignment, _: usize) void {
    free(@ptrCast(memory.ptr));
}

const c_allocator_vtable: std.mem.Allocator.VTable = .{
    .alloc = cAlloc,
    .resize = cResize,
    .remap = cRemap,
    .free = cFree,
};

/// General-purpose allocator backed by the PS2SDK libc heap (`malloc`).
pub const gpa: std.mem.Allocator = .{ .ptr = undefined, .vtable = &c_allocator_vtable };

/// A stub `std.Io`: only `crashHandler` is set. The engine guards every host
/// file operation on PS2, so no other slot is ever reached.
var io_stub_vtable: std.Io.VTable = undefined;

fn ioStubCrash(userdata: ?*anyopaque) void {
    _ = userdata;
    while (true) {}
}

fn invoke(result: anytype) void {
    if (@typeInfo(@TypeOf(result)) == .error_union) {
        result catch {};
    }
}

/// Builds the `Init` the game expects and calls `main_fn`. `entry.zig` passes
/// `@import("game").main` here.
pub fn run(main_fn: anytype) void {
    var arena: std.heap.ArenaAllocator = std.heap.ArenaAllocator.init(gpa);
    // `Environ.createMap` does not support freestanding, so build the empty
    // map directly (games only ever read it, e.g. `environ_map.get("HOME")`).
    var environ_map: std.process.Environ.Map = .{
        .array_hash_map = .empty,
        .allocator = gpa,
    };
    io_stub_vtable.crashHandler = ioStubCrash;

    const init: std.process.Init = .{
        .minimal = .{
            .args = .{ .vector = {} },
            .environ = std.process.Environ.empty,
        },
        .arena = &arena,
        .gpa = gpa,
        .io = .{ .userdata = null, .vtable = &io_stub_vtable },
        .environ_map = &environ_map,
        .preopens = undefined,
    };

    const info: std.builtin.Type.Fn = @typeInfo(@TypeOf(main_fn)).@"fn";
    if (info.param_types.len == 0) {
        invoke(main_fn());
    } else {
        const P: type = info.param_types[0].?;
        if (P == std.process.Init) {
            invoke(main_fn(init));
        } else if (P == std.process.Init.Minimal) {
            invoke(main_fn(init.minimal));
        } else {
            @compileError("neko/ps2: main must take std.process.Init, Init.Minimal, or no parameter");
        }
    }
}
