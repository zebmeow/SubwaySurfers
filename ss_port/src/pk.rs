//! `.pk` asset packages.
//!
//! Port of `unpackPKObject` (decompiled/js_src/scripts/pk-Bxs1M8oX.js:3131-3208)
//! and the geometry part of `Z.convert` / `convertPrimitive`
//! (ResourceBridge-D6JPsONt.js:2395-2490). Vertex data is used as stored: the
//! JS applies no axis conversion.

use serde_json::Value;

const UINT16: u32 = 0;
const FLOAT32: u32 = 1;
const FLOAT32_COMPRESSED: u32 = 3;
const UINT16_COMPRESSED: u32 = 4;
const UINT32: u32 = 5;
const UINT32_COMPRESSED: u32 = 6;

#[derive(Debug, Clone)]
pub enum Buffer {
    F32(Vec<f32>),
    U16(Vec<u16>),
    U32(Vec<u32>),
}

impl Buffer {
    pub fn f32(&self) -> Option<&[f32]> {
        match self {
            Buffer::F32(v) => Some(v),
            _ => None,
        }
    }
    /// Index buffers: three uses Uint32Array as-is, anything else via Uint16Array.
    pub fn indices(&self) -> Vec<u32> {
        match self {
            Buffer::U32(v) => v.clone(),
            Buffer::U16(v) => v.iter().map(|&x| x as u32).collect(),
            // `new Uint16Array(float32Array)` truncates each value to u16.
            Buffer::F32(v) => v.iter().map(|&x| (x as i64 as u32) & 0xffff).collect(),
        }
    }
}

/// Unpacked package: metadata JSON + buffers (indices in the JSON refer to these).
#[derive(Debug, Clone)]
pub struct PkFile {
    pub meta: Value,
    pub buffers: Vec<Buffer>,
}

fn f32_at(b: &[u8], word: usize) -> f32 {
    let o = word * 4;
    f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// Dequantizer `i()` (pk-Bxs1M8oX.js:14-30). The JS computes in f64 and stores
/// into a Float32Array, so round once at the end.
fn dequantize(u16s: &[u16], raw: &[u8]) -> Vec<f32> {
    let count = f32_at(raw, 0) as usize;
    let table_len = f32_at(raw, 1) as usize;
    let table: Vec<(f64, f64)> = (0..table_len / 2)
        .map(|k| (f32_at(raw, 2 + 2 * k) as f64, f32_at(raw, 3 + 2 * k) as f64))
        .collect();
    let channels = table.len();
    let mut n = (2 + table_len) * 2; // float index -> u16 index
    let mut out = vec![0f32; count];
    let mut s = 0;
    let mut t = 0;
    while t < count {
        for (min, size) in &table {
            let sample = *u16s.get(n).unwrap_or(&0) as f64;
            n += 1;
            if s < count {
                out[s] = ((sample / 65535.0) * size + min) as f32;
            }
            s += 1;
        }
        t += channels.max(1);
    }
    out
}

pub fn parse(bytes: &[u8]) -> Result<PkFile, String> {
    let words = bytes.len() / 4;
    if words < 2 {
        return Err("file too small".into());
    }
    // `a()`: metadata length (UTF-16 units) as float32, then version.
    let meta_units = f32_at(bytes, 0) as usize;
    let meta_bytes = bytes.get(8..8 + meta_units * 2).ok_or("metadata overruns file")?;
    let utf16: Vec<u16> = meta_bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    let text = String::from_utf16(&utf16).map_err(|e| e.to_string())?;
    let meta: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;

    // `T()`: t is a float word index.
    let mut t: f64 = 2.0 + meta_units as f64 / 2.0;
    if t % 1.0 != 0.0 {
        t += 0.5;
    }
    let mut buffers = Vec::new();
    while t < (words - 1) as f64 {
        let count = f32_at(bytes, t as usize);
        let ty = f32_at(bytes, t as usize + 1);
        t += 2.0;
        let data = t as usize * 4;
        let count_n = count as usize;
        match ty as u32 {
            FLOAT32 => {
                buffers.push(Buffer::F32((0..count_n).map(|k| f32_at(bytes, t as usize + k)).collect()));
                t += count as f64;
            }
            UINT16 => {
                let v = (0..count_n)
                    .map(|k| u16::from_le_bytes([bytes[data + 2 * k], bytes[data + 2 * k + 1]]))
                    .collect();
                buffers.push(Buffer::U16(v));
                t += count as f64 / 2.0;
                if t % 1.0 != 0.0 {
                    t += 0.5;
                }
            }
            UINT32 => {
                buffers.push(Buffer::U32(
                    (0..count_n).map(|k| f32_at(bytes, t as usize + k).to_bits()).collect(),
                ));
                t += count as f64;
            }
            FLOAT32_COMPRESSED | UINT16_COMPRESSED | UINT32_COMPRESSED => {
                let raw = miniz_oxide::inflate::decompress_to_vec_zlib(&bytes[data..data + count_n])
                    .map_err(|e| format!("inflate: {e:?}"))?;
                let u16s: Vec<u16> = raw.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
                buffers.push(match ty as u32 {
                    FLOAT32_COMPRESSED => Buffer::F32(dequantize(&u16s, &raw)),
                    UINT16_COMPRESSED => Buffer::U16(u16s),
                    _ => Buffer::U32(raw.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()),
                });
                // `t += a / 4; t += 1 - (t % 1)` (adds a whole word when aligned)
                t += count as f64 / 4.0;
                t += 1.0 - (t % 1.0);
            }
            other => return Err(format!("unsupported buffer type {other}")),
        }
    }
    Ok(PkFile { meta, buffers })
}

/// One drawable primitive: the unit three.js turns into a BufferGeometry.
#[derive(Debug, Clone, Default)]
pub struct Primitive {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    /// Missing uvs become zeros, as in convertPrimitive.
    pub uvs: Vec<[f32; 2]>,
    pub colors: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
    pub skinned: bool,
}

fn chunks<const N: usize>(b: Option<&Buffer>) -> Vec<[f32; N]> {
    b.and_then(Buffer::f32)
        .map(|v| v.chunks_exact(N).map(|c| std::array::from_fn(|i| c[i])).collect())
        .unwrap_or_default()
}

impl PkFile {
    fn buffer(&self, v: &Value) -> Option<&Buffer> {
        v.as_u64().and_then(|i| self.buffers.get(i as usize))
    }

    /// Geometry hash in `Z.convert` order: `"<geometry name> + <prim index or ''>"`.
    pub fn geometry_keys(&self) -> Vec<String> {
        let mut out = Vec::new();
        for g in self.meta["geometry"].as_array().into_iter().flatten() {
            let name = g["name"].as_str().unwrap_or("");
            for (i, _) in g["primitives"].as_array().into_iter().flatten().enumerate() {
                out.push(geometry_key(name, i));
            }
        }
        out
    }

    pub fn primitive(&self, key: &str) -> Option<Primitive> {
        for g in self.meta["geometry"].as_array()? {
            let name = g["name"].as_str().unwrap_or("");
            for (i, p) in g["primitives"].as_array()?.iter().enumerate() {
                if geometry_key(name, i) != key {
                    continue;
                }
                let a = &p["attributes"];
                let positions = chunks::<3>(self.buffer(&a["positions"]));
                let mut uvs = chunks::<2>(self.buffer(&a["uvs"]));
                if uvs.is_empty() {
                    uvs = vec![[0.0; 2]; positions.len()];
                }
                return Some(Primitive {
                    normals: chunks::<3>(self.buffer(&a["normals"])),
                    colors: chunks::<4>(self.buffer(&a["colors"])),
                    indices: self.buffer(&p["indices"]).map(Buffer::indices).unwrap_or_default(),
                    skinned: self.buffer(&a["weights"]).and_then(Buffer::f32).is_some_and(|w| !w.is_empty()),
                    positions,
                    uvs,
                });
            }
        }
        None
    }

    /// `materials[].name` entries (used for `hash___name` material lookup).
    pub fn material_names(&self) -> Vec<String> {
        self.meta["materials"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|m| m["name"].as_str().map(str::to_string))
            .collect()
    }
}

/// `${name} + ${a || ""}` (ResourceBridge-D6JPsONt.js:2405).
pub fn geometry_key(name: &str, prim: usize) -> String {
    if prim == 0 { format!("{name} + ") } else { format!("{name} + {prim}") }
}

/// Animation channel of a `.pk` track (`Z.convertAnimations`, RB:2535).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    /// `t`: position, 3 floats per key.
    Position,
    /// `r`: quaternion xyzw, 4 floats per key.
    Rotation,
    /// `s`: scale, 3 floats per key.
    Scale,
    /// `w`: morph weight, 1 float per key.
    Morph,
}

