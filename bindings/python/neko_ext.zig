// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `_neko` — the native half of the pygame-compatible front end.
//!
//! This is a CPython extension written in Zig. It deliberately defines **no
//! Python types**: the Python `pygame` package owns `Surface` (a `bytearray`),
//! `Rect`, `Color`, events and constants, and calls into this module for the
//! hot paths. Every function therefore takes the pixel buffer as an argument
//! (any object supporting the buffer protocol: `bytearray`, `array('I')`,
//! `memoryview`) plus the dimensions, and does the work in Zig on top of the
//! `neko_pygame` compatibility layer.
//!
//! Pixels are packed `0xAARRGGBB` (little-endian byte order B, G, R, A).

const std: type = @import("std");
const neko: type = @import("neko");
const pg: type = @import("neko_pygame");

const c = @import("c");

const Allocator: type = std.mem.Allocator;

// ── Module state ─────────────────────────────────────────────────────────────

/// Keeps the I/O instance alive for the lifetime of the interpreter.
var threaded: std.Io.Threaded = undefined;
var threaded_ready: bool = false;

var screen_ready: bool = false;
var frame_open: bool = false;

var canvas_w: u32 = 0;
var canvas_h: u32 = 0;
var canvas_tex: ?neko.TextureHandle = null;

// ── Small helpers ────────────────────────────────────────────────────────────

/// Returns Python's `None` (a fresh reference).
fn pyNone() [*c]c.PyObject {
    return c.Py_BuildValue("");
}

/// Sets a `RuntimeError` with `msg` and returns null (the C-API error signal).
fn fail(msg: [*:0]const u8) [*c]c.PyObject {
    c.PyErr_SetString(c.PyExc_RuntimeError, msg);
    return null;
}

/// Borrows the bytes of any buffer-protocol object.
fn getBuffer(obj: [*c]c.PyObject, writable: bool) ?c.Py_buffer {
    var view: c.Py_buffer = undefined;
    const flags: c_int = if (writable) c.PyBUF_WRITABLE | c.PyBUF_SIMPLE else c.PyBUF_SIMPLE;
    if (c.PyObject_GetBuffer(obj, &view, flags) != 0) return null;
    return view;
}

/// Wraps a borrowed pixel buffer as a `neko_pygame.Surface` view. The view
/// never owns the memory, so it must not be passed to code that frees it.
fn surfaceView(view: *const c.Py_buffer, w: u32, h: u32, alpha: u8) pg.Surface {
    const base: [*]u32 = @ptrCast(@alignCast(view.buf));
    return .{
        .w = w,
        .h = h,
        .pixels = base[0 .. @as(usize, w) * @as(usize, h)],
        .alpha = alpha,
        .allocator = std.heap.c_allocator,
    };
}

/// The SDL key code for a `neko.Key` (matches pygame's `K_*` constants).
fn keyCode(key: neko.Key) i32 {
    const ordinal: i32 = @backingInt(key);
    const letters: i32 = @backingInt(neko.Key.a);
    const digits: i32 = @backingInt(neko.Key.number_0);
    if (ordinal >= letters and ordinal <= @backingInt(neko.Key.z)) {
        return @as(i32, 'a') + (ordinal - letters);
    }
    if (ordinal >= digits and ordinal <= @backingInt(neko.Key.number_9)) {
        return @as(i32, '0') + (ordinal - digits);
    }
    return switch (key) {
        .unknown => 0,
        .up => 1073741906,
        .down => 1073741905,
        .left => 1073741904,
        .right => 1073741903,
        .enter => 13,
        .escape => 27,
        .space => 32,
        .tab => 9,
        .backspace => 8,
        else => 0,
    };
}

// ── Window / loop ────────────────────────────────────────────────────────────

/// init(width, height, title) -> None
fn pyInit(_: [*c]c.PyObject, args: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    var width: c.Py_ssize_t = 0;
    var height: c.Py_ssize_t = 0;
    var title: [*c]const u8 = "Neko PYGAME";
    if (c.PyArg_ParseTuple(args, "nn|s", &width, &height, &title) == 0) return null;

    if (!threaded_ready) {
        threaded = std.Io.Threaded.init(std.heap.c_allocator, .{});
        threaded_ready = true;
    }

    const vsync_env: ?[*:0]const u8 = @ptrCast(std.c.getenv("NEKO_VSYNC"));
    const vsync = if (vsync_env) |v|
        !(std.mem.eql(u8, std.mem.span(v), "0") or std.mem.eql(u8, std.mem.span(v), "false"))
    else
        true;

    const config: neko.Config = .{
        .allocator = std.heap.c_allocator,
        .io = threaded.io(),
        .title = std.mem.span(title),
        .width = @intCast(width),
        .height = @intCast(height),
        .assets_dir = "assets",
        .fullscreen = false,
        .vsync = vsync,
    };
    if (!neko.screen.init(config)) return fail("neko: could not open the window");
    screen_ready = true;

    canvas_w = @intCast(width);
    canvas_h = @intCast(height);
    canvas_tex = neko.texture.create(canvas_w, canvas_h);
    neko.screen.set_logical_size(canvas_w, canvas_h);
    return pyNone();
}

/// shutdown() -> None
fn pyShutdown(_: [*c]c.PyObject, _: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    if (screen_ready) {
        neko.screen.shutdown();
        screen_ready = false;
        canvas_tex = null;
        frame_open = false;
    }
    return pyNone();
}

/// present(buf, w, h) -> None
fn pyPresent(_: [*c]c.PyObject, args: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    var obj: [*c]c.PyObject = null;
    var w: c.Py_ssize_t = 0;
    var h: c.Py_ssize_t = 0;
    if (c.PyArg_ParseTuple(args, "Onn", &obj, &w, &h) == 0) return null;

    const view: c.Py_buffer = getBuffer(obj, false) orelse return fail("present: expected a bytes-like object");
    defer c.PyBuffer_Release(@constCast(&view));

    const tex: neko.TextureHandle = canvas_tex orelse return fail("present: call init() first");
    if (@as(u32, @intCast(w)) != canvas_w or @as(u32, @intCast(h)) != canvas_h) {
        canvas_w = @intCast(w);
        canvas_h = @intCast(h);
        neko.screen.set_logical_size(canvas_w, canvas_h);
        canvas_tex = neko.texture.create(canvas_w, canvas_h);
    }
    const active: neko.TextureHandle = canvas_tex orelse tex;
    const bytes: []const u8 = @as([*]const u8, @ptrCast(view.buf))[0..@intCast(view.len)];
    neko.texture.update(active, bytes, @intCast(w * 4));
    neko.texture.draw(active, neko.Rect.init(0, 0, @intCast(w), @intCast(h)), null, null);
    neko.screen.present();
    return pyNone();
}

// ── 3D renderer (M1) ─────────────────────────────────────────────────────────

/// render3d_supported() -> bool
fn pyRender3dSupported(_: [*c]c.PyObject, _: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    return c.PyBool_FromLong(if (neko.render3d.supported()) 1 else 0);
}

/// render3d_begin(px, py, pz, rx, ry, rz, fov, near, far) -> None
fn pyRender3dBegin(_: [*c]c.PyObject, args: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    var px: f32 = 0;
    var py: f32 = 0;
    var pz: f32 = 0;
    var rx: f32 = 0;
    var ry: f32 = 0;
    var rz: f32 = 0;
    var fov: f32 = 40;
    var near: f32 = 0.1;
    var far: f32 = 1000;
    if (c.PyArg_ParseTuple(args, "fffffffff", &px, &py, &pz, &rx, &ry, &rz, &fov, &near, &far) == 0) return null;

    var cam: neko.Camera = neko.Camera.init();
    cam.position = .{ .x = px, .y = py, .z = pz };
    cam.rotation = .{ .x = rx, .y = ry, .z = rz };
    cam.fov = fov;
    cam.near = near;
    cam.far = far;
    neko.render3d.begin(cam);
    return pyNone();
}

/// render3d_draw(kind, px, py, pz, rx, ry, rz, sx, sy, sz, rgba) -> None
fn pyRender3dDraw(_: [*c]c.PyObject, args: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    var kind: c_int = 0;
    var px: f32 = 0;
    var py: f32 = 0;
    var pz: f32 = 0;
    var rx: f32 = 0;
    var ry: f32 = 0;
    var rz: f32 = 0;
    var sx: f32 = 1;
    var sy: f32 = 1;
    var sz: f32 = 1;
    var rgba: c_uint = 0xFFFFFFFF;
    if (c.PyArg_ParseTuple(args, "ifffffffffI", &kind, &px, &py, &pz, &rx, &ry, &rz, &sx, &sy, &sz, &rgba) == 0) return null;

    const mesh_kind: neko.mesh3d.Kind = switch (kind) {
        0 => .cube,
        1 => .quad,
        2 => .plane,
        else => return pyNone(),
    };
    const model: neko.Mat4 = neko.Mat4.translation(.{ .x = px, .y = py, .z = pz })
        .mul(neko.Mat4.fromEulerDegrees(.{ .x = rx, .y = ry, .z = rz }))
        .mul(neko.Mat4.scaling(.{
        .x = if (sx != 0) sx else 0.001,
        .y = if (sy != 0) sy else 0.001,
        .z = if (sz != 0) sz else 0.001,
    }));
    const tint: neko.Color = .{
        .r = @intCast((rgba >> 16) & 0xff),
        .g = @intCast((rgba >> 8) & 0xff),
        .b = @intCast(rgba & 0xff),
        .a = @intCast((rgba >> 24) & 0xff),
    };
    neko.render3d.draw(mesh_kind, model, tint);
    return pyNone();
}

/// render3d_end() -> None
fn pyRender3dEnd(_: [*c]c.PyObject, _: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    neko.render3d.end();
    return pyNone();
}

/// render3d_present() -> None — swaps the buffers after a 3D frame.
fn pyRender3dPresent(_: [*c]c.PyObject, _: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    neko.screen.present();
    return pyNone();
}

/// set_caption(title) -> None
fn pySetCaption(_: [*c]c.PyObject, args: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    var title: [*c]const u8 = null;
    if (c.PyArg_ParseTuple(args, "s", &title) == 0) return null;
    neko.screen.set_caption(std.mem.span(title));
    return pyNone();
}

/// get_ticks() -> int
fn pyGetTicks(_: [*c]c.PyObject, _: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    return c.PyLong_FromLong(@intCast(neko.time.ticks_ms()));
}

/// delay(ms) -> None
fn pyDelay(_: [*c]c.PyObject, args: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    var ms: c.Py_ssize_t = 0;
    if (c.PyArg_ParseTuple(args, "n", &ms) == 0) return null;
    if (ms > 0) std.Io.sleep(threaded.io(), std.Io.Duration.fromMilliseconds(@intCast(ms)), .awake) catch {};
    return pyNone();
}

