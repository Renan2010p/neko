//! Software (CPU) renderer.
//!
//! Rasterizes the frame on the CPU and presents it with `softbuffer`, so it
//! works on any OS with a window and no GPU at all. Without a window
//! ([`SoftRenderer::offscreen`]) it renders into memory, which the editor
//! uses for `--screenshot` and tests.

use std::collections::HashMap;
#[cfg(feature = "window")]
use std::sync::Arc;

#[cfg(feature = "window")]
use neko_platform::{NativeHandles, Window};
use neko_render::epaint::{ClippedPrimitive, ImageData, Mesh, Primitive, TextureId};
use neko_render::{Frame, RenderError, Renderer, RendererInfo};

struct Texture {
    w: usize,
    h: usize,
    /// Premultiplied sRGBA.
    px: Vec<[u8; 4]>,
}

#[cfg(feature = "window")]
type Surface = softbuffer::Surface<Arc<dyn NativeHandles>, Arc<dyn NativeHandles>>;

pub struct SoftRenderer {
    width: usize,
    height: usize,
    /// Premultiplied sRGBA framebuffer.
    color: Vec<[u8; 4]>,
    textures: HashMap<TextureId, Texture>,
    #[cfg(feature = "window")]
    surface: Option<Surface>,
    /// Extra windows: their own framebuffer and surface, shared textures.
    #[cfg(feature = "window")]
    extra: HashMap<u64, ExtraWindow>,
}

/// Width, height, framebuffer and surface of an extra window.
#[cfg(feature = "window")]
type ExtraWindow = (usize, usize, Vec<[u8; 4]>, Option<Surface>);

impl SoftRenderer {
    /// Render into memory only.
    pub fn offscreen(width: u32, height: u32) -> Self {
        let mut r = Self {
            width: 0,
            height: 0,
            color: Vec::new(),
            textures: HashMap::new(),
            #[cfg(feature = "window")]
            surface: None,
            #[cfg(feature = "window")]
            extra: HashMap::new(),
        };
        r.resize(width, height);
        r
    }

