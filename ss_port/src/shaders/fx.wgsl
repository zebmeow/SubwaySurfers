// The original's FX materials (see ss_port/src/render_fx.rs):
//
// PARTICLE  `Jn` (deobfuscated.js:4283, vertex `Kn`, fragment `qn`): one
//           quad per particle around its centre (world space here), expanded
//           in view space (billboard; or stretched along the velocity), size
//           `aData.x` x uSizeScale, rotated by aData.w (or the three axes),
//           sprite-sheet tile, the clip-space bend. Fragment:
//           vColor * pow(texel, 1/2.2) * uMultiplier.
// RIBBON    `pf` with RIBBON (36296, `ff` / `df`): trail strips widened
//           toward the camera (side = cross(tangent, cameraPosition - p)),
//           uv scroll / scale, vertex colours.
// MESH      `pf` without RIBBON: a mesh with the model matrix, uv scroll.
//
// Output is written as is (the target holds encoded values, like the
// original canvas).

#import bevy_pbr::{
    mesh_functions,
    mesh_view_bindings::view,
}

struct Params {
    // size scale xy, tiles xy
    size_tiles: vec4<f32>,
    // multiplier, length scale, speed scale, three's far plane
    misc: vec4<f32>,
    // bend xy, uv scroll xy
    bend_scroll: vec4<f32>,
    // uv scale xy
    uv_scale: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: Params;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var map_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var map_smp: sampler;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
#ifdef PARTICLE
    @location(1) corner: vec2<f32>,
    @location(2) data: vec4<f32>,
    @location(3) color: vec4<f32>,
    @location(4) tile: f32,
    @location(5) velocity: vec3<f32>,
#endif
#ifdef RIBBON
    @location(1) uv: vec2<f32>,
    @location(2) tangent: vec3<f32>,
    @location(3) side: f32,
    @location(4) color: vec4<f32>,
#endif
#ifdef MESH
    @location(1) uv: vec2<f32>,
#endif
}

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) depth: f32,
}

fn bend(c: vec4<f32>) -> vec4<f32> {
    let z_sqr = c.w * c.w;
    return vec4<f32>(c.x + z_sqr * params.bend_scroll.x, c.y + z_sqr * params.bend_scroll.y, c.z, c.w);
}

@vertex
fn vertex(v: Vertex) -> VOut {
    var out: VOut;
    var view_pos: vec4<f32>;
#ifdef PARTICLE
    let size = v.data.x;
    var local = vec3<f32>(v.corner * size, 0.0);
    local = vec3<f32>(local.xy * params.size_tiles.xy, local.z);
#ifdef ROTATION_SEPARATE_AXES
    let a = cos(v.data.y);
    let b = sin(v.data.y);
    let c = cos(v.data.z);
    let d = sin(v.data.z);
    let e = cos(v.data.w);
    let f = sin(v.data.w);
    let ce = c * e;
    let cf = c * f;
    let de = d * e;
    let df = d * f;
    let rot = mat3x3<f32>(
        vec3<f32>(ce - df * b, cf + de * b, -a * d),
        vec3<f32>(-a * f, a * e, b),
        vec3<f32>(de + cf * b, df - ce * b, a * c),
    );
    local = rot * local;
#else
    let ca = cos(v.data.w);
    let sa = sin(v.data.w);
    local = vec3<f32>(ca * local.x - sa * local.y, sa * local.x + ca * local.y, local.z);
#endif
    view_pos = view.view_from_world * vec4<f32>(v.position, 1.0);
#ifdef STRETCH
    let vvel = (view.view_from_world * vec4<f32>(v.velocity, 0.0)).xyz;
    var length_axis = vec3<f32>(0.0, 1.0, 0.0);
    if length(vvel) > 0.0001 {
        length_axis = normalize(vvel);
    }
    let to_cam = normalize(-view_pos.xyz);
    var width_axis = cross(length_axis, to_cam);
    if length(width_axis) > 0.0001 {
        width_axis = normalize(width_axis);
    } else {
        width_axis = normalize(cross(length_axis, vec3<f32>(0.0, 0.0, 1.0)));
    }
    let spd = length(v.velocity);
    let length_amt = size * params.misc.y + spd * params.misc.z;
    view_pos = vec4<f32>(view_pos.xyz + (-length_axis * (v.corner.x * length_amt)) + (width_axis * (v.corner.y * size)), 1.0);
#else
    view_pos = vec4<f32>(view_pos.xyz + local, 1.0);
#endif
    // PlaneGeometry uv = corner + 0.5, then vUv.y = 1 - uv.y
    var uv = v.corner + vec2<f32>(0.5, 0.5);
    uv.y = 1.0 - uv.y;
    let tiles = params.size_tiles.zw;
    if tiles.x > 1.0 || tiles.y > 1.0 {
        let tx = v.tile - tiles.x * floor(v.tile / tiles.x);
        let ty = floor(v.tile / tiles.x);
        uv = vec2<f32>(uv.x / tiles.x + tx / tiles.x, uv.y / tiles.y + ty / tiles.y);
    }
    out.uv = uv;
    out.color = v.color;
#endif
#ifdef RIBBON
    let cam = view.world_position;
    var side = cross(v.tangent, cam - v.position);
    let side_len = length(side);
    if side_len > 0.00001 {
        side = side / side_len;
    } else {
        side = vec3<f32>(1.0, 0.0, 0.0);
    }
    view_pos = view.view_from_world * vec4<f32>(v.position + side * v.side, 1.0);
    out.uv = (v.uv + params.bend_scroll.zw) * params.uv_scale.xy;
    out.color = v.color;
#endif
#ifdef MESH
    let world_from_local = mesh_functions::get_world_from_local(v.instance_index);
    let world = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(v.position, 1.0));
    view_pos = view.view_from_world * world;
    out.uv = (v.uv + params.bend_scroll.zw) * params.uv_scale.xy;
    out.color = vec4<f32>(1.0);
#endif
    out.depth = -view_pos.z;
    out.clip = bend(view.clip_from_view * view_pos);
    return out;
}

@fragment
fn fragment(in: VOut) -> @location(0) vec4<f32> {
    // three PerspectiveCamera far plane (Bevy's projection is infinite)
    if in.depth > params.misc.w {
        discard;
    }
    // the texture is sRGB-decoded on sampling (three: colorSpace sRGB);
    // the shaders re-encode it with pow(1/2.2)
    let texel = textureSample(map_tex, map_smp, in.uv);
#ifdef PARTICLE
    let rgb = pow(texel.rgb + vec3<f32>(0.0000001), vec3<f32>(1.0 / 2.2));
#else
    let rgb = pow(texel.rgb, vec3<f32>(1.0 / 2.2));
#endif
    return in.color * vec4<f32>(rgb, texel.a) * params.misc.x;
}
