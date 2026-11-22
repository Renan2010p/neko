//! Recorded GPU work (Godot's draw lists and compute lists, as data).

use crate::{BufferId, PipelineId, TextureId, TextureRegion, UniformSetId};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LoadOp {
    /// Keep what the attachment had.
    Load,
    /// Clear to a color (RGBA, linear 0..1) or depth value.
    Clear([f32; 4]),
    /// Contents are undefined; fastest when every pixel gets drawn.
    DontCare,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Attachment {
    pub texture: TextureId,
    pub load: LoadOp,
    /// Keep the result (false for transient depth buffers).
    pub store: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndexFormat {
    U16,
    U32,
}

/// Pixel rectangle (scissor and viewport), origin at the top-left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Start drawing into these attachments (Godot's `draw_list_begin`).
    BeginRenderPass { colors: Vec<Attachment>, depth: Option<Attachment> },
    EndRenderPass,
    SetPipeline(PipelineId),
    SetUniformSet { index: u32, set: UniformSetId },
    SetVertexBuffer { slot: u32, buffer: BufferId, offset: u64 },
    SetIndexBuffer { buffer: BufferId, offset: u64, format: IndexFormat },
    SetViewport(PixelRect),
    /// Restrict drawing to a rectangle (Godot's `draw_list_enable_scissor`).
    SetScissor(PixelRect),
    Draw { vertices: std::ops::Range<u32>, instances: std::ops::Range<u32> },
    DrawIndexed { indices: std::ops::Range<u32>, base_vertex: i32, instances: std::ops::Range<u32> },
    /// Run a compute pipeline (outside render passes).
    Dispatch { pipeline: PipelineId, sets: Vec<(u32, UniformSetId)>, groups: [u32; 3] },
    CopyBuffer { src: BufferId, src_offset: u64, dst: BufferId, dst_offset: u64, size: u64 },
    CopyTexture { src: TextureId, src_region: TextureRegion, dst: TextureId, dst_x: u32, dst_y: u32 },
    CopyBufferToTexture { src: BufferId, offset: u64, dst: TextureId, region: TextureRegion },
}

/// Commands for one submit, in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CommandList {
    pub commands: Vec<Command>,
}

impl CommandList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, c: Command) -> &mut Self {
        self.commands.push(c);
        self
    }

    pub fn clear(&mut self) {
        self.commands.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Check structure the backends rely on: render commands only inside a
    /// pass, passes not nested, and every pass closed.
    pub fn validate(&self) -> Result<(), String> {
        let mut in_pass = false;
        for (i, c) in self.commands.iter().enumerate() {
            match c {
                Command::BeginRenderPass { colors, depth } => {
                    if in_pass {
                        return Err(format!("command {i}: render pass begun inside another"));
                    }
                    if colors.is_empty() && depth.is_none() {
                        return Err(format!("command {i}: render pass without attachments"));
                    }
                    in_pass = true;
                }
                Command::EndRenderPass if !in_pass => return Err(format!("command {i}: no render pass to end")),
                Command::EndRenderPass => in_pass = false,
                Command::SetPipeline(_)
                | Command::SetUniformSet { .. }
                | Command::SetVertexBuffer { .. }
                | Command::SetIndexBuffer { .. }
                | Command::SetViewport(_)
                | Command::SetScissor(_)
                | Command::Draw { .. }
                | Command::DrawIndexed { .. }
                    if !in_pass =>
                {
                    return Err(format!("command {i}: {c:?} outside a render pass"));
                }
                Command::Dispatch { .. } | Command::CopyBuffer { .. } | Command::CopyTexture { .. } | Command::CopyBufferToTexture { .. } if in_pass => {
                    return Err(format!("command {i}: {c:?} inside a render pass"));
                }
                _ => {}
            }
        }
        if in_pass { Err("render pass not ended".into()) } else { Ok(()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ShaderCode, ShaderDesc, ShaderFormat, ShaderStage};

    #[test]
    fn command_list_structure() {
        let target = TextureId(1);
        let mut ok = CommandList::new();
        ok.push(Command::BeginRenderPass { colors: vec![Attachment { texture: target, load: LoadOp::Clear([0.0; 4]), store: true }], depth: None })
            .push(Command::Draw { vertices: 0..3, instances: 0..1 })
            .push(Command::EndRenderPass);
        assert!(ok.validate().is_ok());

        let mut outside = CommandList::new();
        outside.push(Command::Draw { vertices: 0..3, instances: 0..1 });
        assert!(outside.validate().is_err());

        let mut open = CommandList::new();
        open.push(Command::BeginRenderPass { colors: vec![Attachment { texture: target, load: LoadOp::Load, store: true }], depth: None });
        assert!(open.validate().is_err());
    }

    #[test]
    fn shader_picks_the_preferred_format() {
        let stages = [(ShaderStage::Vertex, "vs_main"), (ShaderStage::Fragment, "fs_main")];
        let code = [
            ShaderCode { format: ShaderFormat::Wgsl, entry_points: &stages, bytes: b"// wgsl" },
            ShaderCode { format: ShaderFormat::Native("agc"), entry_points: &stages, bytes: &[1, 2, 3] },
        ];
        let desc = ShaderDesc { name: "canvas", code: &code };
        assert_eq!(desc.pick(&[ShaderFormat::Native("agc"), ShaderFormat::Wgsl]).unwrap().bytes, &[1, 2, 3]);
        assert_eq!(desc.pick(&[ShaderFormat::SpirV, ShaderFormat::Wgsl]).unwrap().format, ShaderFormat::Wgsl);
        assert!(desc.pick(&[ShaderFormat::Dxil]).is_none());
    }
}
