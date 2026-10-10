//! SDL2 backend for the engine. This is the only module that includes or
//! calls SDL. It implements the `engine.Backend` vtable faithfully to the
//! C++ `EngineSDL2`.

const std: type = @import("std");
const c: type = @import("c");
const engine: type = @import("neko");
const platform: type = @import("neko_sdl2_platform");

const Allocator: type = std.mem.Allocator;
const fmt: type = std.fmt;
const TextureMap: type = std.AutoHashMapUnmanaged(u32, *c.SDL_Texture);
const TextCacheMap: type = std.AutoHashMapUnmanaged(u64, *c.SDL_Texture);
const ChunkMap: type = std.AutoHashMapUnmanaged(u32, *c.Mix_Chunk);
const FontList: type = std.ArrayListUnmanaged(*c.TTF_Font);

/// Concrete SDL2 backend state. One instance per window.
pub const Sdl2Engine: type = struct {
    allocator: Allocator = undefined,
    /// Shared SDL window / events / timing / files layer.
    sdl: platform.Sdl = .{},
    renderer: ?*c.SDL_Renderer = null,

    textures: TextureMap = .empty,
    text_cache: TextCacheMap = .empty,
    chunks: ChunkMap = .empty,
    fonts: FontList = .empty,

    next_id: u32 = 1,

    master_vol: i32 = 80,
    sfx_vol: i32 = 100,
    music_vol: i32 = 70,

    /// Wraps this backend into the type-erased `engine.Backend` handle.
    pub fn backend(self: *Sdl2Engine) engine.Backend {
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
pub const Engine: type = Sdl2Engine;

/// Process-wide backend instance. Its address is stable so the type-erased
/// handle stays valid for the life of the process.
var instance: Sdl2Engine = .{};

/// Returns the backend as an abstract handle. Called by `src/core/platform.zig`,
/// the single core seam that knows this module.
pub fn create() engine.Backend {
    return instance.backend();
}

/// Runs a hosted game from its `main`.
///
/// The SDL2 `entry.zig` calls this so a game can use the same backend entry
/// point on every platform; on a hosted target the Zig/C runtime already hands
/// us the `std.process.Init`, so this just forwards it.
pub fn run(init: std.process.Init, main_fn: anytype) !void {
    return main_fn(init);
}

/// Which backend this module implements. Checked against `Config.backend`.
pub const kind: engine.BackendKind = .{ .name = "sdl2" };

/// The capabilities this backend declares.
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

/// Shared entries from the SDL platform module, reading `Sdl2Engine.sdl`.
const P: type = platform.adapter(Sdl2Engine, "sdl");

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

fn as_self(ptr: *anyopaque) *Sdl2Engine {
    return @ptrCast(@alignCast(ptr));
}

// ── Sdl2Engine methods ───────────────────────────────────────────────────

fn create_renderer(self: *Sdl2Engine, vsync: bool) bool {
    var flags: c.Uint32 = @intCast(c.SDL_RENDERER_ACCELERATED);
    if (vsync) {
        flags |= @intCast(c.SDL_RENDERER_PRESENTVSYNC);
    }
    self.renderer = c.SDL_CreateRenderer(self.sdl.window, -1, flags);

    // No GPU / no accelerated driver (headless CI, `SDL_VIDEODRIVER=dummy`):
    // fall back to the software renderer instead of failing to start.
    if (self.renderer == null) {
        self.renderer = c.SDL_CreateRenderer(self.sdl.window, -1, @intCast(c.SDL_RENDERER_SOFTWARE));
    }

    if (self.renderer == null) {
        return false;
    }
    // Let primitives honour alpha (translucent panels, fades).
    _ = c.SDL_SetRenderDrawBlendMode(self.renderer, c.SDL_BLENDMODE_BLEND);

    return true;
}

fn destroy_textures(self: *Sdl2Engine) void {
    var it: TextureMap.ValueIterator = self.textures.valueIterator();
    while (it.next()) |tex| {
        c.SDL_DestroyTexture(tex.*);
    }
    self.textures.clearRetainingCapacity();
}

fn destroy_text_cache(self: *Sdl2Engine) void {
    var it: TextCacheMap.ValueIterator = self.text_cache.valueIterator();
    while (it.next()) |tex| {
        c.SDL_DestroyTexture(tex.*);
    }
    self.text_cache.clearRetainingCapacity();
}

/// Builds the default font path (`<assets_dir>/font/font.ttf`). Caller frees.
fn default_font_path(self: *Sdl2Engine) ?[]u8 {
    return fmt.allocPrint(self.allocator, "{s}/font/font.ttf", .{self.sdl.assets_dir}) catch null;
}

/// Opens a font and stores it, returning its index (-1 on failure).
fn open_font(self: *Sdl2Engine, path: []const u8, size: u16) i64 {
    const z: [:0]u8 = self.allocator.dupeSentinel(u8, path, 0) catch return -1;
    defer self.allocator.free(z);

    const font: ?*c.TTF_Font = c.TTF_OpenFont(z.ptr, size);
    if (font == null) return -1;

    const idx: i64 = @intCast(self.fonts.items.len);
    self.fonts.append(self.allocator, font.?) catch {
        c.TTF_CloseFont(font);
        return -1;
    };
    return idx;
}

/// Renders `text` into a new texture. Caller owns the texture.
fn render_text(self: *Sdl2Engine, font: *c.TTF_Font, text: []const u8, color: engine.Color, out_w: *c_int, out_h: *c_int) ?*c.SDL_Texture {
    const z: [:0]u8 = self.allocator.dupeSentinel(u8, text, 0) catch return null;
    defer self.allocator.free(z);

    const sdl_color: c.SDL_Color = c.SDL_Color{
        .r = color.r,
        .g = color.g,
        .b = color.b,
        .a = color.a,
    };
    const surf: [*c]c.SDL_Surface = c.TTF_RenderUTF8_Blended(font, z.ptr, sdl_color);

    if (surf == null) {
        return null;
    }
    defer c.SDL_FreeSurface(surf);

    out_w.* = surf.*.w;
    out_h.* = surf.*.h;

    const raw: ?*c.SDL_Texture = c.SDL_CreateTextureFromSurface(self.renderer, surf);

    if (raw == null) {
        return null;
    }

    _ = c.SDL_SetTextureBlendMode(raw, c.SDL_BLENDMODE_BLEND);
    return raw;
}

// ── Vtable: lifecycle ────────────────────────────────────────────────────

fn vt_init(ptr: *anyopaque, config: engine.Config) bool {
    const self: *Sdl2Engine = as_self(ptr);
    self.allocator = config.allocator;

    const init_flags: c.Uint32 = c.SDL_INIT_VIDEO | c.SDL_INIT_AUDIO | c.SDL_INIT_TIMER;
    if (!self.sdl.begin(config, init_flags)) {
        engine.log.err("sdl2: SDL_Init failed", .{});
        return false;
    }
    if (c.TTF_Init() != 0) {
        engine.log.err("sdl2: TTF_Init failed", .{});
        return false;
    }
    if ((c.IMG_Init(c.IMG_INIT_PNG) & c.IMG_INIT_PNG) == 0) {
        engine.log.err("sdl2: IMG_Init failed", .{});
        return false;
    }
    if (c.Mix_OpenAudio(44100, @intCast(c.MIX_DEFAULT_FORMAT), 2, 1024) != 0) {
        engine.log.err("sdl2: Mix_OpenAudio failed", .{});
        return false;
    }
    _ = c.Mix_Init(c.MIX_INIT_OGG | c.MIX_INIT_MP3 | c.MIX_INIT_FLAC);
    _ = c.Mix_AllocateChannels(32);

    if (!self.sdl.openWindow(config, c.SDL_WINDOW_SHOWN)) {
        engine.log.err("sdl2: SDL_CreateWindow failed", .{});
        return false;
    }

    if (!create_renderer(self, config.vsync)) {
        engine.log.err("sdl2: SDL_CreateRenderer failed", .{});
        return false;
    }
    _ = c.SDL_SetRenderDrawBlendMode(self.renderer, c.SDL_BLENDMODE_BLEND);
    _ = c.SDL_RenderSetLogicalSize(self.renderer, @intCast(config.width), @intCast(config.height));

    engine.log.info("sdl2: window {d}x{d} ready (vsync {}, renderer)", .{ config.width, config.height, config.vsync });
    engine.attach(self.backend());
    return true;
}

fn vt_shutdown(ptr: *anyopaque) void {
    const self: *Sdl2Engine = as_self(ptr);

    destroy_textures(self);
    destroy_text_cache(self);

    for (self.fonts.items) |font| {
        c.TTF_CloseFont(font);
    }
    self.fonts.deinit(self.allocator);

    var chunks: ChunkMap.ValueIterator = self.chunks.valueIterator();
    while (chunks.next()) |chunk| {
        c.Mix_FreeChunk(chunk.*);
    }
    self.chunks.deinit(self.allocator);

    self.textures.deinit(self.allocator);
    self.text_cache.deinit(self.allocator);

    if (self.renderer != null) {
        c.SDL_DestroyRenderer(self.renderer);
        self.renderer = null;
    }
    self.sdl.closeWindow();

    c.Mix_Quit();
    c.IMG_Quit();
    c.TTF_Quit();
    c.SDL_Quit();

    engine.detach();
}

fn vt_present(ptr: *anyopaque) void {
    const self: *Sdl2Engine = as_self(ptr);
    _ = c.SDL_RenderPresent(self.renderer);
}

// ── Vtable: window ───────────────────────────────────────────────────────

fn vt_set_logical_size(ptr: *anyopaque, width: u32, height: u32) void {
    const self: *Sdl2Engine = as_self(ptr);
    self.sdl.setLogicalSize(width, height);
    _ = c.SDL_RenderSetLogicalSize(self.renderer, @intCast(width), @intCast(height));
}

fn vt_set_vsync(ptr: *anyopaque, on: bool) void {
    const self: *Sdl2Engine = as_self(ptr);

    if (on == self.sdl.vsync) {
        return;
    }

    self.sdl.vsync = on;

    _ = c.SDL_RenderSetLogicalSize(self.renderer, 0, 0);
    if (self.renderer != null) {
        c.SDL_DestroyRenderer(self.renderer);
        self.renderer = null;
    }
    destroy_textures(self);
    destroy_text_cache(self);
    engine.sprite.clear_cache();

    if (!create_renderer(self, on)) {
        if (!create_renderer(self, false)) return;
    }
    _ = c.SDL_SetRenderDrawBlendMode(self.renderer, c.SDL_BLENDMODE_BLEND);
    _ = c.SDL_RenderSetLogicalSize(self.renderer, @intCast(self.sdl.logical_w), @intCast(self.sdl.logical_h));
}

fn vt_set_resolution(ptr: *anyopaque, width: u32, height: u32) void {
    const self: *Sdl2Engine = as_self(ptr);
    self.sdl.setResolution(width, height);
    _ = c.SDL_RenderSetLogicalSize(self.renderer, @intCast(width), @intCast(height));
}

// ── Renderer selection ──────────────────────────────────────────────────────

fn vt_render_name(ptr: *anyopaque) []const u8 {
    const self: *Sdl2Engine = as_self(ptr);
    if (self.renderer == null) return "";
    var info: c.SDL_RendererInfo = undefined;
    if (c.SDL_GetRendererInfo(self.renderer, &info) != 0) return "";
    if (info.name == null) return "";
    return std.mem.span(info.name);
}

fn vt_renderers(ptr: *anyopaque, allocator: std.mem.Allocator) []engine.RenderInfo {
    _ = ptr;
    const n: c_int = c.SDL_GetNumRenderDrivers();
    if (n <= 0) return &.{};
    const out: []engine.RenderInfo = allocator.alloc(engine.RenderInfo, @intCast(n)) catch return &.{};
    var count: usize = 0;
    var i: c_int = 0;
    while (i < n) : (i += 1) {
        var info: c.SDL_RendererInfo = undefined;
        if (c.SDL_GetRenderDriverInfo(i, &info) != 0) continue;
        if (info.name == null) continue;
        out[count] = engine.RenderInfo{ .name = std.mem.span(info.name) };
        count += 1;
    }
    return out[0..count];
}

fn vt_set_renderer(ptr: *anyopaque, name: []const u8) bool {
    const self: *Sdl2Engine = as_self(ptr);

    if (self.renderer != null) {
        _ = c.SDL_RenderSetLogicalSize(self.renderer, 0, 0);
        c.SDL_DestroyRenderer(self.renderer);
        self.renderer = null;
    }
    destroy_textures(self);
    destroy_text_cache(self);
    engine.sprite.clear_cache();

    const z: [:0]u8 = self.allocator.dupeSentinel(u8, name, 0) catch return false;
    defer self.allocator.free(z);
    _ = c.SDL_SetHint("SDL_RENDER_DRIVER", z.ptr);

    if (!create_renderer(self, self.sdl.vsync)) {
        _ = c.SDL_ResetHint("SDL_RENDER_DRIVER");
        if (!create_renderer(self, false)) return false;
    }
    _ = c.SDL_SetRenderDrawBlendMode(self.renderer, c.SDL_BLENDMODE_BLEND);
    _ = c.SDL_RenderSetLogicalSize(self.renderer, @intCast(self.sdl.logical_w), @intCast(self.sdl.logical_h));
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
    const self: *Sdl2Engine = as_self(ptr);
    _ = c.SDL_SetRenderDrawColor(self.renderer, color.r, color.g, color.b, color.a);
    _ = c.SDL_RenderClear(self.renderer);
}

fn vt_draw_rect(ptr: *anyopaque, rect: engine.Rect, color: engine.Color, filled: bool) void {
    const self: *Sdl2Engine = as_self(ptr);

    _ = c.SDL_SetRenderDrawColor(self.renderer, color.r, color.g, color.b, color.a);

    const r: c.SDL_Rect = c.SDL_Rect{ .x = rect.x, .y = rect.y, .w = rect.w, .h = rect.h };

    if (filled) {
        _ = c.SDL_RenderFillRect(self.renderer, &r);
    } else {
        _ = c.SDL_RenderDrawRect(self.renderer, &r);
    }
}

fn vt_draw_line(ptr: *anyopaque, x1: i32, y1: i32, x2: i32, y2: i32, color: engine.Color) void {
    const self: *Sdl2Engine = as_self(ptr);

    _ = c.SDL_SetRenderDrawColor(self.renderer, color.r, color.g, color.b, color.a);

    _ = c.SDL_RenderDrawLine(self.renderer, x1, y1, x2, y2);
}

fn vt_draw_circle(ptr: *anyopaque, cx: i32, cy: i32, radius: i32, color: engine.Color, filled: bool) void {
    const self: *Sdl2Engine = as_self(ptr);

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
            const r: c.SDL_Rect = c.SDL_Rect{ .x = cx - half, .y = cy + dy, .w = w, .h = 1 };
            if (filled) {
                _ = c.SDL_RenderFillRect(self.renderer, &r);
            } else {
                _ = c.SDL_RenderDrawPoint(self.renderer, cx, cy + dy);
            }
        }
    }
}

