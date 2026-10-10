// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! The low-level backend dispatch table.
//!
//! A platform backend fills in a `VTable` and hands back a `Backend` handle.
//! The public `neko.*` namespaces call the wrapper methods below, which forward
//! to the backend through `vtable`.
//!
//! The `VTable` is **not** written by hand: it is folded at compile time from
//! the capability modules under `caps/` (`core`, `window`, `graphics`, `text`,
//! `audio`, `files`, `input`, `misc`). Each field has a no-op default, so a
//! backend only has to name the capabilities it supports — everything else
//! degrades gracefully.
//!
//! Game code never touches this type directly: it uses `neko.draw`,
//! `neko.text`, and friends.

const std: type = @import("std");
const Allocator: type = std.mem.Allocator;
const types: type = @import("types.zig");
const event: type = @import("../system/event.zig");
const render3d: type = @import("render3d.zig");

const features: type = @import("caps/features.zig");
const merge_mod: type = @import("caps/merge.zig");
const core_cap: type = @import("caps/core.zig");
const window_cap: type = @import("caps/window.zig");
const graphics_cap: type = @import("caps/graphics.zig");
const text_cap: type = @import("caps/text.zig");
const audio_cap: type = @import("caps/audio.zig");
const files_cap: type = @import("caps/files.zig");
const input_cap: type = @import("caps/input.zig");
const misc_cap: type = @import("caps/misc.zig");
const discord_cap: type = @import("caps/discord.zig");

/// A capability or optional feature a backend may declare.
pub const Feature: type = features.Feature;

/// A set of declared capabilities.
pub const Capabilities: type = features.Capabilities;

/// A type-erased handle to a platform backend.
pub const Backend: type = struct {
    ptr: *anyopaque,
    vtable: *const VTable,
    /// Optional 3D pipeline. `null` for 2D-only backends.
    render3d: ?*const render3d.VTable = null,
    /// The capabilities this backend declares. Empty when undeclared.
    caps: Capabilities = Capabilities.empty,

    /// The backend function table, folded from the capability modules under
    /// `caps/`. Every field has a no-op default, so a backend only names the
    /// capabilities it supports; unsupported calls degrade to a no-op.
    pub const VTable: type = merge_mod.merge(&.{
        core_cap.VTable,
        window_cap.VTable,
        graphics_cap.VTable,
        text_cap.VTable,
        audio_cap.VTable,
        files_cap.VTable,
        input_cap.VTable,
        misc_cap.VTable,
        discord_cap.VTable,
    });

    /// True when the backend declares `feature`.
    pub fn supports(self: Backend, feature: Feature) bool {
        return self.caps.contains(feature);
    }

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

    /// The active renderer's name (e.g. "opengl"), or "".
    pub fn render_name(self: Backend) []const u8 {
        return self.vtable.render_name(self.ptr);
    }

    /// The renderers this backend can use. Caller frees the result.
    pub fn renderers(self: Backend, allocator: Allocator) []types.RenderInfo {
        return self.vtable.renderers(self.ptr, allocator);
    }

    /// Recreates the renderer with the named driver. Returns false on failure.
    pub fn set_renderer(self: Backend, name: []const u8) bool {
        return self.vtable.set_renderer(self.ptr, name);
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

    // ── Shaders ──────────────────────────────────────────────────────────

    /// Compiles a vertex+fragment shader program, or null when unsupported.
    pub fn shader_load(self: Backend, vertex_src: []const u8, fragment_src: []const u8) ?types.ShaderHandle {
        return self.vtable.shader_load(self.ptr, vertex_src, fragment_src);
    }

    /// Loads a shader the backend ships itself, by name (e.g. "cylinder").
    pub fn shader_load_builtin(self: Backend, name: []const u8) ?types.ShaderHandle {
        return self.vtable.shader_load_builtin(self.ptr, name);
    }

    /// Frees a program returned by `shader_load`.
    pub fn shader_free(self: Backend, shader: types.ShaderHandle) void {
        self.vtable.shader_free(self.ptr, shader);
    }

    /// Draws a full-screen quad with `shader`, sampling `tex` on unit 0 and
    /// passing `params` to the fragment shader as `u_params`.
    pub fn shader_draw(self: Backend, shader: types.ShaderHandle, tex: types.TextureHandle, params: [4]f32) void {
        self.vtable.shader_draw(self.ptr, shader, tex, params);
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

    // ── Discord Rich Presence ────────────────────────────────────────────

    /// Connects to the local Discord client. Returns false when Discord is not
    /// running or the backend has no support.
    pub fn discord_connect(self: Backend, client_id: []const u8) bool {
        return self.vtable.discord_connect(self.ptr, client_id);
    }

    /// Sends a presence. Returns false when not connected.
    pub fn discord_set(self: Backend, presence: types.DiscordPresence) bool {
        return self.vtable.discord_set(self.ptr, presence);
    }

    /// Clears the current presence.
    pub fn discord_clear(self: Backend) void {
        self.vtable.discord_clear(self.ptr);
    }

    /// Closes the Discord connection.
    pub fn discord_close(self: Backend) void {
        self.vtable.discord_close(self.ptr);
    }

    /// True when connected to Discord.
    pub fn discord_connected(self: Backend) bool {
        return self.vtable.discord_connected(self.ptr);
    }
};
