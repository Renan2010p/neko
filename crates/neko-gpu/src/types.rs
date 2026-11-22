//! Descriptions passed to [`crate::GpuBackend`].

use crate::{BufferId, SamplerId, ShaderId, TextureId};

/// Which API a backend drives; informative only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuApi {
    Vulkan,
    Metal,
    D3D12,
    OpenGl,
    WebGpu,
    Software,
    /// A platform API outside this list (consoles); see [`GpuInfo::api_name`].
    Other,
}

#[derive(Clone, Debug)]
pub struct GpuInfo {
    pub api: GpuApi,
    /// Human-readable API name, e.g. "Vulkan 1.3" or a console API.
    pub api_name: String,
    /// Adapter name, e.g. "Intel(R) Graphics".
    pub adapter: String,
    pub limits: Limits,
}

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_texture_size: u32,
    pub max_uniform_sets: u32,
    pub max_vertex_buffers: u32,
    /// Required alignment of uniform buffer offsets.
    pub uniform_offset_alignment: u32,
    pub compute: bool,
}

impl Default for Limits {
    /// Conservative values every backend must reach (WebGPU's defaults).
    fn default() -> Self {
        Self { max_texture_size: 8192, max_uniform_sets: 4, max_vertex_buffers: 8, uniform_offset_alignment: 256, compute: true }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentMode {
    /// Wait for vertical blank (no tearing, capped to the refresh rate).
    Vsync,
    /// Present as soon as possible; may tear.
    Immediate,
    /// Triple-buffered: no tearing, newest frame wins.
    Mailbox,
}

#[derive(Clone, Copy, Debug)]
pub struct SwapChainDesc {
    pub width: u32,
    pub height: u32,
    pub present_mode: PresentMode,
}

bitflags_lite! {
    /// How a buffer is used.
    BufferUsage {
        VERTEX = 1,
        INDEX = 2,
        UNIFORM = 4,
        STORAGE = 8,
        /// Can be the target of copies and `write_buffer`.
        COPY_DST = 16,
        COPY_SRC = 32,
        INDIRECT = 64,
    }
}

bitflags_lite! {
    /// How a texture is used.
    TextureUsage {
        /// Sampled in shaders.
        SAMPLED = 1,
        /// Rendered into.
        RENDER_TARGET = 2,
        STORAGE = 4,
        COPY_DST = 8,
        COPY_SRC = 16,
    }
}

/// Minimal bitflags without a dependency.
macro_rules! bitflags_lite {
    ($(#[$m:meta])* $name:ident { $($(#[$fm:meta])* $flag:ident = $v:expr,)* }) => {
        $(#[$m])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
        pub struct $name(pub u32);
        impl $name {
            $($(#[$fm])* pub const $flag: Self = Self($v);)*
            pub const fn contains(self, other: Self) -> bool {
                self.0 & other.0 == other.0
            }
        }
        impl std::ops::BitOr for $name {
            type Output = Self;
            fn bitor(self, o: Self) -> Self {
                Self(self.0 | o.0)
            }
        }
    };
}
use bitflags_lite;

#[derive(Clone, Copy, Debug)]
pub struct BufferDesc {
    pub size: u64,
    pub usage: BufferUsage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextureFormat {
    Rgba8Unorm,
    Rgba8UnormSrgb,
    Bgra8Unorm,
    Bgra8UnormSrgb,
    R8Unorm,
    Rg8Unorm,
    Rgba16Float,
    Rgba32Float,
    R32Float,
    Depth32Float,
    Depth24PlusStencil8,
}

impl TextureFormat {
    /// Bytes per texel.
    pub fn block_size(self) -> u32 {
        match self {
            Self::R8Unorm => 1,
            Self::Rg8Unorm => 2,
            Self::Rgba8Unorm | Self::Rgba8UnormSrgb | Self::Bgra8Unorm | Self::Bgra8UnormSrgb | Self::R32Float | Self::Depth32Float | Self::Depth24PlusStencil8 => 4,
            Self::Rgba16Float => 8,
            Self::Rgba32Float => 16,
        }
    }

    pub fn is_depth(self) -> bool {
        matches!(self, Self::Depth32Float | Self::Depth24PlusStencil8)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureKind {
    D2,
    /// A 2D array with `layers` layers.
    D2Array,
    D3,
    Cube,
}

#[derive(Clone, Copy, Debug)]
pub struct TextureDesc {
    pub kind: TextureKind,
    pub format: TextureFormat,
    pub width: u32,
    pub height: u32,
    /// Depth for 3D textures, layer count for arrays (6 for cubes).
    pub layers: u32,
    pub mips: u32,
    pub samples: u32,
    pub usage: TextureUsage,
}

impl TextureDesc {
    /// A plain 2D texture with one mip.
    pub fn d2(format: TextureFormat, width: u32, height: u32, usage: TextureUsage) -> Self {
        Self { kind: TextureKind::D2, format, width, height, layers: 1, mips: 1, samples: 1, usage }
    }
}

/// A box inside a texture, in texels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextureRegion {
    pub x: u32,
    pub y: u32,
    pub layer: u32,
    pub width: u32,
    pub height: u32,
}

impl TextureRegion {
    pub fn full(width: u32, height: u32) -> Self {
        Self { x: 0, y: 0, layer: 0, width, height }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Filter {
    Nearest,
    Linear,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Repeat {
    Clamp,
    Repeat,
    Mirror,
}

/// Godot's `CanvasItem.texture_filter` / `texture_repeat` map onto this.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SamplerDesc {
    pub mag: Filter,
    pub min: Filter,
    pub mip: Filter,
    pub repeat: Repeat,
    /// 1 disables anisotropic filtering.
    pub anisotropy: u8,
}

impl SamplerDesc {
    pub const LINEAR: Self = Self { mag: Filter::Linear, min: Filter::Linear, mip: Filter::Linear, repeat: Repeat::Clamp, anisotropy: 1 };
    pub const NEAREST: Self = Self { mag: Filter::Nearest, min: Filter::Nearest, mip: Filter::Nearest, repeat: Repeat::Clamp, anisotropy: 1 };
}

/// A shader code format a backend consumes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShaderFormat {
    /// WGSL source, compiled at runtime (wgpu, and the portable fallback).
    Wgsl,
    /// SPIR-V binary (Vulkan).
    SpirV,
    /// DXIL (D3D12).
    Dxil,
    /// Metal Shading Language source.
    Msl,
    /// GLSL ES 3.0 source (OpenGL / WebGL).
    Glsl,
    /// A platform's own binary format; the string names it (e.g. a console's
    /// shader compiler output). Only that platform's backend accepts it.
    Native(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShaderStage {
    Vertex,
    Fragment,
    Compute,
}

/// One shader program in as many formats as were built for it (Godot's
/// `RenderingShaderContainer`). Backends pick the first format they support.
#[derive(Clone, Copy, Debug)]
pub struct ShaderDesc<'a> {
    pub name: &'a str,
    pub code: &'a [ShaderCode<'a>],
}

#[derive(Clone, Copy, Debug)]
pub struct ShaderCode<'a> {
    pub format: ShaderFormat,
    /// Entry point per stage, e.g. `[(Vertex, "vs_main"), (Fragment, "fs_main")]`.
    pub entry_points: &'a [(ShaderStage, &'a str)],
    /// Source text (UTF-8) or binary, depending on `format`.
    pub bytes: &'a [u8],
}

impl ShaderDesc<'_> {
    /// The code for the first format in `supported` that this shader has.
    pub fn pick(&self, supported: &[ShaderFormat]) -> Option<&ShaderCode<'_>> {
        supported.iter().find_map(|f| self.code.iter().find(|c| c.format == *f))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VertexFormat {
    Float32,
    Float32x2,
    Float32x3,
    Float32x4,
    /// Four bytes read as 0..1 floats (packed colors).
    Unorm8x4,
    Uint32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VertexAttribute {
    pub location: u32,
    pub format: VertexFormat,
    pub offset: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct VertexBufferLayout {
    pub stride: u32,
    /// Advance per instance instead of per vertex.
    pub per_instance: bool,
    pub attributes: Vec<VertexAttribute>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Topology {
    Points,
    Lines,
    LineStrip,
    Triangles,
    TriangleStrip,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cull {
    None,
    Front,
    Back,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BlendFactor {
    Zero,
    One,
    SrcColor,
    OneMinusSrcColor,
    SrcAlpha,
    OneMinusSrcAlpha,
    DstColor,
    OneMinusDstColor,
    DstAlpha,
    OneMinusDstAlpha,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BlendOp {
    Add,
    Subtract,
    ReverseSubtract,
    Min,
    Max,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BlendComponent {
    pub src: BlendFactor,
    pub dst: BlendFactor,
    pub op: BlendOp,
}

/// Color blending; `None` in a pipeline means opaque writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Blend {
    pub color: BlendComponent,
    pub alpha: BlendComponent,
}

impl Blend {
    /// Godot's `BLEND_MODE_MIX` (material_storage.cpp).
    pub const MIX: Self = Self {
        color: BlendComponent { src: BlendFactor::SrcAlpha, dst: BlendFactor::OneMinusSrcAlpha, op: BlendOp::Add },
        alpha: BlendComponent { src: BlendFactor::One, dst: BlendFactor::OneMinusSrcAlpha, op: BlendOp::Add },
    };
    /// Godot's `BLEND_MODE_PREMULTIPLIED_ALPHA`, used by the 2D canvas.
    pub const PREMULTIPLIED_ALPHA: Self = Self {
        color: BlendComponent { src: BlendFactor::One, dst: BlendFactor::OneMinusSrcAlpha, op: BlendOp::Add },
        alpha: BlendComponent { src: BlendFactor::One, dst: BlendFactor::OneMinusSrcAlpha, op: BlendOp::Add },
    };
    /// Godot's `BLEND_MODE_ADD`.
    pub const ADD: Self = Self {
        color: BlendComponent { src: BlendFactor::SrcAlpha, dst: BlendFactor::One, op: BlendOp::Add },
        alpha: BlendComponent { src: BlendFactor::SrcAlpha, dst: BlendFactor::One, op: BlendOp::Add },
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CompareOp {
    Never,
    Less,
    Equal,
    LessEqual,
    Greater,
    NotEqual,
    GreaterEqual,
    Always,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DepthState {
    pub format: TextureFormat,
    pub write: bool,
    pub compare: CompareOp,
}

#[derive(Clone, Debug)]
pub struct ColorTarget {
    pub format: TextureFormat,
    pub blend: Option<Blend>,
}

#[derive(Clone, Debug)]
pub struct RenderPipelineDesc {
    pub shader: ShaderId,
    pub vertex_buffers: Vec<VertexBufferLayout>,
    pub topology: Topology,
    pub cull: Cull,
    pub color_targets: Vec<ColorTarget>,
    pub depth: Option<DepthState>,
    pub samples: u32,
}

/// One binding inside a uniform set.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Uniform {
    pub binding: u32,
    pub resource: UniformResource,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UniformResource {
    UniformBuffer { buffer: BufferId, offset: u64, size: u64 },
    StorageBuffer { buffer: BufferId, offset: u64, size: u64 },
    Texture(TextureId),
    Sampler(SamplerId),
    StorageTexture(TextureId),
}
