//! Packs `assets/godot_icons/*.svg` into one RGBA atlas.
//!
//! Godot ships its editor icons as 16x16 monochrome-ish SVGs, each already
//! colored for its category (2D blue, 3D red, UI green). Rendering them once at
//! build time means the editor only needs to upload a single texture and can
//! draw them at any size, on any backend, without an SVG runtime.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// Pixels per icon cell in the atlas (56 px of art + 4 px transparent margin).
const CELL: u32 = 64;
const MARGIN: f32 = 4.0;
const COLS: u32 = 8;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let icons_dir = manifest.join("assets/godot_icons");
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));

    println!("cargo:rerun-if-changed={}", icons_dir.display());

    let mut files: Vec<PathBuf> = std::fs::read_dir(&icons_dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", icons_dir.display()))
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            (path.extension()? == "svg").then_some(path)
        })
        .collect();
    files.sort();

    let rows = (files.len() as u32).div_ceil(COLS).max(1);
    let width = COLS * CELL;
    let height = rows * CELL;
    let mut atlas = resvg::tiny_skia::Pixmap::new(width, height).expect("atlas is not empty");

    let mut table = String::new();
    writeln!(table, "pub const ATLAS_WIDTH: u32 = {width};").unwrap();
    writeln!(table, "pub const ATLAS_HEIGHT: u32 = {height};").unwrap();
    writeln!(
        table,
        "pub const ATLAS_PNG: &[u8] = include_bytes!(concat!(env!(\"OUT_DIR\"), \"/godot_icons.png\"));"
    )
    .unwrap();
    writeln!(table, "pub const ICONS: &[(&str, [f32; 4])] = &[").unwrap();

    for (i, path) in files.iter().enumerate() {
        let name = icon_name(path);
        let source = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        let tree = resvg::usvg::Tree::from_str(&source, &resvg::usvg::Options::default())
            .unwrap_or_else(|e| panic!("cannot parse {}: {e}", path.display()));

        let size = tree.size();
        let scale = (CELL as f32 - 2.0 * MARGIN) / size.width().max(size.height());
        let col = (i as u32) % COLS;
        let row = (i as u32) / COLS;
        let cell_x = (col * CELL) as f32;
        let cell_y = (row * CELL) as f32;
        // Center the art inside its cell (icons are not all 16x16; a few use
        // a smaller viewBox, e.g. AudioStreamPlayer is 2.4 units wide).
        let dx = cell_x + MARGIN + (CELL as f32 - 2.0 * MARGIN - size.width() * scale) / 2.0;
        let dy = cell_y + MARGIN + (CELL as f32 - 2.0 * MARGIN - size.height() * scale) / 2.0;
        let transform = resvg::tiny_skia::Transform::from_translate(dx, dy).pre_scale(scale, scale);
        resvg::render(&tree, transform, &mut atlas.as_mut());

        // UVs cover the art itself, not the transparent margin, so the icon
        // fills the rect the editor gives it.
        let u0 = (cell_x + MARGIN) / width as f32;
        let v0 = (cell_y + MARGIN) / height as f32;
        let u1 = (cell_x + MARGIN + CELL as f32 - 2.0 * MARGIN) / width as f32;
        let v1 = (cell_y + MARGIN + CELL as f32 - 2.0 * MARGIN) / height as f32;
        writeln!(table, "    ({name:?}, [{u0:.6}, {v0:.6}, {u1:.6}, {v1:.6}]),").unwrap();
    }

    writeln!(table, "];").unwrap();

    std::fs::write(out_dir.join("godot_icons.rs"), table).expect("write icon table");
    atlas.save_png(out_dir.join("godot_icons.png")).expect("write icon atlas");
}

fn icon_name(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_else(|| panic!("{} has no file name", path.display()))
        .to_string()
}
