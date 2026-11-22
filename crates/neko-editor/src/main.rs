//! `neko`: the Neko editor. Every screen is drawn by `neko-ui`, Neko's own GUI.
//!
//! ```text
//! neko [--window sdl3|winit] [--render wgpu|soft] [--gpu auto|vulkan|gl|metal|dx12]
//! neko --open path/to/project [--add-node]
//! neko --screenshot out.png [--size 1280x800] [--scale 1.0] [--new-dialog] [--splash]
//! ```
//!
//! In screenshot mode, `NEKO_CLICKS="x,y;x,y"` replays left clicks first
//! (`r x,y` right click, `d x,y` double click). `NEKO_TYPE="text"` then types
//! text (`\n` is Enter) into the focused control.

mod create_node;
mod editor;
mod file_dialog;
mod icons;
mod node_icons;
mod pm_native;
mod script_completion;
mod script_highlight;
mod theme;
mod ui_kit;
mod util;

use std::path::PathBuf;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use neko::platform::{App, Event, Flow, GamepadEvent, Key, MouseButton, Window, WindowConfig};
use neko::project::{BackendChoice, ProjectConfig, Registry, RenderBackend, RenderingDriver, WindowBackend};
use neko::render::{Frame, Renderer};
use neko::{GpuApi, RenderOptions};
use neko_ui::UiOutput;

use editor::Editor;
use pm_native::NativeProjectManager;
use util::Action;

struct Args {
    window: Option<WindowBackend>,
    render: RenderBackend,
    gpu: GpuApi,
    screenshot: Option<PathBuf>,
    size: (u32, u32),
    scale: f32,
    new_dialog: bool,
    registry: Option<PathBuf>,
    open: Option<PathBuf>,
    add_node: bool,
    splash: bool,
    /// `--single-window`: dialogs inside the main window (Godot's flag).
    single_window: bool,
    /// `--render`/`--gpu` given: they override the project's renderer.
    explicit_render: bool,
    /// Options kept when the editor restarts itself (Godot's `restart_editor`).
    relaunch: Vec<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args {
        window: None,
        render: RenderBackend::Wgpu,
        gpu: GpuApi::Auto,
        screenshot: None,
        size: (1280, 800),
        scale: 1.0,
        new_dialog: false,
        registry: None,
        open: None,
        add_node: false,
        single_window: false,
        splash: false,
        explicit_render: false,
        relaunch: Vec::new(),
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut val = || it.next().ok_or_else(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--window" => {
                let v = val()?;
                a.relaunch.extend(["--window".to_string(), v.clone()]);
                a.window = Some(match v.as_str() {
                    "sdl3" | "sdl" => WindowBackend::Sdl3,
                    "winit" => WindowBackend::Winit,
                    o => return Err(format!("unknown window backend '{o}' (sdl3, winit)")),
                })
            }
            "--render" => {
                a.explicit_render = true;
                a.render = match val()?.as_str() {
                    "wgpu" => RenderBackend::Wgpu,
                    "soft" | "software" => RenderBackend::Soft,
                    o => return Err(format!("unknown render backend '{o}' (wgpu, soft)")),
                }
            }
            "--gpu" => {
                a.explicit_render = true;
                let v = val()?;
                a.gpu = GpuApi::parse(&v).ok_or(format!("unknown GPU API '{v}' (auto, vulkan, gl, metal, dx12)"))?
            }
            "--screenshot" => a.screenshot = Some(PathBuf::from(val()?)),
            "--size" => {
                let v = val()?;
                let (w, h) = v.split_once('x').ok_or("--size expects WIDTHxHEIGHT")?;
                a.size = (w.parse().map_err(|_| "bad width")?, h.parse().map_err(|_| "bad height")?);
            }
            "--scale" => a.scale = val()?.parse().map_err(|_| "bad scale")?,
            "--new-dialog" => a.new_dialog = true,
            "--registry" => {
                let v = val()?;
                a.relaunch.extend(["--registry".to_string(), v.clone()]);
                a.registry = Some(PathBuf::from(v))
            }
            "--open" => a.open = Some(PathBuf::from(val()?)),
            "--add-node" => a.add_node = true,
            "--splash" => a.splash = true,
            "--single-window" => {
                a.relaunch.push("--single-window".into());
                a.single_window = true
            }
            "-h" | "--help" => {
                println!(
                    "neko {}\n\nUSAGE:\n  neko [--window sdl3|winit] [--render wgpu|soft] [--gpu auto|vulkan|gl|metal|dx12]\n  neko --open DIR [--add-node]\n  neko --screenshot out.png [--size 1280x800] [--scale 1.0] [--new-dialog] [--splash]\n\nOPTIONS:\n  --registry FILE   use a different project list file\n\nENV:\n  NEKO_SDL3_LIB     path to libSDL3 (default: system library)\n  WGPU_BACKEND      force a wgpu API (vulkan, gl, metal, dx12)",
                    env!("CARGO_PKG_VERSION")
                );
                std::process::exit(0);
            }
            o => return Err(format!("unknown argument '{o}', see --help")),
        }
    }
    Ok(a)
}