// ── Vtable: textures ─────────────────────────────────────────────────────

fn vt_load_texture(ptr: *anyopaque, path: []const u8) ?engine.TextureHandle {
    const self: *Sdl2Engine = as_self(ptr);

    const z: [:0]u8 = self.allocator.dupeSentinel(u8, path, 0) catch return null;
    defer self.allocator.free(z);

    const surf: [*c]c.SDL_Surface = c.IMG_Load(z.ptr);
    if (surf == null) return null;
    defer c.SDL_FreeSurface(surf);

    const raw: ?*c.SDL_Texture = c.SDL_CreateTextureFromSurface(self.renderer, surf);
    if (raw == null) return null;
    _ = c.SDL_SetTextureBlendMode(raw, c.SDL_BLENDMODE_BLEND);

    const id: u32 = self.next_id;

    self.next_id += 1;

    self.textures.put(self.allocator, id, raw.?) catch {
        c.SDL_DestroyTexture(raw);
        return null;
    };
    return engine.TextureHandle{ .id = id };
}

fn vt_create_target(ptr: *anyopaque, width: u32, height: u32) ?engine.TextureHandle {
    const self: *Sdl2Engine = as_self(ptr);

    const raw: ?*c.SDL_Texture = c.SDL_CreateTexture(
        self.renderer,
        @intCast(c.SDL_PIXELFORMAT_RGBA8888),
        c.SDL_TEXTUREACCESS_TARGET,
        @intCast(width),
        @intCast(height),
    );

    if (raw == null) {
        return null;
    }

    const id: u32 = self.next_id;

    self.next_id += 1;

    self.textures.put(self.allocator, id, raw.?) catch {
        c.SDL_DestroyTexture(raw);
        return null;
    };
    return engine.TextureHandle{ .id = id };
}

