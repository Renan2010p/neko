#version 450
// Full-screen textured quad for the Neko Vulkan presenter.
// `a_pos` is a unit quad (0..1); `pc.rect` maps it to the logical rect in NDC.
layout(location = 0) in vec2 a_pos;
layout(location = 1) in vec2 a_uv;

layout(location = 0) out vec2 v_uv;

layout(push_constant) uniform Push {
    vec4 rect;   // x0, y_top, x1, y_bottom (NDC, Vulkan has Y down)
    vec4 params; // free parameters for custom shaders
    float alpha;
} pc;

void main() {
    vec2 p = mix(pc.rect.xy, pc.rect.zw, a_pos);
    gl_Position = vec4(p, 0.0, 1.0);
    v_uv = a_uv;
}
