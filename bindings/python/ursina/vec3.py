# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Renan Lucas Vieira Hilário
"""``ursina.Vec3`` — a small mutable 3D vector, Ursina-shaped."""

from __future__ import annotations

import math

_SWIZZLE = {"x": 0, "y": 1, "z": 2}


class Vec3:
    __slots__ = ("x", "y", "z")

    def __init__(self, x=0.0, y=None, z=None):
        if y is None and z is None:
            if isinstance(x, (tuple, list, Vec3)):
                x, y, z = (list(x) + [0, 0, 0])[:3]
            else:
                y = z = x
        self.x, self.y, self.z = float(x), float(y), float(z)

    # -- sequence protocol (Ursina allows position[1] = ...) ----------------
    def __iter__(self):
        return iter((self.x, self.y, self.z))

    def __len__(self):
        return 3

    def __getitem__(self, i):
        return (self.x, self.y, self.z)[i]

    def __setitem__(self, i, value):
        if i == 0:
            self.x = float(value)
        elif i == 1:
            self.y = float(value)
        else:
            self.z = float(value)

    # -- math ---------------------------------------------------------------
    @staticmethod
    def _coerce(o):
        return o if isinstance(o, Vec3) else Vec3(o)

    def __add__(self, o):
        o = Vec3._coerce(o)
        return Vec3(self.x + o.x, self.y + o.y, self.z + o.z)

    def __sub__(self, o):
        o = Vec3._coerce(o)
        return Vec3(self.x - o.x, self.y - o.y, self.z - o.z)

    def __mul__(self, o):
        if isinstance(o, (int, float)):
            return Vec3(self.x * o, self.y * o, self.z * o)
        o = Vec3._coerce(o)
        return Vec3(self.x * o.x, self.y * o.y, self.z * o.z)

    __rmul__ = __mul__

    def __neg__(self):
        return Vec3(-self.x, -self.y, -self.z)

    def dot(self, o):
        o = Vec3._coerce(o)
        return self.x * o.x + self.y * o.y + self.z * o.z

    def length(self):
        return math.sqrt(self.dot(self))

    def normalized(self):
        n = self.length()
        return Vec3(self.x / n, self.y / n, self.z / n) if n > 1e-8 else Vec3()

    def __repr__(self):
        return f"Vec3({self.x:g}, {self.y:g}, {self.z:g})"


Vec3.zero = Vec3(0, 0, 0)
Vec3.one = Vec3(1, 1, 1)
Vec3.up = Vec3(0, 1, 0)
Vec3.right = Vec3(1, 0, 0)
Vec3.forward = Vec3(0, 0, 1)
