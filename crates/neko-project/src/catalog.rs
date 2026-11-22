//! Backend catalog and platform support matrix.
//!
//! Support levels are the project's current estimate per backend; the
//! effective support of a project is the minimum across its chosen backends.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Support {
    No,
    Partial,
    Yes,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TargetPlatform {
    Windows,
    Macos,
    Linux,
    Freebsd,
    Openbsd,
    Netbsd,
    Android,
    Ios,
    Web,
    Haiku,
}

impl TargetPlatform {
    pub const ALL: [Self; 10] = [
        Self::Windows,
        Self::Macos,
        Self::Linux,
        Self::Freebsd,
        Self::Openbsd,
        Self::Netbsd,
        Self::Android,
        Self::Ios,
        Self::Web,
        Self::Haiku,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Windows => "Windows",
            Self::Macos => "macOS",
            Self::Linux => "Linux",
            Self::Freebsd => "FreeBSD",
            Self::Openbsd => "OpenBSD",
            Self::Netbsd => "NetBSD",
            Self::Android => "Android",
            Self::Ios => "iOS",
            Self::Web => "Web",
            Self::Haiku => "Haiku",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::Macos => "macos",
            Self::Linux => "linux",
            Self::Freebsd => "freebsd",
            Self::Openbsd => "openbsd",
            Self::Netbsd => "netbsd",
            Self::Android => "android",
            Self::Ios => "ios",
            Self::Web => "web",
            Self::Haiku => "haiku",
        }
    }

    /// The platform this binary was compiled for, if it is a Neko target.
    pub fn host() -> Option<Self> {
        Some(match std::env::consts::OS {
            "windows" => Self::Windows,
            "macos" => Self::Macos,
            "linux" => Self::Linux,
            "freebsd" => Self::Freebsd,
            "openbsd" => Self::Openbsd,
            "netbsd" => Self::Netbsd,
            "android" => Self::Android,
            "ios" => Self::Ios,
            "haiku" => Self::Haiku,
            _ => return None,
        })
    }
}

/// Common interface of the three backend enums, for UI code.
pub trait BackendChoice: Copy + PartialEq + 'static {
    const ALL: &'static [Self];
    fn label(self) -> &'static str;
    /// Short technical subtitle.
    fn detail(self) -> &'static str;
    fn description(self) -> &'static str;
    /// Cargo feature of the `neko` crate, if any.
    fn feature(self) -> Option<&'static str>;
    fn support(self, p: TargetPlatform) -> Support;
    /// Window backend this choice depends on, if any.
    fn requires_window(self) -> Option<WindowBackend> {
        None
    }
    /// Whether the backend is implemented in this engine build (vs. planned).
    fn implemented(self) -> bool;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WindowBackend {
    Sdl3,
    Winit,
    Headless,
}

