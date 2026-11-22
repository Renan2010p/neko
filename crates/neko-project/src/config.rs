//! `project.neko`: per-project settings read by the editor and the runtime.
//!
//! It uses Godot's `ConfigFile` syntax, the same as `project.godot`, so
//! Godot settings copied into it keep working and keys Neko does not know are
//! preserved. Neko's own settings live under `[neko]`:
//!
//! ```text
//! config_version=5
//!
//! [application]
//!
//! config/name="Cat Platformer"
//! run/main_scene="res://scenes/main.tscn"
//!
//! [neko]
//!
//! template="2d"
//! engine="0.1.0"
//! backends/window="winit"
//! backends/render="wgpu"
//! backends/audio="none"
//! targets/platforms=PackedStringArray("linux")
//! ```
//!
//! Projects from Neko 0.1 used `neko.toml`; it is still read and converted.

use std::path::{Path, PathBuf};

use neko_core::godot::Value;
use neko_core::godot::config::ConfigFile;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::catalog::{BackendSet, RenderBackend, RenderingDriver, TargetPlatform, Template};
use crate::{ProjectError, io_err};

pub const FILE_NAME: &str = "project.neko";
/// The Neko 0.1 project file.
pub const LEGACY_FILE_NAME: &str = "neko.toml";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub project: ProjectSection,
    pub backends: BackendSet,
    pub targets: TargetsSection,
    /// Shown while the game starts (Godot's `application/boot_splash`).
    #[serde(default)]
    pub boot_splash: BootSplash,
    /// The whole file as loaded, so unknown keys survive a save.
    #[serde(skip)]
    pub file: ConfigFile,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BootSplash {
    pub show_image: bool,
    /// sRGB color, 0..1 per channel.
    pub bg_color: [f32; 3],
    pub minimum_display_time_ms: u64,
}

