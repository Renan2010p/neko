// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! `sdl2-opengl` backend — an SDL2 window with an **OpenGL** presenter.
//!
//! This backend targets the pygame translation layer: the game draws into a
//! CPU `Surface` in the Python layer and `display.flip` uploads it as one
//! streaming texture, drawn as a full-screen quad. It implements that path
//! (window, events, timing, a streaming texture and `present`) in OpenGL; the
//! engine's own primitives/`text`/`sound` are not provided here (the Python
//! path does not use them). Select it with `-Dbackend=sdl2-opengl`.

const std: type = @import("std");
const c: type = @import("c");
const gl: type = @import("gl");
const engine: type = @import("neko");

const Allocator: type = std.mem.Allocator;

/// GL texture id -> (w, h).
const SizeMap: type = std.AutoHashMapUnmanaged(u32, [2]u32);

const Vertex: type = extern struct {
    x: f32,
    y: f32,
    u: f32,
    v: f32,
};

const QUAD: [4]Vertex = .{
    .{ .x = 0, .y = 0, .u = 0, .v = 1 },
    .{ .x = 1, .y = 0, .u = 1, .v = 1 },
    .{ .x = 0, .y = 1, .u = 0, .v = 0 },
    .{ .x = 1, .y = 1, .u = 1, .v = 0 },
};

const VERT_SRC =
    \\#version 330 core
    \\layout(location = 0) in vec2 a_pos;
    \\layout(location = 1) in vec2 a_uv;
    \\uniform vec4 u_rect; // x0, y_bottom, x1, y_top (NDC)
    \\out vec2 v_uv;
    \\void main() {
    \\    vec2 p = mix(u_rect.xy, u_rect.zw, a_pos);
    \\    gl_Position = vec4(p, 0.0, 1.0);
    \\    v_uv = a_uv;
    \\}
;

const FRAG_SRC =
    \\#version 330 core
    \\in vec2 v_uv;
    \\uniform sampler2D u_tex;
    \\uniform float u_alpha;
    \\out vec4 o_color;
    \\void main() {
    \\    vec4 c = texture(u_tex, v_uv);
    \\    o_color = vec4(c.rgb, c.a * u_alpha);
    \\}
;

const OpenglEngine: type = struct {
    allocator: Allocator = undefined,
    assets_dir: []u8 = &.{},
    window: ?*c.SDL_Window = null,
    context: c.SDL_GLContext = null,
    program: gl.GLuint = 0,
    vao: gl.GLuint = 0,
    vbo: gl.GLuint = 0,
    u_rect: gl.GLint = -1,
    u_alpha: gl.GLint = -1,
    sizes: SizeMap = .empty,
    logical_w: u32 = 0,
    logical_h: u32 = 0,
    running: bool = false,

    pub fn backend(self: *OpenglEngine) engine.Backend {
        return engine.Backend{ .ptr = @ptrCast(self), .vtable = &vtable };
    }
};

pub const Engine: type = OpenglEngine;
var instance: OpenglEngine = .{};

/// Which backend this module implements.
pub const kind: engine.BackendKind = .sdl2_opengl;

/// Returns the backend as an abstract handle. Called by `src/core/platform.zig`.
pub fn create() engine.Backend {
    return instance.backend();
}

fn as_self(ptr: *anyopaque) *OpenglEngine {
    return @ptrCast(@alignCast(ptr));
}

fn cstr(ptr: [*c]const u8) []const u8 {
    if (ptr == null) return "";
    return std.mem.span(@as([*:0]const u8, @ptrCast(ptr)));
}

// ── GL helpers ───────────────────────────────────────────────────────────────

fn compileShader(kind_: gl.GLenum, src: [*:0]const u8) ?gl.GLuint {
    const sh: gl.GLuint = gl.glCreateShader(kind_);
    if (sh == 0) return null;
    var ptr: [*c]const gl.GLchar = src;
    gl.glShaderSource(sh, 1, @ptrCast(&ptr), null);
    gl.glCompileShader(sh);
    var ok: gl.GLint = 0;
    gl.glGetShaderiv(sh, gl.GL_COMPILE_STATUS, &ok);
    if (ok == 0) {
        gl.glDeleteShader(sh);
        return null;
    }
    return sh;
}