fn load_registry(path: Option<PathBuf>) -> Registry {
    let r = match path {
        Some(p) => Registry::load_from(p),
        None => Registry::load_default(),
    };
    r.unwrap_or_else(|e| {
        eprintln!("neko: could not read the project list ({e}); starting empty");
        Registry::default()
    })
}

enum Screen {
    Projects(Box<NativeProjectManager>),
    Editor(Box<Editor>, Box<NativeProjectManager>),
}

impl Screen {
    fn pm(&mut self) -> &mut NativeProjectManager {
        match self {
            Screen::Projects(pm) | Screen::Editor(_, pm) => pm,
        }
    }
}

/// The boot splash shown while the editor starts or opens a project (like Godot's).
struct Splash {
    ui: neko_ui::Ui,
    until: Option<Instant>,
}

impl Splash {
    fn new() -> Self {
        let mut ui = neko_ui::Ui::new(neko_ui::Theme::neko_dark());
        let root = ui.root();
        ui.add(root, neko_ui::splash::splash_control(neko_ui::splash::SPLASH_BG, true, "Game engine"));
        Self { ui, until: None }
    }

    fn show(&mut self, d: Duration) {
        self.until = Some(Instant::now() + d);
    }

    fn active(&self) -> bool {
        self.until.is_some_and(|t| Instant::now() < t)
    }
}

const SPLASH_STARTUP: Duration = Duration::from_millis(1200);
const SPLASH_OPEN_PROJECT: Duration = Duration::from_millis(800);

struct EditorApp {
    splash: Splash,
    screen: Option<Screen>,
    render_kind: RenderBackend,
    gpu: GpuApi,
    /// The renderer came from the command line, not from the project.
    explicit_render: bool,
    relaunch: Vec<String>,
    /// What the editor is drawing with, once the renderer exists.
    running: Option<RenderingDriver>,
    window_name: String,
    window: Option<Arc<dyn Window>>,
    renderer: Option<Box<dyn Renderer>>,
    exit: bool,
    /// Dialogs open as OS windows: key -> (window, dialog id in the active Ui).
    subwindows: HashMap<u64, (Arc<dyn Window>, neko_ui::Id)>,
}

