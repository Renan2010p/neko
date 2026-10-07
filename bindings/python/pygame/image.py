# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Renan Lucas Vieira Hilário
"""``pygame.image`` — pixel (un)packing; ``load``/``save`` use Pillow if present."""

from . import Surface, _neko


def tobytes(surface, format="RGBA"):
    """Packs the surface into ``RGBA`` bytes (conversion done in Zig)."""
    return _neko.to_rgba(surface)


# Old name, still used by many games.
tostring = tobytes


def frombytes(data, size, format="RGBA"):
    w, h = size
    w, h = int(w), int(h)
    s = Surface((w, h), 0x00010000)  # SRCALPHA
    if format == "RGBA":
        _neko.unpack_rgba(s, data, w * h)
    return s


frombuffer = frombytes


def load(path, namehint=""):
    try:
        from PIL import Image
    except ImportError as exc:  # pragma: no cover
        raise NotImplementedError(
            "pygame.image.load needs Pillow installed in the Neko shim") from exc
    im = Image.open(path).convert("RGBA")
    return frombytes(im.tobytes(), im.size, "RGBA")


def save(surface, path, namehint=""):
    try:
        from PIL import Image
    except ImportError as exc:  # pragma: no cover
        raise NotImplementedError(
            "pygame.image.save needs Pillow installed in the Neko shim") from exc
    data = tobytes(surface, "RGBA")
    im = Image.frombytes("RGBA", (surface.get_width(), surface.get_height()), data)
    im.save(path)
