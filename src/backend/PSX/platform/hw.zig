// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! PlayStation 1 hardware rendering core (pure Zig, freestanding).
//!
//! Instead of a CPU framebuffer (which cannot reach 60 fps on the FPU-less
//! R3000A), everything is drawn by the GPU: flat quads, fills and CLUT-textured
//! quads are accumulated into an ordering table (OT) and streamed to the GPU
//! with one DMA transfer per frame. The GTE (COP2) is exposed for 3D games.
//!
//! Textures are stored 4bpp CLUT in VRAM (4 pixels per 16-bit word = 4x less
//! VRAM/DMA than 15bpp) — the "compression" that keeps the bandwidth at 60 Hz.

const std: type = @import("std");

pub const W: u32 = 320;
pub const H: u32 = 240;

pub const GP0: *volatile u32 = @ptrFromInt(0x1F801810);
pub const GP1: *volatile u32 = @ptrFromInt(0x1F801814);
const I_STAT: *volatile u32 = @ptrFromInt(0x1F801070);
const I_MASK: *volatile u32 = @ptrFromInt(0x1F801074);
const DMA_DPCR: *volatile u32 = @ptrFromInt(0x1F8010F0);
const DMA_MADR: *volatile u32 = @ptrFromInt(0x1F8010A0);
const DMA_BCR: *volatile u32 = @ptrFromInt(0x1F8010A4);
const DMA_CHCR: *volatile u32 = @ptrFromInt(0x1F8010A8);

pub fn waitGpu() void {
    while ((GP1.* & 0x04000000) == 0) {}
}

pub fn vsync() void {
    while ((I_STAT.* & 1) == 0) {}
    I_STAT.* = 0xFFFFFFFE;
}

pub fn dmaInit() void {
    DMA_DPCR.* |= 0x0800;
    DMA_CHCR.* = 0;
    GP1.* = 0x04000000;
    I_MASK.* |= 1;
}

pub fn gpuInit() void {
    GP1.* = 0x00000000;
    GP1.* = 0x06000000 | 0x260 | (0xC60 << 12);
    GP1.* = 0x07000000 | 0x10 | (0x100 << 10);
    GP1.* = 0x08000001; // 320x240 NTSC 15bpp
    GP1.* = 0x05000000;
    GP1.* = 0x03000000;
    waitGpu();
    GP0.* = 0xE3000000;
    GP0.* = 0xE4000000 | (511 << 10) | 1023; // full-VRAM drawing area
    GP0.* = 0xE5000000;
}

/// Immediate GPU fill (24-bit RGB), absolute VRAM coords.
pub fn fill(x: u32, y: u32, w: u32, h: u32, rgb: u32) void {
    waitGpu();
    GP0.* = 0x02000000 | (rgb & 0xFFFFFF);
    GP0.* = (y << 16) | x;
    GP0.* = (h << 16) | w;
}

/// Uploads `w` 16-bit words per row for `h` rows to VRAM at (x,y) over DMA2.
pub fn uploadWords(x: u32, y: u32, w: u32, h: u32, data: []const u16) void {
    const words: u32 = w * h;
    while ((DMA_CHCR.* & 0x01000000) != 0) {}
    GP1.* = 0x04000000;
    waitGpu();
    GP0.* = 0xA0000000;
    GP0.* = (y << 16) | x;
    GP0.* = (h << 16) | w;
    GP1.* = 0x04000002;
    DMA_MADR.* = @intCast(@intFromPtr(data.ptr) & 0x1FFFFFFF);
    DMA_BCR.* = 16 | ((words / 16) << 16);
    DMA_CHCR.* = 0x00000001 | 0x00000200 | 0x01000000;
    while ((DMA_CHCR.* & 0x01000000) != 0) {}
    GP1.* = 0x04000000;
}

// ── Ordering table / display list ────────────────────────────────────────────

pub const OT_LEN: u32 = 256;
pub var ot: [OT_LEN]u32 = undefined;
pub var prim_buf: [49152]u32 align(4) = undefined;
pub var prim_next: u32 = 0;

pub fn otClear() void {
    var i: u32 = 0;
    while (i < OT_LEN) : (i += 1) {
        ot[i] = if (i + 1 < OT_LEN) (@intFromPtr(&ot[i + 1]) & 0x1FFFFC) else 0xFFFFFF;
    }
    prim_next = 0;
}

fn link(bucket: u32, addr: u32) void {
    ot[bucket] = addr & 0x1FFFFC;
}

/// Flat opaque quad (GP0 28h) at screen coords; bit 24-31 = command.
pub fn addFlatQuad(bucket: u32, x: i32, y: i32, w: i32, h: i32, rgb: u32) void {
    const b = prim_next;
    const next = ot[bucket] & 0x1FFFFC;
    link(bucket, @intFromPtr(&prim_buf[b]));
    const p = prim_buf[b .. b + 6];
    p[0] = (5 << 24) | next;
    p[1] = (0x28 << 24) | (rgb & 0xFFFFFF);
    p[2] = xy(x, y);
    p[3] = xy(x + w, y);
    p[4] = xy(x, y + h);
    p[5] = xy(x + w, y + h);
    prim_next = b + 6;
}