fn linkProgram() ?gl.GLuint {
    const vs = compileShader(gl.GL_VERTEX_SHADER, VERT_SRC) orelse return null;
    defer gl.glDeleteShader(vs);
    const fs = compileShader(gl.GL_FRAGMENT_SHADER, FRAG_SRC) orelse return null;
    defer gl.glDeleteShader(fs);

    const prog: gl.GLuint = gl.glCreateProgram();
    gl.glAttachShader(prog, vs);
    gl.glAttachShader(prog, fs);
    gl.glLinkProgram(prog);
    var ok: gl.GLint = 0;
    gl.glGetProgramiv(prog, gl.GL_LINK_STATUS, &ok);
    if (ok == 0) {
        gl.glDeleteProgram(prog);
        return null;
    }
    return prog;
}

fn glTextureFor(handle: engine.TextureHandle) gl.GLuint {
    return @intCast(handle.id);
}

// ── Vtable ───────────────────────────────────────────────────────────────────

const vtable: engine.Backend.VTable = .{
    .init = vt_init,
    .shutdown = vt_shutdown,
    .keeps_running = vt_keeps_running,
    .request_stop = vt_request_stop,
    .present = vt_present,
    .poll_event = vt_poll_event,
    .ticks_ms = vt_ticks_ms,
    .set_logical_size = vt_set_logical_size,
    .set_title = vt_set_title,
    .set_fullscreen = vt_set_fullscreen,
    .set_vsync = vt_set_vsync,
    .set_resolution = vt_set_resolution,
    .logical_size = vt_logical_size,
    .display_modes = vt_display_modes,
    .supports_curved_panorama = vt_supports_curved_panorama,
    .supports_offscreen_targets = vt_supports_offscreen_targets,
    .set_draw_offset = vt_set_draw_offset,
    .clear = vt_clear,
    .draw_rect = s_rect,
    .draw_line = s_line,
    .draw_circle = s_circle,
    .load_texture = vt_load_texture,
    .create_target = vt_create_target,
    .create_texture = vt_create_texture,
    .update_texture = vt_update_texture,
    .draw_texture = vt_draw_texture,
    .draw_texture_rotated = vt_draw_texture_rotated,
    .texture_size = vt_texture_size,
    .geometry = vt_geometry,
    .set_render_target = vt_set_render_target,
    .load_font = vt_load_font,
    .draw_text = vt_draw_text,
    .draw_text_rotated = vt_draw_text_rotated,
    .text_size = vt_text_size,
    .load_sound = vt_load_sound,
    .play_sound = vt_play_sound,
    .stop_channel = s_i32,
    .stop_all_sounds = s_void,
    .set_master_volume = s_i32,
    .set_sfx_volume = s_i32,
    .set_music_volume = s_i32,
    .read_file = vt_read_file,
    .write_file = vt_write_file,
    .delete_file = s_delfile,
    .file_exists = vt_false,
    .mouse_pos = vt_mouse_pos,
    .update_discord = s_discord,
};

fn vt_init(ptr: *anyopaque, config: engine.Config) bool {
    const self: *OpenglEngine = as_self(ptr);
    self.allocator = config.allocator;
    self.assets_dir = self.allocator.dupe(u8, config.assets_dir) catch return false;

    if (c.SDL_Init(c.SDL_INIT_VIDEO) != 0) return false;

    _ = c.SDL_GL_SetAttribute(c.SDL_GL_CONTEXT_MAJOR_VERSION, 3);
    _ = c.SDL_GL_SetAttribute(c.SDL_GL_CONTEXT_MINOR_VERSION, 3);
    _ = c.SDL_GL_SetAttribute(c.SDL_GL_CONTEXT_PROFILE_MASK, c.SDL_GL_CONTEXT_PROFILE_CORE);

    const title_z: [:0]u8 = self.allocator.dupeZ(u8, config.title) catch return false;
    defer self.allocator.free(title_z);

    self.window = c.SDL_CreateWindow(
        title_z.ptr,
        c.SDL_WINDOWPOS_CENTERED,
        c.SDL_WINDOWPOS_CENTERED,
        @intCast(config.width),
        @intCast(config.height),
        c.SDL_WINDOW_OPENGL | c.SDL_WINDOW_SHOWN,
    );
    if (self.window == null) return false;

    self.context = c.SDL_GL_CreateContext(self.window);
    if (self.context == null) return false;
    _ = c.SDL_GL_MakeCurrent(self.window, self.context);
    _ = c.SDL_GL_SetSwapInterval(if (config.vsync) 1 else 0);

    self.program = linkProgram() orelse return false;
    self.u_rect = gl.glGetUniformLocation(self.program, "u_rect");
    self.u_alpha = gl.glGetUniformLocation(self.program, "u_alpha");

    gl.glGenVertexArrays(1, &self.vao);
    gl.glGenBuffers(1, &self.vbo);
    gl.glBindVertexArray(self.vao);
    gl.glBindBuffer(gl.GL_ARRAY_BUFFER, self.vbo);
    gl.glBufferData(gl.GL_ARRAY_BUFFER, @sizeOf([4]Vertex), &QUAD, gl.GL_STATIC_DRAW);
    gl.glEnableVertexAttribArray(0);
    gl.glVertexAttribPointer(0, 2, gl.GL_FLOAT, gl.GL_FALSE, @sizeOf(Vertex), @ptrFromInt(0));
    gl.glEnableVertexAttribArray(1);
    gl.glVertexAttribPointer(1, 2, gl.GL_FLOAT, gl.GL_FALSE, @sizeOf(Vertex), @ptrFromInt(@sizeOf([2]f32)));

    self.logical_w = config.width;
    self.logical_h = config.height;
    self.running = true;
    engine.attach(self.backend());
    return true;
}

