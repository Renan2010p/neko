//! Small helpers shared by the project manager and the editor.

use std::path::{Path, PathBuf};

use neko::project::{RenderBackend, registry};

/// What the editor is running on, shown in the status bar.
#[derive(Clone, Debug, Default)]
pub struct RuntimeInfo {
    pub window: String,
    pub renderer: String,
    /// Connected gamepads by id.
    pub gamepads: std::collections::BTreeMap<u32, String>,
}

/// Request from the project manager to the app.
pub enum Action {
    OpenEditor(PathBuf),
}

/// Godot-style summary next to the renderer choice.
pub fn renderer_bullets(r: RenderBackend) -> &'static [&'static str] {
    match r {
        RenderBackend::Wgpu => &[
            "Vulkan, Metal, DX12 or GLES, picked at runtime.",
            "Desktop, mobile and web (WebGPU/WebGL2).",
            "Advanced 2D and 3D graphics.",
            "Best choice for most projects.",
        ],
        RenderBackend::Gl => &["OpenGL 3.3 / ES 3 through glow.", "Runs on old hardware, the BSDs and Haiku.", "Lower graphics features."],
        RenderBackend::Soft => &["Draws on the CPU, needs no GPU.", "Works everywhere, including CI.", "Slow; for tools and small 2D games."],
        RenderBackend::Sdl3Gpu => &["SDL3's own GPU API (Vulkan, Metal, D3D12).", "Requires the SDL3 window backend.", "Not implemented yet."],
    }
}

/// Path with the home folder shortened to `~`.
pub fn display_path(p: &Path) -> String {
    if let Some(home) = std::env::var_os("HOME")
        && let Ok(rest) = p.strip_prefix(&home)
    {
        return format!("~/{}", rest.display());
    }
    p.display().to_string()
}

pub fn ago(t: u64) -> String {
    if t == 0 {
        return "never".into();
    }
    let secs = registry::now().saturating_sub(t);
    match secs {
        0..60 => "just now".into(),
        60..3600 => format!("{} min ago", secs / 60),
        3600..86_400 => format!("{} h ago", secs / 3600),
        86_400..2_592_000 => format!("{} d ago", secs / 86_400),
        _ => format!("{} mo ago", secs / 2_592_000),
    }
}

pub fn expand_home(p: &str) -> String {
    match (p.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => Path::new(&home).join(rest).display().to_string(),
        _ => p.to_string(),
    }
}

/// Root of the engine checkout this editor was built from, if still present.
/// New projects depend on it by path so they build without crates.io.
pub fn engine_checkout() -> Option<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent()?.parent()?;
    root.join("crates/neko/Cargo.toml").is_file().then(|| root.to_path_buf())
}

pub fn open_folder(p: &Path) {
    let cmd = if cfg!(target_os = "windows") {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(cmd).arg(p).spawn();
}

/// Copy a project folder, skipping build output.
pub fn copy_dir(src: &Path, dest: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        if name == "target" {
            continue;
        }
        let to = dest.join(&name);
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &to)?;
        } else {
            std::fs::copy(entry.path(), to)?;
        }
    }
    Ok(())
}

/// Whether dialogs open as separate OS windows (Godot's default) or inside
/// the main window ("Single Window Mode"). Decided once at startup.
pub static NATIVE_DIALOGS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn native_dialogs() -> bool {
    NATIVE_DIALOGS.load(std::sync::atomic::Ordering::Relaxed)
}

/// Editor-wide settings (Godot's editor_settings), in the user data folder.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct EditorSettings {
    /// `interface/editor/display/single_window_mode`: dialogs inside the main window.
    #[serde(default)]
    pub single_window_mode: bool,
}

impl EditorSettings {
    fn file() -> Option<std::path::PathBuf> {
        dirs::data_dir().map(|d| d.join("neko").join("editor_settings.toml"))
    }

    pub fn load() -> Self {
        Self::file().and_then(|f| std::fs::read_to_string(f).ok()).and_then(|t| toml::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn save(&self) {
        if let Some(f) = Self::file() {
            if let Some(d) = f.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            if let Ok(t) = toml::to_string(self) {
                let _ = std::fs::write(f, t);
            }
        }
    }
}
