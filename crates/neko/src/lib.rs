//! # Neko
//!
//! A modular game engine. Every platform, renderer and audio backend is a
//! separate crate behind a Cargo feature of this crate, so a game only
//! compiles what it uses:
//!
//! ```toml
//! neko = { version = "0.1", default-features = false, features = ["window-sdl3", "render-wgpu"] }
//! ```
//!
//! ## Porting
//!
//! A port to a new platform implements the backend traits, like Godot's
//! `platform/` and `drivers/` folders:
//!
//! | Trait | Godot equivalent | Crate |
//! |---|---|---|
//! | [`os::Os`], [`os::FileSystem`] | `OS`, `FileAccess`, `DirAccess` | `neko-os` |
//! | [`platform::Platform`], [`platform::Window`] | `DisplayServer` | `neko-platform` |
//! | [`gpu::GpuBackend`] | `RenderingDeviceDriver` | `neko-gpu` |
//! | [`audio::AudioBackend`] | `AudioDriver` | `neko-audio` |
//!
//! See `docs/PORTING.md`.

use std::sync::Arc;

pub mod scene_runner;
pub mod script_host;

pub use neko_audio as audio;
pub use neko_core as core;
pub use neko_gpu as gpu;
pub use neko_os as os;
pub use neko_platform as platform;
pub use neko_script as script;
pub use neko_project as project;
pub use neko_render as render;
pub use neko_ui as ui;

use neko_platform::{App, Event, Flow, GamepadButton, GamepadEvent, Platform, Window, WindowConfig};
use neko_project::{BackendChoice, ProjectConfig, RenderBackend, WindowBackend};
use neko_render::{Frame, Renderer};

#[cfg(feature = "render-wgpu")]
pub use neko_wgpu::GpuApi;

/// Window backends compiled into this build.
#[allow(clippy::vec_init_then_push)] // pushes are feature-gated
pub fn available_window_backends() -> Vec<WindowBackend> {
    // Native windowing first, like Godot's display servers; SDL3 windows are the fallback.
    let mut v = Vec::new();
    #[cfg(feature = "window-winit")]
    v.push(WindowBackend::Winit);
    #[cfg(feature = "window-sdl3")]
    v.push(WindowBackend::Sdl3);
    v
}

/// Render backends compiled into this build.
#[allow(clippy::vec_init_then_push)] // pushes are feature-gated
pub fn available_render_backends() -> Vec<RenderBackend> {
    let mut v = Vec::new();
    #[cfg(feature = "render-wgpu")]
    v.push(RenderBackend::Wgpu);
    #[cfg(feature = "render-soft")]
    v.push(RenderBackend::Soft);
    v
}

/// Create a platform backend by id. Errors name the missing feature or library.
pub fn create_platform(kind: WindowBackend) -> Result<Box<dyn Platform>, String> {
    match kind {
        #[cfg(feature = "window-sdl3")]
        WindowBackend::Sdl3 => neko_sdl3::Sdl3Platform::new().map(|p| Box::new(p) as Box<dyn Platform>).map_err(|e| e.to_string()),
        #[cfg(feature = "window-winit")]
        WindowBackend::Winit => Ok(Box::new(neko_winit::WinitPlatform::new())),
        other => Err(not_built(other)),
    }
}

/// Options for [`create_renderer`].
#[derive(Clone, Copy, Debug, Default)]
pub struct RenderOptions {
    #[cfg(feature = "render-wgpu")]
    pub gpu_api: GpuApi,
}

/// Create a renderer for `window`.
/// The renderer and options for a project's [`neko_project::RenderingDriver`].
pub fn renderer_for(driver: neko_project::RenderingDriver) -> (RenderBackend, RenderOptions) {
    use neko_project::RenderingDriver as D;
    #[cfg(feature = "render-wgpu")]
    let gpu_api = match driver {
        D::Vulkan => GpuApi::Vulkan,
        D::D3d12 => GpuApi::Dx12,
        D::Metal => GpuApi::Metal,
        D::OpenGl => GpuApi::Gl,
        D::Software => GpuApi::Auto,
    };
    let kind = if driver == D::Software { RenderBackend::Soft } else { RenderBackend::Wgpu };
    (kind, RenderOptions {
        #[cfg(feature = "render-wgpu")]
        gpu_api,
    })
}

