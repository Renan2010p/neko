# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Renan Lucas Vieira Hilário
"""``ursina.color`` — RGBA colors in 0..1, plus the named palette and ``hsv``."""

from __future__ import annotations

import colorsys


def _clamp(v):
    return 0.0 if v < 0 else (1.0 if v > 1 else float(v))


class Color:
    __slots__ = ("r", "g", "b", "a")

    def __init__(self, r=1.0, g=1.0, b=1.0, a=1.0):
        self.r, self.g, self.b, self.a = _clamp(r), _clamp(g), _clamp(b), _clamp(a)

    def to_rgba8(self):
        return (
            round(self.r * 255),
            round(self.g * 255),
            round(self.b * 255),
            round(self.a * 255),
        )

    def packed(self):
        """The 0xAARRGGBB integer ``_neko.render3d_draw`` expects."""
        r, g, b, a = self.to_rgba8()
        return (a << 24) | (r << 16) | (g << 8) | b

    @property
    def rgb(self):
        return (self.r, self.g, self.b)

    def __iter__(self):
        return iter((self.r, self.g, self.b, self.a))

    def __repr__(self):
        return f"Color({self.r:g}, {self.g:g}, {self.b:g}, {self.a:g})"


def rgb(r, g, b, a=1.0):
    # Ursina's rgb() takes 0..255.
    return Color(r / 255.0, g / 255.0, b / 255.0, a)


def hsv(h, s, v, a=1.0):
    # Ursina's hsv() takes h in 0..360 and s/v in 0..1.
    r, g, b = colorsys.hsv_to_rgb((h % 360) / 360.0, _clamp(s), _clamp(v))
    return Color(r, g, b, a)


def hex(value):
    value = value.lstrip("#")
    return rgb(int(value[0:2], 16), int(value[2:4], 16), int(value[4:6], 16))


white = Color(1, 1, 1)
black = Color(0, 0, 0)
red = Color(1, 0, 0)
green = Color(0, 1, 0)
blue = Color(0, 0, 1)
yellow = Color(1, 1, 0)
orange = Color(1, 0.5, 0)
magenta = Color(1, 0, 1)
cyan = Color(0, 1, 1)
gray = Color(0.5, 0.5, 0.5)
clear = Color(0, 0, 0, 0)
