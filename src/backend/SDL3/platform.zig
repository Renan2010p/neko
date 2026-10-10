// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! The **shared SDL3 platform layer** used by the SDL3 backend.
//!
//! Everything presenter-independent — bringing up SDL, opening the window,
//! polling and translating events, timing, window properties, display modes,
//! the mouse and file access — lives here once.

const std: type = @import("std");
const c: type = @import("c");
const engine: type = @import("neko");
const keys: type = @import("platform/keys.zig");
const discord_mod: type = @import("discord.zig");

const Allocator: type = std.mem.Allocator;
const mem: type = std.mem;
const DisplayModeList: type = std.ArrayListUnmanaged(engine.DisplayMode);

/// The shared, presenter-independent SDL state.
pub const Sdl: type = struct {
    allocator: Allocator = undefined,
    io: ?std.Io = null,
    assets_dir: []u8 = &.{},
    window: ?*c.SDL_Window = null,
    window_flags: c.SDL_WindowFlags = 0,
    fullscreen: bool = false,
    /// Keeps the NUL-terminated window title alive for the window's lifetime
    /// (some SDL backends keep the pointer rather than copying it).
    title_alloc: []u8 = &.{},
    title_frames: u32 = 0,
    logical_w: u32 = 0,
    logical_h: u32 = 0,
    running: bool = false,
    vsync: bool = false,
    /// Discord Rich Presence (best effort).
    discord: discord_mod.Client = .{},

    pub fn begin(self: *Sdl, config: engine.Config, init_flags: c.SDL_InitFlags) bool {
        self.allocator = config.allocator;
        self.io = config.io;
        self.assets_dir = self.allocator.dupe(u8, config.assets_dir) catch return false;
        // Avoid the libdecor → GTK → Pango path on Wayland: its client-side
        // decorations crash on the title string here (invalid UTF-8 warnings
        // and a garbled title). With libdecor disabled SDL uses the
        // compositor's/server decorations instead.
        _ = c.SDL_SetHint("SDL_VIDEO_WAYLAND_ALLOW_LIBDECOR", "0");
        if (!c.SDL_Init(init_flags)) return false;
        return true;
    }

    pub fn openWindow(self: *Sdl, config: engine.Config, window_flags: c.SDL_WindowFlags) bool {
        if (self.title_alloc.len > 0) {
            self.allocator.free(self.title_alloc);
            self.title_alloc = &.{};
        }
        const title_z: [:0]u8 = self.allocator.dupeSentinel(u8, config.title, 0) catch return false;
        self.title_alloc = title_z;

        self.window = c.SDL_CreateWindow(
            title_z.ptr,
            @intCast(config.width),
            @intCast(config.height),
            window_flags,
        );
        if (self.window == null) return false;
        self.title_frames = 0;

        self.window_flags = window_flags;
        self.fullscreen = config.fullscreen;
        if (config.fullscreen) {
            _ = c.SDL_SetWindowFullscreen(self.window, true);
        }

        self.logical_w = config.width;
        self.logical_h = config.height;
        self.vsync = config.vsync;
        self.running = true;
        return true;
    }

    pub fn closeWindow(self: *Sdl) void {
        self.discord.close();
        if (self.window != null) {
            c.SDL_DestroyWindow(self.window);
            self.window = null;
        }
        if (self.title_alloc.len > 0) {
            self.allocator.free(self.title_alloc);
            self.title_alloc = &.{};
        }
        if (self.assets_dir.len > 0) {
            self.allocator.free(self.assets_dir);
            self.assets_dir = &.{};
        }
        self.running = false;
    }

    // ── Window ───────────────────────────────────────────────────────────

    pub fn setTitle(self: *Sdl, title: []const u8) void {
        if (self.window == null) return;
        const z: [:0]u8 = self.allocator.dupeSentinel(u8, title, 0) catch return;
        defer self.allocator.free(z);
        _ = c.SDL_SetWindowTitle(self.window, z.ptr);
    }

    /// Re-applies the window title a few frames after the window is mapped.
    /// SDL3's Wayland backend corrupts a title set before the surface is shown,
    /// so we set it again once the window is on screen.
    pub fn tickTitle(self: *Sdl) void {
        if (self.window == null or self.title_alloc.len == 0) return;
        self.title_frames +%= 1;
        if (self.title_frames == 2 or self.title_frames == 12) {
            _ = c.SDL_SetWindowTitle(self.window, @ptrCast(self.title_alloc.ptr));
        }
    }

    pub fn setLogicalSize(self: *Sdl, width: u32, height: u32) void {
        self.logical_w = width;
        self.logical_h = height;
    }

    pub fn setFullscreen(self: *Sdl, on: bool) void {
        if (self.window == null) return;
        self.fullscreen = on;
        _ = c.SDL_SetWindowFullscreen(self.window, on);
    }

    /// Recreates the window with `flags`, keeping title, size and fullscreen.
    /// Some renderers (Vulkan) need a window flag set *before* they can attach.
    pub fn recreateWindow(self: *Sdl, flags: c.SDL_WindowFlags) bool {
        if (self.window == null) return false;
        if (flags == self.window_flags) return true;

        c.SDL_DestroyWindow(self.window);
        const title_ptr: [*:0]const u8 = if (self.title_alloc.len > 0) @ptrCast(self.title_alloc.ptr) else "";
        self.window = c.SDL_CreateWindow(title_ptr, @intCast(self.logical_w), @intCast(self.logical_h), flags);
        if (self.window == null) return false;
        self.window_flags = flags;
        // SDL3/Wayland corrupts a title set before the new window is mapped, so
        // restart the deferred re-apply for the fresh window.
        self.title_frames = 0;
        if (self.fullscreen) {
            _ = c.SDL_SetWindowFullscreen(self.window, true);
        }
        return true;
    }

    pub fn setResolution(self: *Sdl, width: u32, height: u32) void {
        if (self.window != null) {
            _ = c.SDL_SetWindowSize(self.window, @intCast(width), @intCast(height));
        }
        self.logical_w = width;
        self.logical_h = height;
    }

    pub fn logicalSize(self: *Sdl) engine.Point {
        return engine.Point{ .x = @intCast(self.logical_w), .y = @intCast(self.logical_h) };
    }

    pub fn setDrawOffset(self: *Sdl, dx: i32, dy: i32) void {
        _ = self;
        _ = dx;
        _ = dy;
    }

    // ── Timing and input ─────────────────────────────────────────────────

    pub fn ticksMs(self: *Sdl) u64 {
        _ = self;
        return c.SDL_GetTicks();
    }

    pub fn mousePos(self: *Sdl) engine.Point {
        _ = self;
        var x: f32 = 0;
        var y: f32 = 0;
        _ = c.SDL_GetMouseState(&x, &y);
        return engine.Point{ .x = @intFromFloat(x), .y = @intFromFloat(y) };
    }

    // ── Displays ─────────────────────────────────────────────────────────

    pub fn displayModes(self: *Sdl, allocator: Allocator) []engine.DisplayMode {
        _ = self;
        var list: DisplayModeList = .empty;
        var nd: c_int = 0;
        const displays: [*c]c.SDL_DisplayID = c.SDL_GetDisplays(&nd);
        if (displays == null) return &.{};
        defer c.SDL_free(displays);

        var d: c_int = 0;
        while (d < nd) : (d += 1) {
            var nm: c_int = 0;
            const modes: [*c][*c]c.SDL_DisplayMode = c.SDL_GetFullscreenDisplayModes(displays[@intCast(d)], &nm);
            if (modes == null) continue;
            defer c.SDL_free(@ptrCast(modes));

            var i: c_int = 0;
            while (i < nm) : (i += 1) {
                const dm: [*c]c.SDL_DisplayMode = modes[@intCast(i)];
                if (dm == null) continue;
                list.append(allocator, engine.DisplayMode{
                    .width = dm.*.w,
                    .height = dm.*.h,
                    .refresh_hz = @intFromFloat(dm.*.refresh_rate),
                }) catch break;
            }
        }
        return list.toOwnedSlice(allocator) catch &.{};
    }

    // ── Files ────────────────────────────────────────────────────────────

    pub fn readFile(self: *Sdl, allocator: Allocator, dir_path: []const u8, file_name: []const u8, max: usize) ?[]u8 {
        const io: std.Io = self.io orelse return null;
        const dir: std.Io.Dir = std.Io.Dir.cwd().createDirPathOpen(io, dir_path, .{}) catch return null;
        return dir.readFileAlloc(io, file_name, allocator, .limited(max)) catch null;
    }

    pub fn writeFile(self: *Sdl, dir_path: []const u8, file_name: []const u8, data: []const u8) bool {
        const io: std.Io = self.io orelse return false;
        const dir: std.Io.Dir = std.Io.Dir.cwd().createDirPathOpen(io, dir_path, .{}) catch return false;
        dir.writeFile(io, .{ .sub_path = file_name, .data = data }) catch return false;
        return true;
    }

    pub fn deleteFile(self: *Sdl, dir_path: []const u8, file_name: []const u8) void {
        const io: std.Io = self.io orelse return;
        const dir: std.Io.Dir = std.Io.Dir.cwd().createDirPathOpen(io, dir_path, .{}) catch return;
        dir.deleteFile(io, file_name) catch {};
    }

    pub fn fileExists(self: *Sdl, dir_path: []const u8, file_name: []const u8) bool {
        const io: std.Io = self.io orelse return false;
        const dir: std.Io.Dir = std.Io.Dir.cwd().createDirPathOpen(io, dir_path, .{}) catch return false;
        const file: std.Io.File = dir.openFile(io, file_name, .{}) catch return false;
        file.close(io);
        return true;
    }

    // ── Discord Rich Presence ────────────────────────────────────────────

    pub fn discordConnect(self: *Sdl, client_id: []const u8) bool {
        return self.discord.connect(self.io, client_id);
    }

    pub fn discordSet(self: *Sdl, presence: engine.DiscordPresence) bool {
        return self.discord.set(presence);
    }

    pub fn discordClear(self: *Sdl) void {
        self.discord.clear();
    }

    pub fn discordClose(self: *Sdl) void {
        self.discord.close();
    }

    pub fn discordConnected(self: *Sdl) bool {
        return self.discord.isConnected();
    }
};

