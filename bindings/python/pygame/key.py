# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Renan Lucas Vieira Hilário
"""``pygame.key`` — live keyboard state."""

from . import _neko

_NAMES = {
    8: "backspace", 9: "tab", 13: "return", 27: "escape", 32: "space",
    1073741904: "left", 1073741903: "right", 1073741906: "up", 1073741905: "down",
}


class _KeyState:
    """Indexable like pygame's sequence: ``state[pygame.K_LEFT]``."""

    __slots__ = ("_codes",)

    def __init__(self, codes):
        self._codes = set(codes)

    def __getitem__(self, code):
        return code in self._codes

    def __len__(self):
        return (max(self._codes) + 1) if self._codes else 0

    def __iter__(self):
        n = len(self)
        for i in range(n):
            yield i in self._codes


def get_pressed():
    _neko.frame_begin()
    return _KeyState(_neko.key_state())


def get_mods():
    return 0


def set_mods(mods):
    pass


def get_focused():
    return True


def set_repeat(delay=0, interval=0):
    pass


def name(code):
    return _NAMES.get(code, "unknown")


def key_code(name):
    return 0
