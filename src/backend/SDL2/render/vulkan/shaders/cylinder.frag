#version 450
// Built-in cylindrical panorama shader (the Neko `shader` built-in "cylinder").
// `pc.params` = pan, source_width, screen_w, screen_h.
layout(location = 0) in vec2 v_uv;
layout(location = 0) out vec4 o_color;

layout(set = 0, binding = 0) uniform sampler2D u_tex;

layout(push_constant) uniform Push {
    vec4 rect;
    vec4 params;
    float alpha;
} pc;

const float PI = 3.14159265;

void main() {
    float pan = pc.params.x;
    float srcw = pc.params.y;
    float sw = pc.params.z;
    float sh = pc.params.w;

    float focal = (sw * 0.5) / tan(radians(100.0) * 0.5);
    float ppr = srcw / PI;
    float center = PI * 0.5 + pan * radians(45.0);

    float theta = atan(((v_uv.x - 0.5) * sw) / focal);
    float u = (center + theta) * ppr / srcw;
    float th = sh / cos(theta);
    float y0 = (sh - th) * 0.5;
    float sy = v_uv.y * sh;

    if (sy < y0 || sy > y0 + th) {
        o_color = vec4(0.0);
        return;
    }
    o_color = texture(u_tex, vec2(clamp(u, 0.0, 1.0), 1.0 - (sy - y0) / th));
}
