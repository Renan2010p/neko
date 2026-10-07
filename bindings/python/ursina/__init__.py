# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Renan Lucas Vieira Hilário
"""Neko Ursina — an Ursina-shaped 3D layer backed by the Neko engine.

Put the directory that contains this package on ``PYTHONPATH`` (for example
``zig-out/python`` after ``zig build python``) and run simple Ursina games on
Neko. Build with a 3D backend:

    zig build python -Dbackend=sdl2-opengl

    from ursina import *

    app = Ursina()
    box = Entity(model='cube', color=color.orange)

    def update():
        box.rotation_y += 40 * time.dt

    app.run()
"""

from __future__ import annotations

from .vec3 import Vec3
from .color import Color
from . import color
from .camera import camera
from .entity import Entity
from .application import Ursina
from .input_handler import held_keys
from . import time
from . import scene

__version__ = "0.1.0-neko"

__all__ = [
    "Vec3",
    "Color",
    "color",
    "camera",
    "Entity",
    "Ursina",
    "held_keys",
    "time",
    "scene",
]
