# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Renan Lucas Vieira Hilário
"""Neko pygame — a pygame-compatible API backed by the Neko Zig engine.

Put the directory that contains this package on ``PYTHONPATH`` (for example
``zig-out/python`` after ``zig build python``) and existing games keep their
``import pygame`` unchanged while the hot paths run in Zig.
"""

from . import _neko  # noqa: F401  (native core)

__version__ = "0.1.0-neko"

# ── Event type constants (match pygame) ──────────────────────────────────────
QUIT = 256
ACTIVEEVENT = 1
KEYDOWN = 768
KEYUP = 769
MOUSEMOTION = 1024
MOUSEBUTTONDOWN = 1025
MOUSEBUTTONUP = 1026
MOUSEWHEEL = 1027
VIDEORESIZE = 1028
VIDEOEXPOSE = 1029
USEREVENT = 32866

# Window flags (accepted, mostly ignored).
FULLSCREEN = 0x00000001
HWSURFACE = 0x00000002
RESIZABLE = 0x00000010
NOFRAME = 0x00000020
DOUBLEBUF = 0x40000000
SCALED = 0x00000200

# Surface flags.
SRCALPHA = 0x00010000
RLEACCEL = 0x00020000
SRCCOLORKEY = 0x00001000

# ── Key constants (SDL keycodes, like pygame 2) ──────────────────────────────
K_BACKSPACE = 8
K_TAB = 9
K_RETURN = 13
K_ESCAPE = 27
K_SPACE = 32
K_LEFT = 1073741904
K_RIGHT = 1073741903
K_UP = 1073741906
K_DOWN = 1073741905

for _i in range(ord("a"), ord("z") + 1):
    globals()["K_" + chr(_i)] = _i
for _i in range(ord("0"), ord("9") + 1):
    globals()["K_" + chr(_i)] = _i
del _i

# ── Color ────────────────────────────────────────────────────────────────────

_NAMED = {
    "black": (0, 0, 0), "white": (255, 255, 255), "red": (255, 0, 0),
    "green": (0, 255, 0), "blue": (0, 0, 255), "yellow": (255, 255, 0),
    "cyan": (0, 255, 255), "magenta": (255, 0, 255), "gray": (128, 128, 128),
    "grey": (128, 128, 128), "orange": (255, 165, 0), "purple": (160, 32, 240),
    "brown": (165, 42, 42), "pink": (255, 192, 203), "transparent": (0, 0, 0, 0),
}


class Color:
    """An RGBA colour, like ``pygame.Color``."""

    __slots__ = ("r", "g", "b", "a")

    def __init__(self, *args):
        if len(args) == 1:
            v = args[0]
            if isinstance(v, Color):
                self.r, self.g, self.b, self.a = v.r, v.g, v.b, v.a
                return
            if isinstance(v, str):
                name = _NAMED.get(v.lower())
                if name is None:
                    raise ValueError("unknown colour: %r" % (v,))
                args = name
            elif isinstance(v, (tuple, list)):
                args = tuple(v)
            else:
                n = int(v)
                if n > 0xFFFFFF:
                    self.a = (n >> 24) & 0xFF
                    self.r = (n >> 16) & 0xFF
                    self.g = (n >> 8) & 0xFF
                    self.b = n & 0xFF
                else:
                    self.r = (n >> 16) & 0xFF
                    self.g = (n >> 8) & 0xFF
                    self.b = n & 0xFF
                    self.a = 255
                return
        if len(args) == 3:
            self.r = int(args[0]) & 0xFF
            self.g = int(args[1]) & 0xFF
            self.b = int(args[2]) & 0xFF
            self.a = 255
        elif len(args) == 4:
            self.r = int(args[0]) & 0xFF
            self.g = int(args[1]) & 0xFF
            self.b = int(args[2]) & 0xFF
            self.a = int(args[3]) & 0xFF
        else:
            raise TypeError("Color() takes 1, 3 or 4 arguments")

    def __iter__(self):
        yield self.r
        yield self.g
        yield self.b
        yield self.a

    def __getitem__(self, i):
        return (self.r, self.g, self.b, self.a)[i]

    def __len__(self):
        return 4

    def __eq__(self, other):
        try:
            return (self.r, self.g, self.b, self.a) == (
                other.r, other.g, other.b, other.a)
        except AttributeError:
            return NotImplemented

    def __hash__(self):
        return hash((self.r, self.g, self.b, self.a))

    def __repr__(self):
        return "Color(%d, %d, %d, %d)" % (self.r, self.g, self.b, self.a)

    def normalize(self):
        return (self.r / 255.0, self.g / 255.0, self.b / 255.0, self.a / 255.0)

    def correct_gamma(self, _gamma):
        return self


