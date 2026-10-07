//! The low-level backend dispatch table.
//!
//! Every platform backend (SDL2 today; PS2 later) fills in a `VTable` and
//! hands back a `Backend` handle. The public `engine.*` namespaces call the
//! wrapper methods below, which forward to the backend through `ptr`.
//!
//! Game code never touches this type directly: it uses `engine.draw`,
//! `engine.text`, and friends.

const std: type = @import("std");
const Allocator: type = std.mem.Allocator;
const types: type = @import("types.zig");
const event: type = @import("../system/event.zig");

/// A type-erased handle to a platform backend.
pub const Backend: type = struct {
    ptr: *anyopaque,
    vtable: *const VTable,

    /// The backend function table. Every backend must provide all of these.
    pub const VTable: type = struct {
        // ── Lifecycle ────────────────────────────────────────────────────
        init: *const fn (ptr: *anyopaque, config: types.Config) bool,
        shutdown: *const fn (ptr: *anyopaque) void,
        keeps_running: *const fn (ptr: *anyopaque) bool,
        request_stop: *const fn (ptr: *anyopaque) void,
        present: *const fn (ptr: *anyopaque) void,

        // ── Events and timing ────────────────────────────────────────────
        poll_event: *const fn (ptr: *anyopaque) ?event.Event,
        ticks_ms: *const fn (ptr: *anyopaque) u64,

        // ── Window ───────────────────────────────────────────────────────
        set_title: *const fn (ptr: *anyopaque, title: []const u8) void,
        set_logical_size: *const fn (ptr: *anyopaque, width: u32, height: u32) void,
        set_fullscreen: *const fn (ptr: *anyopaque, on: bool) void,
        set_vsync: *const fn (ptr: *anyopaque, on: bool) void,
        set_resolution: *const fn (ptr: *anyopaque, width: u32, height: u32) void,
        logical_size: *const fn (ptr: *anyopaque) types.Point,
        display_modes: *const fn (ptr: *anyopaque, allocator: Allocator) []types.DisplayMode,
        supports_curved_panorama: *const fn (ptr: *anyopaque) bool,
        supports_offscreen_targets: *const fn (ptr: *anyopaque) bool,
        set_draw_offset: *const fn (ptr: *anyopaque, dx: i32, dy: i32) void,

        // ── Drawing primitives ───────────────────────────────────────────
        clear: *const fn (ptr: *anyopaque, color: types.Color) void,
        draw_rect: *const fn (ptr: *anyopaque, rect: types.Rect, color: types.Color, filled: bool) void,
        draw_line: *const fn (ptr: *anyopaque, x1: i32, y1: i32, x2: i32, y2: i32, color: types.Color) void,
        draw_circle: *const fn (ptr: *anyopaque, cx: i32, cy: i32, radius: i32, color: types.Color, filled: bool) void,

        // ── Textures and render targets ──────────────────────────────────
        load_texture: *const fn (ptr: *anyopaque, path: []const u8) ?types.TextureHandle,
        create_target: *const fn (ptr: *anyopaque, width: u32, height: u32) ?types.TextureHandle,
        create_texture: *const fn (ptr: *anyopaque, width: u32, height: u32, pixels: ?[]const u8, pitch: u32) ?types.TextureHandle,
        update_texture: *const fn (ptr: *anyopaque, tex: types.TextureHandle, pixels: []const u8, pitch: u32) void,
        draw_texture: *const fn (ptr: *anyopaque, tex: types.TextureHandle, dst: types.Rect, src: ?types.Rect, alpha: ?u8) void,
        draw_texture_rotated: *const fn (ptr: *anyopaque, tex: types.TextureHandle, dst: types.Rect, angle: f32, alpha: ?u8) void,
        texture_size: *const fn (ptr: *anyopaque, tex: types.TextureHandle) types.Point,
        geometry: *const fn (ptr: *anyopaque, tex: types.TextureHandle, vertices: []const types.Vertex, indices: []const i32) void,
        set_render_target: *const fn (ptr: *anyopaque, target: ?types.TextureHandle) void,

        // ── Text ─────────────────────────────────────────────────────────
        load_font: *const fn (ptr: *anyopaque, path: []const u8, size: u16) i64,
        draw_text: *const fn (ptr: *anyopaque, text: []const u8, x: i32, y: i32, size: u32, color: types.Color, center: bool, font_idx: i64) bool,
        draw_text_rotated: *const fn (ptr: *anyopaque, text: []const u8, x: i32, y: i32, size: u32, angle: f32, color: types.Color, center: bool, font_idx: i64) bool,
        text_size: *const fn (ptr: *anyopaque, text: []const u8, font_idx: u32) ?types.Point,

        // ── Sound ────────────────────────────────────────────────────────
        load_sound: *const fn (ptr: *anyopaque, path: []const u8) ?types.SoundHandle,
        play_sound: *const fn (ptr: *anyopaque, snd: types.SoundHandle, loops: i32, channel: i32) i32,
        stop_channel: *const fn (ptr: *anyopaque, channel: i32) void,
        stop_all_sounds: *const fn (ptr: *anyopaque) void,
        set_master_volume: *const fn (ptr: *anyopaque, vol: i32) void,
        set_sfx_volume: *const fn (ptr: *anyopaque, vol: i32) void,
        set_music_volume: *const fn (ptr: *anyopaque, vol: i32) void,

        // ── Files ────────────────────────────────────────────────────────
        // The core never touches the host filesystem directly: save files and
        // any other file access go through the backend. Backends without a
        // filesystem (e.g. the PS2) return null/false and do nothing.
        read_file: *const fn (ptr: *anyopaque, allocator: Allocator, dir_path: []const u8, file_name: []const u8, max: usize) ?[]u8,
        write_file: *const fn (ptr: *anyopaque, dir_path: []const u8, file_name: []const u8, data: []const u8) bool,
        delete_file: *const fn (ptr: *anyopaque, dir_path: []const u8, file_name: []const u8) void,
        file_exists: *const fn (ptr: *anyopaque, dir_path: []const u8, file_name: []const u8) bool,

        // ── Input and misc ───────────────────────────────────────────────
        mouse_pos: *const fn (ptr: *anyopaque) types.Point,
        update_discord: *const fn (ptr: *anyopaque, details: []const u8, state: []const u8) void,
    };

    // ── Lifecycle ────────────────────────────────────────────────────────

    pub fn init(self: Backend, config: types.Config) bool {
        return self.vtable.init(self.ptr, config);
    }

    pub fn shutdown(self: Backend) void {
        self.vtable.shutdown(self.ptr);
    }

    pub fn keeps_running(self: Backend) bool {
        return self.vtable.keeps_running(self.ptr);
    }

    pub fn request_stop(self: Backend) void {
        self.vtable.request_stop(self.ptr);
    }

    pub fn present(self: Backend) void {
        self.vtable.present(self.ptr);
    }

    // ── Events and timing ────────────────────────────────────────────────

    pub fn poll_event(self: Backend) ?event.Event {
        return self.vtable.poll_event(self.ptr);
    }

    pub fn ticks_ms(self: Backend) u64 {
        return self.vtable.ticks_ms(self.ptr);
    }

    // ── Window ───────────────────────────────────────────────────────────

    pub fn set_title(self: Backend, title: []const u8) void {
        self.vtable.set_title(self.ptr, title);
    }

    pub fn set_logical_size(self: Backend, width: u32, height: u32) void {
        self.vtable.set_logical_size(self.ptr, width, height);
    }

    pub fn set_fullscreen(self: Backend, on: bool) void {
        self.vtable.set_fullscreen(self.ptr, on);
    }

    pub fn set_vsync(self: Backend, on: bool) void {
        self.vtable.set_vsync(self.ptr, on);
    }

    pub fn set_resolution(self: Backend, width: u32, height: u32) void {
        self.vtable.set_resolution(self.ptr, width, height);
    }

    pub fn logical_size(self: Backend) types.Point {
        return self.vtable.logical_size(self.ptr);
    }

    pub fn display_modes(self: Backend, allocator: Allocator) []types.DisplayMode {
        return self.vtable.display_modes(self.ptr, allocator);
    }

    pub fn supports_curved_panorama(self: Backend) bool {
        return self.vtable.supports_curved_panorama(self.ptr);
    }

    pub fn supports_offscreen_targets(self: Backend) bool {
        return self.vtable.supports_offscreen_targets(self.ptr);
    }

    pub fn set_draw_offset(self: Backend, dx: i32, dy: i32) void {
        self.vtable.set_draw_offset(self.ptr, dx, dy);
    }

    // ── Drawing primitives ───────────────────────────────────────────────

    pub fn clear(self: Backend, color: types.Color) void {
        self.vtable.clear(self.ptr, color);
    }

    pub fn draw_rect(self: Backend, rect: types.Rect, color: types.Color, filled: bool) void {
        self.vtable.draw_rect(self.ptr, rect, color, filled);
    }

    pub fn draw_line(self: Backend, x1: i32, y1: i32, x2: i32, y2: i32, color: types.Color) void {
        self.vtable.draw_line(self.ptr, x1, y1, x2, y2, color);
    }

    pub fn draw_circle(self: Backend, cx: i32, cy: i32, radius: i32, color: types.Color, filled: bool) void {
        self.vtable.draw_circle(self.ptr, cx, cy, radius, color, filled);
    }

    // ── Textures ─────────────────────────────────────────────────────────

    pub fn load_texture(self: Backend, path: []const u8) ?types.TextureHandle {
        return self.vtable.load_texture(self.ptr, path);
    }

    pub fn create_target(self: Backend, width: u32, height: u32) ?types.TextureHandle {
        return self.vtable.create_target(self.ptr, width, height);
    }

    /// Creates a CPU-writable streaming texture. `pixels` (when non-null) is an
    /// initial upload in the backend's 32-bit pixel format; `pitch` is the
    /// number of bytes per row.
    pub fn create_texture(self: Backend, width: u32, height: u32, pixels: ?[]const u8, pitch: u32) ?types.TextureHandle {
        return self.vtable.create_texture(self.ptr, width, height, pixels, pitch);
    }

    /// Re-uploads `pixels` into a texture created with `create_texture`.
    pub fn update_texture(self: Backend, tex: types.TextureHandle, pixels: []const u8, pitch: u32) void {
        self.vtable.update_texture(self.ptr, tex, pixels, pitch);
    }

    pub fn draw_texture(self: Backend, tex: types.TextureHandle, dst: types.Rect, src: ?types.Rect, alpha: ?u8) void {
        self.vtable.draw_texture(self.ptr, tex, dst, src, alpha);
    }

    pub fn draw_texture_rotated(self: Backend, tex: types.TextureHandle, dst: types.Rect, angle: f32, alpha: ?u8) void {
        self.vtable.draw_texture_rotated(self.ptr, tex, dst, angle, alpha);
    }

    pub fn texture_size(self: Backend, tex: types.TextureHandle) types.Point {
        return self.vtable.texture_size(self.ptr, tex);
    }

    pub fn geometry(self: Backend, tex: types.TextureHandle, vertices: []const types.Vertex, indices: []const i32) void {
        self.vtable.geometry(self.ptr, tex, vertices, indices);
    }

    pub fn set_render_target(self: Backend, target: ?types.TextureHandle) void {
        self.vtable.set_render_target(self.ptr, target);
    }

    // ── Text ─────────────────────────────────────────────────────────────

    pub fn load_font(self: Backend, path: []const u8, size: u16) i64 {
        return self.vtable.load_font(self.ptr, path, size);
    }

    pub fn draw_text(self: Backend, text: []const u8, x: i32, y: i32, size: u32, color: types.Color, center: bool, font_idx: i64) bool {
        return self.vtable.draw_text(self.ptr, text, x, y, size, color, center, font_idx);
    }

    pub fn draw_text_rotated(self: Backend, text: []const u8, x: i32, y: i32, size: u32, angle: f32, color: types.Color, center: bool, font_idx: i64) bool {
        return self.vtable.draw_text_rotated(self.ptr, text, x, y, size, angle, color, center, font_idx);
    }

    pub fn text_size(self: Backend, text: []const u8, font_idx: u32) ?types.Point {
        return self.vtable.text_size(self.ptr, text, font_idx);
    }

    // ── Sound ────────────────────────────────────────────────────────────

    pub fn load_sound(self: Backend, path: []const u8) ?types.SoundHandle {
        return self.vtable.load_sound(self.ptr, path);
    }

    pub fn play_sound(self: Backend, snd: types.SoundHandle, loops: i32, channel: i32) i32 {
        return self.vtable.play_sound(self.ptr, snd, loops, channel);
    }

    pub fn stop_channel(self: Backend, channel: i32) void {
        self.vtable.stop_channel(self.ptr, channel);
    }

    pub fn stop_all_sounds(self: Backend) void {
        self.vtable.stop_all_sounds(self.ptr);
    }

    pub fn set_master_volume(self: Backend, vol: i32) void {
        self.vtable.set_master_volume(self.ptr, vol);
    }

    pub fn set_sfx_volume(self: Backend, vol: i32) void {
        self.vtable.set_sfx_volume(self.ptr, vol);
    }

    pub fn set_music_volume(self: Backend, vol: i32) void {
        self.vtable.set_music_volume(self.ptr, vol);
    }

    // ── Files ────────────────────────────────────────────────────────────

    /// Reads `dir_path/file_name` into a fresh buffer, or null when the file
    /// does not exist / the backend has no filesystem.
    pub fn read_file(self: Backend, allocator: Allocator, dir_path: []const u8, file_name: []const u8, max: usize) ?[]u8 {
        return self.vtable.read_file(self.ptr, allocator, dir_path, file_name, max);
    }

    /// Writes `data` to `dir_path/file_name`, creating the directory. Returns
    /// false when the backend has no filesystem or the write failed.
    pub fn write_file(self: Backend, dir_path: []const u8, file_name: []const u8, data: []const u8) bool {
        return self.vtable.write_file(self.ptr, dir_path, file_name, data);
    }

    /// Deletes `dir_path/file_name` if it exists. No-op without a filesystem.
    pub fn delete_file(self: Backend, dir_path: []const u8, file_name: []const u8) void {
        self.vtable.delete_file(self.ptr, dir_path, file_name);
    }

    /// True when `dir_path/file_name` exists.
    pub fn file_exists(self: Backend, dir_path: []const u8, file_name: []const u8) bool {
        return self.vtable.file_exists(self.ptr, dir_path, file_name);
    }

    // ── Input and misc ───────────────────────────────────────────────────

    pub fn mouse_pos(self: Backend) types.Point {
        return self.vtable.mouse_pos(self.ptr);
    }

    pub fn update_discord(self: Backend, details: []const u8, state: []const u8) void {
        self.vtable.update_discord(self.ptr, details, state);
    }
};
