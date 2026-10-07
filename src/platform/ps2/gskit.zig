// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! gsKit bindings — 2D/3D drawing on the PS2 Graphics Synthesizer (GS).
//!
//! Hand-written `extern fn` declarations (no `@cImport`/translate-c on this
//! freestanding target) resolved at link time against `libgskit.a` +
//! `libdmakit.a` (+ `libgskit_toolkit.a` for the image loaders).
//!
//! The C macros that the linker cannot see are re-implemented in Zig:
//!   - `GS_SETREG_*` packing macros  -> `reg.rgbaq`, `reg.scissor`, `reg.test`, …
//!   - `gsKit_prim_line/quad/...`    -> wrappers in `prim`
//!   - `gsKit_prim_sprite_texture`   -> `texture.sprite`
//!   - `gsKit_fontm_print`           -> `fontm.print`
//!
//! Minimal startup (also see `hello.zig`):
//!
//!     const gs = core.initGlobal();
//!     const font = fontm.init();
//!     dma.init();
//!     core.initScreen(gs);
//!     _ = fontm.upload(gs, font);
//!     core.modeSwitch(gs, ONESHOT);
//!     while (true) {
//!         core.clear(gs, reg.rgbaq(0, 0, 0, 0, 0));
//!         fontm.printScaled(gs, font, 20, 20, 1, 2.0, reg.font(255, 255, 255), "hi");
//!         core.queueExec(gs);
//!         core.syncFlip(gs);
//!     }
//!
//! Note: `core.syncFlip` waits for vblank by busy-polling the GS_CSR register,
//! so it does NOT depend on EE interrupts (unlike `thread.SleepThread`).

/// The global gsKit context. Opaque — gsKit allocates and owns it; you only
/// pass the pointer around.
pub const GSGLOBAL: type = opaque {};

/// `struct gsTexture`. Allocate one yourself (`var t: GSTEXTURE = undefined;`)
/// and hand its address to the loaders/uploaders.
pub const GSTEXTURE: type = extern struct {
    Width: u32,
    Height: u32,
    PSM: u8,
    ClutPSM: u8,
    TBW: u32,
    Mem: ?[*]u32,
    Clut: ?[*]u32,
    Vram: u32,
    VramClut: u32,
    Filter: u32,
    ClutStorageMode: u8,
    Delayed: u8,
};

/// `struct gsKit_fontm_header`.
pub const GSFONTMHDR: type = extern struct {
    sig: u32,
    version: u32,
    bitsize: u32,
    baseoffset: u32,
    num_entries: u32,
    eof: u32,
    offset_table: ?[*]u32,
};

pub const GS_FONTM_PAGE_COUNT: comptime_int = 8;

/// `struct gsFontM` — an FONTM bitmap font. gsKit fills the ROM font (FONTM)
/// into one of these; you may tweak `Spacing` and `Align`.
pub const GSFONTM: type = extern struct {
    Texture: [GS_FONTM_PAGE_COUNT]?*GSTEXTURE,
    Header: GSFONTMHDR,
    VramIdx: u32,
    Align: u8,
    Spacing: f32,
    TexBase: ?*anyopaque,
};

// ── Constants (expanded from the gsKit headers) ─────────────────────────────

// Display modes (`GSGLOBAL::Mode`, for `gsKit_init_global_custom` setups).
pub const MODE_NTSC: c_int = 0x02;
pub const MODE_PAL: c_int = 0x03;
pub const MODE_DTV_480P: c_int = 0x50;
pub const MODE_DTV_1080I: c_int = 0x51;
pub const MODE_DTV_720P: c_int = 0x52;
pub const MODE_VGA_640_60: c_int = 0x1A;

pub const INTERLACED: c_int = 0x01;
pub const NONINTERLACED: c_int = 0x00;

pub const FIELD: c_int = 0x00;
pub const FIELD_NORMAL: c_int = 0x00;
pub const FIELD_ODD: c_int = 0x02;
pub const FIELD_EVEN: c_int = 0x03;

pub const ASPECT_4_3: c_int = 0x0;
pub const ASPECT_16_9: c_int = 0x1;

pub const SETTING_OFF: u8 = 0x00;
pub const SETTING_ON: u8 = 0x01;

