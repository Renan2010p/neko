// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! The **shared SDL2 platform layer** used by every SDL2 backend.
//!
//! The three desktop backends (`sdl2`, `sdl2-opengl`, `sdl2-vulkan`) differ
//! only in how they present a frame. Everything else — bringing up SDL, opening
//! the window, polling and translating events, timing, window properties,
//! display modes, the mouse and file access — is identical, so it lives here
//! once.
//!
//! A backend embeds a `Sdl` instance in a field (conventionally named `sdl`)
//! and wires the shared vtable entries with `adapter(Engine, "sdl")`, which
//! casts the opaque `Backend.ptr` back to the owning engine. The backend keeps
//! only the calls that are genuinely platform-specific (init, shutdown,
//! present, vsync, capability queries).

const std: type = @import("std");
const c: type = @import("c");
const engine: type = @import("neko");
const keys: type = @import("platform/keys.zig");
const discord_mod: type = @import("discord.zig");

const Allocator: type = std.mem.Allocator;
const mem: type = std.mem;
const DisplayModeList: type = std.ArrayListUnmanaged(engine.DisplayMode);

/// The shared, presenter-independent SDL2 state.
pub const Sdl: type = struct {
    allocator: Allocator = undefined,
    io: ?std.Io = null,
    assets_dir: []u8 = &.{},
    window: ?*c.SDL_Window = null,
    logical_w: u32 = 0,
    logical_h: u32 = 0,
    running: bool = false,
    vsync: bool = false,
    /// Discord Rich Presence (best effort).
    discord: discord_mod.Client = .{},

    /// Stores the bootstrap data and brings up SDL with `init_flags`.
    ///
    /// Window creation is split out so a GL backend can set its context
    /// attributes between the two calls.
    pub fn begin(self: *Sdl, config: engine.Config, init_flags: c.Uint32) bool {
        self.allocator = config.allocator;
        self.io = config.io;
        self.assets_dir = self.allocator.dupe(u8, config.assets_dir) catch return false;
        if (c.SDL_Init(init_flags) != 0) return false;
        return true;
    }

    /// Creates the window with `window_flags`, applies fullscreen and records
    /// the logical size and the running flag.
    pub fn openWindow(self: *Sdl, config: engine.Config, window_flags: c.Uint32) bool {
        const title_z: [:0]u8 = self.allocator.dupeSentinel(u8, config.title, 0) catch return false;
        defer self.allocator.free(title_z);

        self.window = c.SDL_CreateWindow(
            title_z.ptr,
            c.SDL_WINDOWPOS_CENTERED,
            c.SDL_WINDOWPOS_CENTERED,
            @intCast(config.width),
            @intCast(config.height),
            window_flags,
        );
        if (self.window == null) return false;

        if (config.fullscreen) {
            _ = c.SDL_SetWindowFullscreen(self.window, c.SDL_WINDOW_FULLSCREEN_DESKTOP);
        }

        self.logical_w = config.width;
        self.logical_h = config.height;
        self.vsync = config.vsync;
        self.running = true;
        return true;
    }

    /// Destroys the window and releases the copied assets directory. Does not
    /// call `SDL_Quit`; the backend quits its own subsystems first.
    pub fn closeWindow(self: *Sdl) void {
        self.discord.close();
        if (self.window != null) {
            c.SDL_DestroyWindow(self.window);
            self.window = null;
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
        c.SDL_SetWindowTitle(self.window, z.ptr);
    }

    pub fn setLogicalSize(self: *Sdl, width: u32, height: u32) void {
        self.logical_w = width;
        self.logical_h = height;
    }

    pub fn setFullscreen(self: *Sdl, on: bool) void {
        if (self.window == null) return;
        _ = c.SDL_SetWindowFullscreen(self.window, if (on) c.SDL_WINDOW_FULLSCREEN_DESKTOP else 0);
    }

    /// Resizes the real window and follows it with the logical size.
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
        return c.SDL_GetTicks64();
    }

    pub fn mousePos(self: *Sdl) engine.Point {
        _ = self;
        var x: c_int = 0;
        var y: c_int = 0;
        _ = c.SDL_GetMouseState(&x, &y);
        return engine.Point{ .x = x, .y = y };
    }

    // ── Displays ─────────────────────────────────────────────────────────

    pub fn displayModes(self: *Sdl, allocator: Allocator) []engine.DisplayMode {
        _ = self;
        var list: DisplayModeList = .empty;
        const displays: c_int = c.SDL_GetNumVideoDisplays();
        var d: c_int = 0;
        while (d < displays) : (d += 1) {
            const count: c_int = c.SDL_GetNumDisplayModes(d);
            var i: c_int = 0;
            while (i < count) : (i += 1) {
                var dm: c.SDL_DisplayMode = undefined;
                if (c.SDL_GetDisplayMode(d, i, &dm) == 0) {
                    list.append(allocator, engine.DisplayMode{
                        .width = dm.w,
                        .height = dm.h,
                        .refresh_hz = dm.refresh_rate,
                    }) catch break;
                }
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
    switch (@as(c_int, @intCast(raw.type))) {
        c.SDL_QUIT => return engine.Event.quit,
        c.SDL_KEYDOWN => return engine.Event{ .key_down = .{
            .code = raw.key.keysym.sym,
            .key = keys.mapKey(raw.key.keysym.sym),
            .name = cstr(c.SDL_GetKeyName(raw.key.keysym.sym)),
            .scan_name = cstr(c.SDL_GetScancodeName(raw.key.keysym.scancode)),
        } },
        c.SDL_KEYUP => return engine.Event{ .key_up = .{
            .code = raw.key.keysym.sym,
            .key = keys.mapKey(raw.key.keysym.sym),
            .name = cstr(c.SDL_GetKeyName(raw.key.keysym.sym)),
            .scan_name = cstr(c.SDL_GetScancodeName(raw.key.keysym.scancode)),
        } },
        c.SDL_MOUSEBUTTONDOWN => return engine.Event{ .mouse_button_down = .{
            .button = keys.mapButton(raw.button.button),
            .x = raw.button.x,
            .y = raw.button.y,
        } },
        c.SDL_MOUSEBUTTONUP => return engine.Event{ .mouse_button_up = .{
            .button = keys.mapButton(raw.button.button),
            .x = raw.button.x,
            .y = raw.button.y,
        } },
        c.SDL_MOUSEMOTION => return engine.Event{ .mouse_motion = .{ .x = raw.motion.x, .y = raw.motion.y } },
        c.SDL_MOUSEWHEEL => return engine.Event{ .mouse_wheel = .{
            .x = @floatFromInt(raw.wheel.x),
            .y = @floatFromInt(raw.wheel.y),
        } },
        else => {},
    }
    return null;
}

/// Drains SDL's queue until one forwardable event is found.
pub fn pollEvent() ?engine.Event {
    var raw: c.SDL_Event = undefined;
    while (c.SDL_PollEvent(&raw) != 0) {
        if (translateEvent(raw)) |event| return event;
    }
    return null;
}

/// Builds a vtable adapter for the shared entries, reading the `Sdl` value
/// stored in `@field(Owner, field_name)`.
///
/// The owner is the concrete backend engine (its address is `Backend.ptr`),
/// so `neko_backend` keeps a single opaque pointer while still sharing this
/// whole layer.
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