impl EditorApp {
    /// Run the screen on display for one frame and handle screen switches.
    fn screen_frame(&mut self, size: (u32, u32), ppp: f32, max_tex: usize) -> (UiOutput, Option<Duration>) {
        if self.splash.active() {
            return (self.splash.ui.frame(size, ppp, max_tex), self.splash.until.map(|t| t.saturating_duration_since(Instant::now())));
        }
        let screen = self.screen.take().expect("screen");
        let (screen, out, wake) = match screen {
            Screen::Projects(mut pm) => {
                pm.update();
                if let Some(Action::OpenEditor(path)) = pm.take_action() {
                    self.splash.show(SPLASH_OPEN_PROJECT);
                    let mut ed = Editor::open(path);
                    // Like Godot, a project opens with its own renderer: start a new
                    // editor process when it differs from the one drawing now.
                    if !self.explicit_render && self.running.is_some_and(|r| r != ed.project_driver()) {
                        self.relaunch(ed.project_path());
                    }
                    ed.set_running_driver(self.running);
                    let out = self.splash.ui.frame(size, ppp, max_tex);
                    (Screen::Editor(Box::new(ed), pm), out, Some(SPLASH_OPEN_PROJECT))
                } else {
                    if let (Some(w), Some(text)) = (&self.window, pm.clipboard.take()) {
                        w.set_clipboard(&text);
                    }
                    let out = pm.ui.frame(size, ppp, max_tex);
                    let wake = pm.repaint_after();
                    (Screen::Projects(pm), out, wake)
                }
            }
            Screen::Editor(mut ed, mut pm) => {
                let out = ed.frame(size, ppp, max_tex);
                if ed.quit {
                    self.exit = true;
                }
                if ed.restart {
                    ed.restart = false;
                    self.relaunch(ed.project_path());
                }
                if let Some(w) = &self.window {
                    if let Some(text) = ed.clipboard.take() {
                        w.set_clipboard(&text);
                    }
                    if let Some(f) = ed.fullscreen_request.take() {
                        w.set_fullscreen(f);
                    }
                }
                let wake = ed.wake_after();
                if ed.back {
                    pm.reload();
                    (Screen::Projects(pm), out, Some(Duration::ZERO))
                } else {
                    (Screen::Editor(ed, pm), out, wake)
                }
            }
        };
        self.screen = Some(screen);
        (out, wake)
    }

    fn title(&self) -> String {
        if self.splash.active() {
            return "Neko".into();
        }
        match &self.screen {
            Some(Screen::Editor(ed, _)) => ed.title(),
            _ => "Neko - Project Manager".into(),
        }
    }

    fn wants_text(&self) -> bool {
        match &self.screen {
            Some(Screen::Projects(pm)) => pm.ui.wants_text_input(),
            Some(Screen::Editor(ed, _)) => ed.ui.wants_text_input(),
            None => false,
        }
    }
}

impl EditorApp {
    /// The UI on screen (the splash has no dialogs).
    fn active_ui(&mut self) -> Option<&mut neko_ui::Ui> {
        if self.splash.active() {
            return None;
        }
        match &mut self.screen {
            Some(Screen::Projects(pm)) => Some(&mut pm.ui),
            Some(Screen::Editor(ed, _)) => Some(&mut ed.ui),
            None => None,
        }
    }

    /// Keys keep the project manager's and the editor's dialogs apart.
    fn screen_tag(&self) -> u64 {
        match &self.screen {
            Some(Screen::Editor(..)) => 1 << 63,
            _ => 0,
        }
    }
}

impl App for EditorApp {
    fn subwindows(&mut self) -> Vec<(u64, WindowConfig)> {
        let tag = self.screen_tag();
        let Some(ui) = self.active_ui() else { return Vec::new() };
        ui.open_windows()
            .into_iter()
            .map(|w| {
                let (mw, mh) = (w.min_size.x.ceil().max(200.0) as u32, w.min_size.y.ceil().max(120.0) as u32);
                (tag | w.id.to_bits(), WindowConfig { title: w.title, width: mw, height: mh, min_width: mw.min(480), min_height: mh.min(320), resizable: true })
            })
            .collect()
    }

