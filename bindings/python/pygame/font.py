# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Renan Lucas Vieira Hilário
"""``pygame.font`` — text rendering.

Rasterisation uses Pillow's ``ImageFont`` (the same TTF/FreeType stack Neko
uses) and the result is packed into a Neko :class:`Surface`.
"""

from . import Color, Surface
from . import image as _image

import os

# A real system TTF gives accents, `•` and box-drawing glyphs that Pillow's
# bundled bitmap font lacks. pygame's default font is a sans face too.
_REGULAR = [
    "/usr/share/fonts/TTF/FreeSans.ttf",
    "/usr/share/fonts/truetype/freefont/FreeSans.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/TTF/liberation/LiberationSans-Regular.ttf",
]
_BOLD = [
    "/usr/share/fonts/TTF/FreeSansBold.ttf",
    "/usr/share/fonts/truetype/freefont/FreeSansBold.ttf",
    "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/TTF/liberation/LiberationSans-Bold.ttf",
]


def _first_existing(paths):
    for p in paths:
        if os.path.exists(p):
            return p
    return None


def _find_default():
    """pygame's bundled default font (`freesansbold.ttf`), which has very
    different metrics from the system FreeSans/DejaVu. Search the import path
    (the real pygame still ships it) and fall back to a system bold sans."""
    import sys
    for base in sys.path:
        if base:
            p = os.path.join(base, "pygame", "freesansbold.ttf")
            if os.path.exists(p):
                return p
    return _first_existing(_BOLD)


def _pil():
    try:
        from PIL import Image, ImageDraw, ImageFont
    except ImportError as exc:  # pragma: no cover
        raise NotImplementedError(
            "pygame.font needs Pillow installed in the Neko shim") from exc
    return Image, ImageDraw, ImageFont


class Font:
    """A font at a fixed pixel size."""

    def __init__(self, name=None, size=20, bold=False, italic=False):
        _Image, _ImageDraw, _ImageFont = _pil()
        self.size_ = int(size)
        self._name = name
        self._bold = bold
        self._italic = italic
        self._font = self._load()

    def _load(self):
        _Image, _ImageDraw, ImageFont = _pil()
        path = self._name
        if path is None:
            # pygame's default font (freesansbold) renders at ~2/3 of the
            # requested size; match its metrics.
            path = _find_default()
            pil_size = (self.size_ * 2 + 1) // 3
        else:
            pil_size = self.size_
        if path:
            try:
                return ImageFont.truetype(path, max(1, pil_size))
            except Exception:
                pass
        return ImageFont.load_default(size=max(1, pil_size))

    def render(self, text, antialias=True, color=Color(255, 255, 255), background=None):
        Image, ImageDraw, _ImageFont = _pil()
        c = color if isinstance(color, Color) else Color(color)
        left, top, right, bottom = self._font.getbbox(text)
        ascent, descent = self._font.getmetrics()
        w = max(1, right - left)
        h = max(1, ascent + descent)
        if background is None:
            bg = (0, 0, 0, 0)
        else:
            bc = background if isinstance(background, Color) else Color(background)
            bg = (bc.r, bc.g, bc.b, bc.a)
        img = Image.new("RGBA", (w, h), bg)
        draw = ImageDraw.Draw(img)
        draw.text((-left, 0), text, font=self._font, fill=(c.r, c.g, c.b, c.a))
        return _image.frombytes(img.tobytes(), img.size, "RGBA")

    def size(self, text):
        left, top, right, bottom = self._font.getbbox(text)
        return (max(0, right - left), self.get_linesize())

    def get_linesize(self):
        a, d = self._font.getmetrics()
        return a + d

    def get_height(self):
        return self.get_linesize()

    def get_ascent(self):
        return self._font.getmetrics()[0]

    def get_descent(self):
        return self._font.getmetrics()[1]

    def set_bold(self, value):
        if bool(value) != self._bold:
            self._bold = bool(value)
            self._font = self._load()

    def set_italic(self, value):
        if bool(value) != self._italic:
            self._italic = bool(value)
            self._font = self._load()

    def set_underline(self, value):
        pass


def init():
    pass


def quit():
    pass


def get_init():
    return True


def get_default_font():
    return None


def SysFont(name, size, bold=False, italic=False):
    return Font(None, size, bold, italic)


def match_font(name, bold=False, italic=False):
    return None