/// frame_begin() -> None — drains the platform event queue once per frame.
fn pyFrameBegin(_: [*c]c.PyObject, _: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    if (!frame_open) {
        neko.input.beginFrame();
        frame_open = true;
    }
    return pyNone();
}

/// frame_end() -> None
fn pyFrameEnd(_: [*c]c.PyObject, _: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    if (frame_open) {
        neko.input.endFrame();
        frame_open = false;
    }
    return pyNone();
}

/// get_events() -> list[tuple]
///
/// Each tuple is `(kind, a, b, c, name)` where `kind` matches the pygame event
/// type constants mapped by the Python layer.
fn pyGetEvents(_: [*c]c.PyObject, _: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    const list: [*c]c.PyObject = c.PyList_New(0);
    if (list == null) return null;

    while (neko.input.poll_event()) |ev| {
        const item: [*c]c.PyObject = switch (ev) {
            .quit => makeEvent(0, 0, 0, 0, ""),
            .key_down => |k| makeEvent(1, k.code, 0, 0, k.name),
            .key_up => |k| makeEvent(2, k.code, 0, 0, k.name),
            .mouse_motion => |m| makeEvent(3, m.x, m.y, 0, ""),
            .mouse_button_down => |b| makeEvent(4, @backingInt(b.button) + 1, b.x, b.y, ""),
            .mouse_button_up => |b| makeEvent(5, @backingInt(b.button) + 1, b.x, b.y, ""),
            .mouse_wheel => makeEvent(6, 0, 0, 0, ""),
        };
        if (item == null) {
            c.Py_DecRef(list);
            return null;
        }
        if (c.PyList_Append(list, item) != 0) {
            c.Py_DecRef(item);
            c.Py_DecRef(list);
            return null;
        }
        c.Py_DecRef(item);
    }
    return list;
}

/// Builds the `(kind, a, b, c, name)` event tuple.
fn makeEvent(kind: i32, a: i32, b: i32, cc: i32, name: []const u8) [*c]c.PyObject {
    const tup: [*c]c.PyObject = c.PyTuple_New(5);
    if (tup == null) return null;
    const name_obj: [*c]c.PyObject = c.PyUnicode_FromStringAndSize(
        if (name.len == 0) "" else name.ptr,
        @intCast(name.len),
    );
    if (name_obj == null) {
        c.Py_DecRef(tup);
        return null;
    }
    _ = c.PyTuple_SetItem(tup, 0, c.PyLong_FromLong(kind));
    _ = c.PyTuple_SetItem(tup, 1, c.PyLong_FromLong(a));
    _ = c.PyTuple_SetItem(tup, 2, c.PyLong_FromLong(b));
    _ = c.PyTuple_SetItem(tup, 3, c.PyLong_FromLong(cc));
    _ = c.PyTuple_SetItem(tup, 4, name_obj);
    return tup;
}

/// key_state() -> list[int] — the SDL key codes currently held down.
fn pyKeyState(_: [*c]c.PyObject, _: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    const list: [*c]c.PyObject = c.PyList_New(0);
    if (list == null) return null;
    const key_info = @typeInfo(neko.Key).@"enum";
    inline for (key_info.field_names, key_info.field_values) |_, value| {
        const k: neko.Key = @fromBackingInt(@intCast(value));
        if (neko.input.key(k)) {
            _ = c.PyList_Append(list, c.PyLong_FromLong(keyCode(k)));
        }
    }
    return list;
}

// ── Surface operations ───────────────────────────────────────────────────────

/// fill(buf, w, h, rgba) -> None
fn pyFill(_: [*c]c.PyObject, args: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    var obj: [*c]c.PyObject = null;
    var w: c.Py_ssize_t = 0;
    var h: c.Py_ssize_t = 0;
    var rgba: c_uint = 0;
    if (c.PyArg_ParseTuple(args, "OnnI", &obj, &w, &h, &rgba) == 0) return null;
    const view: c.Py_buffer = getBuffer(obj, true) orelse return fail("fill: expected a writable buffer");
    defer c.PyBuffer_Release(@constCast(&view));
    var s: pg.Surface = surfaceView(&view, @intCast(w), @intCast(h), 255);
    s.fill(unpackColor(rgba));
    return pyNone();
}

/// fill_rect(buf, w, h, x, y, rw, rh, rgba) -> None
fn pyFillRect(_: [*c]c.PyObject, args: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    var obj: [*c]c.PyObject = null;
    var w: c.Py_ssize_t = 0;
    var h: c.Py_ssize_t = 0;
    var x: c_int = 0;
    var y: c_int = 0;
    var rw: c_int = 0;
    var rh: c_int = 0;
    var rgba: c_uint = 0;
    if (c.PyArg_ParseTuple(args, "OnniiiiI", &obj, &w, &h, &x, &y, &rw, &rh, &rgba) == 0) return null;
    const view: c.Py_buffer = getBuffer(obj, true) orelse return fail("fill_rect: expected a writable buffer");
    defer c.PyBuffer_Release(@constCast(&view));
    var s: pg.Surface = surfaceView(&view, @intCast(w), @intCast(h), 255);
    s.fill_rect(pg.Rect.init(x, y, rw, rh), unpackColor(rgba));
    return pyNone();
}

/// blit(dst, dw, dh, src, sw, sh, x, y, alpha, src_has_alpha) -> None
fn pyBlit(_: [*c]c.PyObject, args: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    var dobj: [*c]c.PyObject = null;
    var dw: c.Py_ssize_t = 0;
    var dh: c.Py_ssize_t = 0;
    var sobj: [*c]c.PyObject = null;
    var sw: c.Py_ssize_t = 0;
    var sh: c.Py_ssize_t = 0;
    var x: c_int = 0;
    var y: c_int = 0;
    var alpha: c_int = 255;
    var has_alpha: c_int = 1;
    if (c.PyArg_ParseTuple(args, "OnnOnnii|ii", &dobj, &dw, &dh, &sobj, &sw, &sh, &x, &y, &alpha, &has_alpha) == 0) return null;
    const dview: c.Py_buffer = getBuffer(dobj, true) orelse return fail("blit: expected a writable destination");
    defer c.PyBuffer_Release(@constCast(&dview));
    const sview: c.Py_buffer = getBuffer(sobj, false) orelse return fail("blit: expected a readable source");
    defer c.PyBuffer_Release(@constCast(&sview));
    var dst: pg.Surface = surfaceView(&dview, @intCast(dw), @intCast(dh), 255);
    var src: pg.Surface = surfaceView(&sview, @intCast(sw), @intCast(sh), @intCast(alpha));
    src.has_alpha = has_alpha != 0;
    dst.blit(&src, pg.Point.init(x, y));
    return pyNone();
}

/// blit_area(dst, dw, dh, src, sw, sh, x, y, alpha, src_has_alpha, sx, sy, aw, ah) -> None
///
/// Like `blit`, but copies the `aw`x`ah` sub-rectangle of `src` starting at
/// `(sx, sy)` (pygame's `blit(source, dest, area=...)`).
fn pyBlitArea(_: [*c]c.PyObject, args: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    var dobj: [*c]c.PyObject = null;
    var dw: c.Py_ssize_t = 0;
    var dh: c.Py_ssize_t = 0;
    var sobj: [*c]c.PyObject = null;
    var sw: c.Py_ssize_t = 0;
    var sh: c.Py_ssize_t = 0;
    var x: c_int = 0;
    var y: c_int = 0;
    var alpha: c_int = 255;
    var has_alpha: c_int = 1;
    var sx: c_int = 0;
    var sy: c_int = 0;
    var aw: c_int = 0;
    var ah: c_int = 0;
    if (c.PyArg_ParseTuple(args, "OnnOnniiiiiiii", &dobj, &dw, &dh, &sobj, &sw, &sh, &x, &y, &alpha, &has_alpha, &sx, &sy, &aw, &ah) == 0) return null;
    const dview: c.Py_buffer = getBuffer(dobj, true) orelse return fail("blit_area: expected a writable destination");
    defer c.PyBuffer_Release(@constCast(&dview));
    const sview: c.Py_buffer = getBuffer(sobj, false) orelse return fail("blit_area: expected a readable source");
    defer c.PyBuffer_Release(@constCast(&sview));
    const dst: pg.Surface = surfaceView(&dview, @intCast(dw), @intCast(dh), 255);
    const src: pg.Surface = surfaceView(&sview, @intCast(sw), @intCast(sh), 255);
    blitRegion(dst, src, x, y, @intCast(alpha), sx, sy, aw, ah);
    return pyNone();
}

/// Alpha-composites a source sub-rectangle onto `dst` (same math as
/// `neko_pygame.Surface.blit`).
fn blitRegion(dst: pg.Surface, src: pg.Surface, dx: i32, dy: i32, alpha: u8, sx0: i32, sy0: i32, aw: i32, ah: i32) void {
    var row: i32 = 0;
    while (row < ah) : (row += 1) {
        const sy = sy0 + row;
        const ty = dy + row;
        if (sy < 0 or sy >= @as(i32, @intCast(src.h)) or ty < 0 or ty >= @as(i32, @intCast(dst.h))) continue;
        var col: i32 = 0;
        while (col < aw) : (col += 1) {
            const sx = sx0 + col;
            const tx = dx + col;
            if (sx < 0 or sx >= @as(i32, @intCast(src.w)) or tx < 0 or tx >= @as(i32, @intCast(dst.w))) continue;
            const sp = src.pixels[@intCast(sy * @as(i32, @intCast(src.w)) + sx)];
            const dp = dst.pixels[@intCast(ty * @as(i32, @intCast(dst.w)) + tx)];
            dst.pixels[@intCast(ty * @as(i32, @intCast(dst.w)) + tx)] = blendPixel(dp, sp, alpha);
        }
    }
}

/// The alpha-blend used by `neko_pygame`, replicated for region blits.
fn blendPixel(dst: u32, src: u32, extra: u8) u32 {
    var sa: u32 = src >> 24;
    if (extra != 255) sa = div255(sa * @as(u32, extra));
    var out: u32 = undefined;
    if (sa >= 255) {
        out = src | 0xff000000;
    } else if (sa == 0) {
        out = dst;
    } else {
        const inv: u32 = 255 - sa;
        const r: u32 = div255(((src >> 16) & 0xff) * sa + ((dst >> 16) & 0xff) * inv);
        const g: u32 = div255(((src >> 8) & 0xff) * sa + ((dst >> 8) & 0xff) * inv);
        const b: u32 = div255((src & 0xff) * sa + (dst & 0xff) * inv);
        const a: u32 = sa + div255(((dst >> 24) & 0xff) * inv);
        out = (a << 24) | (r << 16) | (g << 8) | b;
    }
    return out | (@max(src >> 24, dst >> 24) << 24);
}

/// Exact `x / 255` for `0 <= x <= 255*255` using shifts.
inline fn div255(x: u32) u32 {
    const t = x + 128;
    return (t + (t >> 8)) >> 8;
}

