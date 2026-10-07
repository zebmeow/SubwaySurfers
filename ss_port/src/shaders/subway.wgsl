// The original renderer's world materials, ported from the compiled WebGL
// programs of the game (three r17x, see ss_port/docs/rendering.md):
//
// MODE 0  `$n` (deobfuscated.js:4377): MeshBasicMaterial + onBeforeCompile
//         patches. color = map(sRGB-decoded) * diffuse; fog
//         mix(c, fogColor * a, smoothstep(near, far, |viewPos|));
//         output linearToOutputTexel (sRGB OETF), no tone mapping.
// MODE 1  `Ir` displaced water (7528, tube_mid caustics): raw shader, no
//         output encoding; fog factor clamp((clip.z - D) / D) toward
//         pow(fogColor, 2.2).
// MODE 2  `Vr` foam / fountain (7663): raw shader, scrolling uv, alpha kill.
//
// Every mode applies the world bend after projection, in clip space:
//     gl_Position.xy += gl_Position.w^2 * uBend
//
// Like WebGL writing sRGB-encoded values into a non-sRGB canvas, the port
// renders into a float target and blends the *encoded* values; a final
// fullscreen pass (srgb_decode.wgsl) decodes once for the sRGB swapchain.

#import bevy_pbr::{
    mesh_functions,
    mesh_view_bindings::view,
}

struct Params {
    // linear rgb diffuse (mode 0) or raw uColor (modes 1, 2); a = opacity
    color: vec4<f32>,
    // fog color rgb; a = 1 when fog is on
    fog_color: vec4<f32>,
    // near, far, far clip plane (1200), fog distance (modes 1, 2: 410)
    fog: vec4<f32>,
    // bend xy; zw = map uv offset
    bend: vec4<f32>,
    // mode 1: displacement uv offset xy, strength; mode 2: foam color rgb
    extra: vec4<f32>,
    // mode 2: alpha kill; near plane
    extra2: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: Params;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var map_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var map_smp: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var disp_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var disp_smp: sampler;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
}

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) uv2: vec2<f32>,
    // mode 0: |viewPos| (three mvPosition); modes 1, 2: fog factor
    @location(2) fog: f32,
    // planar depth (-viewPos.z) for three's far plane
    @location(3) depth: f32,
}

@vertex
fn vertex(v: Vertex) -> VOut {
    var out: VOut;
    let world_from_local = mesh_functions::get_world_from_local(v.instance_index);
    let world = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(v.position, 1.0));
    let view_pos = view.view_from_world * world;
    var clip = view.clip_from_view * view_pos;
    out.depth = -view_pos.z;
#ifdef MODE_BASIC
    out.uv = v.uv;
    out.fog = length(view_pos.xyz);
#else
    // WebGL clip z of three's PerspectiveCamera(near, far) before the bend
    let n = params.extra2.y;
    let f = params.fog.z;
    let clip_z = -(f + n) / (f - n) * view_pos.z - 2.0 * f * n / (f - n);
    let fog_d = params.fog.w;
    out.fog = clamp((clip_z - fog_d) / fog_d, 0.0, 1.0);
#endif
#ifdef MODE_WATER
    // vUv = uv + uMapFrame.xy; vDisplacementUV = uv + frame.xy / size
    out.uv = v.uv + params.bend.zw;
    out.uv2 = v.uv + params.extra.xy;
#endif
#ifdef MODE_FOAM
    // vUv = uv + uMapFrame.xy / uMapSize + fract(uTime * scroll.xy)
    out.uv = v.uv + params.bend.zw;
#endif
    let z_sqr = clip.w * clip.w;
    clip.x = clip.x + z_sqr * params.bend.x;
    clip.y = clip.y + z_sqr * params.bend.y;
    out.clip = clip;
    return out;
}

// three sRGBTransferOETF
fn srgb_oetf(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = pow(c, vec3<f32>(0.41666)) * 1.055 - vec3<f32>(0.055);
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

@fragment
fn fragment(in: VOut) -> @location(0) vec4<f32> {
    // three PerspectiveCamera far plane (Bevy's projection is infinite)
    if in.depth > params.fog.z {
        discard;
    }
#ifdef MODE_BASIC
    var c = params.color;
#ifdef HAS_MAP
    c = c * textureSample(map_tex, map_smp, in.uv);
#endif
#ifdef FOG
    let k = smoothstep(params.fog.x, params.fog.y, in.fog);
    c = vec4<f32>(mix(c.rgb, params.fog_color.rgb * c.a, k), c.a);
#endif
#ifndef BLEND
    c.a = 1.0;
#endif
    return vec4<f32>(srgb_oetf(c.rgb), c.a);
#endif

#ifdef MODE_WATER
    let displacement = textureSample(disp_tex, disp_smp, in.uv2).r;
    let uv = in.uv + displacement * params.extra.z;
    var c = textureSample(map_tex, map_smp, uv) * params.color.a;
    c = c * vec4<f32>(params.color.rgb, 1.0);
    let fog_c = pow(params.fog_color.rgb, vec3<f32>(2.2));
    c = vec4<f32>(mix(c.rgb, fog_c, in.fog), c.a);
    return c;
#endif

#ifdef MODE_FOAM
    let tex = textureSample(map_tex, map_smp, in.uv);
    let main = tex.g - tex.r;
    if tex.r + tex.g < params.extra2.x {
        discard;
    }
    var rgb = mix(params.color.rgb, params.extra.rgb, main);
    let fog_c = pow(params.fog_color.rgb, vec3<f32>(2.2));
    rgb = mix(rgb, fog_c, in.fog);
    return vec4<f32>(rgb, params.color.a);
#endif
}
