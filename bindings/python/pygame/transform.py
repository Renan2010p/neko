# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Renan Lucas Vieira Hilário
"""``pygame.transform`` — scale, flips and rotation (all in Zig)."""

import math

from . import _neko


def scale(surface, size, dest_surface=None):
    w, h = size
    w, h = int(w), int(h)
    out = dest_surface if dest_surface is not None else _neko.new_surface(w, h, surface.has_alpha())
    _neko.scale(surface, surface.get_width(), surface.get_height(), out, w, h)
    return out


def smoothscale(surface, size, dest_surface=None):
    return scale(surface, size, dest_surface)


def flip(surface, flip_x, flip_y):
    w, h = surface.get_width(), surface.get_height()
    out = surface.copy()
    if flip_x:
        tmp = _neko.new_surface(w, h, surface.has_alpha())
        _neko.flip_x(surface, w, h, tmp)
        out = tmp
    if flip_y:
        tmp = _neko.new_surface(w, h, surface.has_alpha())
        _neko.flip_y(surface, w, h, tmp)
        out = tmp
    out.set_alpha(surface.get_alpha())
    return out


def rotate(surface, angle):
    w, h = surface.get_width(), surface.get_height()
    rad = math.radians(angle)
    cos, sin = abs(math.cos(rad)), abs(math.sin(rad))
    nw = max(1, int(w * cos + h * sin))
    nh = max(1, int(w * sin + h * cos))
    out = _neko.new_surface(nw, nh, surface.has_alpha())
    _neko.rotate(surface, w, h, out, nw, nh, float(angle))
    out.set_alpha(surface.get_alpha())
    return out


def rotozoom(surface, angle, scale_factor):
    w, h = surface.get_width(), surface.get_height()
    scaled = scale(surface, (max(1, int(w * scale_factor)),
                             max(1, int(h * scale_factor))))
    return rotate(scaled, angle)
