//! 2D canvas renderer, modeled on Godot's `RendererCanvasRenderRD`:
//! batches are already formed upstream (a new batch whenever the clip rect or
//! texture changes), each batch is one indexed draw with a scissor rect, and
//! blending is Godot's `BLEND_MODE_PREMULTIPLIED_ALPHA` (ONE, ONE_MINUS_SRC_ALPHA).

use std::collections::HashMap;

use neko_render::epaint::textures::{TextureFilter, TexturesDelta};
use neko_render::epaint::{ClippedPrimitive, ImageData, ImageDelta, Primitive, TextureId};
use wgpu::util::DeviceExt;

/// Bytes per vertex: pos (2 x f32), uv (2 x f32), color (u32, RGBA8).
const VERTEX_SIZE: u64 = 20;

struct Texture {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
}

struct Batch {
    texture: TextureId,
    scissor: [u32; 4],
    index_start: u32,
    index_count: u32,
    base_vertex: i32,
}

pub struct CanvasRenderer {
    pipeline: wgpu::RenderPipeline,
    locals: wgpu::Buffer,
    locals_group: wgpu::BindGroup,
    texture_layout: wgpu::BindGroupLayout,
    linear: wgpu::Sampler,
    nearest: wgpu::Sampler,
    textures: HashMap<TextureId, Texture>,
    batches: Vec<Batch>,
    vertices: Option<wgpu::Buffer>,
    indices: Option<wgpu::Buffer>,
}

impl CanvasRenderer {
    pub fn new(device: &wgpu::Device, target: wgpu::TextureFormat) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("neko canvas"), source: wgpu::ShaderSource::Wgsl(include_str!("canvas.wgsl").into()) });
        let locals = device.create_buffer(&wgpu::BufferDescriptor { label: Some("canvas locals"), size: 16, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let locals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("canvas locals"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });
        let locals_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("canvas locals"),
            layout: &locals_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: locals.as_entire_binding() }],
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("canvas texture"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { multisampled: false, sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2 },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("canvas"), bind_group_layouts: &[Some(&locals_layout), Some(&texture_layout)], immediate_size: 0 });
        // Godot's BLEND_MODE_PREMULTIPLIED_ALPHA (material_storage.cpp).
        let premultiplied = wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("canvas"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: VERTEX_SIZE,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Uint32],
                })],
            },
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState { format: target, blend: Some(wgpu::BlendState { color: premultiplied, alpha: premultiplied }), write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = |filter: wgpu::FilterMode| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("canvas sampler"),
                mag_filter: filter,
                min_filter: filter,
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                ..Default::default()
            })
        };
        Self {
            pipeline,
            locals,
            locals_group,
            texture_layout,
            linear: sampler(wgpu::FilterMode::Linear),
            nearest: sampler(wgpu::FilterMode::Nearest),
            textures: HashMap::new(),
            batches: Vec::new(),
            vertices: None,
            indices: None,
        }
    }

    fn set_texture(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, id: TextureId, delta: &ImageDelta) {
        let ImageData::Color(image) = &delta.image;
        let [w, h] = image.size;
        if w == 0 || h == 0 {
            return;
        }
        let bytes: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
        let size = wgpu::Extent3d { width: w as u32, height: h as u32, depth_or_array_layers: 1 };
        let origin = match delta.pos {
            Some([x, y]) => wgpu::Origin3d { x: x as u32, y: y as u32, z: 0 },
            None => {
                // A whole image: (re)create the texture, like Godot's texture_2d_initialize.
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("canvas texture"),
                    size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                });
                let sampler = if delta.options.magnification == TextureFilter::Nearest { &self.nearest } else { &self.linear };
                let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("canvas texture"),
                    layout: &self.texture_layout,
                    entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) }],
                });
                self.textures.insert(id, Texture { texture, bind_group });
                wgpu::Origin3d::ZERO
            }
        };
        let Some(t) = self.textures.get(&id) else { return };
        queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &t.texture, mip_level: 0, origin, aspect: wgpu::TextureAspect::All },
            &bytes,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * w as u32), rows_per_image: Some(h as u32) },
            size,
        );
    }

    /// Upload textures and geometry for a frame. Call before the render pass.
    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, textures: &TexturesDelta, primitives: &[ClippedPrimitive], size_px: [u32; 2], ppp: f32) {
        for (id, deltas) in &textures.set {
            for d in deltas {
                self.set_texture(device, queue, *id, d);
            }
        }
        queue.write_buffer(&self.locals, 0, &[(size_px[0] as f32 / ppp).to_ne_bytes(), (size_px[1] as f32 / ppp).to_ne_bytes(), [0; 4], [0; 4]].concat());

        let mut vertices: Vec<u8> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();
        self.batches.clear();
        let mut vertex_count = 0i32;
        for prim in primitives {
            let Primitive::Mesh(mesh) = &prim.primitive else { continue };
            if mesh.indices.is_empty() {
                continue;
            }
            // Clip rect in physical pixels, clamped to the target (the scissor).
            let c = prim.clip_rect;
            let x0 = (c.min.x * ppp).round().clamp(0.0, size_px[0] as f32) as u32;
            let y0 = (c.min.y * ppp).round().clamp(0.0, size_px[1] as f32) as u32;
            let x1 = (c.max.x * ppp).round().clamp(0.0, size_px[0] as f32) as u32;
            let y1 = (c.max.y * ppp).round().clamp(0.0, size_px[1] as f32) as u32;
            if x1 <= x0 || y1 <= y0 {
                continue;
            }
            for v in &mesh.vertices {
                vertices.extend_from_slice(&v.pos.x.to_ne_bytes());
                vertices.extend_from_slice(&v.pos.y.to_ne_bytes());
                vertices.extend_from_slice(&v.uv.x.to_ne_bytes());
                vertices.extend_from_slice(&v.uv.y.to_ne_bytes());
                vertices.extend_from_slice(&u32::from_le_bytes(v.color.to_array()).to_ne_bytes());
            }
            self.batches.push(Batch { texture: mesh.texture_id, scissor: [x0, y0, x1 - x0, y1 - y0], index_start: indices.len() as u32, index_count: mesh.indices.len() as u32, base_vertex: vertex_count });
            indices.extend_from_slice(&mesh.indices);
            vertex_count += mesh.vertices.len() as i32;
        }
        self.vertices = (!vertices.is_empty()).then(|| device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("canvas vertices"), contents: &vertices, usage: wgpu::BufferUsages::VERTEX }));
        let index_bytes: Vec<u8> = indices.iter().flat_map(|i| i.to_ne_bytes()).collect();
        self.indices = (!index_bytes.is_empty()).then(|| device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("canvas indices"), contents: &index_bytes, usage: wgpu::BufferUsages::INDEX }));
    }

    /// Record the batches into a render pass.
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        let (Some(vb), Some(ib)) = (&self.vertices, &self.indices) else { return };
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.locals_group, &[]);
        pass.set_vertex_buffer(0, vb.slice(..));
        pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
        for b in &self.batches {
            let Some(t) = self.textures.get(&b.texture) else { continue };
            pass.set_scissor_rect(b.scissor[0], b.scissor[1], b.scissor[2], b.scissor[3]);
            pass.set_bind_group(1, &t.bind_group, &[]);
            pass.draw_indexed(b.index_start..b.index_start + b.index_count, b.base_vertex, 0..1);
        }
    }

    /// Free textures the UI no longer uses (after the frame is submitted).
    pub fn free(&mut self, textures: &TexturesDelta) {
        for id in &textures.free {
            if let Some(t) = self.textures.remove(id) {
                t.texture.destroy();
            }
        }
    }
}
