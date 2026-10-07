# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Renan Lucas Vieira Hilário
"""``pygame.display`` — the window and the presented surface."""

from . import _neko, Surface

_screen = None
_caption = "Neko PYGAME"


def set_mode(size=(0, 0), flags=0, depth=0, display=0, vsync=0):
    """Creates the window and returns the drawing surface."""
    global _screen
    try:
        w, h = size
    except TypeError:
        w = h = 0
    if w <= 0 or h <= 0:
        w, h = 640, 360
    w, h = int(w), int(h)
    _neko.init(w, h, _caption)
    _screen = Surface((w, h), 0)
    return _screen


def get_surface():
    return _screen


def flip():
    """Presents the surface (``_neko.present`` + end of frame)."""
    if _screen is not None:
        _neko.present(_screen, _screen.get_width(), _screen.get_height())
    _neko.frame_end()


def update(*args):
    flip()


def set_caption(title):
    global _caption
    _caption = f"{title} (Neko)"
    _neko.set_caption(_caption)


def get_caption():
    return _caption


def get_window_size():
    if _screen is None:
        return (0, 0)
    return (_screen.get_width(), _screen.get_height())


def set_icon(icon):
    pass


def iconify():
    pass


def toggle_fullscreen():
    return 0


def quit():
    _neko.shutdown()