/// Textured quad (GP0 2Ch), 4bpp CLUT. `uv` are texture coords, `clut`/`tpage`
/// select the palette and texture page.
pub fn addTexQuad(bucket: u32, dst: [4][2]i32, uv: [4][2]u32, clut: u32, tpage: u32) void {
    const b = prim_next;
    const next = ot[bucket] & 0x1FFFFC;
    link(bucket, @intFromPtr(&prim_buf[b]));
    const p = prim_buf[b .. b + 10];
    p[0] = (9 << 24) | next;
    p[1] = (0x2C << 24) | 0x808080;
    p[2] = xy(dst[0][0], dst[0][1]);
    p[3] = (clut << 16) | ((uv[0][1] & 0xFF) << 8) | (uv[0][0] & 0xFF);
    p[4] = xy(dst[1][0], dst[1][1]);
    p[5] = (tpage << 16) | ((uv[1][1] & 0xFF) << 8) | (uv[1][0] & 0xFF);
    p[6] = xy(dst[2][0], dst[2][1]);
    p[7] = ((uv[2][1] & 0xFF) << 8) | (uv[2][0] & 0xFF);
    p[8] = xy(dst[3][0], dst[3][1]);
    p[9] = ((uv[3][1] & 0xFF) << 8) | (uv[3][0] & 0xFF);
    prim_next = b + 10;
}

fn xy(x: i32, y: i32) u32 {
    return (@as(u32, @bitCast(y)) << 16) | (@as(u32, @bitCast(x)) & 0xFFFF);
}

pub fn drawOT() void {
    while ((DMA_CHCR.* & 0x01000000) != 0) {}
    GP1.* = 0x04000002;
    DMA_MADR.* = @intCast(@intFromPtr(&ot) & 0x1FFFFFFF);
    DMA_BCR.* = 0;
    DMA_CHCR.* = 0x01000401; // linked list (mode 2) + start
    while ((DMA_CHCR.* & 0x01000000) != 0) {}
    GP1.* = 0x04000000;
}

// ── GTE (COP2) — for 3D games ────────────────────────────────────────────────
//
// Modern binutils dropped the GTE mnemonics; the COP2 commands are emitted as
// raw `.word`s (verified against GNU as and PSn00bSDK).

inline fn mtc2(comptime reg: u5, v: u32) void {
    switch (reg) {
        0 => asm volatile ("mtc2 %[v], $0"
            :
            : [v] "r" (v),
        ),
        1 => asm volatile ("mtc2 %[v], $1"
            :
            : [v] "r" (v),
        ),
        2 => asm volatile ("mtc2 %[v], $2"
            :
            : [v] "r" (v),
        ),
        3 => asm volatile ("mtc2 %[v], $3"
            :
            : [v] "r" (v),
        ),
        4 => asm volatile ("mtc2 %[v], $4"
            :
            : [v] "r" (v),
        ),
        5 => asm volatile ("mtc2 %[v], $5"
            :
            : [v] "r" (v),
        ),
        else => @compileError("mtc2"),
    }
}

inline fn ctc2(comptime reg: u5, v: u32) void {
    switch (reg) {
        // CTC2 $8,$rd = 0x48C80000 | (rd<<11); GPR pinned to $t0.
        0 => asm volatile (".word 0x48C80000"
            :
            : [v] "{r8}" (v),
        ),
        1 => asm volatile (".word 0x48C80800"
            :
            : [v] "{r8}" (v),
        ),
        2 => asm volatile (".word 0x48C81000"
            :
            : [v] "{r8}" (v),
        ),
        3 => asm volatile (".word 0x48C81800"
            :
            : [v] "{r8}" (v),
        ),
        4 => asm volatile (".word 0x48C82000"
            :
            : [v] "{r8}" (v),
        ),
        5 => asm volatile (".word 0x48C82800"
            :
            : [v] "{r8}" (v),
        ),
        6 => asm volatile (".word 0x48C83000"
            :
            : [v] "{r8}" (v),
        ),
        7 => asm volatile (".word 0x48C83800"
            :
            : [v] "{r8}" (v),
        ),
        24 => asm volatile (".word 0x48C8C000"
            :
            : [v] "{r8}" (v),
        ),
        25 => asm volatile (".word 0x48C8C800"
            :
            : [v] "{r8}" (v),
        ),
        26 => asm volatile (".word 0x48C8D000"
            :
            : [v] "{r8}" (v),
        ),
        else => @compileError("ctc2"),
    }
}

inline fn mfc2(comptime reg: u5) u32 {
    var v: u32 = undefined;
    switch (reg) {
        12 => asm volatile ("mfc2 %[v], $12"
            : [v] "=r" (v),
        ),
        13 => asm volatile ("mfc2 %[v], $13"
            : [v] "=r" (v),
        ),
        14 => asm volatile ("mfc2 %[v], $14"
            : [v] "=r" (v),
        ),
        17 => asm volatile ("mfc2 %[v], $17"
            : [v] "=r" (v),
        ),
        18 => asm volatile ("mfc2 %[v], $18"
            : [v] "=r" (v),
        ),
        19 => asm volatile ("mfc2 %[v], $19"
            : [v] "=r" (v),
        ),
        else => @compileError("mfc2"),
    }
    return v;
}

inline fn rtps() void {
    asm volatile ("nop\nnop\n.word 0x4A180001");
}
inline fn rtpt() void {
    asm volatile ("nop\nnop\n.word 0x4A280030");
}

fn packXY(x: i32, y: i32) u32 {
    return (@as(u32, @bitCast(y)) << 16) | (@as(u32, @bitCast(x)) & 0xFFFF);
}

pub fn loadVertex(comptime pair: u5, vx: i32, vy: i32, vz: i32) void {
    mtc2(pair * 2, packXY(vx, vy));
    mtc2(pair * 2 + 1, @bitCast(vz));
}

pub fn sxy(r: u32) [2]i32 {
    const x: i32 = @as(i16, @bitCast(@as(u16, @truncate(r))));
    const y: i32 = @as(i16, @bitCast(@as(u16, @truncate(r >> 16))));
    return .{ x, y };
}

pub fn sz(r: u32) u32 {
    return @as(u16, @truncate(r));
}

pub fn rtp() void {
    rtpt();
}