fn vt_shutdown(ptr: *anyopaque) void {
    const self: *OpenglEngine = as_self(ptr);
    var it: SizeMap.KeyIterator = self.sizes.keyIterator();
    while (it.next()) |key| {
        var tex: gl.GLuint = @intCast(key.*);
        gl.glDeleteTextures(1, &tex);
    }
    self.sizes.deinit(self.allocator);
    if (self.program != 0) gl.glDeleteProgram(self.program);
    if (self.vbo != 0) gl.glDeleteBuffers(1, &self.vbo);
    if (self.vao != 0) gl.glDeleteVertexArrays(1, &self.vao);
    if (self.context != null) {
        _ = c.SDL_GL_DeleteContext(self.context);
        self.context = null;
    }
    if (self.window != null) {
        c.SDL_DestroyWindow(self.window);
        self.window = null;
    }
    if (self.assets_dir.len > 0) {
        self.allocator.free(self.assets_dir);
        self.assets_dir = &.{};
    }
    c.SDL_Quit();
    self.running = false;
    engine.detach();
}

fn vt_keeps_running(ptr: *anyopaque) bool {
    return as_self(ptr).running;
}

fn vt_request_stop(ptr: *anyopaque) void {
    as_self(ptr).running = false;
}

fn vt_present(ptr: *anyopaque) void {
    const self: *OpenglEngine = as_self(ptr);
    _ = c.SDL_GL_SwapWindow(self.window);
}

fn vt_poll_event(_: *anyopaque) ?engine.Event {
    var raw: c.SDL_Event = undefined;
    while (c.SDL_PollEvent(&raw) != 0) {
        switch (@as(c_int, @intCast(raw.type))) {
            c.SDL_QUIT => return engine.Event.quit,
            c.SDL_KEYDOWN => return engine.Event{ .key_down = .{
                .code = raw.key.keysym.sym,
                .key = .unknown,
                .name = cstr(c.SDL_GetKeyName(raw.key.keysym.sym)),
                .scan_name = cstr(c.SDL_GetScancodeName(raw.key.keysym.scancode)),
            } },
            c.SDL_KEYUP => return engine.Event{ .key_up = .{
                .code = raw.key.keysym.sym,
                .key = .unknown,
                .name = cstr(c.SDL_GetKeyName(raw.key.keysym.sym)),
                .scan_name = cstr(c.SDL_GetScancodeName(raw.key.keysym.scancode)),
            } },
            c.SDL_MOUSEMOTION => return engine.Event{ .mouse_motion = .{ .x = raw.motion.x, .y = raw.motion.y } },
            c.SDL_MOUSEBUTTONDOWN => return engine.Event{ .mouse_button_down = .{ .button = .unknown, .x = raw.button.x, .y = raw.button.y } },
            c.SDL_MOUSEBUTTONUP => return engine.Event{ .mouse_button_up = .{ .button = .unknown, .x = raw.button.x, .y = raw.button.y } },
            c.SDL_MOUSEWHEEL => return engine.Event{ .mouse_wheel = .{ .x = @floatFromInt(raw.wheel.x), .y = @floatFromInt(raw.wheel.y) } },
            else => {},
        }
    }
    return null;
}

fn vt_ticks_ms(_: *anyopaque) u64 {
    return c.SDL_GetTicks64();
}

