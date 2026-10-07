#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Renan Lucas Vieira Hilário
"""Neko Ursina demo — a spinning, lit cube over a ground plane.

Build a 3D backend and run it:

    zig build python -Dbackend=sdl2-opengl
    PYTHONPATH=zig-out/python python zig-out/python/ursina_demo.py
"""

from __future__ import annotations

import math

from ursina import *  # noqa: F403

app = Ursina(title="Neko Ursina", size=(800, 600))  # noqa: F405

# A camera slightly above and back, looking at the cube.
camera.position = Vec3(0, 2.5, -9)  # noqa: F405
camera.rotation_x = 8

ground = Entity(  # noqa: F405
    model="plane",
    color=color.rgb(48, 58, 78),  # noqa: F405
    scale=(10, 1, 10),
    y=-1.5,
)
box = Entity(model="cube", color=color.orange, y=0)  # noqa: F405
pillar = Entity(  # noqa: F405
    model="cube",
    color=color.cyan,
    position=(3, 0, 2),
    scale=(0.6, 3, 0.6),
)

_t = 0.0
_orange = True


def update() -> None:
    global _t
    _t += time.dt  # noqa: F405
    box.rotation_y += 45 * time.dt  # noqa: F405
    box.rotation_x = 18
    box.y = math.sin(_t * 1.5) * 0.35
    if held_keys["a"]:  # noqa: F405
        box.x -= 3 * time.dt  # noqa: F405
    if held_keys["d"]:  # noqa: F405
        box.x += 3 * time.dt  # noqa: F405


def input(key: str) -> None:  # noqa: A001
    global _orange
    if key == "space":
        box.color = color.orange if not _orange else color.cyan  # noqa: F405
        _orange = not _orange
    elif key == "escape":
        app.quit()


if __name__ == "__main__":
    app.run()