/// Draw-queue modes (`core.modeSwitch`).
pub const ONESHOT: u8 = 0x01;
pub const PERSISTENT: u8 = 0x00;

/// Render-queue pool sizes, the defaults behind `gsKit_init_global()`.
pub const RENDER_QUEUE_OS_POOLSIZE: c_int = 1024 * 1024;
pub const RENDER_QUEUE_PER_POOLSIZE: c_int = 1024 * 256;

/// Test presets (`core.setTest`).
pub const ZTEST_OFF: u8 = 0x01;
pub const ZTEST_ON: u8 = 0x02;
pub const ATEST_OFF: u8 = 0x03;
pub const ATEST_ON: u8 = 0x04;

/// Texture filters (`GSTEXTURE::Filter`, `core.setTexFilter`).
pub const FILTER_NEAREST: u8 = 0x00;
pub const FILTER_LINEAR: u8 = 0x01;

/// Pixel storage modes (`GSGLOBAL::PSM`, `GSTEXTURE::PSM`).
pub const PSM_CT32: u8 = 0x00;
pub const PSM_CT24: u8 = 0x01;
pub const PSM_CT16: u8 = 0x02;
pub const PSM_CT16S: u8 = 0x0A;
pub const PSM_T8: u8 = 0x13;
pub const PSM_T4: u8 = 0x14;

/// Clamp / wrap modes (`core.setClamp`).
pub const CMODE_REPEAT: u8 = 0x00;
pub const CMODE_CLAMP: u8 = 0x01;
pub const CMODE_REGION_CLAMP: u8 = 0x02;
pub const CMODE_REGION_REPEAT: u8 = 0x03;
pub const CMODE_RESET: u8 = 0xFF;

// ── GS register packing (`GS_SETREG_*` macros) ──────────────────────────────

pub const reg: type = struct {
    /// `GS_SETREG_RGBAQ(r, g, b, a, q)` -> the 64-bit colour gsKit expects.
    pub fn rgbaq(r: u32, g: u32, b: u32, a: u32, q: u32) u64 {
        return @as(u64, r & 0xFF) |
            (@as(u64, g & 0xFF) << 8) |
            (@as(u64, b & 0xFF) << 16) |
            (@as(u64, a & 0xFF) << 24) |
            (@as(u64, q & 0xFF) << 32);
    }

    /// Same colour, but 32-bit — what `fontm.*` takes (`unsigned long`).
    /// Only valid because fonts always use q = 0.
    pub fn font(r: u32, g: u32, b: u32, a: u32) c_ulong {
        return @as(c_ulong, (r & 0xFF) | ((g & 0xFF) << 8) | ((b & 0xFF) << 16) | ((a & 0xFF) << 24));
    }

    /// `GS_SETREG_SCISSOR(x0, x1, y0, y1)`.
    pub fn scissor(x0: u32, x1: u32, y0: u32, y1: u32) u64 {
        return @as(u64, x0 & 0xFFFF) |
            (@as(u64, x1 & 0xFFFF) << 16) |
            (@as(u64, y0 & 0xFFFF) << 32) |
            (@as(u64, y1 & 0xFFFF) << 48);
    }

    /// `GS_SETREG_TEST(ate, atst, aref, afail, date, datm, zte, ztst)`.
    /// Named `testReg` because `test` is a Zig keyword.
    pub fn testReg(ate: u32, atst: u32, aref: u32, afail: u32, date: u32, datm: u32, zte: u32, ztst: u32) u64 {
        return @as(u64, ate & 1) |
            (@as(u64, atst & 7) << 1) |
            (@as(u64, aref & 0xFF) << 4) |
            (@as(u64, afail & 3) << 12) |
            (@as(u64, date & 1) << 14) |
            (@as(u64, datm & 1) << 15) |
            (@as(u64, zte & 1) << 16) |
            (@as(u64, ztst & 3) << 17);
    }

    /// `GS_SETREG_ALPHA(A, B, C, D, FIX)`.
    pub fn alpha(a: u32, b: u32, c: u32, d: u32, fix: u32) u64 {
        return @as(u64, a & 3) |
            (@as(u64, b & 3) << 2) |
            (@as(u64, c & 3) << 4) |
            (@as(u64, d & 3) << 6) |
            (@as(u64, fix & 0xFF) << 32);
    }
};

