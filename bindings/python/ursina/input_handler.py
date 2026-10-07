# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Renan Lucas Vieira Hilário
"""``ursina.input_handler`` — ``held_keys`` and SDL→Ursina key names."""

from __future__ import annotations

_ALIASES = {
    "space": "space",
    "return": "enter",
    "enter": "enter",
    "escape": "escape",
    "tab": "tab",
    "backspace": "backspace",
    "left": "left arrow",
    "right": "right arrow",
    "up": "up arrow",
    "down": "down arrow",
    "left shift": "left shift",
    "right shift": "right shift",
}


class _HeldKeys(dict):
    def __missing__(self, key):
        return 0


held_keys: "_HeldKeys" = _HeldKeys()


def key_name(sdl_name: str) -> str:
    if len(sdl_name) == 1:
        return sdl_name.lower()
    return _ALIASES.get(sdl_name.lower(), sdl_name.lower())


def update(events) -> None:
    """Applies this frame's `(kind, code, x, y, name)` tuples to `held_keys`."""
    for kind, _code, _x, _y, name in events:
        if kind not in (1, 2):
            continue
        key = key_name(name)
        held_keys[key] = 1 if kind == 1 else 0
