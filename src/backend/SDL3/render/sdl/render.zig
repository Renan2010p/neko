//! SDL3 backend for the engine. This is the only module that includes or
//! calls SDL. It implements the `engine.Backend` vtable faithfully to the
//! C++ `EngineSDL2`, ported to the SDL3 family of APIs.

const std: type = @import("std");
const c: type = @import("c");
const engine: type = @import("neko");
const platform: type = @import("neko_sdl3_platform");

const Allocator: type = std.mem.Allocator;
const mem: type = std.mem;
const fmt: type = std.fmt;
const TextureMap: type = std.AutoHashMapUnmanaged(u32, *c.SDL_Texture);
const TextCacheMap: type = std.AutoHashMapUnmanaged(u64, *c.SDL_Texture);
const AudioMap: type = std.AutoHashMapUnmanaged(u32, *c.MIX_Audio);
const FontList: type = std.ArrayListUnmanaged(*c.TTF_Font);

/// How many mixer tracks the backend keeps around, mirroring the SDL2
/// `Mix_AllocateChannels(32)`.
const NUM_CHANNELS: usize = 32;

/// Concrete SDL3 backend state. One instance per window.
pub const Sdl3Engine: type = struct {
    allocator: Allocator = undefined,
    /// Shared SDL window / events / timing / files layer.
    sdl: platform.Sdl = .{},
    renderer: ?*c.SDL_Renderer = null,

    mixer: ?*c.MIX_Mixer = null,
    /// One mixer track per logical sound channel. Channels are assigned the
    /// same way SDL2 mixed them (`-1` picks the first free one).
    tracks: [NUM_CHANNELS]?*c.MIX_Track = @splat(null),

    textures: TextureMap = .empty,
    text_cache: TextCacheMap = .empty,
    sounds: AudioMap = .empty,
    fonts: FontList = .empty,

    next_id: u32 = 1,

    master_vol: i32 = 80,
    sfx_vol: i32 = 100,
    music_vol: i32 = 70,

    /// Wraps this backend into the type-erased `engine.Backend` handle.
    pub fn backend(self: *Sdl3Engine) engine.Backend {
        return engine.Backend{
            .ptr = @ptrCast(self),
            .vtable = &vtable,
            .caps = caps_decl,
        };
    }
};

/// The concrete backend type, under a stable name. Game code uses
/// `neko_backend.Engine` and never names the platform, so switching backends
/// does not touch the game.
pub const Engine: type = Sdl3Engine;

/// Process-wide backend instance. Its address is stable so the type-erased
/// handle stays valid for the life of the process.
var instance: Sdl3Engine = .{};

/// Returns the backend as an abstract handle. Called by `src/core/platform.zig`,
/// the single core seam that knows this module.
pub fn create() engine.Backend {
    return instance.backend();
}

/// Runs a hosted game from its `main`.
///
/// The SDL3 `entry.zig` calls this so a game can use the same backend entry
/// point on every platform; on a hosted target the Zig/C runtime already hands
/// us the `std.process.Init`, so this just forwards it.
pub fn run(init: std.process.Init, main_fn: anytype) !void {
    return main_fn(init);
}

/// Which backend this module implements. Checked against `Config.backend`.
pub const kind: engine.BackendKind = .{ .name = "sdl3" };

/// The capabilities this backend declares. Identical to the SDL2 backend.
const caps_decl: engine.Capabilities = blk: {
    var set: engine.Capabilities = engine.Capabilities.empty;
    set.insert(.graphics2d);
    set.insert(.text);
    set.insert(.audio);
    set.insert(.files);
    set.insert(.input);
    set.insert(.streaming_textures);
    set.insert(.offscreen_targets);
    set.insert(.geometry);
    set.insert(.display_modes);
    set.insert(.curved_panorama);
    set.insert(.discord);
    break :blk set;
};

/// Shared entries from the SDL platform module, reading `Sdl3Engine.sdl`.
const P: type = platform.adapter(Sdl3Engine, "sdl");

// ── Helpers ──────────────────────────────────────────────────────────────

const vtable: engine.Backend.VTable = .{
    .init = vt_init,
    .shutdown = vt_shutdown,
    .keeps_running = P.keeps_running,
    .request_stop = P.request_stop,
    .present = vt_present,
    .poll_event = P.poll_event,
    .ticks_ms = P.ticks_ms,
    .set_logical_size = vt_set_logical_size,
    .set_title = P.set_title,
    .set_fullscreen = P.set_fullscreen,
    .set_vsync = vt_set_vsync,
    .set_resolution = vt_set_resolution,
    .logical_size = P.logical_size,
    .display_modes = P.display_modes,
    .supports_curved_panorama = vt_supports_curved_panorama,
    .supports_offscreen_targets = vt_supports_offscreen_targets,
    .set_draw_offset = P.set_draw_offset,
    .render_name = vt_render_name,
    .renderers = vt_renderers,
    .set_renderer = vt_set_renderer,
    .clear = vt_clear,
    .draw_rect = vt_draw_rect,
    .draw_line = vt_draw_line,
    .draw_circle = vt_draw_circle,
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
    .stop_channel = vt_stop_channel,
    .stop_all_sounds = vt_stop_all_sounds,
    .set_master_volume = vt_set_master_volume,
    .set_sfx_volume = vt_set_sfx_volume,
    .set_music_volume = vt_set_music_volume,
    .read_file = P.read_file,
    .write_file = P.write_file,
    .delete_file = P.delete_file,
    .file_exists = P.file_exists,
    .mouse_pos = P.mouse_pos,
    .update_discord = vt_update_discord,
    .discord_connect = P.discord_connect,
    .discord_set = P.discord_set,
    .discord_clear = P.discord_clear,
    .discord_close = P.discord_close,
    .discord_connected = P.discord_connected,
};