/// Views a dynamic C string returned by SDL as a Zig slice.
pub fn cstr(ptr: [*c]const u8) []const u8 {
    if (ptr == null) return "";
    const sentinel: [*:0]const u8 = @ptrCast(ptr);
    return mem.span(sentinel);
}

/// Translates one raw SDL event; `null` when it is not one we forward.
pub fn translateEvent(raw: c.SDL_Event) ?engine.Event {
    switch (raw.type) {
        c.SDL_EVENT_QUIT => return engine.Event.quit,
        c.SDL_EVENT_KEY_DOWN => return engine.Event{ .key_down = .{
            .code = @intCast(raw.key.key),
            .key = keys.mapKey(raw.key.key),
            .name = cstr(c.SDL_GetKeyName(raw.key.key)),
            .scan_name = cstr(c.SDL_GetScancodeName(raw.key.scancode)),
        } },
        c.SDL_EVENT_KEY_UP => return engine.Event{ .key_up = .{
            .code = @intCast(raw.key.key),
            .key = keys.mapKey(raw.key.key),
            .name = cstr(c.SDL_GetKeyName(raw.key.key)),
            .scan_name = cstr(c.SDL_GetScancodeName(raw.key.scancode)),
        } },
        c.SDL_EVENT_MOUSE_BUTTON_DOWN => return engine.Event{ .mouse_button_down = .{
            .button = keys.mapButton(raw.button.button),
            .x = @intFromFloat(raw.button.x),
            .y = @intFromFloat(raw.button.y),
        } },
        c.SDL_EVENT_MOUSE_BUTTON_UP => return engine.Event{ .mouse_button_up = .{
            .button = keys.mapButton(raw.button.button),
            .x = @intFromFloat(raw.button.x),
            .y = @intFromFloat(raw.button.y),
        } },
        c.SDL_EVENT_MOUSE_MOTION => return engine.Event{ .mouse_motion = .{
            .x = @intFromFloat(raw.motion.x),
            .y = @intFromFloat(raw.motion.y),
        } },
        c.SDL_EVENT_MOUSE_WHEEL => return engine.Event{ .mouse_wheel = .{
            .x = raw.wheel.x,
            .y = raw.wheel.y,
        } },
        else => {},
    }
    return null;
}