// ── GSGLOBAL field access ───────────────────────────────────────────────────
//
// gsKit exposes no setters for a few fields we need (notably the alpha-blend
// enable, which `gsKit_init_screen` turns OFF). The offsets below were verified
// with `offsetof(struct gsGlobal, …)` on this gsKit/N32 build.

const gs_offset: type = struct {
    const Width: usize = 56;
    const Height: usize = 60;
    const Mode: usize = 0;
    const Interlace: usize = 2;
    const DoubleBuffering: usize = 33;
    const ZBuffering: usize = 34;
    const PSM: usize = 196;
    const PrimAlphaEnable: usize = 216;
    const PrimAlpha: usize = 224;
    const PABE: usize = 232;
};

fn at(gs: *GSGLOBAL, comptime T: type, offset: usize) *T {
    return @ptrFromInt(@intFromPtr(gs) + offset);
}

pub const global: type = struct {
    pub fn width(gs: *GSGLOBAL) i32 {
        return at(gs, i32, gs_offset.Width).*;
    }
    pub fn height(gs: *GSGLOBAL) i32 {
        return at(gs, i32, gs_offset.Height).*;
    }
    pub fn setWidth(gs: *GSGLOBAL, value: i32) void {
        at(gs, i32, gs_offset.Width).* = value;
    }
    pub fn setHeight(gs: *GSGLOBAL, value: i32) void {
        at(gs, i32, gs_offset.Height).* = value;
    }
    pub fn setMode(gs: *GSGLOBAL, value: i16) void {
        at(gs, i16, gs_offset.Mode).* = value;
    }
    pub fn setInterlace(gs: *GSGLOBAL, value: i16) void {
        at(gs, i16, gs_offset.Interlace).* = value;
    }
    /// `GS_SETTING_ON` / `GS_SETTING_OFF`. Needed for per-primitive alpha.
    pub fn setPrimAlphaEnable(gs: *GSGLOBAL, on: bool) void {
        at(gs, c_int, gs_offset.PrimAlphaEnable).* = if (on) 1 else 0;
    }
    /// Z-buffering costs ~0.5 MB of VRAM; a 2D game does not need it.
    pub fn setZBuffering(gs: *GSGLOBAL, on: bool) void {
        at(gs, u8, gs_offset.ZBuffering).* = if (on) 1 else 0;
    }
    pub fn setDoubleBuffering(gs: *GSGLOBAL, on: bool) void {
        at(gs, u8, gs_offset.DoubleBuffering).* = if (on) 1 else 0;
    }
    pub fn setPabe(gs: *GSGLOBAL, value: u8) void {
        at(gs, u8, gs_offset.PABE).* = value;
    }
};

// ── dmaKit — EE DMA controller setup ────────────────────────────────────────

pub const dma: type = struct {
    pub const CHANNEL_VIF0: u32 = 0x0;
    pub const CHANNEL_VIF1: u32 = 0x1;
    pub const CHANNEL_GIF: u32 = 0x2;
    pub const CHANNEL_FROMIPU: u32 = 0x3;
    pub const CHANNEL_TOIPU: u32 = 0x4;
    pub const CHANNEL_SIF0: u32 = 0x5;
    pub const CHANNEL_SIF1: u32 = 0x6;
    pub const CHANNEL_SIF2: u32 = 0x7;
    pub const CHANNEL_FROMSPR: u32 = 0x8;
    pub const CHANNEL_TOSPR: u32 = 0x9;

    pub const CTRL_RELE_OFF: u32 = 0x0;
    pub const CTRL_RELE_ON: u32 = 0x1;
    pub const CTRL_MFD_OFF: u32 = 0x0;
    pub const CTRL_STS_UNSPEC: u32 = 0x0;
    pub const CTRL_STD_OFF: u32 = 0x0;
    pub const CTRL_RCYC_8: u32 = 0x0;

    pub extern fn dmaKit_init(rele: u32, mfd: u32, sts: u32, std: u32, rcyc: u32, fastwaitchannels: u16) c_int;
    pub extern fn dmaKit_chan_init(channel: u32) c_int;
    pub extern fn dmaKit_wait(channel: u16, timeout: u32) c_int;
    pub extern fn dmaKit_wait_fast() void;
    pub extern fn dmaKit_send(channel: u16, data: ?*anyopaque, size: u32) void;

    /// The usual gsKit DMA bring-up: init the controller and the GIF channel.
    pub fn init() c_int {
        const result: c_int = dmaKit_init(CTRL_RELE_OFF, CTRL_MFD_OFF, CTRL_STS_UNSPEC, CTRL_STD_OFF, CTRL_RCYC_8, @as(u16, 1) << CHANNEL_GIF);
        _ = dmaKit_chan_init(CHANNEL_GIF);
        return result;
    }
};