    fn subwindow_opened(&mut self, key: u64, window: Arc<dyn Window>) {
        if let Some(r) = &mut self.renderer
            && let Err(e) = r.attach_window(key, window.clone())
        {
            eprintln!("neko: cannot draw into a dialog window: {e}");
        }
        let id = neko_ui::Id::from_bits(key & !(1 << 63));
        self.subwindows.insert(key, (window, id));
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn subwindow_closed(&mut self, key: u64) {
        if let Some(r) = &mut self.renderer {
            r.detach_window(key);
        }
        self.subwindows.remove(&key);
    }

    fn subwindow_event(&mut self, key: u64, event: Event) {
        let Some((win, id)) = self.subwindows.get(&key).map(|(w, i)| (w.clone(), *i)) else { return };
        let paste = match &event {
            Event::Key { key: Key::Char('v'), pressed: true, modifiers, .. } if modifiers.ctrl || modifiers.logo => win.clipboard(),
            _ => None,
        };
        let Some(ui) = self.active_ui() else { return };
        if let Some(text) = paste
            && ui.wants_text_input()
        {
            ui.window_event(id, &Event::Text(text));
            return;
        }
        ui.window_event(id, &event);
        win.set_text_input(ui.wants_text_input());
    }

    fn resumed(&mut self, window: Arc<dyn Window>) {
        let renderer = match neko::create_renderer(self.render_kind, window.clone(), RenderOptions { gpu_api: self.gpu }) {
            Ok(r) => r,
            Err(e) if self.render_kind != RenderBackend::Soft => {
                eprintln!("neko: {} renderer failed ({e}); falling back to software", self.render_kind.label());
                neko::create_renderer(RenderBackend::Soft, window.clone(), RenderOptions::default()).unwrap_or_else(|e| {
                    eprintln!("neko: software renderer failed too: {e}");
                    std::process::exit(1)
                })
            }
            Err(e) => {
                eprintln!("neko: {e}");
                std::process::exit(1)
            }
        };
        let info = renderer.info();
        self.running = Some(running_driver(&info));
        // Godot prints the same line at startup ("Vulkan 1.3 - Forward+ - Using Device: ...").
        println!("{} - Using Device: {}", self.running.map_or("?", |d| d.label()), info.device);
        if let Some(Screen::Editor(ed, _)) = &mut self.screen {
            ed.set_running_driver(self.running);
        }
        if let Some(s) = &mut self.screen {
            s.pm().info.window = self.window_name.clone();
            s.pm().info.renderer = format!("{} {} · {}", info.backend, info.api, info.device);
        }
        self.renderer = Some(renderer);
        self.window = Some(window);
    }

    fn event(&mut self, event: Event) {
        if let Event::Gamepad { id, event: pad } = &event {
            if let Some(s) = &mut self.screen {
                match pad {
                    GamepadEvent::Connected(info) => {
                        s.pm().info.gamepads.insert(*id, info.name.clone());
                    }
                    GamepadEvent::Disconnected => {
                        s.pm().info.gamepads.remove(id);
                    }
                    _ => {}
                }
            }
            if let Some(Screen::Editor(ed, _)) = &self.screen {
                match pad {
                    GamepadEvent::Connected(info) => ed.log_line(format!("Gamepad {id} connected: {}", info.name)),
                    GamepadEvent::Disconnected => ed.log_line(format!("Gamepad {id} disconnected")),
                    _ => {}
                }
            }
            return;
        }
        if let Event::CloseRequested = event {
            match &mut self.screen {
                Some(Screen::Editor(ed, _)) => ed.request_quit(),
                _ => self.exit = true,
            }
        }
        if let (Event::Resized { width, height }, Some(r)) = (&event, &mut self.renderer) {
            r.resize(*width, *height);
        }
        if self.splash.active() {
            return;
        }
        // Files dropped from the OS: import into the project (editor) or find
        // projects inside them (project manager), like Godot's files_dropped.
        if let Event::DroppedFiles(paths) = &event {
            match &mut self.screen {
                Some(Screen::Editor(ed, _)) => {
                    let at = ed.ui.mouse_pos();
                    ed.import_files(paths.clone(), at);
                }
                Some(Screen::Projects(pm)) => pm.import_paths(paths),
                None => {}
            }
            return;
        }
        // Paste from the system clipboard into the focused text field.
        let paste = match &event {
            Event::Key { key: Key::Char('v'), pressed: true, modifiers, .. } if modifiers.ctrl || modifiers.logo => self.window.as_ref().and_then(|w| w.clipboard()),
            _ => None,
        };
        let ui = match &mut self.screen {
            Some(Screen::Projects(pm)) => &mut pm.ui,
            Some(Screen::Editor(ed, _)) => &mut ed.ui,
            None => return,
        };
        if let Some(text) = paste
            && ui.wants_text_input()
        {
            ui.on_event(&Event::Text(text));
            return;
        }
        ui.on_event(&event);
    }

    fn frame(&mut self) -> Flow {
        if self.exit {
            return Flow::Exit;
        }
        let Some(window) = self.window.clone() else { return Flow::Wait };
        let (w, h) = window.inner_size();
        let ppp = window.scale_factor();
        let max_tex = self.renderer.as_ref().map_or(2048, |r| r.max_texture_side());
        // Dialog windows are painted first: the main frame then uploads every
        // texture (fonts included) before any window is drawn.
        let mut dialog_frames = Vec::new();
        let subs: Vec<(u64, Arc<dyn Window>, neko_ui::Id)> = self.subwindows.iter().map(|(k, (w, i))| (*k, w.clone(), *i)).collect();
        if let Some(ui) = self.active_ui() {
            for (key, win, id) in &subs {
                let (ww, wh) = win.inner_size();
                let o = ui.frame_window(*id, (ww, wh), win.scale_factor());
                dialog_frames.push((*key, win.clone(), o, (ww, wh)));
            }
        }
        let (mut out, wake) = self.screen_frame((w, h), ppp, max_tex);
        window.set_cursor(out.cursor);
        window.set_text_input(self.wants_text());
        window.set_title(&self.title());
        if let Some(r) = &mut self.renderer {
            let frame = Frame { clear_color: [0.0, 0.0, 0.0, 1.0], size_px: [w, h], pixels_per_point: ppp, textures: &out.textures, ui: &out.primitives };
            if let Err(e) = r.render(&frame) {
                eprintln!("neko: render error: {e}");
            }
            for (key, win, o, (ww, wh)) in &mut dialog_frames {
                win.set_cursor(o.cursor);
                let f = Frame { clear_color: [0.0, 0.0, 0.0, 1.0], size_px: [*ww, *wh], pixels_per_point: win.scale_factor(), textures: &o.textures, ui: &o.primitives };
                if let Err(e) = r.render_window(*key, &f) {
                    eprintln!("neko: render error in a dialog window: {e}");
                }
            }
        }
        for (_, _, o, _) in &mut dialog_frames {
            o.textures.clear();
        }
        out.textures.clear();
        if self.exit {
            return Flow::Exit;
        }
        let wake = [wake, out.repaint_after].into_iter().flatten().min();
        match wake {
            Some(Duration::ZERO) => {
                window.request_redraw();
                Flow::Wait
            }
            Some(d) => Instant::now().checked_add(d).map(Flow::WaitUntil).unwrap_or(Flow::Wait),
            None => Flow::Wait,
        }
    }
}

impl EditorApp {
    /// Godot's `restart_editor`: start a new editor on `project` and quit this one.
    fn relaunch(&mut self, project: &std::path::Path) {
        if self.window.is_none() {
            return; // headless (--screenshot): nothing to restart
        }
        let started = std::env::current_exe().and_then(|exe| std::process::Command::new(exe).args(&self.relaunch).arg("--open").arg(project).spawn());
        match started {
            Ok(_) => self.exit = true,
            Err(e) => eprintln!("neko: could not restart the editor: {e}"),
        }
    }
}

/// The rendering driver a renderer reports (wgpu's adapter backend, or the CPU).
fn running_driver(info: &neko::render::RendererInfo) -> RenderingDriver {
    if info.backend != "wgpu" {
        return RenderingDriver::Software;
    }
    match info.api.as_str() {
        "Vulkan" => RenderingDriver::Vulkan,
        "Dx12" => RenderingDriver::D3d12,
        "Metal" => RenderingDriver::Metal,
        "Gl" => RenderingDriver::OpenGl,
        _ => RenderingDriver::Vulkan,
    }
}

fn initial_screen(args: &Args) -> Screen {
    let mut pm = NativeProjectManager::new(load_registry(args.registry.clone()));
    if args.new_dialog {
        pm.open_new_dialog();
    }
    match &args.open {
        Some(dir) => {
            let mut ed = Editor::open(dir.clone());
            if args.add_node {
                ed.open_create_dialog();
            }
            Screen::Editor(Box::new(ed), Box::new(pm))
        }
        None => Screen::Projects(Box::new(pm)),
    }
}

/// Render the UI with the CPU renderer into a PNG, without opening a window.
fn screenshot(args: &Args, path: &PathBuf) -> Result<(), String> {
    let (w, h) = (((args.size.0 as f32) * args.scale) as u32, ((args.size.1 as f32) * args.scale) as u32);
    let mut app = EditorApp {
        splash: Splash::new(),
        screen: Some(initial_screen(args)),
        render_kind: RenderBackend::Soft,
        gpu: GpuApi::Auto,
        explicit_render: true,
        relaunch: Vec::new(),
        running: None,
        window_name: "headless".into(),
        window: None,
        renderer: None,
        exit: false,
        subwindows: HashMap::new(),
    };
    if args.splash {
        app.splash.show(Duration::from_secs(60));
    }
    if let Some(s) = &mut app.screen {
        s.pm().info.window = "headless".into();
        s.pm().info.renderer = "soft CPU".into();
    }
    // "x,y" is a left click; "r x,y" / "rx,y" is a right click.
    // (button, double, x, y)
    let clicks: Vec<(bool, bool, f32, f32)> = std::env::var("NEKO_CLICKS")
        .unwrap_or_default()
        .split(';')
        .filter_map(|c| {
            let c = c.trim();
            let (right, double, xy) = match (c.strip_prefix('r'), c.strip_prefix('d')) {
                (Some(rest), _) => (true, false, rest.trim_start()),
                (_, Some(rest)) => (false, true, rest.trim_start()),
                _ => (false, false, c),
            };
            let (x, y) = xy.split_once(',')?;
            Some((right, double, x.trim().parse().ok()?, y.trim().parse().ok()?))
        })
        .collect();
    // `--render wgpu` renders the last frame on the GPU (same pipeline as the window).
    let gpu = args.render == RenderBackend::Wgpu && std::env::var_os("NEKO_SCREENSHOT_GPU").is_some();
    let mut renderer = neko_soft::SoftRenderer::offscreen(w, h);
    let mut all_textures = neko::render::epaint::textures::TexturesDelta::default();
    let passes = 3 + clicks.len() * 2;
    for pass in 0..passes {
        if pass >= 1 && (pass - 1) % 2 == 0 && let Some(&(right, double, x, y)) = clicks.get((pass - 1) / 2) {
            let (px, py) = (x * args.scale, y * args.scale);
            let button = if right { MouseButton::Right } else { MouseButton::Left };
            app.event(Event::MouseMoved { x: px, y: py });
            for _ in 0..if double { 2 } else { 1 } {
                app.event(Event::MouseButton { button, pressed: true });
                app.event(Event::MouseButton { button, pressed: false });
            }
            // Stay on the clicked item so menus can show their submenu.
            app.event(Event::MouseMoved { x: px + 1.0, y: py });
        }
        // After the clicks, type NEKO_TYPE into the focused control.
        if pass == clicks.len() * 2 + 1
            && let Ok(text) = std::env::var("NEKO_TYPE")
        {
            for part in text.split_inclusive('\n') {
                let (t, enter) = match part.strip_suffix('\n') {
                    Some(t) => (t, true),
                    None => (part, false),
                };
                for ch in t.chars() {
                    app.event(Event::Text(ch.to_string()));
                }
                if enter {
                    app.event(Event::Key { key: Key::Enter, pressed: true, repeat: false, modifiers: Default::default() });
                }
            }
            // Let timers (completion, script checks) run before the frame.
            std::thread::sleep(Duration::from_millis(400));
        }
        let (mut out, _) = app.screen_frame((w, h), args.scale, 4096);
        if gpu && pass + 1 == passes {
            // The GPU path starts from scratch, so give it every texture seen so far.
            all_textures.append(std::mem::take(&mut out.textures));
            let frame = Frame { clear_color: [0.0, 0.0, 0.0, 1.0], size_px: [w, h], pixels_per_point: args.scale, textures: &all_textures, ui: &out.primitives };
            let rgba = neko_wgpu::render_offscreen(&frame, args.gpu).map_err(|e| e.to_string())?;
            all_textures.clear();
            return write_png(path, w, h, &rgba);
        }
        let frame = Frame { clear_color: [0.0, 0.0, 0.0, 1.0], size_px: [w, h], pixels_per_point: args.scale, textures: &out.textures, ui: &out.primitives };
        renderer.render(&frame).map_err(|e| e.to_string())?;
        all_textures.append(std::mem::take(&mut out.textures));
    }
    all_textures.clear();
    write_png(path, w, h, &renderer.rgba8())
}

fn write_png(path: &PathBuf, w: u32, h: u32, rgba: &[u8]) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut wr = enc.write_header().map_err(|e| e.to_string())?;
    wr.write_image_data(rgba).map_err(|e| e.to_string())?;
    Ok(())
}

