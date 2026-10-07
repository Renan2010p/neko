// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `neko.mesh3d` — the built-in 3D meshes and their vertex layout.
//!
//! A `Vertex` is position + normal + uv, interleaved for a single VBO. The
//! built-in meshes are unit-sized around the origin, so an `Entity` scales them
//! (a `model='cube'` in Ursina is a 1×1×1 cube centred at `origin`).

const std: type = @import("std");

/// One vertex as uploaded to the GPU: `position(3) normal(3) uv(2)` = 32 bytes.
pub const Vertex: type = extern struct {
    position: [3]f32,
    normal: [3]f32,
    uv: [2]f32,
};

/// The built-in mesh kinds a backend can draw without being handed geometry.
pub const Kind: type = enum(u32) {
    cube = 0,
    quad = 1,
    plane = 2,
};

/// Looks a mesh kind up by Ursina's `model=` name. `null` when unknown.
pub fn kindFromName(name: []const u8) ?Kind {
    if (std.mem.eql(u8, name, "cube")) return .cube;
    if (std.mem.eql(u8, name, "quad")) return .quad;
    if (std.mem.eql(u8, name, "plane")) return .plane;
    return null;
}

const uv: [4][2]f32 = .{ .{ 0, 0 }, .{ 0, 1 }, .{ 1, 1 }, .{ 1, 0 } };

/// A unit cube (−.5..+.5) with per-face normals: 6 faces × 4 vertices.
const cube_vertices: [24]Vertex = blk: {
    @setEvalBranchQuota(20000);
    const faces = [6]struct { n: [3]f32, c: [4][3]f32 }{
        .{ .n = .{ 1, 0, 0 }, .c = .{ .{ 0.5, -0.5, -0.5 }, .{ 0.5, 0.5, -0.5 }, .{ 0.5, 0.5, 0.5 }, .{ 0.5, -0.5, 0.5 } } },
        .{ .n = .{ -1, 0, 0 }, .c = .{ .{ -0.5, -0.5, 0.5 }, .{ -0.5, 0.5, 0.5 }, .{ -0.5, 0.5, -0.5 }, .{ -0.5, -0.5, -0.5 } } },
        .{ .n = .{ 0, 1, 0 }, .c = .{ .{ -0.5, 0.5, 0.5 }, .{ 0.5, 0.5, 0.5 }, .{ 0.5, 0.5, -0.5 }, .{ -0.5, 0.5, -0.5 } } },
        .{ .n = .{ 0, -1, 0 }, .c = .{ .{ -0.5, -0.5, -0.5 }, .{ 0.5, -0.5, -0.5 }, .{ 0.5, -0.5, 0.5 }, .{ -0.5, -0.5, 0.5 } } },
        .{ .n = .{ 0, 0, 1 }, .c = .{ .{ -0.5, -0.5, 0.5 }, .{ 0.5, -0.5, 0.5 }, .{ 0.5, 0.5, 0.5 }, .{ -0.5, 0.5, 0.5 } } },
        .{ .n = .{ 0, 0, -1 }, .c = .{ .{ 0.5, -0.5, -0.5 }, .{ -0.5, -0.5, -0.5 }, .{ -0.5, 0.5, -0.5 }, .{ 0.5, 0.5, -0.5 } } },
    };
    var out: [24]Vertex = undefined;
    for (faces, 0..) |f, fi| {
        for (f.c, 0..) |c, ci| {
            out[fi * 4 + ci] = .{ .position = c, .normal = f.n, .uv = uv[ci] };
        }
    }
    break :blk out;
};

const cube_indices: [36]u32 = blk: {
    var out: [36]u32 = undefined;
    var fi: usize = 0;
    while (fi < 6) : (fi += 1) {
        const base: u32 = @intCast(fi * 4);
        out[fi * 6 + 0] = base + 0;
        out[fi * 6 + 1] = base + 1;
        out[fi * 6 + 2] = base + 2;
        out[fi * 6 + 3] = base + 0;
        out[fi * 6 + 4] = base + 2;
        out[fi * 6 + 5] = base + 3;
    }
    break :blk out;
};

/// A unit quad in the XY plane, facing `+Z`.
const quad_vertices: [4]Vertex = .{
    .{ .position = .{ -0.5, -0.5, 0 }, .normal = .{ 0, 0, 1 }, .uv = uv[0] },
    .{ .position = .{ 0.5, -0.5, 0 }, .normal = .{ 0, 0, 1 }, .uv = uv[1] },
    .{ .position = .{ 0.5, 0.5, 0 }, .normal = .{ 0, 0, 1 }, .uv = uv[2] },
    .{ .position = .{ -0.5, 0.5, 0 }, .normal = .{ 0, 0, 1 }, .uv = uv[3] },
};

/// A unit quad in the XZ plane, facing `+Y` (Ursina's ground plane).
const plane_vertices: [4]Vertex = .{
    .{ .position = .{ -0.5, 0, -0.5 }, .normal = .{ 0, 1, 0 }, .uv = uv[0] },
    .{ .position = .{ 0.5, 0, -0.5 }, .normal = .{ 0, 1, 0 }, .uv = uv[1] },
    .{ .position = .{ 0.5, 0, 0.5 }, .normal = .{ 0, 1, 0 }, .uv = uv[2] },
    .{ .position = .{ -0.5, 0, 0.5 }, .normal = .{ 0, 1, 0 }, .uv = uv[3] },
};

const quad_indices: [6]u32 = .{ 0, 1, 2, 0, 2, 3 };

/// The vertices of a built-in mesh.
pub fn vertices(kind: Kind) []const Vertex {
    return switch (kind) {
        .cube => &cube_vertices,
        .quad => &quad_vertices,
        .plane => &plane_vertices,
    };
}

/// The triangle indices of a built-in mesh.
pub fn indices(kind: Kind) []const u32 {
    return switch (kind) {
        .cube => &cube_indices,
        .quad => &quad_indices,
        .plane => &quad_indices,
    };
}

// ── Tests ────────────────────────────────────────────────────────────────────

const testing: type = @import("std").testing;

test "mesh kinds have matching vertices and indices" {
    for ([_]Kind{ .cube, .quad, .plane }) |kind| {
        const verts: []const Vertex = vertices(kind);
        const idx: []const u32 = indices(kind);
        try testing.expect(verts.len > 0);
        try testing.expect(idx.len % 3 == 0);
        for (idx) |i| try testing.expect(i < verts.len);
    }
}

test "cube is unit-sized and faces outward" {
    const verts: []const Vertex = vertices(.cube);
    try testing.expectEqual(@as(usize, 24), verts.len);
    // The +X face normal points to +X.
    try testing.expectApproxEqAbs(@as(f32, 1), verts[0].normal[0], 0.0001);
}
