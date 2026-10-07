# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Renan Lucas Vieira Hilário
"""``ursina.camera`` — the singleton 3D camera (Ursina world convention)."""

from __future__ import annotations

from .vec3 import Vec3


class Camera:
    __slots__ = ("position", "rotation", "fov", "near", "far")

    def __init__(self):
        self.position = Vec3(0, 0, -20)  # in front of the origin, looking +Z
        self.rotation = Vec3(0, 0, 0)
        self.fov = 40
        self.near = 0.1
        self.far = 1000

    @property
    def x(self):
        return self.position.x

    @x.setter
    def x(self, value):
        self.position.x = value

    @property
    def y(self):
        return self.position.y

    @y.setter
    def y(self, value):
        self.position.y = value

    @property
    def z(self):
        return self.position.z

    @z.setter
    def z(self, value):
        self.position.z = value

    @property
    def rotation_x(self):
        return self.rotation.x

    @rotation_x.setter
    def rotation_x(self, value):
        self.rotation.x = value

    @property
    def rotation_y(self):
        return self.rotation.y

    @rotation_y.setter
    def rotation_y(self, value):
        self.rotation.y = value

    @property
    def rotation_z(self):
        return self.rotation.z

    @rotation_z.setter
    def rotation_z(self, value):
        self.rotation.z = value


camera = Camera()
