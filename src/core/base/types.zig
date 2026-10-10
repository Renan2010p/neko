//! Shared engine data types. No platform code lives here.
//!
//! These are plain value types (plus a few convenience helpers) that both game
//! code and backends use. Keeping them platform-free is what lets a game compile
//! against the SDL2 backend or the PS2 backend without changes.

const std: type = @import("std");
const Allocator: type = std.mem.Allocator;

/// An RGBA colour, 0-255 per channel.
///
/// ```
/// const red = neko.Color.red;
/// const fade = neko.Color.rgb(255, 0, 0).withAlpha(128);
/// const sky = neko.Color.hex(0x66ccff); // opaque
/// ```
pub const Color: type = struct {
    r: u8,
    g: u8,
    b: u8,
    a: u8 = 255,

    // ── Named colours ────────────────────────────────────────────────────

    pub const black: Color = .{ .r = 0, .g = 0, .b = 0 };
    pub const white: Color = .{ .r = 255, .g = 255, .b = 255 };
    pub const red: Color = .{ .r = 255, .g = 0, .b = 0 };
    pub const green: Color = .{ .r = 0, .g = 255, .b = 0 };
    pub const blue: Color = .{ .r = 0, .g = 0, .b = 255 };
    pub const yellow: Color = .{ .r = 255, .g = 255, .b = 0 };
    pub const cyan: Color = .{ .r = 0, .g = 255, .b = 255 };
    pub const magenta: Color = .{ .r = 255, .g = 0, .b = 255 };
    pub const transparent: Color = .{ .r = 0, .g = 0, .b = 0, .a = 0 };

    // ── Constructors ─────────────────────────────────────────────────────

    /// An opaque colour.
    pub fn rgb(r: u8, g: u8, b: u8) Color {
        return .{ .r = r, .g = g, .b = b };
    }

    /// A colour with explicit alpha.
    pub fn rgba(r: u8, g: u8, b: u8, a: u8) Color {
        return .{ .r = r, .g = g, .b = b, .a = a };
    }

    /// An opaque colour from a `0xRRGGBB` literal.
    pub fn hex(value: u32) Color {
        return .{
            .r = @intCast((value >> 16) & 0xff),
            .g = @intCast((value >> 8) & 0xff),
            .b = @intCast(value & 0xff),
        };
    }

    // ── Helpers ──────────────────────────────────────────────────────────

    /// The same colour with a different alpha.
    pub fn withAlpha(self: Color, a: u8) Color {
        return .{ .r = self.r, .g = self.g, .b = self.b, .a = a };
    }

    /// Scales the alpha by `factor` (1.0 keeps it, 0.5 halves it).
    pub fn fade(self: Color, factor: f32) Color {
        const scaled: f32 = @as(f32, @floatFromInt(self.a)) * factor;
        return self.withAlpha(@intFromFloat(@max(0.0, @min(255.0, scaled))));
    }

    /// Linearly blends two colours. `t` is clamped to 0..1.
    pub fn lerp(a: Color, b: Color, t: f32) Color {
        const c: f32 = @max(0.0, @min(1.0, t));
        return .{
            .r = mixChannel(a.r, b.r, c),
            .g = mixChannel(a.g, b.g, c),
            .b = mixChannel(a.b, b.b, c),
            .a = mixChannel(a.a, b.a, c),
        };
    }

    fn mixChannel(a: u8, b: u8, t: f32) u8 {
        const af: f32 = @floatFromInt(a);
        const bf: f32 = @floatFromInt(b);
        return @intFromFloat(@round(af + (bf - af) * t));
    }
};

/// A 2D point in logical pixels.
pub const Point: type = struct {
    x: i32 = 0,
    y: i32 = 0,

    pub const zero: Point = .{ .x = 0, .y = 0 };

    pub fn init(x: i32, y: i32) Point {
        return .{ .x = x, .y = y };
    }

    pub fn add(self: Point, other: Point) Point {
        return .{ .x = self.x + other.x, .y = self.y + other.y };
    }

    pub fn sub(self: Point, other: Point) Point {
        return .{ .x = self.x - other.x, .y = self.y - other.y };
    }

    pub fn scale(self: Point, factor: i32) Point {
        return .{ .x = self.x * factor, .y = self.y * factor };
    }

    pub fn offset(self: Point, dx: i32, dy: i32) Point {
        return .{ .x = self.x + dx, .y = self.y + dy };
    }

    pub fn eql(self: Point, other: Point) bool {
        return self.x == other.x and self.y == other.y;
    }
};

/// An axis-aligned rectangle in logical pixels. `x`/`y` is the top-left corner.
pub const Rect: type = struct {
    x: i32 = 0,
    y: i32 = 0,
    w: i32 = 0,
    h: i32 = 0,

    pub fn init(x: i32, y: i32, w: i32, h: i32) Rect {
        return .{ .x = x, .y = y, .w = w, .h = h };
    }

    /// Builds a rectangle from a position and a size.
    pub fn fromPointSize(pos: Point, dim: Point) Rect {
        return .{ .x = pos.x, .y = pos.y, .w = dim.x, .h = dim.y };
    }

    pub fn right(self: Rect) i32 {
        return self.x + self.w;
    }

    pub fn bottom(self: Rect) i32 {
        return self.y + self.h;
    }

    pub fn center(self: Rect) Point {
        return .{ .x = self.x + @divTrunc(self.w, 2), .y = self.y + @divTrunc(self.h, 2) };
    }

    pub fn size(self: Rect) Point {
        return .{ .x = self.w, .y = self.h };
    }

    /// True when `p` lies inside the rectangle (top-left inclusive).
    pub fn contains(self: Rect, p: Point) bool {
        return p.x >= self.x and p.y >= self.y and p.x < self.right() and p.y < self.bottom();
    }

    /// True when two rectangles overlap.
    pub fn overlaps(self: Rect, other: Rect) bool {
        return self.x < other.right() and other.x < self.right() and
            self.y < other.bottom() and other.y < self.bottom();
    }

    /// A rectangle shrunk by `amount` on every side (clamped to non-negative).
    pub fn inset(self: Rect, amount: i32) Rect {
        const w: i32 = @max(0, self.w - amount * 2);
        const h: i32 = @max(0, self.h - amount * 2);
        return .{ .x = self.x + amount, .y = self.y + amount, .w = w, .h = h };
    }
};

