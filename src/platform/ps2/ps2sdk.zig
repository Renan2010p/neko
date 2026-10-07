// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! PS2SDK bindings for the EE (Emotion Engine / R5900), EE side only.
//!
//! These are hand-written `extern fn` declarations plus the constants and
//! structs from the PS2SDK headers. We declare them by hand instead of using
//! `@cImport`/translate-c because the PS2 target is freestanding and has no
//! libc headers for translate-c to consume. Every symbol below is resolved at
//! link time against the prebuilt PS2SDK archives (`libkernel.a`, `libpad.a`,
//! `libdebug.a`, …).
//!
//! Typical startup (also see `hello.zig`):
//!
//!     _ = ps2sdk.sif.sceSifInitRpc(0);
//!     _ = ps2sdk.loader.SifLoadModule("rom0:SIO2MAN", 0, "");
//!     _ = ps2sdk.loader.SifLoadModule("rom0:PADMAN", 0, "");
//!     _ = ps2sdk.pad.padInit(0);
//!     _ = ps2sdk.pad.padPortOpen(0, 0, &pad_area);
//!
//! Gotchas discovered on PCSX2/hardware:
//!   - `SleepThread()` blocks forever unless an IRQ is enabled for the thread.
//!     Prefer a busy-wait on `timer.GetTimerSystemTime()` for frame pacing.
//!   - `pad.padInit()` hangs on SIF RPC unless SIO2MAN/PADMAN were loaded from
//!     `rom0:` first.
//!   - `padRead()` reports buttons ACTIVE LOW: invert `~status.btns` to test.

/// SIF RPC — the EE <-> IOP transport. Call `sceSifInitRpc(0)` once before any
/// other IOP service (including the pad and the ROM font loader).
pub const sif: type = struct {
    pub extern fn sceSifInitRpc(mode: c_int) c_int;
    pub extern fn sceSifInitIopHeap() c_int;
    pub extern fn sceSifRebootIop(irx: [*c]const u8) c_int;
    pub extern fn sceSifSyncIop() void;
};

/// IOP module loading.
pub const loader: type = struct {
    /// Modules shipped in the console ROM, loadable as `rom0:<name>`.
    pub const SIO2MAN: [*c]const u8 = "rom0:SIO2MAN";
    pub const PADMAN: [*c]const u8 = "rom0:PADMAN";
    /// The ROM font used by gsKit's FONTM loader.
    pub const FONTM: [*c]const u8 = "rom0:FONTM";

    /// Loads and starts an IRX module. Returns a module id (>= 0) on success.
    pub extern fn SifLoadModule(path: [*c]const u8, arg_len: c_int, args: [*c]const u8) c_int;
    /// Like `SifLoadModule`, but also reports the module's `_start` result.
    pub extern fn SifLoadStartModule(path: [*c]const u8, arg_len: c_int, args: [*c]const u8, mod_res: *c_int) c_int;
    pub extern fn SifLoadFileInit() c_int;
};

/// Threads.
pub const thread: type = struct {
    /// Blocks the calling thread until an interrupt wakes it. On a bare PS2SDK
    /// program no IRQ is enabled for the thread, so this never returns — do not
    /// use it as a frame limiter. Use `timer` busy-waiting instead.
    pub extern fn SleepThread() c_int;
    pub extern fn WakeupThread(thread_id: c_int) c_int;
    pub extern fn CancelWakeupThread(thread_id: c_int) c_int;
    pub extern fn GetThreadId() c_int;
};

/// EE timers and system time.
pub const timer: type = struct {
    /// The EE bus clock, in Hz.
    pub const BUSCLK: u64 = 147_456_000;

    pub extern fn GetTimerSystemTime() u64;
    pub extern fn iGetTimerSystemTime() u64;
    pub extern fn InitTimer(mode: c_int) c_int;
    pub extern fn StartTimerSystemTime() c_int;
    /// Converts a bus-clock count to seconds + microseconds.
    pub extern fn TimerBusClock2USec(clocks: u64, seconds: *u32, microseconds: *u32) void;

    /// Converts a `GetTimerSystemTime()` count to microseconds.
    pub fn toUSec(clocks: u64) u64 {
        var seconds: u32 = 0;
        var micros: u32 = 0;
        TimerBusClock2USec(clocks, &seconds, &micros);
        return @as(u64, seconds) * 1_000_000 + @as(u64, micros);
    }

    /// Bus-clock cycles equivalent to `milliseconds`.
    pub fn ms(milliseconds: u64) u64 {
        return BUSCLK * milliseconds / 1_000;
    }
};