    /// Render and present into a native window.
    #[cfg(feature = "window")]
    pub fn new(window: Arc<dyn Window>) -> Result<Self, RenderError> {
        let native = window.clone().surface().native().ok_or_else(|| RenderError::Surface("softbuffer needs a native window surface".into()))?;
        let ctx = softbuffer::Context::new(native.clone()).map_err(|e| RenderError::Surface(e.to_string()))?;
        let surface = Surface::new(&ctx, native).map_err(|e| RenderError::Surface(e.to_string()))?;
        let (w, h) = window.inner_size();
        let mut r = Self::offscreen(w, h);
        r.surface = Some(surface);
        r.resize(w, h);
        Ok(r)
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width as u32, self.height as u32)
    }

    /// Framebuffer as straight (non-premultiplied) RGBA8, row-major.
    pub fn rgba8(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.color.len() * 4);
        for &[r, g, b, a] in &self.color {
            if a == 0 {
                out.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                let un = |c: u8| ((c as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8;
                out.extend_from_slice(&[un(r), un(g), un(b), a]);
            }
        }
        out
    }

    fn set_texture(&mut self, id: TextureId, delta: &neko_render::epaint::ImageDelta) {
        let ImageData::Color(img) = &delta.image;
        let [w, h] = img.size;
        let src: Vec<[u8; 4]> = img.pixels.iter().map(|c| c.to_array()).collect();
        match delta.pos {
            None => {
                self.textures.insert(id, Texture { w, h, px: src });
            }
            Some([x0, y0]) => {
                let Some(t) = self.textures.get_mut(&id) else { return };
                for y in 0..h {
                    for x in 0..w {
                        if x0 + x < t.w && y0 + y < t.h {
                            t.px[(y0 + y) * t.w + x0 + x] = src[y * w + x];
                        }
                    }
                }
            }
        }
    }

    fn draw_mesh(&mut self, mesh: &Mesh, clip: [f32; 4], ppp: f32) {
        let Some(tex) = self.textures.get(&mesh.texture_id) else { return };
        let (fw, fh) = (self.width as f32, self.height as f32);
        let cx0 = (clip[0] * ppp).max(0.0);
        let cy0 = (clip[1] * ppp).max(0.0);
        let cx1 = (clip[2] * ppp).min(fw);
        let cy1 = (clip[3] * ppp).min(fh);
        if cx0 >= cx1 || cy0 >= cy1 {
            return;
        }
        for tri in mesh.indices.as_chunks::<3>().0 {
            let v = [&mesh.vertices[tri[0] as usize], &mesh.vertices[tri[1] as usize], &mesh.vertices[tri[2] as usize]];
            let p = v.map(|v| (v.pos.x * ppp, v.pos.y * ppp));
            let area = edge(p[0], p[1], p[2]);
            if area.abs() < 1e-6 {
                continue;
            }
            let minx = p.iter().map(|q| q.0).fold(f32::MAX, f32::min).max(cx0).floor() as i32;
            let maxx = p.iter().map(|q| q.0).fold(f32::MIN, f32::max).min(cx1).ceil() as i32;
            let miny = p.iter().map(|q| q.1).fold(f32::MAX, f32::min).max(cy0).floor() as i32;
            let maxy = p.iter().map(|q| q.1).fold(f32::MIN, f32::max).min(cy1).ceil() as i32;
            let col = v.map(|v| v.color.to_array().map(|c| c as f32));
            let uv = v.map(|v| (v.uv.x, v.uv.y));
            for y in miny.max(0)..maxy.min(self.height as i32) {
                let py = y as f32 + 0.5;
                if py < cy0 || py > cy1 {
                    continue;
                }
                for x in minx.max(0)..maxx.min(self.width as i32) {
                    let px = x as f32 + 0.5;
                    if px < cx0 || px > cx1 {
                        continue;
                    }
                    let w0 = edge(p[1], p[2], (px, py)) / area;
                    let w1 = edge(p[2], p[0], (px, py)) / area;
                    let w2 = 1.0 - w0 - w1;
                    if w0 < -1e-4 || w1 < -1e-4 || w2 < -1e-4 {
                        continue;
                    }
                    let u = uv[0].0 * w0 + uv[1].0 * w1 + uv[2].0 * w2;
                    let vv = uv[0].1 * w0 + uv[1].1 * w1 + uv[2].1 * w2;
                    let t = sample(tex, u, vv);
                    let mut s = [0f32; 4];
                    for i in 0..4 {
                        let vc = col[0][i] * w0 + col[1][i] * w1 + col[2][i] * w2;
                        s[i] = t[i] * vc / 255.0;
                    }
                    let d = &mut self.color[y as usize * self.width + x as usize];
                    let inv = 1.0 - s[3] / 255.0;
                    for i in 0..4 {
                        d[i] = (s[i] + d[i] as f32 * inv).round().clamp(0.0, 255.0) as u8;
                    }
                }
            }
        }
    }

    fn draw(&mut self, prims: &[ClippedPrimitive], ppp: f32) {
        for p in prims {
            if let Primitive::Mesh(mesh) = &p.primitive {
                let c = p.clip_rect;
                self.draw_mesh(mesh, [c.min.x, c.min.y, c.max.x, c.max.y], ppp);
            }
        }
    }
}