/// set_at(buf, w, h, x, y, rgba) -> None
fn pySetAt(_: [*c]c.PyObject, args: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    var obj: [*c]c.PyObject = null;
    var w: c.Py_ssize_t = 0;
    var h: c.Py_ssize_t = 0;
    var x: c_int = 0;
    var y: c_int = 0;
    var rgba: c_uint = 0;
    if (c.PyArg_ParseTuple(args, "OnniiI", &obj, &w, &h, &x, &y, &rgba) == 0) return null;
    const view: c.Py_buffer = getBuffer(obj, true) orelse return fail("set_at: expected a writable buffer");
    defer c.PyBuffer_Release(@constCast(&view));
    var s: pg.Surface = surfaceView(&view, @intCast(w), @intCast(h), 255);
    s.set_pixel(x, y, unpackColor(rgba));
    return pyNone();
}

/// get_at(buf, w, h, x, y) -> int
fn pyGetAt(_: [*c]c.PyObject, args: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    var obj: [*c]c.PyObject = null;
    var w: c.Py_ssize_t = 0;
    var h: c.Py_ssize_t = 0;
    var x: c_int = 0;
    var y: c_int = 0;
    if (c.PyArg_ParseTuple(args, "Onnii", &obj, &w, &h, &x, &y) == 0) return null;
    const view: c.Py_buffer = getBuffer(obj, false) orelse return fail("get_at: expected a readable buffer");
    defer c.PyBuffer_Release(@constCast(&view));
    const s: pg.Surface = surfaceView(&view, @intCast(w), @intCast(h), 255);
    return c.PyLong_FromUnsignedLong(packColor(s.get_pixel(x, y)));
}

/// draw_rect(buf, w, h, x, y, rw, rh, rgba, width, border_radius) -> None
fn pyDrawRect(_: [*c]c.PyObject, args: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    var obj: [*c]c.PyObject = null;
    var w: c.Py_ssize_t = 0;
    var h: c.Py_ssize_t = 0;
    var x: f64 = 0;
    var y: f64 = 0;
    var rw: f64 = 0;
    var rh: f64 = 0;
    var rgba: c_uint = 0;
    var width: c_int = 0;
    var radius: c_int = 0;
    if (c.PyArg_ParseTuple(args, "OnnddddI|ii", &obj, &w, &h, &x, &y, &rw, &rh, &rgba, &width, &radius) == 0) return null;
    const view: c.Py_buffer = getBuffer(obj, true) orelse return fail("draw_rect: expected a writable buffer");
    defer c.PyBuffer_Release(@constCast(&view));
    var s: pg.Surface = surfaceView(&view, @intCast(w), @intCast(h), 255);
    const r: pg.Rect = pg.Rect.init(@intFromFloat(x), @intFromFloat(y), @intFromFloat(rw), @intFromFloat(rh));
    const col = unpackColor(rgba);
    if (radius > 0) {
        pg.draw.round_rect(&s, r, col, width == 0, radius, width);
    } else if (width > 0) {
        pg.draw.round_rect(&s, r, col, false, 0, width);
    } else {
        pg.draw.rect(&s, r, col, true);
    }
    return pyNone();
}

/// draw_line(buf, w, h, x1, y1, x2, y2, rgba, width) -> None
fn pyDrawLine(_: [*c]c.PyObject, args: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    var obj: [*c]c.PyObject = null;
    var w: c.Py_ssize_t = 0;
    var h: c.Py_ssize_t = 0;
    var x1: f64 = 0;
    var y1: f64 = 0;
    var x2: f64 = 0;
    var y2: f64 = 0;
    var rgba: c_uint = 0;
    var width: c_int = 1;
    if (c.PyArg_ParseTuple(args, "OnnddddI|i", &obj, &w, &h, &x1, &y1, &x2, &y2, &rgba, &width) == 0) return null;
    const view: c.Py_buffer = getBuffer(obj, true) orelse return fail("draw_line: expected a writable buffer");
    defer c.PyBuffer_Release(@constCast(&view));
    var s: pg.Surface = surfaceView(&view, @intCast(w), @intCast(h), 255);
    pg.draw.thick_line(&s, @intFromFloat(x1), @intFromFloat(y1), @intFromFloat(x2), @intFromFloat(y2), unpackColor(rgba), width);
    return pyNone();
}

/// draw_circle(buf, w, h, cx, cy, radius, rgba, width) -> None
fn pyDrawCircle(_: [*c]c.PyObject, args: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    var obj: [*c]c.PyObject = null;
    var w: c.Py_ssize_t = 0;
    var h: c.Py_ssize_t = 0;
    var cx: f64 = 0;
    var cy: f64 = 0;
    var radius: f64 = 0;
    var rgba: c_uint = 0;
    var width: c_int = 0;
    if (c.PyArg_ParseTuple(args, "OnndddI|i", &obj, &w, &h, &cx, &cy, &radius, &rgba, &width) == 0) return null;
    const view: c.Py_buffer = getBuffer(obj, true) orelse return fail("draw_circle: expected a writable buffer");
    defer c.PyBuffer_Release(@constCast(&view));
    var s: pg.Surface = surfaceView(&view, @intCast(w), @intCast(h), 255);
    const rad: i32 = @intFromFloat(radius);
    pg.draw.ellipse_w(&s, pg.Rect.init(@as(i32, @intFromFloat(cx)) - rad, @as(i32, @intFromFloat(cy)) - rad, rad * 2, rad * 2), unpackColor(rgba), width == 0, width);
    return pyNone();
}

/// draw_ellipse(buf, w, h, x, y, rw, rh, rgba, width) -> None
fn pyDrawEllipse(_: [*c]c.PyObject, args: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    var obj: [*c]c.PyObject = null;
    var w: c.Py_ssize_t = 0;
    var h: c.Py_ssize_t = 0;
    var x: f64 = 0;
    var y: f64 = 0;
    var rw: f64 = 0;
    var rh: f64 = 0;
    var rgba: c_uint = 0;
    var width: c_int = 0;
    if (c.PyArg_ParseTuple(args, "OnnddddI|i", &obj, &w, &h, &x, &y, &rw, &rh, &rgba, &width) == 0) return null;
    const view: c.Py_buffer = getBuffer(obj, true) orelse return fail("draw_ellipse: expected a writable buffer");
    defer c.PyBuffer_Release(@constCast(&view));
    var s: pg.Surface = surfaceView(&view, @intCast(w), @intCast(h), 255);
    pg.draw.ellipse_w(&s, pg.Rect.init(@intFromFloat(x), @intFromFloat(y), @intFromFloat(rw), @intFromFloat(rh)), unpackColor(rgba), width == 0, width);
    return pyNone();
}

/// draw_arc(buf, w, h, x, y, rw, rh, rgba, start, stop, width) -> None
fn pyDrawArc(_: [*c]c.PyObject, args: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    var obj: [*c]c.PyObject = null;
    var w: c.Py_ssize_t = 0;
    var h: c.Py_ssize_t = 0;
    var x: f64 = 0;
    var y: f64 = 0;
    var rw: f64 = 0;
    var rh: f64 = 0;
    var rgba: c_uint = 0;
    var start: f32 = 0;
    var stop: f32 = 0;
    var width: c_int = 1;
    if (c.PyArg_ParseTuple(args, "OnnddddIff|i", &obj, &w, &h, &x, &y, &rw, &rh, &rgba, &start, &stop, &width) == 0) return null;
    const view: c.Py_buffer = getBuffer(obj, true) orelse return fail("draw_arc: expected a writable buffer");
    defer c.PyBuffer_Release(@constCast(&view));
    var s: pg.Surface = surfaceView(&view, @intCast(w), @intCast(h), 255);
    pg.draw.arc(&s, pg.Rect.init(@intFromFloat(x), @intFromFloat(y), @intFromFloat(rw), @intFromFloat(rh)), unpackColor(rgba), start, stop, width);
    return pyNone();
}

/// draw_polygon(buf, w, h, points, rgba) -> None
///
/// `points` is a buffer of packed little-endian i32 `(x, y)` pairs
/// (`array('i', ...)` in Python).
fn pyDrawPolygon(_: [*c]c.PyObject, args: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    var obj: [*c]c.PyObject = null;
    var w: c.Py_ssize_t = 0;
    var h: c.Py_ssize_t = 0;
    var pts: [*c]c.PyObject = null;
    var rgba: c_uint = 0;
    if (c.PyArg_ParseTuple(args, "OnnOI", &obj, &w, &h, &pts, &rgba) == 0) return null;
    const view: c.Py_buffer = getBuffer(obj, true) orelse return fail("draw_polygon: expected a writable buffer");
    defer c.PyBuffer_Release(@constCast(&view));
    const pview: c.Py_buffer = getBuffer(pts, false) orelse return fail("draw_polygon: expected an int32 point buffer");
    defer c.PyBuffer_Release(@constCast(&pview));

    const raw: [*]const i32 = @ptrCast(@alignCast(pview.buf));
    const n: usize = @intCast(@divTrunc(pview.len, 8));
    const points: []const pg.Point = @ptrCast(raw[0 .. n * 2]);

    var s: pg.Surface = surfaceView(&view, @intCast(w), @intCast(h), 255);
    pg.draw.polygon(&s, points, unpackColor(rgba));
    return pyNone();
}

/// scale(src, sw, sh, dst, dw, dh) -> None
fn pyScale(_: [*c]c.PyObject, args: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    var sobj: [*c]c.PyObject = null;
    var sw: c.Py_ssize_t = 0;
    var sh: c.Py_ssize_t = 0;
    var dobj: [*c]c.PyObject = null;
    var dw: c.Py_ssize_t = 0;
    var dh: c.Py_ssize_t = 0;
    if (c.PyArg_ParseTuple(args, "OnnOnn", &sobj, &sw, &sh, &dobj, &dw, &dh) == 0) return null;
    const sview: c.Py_buffer = getBuffer(sobj, false) orelse return fail("scale: expected a readable source");
    defer c.PyBuffer_Release(@constCast(&sview));
    const dview: c.Py_buffer = getBuffer(dobj, true) orelse return fail("scale: expected a writable destination");
    defer c.PyBuffer_Release(@constCast(&dview));

    const src: pg.Surface = surfaceView(&sview, @intCast(sw), @intCast(sh), 255);
    var dst: pg.Surface = surfaceView(&dview, @intCast(dw), @intCast(dh), 255);
    if (src.w == 0 or src.h == 0 or dst.w == 0 or dst.h == 0) return pyNone();
    var y: u32 = 0;
    while (y < dst.h) : (y += 1) {
        const sy: u32 = @min(@as(u32, @intCast(@as(u64, y) * src.h / dst.h)), src.h - 1);
        var x: u32 = 0;
        while (x < dst.w) : (x += 1) {
            const sx: u32 = @min(@as(u32, @intCast(@as(u64, x) * src.w / dst.w)), src.w - 1);
            dst.pixels[y * dst.w + x] = src.pixels[sy * src.w + sx];
        }
    }
    return pyNone();
}