fn as_self(ptr: *anyopaque) *Sdl3Engine {
    return @ptrCast(@alignCast(ptr));
}

// ── Sdl3Engine methods ───────────────────────────────────────────────────

/// Creates a renderer for `name` (null = SDL's default). Returns false when the
/// driver is unavailable (leaving `self.renderer` null).
fn make_renderer(self: *Sdl3Engine, name: [*c]const u8, vsync: bool) bool {
    self.renderer = c.SDL_CreateRenderer(self.sdl.window, name);
    if (self.renderer == null) return false;
    _ = c.SDL_SetRenderVSync(self.renderer, if (vsync) 1 else c.SDL_RENDERER_VSYNC_DISABLED);
    // Let primitives honour alpha (translucent panels, fades).
    _ = c.SDL_SetRenderDrawBlendMode(self.renderer, c.SDL_BLENDMODE_BLEND);
    return true;
}

/// Picks the startup renderer. OpenGL first: SDL3's driver order puts Vulkan
/// (and the experimental GPU driver) first, which is much slower for this
/// many-2D-draw workload. Falls back to the default and then software.
fn create_default_renderer(self: *Sdl3Engine, vsync: bool) bool {
    if (make_renderer(self, "opengl", vsync)) return true;
    if (make_renderer(self, "opengles2", vsync)) return true;
    if (make_renderer(self, null, vsync)) return true;
    if (make_renderer(self, "software", vsync)) return true;
    return false;
}

/// Applies the logical size in logical-presentation coordinates, or disables
/// logical presentation when either dimension is zero.
fn apply_logical(self: *Sdl3Engine, width: u32, height: u32) void {
    if (width == 0 or height == 0) {
        _ = c.SDL_SetRenderLogicalPresentation(self.renderer, 0, 0, @intCast(c.SDL_LOGICAL_PRESENTATION_DISABLED));
        return;
    }
    _ = c.SDL_SetRenderLogicalPresentation(self.renderer, @intCast(width), @intCast(height), @intCast(c.SDL_LOGICAL_PRESENTATION_LETTERBOX));
}

fn destroy_textures(self: *Sdl3Engine) void {
    var it: TextureMap.ValueIterator = self.textures.valueIterator();
    while (it.next()) |tex| {
        c.SDL_DestroyTexture(tex.*);
    }
    self.textures.clearRetainingCapacity();
}

fn destroy_text_cache(self: *Sdl3Engine) void {
    var it: TextCacheMap.ValueIterator = self.text_cache.valueIterator();
    while (it.next()) |tex| {
        c.SDL_DestroyTexture(tex.*);
    }
    self.text_cache.clearRetainingCapacity();
}

/// Builds the default font path (`<assets_dir>/font/font.ttf`). Caller frees.
fn default_font_path(self: *Sdl3Engine) ?[]u8 {
    return fmt.allocPrint(self.allocator, "{s}/font/font.ttf", .{self.sdl.assets_dir}) catch null;
}

/// Opens a font and stores it, returning its index (-1 on failure).
fn open_font(self: *Sdl3Engine, path: []const u8, size: u16) i64 {
    const z: [:0]u8 = self.allocator.dupeSentinel(u8, path, 0) catch return -1;
    defer self.allocator.free(z);

    const font: ?*c.TTF_Font = c.TTF_OpenFont(z.ptr, @floatFromInt(size));
    if (font == null) return -1;

    const idx: i64 = @intCast(self.fonts.items.len);
    self.fonts.append(self.allocator, font.?) catch {
        c.TTF_CloseFont(font);
        return -1;
    };
    return idx;
}

/// Renders `text` into a new texture. Caller owns the texture.
fn render_text(self: *Sdl3Engine, font: *c.TTF_Font, text: []const u8, color: engine.Color, out_w: *c_int, out_h: *c_int) ?*c.SDL_Texture {
    const sdl_color: c.SDL_Color = c.SDL_Color{
        .r = color.r,
        .g = color.g,
        .b = color.b,
        .a = color.a,
    };
    const surf: [*c]c.SDL_Surface = c.TTF_RenderText_Blended(font, text.ptr, text.len, sdl_color);

    if (surf == null) {
        return null;
    }
    defer c.SDL_DestroySurface(surf);

    out_w.* = surf.*.w;
    out_h.* = surf.*.h;

    const raw: [*c]c.SDL_Texture = c.SDL_CreateTextureFromSurface(self.renderer, surf);

    if (raw == null) {
        return null;
    }

    _ = c.SDL_SetTextureBlendMode(raw, c.SDL_BLENDMODE_BLEND);
    return @as(*c.SDL_Texture, @ptrCast(raw));
}

