//! Icons taken from Godot's editor.
//!
//! The SVGs live in `assets/godot_icons` (MIT, see the LICENSE/COPYRIGHT and
//! UPSTREAM files there) and `build.rs` rasterizes them into one atlas, which
//! is uploaded once as a neko-ui texture. Node class icons come colored per
//! category; action icons are light gray and get tinted by the caller.

use neko::core::ClassDb;
use neko_ui::{Color32, ColorImage, Image, Rect, TextureId, Ui, pos2, vec2};

mod table {
    include!(concat!(env!("OUT_DIR"), "/godot_icons.rs"));
}

/// UV rect of an icon in the atlas, if one was vendored for it.
pub fn uv(name: &str) -> Option<Rect> {
    let name = match name {
        "GpuParticles2D" => "GPUParticles2D",
        "GpuParticles3D" => "GPUParticles3D",
        "HttpRequest" => "HTTPRequest",
        other => other,
    };
    let (_, uv) = table::ICONS.iter().find(|(n, _)| *n == name)?;
    Some(Rect::from_min_max(pos2(uv[0], uv[1]), pos2(uv[2], uv[3])))
}

/// The uploaded atlas.
#[derive(Clone, Copy)]
pub struct Icons {
    texture: TextureId,
}

impl Icons {
    pub fn load(ui: &mut Ui) -> Self {
        Self { texture: ui.add_texture(decode_atlas()) }
    }

    /// An icon by Godot name ("Add", "Play", "Node2D"...) at 16 px.
    pub fn get(&self, name: &str) -> Option<Image> {
        uv(name).map(|uv| Image::new(self.texture, uv, vec2(16.0, 16.0)))
    }

    pub fn tinted(&self, name: &str, tint: Color32) -> Option<Image> {
        self.get(name).map(|i| i.tinted(tint))
    }

    /// A node class's icon, falling back to the nearest ancestor that has one
    /// (Godot's class icon lookup).
    pub fn class(&self, class: &str) -> Option<Image> {
        ClassDb::get().ancestry(class).into_iter().rev().find_map(|c| self.get(c)).or_else(|| self.get("Node"))
    }
}

fn decode_atlas() -> ColorImage {
    let mut reader = png::Decoder::new(std::io::Cursor::new(table::ATLAS_PNG)).read_info().expect("the build script wrote a valid atlas");
    let mut buf = vec![0; reader.output_buffer_size().expect("atlas size is known")];
    let info = reader.next_frame(&mut buf).expect("atlas has one frame");
    debug_assert_eq!((info.width, info.height), (table::ATLAS_WIDTH, table::ATLAS_HEIGHT));
    ColorImage::from_rgba_unmultiplied([info.width as usize, info.height as usize], &buf[..info.buffer_size()])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every class the editor can show must have an icon of its own or inherit
    /// one, as Godot's class icon lookup does.
    #[test]
    fn every_class_resolves_to_an_icon() {
        let db = ClassDb::get();
        for class in db.all() {
            let found = db.ancestry(class.name).into_iter().rev().find(|name| uv(name).is_some());
            assert!(found.is_some(), "{} has no icon and no ancestor with one", class.name);
        }
    }
}
