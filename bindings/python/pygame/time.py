# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Renan Lucas Vieira Hilário
"""``pygame.time`` — ticks, delays and a frame clock."""

from . import _neko


def get_ticks():
    return _neko.get_ticks()


def delay(milliseconds):
    _neko.delay(int(milliseconds))
    return int(milliseconds)


def wait(milliseconds):
    return delay(milliseconds)


def set_timer(event, millis=0, loops=0):
    pass


class Clock:
    """A frame clock compatible with ``pygame.time.Clock``."""

    def __init__(self):
        self._last = _neko.get_ticks()
        self._time = 0
        self._raw = 0
        self._fps = 0.0
        self._fps_acc = 0.0
        self._fps_count = 0

    def tick(self, framerate=0):
        now = _neko.get_ticks()
        self._raw = now - self._last
        self._time = self._raw
        self._last = now
        if framerate:
            target = 1000.0 / framerate
            wait_ms = target - self._raw
            if wait_ms > 0:
                _neko.delay(int(wait_ms))
            self._time = target
        self._fps_acc += self._time
        self._fps_count += 1
        if self._fps_acc >= 1000.0 and self._fps_count:
            self._fps = self._fps_count * 1000.0 / self._fps_acc
            self._fps_acc = 0.0
            self._fps_count = 0
        return int(self._time)

    def tick_busy_loop(self, framerate=0):
        return self.tick(framerate)

    def get_time(self):
        return int(self._time)

    def get_rawtime(self):
        return int(self._raw)

    def get_fps(self):
        return self._fps