// ── gsKit core ──────────────────────────────────────────────────────────────

pub const core: type = struct {
    /// Equivalent of the `gsKit_init_global()` macro (which expands to
    /// `gsKit_init_global_custom(GS_RENDER_QUEUE_OS_POOLSIZE,
    /// GS_RENDER_QUEUE_PER_POOLSIZE)`).
    pub fn initGlobal() *GSGLOBAL {
        return gsKit_init_global_custom(RENDER_QUEUE_OS_POOLSIZE, RENDER_QUEUE_PER_POOLSIZE);
    }

    pub extern fn gsKit_init_global_custom(os_poolsize: c_int, per_poolsize: c_int) *GSGLOBAL;
    pub extern fn gsKit_init_screen(gs: *GSGLOBAL) void;
    pub extern fn gsKit_mode_switch(gs: *GSGLOBAL, mode: u8) void;
    pub extern fn gsKit_clear(gs: *GSGLOBAL, color: u64) void;
    pub extern fn gsKit_queue_exec(gs: *GSGLOBAL) void;
    pub extern fn gsKit_sync_flip(gs: *GSGLOBAL) void;
    pub extern fn gsKit_vsync_wait() void;
    pub extern fn gsKit_vsync_nowait() void;
    pub extern fn gsKit_setactive(gs: *GSGLOBAL) void;
    pub extern fn gsKit_set_scissor(gs: *GSGLOBAL, bounds: u64) void;
    pub extern fn gsKit_set_test(gs: *GSGLOBAL, preset: u8) void;
    pub extern fn gsKit_set_primalpha(gs: *GSGLOBAL, alpha: u64, per_pixel: u8) void;
    pub extern fn gsKit_set_texfilter(gs: *GSGLOBAL, filter: u8) void;
    pub extern fn gsKit_vram_alloc(gs: *GSGLOBAL, size: u32, kind: u8) u32;
    pub extern fn gsKit_vram_clear(gs: *GSGLOBAL) void;

    // Friendly aliases (call sites drop the `gsKit_` prefix).
    pub inline fn initScreen(gs: *GSGLOBAL) void {
        gsKit_init_screen(gs);
    }
    pub inline fn modeSwitch(gs: *GSGLOBAL, mode: u8) void {
        gsKit_mode_switch(gs, mode);
    }
    pub inline fn clear(gs: *GSGLOBAL, color: u64) void {
        gsKit_clear(gs, color);
    }
    pub inline fn queueExec(gs: *GSGLOBAL) void {
        gsKit_queue_exec(gs);
    }
    pub inline fn syncFlip(gs: *GSGLOBAL) void {
        gsKit_sync_flip(gs);
    }
};

// ── Primitives ──────────────────────────────────────────────────────────────