impl Channel {
    pub fn size(self) -> usize {
        match self {
            Channel::Position | Channel::Scale => 3,
            Channel::Rotation => 4,
            Channel::Morph => 1,
        }
    }
}

/// One keyframe track `<target>.<channel>`.
#[derive(Debug, Clone)]
pub struct RawTrack {
    pub target: String,
    pub channel: Channel,
    pub times: Vec<f32>,
    pub values: Vec<f32>,
}

/// One `animations[]` entry as a three.js master clip.
#[derive(Debug, Clone)]
pub struct RawAnimation {
    pub name: String,
    /// `max(data[].duration)`.
    pub duration: f64,
    pub tracks: Vec<RawTrack>,
}

impl PkFile {
    /// `Z.convertAnimations`: tracks per data entry in t, r, s, w order; the
    /// target is the node's name (`bone_<id>` without one).
    pub fn animations(&self) -> Vec<RawAnimation> {
        let nodes = self.meta["nodes"].as_array();
        let mut out = Vec::new();
        for (ai, a) in self.meta["animations"].as_array().into_iter().flatten().enumerate() {
            let mut tracks = Vec::new();
            let mut duration: f64 = 0.0;
            for d in a["data"].as_array().into_iter().flatten() {
                let id = d["id"].as_u64().unwrap_or(0) as usize;
                let target = nodes.and_then(|n| n.get(id)).and_then(|n| n["name"].as_str()).map(str::to_string).unwrap_or(format!("bone_{id}"));
                duration = duration.max(d["duration"].as_f64().unwrap_or(0.0));
                for (key, channel) in [("t", Channel::Position), ("r", Channel::Rotation), ("s", Channel::Scale), ("w", Channel::Morph)] {
                    let c = &d[key];
                    if c.is_null() {
                        continue;
                    }
                    let times = self.buffer(&c["times"]).and_then(Buffer::f32).map(<[f32]>::to_vec).unwrap_or_default();
                    if times.is_empty() {
                        continue;
                    }
                    let values = self.buffer(&c["values"]).and_then(Buffer::f32).map(<[f32]>::to_vec).unwrap_or_default();
                    tracks.push(RawTrack { target: target.clone(), channel, times, values });
                }
            }
            if !tracks.is_empty() {
                let name = a["name"].as_str().map(str::to_string).unwrap_or(format!("animation_{ai}"));
                out.push(RawAnimation { name, duration, tracks });
            }
        }
        out
    }
}
