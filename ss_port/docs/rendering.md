# Rendering parity: world bend, fog, sky, colour pipeline

The values below come from `decompiled/js_src/deobfuscated.js` and were then
**measured in the running original**: the oracle at f1200 (seed 1). The
probe reads three's compiled program uniforms
(`renderer.properties.get(material).uniforms`), the program sources,
`getClearColor` and the WebGL context attributes.

## World bend (`subwayBend`, every material)

`$n` (the library material, 4377) patches three's MeshBasicMaterial through
`onBeforeCompile`, after `#include <project_vertex>`:

```glsl
vec4 subwayBend(vec4 pos) { float zSqr = pos.w * pos.w; pos.x += zSqr * uBend.x; pos.y += zSqr * uBend.y; return pos; }
gl_Position = subwayBend(gl_Position);
```

The bend is applied in **clip space after projection**, not to world positions: x and y move by
`w² · uBend`, so in NDC the offset is `w · uBend`, growing linearly with the
distance ahead. The particle, water, foam and ribbon shaders use the same function.

| uniform | value | where it comes from |
|---|---|---|
| `uBend.x` | **−5e-4** | `config.bendX (−5e-4) × game.aspectRatio` in `updateWorldBend` (50063) |
| `uBend.y` | **−3e-4** | `config.bendY` |

`resize()` (49871) calls `updateWorldBend()` **before** it sets
`aspectRatio = h / w`. The oracle resizes once, so the multiplier is the
initial `aspectRatio = 1` (49904). A browser that resizes again would get
`−5e-4 × 0.5625` instead. `Game.reset()` zeroes `game.bendX/bendY`, which is
what the trace's `bend` field records (`[0, 0]`), but it leaves the shared
uniform `Gn.group.uBend` alone. The measured uniform is
`[-0.0005000000237, -0.0003000000142]`.

There is no "camera distance threshold": `w` is the planar view depth,
which is positive for everything in front of the camera.

## Fog

`$n` with `customFog` (every textured library part except glow/light parts;
colour materials only for shadows):

```glsl
vSubwayFogDepth = length(mvPosition.xyz);                       // view-space distance, before the bend
float f = smoothstep(uSubwayFogNear, uSubwayFogFar, vSubwayFogDepth);
outgoingLight = mix(outgoingLight, uSubwayFogColor * diffuseColor.a, f);
```

| value | measured | source |
|---|---|---|
| fog colour (linear) | (0.454, 0.894, 0.917) | `Wn.uFogColor` (4259); Bali sets no `fogColor` override |
| near / far | 400 / 900 | `Wn.uFogNear/uFogFar`; Bali has no `glslFogNear/Far` |
| shadow parts | fog to **white** | `whiteFog` (getEntityFromGeometry 67025) |
| glow / light parts (additive) | no fog | `customNoFog` |
| epic_start glass | far × 1.6 | `fogMultiplier` |

The raw theme shaders (water `Ir`, foam `Vr`) use the older fog:
`clamp((clip.z − 410) / 410, 0, 1)` toward `pow(uFogColor, 2.2)`.

## Sky / clear colour

`renderer.setClearColor(0x87CEEB)` (49954) sets a flat sky blue. The measured
clear colour is linear (0.2423, 0.6172, 0.8308), which displays as (135, 206, 235),
and the sky pixels of the original's screenshot are exactly that. The theme's
`backgroundUpperGradient/LowerGradient` (#88BAFF / #82FFEE) are assigned to the
config but **nothing draws them**. The scene has no `background`, no
three fog and no lights.

## Lights

None. Every world material is an unlit `MeshBasicMaterial` or a raw shader, so
there is no ambient or directional term to port.

## Colour pipeline / "grading"

| | original | port |
|---|---|---|
| tone mapping | none (`toneMapping 0`) | `Tonemapping::None` |
| output | `linearToOutputTexel` = sRGB OETF in the fragment shader | same, in `subway.wgsl` |
| textures | `colorSpace "srgb"`, decoded by the sampler | sRGB images |
| material colour | `Color.setHex` → linear | `hex → linear` |
| vertex colours | ignored (`vertexColors` never set) | ignored |
| blending | on **encoded** values (8-bit non-sRGB canvas) | float target holding encoded values, decoded once by `SrgbDecode` |
| MSAA | off (`antialias: false`) | `Msaa::Off` |
| dithering | none | `DebandDither::Disabled` |
| mipmaps | `generateMipmaps`, `LinearMipmapLinear` | CPU-built mip chains (linear-space box filter), trilinear sampler |
| saturation | `config.canvasSaturation 1.24` is **not applied** (the canvas has no CSS filter) | none |
| far plane | 1200 (near 3) | discard at planar depth > 1200 |

Blend states, from the bridge's `state → three material` mapping
(ResourceBridge 2026-2040):

| state.blendMode | three | Bevy `BlendState` |
|---|---|---|
| NORMAL + blend | NormalBlending | SrcAlpha / OneMinusSrcAlpha (alpha One / OneMinusSrcAlpha) |
| ADD (1) | AdditiveBlending | SrcAlpha / One |
| MULTIPLY (2) | Custom DstColor × Zero, renderOrder −1 | Dst / Zero, sorted first |
| opaque | no blending, alpha forced to 1, no alpha test | opaque |

## Bali theme materials (handleThemeMaterial 8366)

| part | shader | parameters |
|---|---|---|
| `tube_mid` / `environment_water` | `Ir` displaced water: `texture(caustic, uv + frameOffset + texture(wave_noise, ...).r × 0.3) × 0x313131` | additive, no depth write; offsets scroll at `now_s × (3, 3) / 64` and `now_s × (−3, −4) / 32` (component `Lr`) |
| `tube_end` foam, `epic_start` water | `Vr`: `mix(0xCEFDF6, 0x7CFAE9, tex.g − tex.r)`, discard if `r + g < 0.2` | uv scroll `fract(now_s / 20 × (0, −20))` (`Rr`); three ShaderMaterial defaults (opaque, front side) |
| `Boat1` / `Boat1_reflection` | `$n` | `Wr` bob: `y = ∓2 · sin(t · 4)` |

Raw-shader colours are raw `hex / 255` (`sn`) and their output is not encoded.
`Er` (ocean) throws "not fully ported" in the original and is never created.

## Result

With `cargo run --release -- --replay --screenshot out.png --at F` (960×540,
pixel ratio 1) against `node oracle/screenshot.mjs`, outside the HUDs and
the hero:

| frame | scene | mean diff (max channel) | pixels off by > 32 |
|---|---|---|---|
| 40 | intro (guard + dog) | 0.82 | 0.43 % |
| 300 | tunnel entry (blob shadow) | 0.60 | 0.33 % |
| 640 | ramp (Bali gate sparkles) | 0.41 | 0.29 % |
| 1150 | pogo (paint clouds + trail, coin pops) | 0.54 | 0.37 % |
| 1200 | `compare-1200.png` | 0.40 | 0.27 % |
| 1950 | | 0.47 | 0.29 % |
| 2600 | | 0.84 | 0.53 % |
| 3400 | | 0.69 | 0.65 % |

(Whole frames, HUD included, with the particle effects of `src/fx.rs` and
the hero's shadow / pops of `src/hero_fx.rs`; before them f1150 was 2.04 %.)
(With the characters rendered; before them f40 was 13.7 % and f1150 4.2 %.)
The remaining differences are the HUD, the hero's pose at a few frames, and
single-pixel edges and thin rails at grazing angles (mip filtering).
