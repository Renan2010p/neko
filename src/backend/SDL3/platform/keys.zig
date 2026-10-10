// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! Shared SDL3 key/button mapping for the SDL3 backend.

const c: type = @import("c");
const engine: type = @import("neko");

/// Maps an SDL key symbol onto our convenience key names.
pub fn mapKey(sym: c.SDL_Keycode) engine.Key {
    return switch (sym) {
        c.SDLK_UP => engine.Key.up,
        c.SDLK_DOWN => engine.Key.down,
        c.SDLK_LEFT => engine.Key.left,
        c.SDLK_RIGHT => engine.Key.right,
        c.SDLK_RETURN => engine.Key.enter,
        c.SDLK_ESCAPE => engine.Key.escape,
        c.SDLK_SPACE => engine.Key.space,
        c.SDLK_TAB => engine.Key.tab,
        c.SDLK_BACKSPACE => engine.Key.backspace,
        c.SDLK_A => engine.Key.a,
        c.SDLK_B => engine.Key.b,
        c.SDLK_C => engine.Key.c,
        c.SDLK_D => engine.Key.d,
        c.SDLK_E => engine.Key.e,
        c.SDLK_F => engine.Key.f,
        c.SDLK_G => engine.Key.g,
        c.SDLK_H => engine.Key.h,
        c.SDLK_I => engine.Key.i,
        c.SDLK_J => engine.Key.j,
        c.SDLK_K => engine.Key.k,
        c.SDLK_L => engine.Key.l,
        c.SDLK_M => engine.Key.m,
        c.SDLK_N => engine.Key.n,
        c.SDLK_O => engine.Key.o,
        c.SDLK_P => engine.Key.p,
        c.SDLK_Q => engine.Key.q,
        c.SDLK_R => engine.Key.r,
        c.SDLK_S => engine.Key.s,
        c.SDLK_T => engine.Key.t,
        c.SDLK_U => engine.Key.u,
        c.SDLK_V => engine.Key.v,
        c.SDLK_W => engine.Key.w,
        c.SDLK_X => engine.Key.x,
        c.SDLK_Y => engine.Key.y,
        c.SDLK_Z => engine.Key.z,
        c.SDLK_0 => engine.Key.number_0,
        c.SDLK_1 => engine.Key.number_1,
        c.SDLK_2 => engine.Key.number_2,
        c.SDLK_3 => engine.Key.number_3,
        c.SDLK_4 => engine.Key.number_4,
        c.SDLK_5 => engine.Key.number_5,
        c.SDLK_6 => engine.Key.number_6,
        c.SDLK_7 => engine.Key.number_7,
        c.SDLK_8 => engine.Key.number_8,
        c.SDLK_9 => engine.Key.number_9,
        else => engine.Key.unknown,
    };
}

/// Maps an SDL mouse button onto our names.
pub fn mapButton(button: c.Uint8) engine.MouseButton {
    return switch (button) {
        c.SDL_BUTTON_LEFT => engine.MouseButton.left,
        c.SDL_BUTTON_MIDDLE => engine.MouseButton.middle,
        c.SDL_BUTTON_RIGHT => engine.MouseButton.right,
        c.SDL_BUTTON_X1 => engine.MouseButton.x1,
        c.SDL_BUTTON_X2 => engine.MouseButton.x2,
        else => engine.MouseButton.unknown,
    };
}
