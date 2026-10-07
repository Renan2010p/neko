// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! Reference program for the PS2 bindings (`ps2sdk.zig` + `gskit.zig`).
//!
//! It is not wired into `build.zig` (the engine builds the `.ps2` backend,
//! which uses these same modules); it exists to document the usage and to
//! serve as a compile/link/run smoke test:
//!
//!     zig build-obj hello.zig -target mips64el-freestanding-gnuabin32 \
//!         -O ReleaseFast -ofmt=c -femit-bin=hello.c
//!     mips64r5900el-ps2-elf-gcc -D_EE -G0 -O2 -I<ziglib> -c hello.c -o hello.o
//!     mips64r5900el-ps2-elf-gcc -T<linkfile> -o hello.elf hello.o \
//!         -L<sdk>/ee/lib -L<gsKit>/lib -lgskit -ldmakit -lc -lkernel -lpad …
//!
//! Draws a bouncing square with gsKit, prints text with the ROM font, and
//! exits on START. Watch the pad's buttons with `pad.held`.

const ps2sdk: type = @import("ps2sdk.zig");
const gskit: type = @import("gskit.zig");

// Pad port scratch area: 256 bytes, 64-byte aligned (PS2SDK requirement).
var pad_area: [256]u8 align(64) = undefined;
var pad_status: ps2sdk.pad.ButtonStatus = undefined;
var previous_buttons: u16 = 0;

export fn main() void {
    _ = ps2sdk.sif.sceSifInitRpc(0);

    // The pad needs its IOP modules loaded before padInit().
    _ = ps2sdk.loader.SifLoadModule(ps2sdk.loader.SIO2MAN, 0, "");
    _ = ps2sdk.loader.SifLoadModule(ps2sdk.loader.PADMAN, 0, "");
    _ = ps2sdk.pad.padInit(0);
    _ = ps2sdk.pad.padPortOpen(0, 0, &pad_area);

    // ── gsKit up ────────────────────────────────────────────────────────────
    const gs: *gskit.GSGLOBAL = gskit.core.initGlobal();
    const font: *gskit.GSFONTM = gskit.fontm.init();

    _ = gskit.dma.init();

    gskit.core.initScreen(gs);
    _ = gskit.fontm.upload(gs, font); // ROM font, no asset files
    gskit.core.modeSwitch(gs, gskit.ONESHOT);

    // Wait for the pad to become stable (never SleepThread — it hangs).
    var tries: u32 = 0;
    while (tries < 400) : (tries += 1) {
        const state: c_int = ps2sdk.pad.padGetState(0, 0);
        if (state == ps2sdk.pad.STATE_STABLE or state == ps2sdk.pad.STATE_FINDCTP1) break;

        const start: u64 = ps2sdk.timer.GetTimerSystemTime();
        while (ps2sdk.timer.GetTimerSystemTime() -% start < ps2sdk.timer.ms(4)) {}
    }

    var frame: u32 = 0;
    while (true) {
        // Input: read + edge detection.
        if (ps2sdk.pad.padRead(0, 0, &pad_status) != 0) {
            const held: u16 = ps2sdk.pad.held(&pad_status);
            const pressed: u16 = ps2sdk.pad.pressed(held, previous_buttons);
            previous_buttons = held;
            if ((pressed & ps2sdk.pad.BTN_START) != 0) return;
        }

        const hue: u64 = gskit.reg.rgbaq((frame *% 4) & 0xFF, (frame *% 8 +% 80) & 0xFF, (frame *% 12 +% 160) & 0xFF, 0x80, 0);
        const px: f32 = @floatFromInt(frame % 560);

        gskit.core.clear(gs, gskit.reg.rgbaq(8, 10, 28, 0, 0));
        gskit.prim.sprite(gs, px, 360.0, px + 40.0, 400.0, 1, hue);
        gskit.fontm.printScaled(gs, font, 90.0, 120.0, 1, 1.5, gskit.reg.font(255, 255, 255, 0x80), "Neko on PS2");
        gskit.fontm.printScaled(gs, font, 90.0, 220.0, 1, 1.0, gskit.reg.font(0x50, 0xC8, 0xFF, 0x80), "START para sair");
        gskit.core.queueExec(gs);
        gskit.core.syncFlip(gs);

        frame +%= 1;
    }
}