impl Default for BootSplash {
    fn default() -> Self {
        // Godot's default background (0.14, 0.14, 0.14).
        Self { show_image: true, bg_color: [0.14, 0.14, 0.14], minimum_display_time_ms: 1000 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectSection {
    pub name: String,
    pub template: Template,
    /// Main scene, relative to the project folder (no `res://`), or a
    /// `uid://` that [`crate::resolve_uid`] turns into a path.
    pub main_scene: String,
    /// Engine version the project was created with.
    pub engine: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TargetsSection {
    pub platforms: Vec<TargetPlatform>,
}

/// A serde enum as its text name (`"wgpu"`, `"2d"`...).
fn enum_name<T: Serialize>(v: &T) -> String {
    match toml::Value::try_from(v) {
        Ok(toml::Value::String(s)) => s,
        _ => String::new(),
    }
}

fn enum_from<T: DeserializeOwned>(s: &str) -> Option<T> {
    toml::Value::String(s.to_string()).try_into().ok()
}

impl ProjectConfig {
    pub fn path_in(dir: &Path) -> PathBuf {
        dir.join(FILE_NAME)
    }

    /// Load `project.neko`, or a Neko 0.1 `neko.toml`.
    pub fn load(dir: &Path) -> Result<Self, ProjectError> {
        let path = Self::path_in(dir);
        let (path, text) = match std::fs::read_to_string(&path) {
            Ok(t) => (path, t),
            Err(_) if dir.join(LEGACY_FILE_NAME).is_file() => {
                let legacy = dir.join(LEGACY_FILE_NAME);
                let t = std::fs::read_to_string(&legacy).map_err(io_err(&legacy))?;
                (legacy, t)
            }
            Err(e) => return Err(ProjectError::Io { path, source: e }),
        };
        let mut c = Self::parse(&text).map_err(|message| ProjectError::Parse { path, message })?;
        if c.project.main_scene.starts_with("uid://")
            && let Some(p) = crate::resolve_uid(dir, &c.project.main_scene)
        {
            c.project.main_scene = p;
        }
        Ok(c)
    }

    /// Parse `project.neko` text (or legacy `neko.toml` text).
    pub fn parse(text: &str) -> Result<Self, String> {
        let looks_legacy = text.lines().any(|l| l.trim() == "[project]") && !text.contains("config_version");
        if looks_legacy {
            return toml::from_str(text).map_err(|e| e.to_string());
        }
        let f = ConfigFile::parse(text)?;
        let s = |sec: &str, key: &str| f.get_str(sec, key).map(str::to_string);
        let name = s("application", "config/name").unwrap_or_else(|| "Unnamed Project".into());
        let main_scene = s("application", "run/main_scene").unwrap_or_default();
        let main_scene = main_scene.strip_prefix("res://").map(str::to_string).unwrap_or(main_scene);
        let template = s("neko", "template").and_then(|t| enum_from(&t)).unwrap_or(Template::Empty);
        let engine = s("neko", "engine").unwrap_or_default();
        let mut backends = BackendSet::default();
        if let Some(w) = s("neko", "backends/window").and_then(|v| enum_from(&v)) {
            backends.window = w;
        }
        if let Some(r) = s("neko", "backends/render").and_then(|v| enum_from(&v)) {
            backends.render = r;
        }
        if let Some(a) = s("neko", "backends/audio").and_then(|v| enum_from(&v)) {
            backends.audio = a;
        }
        let platforms = match f.get("neko", "targets/platforms") {
            Some(Value::Call(_, items)) | Some(Value::Array(items)) => items.iter().filter_map(|v| v.as_str()).filter_map(enum_from).collect(),
            _ => Vec::new(),
        };
        let mut boot_splash = BootSplash::default();
        if let Some(c) = f.get("application", "boot_splash/bg_color").and_then(|v| v.numbers("Color")).filter(|c| c.len() >= 3) {
            boot_splash.bg_color = [c[0] as f32, c[1] as f32, c[2] as f32];
        }
        if let Some(b) = f.get("application", "boot_splash/show_image").and_then(Value::as_bool) {
            boot_splash.show_image = b;
        }
        if let Some(ms) = f.get("application", "boot_splash/minimum_display_time").and_then(Value::as_i64) {
            boot_splash.minimum_display_time_ms = ms.max(0) as u64;
        }
        Ok(Self {
            project: ProjectSection { name, template, main_scene, engine },
            backends,
            targets: TargetsSection { platforms },
            boot_splash,
            file: f,
        })
    }

    /// The rendering driver, read from the same keys as Godot's
    /// `project.godot` (so imported Godot projects keep their renderer):
    /// `rendering/renderer/rendering_method` = `gl_compatibility` is OpenGL,
    /// otherwise `rendering/rendering_device/driver[.<os>]`. Neko's software
    /// renderer is `[neko] backends/render="soft"`.
    pub fn rendering_driver(&self) -> RenderingDriver {
        if self.backends.render == RenderBackend::Soft {
            return RenderingDriver::Software;
        }
        let os = RenderingDriver::host_feature();
        let get = |key: &str| {
            self.file
                .get_str("rendering", &format!("{key}.{os}"))
                .or_else(|| self.file.get_str("rendering", key))
                .map(str::to_ascii_lowercase)
        };
        if get("renderer/rendering_method").as_deref() == Some("gl_compatibility") {
            return RenderingDriver::OpenGl;
        }
        get("rendering_device/driver")
            .and_then(|d| RenderingDriver::from_id(&d))
            .filter(|d| d.available_on(TargetPlatform::host().unwrap_or(TargetPlatform::Linux)))
            .unwrap_or_else(RenderingDriver::host_default)
    }

    /// Store `d` the way Godot's renderer selector does (rendering method plus
    /// the mobile override), and the device driver for this OS.
    pub fn set_rendering_driver(&mut self, d: RenderingDriver) {
        if d == RenderingDriver::Software {
            self.backends.render = RenderBackend::Soft;
            return;
        }
        if self.backends.render == RenderBackend::Soft {
            self.backends.render = RenderBackend::Wgpu;
        }
        let str = |s: &str| Value::String(s.to_string());
        let method = d.rendering_method();
        let mobile = if d == RenderingDriver::OpenGl { "gl_compatibility" } else { "mobile" };
        let os = RenderingDriver::host_feature();
        self.file.set("rendering", "renderer/rendering_method", str(method));
        self.file.set("rendering", "renderer/rendering_method.mobile", str(mobile));
        // A per-OS override would otherwise win over the value set here.
        if self.file.get("rendering", &format!("renderer/rendering_method.{os}")).is_some() {
            self.file.set("rendering", &format!("renderer/rendering_method.{os}"), str(method));
        }
        if d != RenderingDriver::OpenGl {
            self.file.set("rendering", "rendering_device/driver", str(d.id()));
            if self.file.get("rendering", &format!("rendering_device/driver.{os}")).is_some() {
                self.file.set("rendering", &format!("rendering_device/driver.{os}"), str(d.id()));
            }
        }
    }

    /// The file text, keeping keys this struct does not model.
    pub fn to_text(&self) -> String {
        let mut f = self.file.clone();
        let str = |s: &str| Value::String(s.to_string());
        f.set("", "config_version", Value::Int(5));
        f.set("application", "config/name", str(&self.project.name));
        if self.project.main_scene.is_empty() {
            f.remove("application", "run/main_scene");
        } else if self.project.main_scene.starts_with("uid://") {
            f.set("application", "run/main_scene", str(&self.project.main_scene));
        } else {
            f.set("application", "run/main_scene", str(&format!("res://{}", self.project.main_scene)));
        }
        let d = BootSplash::default();
        let b = &self.boot_splash;
        if b.bg_color != d.bg_color {
            let [r, g, bl] = b.bg_color.map(|x| x as f64);
            f.set("application", "boot_splash/bg_color", Value::color([r, g, bl, 1.0]));
        }
        if b.show_image != d.show_image {
            f.set("application", "boot_splash/show_image", Value::Bool(b.show_image));
        }
        if b.minimum_display_time_ms != d.minimum_display_time_ms {
            f.set("application", "boot_splash/minimum_display_time", Value::Int(b.minimum_display_time_ms as i64));
        }
        f.set("neko", "template", str(&enum_name(&self.project.template)));
        f.set("neko", "engine", str(&self.project.engine));
        f.set("neko", "backends/window", str(&enum_name(&self.backends.window)));
        f.set("neko", "backends/render", str(&enum_name(&self.backends.render)));
        f.set("neko", "backends/audio", str(&enum_name(&self.backends.audio)));
        let platforms = self.targets.platforms.iter().map(|p| str(&enum_name(p))).collect();
        f.set("neko", "targets/platforms", Value::Call("PackedStringArray".into(), platforms));
        f.write("Neko project file (Godot ConfigFile syntax).\n\nNeko and Godot share this format; Neko's own settings are under [neko].")
    }

    pub fn save(&self, dir: &Path) -> Result<(), ProjectError> {
        let path = Self::path_in(dir);
        std::fs::write(&path, self.to_text()).map_err(io_err(&path))?;
        // The Neko 0.1 file is replaced by project.neko.
        let legacy = dir.join(LEGACY_FILE_NAME);
        if legacy.is_file() {
            let _ = std::fs::remove_file(legacy);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let c = ProjectConfig {
            project: ProjectSection {
                name: "Cat Platformer".into(),
                template: Template::TwoD,
                main_scene: "scenes/main.tscn".into(),
                engine: "0.1.0".into(),
            },
            backends: BackendSet::default(),
            targets: TargetsSection { platforms: vec![TargetPlatform::Linux, TargetPlatform::Freebsd] },
            boot_splash: BootSplash::default(),
            file: ConfigFile::default(),
        };
        let text = c.to_text();
        assert!(text.contains("template=\"2d\""), "{text}");
        assert!(text.contains("backends/render=\"wgpu\""), "{text}");
        assert!(text.contains("run/main_scene=\"res://scenes/main.tscn\""), "{text}");
        let back = ProjectConfig::parse(&text).unwrap();
        assert_eq!(back.project, c.project);
        assert_eq!(back.backends, c.backends);
        assert_eq!(back.targets, c.targets);
        assert_eq!(back.to_text(), text);
    }
}

#[cfg(test)]
mod godot_tests {
    use super::*;

    #[test]
    fn keeps_godot_settings_and_reads_legacy_toml() {
        let text = "config_version=5\n\n[application]\n\nconfig/name=\"Ported\"\nrun/main_scene=\"res://main.tscn\"\nconfig/features=PackedStringArray(\"4.7\")\n\n[display]\n\nwindow/size/viewport_width=640\n";
        let c = ProjectConfig::parse(text).unwrap();
        assert_eq!(c.project.name, "Ported");
        assert_eq!(c.project.main_scene, "main.tscn");
        let out = c.to_text();
        assert!(out.contains("window/size/viewport_width=640"), "{out}");
        assert!(out.contains("config/features=PackedStringArray(\"4.7\")"), "{out}");

        let legacy = "[project]\nname = \"Old\"\ntemplate = \"2d\"\nmain_scene = \"scenes/main.scn\"\nengine = \"0.1.0\"\n\n[backends]\nwindow = \"sdl3\"\nrender = \"wgpu\"\naudio = \"none\"\n\n[targets]\nplatforms = [\"linux\"]\n";
        let c = ProjectConfig::parse(legacy).unwrap();
        assert_eq!(c.project.name, "Old");
        assert_eq!(c.project.main_scene, "scenes/main.scn");
    }

    #[test]
    fn rendering_driver_uses_godot_keys() {
        use crate::catalog::RenderingDriver as D;
        // A Godot Compatibility project opens with OpenGL.
        let godot = "config_version=5\n\n[rendering]\n\nrenderer/rendering_method=\"gl_compatibility\"\n";
        let mut c = ProjectConfig::parse(godot).unwrap();
        assert_eq!(c.rendering_driver(), D::OpenGl);
        c.set_rendering_driver(D::Vulkan);
        let text = c.to_text();
        assert!(text.contains("renderer/rendering_method=\"forward_plus\""), "{text}");
        assert!(text.contains("rendering_device/driver=\"vulkan\""), "{text}");
        let mut back = ProjectConfig::parse(&text).unwrap();
        if D::Vulkan.available_on(TargetPlatform::host().unwrap_or(TargetPlatform::Linux)) {
            assert_eq!(back.rendering_driver(), D::Vulkan);
        }
        back.set_rendering_driver(D::Software);
        let back = ProjectConfig::parse(&back.to_text()).unwrap();
        assert_eq!(back.rendering_driver(), D::Software);
        assert_eq!(back.backends.render, RenderBackend::Soft);
    }

}
