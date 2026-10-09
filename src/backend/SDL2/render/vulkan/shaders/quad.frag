#version 450
// Samples the streaming canvas texture for the Neko Vulkan presenter.
layout(location = 0) in vec2 v_uv;
layout(location = 0) out vec4 o_color;

layout(set = 0, binding = 0) uniform sampler2D u_tex;

layout(push_constant) uniform Push {
    vec4 rect;
    float alpha;
} pc;

void main() {
    vec4 c = texture(u_tex, v_uv);
    o_color = vec4(c.rgb, c.a * pc.alpha);
}
