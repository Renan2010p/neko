//! Draw the README banner with Neko itself (neko-ui + the software renderer).
//!
//! `cargo run -p neko --example banner --features render-soft -- <background.png> <out.png>`

use neko::render::{Frame, Renderer};
use neko::ui::{Align2, Color32, ColorImage, Control, FontId, Image, Rect, Ui, bold, pos2, vec2};

const W: u32 = 1600;
const H: u32 = 400;
const AMBER: Color32 = Color32::from_rgb(0xf2, 0xb5, 0x44);

fn load_png(path: &str) -> ColorImage {
    let file = std::fs::File::open(path).expect("background image");
    let mut reader = png::Decoder::new(std::io::BufReader::new(file)).read_info().expect("png");
    let mut buf = vec![0; reader.output_buffer_size().expect("size")];
    let info = reader.next_frame(&mut buf).expect("frame");
    let px = &buf[..info.buffer_size()];
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgb => px.chunks_exact(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        _ => px.to_vec(),
    };
    ColorImage::from_rgba_unmultiplied([info.width as usize, info.height as usize], &rgba)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [bg, out, ..] = args.as_slice() else {
        eprintln!("usage: banner <background.png> <out.png>");
        std::process::exit(2);
    };
    let mut ui = Ui::new(neko::ui::Theme::neko_dark());
    let bg_img = load_png(bg);
    let [bw, bh] = bg_img.size;
    let tex = ui.add_texture(bg_img);
    let root = ui.root();
    let canvas = ui.add(root, Control::canvas());
    let mut r = neko_soft::SoftRenderer::offscreen(W, H);
    for _ in 0..2 {
        ui.paint_canvas(canvas, |p, rect| {
            // Background: the editor, scaled to cover, darkened like OpenShooter's banner.
            let scale = (rect.width() / bw as f32).max(rect.height() / bh as f32);
            let size = vec2(bw as f32, bh as f32) * scale;
            let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
            p.image(Image::new(tex, uv, size).tinted(Color32::from_gray(255)), Rect::from_center_size(rect.center(), size));
            p.rect_filled(rect, 0, Color32::from_rgba_unmultiplied(14, 12, 10, 200));
            // A warm glow behind the title.
            for i in 0..12 {
                let t = i as f32 / 12.0;
                let r = Rect::from_center_size(rect.center() - vec2(0.0, 30.0), vec2(900.0 * (1.0 - t * 0.6), 260.0 * (1.0 - t * 0.6)));
                p.rect_filled(r, (130.0 * (1.0 - t * 0.6)) as u8, Color32::from_rgba_unmultiplied(0xf2, 0xb5, 0x44, 3));
            }
            // Logo + title, centered as a group.
            let title_size = 150.0;
            let galley = p.text_server().layout_bold("NEKO", title_size, Color32::WHITE);
            let tw = galley.size().x;
            let logo = 150.0;
            let gap = 28.0;
            let total = logo + gap + tw;
            let left = rect.center().x - total / 2.0;
            let cy = rect.top() + 140.0;
            neko::ui::splash::draw_logo(p, Rect::from_center_size(pos2(left + logo / 2.0, cy), vec2(logo, logo)));
            let title_at = pos2(left + logo + gap, cy - galley.size().y / 2.0);
            let shadow = p.text_server().layout_bold("NEKO", title_size, Color32::from_black_alpha(160));
            p.galley(title_at + vec2(4.0, 5.0), shadow, Color32::from_black_alpha(160));
            p.galley(title_at, galley, Color32::WHITE);
            // Amber rule, subtitle and the small caps line.
            let rule_y = rect.top() + 250.0;
            p.rect_filled(Rect::from_center_size(pos2(rect.center().x, rule_y), vec2(360.0, 4.0)), 2, AMBER);
            p.text(pos2(rect.center().x + 2.0, rule_y + 42.0), Align2::CENTER_CENTER, "The Godot engine, rewritten in Rust", FontId::proportional(34.0), Color32::from_black_alpha(150));
            p.text(pos2(rect.center().x, rule_y + 40.0), Align2::CENTER_CENTER, "The Godot engine, rewritten in Rust", FontId::proportional(34.0), Color32::WHITE);
            p.text(pos2(rect.center().x, rule_y + 88.0), Align2::CENTER_CENTER, "RL PROJECTS  ·  BUILT WITH RUST  ·  OPENS GODOT PROJECTS  ·  MPL-2.0", FontId::new(22.0, bold()), AMBER);
        });
        let mut o = ui.frame((W, H), 1.0, 8192);
        r.render(&Frame { clear_color: [0.0, 0.0, 0.0, 1.0], size_px: [W, H], pixels_per_point: 1.0, textures: &o.textures, ui: &o.primitives }).expect("render");
        o.textures.clear();
    }
    let file = std::fs::File::create(out).expect("output");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), W, H);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().expect("png header").write_image_data(&r.rgba8()).expect("png data");
    println!("wrote {out}");
}
