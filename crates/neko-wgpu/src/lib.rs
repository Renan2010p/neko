//! wgpu renderer backend.
//!
//! wgpu picks the native API at runtime: Vulkan, Metal, DX12, OpenGL/GLES,
//! or WebGPU/WebGL2 in the browser. Restrict it with [`GpuApi`] or the
//! `WGPU_BACKEND` environment variable.

mod canvas;

use canvas::CanvasRenderer;

use std::sync::Arc;

use neko_platform::Window;
use neko_render::{Frame, RenderError, Renderer, RendererInfo};

/// Which native graphics API wgpu may use.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GpuApi {
    /// Let wgpu choose the best available API.
    #[default]
    Auto,
    Vulkan,
    Metal,
    Dx12,
    Gl,
}

impl GpuApi {
    fn backends(self) -> wgpu::Backends {
        match self {
            GpuApi::Auto => wgpu::Backends::all(),
            GpuApi::Vulkan => wgpu::Backends::VULKAN,
            GpuApi::Metal => wgpu::Backends::METAL,
            GpuApi::Dx12 => wgpu::Backends::DX12,
            GpuApi::Gl => wgpu::Backends::GL,
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.to_ascii_lowercase().as_str() {
            "auto" => Self::Auto,
            "vulkan" | "vk" => Self::Vulkan,
            "metal" | "mtl" => Self::Metal,
            "dx12" | "d3d12" => Self::Dx12,
            "gl" | "gles" | "opengl" => Self::Gl,
            _ => return None,
        })
    }
}

/// The window's display connection, as wgpu wants it (`Debug` + `HasDisplayHandle`).
struct DisplayOf(Arc<dyn neko_platform::NativeHandles>);

impl std::fmt::Debug for DisplayOf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DisplayOf")
    }
}

impl wgpu::rwh::HasDisplayHandle for DisplayOf {
    fn display_handle(&self) -> Result<wgpu::rwh::DisplayHandle<'_>, wgpu::rwh::HandleError> {
        self.0.display_handle()
    }
}

/// A dialog window drawn with the main device.
struct Extra {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    needs_reconfigure: bool,
}

pub struct WgpuRenderer {
    instance: wgpu::Instance,
    extra: std::collections::HashMap<u64, Extra>,
    surface: wgpu::Surface<'static>,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    canvas: CanvasRenderer,
    needs_reconfigure: bool,
}

impl WgpuRenderer {
    pub fn new(window: Arc<dyn Window>, api: GpuApi) -> Result<Self, RenderError> {
        // The surface holds an `Arc` of the window, so it can never outlive it.
        let native = window.clone().surface().native().ok_or_else(|| RenderError::Surface("wgpu needs a native window surface".into()))?;
        // OpenGL (EGL) needs the display at instance creation to use the window's surface.
        let mut desc = wgpu::InstanceDescriptor::new_with_display_handle_from_env(Box::new(DisplayOf(native.clone())));
        if std::env::var_os("WGPU_BACKEND").is_none() {
            desc.backends = api.backends();
        }
        let instance = wgpu::Instance::new(desc);
        let (width, height) = window.inner_size();
        let surface = instance.create_surface(native).map_err(|e| RenderError::Surface(e.to_string()))?;

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .map_err(|e| RenderError::NoAdapter(e.to_string()))?;

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("neko"),
            required_limits: wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()),
            ..Default::default()
        }))
        .map_err(|e| RenderError::Device(e.to_string()))?;

        let caps = surface.get_capabilities(&adapter);
        // 2D draws in sRGB space onto a non-sRGB target, like Godot's canvas renderer.
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| !f.is_srgb())
            .or_else(|| caps.formats.first().copied())
            .ok_or_else(|| RenderError::Surface("surface reports no formats".into()))?;
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: caps.alpha_modes.first().copied().unwrap_or(wgpu::CompositeAlphaMode::Auto),
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
            ..surface
                .get_default_config(&adapter, width.max(1), height.max(1))
                .ok_or_else(|| RenderError::Surface("surface not supported by this adapter".into()))?
        };
        surface.configure(&device, &config);
        let canvas = CanvasRenderer::new(&device, format);
        let info = adapter.get_info();
        log::info!("wgpu: {} on {:?} ({format:?})", info.name, info.backend);
        Ok(Self { instance, extra: std::collections::HashMap::new(), surface, adapter, device, queue, config, canvas, needs_reconfigure: false })
    }

    pub fn factory(api: GpuApi) -> impl Fn(Arc<dyn Window>) -> Result<Box<dyn Renderer>, RenderError> {
        move |w| Ok(Box::new(Self::new(w, api)?) as Box<dyn Renderer>)
    }
}

impl Renderer for WgpuRenderer {
    fn info(&self) -> RendererInfo {
        let i = self.adapter.get_info();
        RendererInfo { backend: "wgpu".into(), api: format!("{:?}", i.backend), device: i.name }
    }

    fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    fn max_texture_side(&self) -> usize {
        self.device.limits().max_texture_dimension_2d as usize
    }

