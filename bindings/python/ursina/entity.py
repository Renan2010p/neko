# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Renan Lucas Vieira Hilário
"""``ursina.Entity`` — a 3D object with a transform, a model and a color.

M1 keeps the transform local (no parent composition yet): `position`,
`rotation` (degrees) and `scale` are passed straight to the renderer, matching
Ursina's per-entity transform for unparented objects.
"""

from __future__ import annotations

from . import _neko  # native 3D submission
from . import color as color_mod
from . import scene
from .vec3 import Vec3

_MESH_KINDS = {"cube": 0, "quad": 1, "plane": 2}


def _as_vec3(value, default=(0, 0, 0)):
    if value is None:
        return Vec3(*default)
    if isinstance(value, Vec3):
        return Vec3(value.x, value.y, value.z)
    if isinstance(value, (int, float)):
        return Vec3(value, value, value)
    return Vec3(*value)


class Entity:
    def __init__(
        self,
        model=None,
        color=None,
        position=(0, 0, 0),
        rotation=(0, 0, 0),
        scale=(1, 1, 1),
        parent=None,
        enabled=True,
        name="",
        texture=None,
        x=None,
        y=None,
        z=None,
        rotation_x=None,
        rotation_y=None,
        rotation_z=None,
        scale_x=None,
        scale_y=None,
        scale_z=None,
        add_to_scene_entities=True,
        **kwargs,
    ):
        self.name = name
        self.position = _as_vec3(position)
        self.rotation = _as_vec3(rotation)
        self.scale = _as_vec3(scale, (1, 1, 1))
        if x is not None:
            self.position.x = x
        if y is not None:
            self.position.y = y
        if z is not None:
            self.position.z = z
        if rotation_x is not None:
            self.rotation.x = rotation_x
        if rotation_y is not None:
            self.rotation.y = rotation_y
        if rotation_z is not None:
            self.rotation.z = rotation_z
        if scale_x is not None:
            self.scale.x = scale_x
        if scale_y is not None:
            self.scale.y = scale_y
        if scale_z is not None:
            self.scale.z = scale_z

        self.model = model
        self._kind = _MESH_KINDS.get(model) if isinstance(model, str) else None
        self.color = color if color is not None else color_mod.white
        self.texture = texture
        self.enabled = enabled
        self.parent = parent
        self.children = []
        if parent is not None:
            parent.children.append(self)

        self.visible = True
        for key, value in kwargs.items():
            setattr(self, key, value)

        if add_to_scene_entities:
            scene.entities.append(self)

    # -- transform shorthands ----------------------------------------------
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

    @property
    def scale_x(self):
        return self.scale.x

    @scale_x.setter
    def scale_x(self, value):
        self.scale.x = value

    @property
    def scale_y(self):
        return self.scale.y

    @scale_y.setter
    def scale_y(self, value):
        self.scale.y = value

    @property
    def scale_z(self):
        return self.scale.z

    @scale_z.setter
    def scale_z(self, value):
        self.scale.z = value

    # -- lifecycle ----------------------------------------------------------
    def enable(self):
        self.enabled = True

    def disable(self):
        self.enabled = False

    def __bool__(self):
        return self.enabled

    # -- rendering ----------------------------------------------------------
    def _draw(self) -> None:
        if self._kind is not None and self.visible:
            tint = self.color.packed() if isinstance(self.color, color_mod.Color) else 0xFFFFFFFF
            p, r, s = self.position, self.rotation, self.scale
            _neko.render3d_draw(
                self._kind,
                p.x, p.y, p.z,
                r.x, r.y, r.z,
                s.x, s.y, s.z,
                tint,
            )
        for child in self.children:
            if child.enabled:
                child._draw()
