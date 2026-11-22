// Neko's 2D canvas shader, following Godot's canvas renderer
// (servers/rendering/renderer_rd/shaders/canvas.glsl): 2D works in sRGB
// space, so vertex color times texture is written unchanged to a non-sRGB
// target. Colors and textures carry premultiplied alpha.

struct Locals {
    screen_size: vec2<f32>, // in points
    _pad: vec2<f32>,
};

@group(0) @binding(0) var<uniform> locals: Locals;
@group(1) @binding(0) var color_texture: texture_2d<f32>;
@group(1) @binding(1) var texture_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(@location(0) pos: vec2<f32>, @location(1) uv: vec2<f32>, @location(2) color: u32) -> VertexOutput {
    var out: VertexOutput;
    out.uv = uv;
    out.color = vec4<f32>(
        f32(color & 255u),
        f32((color >> 8u) & 255u),
        f32((color >> 16u) & 255u),
        f32((color >> 24u) & 255u),
    ) / 255.0;
    out.position = vec4<f32>(
        2.0 * pos.x / locals.screen_size.x - 1.0,
        1.0 - 2.0 * pos.y / locals.screen_size.y,
        0.0,
        1.0,
    );
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color * textureSample(color_texture, texture_sampler, in.uv);
}
