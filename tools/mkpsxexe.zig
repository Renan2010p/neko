// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `mkpsxexe` — wraps a linked 32-bit little-endian MIPS ELF into a PlayStation
//! 1 PS-EXE (a 2048-byte "PS-X EXE" header followed by the loadable payload).
//!
//! Usage: `mkpsxexe <input.elf> <output.ps-exe>`

const std: type = @import("std");

fn rd16(d: []const u8, off: usize) u16 {
    return @as(u16, d[off]) | (@as(u16, d[off + 1]) << 8);
}

fn rd32(d: []const u8, off: usize) u32 {
    return @as(u32, d[off]) |
        (@as(u32, d[off + 1]) << 8) |
        (@as(u32, d[off + 2]) << 16) |
        (@as(u32, d[off + 3]) << 24);
}

fn wr32(d: []u8, off: usize, v: u32) void {
    d[off] = @truncate(v);
    d[off + 1] = @truncate(v >> 8);
    d[off + 2] = @truncate(v >> 16);
    d[off + 3] = @truncate(v >> 24);
}

pub fn main(init: std.process.Init) !void {
    const io = init.io;
    const gpa = init.gpa;

    var args = try init.minimal.args.iterateAllocator(gpa);
    defer args.deinit();
    _ = args.next(); // program name
    const in_path = args.next() orelse return error.Usage;
    const out_path = args.next() orelse return error.Usage;

    const dir = std.Io.Dir.cwd();
    const data = try dir.readFileAlloc(io, in_path, gpa, std.Io.Limit.limited(1 << 24));
    defer gpa.free(data);

    if (data.len < 52 or !std.mem.eql(u8, data[0..4], "\x7fELF") or data[4] != 1 or data[5] != 1) {
        return error.NotElf32LE;
    }

    const e_entry = rd32(data, 0x18);
    const e_phoff = rd32(data, 0x1C);
    const e_phentsize = rd16(data, 0x2A);
    const e_phnum = rd16(data, 0x2C);

    var load: u32 = 0xFFFFFFFF;
    var end: u32 = 0;
    var i: usize = 0;
    while (i < e_phnum) : (i += 1) {
        const o = e_phoff + i * e_phentsize;
        const p_type = rd32(data, o);
        const p_paddr = rd32(data, o + 12);
        const p_filesz = rd32(data, o + 16);
        if (p_type == 1 and p_filesz > 0) {
            if (p_paddr < load) load = p_paddr;
            if (p_paddr + p_filesz > end) end = p_paddr + p_filesz;
        }
    }
    if (load == 0xFFFFFFFF) return error.NoLoadableSegments;

    const payload: u32 = end - load;
    const padded: u32 = (payload + 2047) / 2048 * 2048;

    const out = try gpa.alloc(u8, 2048 + padded);
    defer gpa.free(out);
    @memset(out, 0);

    @memcpy(out[0..8], "PS-X EXE");
    wr32(out, 0x10, e_entry); // program counter
    wr32(out, 0x14, 0); // initial $gp (the startup sets it itself)
    wr32(out, 0x18, load); // load address
    wr32(out, 0x1C, padded); // payload size
    wr32(out, 0x30, 0x801FFFF0); // initial $sp (informational)

    i = 0;
    while (i < e_phnum) : (i += 1) {
        const o = e_phoff + i * e_phentsize;
        const p_type = rd32(data, o);
        const p_offset = rd32(data, o + 4);
        const p_paddr = rd32(data, o + 12);
        // p_filesz was read above as needed; keep the offsets.
        const p_filesz = rd32(data, o + 16);
        if (p_type == 1 and p_filesz > 0) {
            const dst: usize = 2048 + (p_paddr - load);
            @memcpy(out[dst .. dst + p_filesz], data[p_offset .. p_offset + p_filesz]);
        }
    }

    try dir.writeFile(io, .{ .sub_path = out_path, .data = out });
}