    fn render(&mut self, frame: &Frame<'_>) -> Result<(), RenderError> {
        draw_to(&self.device, &self.queue, &mut self.canvas, &self.surface, &self.config, &mut self.needs_reconfigure, frame);
        self.canvas.free(frame.textures);
        Ok(())
    }

    fn attach_window(&mut self, id: u64, window: Arc<dyn Window>) -> Result<(), RenderError> {
        let native = window.clone().surface().native().ok_or_else(|| RenderError::Surface("wgpu needs a native window surface".into()))?;
        let surface = self.instance.create_surface(native).map_err(|e| RenderError::Surface(e.to_string()))?;
        let caps = surface.get_capabilities(&self.adapter);
        // The canvas pipeline was built for the main window's format.
        if !caps.formats.contains(&self.config.format) {
            return Err(RenderError::Surface(format!("window does not support {:?}", self.config.format)));
        }
        let (w, h) = window.inner_size();
        let config = wgpu::SurfaceConfiguration { width: w.max(1), height: h.max(1), alpha_mode: caps.alpha_modes.first().copied().unwrap_or(wgpu::CompositeAlphaMode::Auto), ..self.config.clone() };
        surface.configure(&self.device, &config);
        self.extra.insert(id, Extra { surface, config, needs_reconfigure: false });
        Ok(())
    }

    fn detach_window(&mut self, id: u64) {
        self.extra.remove(&id);
    }

    fn render_window(&mut self, id: u64, frame: &Frame<'_>) -> Result<(), RenderError> {
        let Some(e) = self.extra.get_mut(&id) else { return Ok(()) };
        let [w, h] = frame.size_px;
        if w > 0 && h > 0 && (e.config.width != w || e.config.height != h) {
            e.config.width = w;
            e.config.height = h;
            e.needs_reconfigure = true;
        }
        draw_to(&self.device, &self.queue, &mut self.canvas, &e.surface, &e.config, &mut e.needs_reconfigure, frame);
        Ok(())
    }
}

/// Draw one frame into a surface with the shared canvas pipeline.
fn draw_to(device: &wgpu::Device, queue: &wgpu::Queue, canvas: &mut CanvasRenderer, surface: &wgpu::Surface<'static>, config: &wgpu::SurfaceConfiguration, needs_reconfigure: &mut bool, frame: &Frame<'_>) {
    if *needs_reconfigure {
        surface.configure(device, config);
        *needs_reconfigure = false;
    }
    let output = match surface.get_current_texture() {
        wgpu::CurrentSurfaceTexture::Success(t) => t,
        wgpu::CurrentSurfaceTexture::Suboptimal(t) => {
            *needs_reconfigure = true;
            t
        }
        wgpu::CurrentSurfaceTexture::Occluded | wgpu::CurrentSurfaceTexture::Timeout => return,
        other => {
            log::warn!("surface unavailable ({other:?}), reconfiguring");
            *needs_reconfigure = true;
            return;
        }
    };
    let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
    canvas.prepare(device, queue, frame.textures, frame.ui, frame.size_px, frame.pixels_per_point);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("neko frame") });
    {
        let [r, g, b, a] = frame.clear_color.map(f64::from);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("neko canvas"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a }), store: wgpu::StoreOp::Store },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        canvas.draw(&mut pass);
    }
    queue.submit([encoder.finish()]);
    queue.present(output);
}

/// Render one frame on the GPU into memory (no window): straight RGBA8 rows.
/// Uses the same canvas pipeline as the window path; for screenshots and tests.
pub fn render_offscreen(frame: &Frame<'_>, api: GpuApi) -> Result<Vec<u8>, RenderError> {
    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
    if std::env::var_os("WGPU_BACKEND").is_none() {
        desc.backends = api.backends();
    }
    let instance = wgpu::Instance::new(desc);
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).map_err(|e| RenderError::NoAdapter(e.to_string()))?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor { label: Some("neko offscreen"), ..Default::default() })).map_err(|e| RenderError::Device(e.to_string()))?;
    let [w, h] = frame.size_px;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("offscreen target"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let mut canvas = CanvasRenderer::new(&device, format);
    canvas.prepare(&device, &queue, frame.textures, frame.ui, frame.size_px, frame.pixels_per_point);
    let row = (4 * w).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let readback = device.create_buffer(&wgpu::BufferDescriptor { label: Some("readback"), size: (row * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("offscreen") });
    {
        let [r, g, b, a] = frame.clear_color.map(f64::from);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("offscreen canvas"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a }), store: wgpu::StoreOp::Store },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        canvas.draw(&mut pass);
    }
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo { texture: &target, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::TexelCopyBufferInfo { buffer: &readback, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) } },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    queue.submit([encoder.finish()]);
    readback.map_async(wgpu::MapMode::Read, .., |_| {});
    device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| RenderError::Device(e.to_string()))?;
    let data = readback.get_mapped_range(..).map_err(|e| RenderError::Device(e.to_string()))?;
    let mut out = Vec::with_capacity((4 * w * h) as usize);
    for y in 0..h as usize {
        out.extend_from_slice(&data[y * row as usize..y * row as usize + 4 * w as usize]);
    }
    log::info!("offscreen frame rendered on {}", adapter.get_info().name);
    Ok(out)
}