impl BackendChoice for WindowBackend {
    const ALL: &'static [Self] = &[Self::Sdl3, Self::Winit, Self::Headless];
    fn label(self) -> &'static str {
        match self {
            Self::Sdl3 => "SDL3",
            Self::Winit => "winit",
            Self::Headless => "Headless",
        }
    }
    fn detail(self) -> &'static str {
        match self {
            Self::Sdl3 => "SDL3 windows",
            Self::Winit => "native, pure Rust",
            Self::Headless => "no window",
        }
    }
    fn description(self) -> &'static str {
        match self {
            Self::Sdl3 => "SDL3 creates the window too. Use it where native backends are missing: the BSDs, Haiku, consoles.",
            Self::Winit => "Native windows per OS (Wayland, X11, Win32, Cocoa, Android, Web), like Godot's display servers. Gamepads still go through SDL3.",
            Self::Headless => "No window: dedicated servers, tests and offline rendering.",
        }
    }
    fn feature(self) -> Option<&'static str> {
        Some(match self {
            Self::Sdl3 => "window-sdl3",
            Self::Winit => "window-winit",
            Self::Headless => "window-headless",
        })
    }
    fn support(self, p: TargetPlatform) -> Support {
        use TargetPlatform as P;
        match (self, p) {
            (Self::Sdl3, _) => Support::Yes,
            (Self::Winit, P::Haiku) => Support::No,
            (Self::Winit, P::Openbsd | P::Netbsd) => Support::Partial,
            (Self::Winit, _) => Support::Yes,
            (Self::Headless, P::Android | P::Ios | P::Web) => Support::No,
            (Self::Headless, _) => Support::Yes,
        }
    }
    fn implemented(self) -> bool {
        !matches!(self, Self::Headless)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RenderBackend {
    Wgpu,
    Sdl3Gpu,
    Gl,
    Soft,
}

impl BackendChoice for RenderBackend {
    const ALL: &'static [Self] = &[Self::Wgpu, Self::Sdl3Gpu, Self::Gl, Self::Soft];
    fn label(self) -> &'static str {
        match self {
            Self::Wgpu => "wgpu",
            Self::Sdl3Gpu => "SDL3 GPU",
            Self::Gl => "OpenGL",
            Self::Soft => "Software",
        }
    }
    fn detail(self) -> &'static str {
        match self {
            Self::Wgpu => "Vulkan / Metal / DX12 / GL",
            Self::Sdl3Gpu => "SDL_GPU",
            Self::Gl => "glow, GL 3.3 / ES 3",
            Self::Soft => "CPU rasterizer",
        }
    }
    fn description(self) -> &'static str {
        match self {
            Self::Wgpu => "Main renderer. Picks the native API at runtime: Vulkan, Metal, DX12, GLES, or WebGPU/WebGL2 in the browser.",
            Self::Sdl3Gpu => "SDL3's GPU API (Vulkan, Metal, D3D12). Requires the SDL3 window backend. WGSL shaders are translated at export.",
            Self::Gl => "Compatibility mode for old hardware, the BSDs and Haiku via Mesa, and WebGL2 in the browser.",
            Self::Soft => "Runs on the CPU, anywhere. Useful for CI, servers without a GPU and low-resolution 2D games.",
        }
    }
    fn feature(self) -> Option<&'static str> {
        Some(match self {
            Self::Wgpu => "render-wgpu",
            Self::Sdl3Gpu => "render-sdl3-gpu",
            Self::Gl => "render-gl",
            Self::Soft => "render-soft",
        })
    }
    fn support(self, p: TargetPlatform) -> Support {
        use TargetPlatform as P;
        match (self, p) {
            (Self::Wgpu, P::Haiku) => Support::No,
            (Self::Wgpu, P::Openbsd | P::Netbsd) => Support::Partial,
            (Self::Wgpu, _) => Support::Yes,
            (Self::Sdl3Gpu, P::Web | P::Haiku) => Support::No,
            (Self::Sdl3Gpu, P::Openbsd | P::Netbsd) => Support::Partial,
            (Self::Sdl3Gpu, _) => Support::Yes,
            (Self::Gl, P::Ios) => Support::Partial,
            (Self::Gl, _) => Support::Yes,
            (Self::Soft, _) => Support::Yes,
        }
    }
    fn requires_window(self) -> Option<WindowBackend> {
        matches!(self, Self::Sdl3Gpu).then_some(WindowBackend::Sdl3)
    }
    fn implemented(self) -> bool {
        matches!(self, Self::Wgpu | Self::Soft)
    }
}

/// The graphics API the game and the editor draw with: the editor's renderer
/// selector (top right, like Godot's Forward+ / Mobile / Compatibility).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RenderingDriver {
    Vulkan,
    D3d12,
    Metal,
    OpenGl,
    /// Neko's CPU rasterizer: runs anywhere, no GPU needed.
    Software,
}

impl RenderingDriver {
    pub const ALL: [Self; 5] = [Self::Vulkan, Self::D3d12, Self::Metal, Self::OpenGl, Self::Software];