fn vt_set_logical_size(ptr: *anyopaque, width: u32, height: u32) void {
    const self: *OpenglEngine = as_self(ptr);
    self.logical_w = width;
    self.logical_h = height;
}

fn vt_set_title(ptr: *anyopaque, title: []const u8) void {
    const self: *OpenglEngine = as_self(ptr);
    if (self.window == null) return;
    const z: [:0]u8 = self.allocator.dupeZ(u8, title) catch return;
    defer self.allocator.free(z);
    c.SDL_SetWindowTitle(self.window, z.ptr);
}

fn vt_set_fullscreen(ptr: *anyopaque, on: bool) void {
    const self: *OpenglEngine = as_self(ptr);
    _ = c.SDL_SetWindowFullscreen(self.window, if (on) c.SDL_WINDOW_FULLSCREEN_DESKTOP else 0);
}

fn vt_set_vsync(ptr: *anyopaque, on: bool) void {
    _ = ptr;
    _ = c.SDL_GL_SetSwapInterval(if (on) 1 else 0);
}

fn vt_set_resolution(ptr: *anyopaque, width: u32, height: u32) void {
    const self: *OpenglEngine = as_self(ptr);
    _ = c.SDL_SetWindowSize(self.window, @intCast(width), @intCast(height));
    self.logical_w = width;
    self.logical_h = height;
}

fn vt_logical_size(ptr: *anyopaque) engine.Point {
    const self: *OpenglEngine = as_self(ptr);
    return .{ .x = @intCast(self.logical_w), .y = @intCast(self.logical_h) };
}

fn vt_display_modes(_: *anyopaque, _: Allocator) []engine.DisplayMode {
    return &.{};
}

fn vt_supports_curved_panorama(_: *anyopaque) bool {
    return false;
}

fn vt_supports_offscreen_targets(_: *anyopaque) bool {
    return false;
}

fn vt_set_draw_offset(_: *anyopaque, _: i32, _: i32) void {}

fn vt_clear(_: *anyopaque, _: engine.Color) void {}

fn vt_create_texture(ptr: *anyopaque, width: u32, height: u32, _: ?[]const u8, _: u32) ?engine.TextureHandle {
    const self: *OpenglEngine = as_self(ptr);
    var tex: gl.GLuint = 0;
    gl.glGenTextures(1, &tex);
    if (tex == 0) return null;
    gl.glBindTexture(gl.GL_TEXTURE_2D, tex);
    gl.glTexParameteri(gl.GL_TEXTURE_2D, gl.GL_TEXTURE_MIN_FILTER, gl.GL_NEAREST);
    gl.glTexParameteri(gl.GL_TEXTURE_2D, gl.GL_TEXTURE_MAG_FILTER, gl.GL_NEAREST);
    gl.glTexParameteri(gl.GL_TEXTURE_2D, gl.GL_TEXTURE_WRAP_S, gl.GL_CLAMP_TO_EDGE);
    gl.glTexParameteri(gl.GL_TEXTURE_2D, gl.GL_TEXTURE_WRAP_T, gl.GL_CLAMP_TO_EDGE);
    gl.glTexImage2D(gl.GL_TEXTURE_2D, 0, gl.GL_RGBA8, @intCast(width), @intCast(height), 0, gl.GL_BGRA, gl.GL_UNSIGNED_BYTE, null);
    self.sizes.put(self.allocator, tex, .{ width, height }) catch {};
    return .{ .id = tex };
}

fn vt_update_texture(ptr: *anyopaque, tex: engine.TextureHandle, pixels: []const u8, pitch: u32) void {
    const self: *OpenglEngine = as_self(ptr);
    _ = pitch;
    const size = self.sizes.get(tex.id) orelse return;
    gl.glBindTexture(gl.GL_TEXTURE_2D, glTextureFor(tex));
    gl.glPixelStorei(gl.GL_UNPACK_ALIGNMENT, 1);
    gl.glTexSubImage2D(gl.GL_TEXTURE_2D, 0, 0, 0, @intCast(size[0]), @intCast(size[1]), gl.GL_BGRA, gl.GL_UNSIGNED_BYTE, pixels.ptr);
}