fn vt_create_texture(ptr: *anyopaque, width: u32, height: u32, pixels: ?[]const u8, pitch: u32) ?engine.TextureHandle {
    const self: *Sdl2Engine = as_self(ptr);

    const raw: ?*c.SDL_Texture = c.SDL_CreateTexture(
        self.renderer,
        @intCast(c.SDL_PIXELFORMAT_ARGB8888),
        c.SDL_TEXTUREACCESS_STREAMING,
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

    const id: u32 = self.next_id;
    self.next_id += 1;
    self.textures.put(self.allocator, id, raw.?) catch {
        c.SDL_DestroyTexture(raw);
        return null;
    };
    return engine.TextureHandle{ .id = id };
}

fn vt_update_texture(ptr: *anyopaque, tex: engine.TextureHandle, pixels: []const u8, pitch: u32) void {
    const self: *Sdl2Engine = as_self(ptr);

    const t: ?*c.SDL_Texture = self.textures.get(tex.id) orelse return;

    _ = c.SDL_UpdateTexture(t, null, pixels.ptr, @intCast(pitch));
}

fn vt_draw_texture(ptr: *anyopaque, tex: engine.TextureHandle, dst: engine.Rect, src: ?engine.Rect, alpha: ?u8) void {
    const self: *Sdl2Engine = as_self(ptr);
    const t: ?*c.SDL_Texture = self.textures.get(tex.id) orelse return;

    if (alpha) |a| {
        _ = c.SDL_SetTextureAlphaMod(t, a);
    }

    const dst_rect: c.SDL_Rect = c.SDL_Rect{ .x = dst.x, .y = dst.y, .w = dst.w, .h = dst.h };

    if (src) |s| {
        const src_rect: c.SDL_Rect = c.SDL_Rect{ .x = s.x, .y = s.y, .w = s.w, .h = s.h };
        _ = c.SDL_RenderCopy(self.renderer, t, &src_rect, &dst_rect);
    } else {
        _ = c.SDL_RenderCopy(self.renderer, t, null, &dst_rect);
    }

    if (alpha != null) {
        _ = c.SDL_SetTextureAlphaMod(t, 255);
    }
}

fn vt_draw_texture_rotated(ptr: *anyopaque, tex: engine.TextureHandle, dst: engine.Rect, angle: f32, alpha: ?u8) void {
    const self: *Sdl2Engine = as_self(ptr);
    const t: ?*c.SDL_Texture = self.textures.get(tex.id) orelse return;

    if (alpha) |a| {
        _ = c.SDL_SetTextureAlphaMod(t, a);
    }

    const dst_rect: c.SDL_Rect = c.SDL_Rect{ .x = dst.x, .y = dst.y, .w = dst.w, .h = dst.h };

    _ = c.SDL_RenderCopyEx(self.renderer, t, null, &dst_rect, @floatCast(angle), null, c.SDL_FLIP_NONE);

    if (alpha != null) {
        _ = c.SDL_SetTextureAlphaMod(t, 255);
    }
}

fn vt_texture_size(ptr: *anyopaque, tex: engine.TextureHandle) engine.Point {
    const self: *Sdl2Engine = as_self(ptr);

    const t: *c.SDL_Texture = self.textures.get(tex.id) orelse return engine.Point{ .x = 0, .y = 0 };

    var w: c_int = 0;
    var h: c_int = 0;

    _ = c.SDL_QueryTexture(t, null, null, &w, &h);

    return engine.Point{
        .x = w,
        .y = h,
    };
}

fn vt_geometry(ptr: *anyopaque, tex: engine.TextureHandle, vertices: []const engine.Vertex, indices: []const i32) void {
    const self: *Sdl2Engine = as_self(ptr);

    const t: *c.SDL_Texture = self.textures.get(tex.id) orelse return;

    if (vertices.len == 0 or indices.len == 0) {
        return;
    }

    _ = c.SDL_RenderGeometry(
        self.renderer,
        t,
        @ptrCast(vertices.ptr),
        @intCast(vertices.len),
        @ptrCast(indices.ptr),
        @intCast(indices.len),
    );
}

fn vt_set_render_target(ptr: *anyopaque, target: ?engine.TextureHandle) void {
    const self: *Sdl2Engine = as_self(ptr);

    if (target) |handle| {
        const t: *c.SDL_Texture = self.textures.get(handle.id) orelse return;
        _ = c.SDL_SetRenderTarget(self.renderer, t);
    } else {
        _ = c.SDL_SetRenderTarget(self.renderer, null);
        _ = c.SDL_RenderSetLogicalSize(self.renderer, @intCast(self.sdl.logical_w), @intCast(self.sdl.logical_h));
    }
}

// ── Vtable: text ─────────────────────────────────────────────────────────

fn vt_load_font(ptr: *anyopaque, path: []const u8, size: u16) i64 {
    const self: *Sdl2Engine = as_self(ptr);
    return open_font(self, path, size);
}

fn vt_draw_text(ptr: *anyopaque, text: []const u8, x: i32, y: i32, size: u32, color: engine.Color, center: bool, font_idx: i64) bool {
    const self: *Sdl2Engine = as_self(ptr);
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
        _ = c.SDL_QueryTexture(tex, null, null, &tw, &th);
    }

    const px: c_int = if (center) x - @divTrunc(tw, 2) else x;
    const py: c_int = if (center) y - @divTrunc(th, 2) else y;
    const dst: c.SDL_Rect = c.SDL_Rect{ .x = px, .y = py, .w = tw, .h = th };

    if (color.a != 255) _ = c.SDL_SetTextureAlphaMod(tex, color.a);

    _ = c.SDL_RenderCopy(self.renderer, tex, null, &dst);

    if (color.a != 255) _ = c.SDL_SetTextureAlphaMod(tex, 255);
    return true;
}