/// A vertex for textured meshes (`neko.texture.geometry`). The field layout
/// matches SDL's `SDL_Vertex` (x, y, r, g, b, a, u, v) so the backend can pass
/// a slice straight through.
pub const Vertex: type = extern struct {
    x: f32,
    y: f32,
    r: u8,
    g: u8,
    b: u8,
    a: u8,
    u: f32,
    v: f32,

    pub fn init(x: f32, y: f32, u: f32, v: f32, color: Color) Vertex {
        return Vertex{ .x = x, .y = y, .r = color.r, .g = color.g, .b = color.b, .a = color.a, .u = u, .v = v };
    }
};

/// One display mode reported by the backend.
pub const DisplayMode: type = struct {
    width: i32,
    height: i32,
    refresh_hz: i32,
};

/// One renderer a backend can use (e.g. "opengl", "vulkan", "software").
pub const RenderInfo: type = struct {
    name: []const u8,
};

/// Opaque handle to a GPU texture, or to an offscreen render target.
pub const TextureHandle: type = struct {
    id: u32,
};

/// Opaque handle to a loaded sound.
pub const SoundHandle: type = struct {
    id: u32,
};

/// Opaque handle to a compiled shader program.
pub const ShaderHandle: type = struct {
    id: u32,
};

/// The name of the platform backend this build selected, e.g. `"sdl2"`.
///
/// The core does **not** enumerate backends: a backend declares its own name,
/// so adding one never touches `src/core/**`. Prefer asking
/// `neko.Backend.supports(.feature)` over comparing names.
pub const BackendKind: type = struct {
    /// The `-Dbackend=<name>` value (`"sdl2"`, `"ps2"`, `"headless"`, …).
    name: []const u8,

    /// True when this backend's name equals `other`.
    pub fn eql(self: BackendKind, other: []const u8) bool {
        return std.mem.eql(u8, self.name, other);
    }
};

/// A Discord Rich Presence payload. Games fill this and hand it to
/// `neko.net.discord_rich_presence.set`; the backend sends it to Discord when
/// it is running. Empty strings and zero timestamps are omitted.
pub const DiscordPresence: type = struct {
    details: []const u8 = "",
    state: []const u8 = "",
    /// Unix seconds to show "elapsed"; 0 omits it.
    start_timestamp: i64 = 0,
    /// Unix seconds to show "remaining"; 0 omits it.
    end_timestamp: i64 = 0,
    large_image: []const u8 = "",
    large_text: []const u8 = "",
    small_image: []const u8 = "",
    small_text: []const u8 = "",
    party_size: u32 = 0,
    party_max: u32 = 0,
};

/// Everything a backend needs to open a window and run.
///
/// This is bootstrap data: the engine core stores only `allocator` and
/// `assets_dir`; the rest is consumed by the backend chosen at build time.
pub const Config: type = struct {
    allocator: Allocator,
    /// The process I/O handle (from `main(init: std.process.Init)`), or `null`
    /// on a target without one (freestanding/bare-metal). Read only by the
    /// backend, which uses it to satisfy the `Backend` file operations (save
    /// files, asset access). The core never touches it directly.
    io: ?std.Io = null,
    title: []const u8,
    width: u32,
    height: u32,
    /// Directory the backend resolves relative asset paths against, e.g. the
    /// default font. Each game passes its own (assets are per game).
    assets_dir: []const u8 = "assets",
    fullscreen: bool = false,
    vsync: bool = true,
};

// ── Tests ────────────────────────────────────────────────────────────────────

const testing: type = @import("std").testing;

test "Color.rgb keeps alpha opaque" {
    const c: Color = Color.rgb(1, 2, 3);
    try testing.expectEqual(@as(u8, 1), c.r);
    try testing.expectEqual(@as(u8, 255), c.a);
}

test "Color.hex decodes RRGGBB" {
    const c: Color = Color.hex(0x66ccff);
    try testing.expectEqual(@as(u8, 0x66), c.r);
    try testing.expectEqual(@as(u8, 0xcc), c.g);
    try testing.expectEqual(@as(u8, 0xff), c.b);
}

test "Color.lerp blends channels" {
    const c: Color = Color.lerp(Color.black, Color.white, 0.5);
    try testing.expectEqual(@as(u8, 128), c.r);
}

test "Rect.contains and overlaps" {
    const r: Rect = Rect.init(0, 0, 10, 10);
    try testing.expect(r.contains(Point.init(9, 9)));
    try testing.expect(!r.contains(Point.init(10, 10)));
    try testing.expect(r.overlaps(Rect.init(5, 5, 10, 10)));
    try testing.expect(!r.overlaps(Rect.init(20, 20, 1, 1)));
}

test "Point arithmetic" {
    try testing.expect(Point.init(1, 2).add(Point.init(3, 4)).eql(Point.init(4, 6)));
    try testing.expect(Point.init(4, 6).sub(Point.init(3, 4)).eql(Point.init(1, 2)));
}