/// unpack_rgba(dst, src, npixels) -> None
///
/// Converts packed `R,G,B,A` bytes (as PIL/Pillow produces) into Neko's
/// internal `0xAARRGGBB` little-endian layout. Doing this per-pixel in Python
/// was the single hottest loop when rendering text.
fn pyUnpackRgba(_: [*c]c.PyObject, args: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    var dobj: [*c]c.PyObject = null;
    var sobj: [*c]c.PyObject = null;
    var n: c.Py_ssize_t = 0;
    if (c.PyArg_ParseTuple(args, "OOn", &dobj, &sobj, &n) == 0) return null;
    const dview: c.Py_buffer = getBuffer(dobj, true) orelse return fail("unpack_rgba: expected a writable destination");
    defer c.PyBuffer_Release(@constCast(&dview));
    const sview: c.Py_buffer = getBuffer(sobj, false) orelse return fail("unpack_rgba: expected a readable source");
    defer c.PyBuffer_Release(@constCast(&sview));

    const dp: [*]u8 = @ptrCast(dview.buf);
    const sp: [*]const u8 = @ptrCast(sview.buf);
    const count: usize = @intCast(n);
    if (count * 4 > @as(usize, @intCast(sview.len)) or count * 4 > @as(usize, @intCast(dview.len))) {
        return fail("unpack_rgba: buffer too small");
    }
    var i: usize = 0;
    while (i < count) : (i += 1) {
        const o = i * 4;
        const r = sp[o];
        const g = sp[o + 1];
        const b = sp[o + 2];
        const a = sp[o + 3];
        dp[o] = b;
        dp[o + 1] = g;
        dp[o + 2] = r;
        dp[o + 3] = a;
    }
    return pyNone();
}

/// draw_polygon_seq(buf, w, h, points, rgba) -> None
///
/// Like `draw_polygon`, but takes the Python sequence of `(x, y)` pairs
/// directly and extracts the coordinates in C — the per-point Python loop was
/// the hottest path when a game fills many road segments per frame.
fn pyDrawPolygonSeq(_: [*c]c.PyObject, args: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    var obj: [*c]c.PyObject = null;
    var w: c.Py_ssize_t = 0;
    var h: c.Py_ssize_t = 0;
    var pts: [*c]c.PyObject = null;
    var rgba: c_uint = 0;
    if (c.PyArg_ParseTuple(args, "OnnOI", &obj, &w, &h, &pts, &rgba) == 0) return null;
    const view: c.Py_buffer = getBuffer(obj, true) orelse return fail("draw_polygon: expected a writable buffer");
    defer c.PyBuffer_Release(@constCast(&view));

    const fast: [*c]c.PyObject = c.PySequence_Fast(pts, "draw_polygon: points must be a sequence") orelse return null;
    defer c.Py_DecRef(fast);
    const n: c.Py_ssize_t = c.PySequence_Size(fast);
    if (n < 0) return null;
    const count: usize = @intCast(n);
    if (count == 0) return pyNone();

    const points = std.heap.c_allocator.alloc(pg.PointF, count) catch return fail("draw_polygon: out of memory");
    defer std.heap.c_allocator.free(points);

    var i: usize = 0;
    while (i < count) : (i += 1) {
        const item = c.PySequence_GetItem(fast, @intCast(i)) orelse return null;
        defer c.Py_DecRef(item);
        const xo = c.PySequence_GetItem(item, 0) orelse return null;
        defer c.Py_DecRef(xo);
        const yo = c.PySequence_GetItem(item, 1) orelse return null;
        defer c.Py_DecRef(yo);
        points[i] = .{ .x = coordOf(xo) orelse return null, .y = coordOf(yo) orelse return null };
    }

    var s: pg.Surface = surfaceView(&view, @intCast(w), @intCast(h), 255);
    pg.draw.polygon_f(&s, points, unpackColor(rgba));
    return pyNone();
}

/// Truncates a Python number (int *or* float) to `f32`, like pygame does.
fn coordOf(obj: [*c]c.PyObject) ?f32 {
    const v = c.PyLong_AsLong(obj);
    if (v == -1 and c.PyErr_Occurred() != null) {
        c.PyErr_Clear();
        const d = c.PyFloat_AsDouble(obj);
        if (d == -1.0 and c.PyErr_Occurred() != null) {
            c.PyErr_Clear();
            return null;
        }
        return @floatCast(d);
    }
    return @floatFromInt(v);
}

/// rotate(src, sw, sh, dst, dw, dh, angle_degrees) -> None
///
/// Nearest-neighbour rotation (positive angle rotates counter-clockwise, like
/// pygame). `dst` must already have the expanded dimensions.
fn pyRotate(_: [*c]c.PyObject, args: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    var sobj: [*c]c.PyObject = null;
    var sw: c.Py_ssize_t = 0;
    var sh: c.Py_ssize_t = 0;
    var dobj: [*c]c.PyObject = null;
    var dw: c.Py_ssize_t = 0;
    var dh: c.Py_ssize_t = 0;
    var angle: f64 = 0;
    if (c.PyArg_ParseTuple(args, "OnnOnnd", &sobj, &sw, &sh, &dobj, &dw, &dh, &angle) == 0) return null;
    const sview: c.Py_buffer = getBuffer(sobj, false) orelse return fail("rotate: expected a readable source");
    defer c.PyBuffer_Release(@constCast(&sview));
    const dview: c.Py_buffer = getBuffer(dobj, true) orelse return fail("rotate: expected a writable destination");
    defer c.PyBuffer_Release(@constCast(&dview));
    const src: pg.Surface = surfaceView(&sview, @intCast(sw), @intCast(sh), 255);
    const dst: pg.Surface = surfaceView(&dview, @intCast(dw), @intCast(dh), 255);
    rotateInto(src, dst, angle);
    return pyNone();
}

/// Nearest-neighbour rotation of `src` into `dst`.
fn rotateInto(src: pg.Surface, dst: pg.Surface, angle_deg: f64) void {
    // pygame rotates counter-clockwise for positive angles; with screen
    // coordinates (y down) that means negating here.
    const rad: f64 = -angle_deg * std.math.pi / 180.0;
    const cos: f64 = @cos(rad);
    const sin: f64 = @sin(rad);
    const scx: f64 = @as(f64, @floatFromInt(src.w)) / 2.0;
    const scy: f64 = @as(f64, @floatFromInt(src.h)) / 2.0;
    const dcx: f64 = @as(f64, @floatFromInt(dst.w)) / 2.0;
    const dcy: f64 = @as(f64, @floatFromInt(dst.h)) / 2.0;
    const fw: f64 = @floatFromInt(src.w);
    const fh: f64 = @floatFromInt(src.h);
    var y: u32 = 0;
    while (y < dst.h) : (y += 1) {
        var x: u32 = 0;
        while (x < dst.w) : (x += 1) {
            const dx: f64 = @as(f64, @floatFromInt(x)) - dcx;
            const dy: f64 = @as(f64, @floatFromInt(y)) - dcy;
            const sx: f64 = dx * cos + dy * sin + scx;
            const sy: f64 = -dx * sin + dy * cos + scy;
            var px: u32 = 0;
            if (sx >= 0 and sy >= 0 and sx < fw and sy < fh) {
                const ix: u32 = @intFromFloat(sx);
                const iy: u32 = @intFromFloat(sy);
                px = src.pixels[iy * src.w + ix];
            }
            dst.pixels[y * dst.w + x] = px;
        }
    }
}

/// flip_x(src, sw, sh, dst) -> None
fn pyFlipX(_: [*c]c.PyObject, args: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    var sobj: [*c]c.PyObject = null;
    var sw: c.Py_ssize_t = 0;
    var sh: c.Py_ssize_t = 0;
    var dobj: [*c]c.PyObject = null;
    if (c.PyArg_ParseTuple(args, "OnnO", &sobj, &sw, &sh, &dobj) == 0) return null;
    const sview: c.Py_buffer = getBuffer(sobj, false) orelse return fail("flip_x: expected a readable source");
    defer c.PyBuffer_Release(@constCast(&sview));
    const dview: c.Py_buffer = getBuffer(dobj, true) orelse return fail("flip_x: expected a writable destination");
    defer c.PyBuffer_Release(@constCast(&dview));
    const src: pg.Surface = surfaceView(&sview, @intCast(sw), @intCast(sh), 255);
    var dst: pg.Surface = surfaceView(&dview, @intCast(sw), @intCast(sh), 255);
    if (src.w == 0) return pyNone();
    var y: u32 = 0;
    while (y < src.h) : (y += 1) {
        var x: u32 = 0;
        while (x < src.w) : (x += 1) {
            dst.pixels[y * src.w + x] = src.pixels[y * src.w + (src.w - 1 - x)];
        }
    }
    return pyNone();
}

/// flip_y(src, sw, sh, dst) -> None
fn pyFlipY(_: [*c]c.PyObject, args: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    var sobj: [*c]c.PyObject = null;
    var sw: c.Py_ssize_t = 0;
    var sh: c.Py_ssize_t = 0;
    var dobj: [*c]c.PyObject = null;
    if (c.PyArg_ParseTuple(args, "OnnO", &sobj, &sw, &sh, &dobj) == 0) return null;
    const sview: c.Py_buffer = getBuffer(sobj, false) orelse return fail("flip_y: expected a readable source");
    defer c.PyBuffer_Release(@constCast(&sview));
    const dview: c.Py_buffer = getBuffer(dobj, true) orelse return fail("flip_y: expected a writable destination");
    defer c.PyBuffer_Release(@constCast(&dview));
    const src: pg.Surface = surfaceView(&sview, @intCast(sw), @intCast(sh), 255);
    var dst: pg.Surface = surfaceView(&dview, @intCast(sw), @intCast(sh), 255);
    var y: u32 = 0;
    while (y < src.h) : (y += 1) {
        const srow: usize = @as(usize, src.h - 1 - y) * src.w;
        const drow: usize = @as(usize, y) * dst.w;
        @memcpy(dst.pixels[drow .. drow + src.w], src.pixels[srow .. srow + src.w]);
    }
    return pyNone();
}

/// to_rgba(surface) -> bytes
fn pyToRgba(_: [*c]c.PyObject, args: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    var obj: [*c]c.PyObject = null;
    if (c.PyArg_ParseTuple(args, "O", &obj) == 0) return null;
    const view: c.Py_buffer = getBuffer(obj, false) orelse return fail("to_rgba: expected a readable buffer");
    defer c.PyBuffer_Release(@constCast(&view));
    const n: usize = @intCast(view.len);
    const src: [*]const u8 = @ptrCast(view.buf);
    const out: [*c]c.PyObject = c.PyBytes_FromStringAndSize(null, @intCast(n));
    if (out == null) return null;
    const dp: [*c]u8 = c.PyBytes_AsString(out);
    var i: usize = 0;
    while (i + 4 <= n) : (i += 4) {
        dp[i] = src[i + 2];
        dp[i + 1] = src[i + 1];
        dp[i + 2] = src[i];
        dp[i + 3] = src[i + 3];
    }
    return out;
}