    pub fn label(self) -> &'static str {
        match self {
            Self::Vulkan => "Vulkan",
            Self::D3d12 => "Direct3D 12",
            Self::Metal => "Metal",
            Self::OpenGl => "OpenGL",
            Self::Software => "Software",
        }
    }

    /// The value Godot uses (`rendering/rendering_device/driver`,
    /// `rendering/gl_compatibility/driver`).
    pub fn id(self) -> &'static str {
        match self {
            Self::Vulkan => "vulkan",
            Self::D3d12 => "d3d12",
            Self::Metal => "metal",
            Self::OpenGl => "opengl3",
            Self::Software => "software",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|d| d.id() == id.to_ascii_lowercase())
    }

    /// Godot's `rendering/renderer/rendering_method` for this driver.
    pub fn rendering_method(self) -> &'static str {
        match self {
            Self::OpenGl => "gl_compatibility",
            _ => "forward_plus",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Vulkan => "Modern GPU API for Windows, Linux, the BSDs and Android. The default.",
            Self::D3d12 => "Microsoft's GPU API, for Windows and Xbox.",
            Self::Metal => "Apple's GPU API, for macOS and iOS.",
            Self::OpenGl => "Compatibility: older GPUs and drivers (OpenGL 3.3 / OpenGL ES 3), like Godot's Compatibility renderer.",
            Self::Software => "Draws on the CPU. Works without a GPU (servers, virtual machines, very old hardware).",
        }
    }

    /// Whether the editor on `p` can draw with it.
    pub fn available_on(self, p: TargetPlatform) -> bool {
        use TargetPlatform as P;
        match self {
            Self::Vulkan => !matches!(p, P::Macos | P::Ios | P::Web | P::Haiku),
            Self::D3d12 => p == P::Windows,
            Self::Metal => matches!(p, P::Macos | P::Ios),
            Self::OpenGl | Self::Software => true,
        }
    }

    /// The drivers offered on this machine, in menu order.
    pub fn host_choices() -> Vec<Self> {
        let host = TargetPlatform::host().unwrap_or(TargetPlatform::Linux);
        Self::ALL.into_iter().filter(|d| d.available_on(host)).collect()
    }

    /// Godot's default for a new project: Metal on Apple, Vulkan elsewhere.
    pub fn host_default() -> Self {
        match TargetPlatform::host() {
            Some(TargetPlatform::Macos | TargetPlatform::Ios) => Self::Metal,
            Some(TargetPlatform::Haiku) => Self::OpenGl,
            _ => Self::Vulkan,
        }
    }

    /// Godot's OS feature name for per-platform overrides (`driver.linuxbsd`).
    pub fn host_feature() -> &'static str {
        match TargetPlatform::host() {
            Some(TargetPlatform::Windows) => "windows",
            Some(TargetPlatform::Macos) => "macos",
            Some(TargetPlatform::Android) => "android",
            Some(TargetPlatform::Ios) => "ios",
            Some(TargetPlatform::Web) => "web",
            _ => "linuxbsd",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioBackend {
    Sdl3,
    Cpal,
    None,
}

impl BackendChoice for AudioBackend {
    const ALL: &'static [Self] = &[Self::Sdl3, Self::Cpal, Self::None];
    fn label(self) -> &'static str {
        match self {
            Self::Sdl3 => "SDL3",
            Self::Cpal => "cpal",
            Self::None => "None",
        }
    }
    fn detail(self) -> &'static str {
        match self {
            Self::Sdl3 => "SDL_audio",
            Self::Cpal => "pure Rust",
            Self::None => "silent",
        }
    }
    fn description(self) -> &'static str {
        match self {
            Self::Sdl3 => "Audio through SDL3. Requires the SDL3 window backend.",
            Self::Cpal => "Pure Rust audio output (WASAPI, CoreAudio, ALSA/PulseAudio, AAudio, WebAudio).",
            Self::None => "No audio device is opened.",
        }
    }
    fn feature(self) -> Option<&'static str> {
        match self {
            Self::Sdl3 => Some("audio-sdl3"),
            Self::Cpal => Some("audio-cpal"),
            Self::None => None,
        }
    }
    fn support(self, p: TargetPlatform) -> Support {
        use TargetPlatform as P;
        match (self, p) {
            (Self::Cpal, P::Haiku) => Support::No,
            (Self::Cpal, P::Openbsd | P::Netbsd) => Support::Partial,
            _ => Support::Yes,
        }
    }
    fn requires_window(self) -> Option<WindowBackend> {
        matches!(self, Self::Sdl3).then_some(WindowBackend::Sdl3)
    }
    fn implemented(self) -> bool {
        matches!(self, Self::None)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Template {
    Empty,
    #[serde(rename = "2d")]
    TwoD,
    #[serde(rename = "3d")]
    ThreeD,
    Ui,
}

impl Template {
    pub const ALL: [Self; 4] = [Self::Empty, Self::TwoD, Self::ThreeD, Self::Ui];
    pub fn label(self) -> &'static str {
        match self {
            Self::Empty => "Empty",
            Self::TwoD => "2D",
            Self::ThreeD => "3D",
            Self::Ui => "App / UI",
        }
    }
    pub fn root_node(self) -> &'static str {
        match self {
            Self::Empty => "Node",
            Self::TwoD => "Node2D",
            Self::ThreeD => "Node3D",
            Self::Ui => "Control",
        }
    }
    pub fn features(self) -> &'static [&'static str] {
        match self {
            Self::Empty => &[],
            Self::TwoD => &["2d", "physics2d"],
            Self::ThreeD => &["3d", "physics3d"],
            Self::Ui => &["ui"],
        }
    }
    pub fn tag(self) -> Option<&'static str> {
        match self {
            Self::Empty => None,
            Self::TwoD => Some("2d"),
            Self::ThreeD => Some("3d"),
            Self::Ui => Some("ui"),
        }
    }
}

