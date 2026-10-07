// Final pass: the main target holds sRGB-encoded values (as the WebGL canvas
// did); decode them once so the sRGB swapchain writes them back unchanged.

#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

@group(0) @binding(0) var screen: texture_2d<f32>;
@group(0) @binding(1) var screen_smp: sampler;
struct Settings { unused: f32 }
@group(0) @binding(2) var<uniform> settings: Settings;

fn srgb_eotf(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let c = textureLoad(screen, vec2<i32>(in.position.xy), 0);
    return vec4<f32>(srgb_eotf(clamp(c.rgb, vec3<f32>(0.0), vec3<f32>(1.0))), c.a);
}