fn edge(a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> f32 {
    (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)
}

/// Bilinear sample, returns premultiplied RGBA in 0..=255.
fn sample(t: &Texture, u: f32, v: f32) -> [f32; 4] {
    let x = (u * t.w as f32 - 0.5).clamp(0.0, (t.w - 1) as f32);
    let y = (v * t.h as f32 - 0.5).clamp(0.0, (t.h - 1) as f32);
    let (x0, y0) = (x as usize, y as usize);
    let (x1, y1) = ((x0 + 1).min(t.w - 1), (y0 + 1).min(t.h - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let at = |x: usize, y: usize| t.px[y * t.w + x];
    let (a, b, c, d) = (at(x0, y0), at(x1, y0), at(x0, y1), at(x1, y1));
    let mut out = [0f32; 4];
    for i in 0..4 {
        let top = a[i] as f32 * (1.0 - fx) + b[i] as f32 * fx;
        let bot = c[i] as f32 * (1.0 - fx) + d[i] as f32 * fx;
        out[i] = top * (1.0 - fy) + bot * fy;
    }
    out
}

impl Renderer for SoftRenderer {
    fn info(&self) -> RendererInfo {
        RendererInfo { backend: "soft".into(), api: "CPU".into(), device: std::env::consts::ARCH.into() }
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.width = width.max(1) as usize;
        self.height = height.max(1) as usize;
        self.color = vec![[0; 4]; self.width * self.height];
        #[cfg(feature = "window")]
        if let Some(s) = &mut self.surface {
            let nz = |v: usize| std::num::NonZeroU32::new(v as u32).unwrap();
            let _ = s.resize(nz(self.width), nz(self.height));
        }
    }

    fn render(&mut self, frame: &Frame<'_>) -> Result<(), RenderError> {
        let [w, h] = frame.size_px;
        if (w as usize, h as usize) != (self.width, self.height) {
            self.resize(w, h);
        }
        for (id, deltas) in &frame.textures.set {
            for delta in deltas {
                self.set_texture(*id, delta);
            }
        }
        let [r, g, b, a] = frame.clear_color;
        let a8 = (a.clamp(0.0, 1.0) * 255.0).round() as u8;
        // sRGB in, premultiplied sRGB stored (the same values the wgpu path writes).
        let c8 = |v: f32| (v.clamp(0.0, 1.0) * a.clamp(0.0, 1.0) * 255.0).round() as u8;
        let clear = [c8(r), c8(g), c8(b), a8];
        self.color.fill(clear);
        self.draw(frame.ui, frame.pixels_per_point);
        for id in &frame.textures.free {
            self.textures.remove(id);
        }
        #[cfg(feature = "window")]
        if let Some(s) = &mut self.surface {
            let mut buf = s.buffer_mut().map_err(|e| RenderError::Surface(e.to_string()))?;
            for (dst, &[r, g, b, _]) in buf.iter_mut().zip(&self.color) {
                *dst = (r as u32) << 16 | (g as u32) << 8 | b as u32;
            }
            buf.present().map_err(|e| RenderError::Surface(e.to_string()))?;
        }
        Ok(())
    }

    #[cfg(feature = "window")]
    fn attach_window(&mut self, id: u64, window: Arc<dyn Window>) -> Result<(), RenderError> {
        let native = window.clone().surface().native().ok_or_else(|| RenderError::Surface("softbuffer needs a native window surface".into()))?;
        let ctx = softbuffer::Context::new(native.clone()).map_err(|e| RenderError::Surface(e.to_string()))?;
        let surface = Surface::new(&ctx, native).map_err(|e| RenderError::Surface(e.to_string()))?;
        self.extra.insert(id, (0, 0, Vec::new(), Some(surface)));
        Ok(())
    }

    #[cfg(feature = "window")]
    fn detach_window(&mut self, id: u64) {
        self.extra.remove(&id);
    }

    /// Draw with the window's own framebuffer swapped in.
    #[cfg(feature = "window")]
    fn render_window(&mut self, id: u64, frame: &Frame<'_>) -> Result<(), RenderError> {
        let Some(mut e) = self.extra.remove(&id) else { return Ok(()) };
        std::mem::swap(&mut self.width, &mut e.0);
        std::mem::swap(&mut self.height, &mut e.1);
        std::mem::swap(&mut self.color, &mut e.2);
        std::mem::swap(&mut self.surface, &mut e.3);
        let r = self.render(frame);
        std::mem::swap(&mut self.width, &mut e.0);
        std::mem::swap(&mut self.height, &mut e.1);
        std::mem::swap(&mut self.color, &mut e.2);
        std::mem::swap(&mut self.surface, &mut e.3);
        self.extra.insert(id, e);
        r
    }
}