/// The three backend choices of a project.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackendSet {
    pub window: WindowBackend,
    pub render: RenderBackend,
    pub audio: AudioBackend,
}

impl Default for BackendSet {
    fn default() -> Self {
        Self { window: WindowBackend::Winit, render: RenderBackend::Wgpu, audio: AudioBackend::Cpal }
    }
}

impl BackendSet {
    /// Effective support on a platform: the weakest of the three backends.
    pub fn support(&self, p: TargetPlatform) -> Support {
        self.window.support(p).min(self.render.support(p)).min(self.audio.support(p))
    }

    /// Fix choices that depend on another backend (e.g. SDL3 GPU needs an SDL3 window).
    /// Returns `true` if something changed.
    pub fn normalize(&mut self) -> bool {
        let before = *self;
        if self.render.requires_window().is_some_and(|w| w != self.window) {
            self.render = RenderBackend::Wgpu;
        }
        if self.audio.requires_window().is_some_and(|w| w != self.window) {
            self.audio = AudioBackend::Cpal;
        }
        *self != before
    }

    /// `neko` crate features for these backends plus a template.
    pub fn features(&self, template: Template) -> Vec<&'static str> {
        let mut f: Vec<&'static str> = template.features().to_vec();
        // Godot projects use SVG textures everywhere (icon.svg).
        f.push("image-svg");
        f.extend(self.window.feature());
        f.extend(self.render.feature());
        f.extend(self.audio.feature());
        if self.window != WindowBackend::Headless {
            f.push("gamepad-sdl3");
        }
        f
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sdl3_gpu_requires_sdl3_window() {
        let mut b = BackendSet { window: WindowBackend::Winit, render: RenderBackend::Sdl3Gpu, audio: AudioBackend::Sdl3 };
        assert!(b.normalize());
        assert_eq!(b.render, RenderBackend::Wgpu);
        assert_eq!(b.audio, AudioBackend::Cpal);
    }

    #[test]
    fn support_is_weakest_backend() {
        let b = BackendSet { window: WindowBackend::Sdl3, render: RenderBackend::Gl, audio: AudioBackend::Sdl3 };
        assert_eq!(b.support(TargetPlatform::Haiku), Support::Yes);
        let b = BackendSet { window: WindowBackend::Winit, render: RenderBackend::Wgpu, audio: AudioBackend::Cpal };
        assert_eq!(b.support(TargetPlatform::Haiku), Support::No);
        assert_eq!(b.support(TargetPlatform::Openbsd), Support::Partial);
    }

    #[test]
    fn features_include_template_and_backends() {
        let f = BackendSet::default().features(Template::TwoD);
        assert_eq!(f, ["2d", "physics2d", "image-svg", "window-winit", "render-wgpu", "audio-cpal", "gamepad-sdl3"]);
    }
}
