//! Render a scene to a PNG without a window, with the same code the game uses.
//!
//! ```sh
//! cargo run -p neko --example render_scene --features render-soft -- <project dir> <res://scene.scn> <out.png> [--debug-collisions]
//! ```

use neko::render::{Frame, Renderer};
use neko::scene_runner::{CLEAR_COLOR, SceneRunner, VIEWPORT_SIZE};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [dir, scene, out, ..] = args.as_slice() else {
        eprintln!("usage: render_scene <project dir> <res://scene.scn> <out.png> [--debug-collisions]");
        std::process::exit(2);
    };
    let mut runner = SceneRunner::load(std::path::Path::new(dir), scene).unwrap_or_else(|e| panic!("{e}"));
    runner.debug_collisions = args.iter().any(|a| a == "--debug-collisions");
    // `--frames N` runs the scene's scripts for N frames at 60 FPS first;
    // `--hold KEY` keeps a key down (Godot key name, e.g. RIGHT, SPACE, A).
    let frames: usize = args.iter().position(|a| a == "--frames").and_then(|i| args.get(i + 1)).and_then(|n| n.parse().ok()).unwrap_or(0);
    let hold = args.iter().position(|a| a == "--hold").and_then(|i| args.get(i + 1)).cloned();
    let text = std::fs::read_to_string(std::path::Path::new(dir).join("project.neko")).or_else(|_| std::fs::read_to_string(std::path::Path::new(dir).join("project.godot"))).unwrap_or_default();
    let settings = neko::core::godot::config::ConfigFile::parse(&text).unwrap_or_default();
    // The project's background color, as the game uses it.
    let clear = match settings.get("rendering", "environment/defaults/default_clear_color").and_then(|v| v.numbers("Color")) {
        Some(c) if c.len() == 4 => [c[0] as f32, c[1] as f32, c[2] as f32, c[3] as f32],
        _ => CLEAR_COLOR,
    };
    if frames > 0 {
        runner.start_scripts(&settings);
        if let Some(k) = &hold {
            let code = neko::script::builtins::global_constant(&format!("KEY_{k}")).and_then(|v| v.as_i64()).unwrap_or(0);
            runner.key(code, true, false);
        }
        for _ in 0..frames {
            if !runner.update(1.0 / 60.0) {
                println!("the game called quit()");
                break;
            }
        }
    }
    let (w, h) = VIEWPORT_SIZE;
    let mut ui = neko::ui::Ui::new(neko::ui::Theme::neko_dark());
    ui.theme.background = neko::ui::Color32::TRANSPARENT;
    let root = ui.root();
    let canvas = ui.add(root, neko::ui::Control::canvas());
    let mut r = neko_soft::SoftRenderer::offscreen(w, h);
    for _ in 0..2 {
        runner.draw(&mut ui, canvas);
        let mut o = ui.frame((w, h), 1.0, 4096);
        r.render(&Frame { clear_color: clear, size_px: [w, h], pixels_per_point: 1.0, textures: &o.textures, ui: &o.primitives }).unwrap();
        o.textures.clear();
    }
    let file = std::fs::File::create(out).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&r.rgba8()).unwrap();
    println!("wrote {out}");
}