// ── Colour packing ───────────────────────────────────────────────────────────

fn unpackColor(rgba: c_uint) neko.Color {
    const v: u32 = rgba;
    return .{
        .a = @truncate(v >> 24),
        .r = @truncate(v >> 16),
        .g = @truncate(v >> 8),
        .b = @truncate(v),
    };
}

fn packColor(col: neko.Color) u32 {
    return (@as(u32, col.a) << 24) | (@as(u32, col.r) << 16) | (@as(u32, col.g) << 8) | @as(u32, col.b);
}

// ── Native Surface + draw (direct C API) ─────────────────────────────────────

/// `Surface` implemented as a native Python type whose pixels live in Zig.
/// Everything hot (blit/fill/draw) reads the struct directly — no Python
/// wrapper, no per-call buffer acquisition.
const SurfaceObj = struct {
    ob_base: c.PyObject,
    w: u32,
    h: u32,
    alpha: u8,
    has_alpha: bool,
    pixels: [*c]u32,
};

var SurfaceType: [*c]c.PyTypeObject = null;
/// The Python `Surface` subclass created in `pygame/__init__.py` (adds
/// `get_rect(**kwargs)` etc.). Factories allocate this class when set.
var SurfaceClass: [*c]c.PyObject = null;

const SRCALPHA_FLAG: c_int = 0x00010000;

fn asSurf(obj: [*c]c.PyObject) ?*SurfaceObj {
    if (obj == null) {
        _ = c.PyErr_SetString(c.PyExc_TypeError, "expected a Surface");
        return null;
    }
    if (c.PyType_IsSubtype(obj.*.ob_type, SurfaceType) == 0) {
        _ = c.PyErr_SetString(c.PyExc_TypeError, "expected a Surface");
        return null;
    }
    return @ptrCast(@alignCast(obj));
}

fn makeSurf(w: u32, h: u32, has_alpha: bool) ?*SurfaceObj {
    const typ: [*c]c.PyTypeObject = if (SurfaceClass != null) @ptrCast(SurfaceClass) else SurfaceType;
    const obj: [*c]c.PyObject = c.PyType_GenericAlloc(typ, 0) orelse return null;
    const cap: usize = @max(1, @as(usize, w) * @as(usize, h));
    const buf = std.heap.c_allocator.alloc(u32, cap) catch {
        c.Py_DecRef(obj);
        _ = c.PyErr_SetString(c.PyExc_MemoryError, "Surface: out of memory");
        return null;
    };
    @memset(buf, 0);
    const s: *SurfaceObj = @ptrCast(@alignCast(obj));
    s.w = w;
    s.h = h;
    s.alpha = 255;
    s.has_alpha = has_alpha;
    s.pixels = buf.ptr;
    return s;
}

fn surfAsPg(s: *SurfaceObj) pg.Surface {
    return .{
        .w = s.w,
        .h = s.h,
        .pixels = s.pixels[0 .. @as(usize, s.w) * @as(usize, s.h)],
        .alpha = s.alpha,
        .has_alpha = s.has_alpha,
        .allocator = std.heap.c_allocator,
    };
}

fn surfaceDealloc(obj: [*c]c.PyObject) callconv(.c) void {
    const s: *SurfaceObj = @ptrCast(@alignCast(obj));
    if (s.pixels != null) {
        const cap: usize = @max(1, @as(usize, s.w) * @as(usize, s.h));
        std.heap.c_allocator.free(s.pixels[0..cap]);
        s.pixels = null;
    }
    const t = obj.*.ob_type;
    if (t.*.tp_free) |f| f(@ptrCast(obj));
}

fn surfaceNew(typ: [*c]c.PyObject, args: [*c]c.PyObject, kwds: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    _ = typ;
    _ = kwds;
    const n: c.Py_ssize_t = c.PyTuple_Size(args);
    if (n < 1) {
        _ = c.PyErr_SetString(c.PyExc_TypeError, "Surface() needs a size");
        return null;
    }
    const a0: [*c]c.PyObject = c.PyTuple_GetItem(args, 0);
    var w: u32 = 0;
    var h: u32 = 0;
    var flags: c_int = 0;
    const seq: [*c]c.PyObject = c.PySequence_Fast(a0, "");
    if (seq != null and c.PySequence_Size(seq) == 2) {
        const w0 = c.PySequence_GetItem(seq, 0);
        const h0 = c.PySequence_GetItem(seq, 1);
        if (w0 != null and h0 != null) {
            w = @intCast(c.PyLong_AsLong(w0));
            h = @intCast(c.PyLong_AsLong(h0));
        }
        if (w0 != null) c.Py_DecRef(w0);
        if (h0 != null) c.Py_DecRef(h0);
        c.Py_DecRef(seq);
        if (n >= 2) flags = @intCast(c.PyLong_AsLong(c.PyTuple_GetItem(args, 1)));
    } else {
        if (seq != null) c.Py_DecRef(seq) else c.PyErr_Clear();
        w = @intCast(c.PyLong_AsLong(a0));
        if (n >= 2) h = @intCast(c.PyLong_AsLong(c.PyTuple_GetItem(args, 1)));
        if (n >= 3) flags = @intCast(c.PyLong_AsLong(c.PyTuple_GetItem(args, 2)));
    }
    if (c.PyErr_Occurred() != null) return null;
    const s = makeSurf(w, h, (flags & SRCALPHA_FLAG) != 0) orelse return null;
    return @ptrCast(s);
}

// -- numeric parsing (int or float, like pygame) ------------------------------

fn numOf(obj: [*c]c.PyObject) f64 {
    const l = c.PyLong_AsLong(obj);
    if (l == -1 and c.PyErr_Occurred() != null) {
        c.PyErr_Clear();
        const d = c.PyFloat_AsDouble(obj);
        if (d == -1.0 and c.PyErr_Occurred() != null) {
            c.PyErr_Clear();
            return 0;
        }
        return d;
    }
    return @floatFromInt(l);
}

/// Fills `out` with up to `out.len` numbers from a Python sequence; returns the
/// count. Returns 0 (and clears the error) when `obj` is not a sequence.
fn numsOf(obj: [*c]c.PyObject, out: []f64) usize {
    const seq: [*c]c.PyObject = c.PySequence_Fast(obj, "") orelse {
        c.PyErr_Clear();
        return 0;
    };
    defer c.Py_DecRef(seq);
    const n: c.Py_ssize_t = c.PySequence_Size(seq);
    var i: usize = 0;
    while (i < out.len and i < n) : (i += 1) {
        const item = c.PySequence_GetItem(seq, @intCast(i)) orelse return i;
        defer c.Py_DecRef(item);
        out[i] = numOf(item);
    }
    return i;
}

fn packNum(f: f64) u32 {
    const v: i64 = @intFromFloat(f);
    return @intCast(v & 0xff);
}

/// Parses a pygame colour (3/4-sequence or int) into `0xAARRGGBB`.
fn colorOf(obj: [*c]c.PyObject) u32 {
    var a: [4]f64 = .{ 0, 0, 0, 0 };
    const n = numsOf(obj, &a);
    if (n == 3) return 0xFF000000 | (packNum(a[0]) << 16) | (packNum(a[1]) << 8) | packNum(a[2]);
    if (n == 4) return (packNum(a[3]) << 24) | (packNum(a[0]) << 16) | (packNum(a[1]) << 8) | packNum(a[2]);
    c.PyErr_Clear();
    const v = c.PyLong_AsLong(obj);
    if (v == -1 and c.PyErr_Occurred() != null) {
        c.PyErr_Clear();
        return 0;
    }
    const u: u32 = @intCast(v);
    return if (u <= 0xFFFFFF) 0xFF000000 | u else u;
}

fn rectOf(obj: [*c]c.PyObject, out: *[4]f64) bool {
    return numsOf(obj, out) == 4;
}

// -- draw submodule -----------------------------------------------------------

var kw_rect = [6][*c]const u8{ "surface", "color", "rect", "width", "border_radius", null };
var kw_circle = [6][*c]const u8{ "surface", "color", "center", "radius", "width", null };
var kw_line = [6][*c]const u8{ "surface", "color", "start", "end", "width", null };
var kw_ellipse = [5][*c]const u8{ "surface", "color", "rect", "width", null };
var kw_polygon = [5][*c]const u8{ "surface", "color", "points", "width", null };
var kw_arc = [7][*c]const u8{ "surface", "color", "rect", "start_angle", "stop_angle", "width", null };

fn drawRect(_: [*c]c.PyObject, args: [*c]c.PyObject, kwds: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    var so: [*c]c.PyObject = null;
    var co: [*c]c.PyObject = null;
    var ro: [*c]c.PyObject = null;
    var width: c_int = 0;
    var radius: c_int = 0;
    if (c.PyArg_ParseTupleAndKeywords(args, kwds, "OOO|ii", @ptrCast(&kw_rect), &so, &co, &ro, &width, &radius) == 0) return null;
    const s = asSurf(so) orelse return null;
    var r: [4]f64 = undefined;
    if (!rectOf(ro, &r)) return fail("draw.rect: rect must be a 4-sequence");
    var ps = surfAsPg(s);
    const rr = pg.Rect.init(@intFromFloat(r[0]), @intFromFloat(r[1]), @intFromFloat(r[2]), @intFromFloat(r[3]));
    const col = unpackColor(colorOf(co));
    if (radius > 0) {
        pg.draw.round_rect(&ps, rr, col, width == 0, radius, width);
    } else if (width > 0) {
        pg.draw.round_rect(&ps, rr, col, false, 0, width);
    } else {
        pg.draw.rect(&ps, rr, col, true);
    }
    return pyNone();
}

fn drawCircle(_: [*c]c.PyObject, args: [*c]c.PyObject, kwds: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    var so: [*c]c.PyObject = null;
    var co: [*c]c.PyObject = null;
    var po: [*c]c.PyObject = null;
    var radius: f64 = 0;
    var width: c_int = 0;
    if (c.PyArg_ParseTupleAndKeywords(args, kwds, "OOOd|i", @ptrCast(&kw_circle), &so, &co, &po, &radius, &width) == 0) return null;
    const s = asSurf(so) orelse return null;
    var c2: [2]f64 = undefined;
    if (numsOf(po, &c2) != 2) return fail("draw.circle: center must be a 2-sequence");
    var ps = surfAsPg(s);
    const rad: i32 = @intFromFloat(radius);
    pg.draw.ellipse_w(&ps, pg.Rect.init(@as(i32, @intFromFloat(c2[0])) - rad, @as(i32, @intFromFloat(c2[1])) - rad, rad * 2, rad * 2), unpackColor(colorOf(co)), width == 0, width);
    return pyNone();
}