fn vt_draw_text_rotated(ptr: *anyopaque, text: []const u8, x: i32, y: i32, size: u32, angle: f32, color: engine.Color, center: bool, font_idx: i64) bool {
    const self: *Sdl2Engine = as_self(ptr);
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
    const dst: c.SDL_Rect = c.SDL_Rect{ .x = px, .y = py, .w = tw, .h = th };
    if (color.a != 255) _ = c.SDL_SetTextureAlphaMod(tex, color.a);
    _ = c.SDL_RenderCopyEx(self.renderer, tex, null, &dst, @floatCast(angle), null, c.SDL_FLIP_NONE);
    return true;
}

fn vt_text_size(ptr: *anyopaque, text: []const u8, font_idx: u32) ?engine.Point {
    const self: *Sdl2Engine = as_self(ptr);
    if (font_idx >= self.fonts.items.len) return null;

    const z: [:0]u8 = self.allocator.dupeSentinel(u8, text, 0) catch return null;
    defer self.allocator.free(z);

    var w: c_int = 0;
    var h: c_int = 0;
    if (c.TTF_SizeUTF8(self.fonts.items[font_idx], z.ptr, &w, &h) != 0) return null;
    return engine.Point{ .x = w, .y = h };
}

// ── Vtable: sound ────────────────────────────────────────────────────────

