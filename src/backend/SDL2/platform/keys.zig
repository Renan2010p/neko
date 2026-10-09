// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! Shared SDL key/button mapping for every SDL2-based backend. Without this,
//! `neko.input.key`/`keyDown` (and therefore `pygame.key.get_pressed`) would
//! only ever see `engine.Key.unknown`.

const c: type = @import("c");
const engine: type = @import("neko");

/// Maps an SDL key symbol onto our convenience key names.
pub fn mapKey(sym: c.SDL_Keycode) engine.Key {
    return switch (sym) {
        c.SDLK_UP => engine.Key.up,
        c.SDLK_DOWN => engine.Key.down,
        c.SDLK_LEFT => engine.Key.left,
        c.SDLK_RIGHT => engine.Key.right,
        c.SDLK_RETURN, c.SDLK_RETURN2 => engine.Key.enter,
        c.SDLK_ESCAPE => engine.Key.escape,
        c.SDLK_SPACE => engine.Key.space,
        c.SDLK_TAB => engine.Key.tab,
        c.SDLK_BACKSPACE => engine.Key.backspace,
        c.SDLK_a => engine.Key.a,
        c.SDLK_b => engine.Key.b,
        c.SDLK_c => engine.Key.c,
        c.SDLK_d => engine.Key.d,
        c.SDLK_e => engine.Key.e,
        c.SDLK_f => engine.Key.f,
        c.SDLK_g => engine.Key.g,
        c.SDLK_h => engine.Key.h,
        c.SDLK_i => engine.Key.i,
        c.SDLK_j => engine.Key.j,
        c.SDLK_k => engine.Key.k,
        c.SDLK_l => engine.Key.l,
        c.SDLK_m => engine.Key.m,
        c.SDLK_n => engine.Key.n,
        c.SDLK_o => engine.Key.o,
        c.SDLK_p => engine.Key.p,
        c.SDLK_q => engine.Key.q,
        c.SDLK_r => engine.Key.r,
        c.SDLK_s => engine.Key.s,
        c.SDLK_t => engine.Key.t,
        c.SDLK_u => engine.Key.u,
        c.SDLK_v => engine.Key.v,
        c.SDLK_w => engine.Key.w,
        c.SDLK_x => engine.Key.x,
        c.SDLK_y => engine.Key.y,
        c.SDLK_z => engine.Key.z,
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