// ── Sdl3Engine methods: audio ────────────────────────────────────────────

/// Creates the mixer and its per-channel tracks. Returns false when audio
/// could not be brought up.
fn open_audio(self: *Sdl3Engine) bool {
    self.mixer = c.MIX_CreateMixerDevice(c.SDL_AUDIO_DEVICE_DEFAULT_PLAYBACK, null);
    if (self.mixer == null) {
        return false;
    }
    var i: usize = 0;
    while (i < NUM_CHANNELS) : (i += 1) {
        self.tracks[i] = c.MIX_CreateTrack(self.mixer);
    }
    apply_master_volume(self);
    apply_sfx_volume(self);
    return true;
}

/// Master gain goes on the mixer, so it multiplies every track.
fn apply_master_volume(self: *Sdl3Engine) void {
    if (self.mixer) |mixer| {
        _ = c.MIX_SetMixerGain(mixer, @as(f32, @floatFromInt(self.master_vol)) / 100.0);
    }
}

/// Every channel is an SFX-style track, so the SFX volume is their gain.
fn apply_sfx_volume(self: *Sdl3Engine) void {
    const gain: f32 = @as(f32, @floatFromInt(self.sfx_vol)) / 100.0;
    for (self.tracks) |maybe| {
        if (maybe) |track| _ = c.MIX_SetTrackGain(track, gain);
    }
}

/// Picks the first track that is not currently mixing, or channel 0.
fn free_track(self: *Sdl3Engine) usize {
    var i: usize = 0;
    while (i < NUM_CHANNELS) : (i += 1) {
        const track: ?*c.MIX_Track = self.tracks[i];
        if (track == null or !c.MIX_TrackPlaying(track)) {
            return i;
        }
    }
    return 0;
}

// ── Vtable: lifecycle ────────────────────────────────────────────────────

fn vt_init(ptr: *anyopaque, config: engine.Config) bool {
    const self: *Sdl3Engine = as_self(ptr);
    self.allocator = config.allocator;

    const init_flags: c.SDL_InitFlags = c.SDL_INIT_VIDEO | c.SDL_INIT_AUDIO;
    if (!self.sdl.begin(config, init_flags)) {
        engine.log.err("sdl3: SDL_Init failed", .{});
        return false;
    }
    if (!c.TTF_Init()) {
        engine.log.err("sdl3: TTF_Init failed", .{});
        return false;
    }
    if (!c.MIX_Init()) {
        engine.log.err("sdl3: MIX_Init failed", .{});
        return false;
    }
    if (!open_audio(self)) {
        engine.log.err("sdl3: MIX_CreateMixerDevice failed", .{});
        return false;
    }

    if (!self.sdl.openWindow(config, 0)) {
        engine.log.err("sdl3: SDL_CreateWindow failed", .{});
        return false;
    }

    if (!create_default_renderer(self, config.vsync)) {
        engine.log.err("sdl3: SDL_CreateRenderer failed: {s}", .{platform.cstr(c.SDL_GetError())});
        return false;
    }
    _ = c.SDL_SetRenderDrawBlendMode(self.renderer, c.SDL_BLENDMODE_BLEND);
    apply_logical(self, config.width, config.height);

    engine.log.info("sdl3: window {d}x{d} ready (vsync {}, renderer)", .{ config.width, config.height, config.vsync });
    engine.attach(self.backend());
    return true;
}

fn vt_shutdown(ptr: *anyopaque) void {
    const self: *Sdl3Engine = as_self(ptr);

    destroy_textures(self);
    destroy_text_cache(self);

    for (self.fonts.items) |font| {
        c.TTF_CloseFont(font);
    }
    self.fonts.deinit(self.allocator);

    for (self.tracks) |maybe| {
        if (maybe) |track| c.MIX_DestroyTrack(track);
    }
    for (&self.tracks) |*slot| slot.* = null;

    var sounds: AudioMap.ValueIterator = self.sounds.valueIterator();
    while (sounds.next()) |audio| {
        c.MIX_DestroyAudio(audio.*);
    }
    self.sounds.deinit(self.allocator);

    if (self.mixer) |mixer| {
        c.MIX_DestroyMixer(mixer);
        self.mixer = null;
    }

    self.textures.deinit(self.allocator);
    self.text_cache.deinit(self.allocator);

    if (self.renderer != null) {
        c.SDL_DestroyRenderer(self.renderer);
        self.renderer = null;
    }
    self.sdl.closeWindow();

    c.MIX_Quit();
    c.TTF_Quit();
    c.SDL_Quit();

    engine.detach();
}