fn vt_load_sound(ptr: *anyopaque, path: []const u8) ?engine.SoundHandle {
    const self: *Sdl2Engine = as_self(ptr);

    const z: [:0]u8 = self.allocator.dupeSentinel(u8, path, 0) catch return null;
    defer self.allocator.free(z);

    const chunk: [*c]c.Mix_Chunk = c.Mix_LoadWAV(z.ptr);
    if (chunk == null) return null;

    const id: u32 = self.next_id;
    self.next_id += 1;
    self.chunks.put(self.allocator, id, chunk) catch {
        c.Mix_FreeChunk(chunk);
        return null;
    };
    return engine.SoundHandle{ .id = id };
}

fn vt_play_sound(ptr: *anyopaque, snd: engine.SoundHandle, loops: i32, channel: i32) i32 {
    const self: *Sdl2Engine = as_self(ptr);
    const chunk: *c.Mix_Chunk = self.chunks.get(snd.id) orelse return channel;
    _ = c.Mix_PlayChannel(channel, chunk, loops);
    return channel;
}

fn vt_stop_channel(ptr: *anyopaque, channel: i32) void {
    _ = ptr;
    _ = c.Mix_HaltChannel(channel);
}

fn vt_stop_all_sounds(ptr: *anyopaque) void {
    _ = ptr;
    _ = c.Mix_HaltChannel(-1);
}

