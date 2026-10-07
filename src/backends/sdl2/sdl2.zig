//! SDL2 backend for the engine. This is the only module that includes or
//! calls SDL. It implements the `engine.Backend` vtable faithfully to the
//! C++ `EngineSDL2`.

const std: type = @import("std");
const c: type = @import("c");
const engine: type = @import("neko");

const Allocator: type = std.mem.Allocator;
const mem: type = std.mem;
const fmt: type = std.fmt;
const TextureMap: type = std.AutoHashMapUnmanaged(u32, *c.SDL_Texture);
const TextCacheMap: type = std.AutoHashMapUnmanaged(u64, *c.SDL_Texture);
const ChunkMap: type = std.AutoHashMapUnmanaged(u32, *c.Mix_Chunk);
const FontList: type = std.ArrayListUnmanaged(*c.TTF_Font);
const DisplayModeList: type = std.ArrayListUnmanaged(engine.DisplayMode);

/// Concrete SDL2 backend state. One instance per window.
pub const Sdl2Engine: type = struct {
    allocator: Allocator = undefined,
    io: std.Io = undefined,
    assets_dir: []u8 = &.{},
    window: ?*c.SDL_Window = null,
    renderer: ?*c.SDL_Renderer = null,

    textures: TextureMap = .empty,
    text_cache: TextCacheMap = .empty,
    chunks: ChunkMap = .empty,
    fonts: FontList = .empty,

    next_id: u32 = 1,
    logical_w: u32 = 0,
    logical_h: u32 = 0,
    running: bool = false,
    vsync: bool = false,

    master_vol: i32 = 80,
    sfx_vol: i32 = 100,
    music_vol: i32 = 70,

    /// Wraps this backend into the type-erased `engine.Backend` handle.
    pub fn backend(self: *Sdl2Engine) engine.Backend {
        return engine.Backend{
            .ptr = @ptrCast(self),
            .vtable = &vtable,
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

/// Which backend this module implements. Checked against `Config.backend`.
pub const kind: engine.BackendKind = .sdl2;

// ── Helpers ──────────────────────────────────────────────────────────────

const vtable: engine.Backend.VTable = engine.Backend.VTable{
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
    .read_file = vt_read_file,
    .write_file = vt_write_file,
    .delete_file = vt_delete_file,
    .file_exists = vt_file_exists,
    .mouse_pos = vt_mouse_pos,
    .update_discord = vt_update_discord,
};

fn as_self(ptr: *anyopaque) *Sdl2Engine {
    return @ptrCast(@alignCast(ptr));
}

/// Views a C string returned by SDL as a Zig slice (SDL strings are static).
fn cstr(ptr: [*c]const u8) []const u8 {
    if (ptr == null) return "";
    const sentinel: [*:0]const u8 = @ptrCast(ptr);
    return mem.span(sentinel);
}

/// Maps an SDL key symbol onto our convenience key names.
fn map_key(sym: c.SDL_Keycode) engine.Key {
    return switch (sym) {
        c.SDLK_UP => engine.Key.up,
        c.SDLK_DOWN => engine.Key.down,
        c.SDLK_LEFT => engine.Key.left,
        c.SDLK_RIGHT => engine.Key.right,
        c.SDLK_RETURN, c.SDLK_RETURN2 => engine.Key.enter,
        c.SDLK_ESCAPE => engine.Key.escape,
        c.SDLK_SPACE => engine.Key.space,
        c.SDLK_TAB => engine.Key.tab,
        c.SDLK_BACKSPACE => engine.Key.backspace,
        c.SDLK_a => engine.Key.a,
        c.SDLK_b => engine.Key.b,
        c.SDLK_c => engine.Key.c,
        c.SDLK_d => engine.Key.d,
        c.SDLK_e => engine.Key.e,
        c.SDLK_f => engine.Key.f,
        c.SDLK_g => engine.Key.g,
        c.SDLK_h => engine.Key.h,
        c.SDLK_i => engine.Key.i,
        c.SDLK_j => engine.Key.j,
        c.SDLK_k => engine.Key.k,
        c.SDLK_l => engine.Key.l,
        c.SDLK_m => engine.Key.m,
        c.SDLK_n => engine.Key.n,
        c.SDLK_o => engine.Key.o,
        c.SDLK_p => engine.Key.p,
        c.SDLK_q => engine.Key.q,
        c.SDLK_r => engine.Key.r,
        c.SDLK_s => engine.Key.s,
        c.SDLK_t => engine.Key.t,
        c.SDLK_u => engine.Key.u,
        c.SDLK_v => engine.Key.v,
        c.SDLK_w => engine.Key.w,
        c.SDLK_x => engine.Key.x,
        c.SDLK_y => engine.Key.y,
        c.SDLK_z => engine.Key.z,
        c.SDLK_0 => engine.Key.number_0,
        c.SDLK_1 => engine.Key.number_1,
        c.SDLK_2 => engine.Key.number_2,
        c.SDLK_3 => engine.Key.number_3,
        c.SDLK_4 => engine.Key.number_4,
        c.SDLK_5 => engine.Key.number_5,
        c.SDLK_6 => engine.Key.number_6,
        c.SDLK_7 => engine.Key.number_7,
        c.SDLK_8 => engine.Key.number_8,
        c.SDLK_9 => engine.Key.number_9,
        else => engine.Key.unknown,
    };
}

/// Maps an SDL mouse button onto our names.
fn map_button(button: c.Uint8) engine.MouseButton {
    return switch (button) {
        c.SDL_BUTTON_LEFT => engine.MouseButton.left,
        c.SDL_BUTTON_MIDDLE => engine.MouseButton.middle,
        c.SDL_BUTTON_RIGHT => engine.MouseButton.right,
        c.SDL_BUTTON_X1 => engine.MouseButton.x1,
        c.SDL_BUTTON_X2 => engine.MouseButton.x2,
        else => engine.MouseButton.unknown,
    };
}

fn make_key_event(raw: c.SDL_Event) engine.Event.KeyEvent {
    const sym: c.SDL_Keycode = raw.key.keysym.sym;
    return engine.Event.KeyEvent{
        .code = sym,
        .key = map_key(sym),
        .name = cstr(c.SDL_GetKeyName(sym)),
        .scan_name = cstr(c.SDL_GetScancodeName(raw.key.keysym.scancode)),
    };
}

fn make_button(raw: c.SDL_Event) engine.Event.Button {
    return engine.Event.Button{
        .button = map_button(raw.button.button),
        .x = raw.button.x,
        .y = raw.button.y,
    };
}

// ── Sdl2Engine methods ───────────────────────────────────────────────────

fn create_renderer(self: *Sdl2Engine, vsync: bool) bool {
    var flags: c.Uint32 = @intCast(c.SDL_RENDERER_ACCELERATED);
    if (vsync) {
        flags |= @intCast(c.SDL_RENDERER_PRESENTVSYNC);
    }
    self.renderer = c.SDL_CreateRenderer(self.window, -1, flags);

    // No GPU / no accelerated driver (headless CI, `SDL_VIDEODRIVER=dummy`):
    // fall back to the software renderer instead of failing to start.
    if (self.renderer == null) {
        self.renderer = c.SDL_CreateRenderer(self.window, -1, @intCast(c.SDL_RENDERER_SOFTWARE));
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
    return fmt.allocPrint(self.allocator, "{s}/font/font.ttf", .{self.assets_dir}) catch null;
}

/// Opens a font and stores it, returning its index (-1 on failure).
fn open_font(self: *Sdl2Engine, path: []const u8, size: u16) i64 {
    const z: [:0]u8 = self.allocator.dupeZ(u8, path) catch return -1;
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
    const z: [:0]u8 = self.allocator.dupeZ(u8, text) catch return null;
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
    self.io = config.io;
    self.assets_dir = self.allocator.dupe(u8, config.assets_dir) catch return false;

    const init_flags: c.Uint32 = c.SDL_INIT_VIDEO | c.SDL_INIT_AUDIO | c.SDL_INIT_TIMER;
    if (c.SDL_Init(init_flags) != 0) return false;
    if (c.TTF_Init() != 0) return false;
    if ((c.IMG_Init(c.IMG_INIT_PNG) & c.IMG_INIT_PNG) == 0) return false;
    if (c.Mix_OpenAudio(44100, @intCast(c.MIX_DEFAULT_FORMAT), 2, 1024) != 0) return false;
    _ = c.Mix_Init(c.MIX_INIT_OGG | c.MIX_INIT_MP3 | c.MIX_INIT_FLAC);
    _ = c.Mix_AllocateChannels(32);

    const title_z: [:0]u8 = self.allocator.dupeZ(u8, config.title) catch return false;
    defer self.allocator.free(title_z);

    self.window = c.SDL_CreateWindow(
        title_z.ptr,
        c.SDL_WINDOWPOS_CENTERED,
        c.SDL_WINDOWPOS_CENTERED,
        @intCast(config.width),
        @intCast(config.height),
        c.SDL_WINDOW_SHOWN,
    );
    if (self.window == null) {
        return false;
    }

    if (config.fullscreen) {
        _ = c.SDL_SetWindowFullscreen(self.window, c.SDL_WINDOW_FULLSCREEN_DESKTOP);
    }

    if (!create_renderer(self, config.vsync)) return false;
    _ = c.SDL_SetRenderDrawBlendMode(self.renderer, c.SDL_BLENDMODE_BLEND);
    _ = c.SDL_RenderSetLogicalSize(self.renderer, @intCast(config.width), @intCast(config.height));

    self.logical_w = config.width;
    self.logical_h = config.height;
    self.running = true;
    self.vsync = config.vsync;

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
    if (self.window != null) {
        c.SDL_DestroyWindow(self.window);
        self.window = null;
    }

    if (self.assets_dir.len > 0) {
        self.allocator.free(self.assets_dir);
        self.assets_dir = &.{};
    }

    c.Mix_Quit();
    c.IMG_Quit();
    c.TTF_Quit();
    c.SDL_Quit();

    self.running = false;
    engine.detach();
}

fn vt_keeps_running(ptr: *anyopaque) bool {
    const self: *Sdl2Engine = as_self(ptr);
    return self.running;
}

fn vt_request_stop(ptr: *anyopaque) void {
    const self: *Sdl2Engine = as_self(ptr);
    self.running = false;
}

fn vt_present(ptr: *anyopaque) void {
    const self: *Sdl2Engine = as_self(ptr);
    _ = c.SDL_RenderPresent(self.renderer);
}

// ── Vtable: events and timing ────────────────────────────────────────────

fn vt_poll_event(ptr: *anyopaque) ?engine.Event {
    _ = ptr;

    var raw: c.SDL_Event = undefined;
    while (c.SDL_PollEvent(&raw) != 0) {
        const event_type: c_int = @intCast(raw.type);
        switch (event_type) {
            c.SDL_QUIT => return engine.Event.quit,
            c.SDL_KEYDOWN => return engine.Event{ .key_down = make_key_event(raw) },
            c.SDL_KEYUP => return engine.Event{ .key_up = make_key_event(raw) },
            c.SDL_MOUSEBUTTONDOWN => return engine.Event{ .mouse_button_down = make_button(raw) },
            c.SDL_MOUSEBUTTONUP => return engine.Event{ .mouse_button_up = make_button(raw) },
            c.SDL_MOUSEMOTION => return engine.Event{
                .mouse_motion = engine.Event.Motion{ .x = raw.motion.x, .y = raw.motion.y },
            },
            c.SDL_MOUSEWHEEL => return engine.Event{
                .mouse_wheel = engine.Event.Wheel{
                    .x = @floatFromInt(raw.wheel.x),
                    .y = @floatFromInt(raw.wheel.y),
                },
            },
            else => {},
        }
    }
    return null;
}

fn vt_ticks_ms(ptr: *anyopaque) u64 {
    _ = ptr;
    return c.SDL_GetTicks64();
}

// ── Vtable: window ───────────────────────────────────────────────────────

fn vt_set_title(ptr: *anyopaque, title: []const u8) void {
    const self: *Sdl2Engine = as_self(ptr);
    if (self.window == null) return;
    const z: [:0]u8 = self.allocator.dupeZ(u8, title) catch return;
    defer self.allocator.free(z);
    c.SDL_SetWindowTitle(self.window, z.ptr);
}

fn vt_set_logical_size(ptr: *anyopaque, width: u32, height: u32) void {
    const self: *Sdl2Engine = as_self(ptr);
    _ = c.SDL_RenderSetLogicalSize(self.renderer, @intCast(width), @intCast(height));
}

fn vt_set_fullscreen(ptr: *anyopaque, on: bool) void {
    const self: *Sdl2Engine = as_self(ptr);
    _ = c.SDL_SetWindowFullscreen(self.window, if (on) c.SDL_WINDOW_FULLSCREEN_DESKTOP else 0);
}

fn vt_set_vsync(ptr: *anyopaque, on: bool) void {
    const self: *Sdl2Engine = as_self(ptr);

    if (on == self.vsync) {
        return;
    }

    self.vsync = on;

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
    _ = c.SDL_RenderSetLogicalSize(self.renderer, @intCast(self.logical_w), @intCast(self.logical_h));
}

fn vt_set_resolution(ptr: *anyopaque, width: u32, height: u32) void {
    const self: *Sdl2Engine = as_self(ptr);
    c.SDL_SetWindowSize(self.window, @intCast(width), @intCast(height));
    _ = c.SDL_RenderSetLogicalSize(self.renderer, @intCast(width), @intCast(height));
}

fn vt_logical_size(ptr: *anyopaque) engine.Point {
    const self: *Sdl2Engine = as_self(ptr);

    return engine.Point{
        .x = @intCast(self.logical_w),
        .y = @intCast(self.logical_h),
    };
}

fn vt_display_modes(ptr: *anyopaque, allocator: Allocator) []engine.DisplayMode {
    _ = ptr;

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

fn vt_supports_curved_panorama(ptr: *anyopaque) bool {
    _ = ptr;
    return true;
}

fn vt_supports_offscreen_targets(ptr: *anyopaque) bool {
    _ = ptr;
    return true;
}

fn vt_set_draw_offset(ptr: *anyopaque, dx: i32, dy: i32) void {
    _ = ptr;
    _ = dx;
    _ = dy;
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

    const z: [:0]u8 = self.allocator.dupeZ(u8, path) catch return null;
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
        _ = c.SDL_RenderSetLogicalSize(self.renderer, @intCast(self.logical_w), @intCast(self.logical_h));
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

    const z: [:0]u8 = self.allocator.dupeZ(u8, text) catch return null;
    defer self.allocator.free(z);

    var w: c_int = 0;
    var h: c_int = 0;
    if (c.TTF_SizeUTF8(self.fonts.items[font_idx], z.ptr, &w, &h) != 0) return null;
    return engine.Point{ .x = w, .y = h };
}

// ── Vtable: sound ────────────────────────────────────────────────────────

fn vt_load_sound(ptr: *anyopaque, path: []const u8) ?engine.SoundHandle {
    const self: *Sdl2Engine = as_self(ptr);

    const z: [:0]u8 = self.allocator.dupeZ(u8, path) catch return null;
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

// ── Vtable: files ─────────────────────────────────────────────────────────
//
// The core never touches the host filesystem: save files and asset access are
// requested through the backend, which owns `std.Io` on the desktop.

fn vt_read_file(ptr: *anyopaque, allocator: Allocator, dir_path: []const u8, file_name: []const u8, max: usize) ?[]u8 {
    const self: *Sdl2Engine = as_self(ptr);
    const dir: std.Io.Dir = std.Io.Dir.cwd().createDirPathOpen(self.io, dir_path, .{}) catch return null;
    return dir.readFileAlloc(self.io, file_name, allocator, .limited(max)) catch null;
}

fn vt_write_file(ptr: *anyopaque, dir_path: []const u8, file_name: []const u8, data: []const u8) bool {
    const self: *Sdl2Engine = as_self(ptr);
    const dir: std.Io.Dir = std.Io.Dir.cwd().createDirPathOpen(self.io, dir_path, .{}) catch return false;
    dir.writeFile(self.io, .{ .sub_path = file_name, .data = data }) catch return false;
    return true;
}

fn vt_delete_file(ptr: *anyopaque, dir_path: []const u8, file_name: []const u8) void {
    const self: *Sdl2Engine = as_self(ptr);
    const dir: std.Io.Dir = std.Io.Dir.cwd().createDirPathOpen(self.io, dir_path, .{}) catch return;
    dir.deleteFile(self.io, file_name) catch {};
}

fn vt_file_exists(ptr: *anyopaque, dir_path: []const u8, file_name: []const u8) bool {
    const self: *Sdl2Engine = as_self(ptr);
    const dir: std.Io.Dir = std.Io.Dir.cwd().createDirPathOpen(self.io, dir_path, .{}) catch return false;
    const f: std.Io.File = dir.openFile(self.io, file_name, .{}) catch return false;
    f.close(self.io);
    return true;
}

// ── Vtable: input and misc ───────────────────────────────────────────────

fn vt_mouse_pos(ptr: *anyopaque) engine.Point {
    _ = ptr;
    var x: c_int = 0;
    var y: c_int = 0;
    _ = c.SDL_GetMouseState(&x, &y);
    return engine.Point{ .x = x, .y = y };
}

fn vt_update_discord(ptr: *anyopaque, details: []const u8, state: []const u8) void {
    _ = ptr;
    _ = details;
    _ = state;
}