fn vt_present(ptr: *anyopaque) void {
    const self: *Sdl3Engine = as_self(ptr);
    _ = c.SDL_RenderPresent(self.renderer);
    // SDL3/Wayland mangles a title set before the window is on screen; reapply
    // it once it has been mapped.
    self.sdl.tickTitle();
}

// ── Vtable: window ───────────────────────────────────────────────────────

fn vt_set_logical_size(ptr: *anyopaque, width: u32, height: u32) void {
    const self: *Sdl3Engine = as_self(ptr);
    self.sdl.setLogicalSize(width, height);
    apply_logical(self, width, height);
}

fn vt_set_vsync(ptr: *anyopaque, on: bool) void {
    const self: *Sdl3Engine = as_self(ptr);

    if (on == self.sdl.vsync) {
        return;
    }

    self.sdl.vsync = on;

    // SDL3 can retune vsync on the live renderer; nothing has to be rebuilt.
    if (self.renderer != null) {
        _ = c.SDL_SetRenderVSync(self.renderer, if (on) 1 else c.SDL_RENDERER_VSYNC_DISABLED);
    }
}

fn vt_set_resolution(ptr: *anyopaque, width: u32, height: u32) void {
    const self: *Sdl3Engine = as_self(ptr);
    self.sdl.setResolution(width, height);
    apply_logical(self, width, height);
}

// ── Renderer selection ──────────────────────────────────────────────────────

fn vt_render_name(ptr: *anyopaque) []const u8 {
    const self: *Sdl3Engine = as_self(ptr);
    if (self.renderer == null) return "";
    return platform.cstr(c.SDL_GetRendererName(self.renderer));
}

fn vt_renderers(ptr: *anyopaque, allocator: std.mem.Allocator) []engine.RenderInfo {
    _ = ptr;
    const n: c_int = c.SDL_GetNumRenderDrivers();
    if (n <= 0) return &.{};
    const out: []engine.RenderInfo = allocator.alloc(engine.RenderInfo, @intCast(n)) catch return &.{};
    var count: usize = 0;
    var i: c_int = 0;
    while (i < n) : (i += 1) {
        const name: [*c]const u8 = c.SDL_GetRenderDriver(i);
        if (name == null) continue;
        const s: []const u8 = platform.cstr(name);
        // Skip the experimental SDL_GPU driver: it has no backend on many GPUs
        // (including incomplete Vulkan) and only confuses the menu.
        if (mem.eql(u8, s, "gpu")) continue;
        out[count] = engine.RenderInfo{ .name = s };
        count += 1;
    }
    return out[0..count];
}

fn vt_set_renderer(ptr: *anyopaque, name: []const u8) bool {
    const self: *Sdl3Engine = as_self(ptr);

    if (self.renderer != null) {
        c.SDL_DestroyRenderer(self.renderer);
        self.renderer = null;
    }
    destroy_textures(self);
    destroy_text_cache(self);
    engine.sprite.clear_cache();

    // Vulkan needs the window created with SDL_WINDOW_VULKAN before the surface
    // can be made; recreate it when the requested driver requires a flag.
    const want_vulkan: bool = mem.eql(u8, name, "vulkan");
    const flags: c.SDL_WindowFlags = if (want_vulkan) c.SDL_WINDOW_VULKAN else 0;
    if (!self.sdl.recreateWindow(flags)) {
        engine.log.err("sdl3: recreate window failed: {s}", .{platform.cstr(c.SDL_GetError())});
    }

    const z: [:0]u8 = self.allocator.dupeSentinel(u8, name, 0) catch return false;
    defer self.allocator.free(z);

    if (!make_renderer(self, z.ptr, self.sdl.vsync)) {
        engine.log.err("sdl3: renderer '{s}' unavailable: {s}", .{ name, platform.cstr(c.SDL_GetError()) });
        // Undo any window-flag change before falling back.
        if (want_vulkan) _ = self.sdl.recreateWindow(0);
        if (!make_renderer(self, "opengl", self.sdl.vsync) and
            !make_renderer(self, null, self.sdl.vsync) and
            !make_renderer(self, "software", false))
        {
            return false;
        }
    }
    apply_logical(self, self.sdl.logical_w, self.sdl.logical_h);
    return true;
}

fn vt_supports_curved_panorama(ptr: *anyopaque) bool {
    _ = ptr;
    return true;
}

fn vt_supports_offscreen_targets(ptr: *anyopaque) bool {
    _ = ptr;
    return true;
}

// ── Vtable: drawing primitives ───────────────────────────────────────────

fn vt_clear(ptr: *anyopaque, color: engine.Color) void {
    const self: *Sdl3Engine = as_self(ptr);
    _ = c.SDL_SetRenderDrawColor(self.renderer, color.r, color.g, color.b, color.a);
    _ = c.SDL_RenderClear(self.renderer);
}