fn vt_draw_texture(ptr: *anyopaque, tex: engine.TextureHandle, dst: engine.Rect, _: ?engine.Rect, alpha: ?u8) void {
    const self: *OpenglEngine = as_self(ptr);
    if (self.logical_w == 0 or self.logical_h == 0) return;
    const lw: f32 = @floatFromInt(self.logical_w);
    const lh: f32 = @floatFromInt(self.logical_h);
    const x0: f32 = 2.0 * @as(f32, @floatFromInt(dst.x)) / lw - 1.0;
    const x1: f32 = 2.0 * @as(f32, @floatFromInt(dst.x + dst.w)) / lw - 1.0;
    const y_top: f32 = 1.0 - 2.0 * @as(f32, @floatFromInt(dst.y)) / lh;
    const y_bottom: f32 = 1.0 - 2.0 * @as(f32, @floatFromInt(dst.y + dst.h)) / lh;

    gl.glUseProgram(self.program);
    gl.glUniform4f(self.u_rect, x0, y_bottom, x1, y_top);
    gl.glUniform1f(self.u_alpha, if (alpha) |a| @as(f32, @floatFromInt(a)) / 255.0 else 1.0);
    gl.glActiveTexture(gl.GL_TEXTURE0);
    gl.glBindTexture(gl.GL_TEXTURE_2D, glTextureFor(tex));
    gl.glUniform1i(gl.glGetUniformLocation(self.program, "u_tex"), 0);
    gl.glBindVertexArray(self.vao);
    gl.glDrawArrays(gl.GL_TRIANGLE_STRIP, 0, 4);
}

fn vt_draw_texture_rotated(ptr: *anyopaque, tex: engine.TextureHandle, dst: engine.Rect, _: f32, alpha: ?u8) void {
    vt_draw_texture(ptr, tex, dst, null, alpha);
}

fn vt_texture_size(ptr: *anyopaque, tex: engine.TextureHandle) engine.Point {
    const self: *OpenglEngine = as_self(ptr);
    const size = self.sizes.get(tex.id) orelse return .{};
    return .{ .x = @intCast(size[0]), .y = @intCast(size[1]) };
}

// ── Unsupported by this backend (the pygame path does not use them) ──────────

fn s_rect(_: *anyopaque, _: engine.Rect, _: engine.Color, _: bool) void {}
fn s_line(_: *anyopaque, _: i32, _: i32, _: i32, _: i32, _: engine.Color) void {}
fn s_circle(_: *anyopaque, _: i32, _: i32, _: i32, _: engine.Color, _: bool) void {}
fn s_void(_: *anyopaque) void {}
fn s_i32(_: *anyopaque, _: i32) void {}
fn s_delfile(_: *anyopaque, _: []const u8, _: []const u8) void {}
fn s_discord(_: *anyopaque, _: []const u8, _: []const u8) void {}
fn vt_false(_: *anyopaque, _: []const u8, _: []const u8) bool {
    return false;
}
fn vt_load_texture(_: *anyopaque, _: []const u8) ?engine.TextureHandle {
    return null;
}
fn vt_create_target(_: *anyopaque, _: u32, _: u32) ?engine.TextureHandle {
    return null;
}
fn vt_geometry(_: *anyopaque, _: engine.TextureHandle, _: []const engine.Vertex, _: []const i32) void {}
fn vt_set_render_target(_: *anyopaque, _: ?engine.TextureHandle) void {}
fn vt_load_font(_: *anyopaque, _: []const u8, _: u16) i64 {
    return -1;
}
fn vt_draw_text(_: *anyopaque, _: []const u8, _: i32, _: i32, _: u32, _: engine.Color, _: bool, _: i64) bool {
    return false;
}
fn vt_draw_text_rotated(_: *anyopaque, _: []const u8, _: i32, _: i32, _: u32, _: f32, _: engine.Color, _: bool, _: i64) bool {
    return false;
}
fn vt_text_size(_: *anyopaque, _: []const u8, _: u32) ?engine.Point {
    return null;
}
fn vt_load_sound(_: *anyopaque, _: []const u8) ?engine.SoundHandle {
    return null;
}
fn vt_play_sound(_: *anyopaque, _: engine.SoundHandle, _: i32, _: i32) i32 {
    return -1;
}
fn vt_read_file(_: *anyopaque, _: Allocator, _: []const u8, _: []const u8, _: usize) ?[]u8 {
    return null;
}
fn vt_write_file(_: *anyopaque, _: []const u8, _: []const u8, _: []const u8) bool {
    return false;
}
fn vt_mouse_pos(_: *anyopaque) engine.Point {
    var x: c_int = 0;
    var y: c_int = 0;
    _ = c.SDL_GetMouseState(&x, &y);
    return .{ .x = x, .y = y };
}
