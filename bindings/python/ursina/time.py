# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Renan Lucas Vieira Hilário
"""``ursina.time`` — ``time.dt`` (seconds since the previous frame)."""

from __future__ import annotations

dt: float = 0.0
"""Seconds elapsed since the previous frame (``1/60`` on a 60 FPS machine)."""

time_scale: float = 1.0
"""Multiplier applied to ``dt`` (Ursina compatibility)."""


def _set(dt_unscaled: float) -> None:
    global dt
    dt = dt_unscaled * time_scale