fn vt_draw_rect(ptr: *anyopaque, rect: engine.Rect, color: engine.Color, filled: bool) void {
    const self: *Sdl3Engine = as_self(ptr);

    _ = c.SDL_SetRenderDrawColor(self.renderer, color.r, color.g, color.b, color.a);

    const r: c.SDL_FRect = c.SDL_FRect{
        .x = @floatFromInt(rect.x),
        .y = @floatFromInt(rect.y),
        .w = @floatFromInt(rect.w),
        .h = @floatFromInt(rect.h),
    };

    if (filled) {
        _ = c.SDL_RenderFillRect(self.renderer, &r);
    } else {
        _ = c.SDL_RenderRect(self.renderer, &r);
    }
}

fn vt_draw_line(ptr: *anyopaque, x1: i32, y1: i32, x2: i32, y2: i32, color: engine.Color) void {
    const self: *Sdl3Engine = as_self(ptr);

    _ = c.SDL_SetRenderDrawColor(self.renderer, color.r, color.g, color.b, color.a);

    _ = c.SDL_RenderLine(
        self.renderer,
        @floatFromInt(x1),
        @floatFromInt(y1),
        @floatFromInt(x2),
        @floatFromInt(y2),
    );
}

fn vt_draw_circle(ptr: *anyopaque, cx: i32, cy: i32, radius: i32, color: engine.Color, filled: bool) void {
    const self: *Sdl3Engine = as_self(ptr);

    if (radius <= 0) {
        return;
    }

    _ = c.SDL_SetRenderDrawColor(self.renderer, color.r, color.g, color.b, color.a);

    const r2: i64 = @as(i64, radius) * @as(i64, radius);

    var dy: i32 = -radius;

    while (dy <= radius) : (dy += 1) {
        const dy2: i64 = @as(i64, dy) * @as(i64, dy);

        const diff: i64 = r2 - dy2;

        if (diff < 0) {
            continue;
        }

        const half: i32 = @intFromFloat(@sqrt(@as(f32, @floatFromInt(diff))));
        const w: i32 = half * 2 + 1;

        if (w > 0) {
            const r: c.SDL_FRect = c.SDL_FRect{
                .x = @floatFromInt(cx - half),
                .y = @floatFromInt(cy + dy),
                .w = @floatFromInt(w),
                .h = 1,
            };
            if (filled) {
                _ = c.SDL_RenderFillRect(self.renderer, &r);
            } else {
                _ = c.SDL_RenderPoint(self.renderer, @floatFromInt(cx), @floatFromInt(cy + dy));
            }
        }
    }
}

// ── Vtable: textures ─────────────────────────────────────────────────────

fn vt_load_texture(ptr: *anyopaque, path: []const u8) ?engine.TextureHandle {
    const self: *Sdl3Engine = as_self(ptr);

    const z: [:0]u8 = self.allocator.dupeSentinel(u8, path, 0) catch return null;
    defer self.allocator.free(z);

    const surf: [*c]c.SDL_Surface = c.IMG_Load(z.ptr);
    if (surf == null) return null;
    defer c.SDL_DestroySurface(surf);

    const raw: [*c]c.SDL_Texture = c.SDL_CreateTextureFromSurface(self.renderer, surf);
    if (raw == null) return null;
    _ = c.SDL_SetTextureBlendMode(raw, c.SDL_BLENDMODE_BLEND);

    const t: *c.SDL_Texture = @as(*c.SDL_Texture, @ptrCast(raw));

    const id: u32 = self.next_id;

    self.next_id += 1;

    self.textures.put(self.allocator, id, t) catch {
        c.SDL_DestroyTexture(t);
        return null;
    };
    return engine.TextureHandle{ .id = id };
}

fn vt_create_target(ptr: *anyopaque, width: u32, height: u32) ?engine.TextureHandle {
    const self: *Sdl3Engine = as_self(ptr);

    const raw: [*c]c.SDL_Texture = c.SDL_CreateTexture(
        self.renderer,
        @intCast(c.SDL_PIXELFORMAT_RGBA8888),
        @intCast(c.SDL_TEXTUREACCESS_TARGET),
        @intCast(width),
        @intCast(height),
    );

    if (raw == null) {
        return null;
    }

    const t: *c.SDL_Texture = @as(*c.SDL_Texture, @ptrCast(raw));

    const id: u32 = self.next_id;

    self.next_id += 1;

    self.textures.put(self.allocator, id, t) catch {
        c.SDL_DestroyTexture(t);
        return null;
    };
    return engine.TextureHandle{ .id = id };
}