_rgba_cache = {}


def _rgba_of(color):
    """Packs a colour into Neko's ``0xAARRGGBB`` integer, fast-pathing the
    tuple/int forms games use in hot loops (no temporary ``Color``), and
    memoising tuple colours (the common case)."""
    t = type(color)
    if t is tuple:
        v = _rgba_cache.get(color)
        if v is not None:
            return v
        n = len(color)
        if n == 4:
            v = ((int(color[3]) << 24) | (int(color[0]) << 16)
                 | (int(color[1]) << 8) | int(color[2]))
        elif n == 3:
            v = (0xFF000000 | (int(color[0]) << 16)
                 | (int(color[1]) << 8) | int(color[2]))
        else:
            c = Color(color)
            v = (c.a << 24) | (c.r << 16) | (c.g << 8) | c.b
        if len(_rgba_cache) > 4096:
            _rgba_cache.clear()
        _rgba_cache[color] = v
        return v
    if t is list:
        n = len(color)
        if n == 4:
            return ((int(color[3]) << 24) | (int(color[0]) << 16)
                    | (int(color[1]) << 8) | int(color[2]))
        if n == 3:
            return (0xFF000000 | (int(color[0]) << 16)
                    | (int(color[1]) << 8) | int(color[2]))
    elif t is int:
        return (0xFF000000 | color) if color <= 0xFFFFFF else color
    elif isinstance(color, Color):
        return (color.a << 24) | (color.r << 16) | (color.g << 8) | color.b
    c = color if isinstance(color, Color) else Color(color)
    return (c.a << 24) | (c.r << 16) | (c.g << 8) | c.b


# ── Rect ─────────────────────────────────────────────────────────────────────


