# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Renan Lucas Vieira Hilário
"""``ursina.scene`` — the list of entities the app renders and updates."""

from __future__ import annotations

entities: list = []


def clear() -> None:
    entities.clear()
