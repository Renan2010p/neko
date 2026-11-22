//! The text server: font loading, shaping and the glyph atlas.
//!
//! Godot keeps text behind its `TextServer` so the shaping engine can change.
//! Neko does the same; this implementation uses epaint's font engine and can
//! be swapped (e.g. for cosmic-text) without touching controls.

use std::sync::Arc;

use neko_render::epaint::text::{FontData, FontDefinitions, Fonts, LayoutJob, TextOptions, TextWrapping, ViewportKey};
use neko_render::epaint::FontFamily;
use neko_render::epaint::{Color32, FontId, Galley, ImageDelta, TextureAtlas};

/// Inter (the font Godot's editor embeds), OFL; see `assets/fonts`.
pub const INTER_REGULAR: &[u8] = include_bytes!("../assets/fonts/Inter_Regular.ttf");
pub const INTER_BOLD: &[u8] = include_bytes!("../assets/fonts/Inter_Bold.ttf");
pub const JETBRAINS_MONO: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono_Regular.ttf");

/// The family name for bold text: `FontFamily::Name("bold")`.
pub fn bold() -> FontFamily {
    FontFamily::Name("bold".into())
}

/// Neko's default fonts: Inter for UI text, Inter Bold, JetBrains Mono for
/// code; epaint's bundled fonts stay as fallbacks for symbols and emoji.
pub fn default_fonts() -> FontDefinitions {
    let mut defs = FontDefinitions::default();
    let fallbacks_prop = defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    defs.font_data.insert("inter".into(), Arc::new(FontData::from_static(INTER_REGULAR)));
    defs.font_data.insert("inter-bold".into(), Arc::new(FontData::from_static(INTER_BOLD)));
    defs.font_data.insert("jetbrains-mono".into(), Arc::new(FontData::from_static(JETBRAINS_MONO)));
    defs.families.entry(FontFamily::Proportional).or_default().insert(0, "inter".into());
    defs.families.entry(FontFamily::Monospace).or_default().insert(0, "jetbrains-mono".into());
    let mut bold_list = vec!["inter-bold".to_string(), "inter".to_string()];
    bold_list.extend(fallbacks_prop);
    defs.families.insert(bold(), bold_list);
    defs
}

pub struct TextServer {
    fonts: Fonts,
    ppp: f32,
    max_texture_side: usize,
}

impl TextServer {
    pub fn new() -> Self {
        let options = TextOptions::default();
        Self { max_texture_side: options.max_texture_side, fonts: Fonts::new(options, default_fonts()), ppp: 1.0 }
    }

    pub(crate) fn begin_frame(&mut self, ppp: f32, max_texture_side: usize) {
        self.ppp = ppp;
        self.max_texture_side = max_texture_side.min(8192);
        let options = TextOptions { max_texture_side: self.max_texture_side, ..TextOptions::default() };
        self.fonts.begin_pass(options, ViewportKey::default());
    }

    /// Single-line text.
    pub fn layout(&mut self, text: &str, size: f32, color: Color32) -> Arc<Galley> {
        self.fonts.with_pixels_per_point(self.ppp).layout_no_wrap(text.to_string(), FontId::proportional(size), color)
    }

    /// Single-line monospace text.
    pub fn layout_mono(&mut self, text: &str, size: f32, color: Color32) -> Arc<Galley> {
        self.fonts.with_pixels_per_point(self.ppp).layout_no_wrap(text.to_string(), FontId::monospace(size), color)
    }

    /// Single-line bold text.
    pub fn layout_bold(&mut self, text: &str, size: f32, color: Color32) -> Arc<Galley> {
        self.fonts.with_pixels_per_point(self.ppp).layout_no_wrap(text.to_string(), FontId::new(size, bold()), color)
    }

    /// Text wrapped at `width`; `max_rows == 1` with `ellipsis` cuts it with "…".
    pub fn layout_wrapped(&mut self, text: &str, size: f32, color: Color32, width: f32, max_rows: usize, ellipsis: bool) -> Arc<Galley> {
        let mut job = LayoutJob::simple(text.to_string(), FontId::proportional(size), color, width.max(1.0));
        job.wrap = TextWrapping { max_width: width.max(1.0), max_rows, break_anywhere: ellipsis, overflow_character: ellipsis.then_some('…') };
        self.fonts.with_pixels_per_point(self.ppp).layout_job(job)
    }

    pub fn row_height(&mut self, size: f32) -> f32 {
        self.fonts.with_pixels_per_point(self.ppp).row_height(&FontId::proportional(size))
    }

    pub(crate) fn atlas(&self) -> &TextureAtlas {
        self.fonts.texture_atlas()
    }

    pub(crate) fn take_delta(&mut self) -> Option<ImageDelta> {
        self.fonts.font_image_delta()
    }
}

impl Default for TextServer {
    fn default() -> Self {
        Self::new()
    }
}
