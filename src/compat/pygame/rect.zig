// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! A pygame-style `Rect`.

const neko: type = @import("neko");

pub const Point: type = neko.Point;

/// A pygame-style rectangle with the helpers the game code reaches for.
pub const Rect: type = struct {
    x: i32 = 0,
    y: i32 = 0,
    w: i32 = 0,
    h: i32 = 0,

    pub fn init(x: i32, y: i32, w: i32, h: i32) Rect {
        return .{ .x = x, .y = y, .w = w, .h = h };
    }

    pub fn right(self: Rect) i32 {
        return self.x + self.w;
    }
    pub fn bottom(self: Rect) i32 {
        return self.y + self.h;
    }
    pub fn centerx(self: Rect) i32 {
        return self.x + @divTrunc(self.w, 2);
    }
    pub fn centery(self: Rect) i32 {
        return self.y + @divTrunc(self.h, 2);
    }
    pub fn center(self: Rect) Point {
        return .{ .x = self.centerx(), .y = self.centery() };
    }
    pub fn size(self: Rect) Point {
        return .{ .x = self.w, .y = self.h };
    }

    pub fn colliderect(self: Rect, other: Rect) bool {
        return self.x < other.right() and self.right() > other.x and
            self.y < other.bottom() and self.bottom() > other.y;
    }

    pub fn collidepoint(self: Rect, p: Point) bool {
        return p.x >= self.x and p.x < self.right() and p.y >= self.y and p.y < self.bottom();
    }

    /// Grows (positive) or shrinks (negative) the rect by `dw`/`dh` on both axes.
    pub fn inflate(self: Rect, dw: i32, dh: i32) Rect {
        return .{ .x = self.x - @divTrunc(dw, 2), .y = self.y - @divTrunc(dh, 2), .w = self.w + dw, .h = self.h + dh };
    }

    pub fn toNeko(self: Rect) neko.Rect {
        return neko.Rect.init(self.x, self.y, self.w, self.h);
    }
};