fn drawLine(_: [*c]c.PyObject, args: [*c]c.PyObject, kwds: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    var so: [*c]c.PyObject = null;
    var co: [*c]c.PyObject = null;
    var ao: [*c]c.PyObject = null;
    var bo: [*c]c.PyObject = null;
    var width: c_int = 1;
    if (c.PyArg_ParseTupleAndKeywords(args, kwds, "OOOO|i", @ptrCast(&kw_line), &so, &co, &ao, &bo, &width) == 0) return null;
    const s = asSurf(so) orelse return null;
    var a: [2]f64 = undefined;
    var b: [2]f64 = undefined;
    if (numsOf(ao, &a) != 2 or numsOf(bo, &b) != 2) return fail("draw.line: points must be 2-sequences");
    var ps = surfAsPg(s);
    pg.draw.thick_line(&ps, @intFromFloat(a[0]), @intFromFloat(a[1]), @intFromFloat(b[0]), @intFromFloat(b[1]), unpackColor(colorOf(co)), width);
    return pyNone();
}

fn drawEllipse(_: [*c]c.PyObject, args: [*c]c.PyObject, kwds: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    var so: [*c]c.PyObject = null;
    var co: [*c]c.PyObject = null;
    var ro: [*c]c.PyObject = null;
    var width: c_int = 0;
    if (c.PyArg_ParseTupleAndKeywords(args, kwds, "OOO|i", @ptrCast(&kw_ellipse), &so, &co, &ro, &width) == 0) return null;
    const s = asSurf(so) orelse return null;
    var r: [4]f64 = undefined;
    if (!rectOf(ro, &r)) return fail("draw.ellipse: rect must be a 4-sequence");
    var ps = surfAsPg(s);
    pg.draw.ellipse_w(&ps, pg.Rect.init(@intFromFloat(r[0]), @intFromFloat(r[1]), @intFromFloat(r[2]), @intFromFloat(r[3])), unpackColor(colorOf(co)), width == 0, width);
    return pyNone();
}

fn drawArc(_: [*c]c.PyObject, args: [*c]c.PyObject, kwds: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    var so: [*c]c.PyObject = null;
    var co: [*c]c.PyObject = null;
    var ro: [*c]c.PyObject = null;
    var start: f64 = 0;
    var stop: f64 = 0;
    var width: c_int = 1;
    if (c.PyArg_ParseTupleAndKeywords(args, kwds, "OOOdd|i", @ptrCast(&kw_arc), &so, &co, &ro, &start, &stop, &width) == 0) return null;
    const s = asSurf(so) orelse return null;
    var r: [4]f64 = undefined;
    if (!rectOf(ro, &r)) return fail("draw.arc: rect must be a 4-sequence");
    var ps = surfAsPg(s);
    pg.draw.arc(&ps, pg.Rect.init(@intFromFloat(r[0]), @intFromFloat(r[1]), @intFromFloat(r[2]), @intFromFloat(r[3])), unpackColor(colorOf(co)), @floatCast(start), @floatCast(stop), width);
    return pyNone();
}

fn drawPolygonF(_: [*c]c.PyObject, args: [*c]c.PyObject, kwds: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    var so: [*c]c.PyObject = null;
    var co: [*c]c.PyObject = null;
    var po: [*c]c.PyObject = null;
    var width: c_int = 0;
    if (c.PyArg_ParseTupleAndKeywords(args, kwds, "OOO|i", @ptrCast(&kw_polygon), &so, &co, &po, &width) == 0) return null;
    const s = asSurf(so) orelse return null;
    const fast = c.PySequence_Fast(po, "points must be a sequence") orelse return null;
    defer c.Py_DecRef(fast);
    const n: c.Py_ssize_t = c.PySequence_Size(fast);
    if (n < 0) return null;
    const count: usize = @intCast(n);
    if (count == 0) return pyNone();
    const points = std.heap.c_allocator.alloc(pg.PointF, count) catch return fail("draw.polygon: out of memory");
    defer std.heap.c_allocator.free(points);
    var i: usize = 0;
    while (i < count) : (i += 1) {
        const item = c.PySequence_GetItem(fast, @intCast(i)) orelse return null;
        defer c.Py_DecRef(item);
        const xo = c.PySequence_GetItem(item, 0) orelse return null;
        defer c.Py_DecRef(xo);
        const yo = c.PySequence_GetItem(item, 1) orelse return null;
        defer c.Py_DecRef(yo);
        points[i] = .{ .x = @floatCast(numOf(xo)), .y = @floatCast(numOf(yo)) };
    }
    var ps = surfAsPg(s);
    pg.draw.polygon_f(&ps, points, unpackColor(colorOf(co)));
    return pyNone();
}

fn drawLines(_: [*c]c.PyObject, args: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    var so: [*c]c.PyObject = null;
    var co: [*c]c.PyObject = null;
    var closed: c_int = 0;
    var po: [*c]c.PyObject = null;
    var width: c_int = 1;
    if (c.PyArg_ParseTuple(args, "OOiO|i", &so, &co, &closed, &po, &width) == 0) return null;
    const s = asSurf(so) orelse return null;
    const fast = c.PySequence_Fast(po, "points must be a sequence") orelse return null;
    defer c.Py_DecRef(fast);
    const n: c.Py_ssize_t = c.PySequence_Size(fast);
    const count: usize = @intCast(n);
    if (count < 2) return pyNone();
    const pts = std.heap.c_allocator.alloc(pg.PointF, count + 1) catch return fail("out of memory");
    defer std.heap.c_allocator.free(pts);
    var i: usize = 0;
    while (i < count) : (i += 1) {
        const item = c.PySequence_GetItem(fast, @intCast(i)) orelse return null;
        defer c.Py_DecRef(item);
        const xo = c.PySequence_GetItem(item, 0) orelse return null;
        defer c.Py_DecRef(xo);
        const yo = c.PySequence_GetItem(item, 1) orelse return null;
        defer c.Py_DecRef(yo);
        pts[i] = .{ .x = @floatCast(numOf(xo)), .y = @floatCast(numOf(yo)) };
    }
    var ps = surfAsPg(s);
    const col = unpackColor(colorOf(co));
    var k: usize = 0;
    while (k + 1 < count) : (k += 1) {
        pg.draw.thick_line(&ps, @intFromFloat(pts[k].x), @intFromFloat(pts[k].y), @intFromFloat(pts[k + 1].x), @intFromFloat(pts[k + 1].y), col, width);
    }
    if (closed != 0) {
        pg.draw.thick_line(&ps, @intFromFloat(pts[count - 1].x), @intFromFloat(pts[count - 1].y), @intFromFloat(pts[0].x), @intFromFloat(pts[0].y), col, width);
    }
    return pyNone();
}

// -- Surface methods ----------------------------------------------------------

fn mGetWidth(self: [*c]c.PyObject, _: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    const s: *SurfaceObj = @ptrCast(@alignCast(self));
    return c.PyLong_FromLong(@intCast(s.w));
}

fn mGetHeight(self: [*c]c.PyObject, _: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    const s: *SurfaceObj = @ptrCast(@alignCast(self));
    return c.PyLong_FromLong(@intCast(s.h));
}

fn mGetSize(self: [*c]c.PyObject, _: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    const s: *SurfaceObj = @ptrCast(@alignCast(self));
    return c.Py_BuildValue("(II)", @as(c_uint, s.w), @as(c_uint, s.h));
}

/// The Python `Rect` class (set from `pygame/__init__.py`) so `get_rect` can
/// return a real Rect with keyword anchoring.
var RectClass: [*c]c.PyObject = null;

fn mGetRect(self: [*c]c.PyObject, args: [*c]c.PyObject, kwds: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    _ = args;
    const s: *SurfaceObj = @ptrCast(@alignCast(self));
    if (RectClass == null) {
        return c.Py_BuildValue("(IIII)", @as(c_int, 0), @as(c_int, 0), @as(c_uint, s.w), @as(c_uint, s.h));
    }
    const r: [*c]c.PyObject = c.PyObject_CallFunction(RectClass, "(iiii)", @as(c_int, 0), @as(c_int, 0), @as(c_int, @intCast(s.w)), @as(c_int, @intCast(s.h))) orelse return null;
    if (kwds != null and c.Py_IsNone(kwds) == 0) {
        var pos: c.Py_ssize_t = 0;
        var key: [*c]c.PyObject = null;
        var val: [*c]c.PyObject = null;
        while (c.PyDict_Next(kwds, &pos, &key, &val) != 0) {
            if (c.PyObject_SetAttr(r, key, val) != 0) {
                c.Py_DecRef(r);
                return null;
            }
        }
    }
    return r;
}

fn mSetAlpha(self: [*c]c.PyObject, args: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    const s: *SurfaceObj = @ptrCast(@alignCast(self));
    var v: [*c]c.PyObject = null;
    if (c.PyArg_ParseTuple(args, "O", &v) == 0) return null;
    if (c.Py_IsNone(v) != 0) {
        s.alpha = 255;
    } else {
        s.alpha = @intCast(c.PyLong_AsLong(v) & 0xff);
    }
    return pyNone();
}

fn mGetAlpha(self: [*c]c.PyObject, _: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    const s: *SurfaceObj = @ptrCast(@alignCast(self));
    return c.PyLong_FromLong(@intCast(s.alpha));
}

fn mGetFlags(self: [*c]c.PyObject, _: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    _ = self;
    return c.PyLong_FromLong(0);
}

fn mHasAlpha(self: [*c]c.PyObject, _: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    const s: *SurfaceObj = @ptrCast(@alignCast(self));
    return c.PyBool_FromLong(if (s.has_alpha) 1 else 0);
}

fn mGetPitch(self: [*c]c.PyObject, _: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    const s: *SurfaceObj = @ptrCast(@alignCast(self));
    return c.PyLong_FromLong(@intCast(s.w * 4));
}

fn noArgs(_: [*c]c.PyObject, _: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    return pyNone();
}

fn mFill(self: [*c]c.PyObject, args: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    var co: [*c]c.PyObject = null;
    var ro: [*c]c.PyObject = null;
    if (c.PyArg_ParseTuple(args, "O|O", &co, &ro) == 0) return null;
    const s: *SurfaceObj = @ptrCast(@alignCast(self));
    var ps = surfAsPg(s);
    const col = unpackColor(colorOf(co));
    if (ro == null or c.Py_IsNone(ro) != 0) {
        ps.fill(col);
    } else {
        var r: [4]f64 = undefined;
        if (!rectOf(ro, &r)) return fail("fill: rect must be a 4-sequence");
        ps.fill_rect(pg.Rect.init(@intFromFloat(r[0]), @intFromFloat(r[1]), @intFromFloat(r[2]), @intFromFloat(r[3])), col);
    }
    c.Py_IncRef(self);
    return self;
}

var kw_blit = [5][*c]const u8{ "source", "dest", "area", "special_flags", null };