fn vt_set_master_volume(ptr: *anyopaque, vol: i32) void {
    const self: *Sdl2Engine = as_self(ptr);
    self.master_vol = vol;
    const music_eff: i32 = @divTrunc(self.master_vol * self.music_vol * c.MIX_MAX_VOLUME, 10000);
    const sfx_eff: i32 = @divTrunc(self.master_vol * self.sfx_vol * c.MIX_MAX_VOLUME, 10000);
    _ = c.Mix_VolumeMusic(music_eff);
    var ch: i32 = 0;
    while (ch < 16) : (ch += 1) {
        _ = c.Mix_Volume(ch, sfx_eff);
    }
}

fn vt_set_sfx_volume(ptr: *anyopaque, vol: i32) void {
    const self: *Sdl2Engine = as_self(ptr);
    self.sfx_vol = vol;
    const eff: i32 = @divTrunc(self.master_vol * self.sfx_vol * c.MIX_MAX_VOLUME, 10000);
    var ch: i32 = 0;
    while (ch < 16) : (ch += 1) {
        _ = c.Mix_Volume(ch, eff);
    }
}

fn vt_set_music_volume(ptr: *anyopaque, vol: i32) void {
    const self: *Sdl2Engine = as_self(ptr);
    self.music_vol = vol;
    const eff: i32 = @divTrunc(self.master_vol * self.music_vol * c.MIX_MAX_VOLUME, 10000);
    _ = c.Mix_VolumeMusic(eff);
}

// ── Vtable: misc ─────────────────────────────────────────────────────────

fn vt_update_discord(ptr: *anyopaque, details: []const u8, state: []const u8) void {
    _ = ptr;
    _ = details;
    _ = state;
}
