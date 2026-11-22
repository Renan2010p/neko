//! # neko-gpu
//!
//! The GPU layer every renderer is written against, after Godot's
//! `RenderingDeviceDriver` (servers/rendering/rendering_device_driver.h).
//! Godot implements that interface once per API (Vulkan, D3D12, Metal) and
//! builds its 2D and 3D renderers on top; Neko does the same with
//! [`GpuBackend`], implemented by `neko-wgpu`, the software renderer, and by
//! whatever a port needs (GNM/AGC on PlayStation, NVN on Switch, ...).
//!
//! The contract is deliberately smaller than Godot's:
//!
//! - Resources are plain ids ([`BufferId`], [`TextureId`], ...) created and
//!   freed through the backend.
//! - Work is recorded as data into a [`CommandList`] and handed over with
//!   [`GpuBackend::submit`]. Backends translate it in one pass, which keeps
//!   ports short: there is no command-buffer object model to mirror.
//! - Barriers, memory types and queue families are the backend's job, the way
//!   wgpu and Godot's `RenderingDevice` (not its driver) handle them.
//! - Shaders arrive precompiled for the backend's format ([`ShaderFormat`]),
//!   like Godot's shader containers baked at export time; WGSL source is the
//!   portable fallback for backends that compile at runtime.

use neko_platform::Surface;

mod command;
mod types;
pub use command::*;
pub use types::*;

macro_rules! ids {
    ($($(#[$m:meta])* $name:ident),* $(,)?) => {$(
        $(#[$m])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(pub u64);
    )*};
}

ids! {
    /// A GPU buffer (vertices, indices, uniforms, storage).
    BufferId,
    /// A texture or render target.
    TextureId,
    /// Sampling state.
    SamplerId,
    /// A compiled shader program (all of its stages).
    ShaderId,
    /// A render or compute pipeline.
    PipelineId,
    /// Resources bound together at one set index (Godot's uniform set).
    UniformSetId,
    /// A presentable surface's swap chain.
    SwapChainId,
}

#[derive(Debug, thiserror::Error)]
pub enum GpuError {
    #[error("no suitable GPU: {0}")]
    NoDevice(String),
    #[error("cannot present to this surface: {0}")]
    Surface(String),
    #[error("out of GPU memory")]
    OutOfMemory,
    #[error("the GPU device was lost")]
    DeviceLost,
    #[error("shader has no code for this backend (needs one of {0:?})")]
    NoShaderCode(Vec<ShaderFormat>),
    #[error("invalid {0}: {1}")]
    Invalid(&'static str, String),
    #[error("not supported by this backend: {0}")]
    Unsupported(&'static str),
}

pub type Result<T> = std::result::Result<T, GpuError>;

/// What a swap chain gave us for this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Acquired {
    /// Draw into this texture, then [`GpuBackend::present`].
    Frame(TextureId),
    /// Nothing to draw into right now (window minimized, app in background).
    Skip,
}

/// A GPU backend. One instance owns one device.
///
/// Call order per frame: [`acquire`](Self::acquire) each swap chain, record a
/// [`CommandList`] that renders into the acquired textures,
/// [`submit`](Self::submit) it, then [`present`](Self::present).
pub trait GpuBackend {
    /// Adapter, API and limits.
    fn info(&self) -> &GpuInfo;

    /// Shader formats this backend consumes, in order of preference.
    fn shader_formats(&self) -> &[ShaderFormat];

    // ---- presentation (Godot: swap_chain_*)

    /// Create a swap chain for a window or platform display.
    fn create_swap_chain(&mut self, surface: Surface, desc: &SwapChainDesc) -> Result<SwapChainId>;
    /// The window was resized (size in physical pixels).
    fn resize_swap_chain(&mut self, swap_chain: SwapChainId, width: u32, height: u32) -> Result<()>;
    /// Format of the textures the swap chain hands out.
    fn swap_chain_format(&self, swap_chain: SwapChainId) -> TextureFormat;
    fn acquire(&mut self, swap_chain: SwapChainId) -> Result<Acquired>;
    fn present(&mut self, swap_chain: SwapChainId) -> Result<()>;
    fn free_swap_chain(&mut self, swap_chain: SwapChainId);

    // ---- resources

    fn create_buffer(&mut self, desc: &BufferDesc) -> Result<BufferId>;
    /// Copy bytes into a buffer; visible to every later submit.
    fn write_buffer(&mut self, buffer: BufferId, offset: u64, data: &[u8]) -> Result<()>;
    fn free_buffer(&mut self, buffer: BufferId);

    fn create_texture(&mut self, desc: &TextureDesc) -> Result<TextureId>;
    /// Upload pixels (tightly packed rows) into a region of mip level `mip`.
    fn write_texture(&mut self, texture: TextureId, region: TextureRegion, mip: u32, data: &[u8]) -> Result<()>;
    /// Read a texture back (screenshots, tests). Blocks until the GPU is done.
    fn read_texture(&mut self, texture: TextureId, region: TextureRegion) -> Result<Vec<u8>>;
    fn free_texture(&mut self, texture: TextureId);

    fn create_sampler(&mut self, desc: &SamplerDesc) -> Result<SamplerId>;
    fn free_sampler(&mut self, sampler: SamplerId);

    /// Compile or load a shader. Fails with [`GpuError::NoShaderCode`] when
    /// `desc` has no code in any of [`shader_formats`](Self::shader_formats).
    fn create_shader(&mut self, desc: &ShaderDesc<'_>) -> Result<ShaderId>;
    fn free_shader(&mut self, shader: ShaderId);

    fn create_render_pipeline(&mut self, desc: &RenderPipelineDesc) -> Result<PipelineId>;
    fn create_compute_pipeline(&mut self, shader: ShaderId) -> Result<PipelineId>;
    fn free_pipeline(&mut self, pipeline: PipelineId);

    /// Bind resources to set `set` of `shader`'s layout.
    fn create_uniform_set(&mut self, shader: ShaderId, set: u32, uniforms: &[Uniform]) -> Result<UniformSetId>;
    fn free_uniform_set(&mut self, set: UniformSetId);

    // ---- work

    /// Execute recorded commands, in order.
    fn submit(&mut self, commands: &CommandList) -> Result<()>;
    /// Block until all submitted work is finished.
    fn wait_idle(&mut self) -> Result<()>;
}