pub const prim: type = struct {
    pub extern fn gsKit_prim_sprite(gs: *GSGLOBAL, x1: f32, y1: f32, x2: f32, y2: f32, iz: c_int, color: u64) void;
    pub extern fn gsKit_prim_point(gs: *GSGLOBAL, x: f32, y: f32, iz: c_int, color: u64) void;
    pub extern fn gsKit_prim_line_3d(gs: *GSGLOBAL, x1: f32, y1: f32, iz1: c_int, x2: f32, y2: f32, iz2: c_int, color: u64) void;
    pub extern fn gsKit_prim_triangle_3d(gs: *GSGLOBAL, x1: f32, y1: f32, iz1: c_int, x2: f32, y2: f32, iz2: c_int, x3: f32, y3: f32, iz3: c_int, color: u64) void;
    pub extern fn gsKit_prim_triangle_gouraud_3d(gs: *GSGLOBAL, x1: f32, y1: f32, iz1: c_int, x2: f32, y2: f32, iz2: c_int, x3: f32, y3: f32, iz3: c_int, c1: u64, c2: u64, c3: u64) void;
    pub extern fn gsKit_prim_quad_3d(gs: *GSGLOBAL, x1: f32, y1: f32, iz1: c_int, x2: f32, y2: f32, iz2: c_int, x3: f32, y3: f32, iz3: c_int, x4: f32, y4: f32, iz4: c_int, color: u64) void;
    pub extern fn gsKit_prim_quad_gouraud_3d(gs: *GSGLOBAL, x1: f32, y1: f32, iz1: c_int, x2: f32, y2: f32, iz2: c_int, x3: f32, y3: f32, iz3: c_int, x4: f32, y4: f32, iz4: c_int, c1: u64, c2: u64, c3: u64, c4: u64) void;
    /// `LineStrip` is x/y pairs: [x1,y1, x2,y2, …].
    pub extern fn gsKit_prim_line_strip(gs: *GSGLOBAL, line_strip: [*]f32, segments: c_int, iz: c_int, color: u64) void;
    /// `TriStrip` is x/y pairs.
    pub extern fn gsKit_prim_triangle_strip(gs: *GSGLOBAL, tri_strip: [*]f32, segments: c_int, iz: c_int, color: u64) void;
    /// `TriFan` is x/y pairs; `vertices` is the vertex count.
    pub extern fn gsKit_prim_triangle_fan(gs: *GSGLOBAL, tri_fan: [*]f32, vertices: c_int, iz: c_int, color: u64) void;

    /// Filled axis-aligned rectangle — the workhorse 2D primitive.
    pub inline fn sprite(gs: *GSGLOBAL, x1: f32, y1: f32, x2: f32, y2: f32, iz: c_int, color: u64) void {
        gsKit_prim_sprite(gs, x1, y1, x2, y2, iz, color);
    }

    // ── `gsKit_prim_*` macro wrappers (they just add `iz` to each vertex) ──

    pub fn line(gs: *GSGLOBAL, x1: f32, y1: f32, x2: f32, y2: f32, iz: c_int, color: u64) void {
        gsKit_prim_line_3d(gs, x1, y1, iz, x2, y2, iz, color);
    }

    pub fn triangle(gs: *GSGLOBAL, x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32, iz: c_int, color: u64) void {
        gsKit_prim_triangle_3d(gs, x1, y1, iz, x2, y2, iz, x3, y3, iz, color);
    }

    pub fn quad(gs: *GSGLOBAL, x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32, x4: f32, y4: f32, iz: c_int, color: u64) void {
        gsKit_prim_quad_3d(gs, x1, y1, iz, x2, y2, iz, x3, y3, iz, x4, y4, iz, color);
    }
};

// ── Textures ────────────────────────────────────────────────────────────────