pub fn create_renderer(
    kind: RenderBackend,
    window: Arc<dyn Window>,
    _opts: RenderOptions,
) -> Result<Box<dyn Renderer>, String> {
    match kind {
        #[cfg(feature = "render-wgpu")]
        RenderBackend::Wgpu => neko_wgpu::WgpuRenderer::new(window, _opts.gpu_api)
            .map(|r| Box::new(r) as Box<dyn Renderer>)
            .map_err(|e| e.to_string()),
        #[cfg(feature = "render-soft")]
        RenderBackend::Soft => neko_soft::SoftRenderer::new(window).map(|r| Box::new(r) as Box<dyn Renderer>).map_err(|e| e.to_string()),
        other => {
            let _ = window;
            Err(not_built(other))
        }
    }
}

fn not_built<B: BackendChoice + std::fmt::Debug>(b: B) -> String {
    match (b.implemented(), b.feature()) {
        (true, Some(f)) => format!("{} backend is not compiled in; enable the `{f}` feature of `neko`", b.label()),
        _ => format!("{} backend is planned but not implemented yet", b.label()),
    }
}

/// Run a game described by the contents of its `neko.toml`.
///
/// This is the entry point generated projects call. For now it opens a window
/// with the configured backends and clears it; scenes come next.
pub fn run(neko_toml: &str) {
    let config: ProjectConfig = match neko_project::config_from_str(neko_toml) {
        Ok(c) => c,
        Err(e) => fatal(&format!("invalid neko.toml: {e}")),
    };
    // Like Godot's command line: an optional scene path (`res://...` or a
    // `.scn` file) runs that scene instead of the main one, and
    // `--debug-collisions` shows collision shapes.
    let args: Vec<String> = std::env::args().skip(1).collect();
    let scene_arg = args.iter().find(|a| a.starts_with("res://") || a.ends_with(".scn")).cloned();
    let debug_collisions = args.iter().any(|a| a == "--debug-collisions");
    let project_dir = std::env::current_dir().unwrap_or_else(|_| ".".into());
    let mut scene_path = scene_arg.unwrap_or_else(|| config.project.main_scene.clone());
    if scene_path.starts_with("uid://") {
        scene_path = neko_project::resolve_uid(&project_dir, &scene_path).unwrap_or(scene_path);
    }
    if scene_path.is_empty() {
        fatal("no main scene defined (set main_scene in neko.toml)");
    }
    let mut scene = scene_runner::SceneRunner::load(&project_dir, &scene_path).unwrap_or_else(|e| fatal(&e));
    scene.debug_collisions = debug_collisions;
    println!("Running scene {scene_path} ({})", scene.root_name());

    let platform = create_platform(config.backends.window).unwrap_or_else(|e| fatal(&e));
    // Godot's display/window/size and default clear color, when the project sets them.
    let setting = |key: &str| config.file.get("display", key).and_then(|v| v.as_i64()).filter(|&v| v > 0).map(|v| v as u32);
    let vw = setting("window/size/window_width_override").or(setting("window/size/viewport_width")).unwrap_or(scene_runner::VIEWPORT_SIZE.0);
    let vh = setting("window/size/window_height_override").or(setting("window/size/viewport_height")).unwrap_or(scene_runner::VIEWPORT_SIZE.1);
    let clear_color = match config.file.get("rendering", "environment/defaults/default_clear_color").and_then(|v| v.numbers("Color")) {
        Some(c) if c.len() == 4 => [c[0] as f32, c[1] as f32, c[2] as f32, c[3] as f32],
        _ => scene_runner::CLEAR_COLOR,
    };
    let window = WindowConfig { title: format!("{} (DEBUG)", config.project.name), width: vw, height: vh, ..Default::default() };
    let splash = &config.boot_splash;
    let [r, g, b] = splash.bg_color.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
    let mut ui = neko_ui::Ui::new(neko_ui::Theme::neko_dark());
    let root = ui.root();
    ui.add(root, neko_ui::splash::splash_control(neko_ui::Color32::from_rgb(r, g, b), splash.show_image, "Made with Neko"));
    let mut game_ui = neko_ui::Ui::new(neko_ui::Theme::neko_dark());
    // The game window shows the clear color, like a Godot Viewport.
    game_ui.theme.background = neko_ui::Color32::TRANSPARENT;
    let game_root = game_ui.root();
    let canvas = game_ui.add(game_root, neko_ui::Control::canvas());
    let app = GameApp {
        scene,
        game_ui,
        canvas,
        clear_color,
        frames: 0,
        settings: config.file.clone(),
        scripts_started: false,
        last_frame: None,
        scale: 1.0,
        render: renderer_for(config.rendering_driver()),
        window: None,
        renderer: None,
        exit: false,
        splash: Some(ui),
        splash_time: std::time::Duration::from_millis(splash.minimum_display_time_ms),
        splash_until: None,
    };
    if let Err(e) = platform.run(window, Box::new(app)) {
        fatal(&e.to_string());
    }
}