/// Drains SDL's queue until one forwardable event is found.
pub fn pollEvent() ?engine.Event {
    var raw: c.SDL_Event = undefined;
    while (c.SDL_PollEvent(&raw)) {
        if (translateEvent(raw)) |event| return event;
    }
    return null;
}

/// Builds a vtable adapter for the shared entries, reading the `Sdl` value
/// stored in `@field(Owner, field_name)`.
pub fn adapter(comptime Owner: type, comptime field_name: []const u8) type {
    return struct {
        fn sdl(ptr: *anyopaque) *Sdl {
            const owner: *Owner = @ptrCast(@alignCast(ptr));
            return &@field(owner, field_name);
        }

        pub fn keeps_running(ptr: *anyopaque) bool {
            return sdl(ptr).running;
        }

        pub fn request_stop(ptr: *anyopaque) void {
            sdl(ptr).running = false;
        }

        pub fn ticks_ms(ptr: *anyopaque) u64 {
            return sdl(ptr).ticksMs();
        }

        pub fn poll_event(ptr: *anyopaque) ?engine.Event {
            _ = ptr;
            return pollEvent();
        }

        pub fn set_title(ptr: *anyopaque, title: []const u8) void {
            sdl(ptr).setTitle(title);
        }

        pub fn set_logical_size(ptr: *anyopaque, width: u32, height: u32) void {
            sdl(ptr).setLogicalSize(width, height);
        }

        pub fn set_fullscreen(ptr: *anyopaque, on: bool) void {
            sdl(ptr).setFullscreen(on);
        }

        pub fn set_resolution(ptr: *anyopaque, width: u32, height: u32) void {
            sdl(ptr).setResolution(width, height);
        }

        pub fn logical_size(ptr: *anyopaque) engine.Point {
            return sdl(ptr).logicalSize();
        }

        pub fn display_modes(ptr: *anyopaque, allocator: Allocator) []engine.DisplayMode {
            return sdl(ptr).displayModes(allocator);
        }

        pub fn set_draw_offset(ptr: *anyopaque, dx: i32, dy: i32) void {
            sdl(ptr).setDrawOffset(dx, dy);
        }

        pub fn mouse_pos(ptr: *anyopaque) engine.Point {
            return sdl(ptr).mousePos();
        }

        pub fn read_file(ptr: *anyopaque, allocator: Allocator, dir_path: []const u8, file_name: []const u8, max: usize) ?[]u8 {
            return sdl(ptr).readFile(allocator, dir_path, file_name, max);
        }

        pub fn write_file(ptr: *anyopaque, dir_path: []const u8, file_name: []const u8, data: []const u8) bool {
            return sdl(ptr).writeFile(dir_path, file_name, data);
        }

        pub fn delete_file(ptr: *anyopaque, dir_path: []const u8, file_name: []const u8) void {
            sdl(ptr).deleteFile(dir_path, file_name);
        }

        pub fn file_exists(ptr: *anyopaque, dir_path: []const u8, file_name: []const u8) bool {
            return sdl(ptr).fileExists(dir_path, file_name);
        }

        pub fn discord_connect(ptr: *anyopaque, client_id: []const u8) bool {
            return sdl(ptr).discordConnect(client_id);
        }

        pub fn discord_set(ptr: *anyopaque, presence: engine.DiscordPresence) bool {
            return sdl(ptr).discordSet(presence);
        }

        pub fn discord_clear(ptr: *anyopaque) void {
            sdl(ptr).discordClear();
        }

        pub fn discord_close(ptr: *anyopaque) void {
            sdl(ptr).discordClose();
        }

        pub fn discord_connected(ptr: *anyopaque) bool {
            return sdl(ptr).discordConnected();
        }
    };
}
