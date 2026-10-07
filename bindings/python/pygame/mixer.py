# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Renan Lucas Vieira Hilário
"""``pygame.mixer`` — minimal stub (audio is not wired to Neko yet)."""


def pre_init(frequency=44100, size=-16, channels=2, buffer=512, **kwargs):
    pass


def init(frequency=44100, size=-16, channels=2, buffer=512, **kwargs):
    pass


def quit():
    pass


def get_init():
    return None


def stop():
    pass


def pause():
    pass


def unpause():
    pass


def set_num_channels(count):
    pass


def get_num_channels():
    return 8


def find_channel(force=False):
    return None


class Sound:
    def __init__(self, file=None, buffer=None):
        self._file = file
        self._buffer = buffer

    def play(self, loops=0, maxtime=0, fade_ms=0):
        return None

    def stop(self):
        pass

    def set_volume(self, value):
        pass

    def get_volume(self):
        return 1.0

    def get_length(self):
        return 0.0


class Channel:
    def play(self, sound, loops=0, maxtime=0, fade_ms=0):
        pass

    def stop(self):
        pass

    def set_volume(self, value):
        pass


def Sound_from_file(file):
    return Sound(file)