fn fatal(msg: &str) -> ! {
    eprintln!("neko: {msg}");
    std::process::exit(1)
}

struct GameApp {
    scene: scene_runner::SceneRunner,
    /// Draws the running scene into a full-window canvas.
    game_ui: neko_ui::Ui,
    canvas: neko_ui::Id,
    clear_color: [f32; 4],
    frames: u32,
    /// Project settings (`[input]`, `[autoload]`...), for the scripts.
    settings: neko_core::godot::config::ConfigFile,
    scripts_started: bool,
    last_frame: Option<std::time::Instant>,
    scale: f32,
    /// The project's rendering driver (`rendering/rendering_device/driver`...).
    render: (RenderBackend, RenderOptions),
    window: Option<Arc<dyn Window>>,
    renderer: Option<Box<dyn Renderer>>,
    exit: bool,
    /// Boot splash, drawn with neko-ui until `splash_until`.
    splash: Option<neko_ui::Ui>,
    splash_time: std::time::Duration,
    splash_until: Option<std::time::Instant>,
}

impl App for GameApp {
    fn resumed(&mut self, window: Arc<dyn Window>) {
        let (kind, opts) = self.render;
        // Like Godot's `rendering/rendering_device/fallback_to_opengl3`: if the
        // chosen API is missing, try the others before giving up.
        let made = create_renderer(kind, window.clone(), opts).or_else(|e| {
            log::warn!("{} renderer failed ({e}); trying the others", kind.label());
            create_renderer(kind, window.clone(), RenderOptions::default()).or_else(|_| create_renderer(RenderBackend::Soft, window.clone(), RenderOptions::default()))
        });
        match made {
            Ok(r) => {
                let i = r.info();
                log::info!("renderer: {} {} ({})", i.backend, i.api, i.device);
                self.renderer = Some(r);
            }
            Err(e) => fatal(&e),
        }
        self.window = Some(window);
    }