fn mBlit(self: [*c]c.PyObject, args: [*c]c.PyObject, kwds: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    var so: [*c]c.PyObject = null;
    var do: [*c]c.PyObject = null;
    var ao: [*c]c.PyObject = null;
    var flags: [*c]c.PyObject = null;
    if (c.PyArg_ParseTupleAndKeywords(args, kwds, "OO|OO", @ptrCast(&kw_blit), &so, &do, &ao, &flags) == 0) return null;
    const dst: *SurfaceObj = @ptrCast(@alignCast(self));
    const src = asSurf(so) orelse return null;
    var d: [4]f64 = undefined;
    _ = numsOf(do, &d);
    var ps_dst = surfAsPg(dst);
    const ps_src = surfAsPg(src);
    if (ao != null and c.Py_IsNone(ao) == 0) {
        var a: [4]f64 = undefined;
        if (rectOf(ao, &a)) {
            blitRegion(ps_dst, ps_src, @intFromFloat(d[0]), @intFromFloat(d[1]), ps_src.alpha, @intFromFloat(a[0]), @intFromFloat(a[1]), @intFromFloat(a[2]), @intFromFloat(a[3]));
        }
    } else {
        ps_dst.blit(&ps_src, pg.Point.init(@intFromFloat(d[0]), @intFromFloat(d[1])));
    }
    return pyNone();
}

fn mSetAt(self: [*c]c.PyObject, args: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    var po: [*c]c.PyObject = null;
    var co: [*c]c.PyObject = null;
    if (c.PyArg_ParseTuple(args, "OO", &po, &co) == 0) return null;
    const s: *SurfaceObj = @ptrCast(@alignCast(self));
    var p: [2]f64 = undefined;
    if (numsOf(po, &p) != 2) return fail("set_at: pos must be a 2-sequence");
    const x: i32 = @intFromFloat(p[0]);
    const y: i32 = @intFromFloat(p[1]);
    if (x < 0 or y < 0 or x >= @as(i32, @intCast(s.w)) or y >= @as(i32, @intCast(s.h))) return pyNone();
    s.pixels[@intCast(y * @as(i32, @intCast(s.w)) + x)] = colorOf(co);
    return pyNone();
}

fn mGetAt(self: [*c]c.PyObject, args: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    var po: [*c]c.PyObject = null;
    if (c.PyArg_ParseTuple(args, "O", &po) == 0) return null;
    const s: *SurfaceObj = @ptrCast(@alignCast(self));
    var p: [2]f64 = undefined;
    if (numsOf(po, &p) != 2) return fail("get_at: pos must be a 2-sequence");
    const x: i32 = @intFromFloat(p[0]);
    const y: i32 = @intFromFloat(p[1]);
    if (x < 0 or y < 0 or x >= @as(i32, @intCast(s.w)) or y >= @as(i32, @intCast(s.h))) return c.PyLong_FromLong(0);
    const v = s.pixels[@intCast(y * @as(i32, @intCast(s.w)) + x)];
    return c.PyLong_FromUnsignedLong(v);
}

fn mCopy(self: [*c]c.PyObject, _: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    const s: *SurfaceObj = @ptrCast(@alignCast(self));
    const out = makeSurf(s.w, s.h, s.has_alpha) orelse return null;
    @memcpy(out.pixels[0 .. @as(usize, s.w) * @as(usize, s.h)], s.pixels[0 .. @as(usize, s.w) * @as(usize, s.h)]);
    out.alpha = s.alpha;
    return @ptrCast(out);
}

fn mConvert(self: [*c]c.PyObject, _: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    const s: *SurfaceObj = @ptrCast(@alignCast(self));
    const out = makeSurf(s.w, s.h, s.has_alpha) orelse return null;
    @memcpy(out.pixels[0 .. @as(usize, s.w) * @as(usize, s.h)], s.pixels[0 .. @as(usize, s.w) * @as(usize, s.h)]);
    out.alpha = s.alpha;
    return @ptrCast(out);
}

fn mConvertAlpha(self: [*c]c.PyObject, _: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    const s: *SurfaceObj = @ptrCast(@alignCast(self));
    const out = makeSurf(s.w, s.h, true) orelse return null;
    @memcpy(out.pixels[0 .. @as(usize, s.w) * @as(usize, s.h)], s.pixels[0 .. @as(usize, s.w) * @as(usize, s.h)]);
    out.alpha = s.alpha;
    return @ptrCast(out);
}

fn mTobytes(self: [*c]c.PyObject, _: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    const s: *SurfaceObj = @ptrCast(@alignCast(self));
    const n: usize = @as(usize, s.w) * @as(usize, s.h) * 4;
    const out: [*c]c.PyObject = c.PyBytes_FromStringAndSize(null, @intCast(n));
    if (out == null) return null;
    const dp: [*c]u8 = c.PyBytes_AsString(out);
    const sp: [*]const u8 = @ptrCast(s.pixels);
    var i: usize = 0;
    while (i < n) : (i += 1) dp[i] = sp[i];
    return out;
}

fn mGetBuffer(self: [*c]c.PyObject, view: [*c]c.Py_buffer, flags: c_int) callconv(.c) c_int {
    const s: *SurfaceObj = @ptrCast(@alignCast(self));
    if ((flags & c.PyBUF_WRITABLE) != 0 and false) {}
    view.* = std.mem.zeroes(c.Py_buffer);
    view.*.buf = @ptrCast(s.pixels);
    view.*.obj = self;
    view.*.len = @intCast(@as(usize, s.w) * @as(usize, s.h) * 4);
    view.*.readonly = 0;
    view.*.itemsize = 4;
    view.*.format = null;
    view.*.ndim = 0;
    c.Py_IncRef(self);
    return 0;
}

fn mReleaseBuffer(_: [*c]c.PyObject, _: [*c]c.Py_buffer) callconv(.c) void {
    // `PyBuffer_Release` decrefs `view->obj` itself; we only release internal
    // resources (none). Decrementing here would double-free the Surface.
}

var surface_methods = [_]c.PyMethodDef{
    .{ .ml_name = "fill", .ml_meth = mFill, .ml_flags = c.METH_VARARGS, .ml_doc = null },
    .{ .ml_name = "blit", .ml_meth = @ptrCast(@constCast(&mBlit)), .ml_flags = c.METH_VARARGS | c.METH_KEYWORDS, .ml_doc = null },
    .{ .ml_name = "set_at", .ml_meth = mSetAt, .ml_flags = c.METH_VARARGS, .ml_doc = null },
    .{ .ml_name = "get_at", .ml_meth = mGetAt, .ml_flags = c.METH_VARARGS, .ml_doc = null },
    .{ .ml_name = "set_pixel", .ml_meth = mSetAt, .ml_flags = c.METH_VARARGS, .ml_doc = null },
    .{ .ml_name = "get_pixel", .ml_meth = mGetAt, .ml_flags = c.METH_VARARGS, .ml_doc = null },
    .{ .ml_name = "get_width", .ml_meth = mGetWidth, .ml_flags = c.METH_NOARGS, .ml_doc = null },
    .{ .ml_name = "get_height", .ml_meth = mGetHeight, .ml_flags = c.METH_NOARGS, .ml_doc = null },
    .{ .ml_name = "get_size", .ml_meth = mGetSize, .ml_flags = c.METH_NOARGS, .ml_doc = null },
    .{ .ml_name = "get_rect", .ml_meth = @ptrCast(@constCast(&mGetRect)), .ml_flags = c.METH_VARARGS | c.METH_KEYWORDS, .ml_doc = null },
    .{ .ml_name = "get_bounding_rect", .ml_meth = @ptrCast(@constCast(&mGetRect)), .ml_flags = c.METH_VARARGS | c.METH_KEYWORDS, .ml_doc = null },
    .{ .ml_name = "set_alpha", .ml_meth = mSetAlpha, .ml_flags = c.METH_VARARGS, .ml_doc = null },
    .{ .ml_name = "get_alpha", .ml_meth = mGetAlpha, .ml_flags = c.METH_NOARGS, .ml_doc = null },
    .{ .ml_name = "get_flags", .ml_meth = mGetFlags, .ml_flags = c.METH_NOARGS, .ml_doc = null },
    .{ .ml_name = "get_pitch", .ml_meth = mGetPitch, .ml_flags = c.METH_NOARGS, .ml_doc = null },
    .{ .ml_name = "lock", .ml_meth = noArgs, .ml_flags = c.METH_NOARGS, .ml_doc = null },
    .{ .ml_name = "unlock", .ml_meth = noArgs, .ml_flags = c.METH_NOARGS, .ml_doc = null },
    .{ .ml_name = "copy", .ml_meth = mCopy, .ml_flags = c.METH_NOARGS, .ml_doc = null },
    .{ .ml_name = "convert", .ml_meth = mConvert, .ml_flags = c.METH_VARARGS, .ml_doc = null },
    .{ .ml_name = "convert_alpha", .ml_meth = mConvertAlpha, .ml_flags = c.METH_VARARGS, .ml_doc = null },
    .{ .ml_name = "tobytes", .ml_meth = mTobytes, .ml_flags = c.METH_NOARGS, .ml_doc = null },
    .{ .ml_name = "has_alpha", .ml_meth = mHasAlpha, .ml_flags = c.METH_NOARGS, .ml_doc = null },
    .{ .ml_name = "set_colorkey", .ml_meth = mSetAlpha, .ml_flags = c.METH_VARARGS, .ml_doc = null },
    .{ .ml_name = null, .ml_meth = null, .ml_flags = 0, .ml_doc = null },
};

var surface_slots = [_]c.PyType_Slot{
    .{ .slot = c.Py_tp_new, .pfunc = @ptrCast(@constCast(&surfaceNew)) },
    .{ .slot = c.Py_tp_dealloc, .pfunc = @ptrCast(@constCast(&surfaceDealloc)) },
    .{ .slot = c.Py_tp_methods, .pfunc = @ptrCast(&surface_methods) },
    .{ .slot = c.Py_bf_getbuffer, .pfunc = @ptrCast(@constCast(&mGetBuffer)) },
    .{ .slot = c.Py_bf_releasebuffer, .pfunc = @ptrCast(@constCast(&mReleaseBuffer)) },
    .{ .slot = 0, .pfunc = null },
};

var surface_spec = c.PyType_Spec{
    .name = "_neko.Surface",
    .basicsize = @sizeOf(SurfaceObj),
    .itemsize = 0,
    .flags = @as(c_uint, c.Py_TPFLAGS_DEFAULT) | @as(c_uint, c.Py_TPFLAGS_BASETYPE),
    .slots = &surface_slots,
};

// -- draw submodule definition ------------------------------------------------

