//! # neko-render
//!
//! Backend-agnostic rendering contract.
//!
//! For now a frame is a clear color plus 2D UI geometry (tessellated
//! [`epaint`] meshes, which the editor UI produces). Scene rendering
//! (2D sprites, 3D meshes) will be added to [`Frame`] as new passes, so
//! every backend keeps a single entry point.

use std::sync::Arc;

pub use epaint;
use epaint::{ClippedPrimitive, textures::TexturesDelta};
use neko_platform::Window;

/// Everything needed to draw one frame.
pub struct Frame<'a> {
    /// RGBA clear color in sRGB space (gamma-encoded, straight alpha), like
    /// every color in Godot's 2D renderer.
    pub clear_color: [f32; 4],
    /// Target size in physical pixels.
    pub size_px: [u32; 2],
    pub pixels_per_point: f32,
    /// Texture uploads/frees that must happen before drawing [`Self::ui`].
    /// The caller clears the delta after [`Renderer::render`] returns.
    pub textures: &'a TexturesDelta,
    /// UI meshes in draw order, positions in points.
    pub ui: &'a [ClippedPrimitive],
}

/// Human-readable info about the active backend (shown in the editor).
#[derive(Clone, Debug, Default)]
pub struct RendererInfo {
    /// Backend crate, e.g. `"wgpu"`, `"soft"`.
    pub backend: String,
    /// Underlying API, e.g. `"Vulkan"`, `"CPU"`.
    pub api: String,
    /// Device / adapter name.
    pub device: String,
}

pub trait Renderer {
    fn info(&self) -> RendererInfo;
    /// The window's drawable size changed (physical pixels).
    fn resize(&mut self, width: u32, height: u32);
    fn render(&mut self, frame: &Frame<'_>) -> Result<(), RenderError>;
    /// Largest texture side the backend accepts.
    fn max_texture_side(&self) -> usize {
        8192
    }

    // ---- extra windows: the same device and textures draw into each.

    /// Start drawing into another window too (a dialog's own OS window).
    fn attach_window(&mut self, _id: u64, _window: Arc<dyn Window>) -> Result<(), RenderError> {
        Err(RenderError::Surface("this renderer draws a single window".into()))
    }
    fn detach_window(&mut self, _id: u64) {}
    /// Draw a frame into an attached window. Render the main window first in
    /// each frame: it uploads the textures every window shares.
    fn render_window(&mut self, _id: u64, _frame: &Frame<'_>) -> Result<(), RenderError> {
        Ok(())
    }
}

/// Creates a [`Renderer`] for a window. Lets the app pick a backend at runtime.
pub type RendererFactory = fn(Arc<dyn Window>) -> Result<Box<dyn Renderer>, RenderError>;

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("no compatible GPU adapter: {0}")]
    NoAdapter(String),
    #[error("failed to create a surface for the window: {0}")]
    Surface(String),
    #[error("device error: {0}")]
    Device(String),
    #[error("{0}")]
    Other(String),
}