fn vt_create_texture(ptr: *anyopaque, width: u32, height: u32, pixels: ?[]const u8, pitch: u32) ?engine.TextureHandle {
    const self: *Sdl3Engine = as_self(ptr);

    const raw: [*c]c.SDL_Texture = c.SDL_CreateTexture(
        self.renderer,
        @intCast(c.SDL_PIXELFORMAT_ARGB8888),
        @intCast(c.SDL_TEXTUREACCESS_STREAMING),
        @intCast(width),
        @intCast(height),
    );

    if (raw == null) {
        return null;
    }

    _ = c.SDL_SetTextureBlendMode(raw, c.SDL_BLENDMODE_BLEND);

    if (pixels) |px| {
        _ = c.SDL_UpdateTexture(raw, null, px.ptr, @intCast(pitch));
    }

    const t: *c.SDL_Texture = @as(*c.SDL_Texture, @ptrCast(raw));

    const id: u32 = self.next_id;
    self.next_id += 1;
    self.textures.put(self.allocator, id, t) catch {
        c.SDL_DestroyTexture(t);
        return null;
    };
    return engine.TextureHandle{ .id = id };
}

fn vt_update_texture(ptr: *anyopaque, tex: engine.TextureHandle, pixels: []const u8, pitch: u32) void {
    const self: *Sdl3Engine = as_self(ptr);

    const t: ?*c.SDL_Texture = self.textures.get(tex.id) orelse return;

    _ = c.SDL_UpdateTexture(t, null, pixels.ptr, @intCast(pitch));
}

fn vt_draw_texture(ptr: *anyopaque, tex: engine.TextureHandle, dst: engine.Rect, src: ?engine.Rect, alpha: ?u8) void {
    const self: *Sdl3Engine = as_self(ptr);
    const t: ?*c.SDL_Texture = self.textures.get(tex.id) orelse return;

    if (alpha) |a| {
        _ = c.SDL_SetTextureAlphaMod(t, a);
    }

    const dst_rect: c.SDL_FRect = c.SDL_FRect{
        .x = @floatFromInt(dst.x),
        .y = @floatFromInt(dst.y),
        .w = @floatFromInt(dst.w),
        .h = @floatFromInt(dst.h),
    };

    if (src) |s| {
        const src_rect: c.SDL_FRect = c.SDL_FRect{
            .x = @floatFromInt(s.x),
            .y = @floatFromInt(s.y),
            .w = @floatFromInt(s.w),
            .h = @floatFromInt(s.h),
        };
        _ = c.SDL_RenderTexture(self.renderer, t, &src_rect, &dst_rect);
    } else {
        _ = c.SDL_RenderTexture(self.renderer, t, null, &dst_rect);
    }

    if (alpha != null) {
        _ = c.SDL_SetTextureAlphaMod(t, 255);
    }
}

fn vt_draw_texture_rotated(ptr: *anyopaque, tex: engine.TextureHandle, dst: engine.Rect, angle: f32, alpha: ?u8) void {
    const self: *Sdl3Engine = as_self(ptr);
    const t: ?*c.SDL_Texture = self.textures.get(tex.id) orelse return;

    if (alpha) |a| {
        _ = c.SDL_SetTextureAlphaMod(t, a);
    }

    const dst_rect: c.SDL_FRect = c.SDL_FRect{
        .x = @floatFromInt(dst.x),
        .y = @floatFromInt(dst.y),
        .w = @floatFromInt(dst.w),
        .h = @floatFromInt(dst.h),
    };

    _ = c.SDL_RenderTextureRotated(self.renderer, t, null, &dst_rect, @floatCast(angle), null, @intCast(c.SDL_FLIP_NONE));

    if (alpha != null) {
        _ = c.SDL_SetTextureAlphaMod(t, 255);
    }
}

fn vt_texture_size(ptr: *anyopaque, tex: engine.TextureHandle) engine.Point {
    const self: *Sdl3Engine = as_self(ptr);

    const t: ?*c.SDL_Texture = self.textures.get(tex.id) orelse return engine.Point{ .x = 0, .y = 0 };

    var w: f32 = 0;
    var h: f32 = 0;

    _ = c.SDL_GetTextureSize(t, &w, &h);

    return engine.Point{
        .x = @intFromFloat(w),
        .y = @intFromFloat(h),
    };
}

fn vt_geometry(ptr: *anyopaque, tex: engine.TextureHandle, vertices: []const engine.Vertex, indices: []const i32) void {
    const self: *Sdl3Engine = as_self(ptr);

    const t: ?*c.SDL_Texture = self.textures.get(tex.id) orelse return;

    if (vertices.len == 0 or indices.len == 0) {
        return;
    }

    // `SDL_Vertex` carries a float colour and reordered fields, so the engine's
    // interleaved u8-colour vertices have to be converted, not reinterpreted.
    const sdl_verts: []c.SDL_Vertex = self.allocator.alloc(c.SDL_Vertex, vertices.len) catch return;
    defer self.allocator.free(sdl_verts);

    for (vertices, 0..) |v, i| {
        sdl_verts[i] = c.SDL_Vertex{
            .position = c.SDL_FPoint{ .x = v.x, .y = v.y },
            .color = c.SDL_FColor{
                .r = @as(f32, @floatFromInt(v.r)) / 255.0,
                .g = @as(f32, @floatFromInt(v.g)) / 255.0,
                .b = @as(f32, @floatFromInt(v.b)) / 255.0,
                .a = @as(f32, @floatFromInt(v.a)) / 255.0,
            },
            .tex_coord = c.SDL_FPoint{ .x = v.u, .y = v.v },
        };
    }

    _ = c.SDL_RenderGeometry(
        self.renderer,
        t,
        sdl_verts.ptr,
        @intCast(sdl_verts.len),
        indices.ptr,
        @intCast(indices.len),
    );
}

