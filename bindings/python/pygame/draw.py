# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Renan Lucas Vieira Hilário
"""``pygame.draw`` — primitives rendered into a :class:`Surface`.

These wrappers stay deliberately thin: they do **not** build `Rect`/`Color`
objects on the way in, they parse the arguments inline and hand raw ints to the
Zig core. Object construction is the measured hot spot when drawing hundreds of
primitives per frame.
"""

from . import _neko, _rgba_of, Rect


def _xywh(r):
    if type(r) is Rect:
        return r.x, r.y, r.w, r.h
    return int(r[0]), int(r[1]), int(r[2]), int(r[3])


def rect(surface, color, rect, width=0, border_radius=0):
    if type(rect) is Rect:
        x, y, w, h = rect.x, rect.y, rect.w, rect.h
    else:
        x, y, w, h = rect
    _neko.draw_rect(surface._buf, surface.w, surface.h,
                    x, y, w, h, _rgba_of(color), int(width), int(border_radius))


def line(surface, color, start, end, width=1):
    _neko.draw_line(surface._buf, surface.w, surface.h,
                    start[0], start[1], end[0], end[1],
                    _rgba_of(color), int(width))


def lines(surface, color, closed, points, width=1):
    pts = list(points)
    n = len(pts)
    if n < 2:
        return
    rgba = _rgba_of(color)
    buf = surface._buf
    sw = surface.w
    sh = surface.h
    w = int(width)
    draw_line = _neko.draw_line
    for i in range(n - 1):
        a = pts[i]
        b = pts[i + 1]
        draw_line(buf, sw, sh, a[0], a[1], b[0], b[1], rgba, w)
    if closed:
        a = pts[-1]
        b = pts[0]
        draw_line(buf, sw, sh, a[0], a[1], b[0], b[1], rgba, w)


def aaline(surface, color, start, end, blend=1):
    return line(surface, color, start, end)


def aalines(surface, color, closed, points, blend=1):
    return lines(surface, color, closed, points)


def circle(surface, color, center, radius, width=0):
    _neko.draw_circle(surface._buf, surface.w, surface.h,
                      center[0], center[1], radius,
                      _rgba_of(color), int(width))


def ellipse(surface, color, rect, width=0):
    if type(rect) is Rect:
        x, y, w, h = rect.x, rect.y, rect.w, rect.h
    else:
        x, y, w, h = rect
    _neko.draw_ellipse(surface._buf, surface.w, surface.h,
                       x, y, w, h, _rgba_of(color), int(width))


def polygon(surface, color, points, width=0):
    if type(points) is not list and type(points) is not tuple:
        points = list(points)
    if not points:
        return
    _neko.draw_polygon_seq(surface._buf, surface.w, surface.h, points,
                           _rgba_of(color))


def arc(surface, color, rect, start_angle, stop_angle, width=1):
    if type(rect) is Rect:
        x, y, w, h = rect.x, rect.y, rect.w, rect.h
    else:
        x, y, w, h = rect
    _neko.draw_arc(surface._buf, surface.w, surface.h, x, y, w, h,
                   _rgba_of(color), start_angle, stop_angle, int(width))