var draw_methods = [_]c.PyMethodDef{
    .{ .ml_name = "rect", .ml_meth = @ptrCast(&drawRect), .ml_flags = c.METH_VARARGS | c.METH_KEYWORDS, .ml_doc = null },
    .{ .ml_name = "circle", .ml_meth = @ptrCast(&drawCircle), .ml_flags = c.METH_VARARGS | c.METH_KEYWORDS, .ml_doc = null },
    .{ .ml_name = "line", .ml_meth = @ptrCast(&drawLine), .ml_flags = c.METH_VARARGS | c.METH_KEYWORDS, .ml_doc = null },
    .{ .ml_name = "ellipse", .ml_meth = @ptrCast(&drawEllipse), .ml_flags = c.METH_VARARGS | c.METH_KEYWORDS, .ml_doc = null },
    .{ .ml_name = "arc", .ml_meth = @ptrCast(&drawArc), .ml_flags = c.METH_VARARGS | c.METH_KEYWORDS, .ml_doc = null },
    .{ .ml_name = "polygon", .ml_meth = @ptrCast(&drawPolygonF), .ml_flags = c.METH_VARARGS | c.METH_KEYWORDS, .ml_doc = null },
    .{ .ml_name = "lines", .ml_meth = drawLines, .ml_flags = c.METH_VARARGS, .ml_doc = null },
    .{ .ml_name = "aaline", .ml_meth = @ptrCast(&drawLine), .ml_flags = c.METH_VARARGS | c.METH_KEYWORDS, .ml_doc = null },
    .{ .ml_name = "aalines", .ml_meth = drawLines, .ml_flags = c.METH_VARARGS, .ml_doc = null },
    .{ .ml_name = null, .ml_meth = null, .ml_flags = 0, .ml_doc = null },
};

var draw_def = c.PyModuleDef{
    .m_name = "_neko.draw",
    .m_doc = null,
    .m_size = -1,
    .m_methods = &draw_methods,
};

// ── Module definition ────────────────────────────────────────────────────────

fn pyNewSurface(_: [*c]c.PyObject, args: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    var w: c.Py_ssize_t = 0;
    var h: c.Py_ssize_t = 0;
    var alpha: c_int = 0;
    if (c.PyArg_ParseTuple(args, "nn|i", &w, &h, &alpha) == 0) return null;
    const s = makeSurf(@intCast(w), @intCast(h), alpha != 0) orelse return null;
    return @ptrCast(s);
}

fn pySetSurfaceClass(_: [*c]c.PyObject, args: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    var cls: [*c]c.PyObject = null;
    if (c.PyArg_ParseTuple(args, "O", &cls) == 0) return null;
    c.Py_IncRef(cls);
    if (SurfaceClass != null) c.Py_DecRef(SurfaceClass);
    SurfaceClass = cls;
    return pyNone();
}

fn pySetRectClass(_: [*c]c.PyObject, args: [*c]c.PyObject) callconv(.c) [*c]c.PyObject {
    var cls: [*c]c.PyObject = null;
    if (c.PyArg_ParseTuple(args, "O", &cls) == 0) return null;
    c.Py_IncRef(cls);
    if (RectClass != null) c.Py_DecRef(RectClass);
    RectClass = cls;
    return pyNone();
}

var surface_funcs = [_]c.PyMethodDef{
    .{ .ml_name = "new_surface", .ml_meth = pyNewSurface, .ml_flags = c.METH_VARARGS, .ml_doc = "new_surface(w, h, alpha=False) -> Surface" },
    .{ .ml_name = "_set_surface_class", .ml_meth = pySetSurfaceClass, .ml_flags = c.METH_VARARGS, .ml_doc = null },
    .{ .ml_name = "_set_rect_class", .ml_meth = pySetRectClass, .ml_flags = c.METH_VARARGS, .ml_doc = null },
    .{ .ml_name = null, .ml_meth = null, .ml_flags = 0, .ml_doc = null },
};

// CPython writes into the module definition during initialisation, so these
// must live in writable memory (a `const` global lands in `.rodata` and
// segfaults in release builds).
var methods = [_]c.PyMethodDef{
    .{ .ml_name = "init", .ml_meth = pyInit, .ml_flags = c.METH_VARARGS, .ml_doc = "init(width, height, title) -> None" },
    .{ .ml_name = "shutdown", .ml_meth = pyShutdown, .ml_flags = c.METH_NOARGS, .ml_doc = "shutdown() -> None" },
    .{ .ml_name = "present", .ml_meth = pyPresent, .ml_flags = c.METH_VARARGS, .ml_doc = "present(buf, w, h) -> None" },
    .{ .ml_name = "set_caption", .ml_meth = pySetCaption, .ml_flags = c.METH_VARARGS, .ml_doc = "set_caption(title) -> None" },
    .{ .ml_name = "get_ticks", .ml_meth = pyGetTicks, .ml_flags = c.METH_NOARGS, .ml_doc = "get_ticks() -> int" },
    .{ .ml_name = "delay", .ml_meth = pyDelay, .ml_flags = c.METH_VARARGS, .ml_doc = "delay(ms) -> None" },
    .{ .ml_name = "frame_begin", .ml_meth = pyFrameBegin, .ml_flags = c.METH_NOARGS, .ml_doc = "frame_begin() -> None" },
    .{ .ml_name = "frame_end", .ml_meth = pyFrameEnd, .ml_flags = c.METH_NOARGS, .ml_doc = "frame_end() -> None" },
    .{ .ml_name = "get_events", .ml_meth = pyGetEvents, .ml_flags = c.METH_NOARGS, .ml_doc = "get_events() -> list" },
    .{ .ml_name = "key_state", .ml_meth = pyKeyState, .ml_flags = c.METH_NOARGS, .ml_doc = "key_state() -> list[int]" },
    .{ .ml_name = "render3d_supported", .ml_meth = pyRender3dSupported, .ml_flags = c.METH_NOARGS, .ml_doc = "render3d_supported() -> bool" },
    .{ .ml_name = "render3d_begin", .ml_meth = pyRender3dBegin, .ml_flags = c.METH_VARARGS, .ml_doc = "render3d_begin(px,py,pz,rx,ry,rz,fov,near,far) -> None" },
    .{ .ml_name = "render3d_draw", .ml_meth = pyRender3dDraw, .ml_flags = c.METH_VARARGS, .ml_doc = "render3d_draw(kind,px,py,pz,rx,ry,rz,sx,sy,sz,rgba) -> None" },
    .{ .ml_name = "render3d_end", .ml_meth = pyRender3dEnd, .ml_flags = c.METH_NOARGS, .ml_doc = "render3d_end() -> None" },
    .{ .ml_name = "render3d_present", .ml_meth = pyRender3dPresent, .ml_flags = c.METH_NOARGS, .ml_doc = "render3d_present() -> None" },
    .{ .ml_name = "fill", .ml_meth = pyFill, .ml_flags = c.METH_VARARGS, .ml_doc = "fill(buf, w, h, rgba) -> None" },
    .{ .ml_name = "fill_rect", .ml_meth = pyFillRect, .ml_flags = c.METH_VARARGS, .ml_doc = "fill_rect(...) -> None" },
    .{ .ml_name = "blit", .ml_meth = pyBlit, .ml_flags = c.METH_VARARGS, .ml_doc = "blit(...) -> None" },
    .{ .ml_name = "blit_area", .ml_meth = pyBlitArea, .ml_flags = c.METH_VARARGS, .ml_doc = "blit_area(...) -> None" },
    .{ .ml_name = "set_at", .ml_meth = pySetAt, .ml_flags = c.METH_VARARGS, .ml_doc = "set_at(...) -> None" },
    .{ .ml_name = "get_at", .ml_meth = pyGetAt, .ml_flags = c.METH_VARARGS, .ml_doc = "get_at(...) -> int" },
    .{ .ml_name = "draw_rect", .ml_meth = pyDrawRect, .ml_flags = c.METH_VARARGS, .ml_doc = "draw_rect(...) -> None" },
    .{ .ml_name = "draw_line", .ml_meth = pyDrawLine, .ml_flags = c.METH_VARARGS, .ml_doc = "draw_line(...) -> None" },
    .{ .ml_name = "draw_circle", .ml_meth = pyDrawCircle, .ml_flags = c.METH_VARARGS, .ml_doc = "draw_circle(...) -> None" },
    .{ .ml_name = "draw_ellipse", .ml_meth = pyDrawEllipse, .ml_flags = c.METH_VARARGS, .ml_doc = "draw_ellipse(...) -> None" },
    .{ .ml_name = "draw_arc", .ml_meth = pyDrawArc, .ml_flags = c.METH_VARARGS, .ml_doc = "draw_arc(...) -> None" },
    .{ .ml_name = "draw_polygon", .ml_meth = pyDrawPolygon, .ml_flags = c.METH_VARARGS, .ml_doc = "draw_polygon(...) -> None" },
    .{ .ml_name = "draw_polygon_seq", .ml_meth = pyDrawPolygonSeq, .ml_flags = c.METH_VARARGS, .ml_doc = "draw_polygon_seq(...) -> None" },
    .{ .ml_name = "scale", .ml_meth = pyScale, .ml_flags = c.METH_VARARGS, .ml_doc = "scale(...) -> None" },
    .{ .ml_name = "flip_x", .ml_meth = pyFlipX, .ml_flags = c.METH_VARARGS, .ml_doc = "flip_x(...) -> None" },
    .{ .ml_name = "flip_y", .ml_meth = pyFlipY, .ml_flags = c.METH_VARARGS, .ml_doc = "flip_y(...) -> None" },
    .{ .ml_name = "to_rgba", .ml_meth = pyToRgba, .ml_flags = c.METH_VARARGS, .ml_doc = "to_rgba(surface) -> bytes" },
    .{ .ml_name = "rotate", .ml_meth = pyRotate, .ml_flags = c.METH_VARARGS, .ml_doc = "rotate(...) -> None" },
    .{ .ml_name = "unpack_rgba", .ml_meth = pyUnpackRgba, .ml_flags = c.METH_VARARGS, .ml_doc = "unpack_rgba(...) -> None" },
    .{ .ml_name = null, .ml_meth = null, .ml_flags = 0, .ml_doc = null },
};

var module_def = c.PyModuleDef{
    .m_name = "_neko",
    .m_doc = "Native core for the Neko pygame backend.",
    .m_size = -1,
    .m_methods = @constCast(&methods),
};

/// The module initialiser CPython looks up (`PyInit__neko`).
pub export fn PyInit__neko() [*c]c.PyObject {
    const m: [*c]c.PyObject = c.PyModule_Create2(@constCast(&module_def), 1013);
    if (m == null) return null;
    SurfaceType = @ptrCast(c.PyType_FromSpec(&surface_spec) orelse return null);
    if (c.PyModule_AddObject(m, "Surface", @ptrCast(SurfaceType)) != 0) return null;
    const draw_mod: [*c]c.PyObject = c.PyModule_Create2(&draw_def, 1013) orelse return null;
    if (c.PyModule_AddObject(m, "draw", draw_mod) != 0) return null;
    if (c.PyModule_AddFunctions(m, @constCast(&surface_funcs)) != 0) return null;
    return m;
}