class Rect:
    """A rectangle, like ``pygame.Rect``."""

    __slots__ = ("x", "y", "w", "h")

    def __init__(self, *args):
        n = len(args)
        if n == 1:
            a = args[0]
            if isinstance(a, Rect):
                self.x, self.y, self.w, self.h = a.x, a.y, a.w, a.h
                return
            args = tuple(a)
            n = len(args)
        if n == 2:
            p, s = args
            if type(p) is tuple or type(p) is list:
                self.x = int(p[0])
                self.y = int(p[1])
                self.w = int(s[0])
                self.h = int(s[1])
                return
            self.x = int(p)
            self.y = int(s)
            self.w = self.h = 0
            return
        if n == 4:
            self.x = int(args[0])
            self.y = int(args[1])
            self.w = int(args[2])
            self.h = int(args[3])
            return
        raise TypeError("Rect() takes a 4-tuple, a 2-tuple, or pos/size pairs")

    # -- edges ---------------------------------------------------------------
    @property
    def left(self):
        return self.x

    @left.setter
    def left(self, v):
        self.x = v

    @property
    def top(self):
        return self.y

    @top.setter
    def top(self, v):
        self.y = v

    @property
    def right(self):
        return self.x + self.w

    @right.setter
    def right(self, v):
        self.x = v - self.w

    @property
    def bottom(self):
        return self.y + self.h

    @bottom.setter
    def bottom(self, v):
        self.y = v - self.h

    @property
    def centerx(self):
        return self.x + self.w // 2

    @centerx.setter
    def centerx(self, v):
        self.x = v - self.w // 2

    @property
    def centery(self):
        return self.y + self.h // 2

    @centery.setter
    def centery(self, v):
        self.y = v - self.h // 2

    @property
    def center(self):
        return (self.centerx, self.centery)

    @center.setter
    def center(self, v):
        self.centerx, self.centery = v

    @property
    def topleft(self):
        return (self.x, self.y)

    @topleft.setter
    def topleft(self, v):
        self.x, self.y = v

    @property
    def topright(self):
        return (self.right, self.y)

    @topright.setter
    def topright(self, v):
        self.right, self.y = v

    @property
    def bottomleft(self):
        return (self.x, self.bottom)

    @bottomleft.setter
    def bottomleft(self, v):
        self.x, self.bottom = v

    @property
    def bottomright(self):
        return (self.right, self.bottom)

    @bottomright.setter
    def bottomright(self, v):
        self.right, self.bottom = v

    @property
    def midtop(self):
        return (self.centerx, self.y)

    @midtop.setter
    def midtop(self, v):
        self.centerx, self.y = v

    @property
    def midbottom(self):
        return (self.centerx, self.bottom)

    @midbottom.setter
    def midbottom(self, v):
        self.centerx, self.bottom = v

    @property
    def midleft(self):
        return (self.x, self.centery)

    @midleft.setter
    def midleft(self, v):
        self.x, self.centery = v

    @property
    def midright(self):
        return (self.right, self.centery)

    @midright.setter
    def midright(self, v):
        self.right, self.centery = v

    @property
    def size(self):
        return (self.w, self.h)

    @size.setter
    def size(self, v):
        self.w, self.h = v

    @property
    def width(self):
        return self.w

    @width.setter
    def width(self, v):
        self.w = v

    @property
    def height(self):
        return self.h

    @height.setter
    def height(self, v):
        self.h = v

    # -- protocol ------------------------------------------------------------
    def __getitem__(self, i):
        return (self.x, self.y, self.w, self.h)[i]

    def __len__(self):
        return 4

    def __iter__(self):
        yield self.x
        yield self.y
        yield self.w
        yield self.h

    def __eq__(self, other):
        try:
            return (self.x, self.y, self.w, self.h) == (
                other.x, other.y, other.w, other.h)
        except AttributeError:
            return NotImplemented

    def __hash__(self):
        return hash((self.x, self.y, self.w, self.h))

    def __repr__(self):
        return "<rect(%d, %d, %d, %d)>" % (self.x, self.y, self.w, self.h)

    def copy(self):
        return Rect(self.x, self.y, self.w, self.h)

    def move(self, dx, dy):
        return Rect(self.x + dx, self.y + dy, self.w, self.h)

    def move_ip(self, dx, dy):
        self.x += dx
        self.y += dy

    def inflate(self, dw, dh):
        return Rect(self.x - dw // 2, self.y - dh // 2, self.w + dw, self.h + dh)

    def clamp(self, other):
        r = Rect(other)
        return Rect(
            min(max(self.x, r.x), r.right - self.w),
            min(max(self.y, r.y), r.bottom - self.h), self.w, self.h)

    def colliderect(self, other):
        o = Rect(other)
        return (self.x < o.right and self.right > o.x and
                self.y < o.bottom and self.bottom > o.y)

    def collidepoint(self, *p):
        if len(p) == 1:
            px, py = p[0]
        else:
            px, py = p
        return self.x <= px < self.right and self.y <= py < self.bottom

    def contains(self, other):
        r = Rect(other)
        return (self.x <= r.x and self.y <= r.y and
                self.right >= r.right and self.bottom >= r.bottom)

    def collide(self, other):
        return self.colliderect(other)

    def inflate_ip(self, dw, dh):
        self.__init__(self.x - dw // 2, self.y - dh // 2,
                      self.w + dw, self.h + dh)

    def clamp_ip(self, other):
        r = Rect(other)
        clamped = self.clamp(r)
        self.x, self.y, self.w, self.h = clamped.x, clamped.y, clamped.w, clamped.h

    def clip(self, other):
        r = Rect(other)
        x = max(self.x, r.x)
        y = max(self.y, r.y)
        right = min(self.right, r.right)
        bottom = min(self.bottom, r.bottom)
        if right <= x or bottom <= y:
            return Rect(x, y, 0, 0)
        return Rect(x, y, right - x, bottom - y)

    def union(self, other):
        r = Rect(other)
        x = min(self.x, r.x)
        y = min(self.y, r.y)
        return Rect(x, y, max(self.right, r.right) - x, max(self.bottom, r.bottom) - y)

    def union_ip(self, other):
        u = self.union(other)
        self.x, self.y, self.w, self.h = u.x, u.y, u.w, u.h

    def normalize(self):
        if self.w < 0:
            self.x += self.w
            self.w = -self.w
        if self.h < 0:
            self.y += self.h
            self.h = -self.h


# ── Surface (native type defined in Zig) ─────────────────────────────────────

# The Surface itself (pixels, blit, fill, draw) is a native C type; only the
# `Rect`/`Color` helpers stay in Python. `get_rect` is a C method that uses
# `Rect` (registered below) so keyword anchoring works.
Surface = _neko.Surface
_neko._set_rect_class(Rect)


# ── Submodules (imported last: they use the names above) ─────────────────────

from . import display   # noqa: E402,F401
draw = _neko.draw  # native C submodule
from . import transform  # noqa: E402,F401
from . import event     # noqa: E402,F401
from . import key       # noqa: E402,F401
from . import time      # noqa: E402,F401
from . import mouse     # noqa: E402,F401
from . import image     # noqa: E402,F401
from . import font      # noqa: E402,F401
from . import mixer     # noqa: E402,F401
from . import version   # noqa: E402,F401


def init():
    """No-op; kept for pygame compatibility."""
    return (0, 0)


def quit():
    """Shuts the engine instance down."""
    _neko.shutdown()


def get_init():
    return True


def error(msg):
    raise RuntimeError(msg)