/// Controllers (libpad).
pub const pad: type = struct {
    pub const PORT_MAX: c_int = 2;
    pub const SLOT_MAX: c_int = 4;

    // Button bits — active HIGH once you invert `status.btns`.
    pub const BTN_SELECT: u16 = 0x0001;
    pub const BTN_L3: u16 = 0x0002;
    pub const BTN_R3: u16 = 0x0004;
    pub const BTN_START: u16 = 0x0008;
    pub const BTN_UP: u16 = 0x0010;
    pub const BTN_RIGHT: u16 = 0x0020;
    pub const BTN_DOWN: u16 = 0x0040;
    pub const BTN_LEFT: u16 = 0x0080;
    pub const BTN_L2: u16 = 0x0100;
    pub const BTN_R2: u16 = 0x0200;
    pub const BTN_L1: u16 = 0x0400;
    pub const BTN_R1: u16 = 0x0800;
    pub const BTN_TRIANGLE: u16 = 0x1000;
    pub const BTN_CIRCLE: u16 = 0x2000;
    pub const BTN_CROSS: u16 = 0x4000;
    pub const BTN_SQUARE: u16 = 0x8000;

    // padGetState()
    pub const STATE_DISCONN: c_int = 0x00;
    pub const STATE_FINDPAD: c_int = 0x01;
    pub const STATE_FINDCTP1: c_int = 0x02;
    pub const STATE_EXECCMD: c_int = 0x05;
    pub const STATE_STABLE: c_int = 0x06;
    pub const STATE_ERROR: c_int = 0x07;

    // Pad request states (padGetReqState)
    pub const RSTAT_COMPLETE: u8 = 0x00;
    pub const RSTAT_FAILED: u8 = 0x01;
    pub const RSTAT_BUSY: u8 = 0x02;

    // padSetMainMode()
    pub const MMODE_DIGITAL: c_int = 0;
    pub const MMODE_DUALSHOCK: c_int = 1;
    pub const MMODE_UNLOCK: c_int = 2;
    pub const MMODE_LOCK: c_int = 3;

    /// `struct padButtonStatus` from `<libpad.h>` (packed, 32 bytes). The pad
    /// area handed to `padPortOpen` must be 256 bytes, 64-byte aligned.
    pub const ButtonStatus: type = extern struct {
        ok: u8,
        mode: u8,
        btns: u16,
        rjoy_h: u8,
        rjoy_v: u8,
        ljoy_h: u8,
        ljoy_v: u8,
        right_p: u8,
        left_p: u8,
        up_p: u8,
        down_p: u8,
        triangle_p: u8,
        circle_p: u8,
        cross_p: u8,
        square_p: u8,
        l1_p: u8,
        r1_p: u8,
        l2_p: u8,
        r2_p: u8,
        unkn16: [12]u8,
    };

    /// Buttons currently held (libpad reports them active low).
    pub inline fn held(status: *const ButtonStatus) u16 {
        return ~status.btns;
    }

    /// Just-pressed edge: `held(current) & ~held(previous)`.
    pub inline fn pressed(current: u16, previous: u16) u16 {
        return current & ~previous;
    }

    pub extern fn padInit(mode: c_int) c_int;
    pub extern fn padPortInit(mode: c_int) c_int;
    pub extern fn padEnd() c_int;
    /// `pad_area` must point to a 256-byte, 64-byte-aligned scratch buffer.
    pub extern fn padPortOpen(port: c_int, slot: c_int, pad_area: ?*anyopaque) c_int;
    pub extern fn padPortClose(port: c_int, slot: c_int) c_int;
    pub extern fn padRead(port: c_int, slot: c_int, data: *ButtonStatus) u8;
    pub extern fn padGetState(port: c_int, slot: c_int) c_int;
    pub extern fn padGetReqState(port: c_int, slot: c_int) u8;
    pub extern fn padSetReqState(port: c_int, slot: c_int, state: c_int) c_int;
    pub extern fn padInfoMode(port: c_int, slot: c_int, info_mode: c_int, index: c_int) c_int;
    pub extern fn padSetMainMode(port: c_int, slot: c_int, mode: c_int, lock: c_int) c_int;
    pub extern fn padInfoPressMode(port: c_int, slot: c_int) c_int;
    pub extern fn padEnterPressMode(port: c_int, slot: c_int) c_int;
    pub extern fn padExitPressMode(port: c_int, slot: c_int) c_int;
    pub extern fn padGetPortMax() c_int;
    pub extern fn padGetSlotMax(port: c_int) c_int;
    pub extern fn padGetConnection(port: c_int, slot: c_int) c_int;
    pub extern fn padStateInt2String(state: c_int, buf: [*c]u8) void;
    pub extern fn padReqStateInt2String(state: c_int, buf: [*c]u8) void;
};

/// audsrv — streaming PCM audio. The IOP modules (`libsd.irx`, `audsrv.irx`)
/// must be loaded with `SifLoadModule` before `audsrv_init`.
pub const audsrv: type = struct {
    pub const Format: type = extern struct {
        freq: c_int,
        bits: c_int,
        channels: c_int,
    };

    pub extern fn audsrv_init() c_int;
    pub extern fn audsrv_quit() c_int;
    pub extern fn audsrv_set_format(fmt: *Format) c_int;
    pub extern fn audsrv_play_audio(chunk: [*]const u8, bytes: c_int) c_int;
    pub extern fn audsrv_wait_audio(bytes: c_int) c_int;
    pub extern fn audsrv_stop_audio() c_int;
    pub extern fn audsrv_set_volume(volume: c_int) c_int;
    pub extern fn audsrv_get_error() c_int;
};

/// libdebug — the tiny on-screen text console. Useful before gsKit is up, or
/// as a fallback when a graphics backend cannot start. The screen is an 80x28
/// grid of 8x16 cells (NTSC 640x448); `scr_setXY` takes cell coordinates.
pub const debug: type = struct {
    pub extern fn init_scr() void;
    pub extern fn scr_printf(fmt: [*c]const u8, ...) void;
    pub extern fn scr_setXY(x: c_int, y: c_int) void;
    pub extern fn scr_getX() c_int;
    pub extern fn scr_getY() c_int;
    pub extern fn scr_clear() void;
    pub extern fn scr_clearline(y: c_int) void;
    pub extern fn scr_clearchar(x: c_int, y: c_int) void;
    pub extern fn scr_setbgcolor(color: u32) void;
    pub extern fn scr_setfontcolor(color: u32) void;
    pub extern fn scr_setcursorcolor(color: u32) void;
    pub extern fn scr_setCursor(enable: c_int) void;
    pub extern fn scr_getCursor() c_int;
    /// `x`/`y` are PIXELS here (not cells), `color` is 0xRRGGBB.
    pub extern fn scr_putchar(x: c_int, y: c_int, color: u32, ch: c_int) void;
};
