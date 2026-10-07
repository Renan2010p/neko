# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Renan Lucas Vieira Hilário
"""``ursina.application`` — the ``Ursina`` app and its render loop."""

from __future__ import annotations

import __main__
import sys

from . import _neko
from . import input_handler
from . import scene
from . import time as _time
from .camera import camera

_QUIT = 0
_KEYDOWN = 1
_KEYUP = 2


class Ursina:
    """Opens the window and drives update → draw → present each frame."""

    def __init__(self, title="ursina", size=(800, 600), vsync=True, **kwargs):
        w, h = size if size else (800, 600)
        _neko.init(int(w), int(h), str(title))
        self.title = title
        self.running = True
        self._last = _neko.get_ticks()
        self.entities = scene.entities

        if not _neko.render3d_supported():
            print(
                "ursina: this Neko build has no 3D backend — rebuild with "
                "`zig build python -Dbackend=sdl2-opengl`",
                file=sys.stderr,
            )

    def quit(self) -> None:
        self.running = False

    def step(self) -> None:
        now = _neko.get_ticks()
        _time._set((now - self._last) / 1000.0)
        self._last = now

        _neko.frame_begin()
        events = _neko.get_events()
        input_handler.update(events)

        for kind, _code, _x, _y, name in events:
            if kind == _QUIT:
                self.running = False
            elif kind in (_KEYDOWN, _KEYUP):
                self._dispatch_input(input_handler.key_name(name))

        update = getattr(__main__, "update", None)
        if update is not None:
            update()
        for entity in list(self.entities):
            if entity.enabled and hasattr(entity, "update"):
                entity.update()

        _neko.render3d_begin(
            camera.position.x, camera.position.y, camera.position.z,
            camera.rotation.x, camera.rotation.y, camera.rotation.z,
            camera.fov, camera.near, camera.far,
        )
        for entity in list(self.entities):
            if entity.enabled:
                entity._draw()
        _neko.render3d_end()
        _neko.render3d_present()
        _neko.frame_end()

    def _dispatch_input(self, key: str) -> None:
        handler = getattr(__main__, "input", None)
        if handler is not None:
            handler(key)
        for entity in list(self.entities):
            if entity.enabled and hasattr(entity, "input"):
                if entity.input(key):
                    break

    def run(self) -> None:
        try:
            while self.running:
                self.step()
        finally:
            try:
                _neko.shutdown()
            except Exception:
                pass