fn vt_set_render_target(ptr: *anyopaque, target: ?engine.TextureHandle) void {
    const self: *Sdl3Engine = as_self(ptr);

    if (target) |handle| {
        const t: ?*c.SDL_Texture = self.textures.get(handle.id) orelse return;
        _ = c.SDL_SetRenderTarget(self.renderer, t);
    } else {
        _ = c.SDL_SetRenderTarget(self.renderer, null);
        apply_logical(self, self.sdl.logical_w, self.sdl.logical_h);
    }
}

// ── Vtable: text ─────────────────────────────────────────────────────────

fn vt_load_font(ptr: *anyopaque, path: []const u8, size: u16) i64 {
    const self: *Sdl3Engine = as_self(ptr);
    return open_font(self, path, size);
}

fn vt_draw_text(ptr: *anyopaque, text: []const u8, x: i32, y: i32, size: u32, color: engine.Color, center: bool, font_idx: i64) bool {
    const self: *Sdl3Engine = as_self(ptr);
    if (text.len == 0) return true;

    var fidx: i64 = font_idx;
    if (fidx < 0) {
        const path: []u8 = default_font_path(self) orelse return true;

        defer self.allocator.free(path);

        fidx = open_font(self, path, @intCast(size));

        if (fidx < 0) {
            return true;
        }
    }
    const idx: usize = @intCast(fidx);
    if (idx >= self.fonts.items.len) {
        return true;
    }

    if (color.a == 0) {
        return true;
    }

    // FNV-1a over the text, mixed with the font and RGB. Alpha is applied via
    // the texture's alpha mod below, so a single texture serves every fade
    // step (the C++ baked alpha into the key and re-rasterized every frame).
    var h: u64 = 0xcbf29ce484222325;
    for (text) |ch| {
        h ^= ch;
        h *%= 0x100000001b3;
    }
    h ^= (@as(u64, @intCast(fidx)) << 32) |
        (@as(u64, color.r) << 16) |
        (@as(u64, color.g) << 8) |
        @as(u64, color.b);

    var tw: c_int = 0;
    var th: c_int = 0;
    var tex: ?*c.SDL_Texture = self.text_cache.get(h);
    if (tex == null) {
        if (self.text_cache.count() > 2048) {
            destroy_text_cache(self);
        }
        const solid: engine.Color = engine.Color{ .r = color.r, .g = color.g, .b = color.b, .a = 255 };
        tex = render_text(self, self.fonts.items[idx], text, solid, &tw, &th);
        if (tex == null) return true;
        self.text_cache.put(self.allocator, h, tex.?) catch {
            c.SDL_DestroyTexture(tex);
            return true;
        };
    } else {
        var wf: f32 = 0;
        var hf: f32 = 0;
        _ = c.SDL_GetTextureSize(tex, &wf, &hf);
        tw = @intFromFloat(wf);
        th = @intFromFloat(hf);
    }

    const px: c_int = if (center) x - @divTrunc(tw, 2) else x;
    const py: c_int = if (center) y - @divTrunc(th, 2) else y;
    const dst: c.SDL_FRect = c.SDL_FRect{
        .x = @floatFromInt(px),
        .y = @floatFromInt(py),
        .w = @floatFromInt(tw),
        .h = @floatFromInt(th),
    };

    if (color.a != 255) _ = c.SDL_SetTextureAlphaMod(tex, color.a);

    _ = c.SDL_RenderTexture(self.renderer, tex, null, &dst);

    if (color.a != 255) _ = c.SDL_SetTextureAlphaMod(tex, 255);
    return true;
}

fn vt_draw_text_rotated(ptr: *anyopaque, text: []const u8, x: i32, y: i32, size: u32, angle: f32, color: engine.Color, center: bool, font_idx: i64) bool {
    const self: *Sdl3Engine = as_self(ptr);
    if (text.len == 0) return true;

    var fidx: i64 = font_idx;

    if (fidx < 0) {
        const path: []u8 = default_font_path(self) orelse return true;
        defer self.allocator.free(path);
        fidx = open_font(self, path, @intCast(size));
        if (fidx < 0) return true;
    }
    const idx: usize = @intCast(fidx);

    if (idx >= self.fonts.items.len) {
        return true;
    }

    var tw: c_int = 0;
    var th: c_int = 0;
    const solid: engine.Color = engine.Color{ .r = color.r, .g = color.g, .b = color.b, .a = 255 };
    const tex: *c.SDL_Texture = render_text(self, self.fonts.items[idx], text, solid, &tw, &th) orelse return true;
    defer c.SDL_DestroyTexture(tex);

    const px: c_int = if (center) x - @divTrunc(tw, 2) else x;
    const py: c_int = if (center) y - @divTrunc(th, 2) else y;
    const dst: c.SDL_FRect = c.SDL_FRect{
        .x = @floatFromInt(px),
        .y = @floatFromInt(py),
        .w = @floatFromInt(tw),
        .h = @floatFromInt(th),
    };
    if (color.a != 255) _ = c.SDL_SetTextureAlphaMod(tex, color.a);
    _ = c.SDL_RenderTextureRotated(self.renderer, tex, null, &dst, @floatCast(angle), null, @intCast(c.SDL_FLIP_NONE));
    return true;
}

