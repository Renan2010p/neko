# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Renan Lucas Vieira Hilário
"""``pygame.mouse`` — live mouse state (tracked from Neko events)."""

from . import event


def get_pos():
    return event._mouse_pos


def get_rel():
    return (0, 0)


def get_pressed(num_buttons=3):
    down = event._mouse_buttons
    if num_buttons == 3:
        return (1 in down, 2 in down, 3 in down)
    return (1 in down, 2 in down, 3 in down, 4 in down, 5 in down)


def get_focused():
    return True


def set_visible(value):
    pass


def get_visible():
    return True


def set_pos(pos):
    pass


def set_cursor(*args):
    pass
