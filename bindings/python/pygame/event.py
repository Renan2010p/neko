# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Renan Lucas Vieira Hilário
"""``pygame.event`` — the event queue, backed by Neko's input pump."""

from . import (_neko, QUIT, KEYDOWN, KEYUP, MOUSEMOTION, MOUSEBUTTONDOWN,
               MOUSEBUTTONUP, MOUSEWHEEL)

_pending = []
_mouse_pos = (0, 0)
_mouse_buttons = set()


class Event(dict):
    """A pygame event: dict access *and* attribute access."""

    def __getattr__(self, name):
        try:
            return self[name]
        except KeyError as exc:
            raise AttributeError(name) from exc

    def __setattr__(self, name, value):
        self[name] = value


def _convert(tup):
    global _mouse_pos
    kind, a, b, c, name = tup
    if kind == 0:
        return Event(type=QUIT)
    if kind == 1:
        uni = chr(a) if 32 <= a < 127 else ""
        return Event(type=KEYDOWN, key=a, scancode=0, unicode=uni, mod=0, name=name)
    if kind == 2:
        return Event(type=KEYUP, key=a, scancode=0, unicode="", mod=0, name=name)
    if kind == 3:
        _mouse_pos = (a, b)
        return Event(type=MOUSEMOTION, pos=(a, b), rel=(0, 0), buttons=(False, False, False))
    if kind == 4:
        _mouse_pos = (b, c)
        _mouse_buttons.add(a)
        return Event(type=MOUSEBUTTONDOWN, pos=(b, c), button=a, touch=False)
    if kind == 5:
        _mouse_pos = (b, c)
        _mouse_buttons.discard(a)
        return Event(type=MOUSEBUTTONUP, pos=(b, c), button=a, touch=False)
    if kind == 6:
        return Event(type=MOUSEWHEEL, flipped=False, x=0, y=0, precise_x=0.0, precise_y=0.0)
    return Event(type=0)


def _collect():
    _neko.frame_begin()
    for tup in _neko.get_events():
        _pending.append(_convert(tup))


def get(eventtype=None, pump=True, exclude=None):
    _collect()
    out = _pending[:]
    _pending.clear()
    if eventtype is not None:
        if isinstance(eventtype, int):
            eventtype = (eventtype,)
        out = [e for e in out if e["type"] in eventtype]
    if exclude is not None:
        if isinstance(exclude, int):
            exclude = (exclude,)
        out = [e for e in out if e["type"] not in exclude]
    return out


def pump():
    return get()


def poll():
    _collect()
    if _pending:
        return _pending.pop(0)
    return Event(type=0)


def peek(eventtype=None, pump=True):
    _collect()
    if not _pending:
        return False
    return True


def clear(eventtype=None, pump=True):
    _pending.clear()


def post(event):
    _pending.append(event)
    return True


def set_allowed(eventtypes=None):
    pass


def set_blocked(eventtypes=None):
    pass


def set_grab(grab):
    pass


def get_grab():
    return False


def wait():
    while True:
        ev = poll()
        if ev is not None and ev.get("type"):
            return ev