fn vt_text_size(ptr: *anyopaque, text: []const u8, font_idx: u32) ?engine.Point {
    const self: *Sdl3Engine = as_self(ptr);
    if (font_idx >= self.fonts.items.len) return null;

    var w: c_int = 0;
    var h: c_int = 0;
    if (!c.TTF_GetStringSize(self.fonts.items[font_idx], text.ptr, text.len, &w, &h)) return null;
    return engine.Point{ .x = w, .y = h };
}

// ── Vtable: sound ────────────────────────────────────────────────────────

fn vt_load_sound(ptr: *anyopaque, path: []const u8) ?engine.SoundHandle {
    const self: *Sdl3Engine = as_self(ptr);
    const mixer: *c.MIX_Mixer = self.mixer orelse return null;

    const z: [:0]u8 = self.allocator.dupeSentinel(u8, path, 0) catch return null;
    defer self.allocator.free(z);

    const audio: ?*c.MIX_Audio = c.MIX_LoadAudio(mixer, z.ptr, true);
    if (audio == null) return null;

    const id: u32 = self.next_id;
    self.next_id += 1;
    self.sounds.put(self.allocator, id, audio.?) catch {
        c.MIX_DestroyAudio(audio);
        return null;
    };
    return engine.SoundHandle{ .id = id };
}

fn vt_play_sound(ptr: *anyopaque, snd: engine.SoundHandle, loops: i32, channel: i32) i32 {
    const self: *Sdl3Engine = as_self(ptr);
    const audio: *c.MIX_Audio = self.sounds.get(snd.id) orelse return channel;

    const ch: usize = if (channel < 0) free_track(self) else blk: {
        if (@as(usize, @intCast(channel)) >= NUM_CHANNELS) break :blk 0;
        break :blk @intCast(channel);
    };
    const track: *c.MIX_Track = self.tracks[ch] orelse return channel;

    _ = c.MIX_SetTrackAudio(track, audio);

    // Loops must travel with the play call: `MIX_SetTrackLoops` on a stopped
    // track is overwritten when playback starts again.
    const props: c.SDL_PropertiesID = c.SDL_CreateProperties();
    if (props != 0) {
        _ = c.SDL_SetNumberProperty(props, c.MIX_PROP_PLAY_LOOPS_NUMBER, loops);
    }
    _ = c.MIX_PlayTrack(track, props);
    if (props != 0) {
        c.SDL_DestroyProperties(props);
    }
    return @intCast(ch);
}

fn vt_stop_channel(ptr: *anyopaque, channel: i32) void {
    const self: *Sdl3Engine = as_self(ptr);
    if (channel < 0) {
        vt_stop_all_sounds(ptr);
        return;
    }
    if (@as(usize, @intCast(channel)) >= NUM_CHANNELS) return;
    if (self.tracks[@intCast(channel)]) |track| {
        _ = c.MIX_StopTrack(track, 0);
    }
}

fn vt_stop_all_sounds(ptr: *anyopaque) void {
    const self: *Sdl3Engine = as_self(ptr);
    for (self.tracks) |maybe| {
        if (maybe) |track| _ = c.MIX_StopTrack(track, 0);
    }
}

fn vt_set_master_volume(ptr: *anyopaque, vol: i32) void {
    const self: *Sdl3Engine = as_self(ptr);
    self.master_vol = vol;
    apply_master_volume(self);
}

fn vt_set_sfx_volume(ptr: *anyopaque, vol: i32) void {
    const self: *Sdl3Engine = as_self(ptr);
    self.sfx_vol = vol;
    apply_sfx_volume(self);
}

fn vt_set_music_volume(ptr: *anyopaque, vol: i32) void {
    const self: *Sdl3Engine = as_self(ptr);
    self.music_vol = vol;
    // TODO(sdl3): approximate. The engine only ever loads SFX-style audio
    // (`MIX_LoadAudio`), never a distinct music stream, so — exactly like the
    // SDL2 backend's `Mix_VolumeMusic` on `Mix_LoadWAV` chunks — this has no
    // audible effect. Master gain lives on the mixer, SFX gain on the tracks.
}

// ── Vtable: misc ─────────────────────────────────────────────────────────

fn vt_update_discord(ptr: *anyopaque, details: []const u8, state: []const u8) void {
    _ = ptr;
    _ = details;
    _ = state;
}