    fn event(&mut self, event: Event) {
        match event {
            // Godot games do not quit on Escape; only closing the window does.
            Event::CloseRequested => self.exit = true,
            Event::Resized { width, height } => {
                if let Some(r) = &mut self.renderer {
                    r.resize(width, height);
                }
            }
            Event::ScaleFactorChanged(s) => self.scale = s,
            Event::Key { key, pressed, repeat, .. } => {
                let code = script_host::godot_keycode(key);
                if code != 0 {
                    self.scene.key(code, pressed, repeat);
                }
            }
            Event::MouseMoved { x, y } => self.scene.mouse_moved(x / self.scale, y / self.scale),
            Event::Gamepad { event, .. } => match event {
                GamepadEvent::Connected(info) => log::info!("gamepad connected: {}", info.name),
                GamepadEvent::Button { button, pressed } => {
                    // Godot's JoyButton indices (SDL order).
                    let index = match button {
                        GamepadButton::South => 0,
                        GamepadButton::East => 1,
                        GamepadButton::West => 2,
                        GamepadButton::North => 3,
                        GamepadButton::Back => 4,
                        GamepadButton::Guide => 5,
                        GamepadButton::Start => 6,
                        GamepadButton::LeftStick => 7,
                        GamepadButton::RightStick => 8,
                        GamepadButton::LeftShoulder => 9,
                        GamepadButton::RightShoulder => 10,
                        GamepadButton::DpadUp => 11,
                        GamepadButton::DpadDown => 12,
                        GamepadButton::DpadLeft => 13,
                        GamepadButton::DpadRight => 14,
                        _ => -1,
                    };
                    if index >= 0 {
                        let mut w = self.scene.world.borrow_mut();
                        if pressed {
                            w.input.joy_buttons.insert(index);
                            w.input.just_pressed.insert(script_host::JOY_BASE + index);
                        } else {
                            w.input.joy_buttons.remove(&index);
                            w.input.just_released.insert(script_host::JOY_BASE + index);
                        }
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn frame(&mut self) -> Flow {
        if self.exit {
            return Flow::Exit;
        }
        if let (Some(w), Some(r), Some(ui)) = (&self.window, &mut self.renderer, &mut self.splash) {
            let until = *self.splash_until.get_or_insert_with(|| std::time::Instant::now() + self.splash_time);
            if std::time::Instant::now() < until {
                let (wd, ht) = w.inner_size();
                let mut out = ui.frame((wd, ht), w.scale_factor(), r.max_texture_side());
                let frame = Frame { clear_color: [0.0, 0.0, 0.0, 1.0], size_px: [wd, ht], pixels_per_point: w.scale_factor(), textures: &out.textures, ui: &out.primitives };
                if let Err(e) = r.render(&frame) {
                    log::error!("render failed: {e}");
                }
                out.textures.clear();
                return Flow::WaitUntil(until);
            }
            self.splash = None;
        }
        // Scripts start once the splash is gone, like Godot's main loop.
        if !self.scripts_started {
            self.scripts_started = true;
            self.scene.start_scripts(&self.settings);
        }
        let now = std::time::Instant::now();
        let delta = self.last_frame.map(|t| (now - t).as_secs_f64()).unwrap_or(1.0 / 60.0).min(0.25);
        self.last_frame = Some(now);
        if !self.scene.update(delta) {
            return Flow::Exit;
        }
        if let (Some(w), Some(r)) = (&self.window, &mut self.renderer) {
            let (wd, ht) = w.inner_size();
            let ppp = w.scale_factor();
            self.scene.world.borrow_mut().viewport_size = [wd as f32 / ppp, ht as f32 / ppp];
            // Paint into the canvas laid out last frame (empty on the very first one).
            self.scene.draw(&mut self.game_ui, self.canvas);
            let mut out = self.game_ui.frame((wd, ht), ppp, r.max_texture_side());
            let frame = Frame { clear_color: self.clear_color, size_px: [wd, ht], pixels_per_point: ppp, textures: &out.textures, ui: &out.primitives };
            if let Err(e) = r.render(&frame) {
                log::error!("render failed: {e}");
            }
            out.textures.clear();
        }
        // Scripted scenes run every frame; static ones redraw only on events.
        self.frames += 1;
        if self.frames < 3 || self.scene.has_scripts() { Flow::Poll } else { Flow::Wait }
    }
}