fn main() {
    let args = parse_args().unwrap_or_else(|e| {
        eprintln!("neko: {e}");
        std::process::exit(2)
    });
    if let Some(path) = &args.screenshot {
        if let Err(e) = screenshot(&args, path) {
            eprintln!("neko: screenshot failed: {e}");
            std::process::exit(1);
        }
        println!("wrote {}", path.display());
        return;
    }

    // Native windows (winit) first, like Godot; SDL3 windows only as a fallback.
    // Gamepads always go through SDL3 when the library is installed.
    let candidates = match args.window {
        Some(w) => vec![w],
        None => neko::available_window_backends(),
    };
    let mut platform = None;
    for kind in candidates {
        match neko::create_platform(kind) {
            Ok(p) => {
                platform = Some((kind, p));
                break;
            }
            Err(e) => eprintln!("neko: {} unavailable: {e}", kind.label()),
        }
    }
    let Some((kind, platform)) = platform else {
        eprintln!("neko: no window backend could start");
        std::process::exit(1)
    };

    // Dialogs as OS windows, like Godot's editor, unless Single Window Mode
    // is on (setting or --single-window) or the window backend cannot.
    let single = args.single_window || util::EditorSettings::load().single_window_mode;
    util::NATIVE_DIALOGS.store(!single && platform.supports_subwindows(), std::sync::atomic::Ordering::Relaxed);

    // `--open DIR` draws with the project's renderer unless --render/--gpu say otherwise.
    let (mut render_kind, mut gpu) = (args.render, args.gpu);
    if !args.explicit_render
        && let Some(dir) = &args.open
        && let Ok(c) = ProjectConfig::load(dir)
    {
        let (k, o) = neko::renderer_for(c.rendering_driver());
        (render_kind, gpu) = (k, o.gpu_api);
    }

    let mut splash = Splash::new();
    splash.show(SPLASH_STARTUP);
    let app = EditorApp {
        splash,
        screen: Some(initial_screen(&args)),
        render_kind,
        gpu,
        explicit_render: args.explicit_render,
        relaunch: args.relaunch.clone(),
        running: None,
        window_name: kind.label().to_string(),
        window: None,
        renderer: None,
        exit: false,
        subwindows: HashMap::new(),
    };
    let config = WindowConfig { title: "Neko".into(), width: 1280, height: 760, ..Default::default() };
    if let Err(e) = platform.run(config, Box::new(app)) {
        eprintln!("neko: {e}");
        std::process::exit(1);
    }
}