pub const texture: type = struct {
    pub extern fn gsKit_texture_bmp(gs: *GSGLOBAL, tex: *GSTEXTURE, path: [*c]const u8) c_int;
    pub extern fn gsKit_texture_png(gs: *GSGLOBAL, tex: *GSTEXTURE, path: [*c]const u8) c_int;
    pub extern fn gsKit_texture_jpeg(gs: *GSGLOBAL, tex: *GSTEXTURE, path: [*c]const u8) c_int;
    pub extern fn gsKit_texture_raw(gs: *GSGLOBAL, tex: *GSTEXTURE, path: [*c]const u8) c_int;
    pub extern fn gsKit_texture_upload(gs: *GSGLOBAL, tex: *GSTEXTURE) void;
    pub extern fn gsKit_texture_size(width: c_int, height: c_int, psm: c_int) u32;

    pub extern fn gsKit_prim_sprite_texture_3d(gs: *GSGLOBAL, tex: *const GSTEXTURE, x1: f32, y1: f32, iz1: c_int, ua: f32, va: f32, x2: f32, y2: f32, iz2: c_int, ub: f32, vb: f32, color: u64) void;

    /// Textured quad with independent UVs, used for rotated sprites.
    pub extern fn gsKit_prim_quad_texture_3d(gs: *GSGLOBAL, tex: *GSTEXTURE, x1: f32, y1: f32, iz1: c_int, ua: f32, va: f32, x2: f32, y2: f32, iz2: c_int, ub: f32, vb: f32, x3: f32, y3: f32, iz3: c_int, uc: f32, vc: f32, x4: f32, y4: f32, iz4: c_int, ud: f32, vd: f32, color: u64) void;

    /// `gsKit_prim_sprite_texture` macro wrapper.
    pub fn sprite(gs: *GSGLOBAL, tex: *const GSTEXTURE, x1: f32, y1: f32, ua: f32, va: f32, x2: f32, y2: f32, ub: f32, vb: f32, iz: c_int, color: u64) void {
        gsKit_prim_sprite_texture_3d(gs, tex, x1, y1, iz, ua, va, x2, y2, iz, ub, vb, color);
    }

    // gsKit's texture VRAM manager (keeps sources/GIF packets valid across
    // frames). `bind` before drawing, `nextFrame` at the end of each frame.
    pub extern fn gsKit_TexManager_init(gs: *GSGLOBAL) void;
    pub extern fn gsKit_TexManager_bind(gs: *GSGLOBAL, tex: *GSTEXTURE) c_uint;
    pub extern fn gsKit_TexManager_invalidate(gs: *GSGLOBAL, tex: *GSTEXTURE) void;
    pub extern fn gsKit_TexManager_free(gs: *GSGLOBAL, tex: *GSTEXTURE) void;
    pub extern fn gsKit_TexManager_nextFrame(gs: *GSGLOBAL) void;
};

// ── FONTM text (the ROM font in `rom0:FONTM`, no asset files) ───────────────

pub const fontm: type = struct {
    pub const ALIGN_LEFT: u8 = 0x00;
    pub const ALIGN_CENTER: u8 = 0x01;
    pub const ALIGN_RIGHT: u8 = 0x02;

    /// One FONTM glyph cell is 26x26 pixels; `scale` multiplies that.
    pub const GLYPH: f32 = 26.0;

    pub extern fn gsKit_init_fontm() *GSFONTM;
    pub extern fn gsKit_free_fontm(gs: *GSGLOBAL, font: *GSFONTM) void;
    /// Loads/unpacks the ROM font and uploads its texture to VRAM.
    /// Returns 0 on success, -1 if `rom0:FONTM` could not be read.
    pub extern fn gsKit_fontm_upload(gs: *GSGLOBAL, font: *GSFONTM) c_int;

    /// Core text call. `color` is a 32-bit RGBAQ (use `reg.font`).
    pub extern fn gsKit_fontm_print_scaled(gs: *GSGLOBAL, font: *GSFONTM, x: f32, y: f32, z: c_int, scale: f32, color: c_ulong, string: [*c]const u8) void;

    // Friendly aliases.
    pub fn init() *GSFONTM {
        return gsKit_init_fontm();
    }
    pub inline fn upload(gs: *GSGLOBAL, font: *GSFONTM) c_int {
        return gsKit_fontm_upload(gs, font);
    }
    pub inline fn printScaled(gs: *GSGLOBAL, font: *GSFONTM, x: f32, y: f32, z: c_int, scale: f32, color: c_ulong, string: [*c]const u8) void {
        gsKit_fontm_print_scaled(gs, font, x, y, z, scale, color, string);
    }

    /// `gsKit_fontm_print` macro wrapper (scale 1.0).
    pub fn print(gs: *GSGLOBAL, font: *GSFONTM, x: f32, y: f32, z: c_int, color: c_ulong, string: [*c]const u8) void {
        gsKit_fontm_print_scaled(gs, font, x, y, z, 1.0, color, string);
    }

    /// Width in pixels of `count` glyphs at `scale`, honouring `font.Spacing`.
    pub fn textWidth(font: *const GSFONTM, count: usize, scale: f32) f32 {
        return GLYPH * @as(f32, @floatFromInt(count)) * font.Spacing * scale;
    }
};
