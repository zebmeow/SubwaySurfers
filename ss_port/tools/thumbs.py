#!/usr/bin/env python3
"""Render the UI thumbnails the Me panel needs and the build has no image for:
the 16 board thumbs (the original renders them in 3D, `fy`).

Input: the JSON meshes written by `cargo run --example thumbs_export -- DIR`.
Output: ss_port/assets/ui/thumbs/board-<id>.png (384x384 RGBA, unlit
textured, orthographic, 2x supersampled).

    python3 tools/thumbs.py DIR
"""
import json, math, os, sys
import numpy as np
from PIL import Image

SIZE, SS = 384, 2
OUT = os.path.join(os.path.dirname(__file__), '..', 'assets', 'ui', 'thumbs')


def rot_y(a):
    c, s = math.cos(a), math.sin(a)
    return np.array([[c, 0, s], [0, 1, 0], [-s, 0, c]])


def rot_x(a):
    c, s = math.cos(a), math.sin(a)
    return np.array([[1, 0, 0], [0, c, -s], [0, s, c]])


def render(parts, tex, rot, fit, anchor_bottom):
    """Rasterize textured triangles (uv v=0 at the image top, as pixi/Bevy)."""
    w = h = SIZE * SS
    img = np.zeros((h, w, 4), np.float32)
    zbuf = np.full((h, w), -np.inf, np.float32)
    textures = {}

    def texels(p):
        # a part's own texture, else the model's
        path = p.get('texture')
        if not path:
            return base
        if path not in textures:
            textures[path] = np.asarray(Image.open(path).convert('RGBA'), np.float32) / 255.0
        return textures[path]
    base = np.asarray(tex.convert('RGBA'), np.float32) / 255.0
    allp = np.concatenate([np.asarray(p['positions'], np.float64) @ rot.T for p in parts])
    lo, hi = allp.min(0), allp.max(0)
    span = max(hi[0] - lo[0], hi[1] - lo[1])
    scale = fit * w / span
    cx = (lo[0] + hi[0]) / 2
    cy = (lo[1] + hi[1]) / 2
    for p in parts:
        t = texels(p)
        th, tw = t.shape[:2]
        P = np.asarray(p['positions'], np.float64) @ rot.T
        UV = np.asarray(p['uvs'], np.float64)
        I = np.asarray(p['indices'], np.int64).reshape(-1, 3)
        sx = (P[:, 0] - cx) * scale + w / 2
        if anchor_bottom:
            sy = h - 2 - (P[:, 1] - lo[1]) * scale
        else:
            sy = h / 2 - (P[:, 1] - cy) * scale
        sz = P[:, 2]
        for a, b, c in I:
            xs = np.array([sx[a], sx[b], sx[c]]); ys = np.array([sy[a], sy[b], sy[c]])
            x0, x1 = int(max(0, np.floor(xs.min()))), int(min(w - 1, np.ceil(xs.max())))
            y0, y1 = int(max(0, np.floor(ys.min()))), int(min(h - 1, np.ceil(ys.max())))
            if x1 < x0 or y1 < y0:
                continue
            det = (ys[1] - ys[2]) * (xs[0] - xs[2]) + (xs[2] - xs[1]) * (ys[0] - ys[2])
            if abs(det) < 1e-12:
                continue
            gx, gy = np.meshgrid(np.arange(x0, x1 + 1) + 0.5, np.arange(y0, y1 + 1) + 0.5)
            l0 = ((ys[1] - ys[2]) * (gx - xs[2]) + (xs[2] - xs[1]) * (gy - ys[2])) / det
            l1 = ((ys[2] - ys[0]) * (gx - xs[2]) + (xs[0] - xs[2]) * (gy - ys[2])) / det
            l2 = 1 - l0 - l1
            m = (l0 >= 0) & (l1 >= 0) & (l2 >= 0)
            if not m.any():
                continue
            z = l0 * sz[a] + l1 * sz[b] + l2 * sz[c]
            zs = zbuf[y0:y1 + 1, x0:x1 + 1]
            m &= z > zs
            if not m.any():
                continue
            u = l0 * UV[a, 0] + l1 * UV[b, 0] + l2 * UV[c, 0]
            v = l0 * UV[a, 1] + l1 * UV[b, 1] + l2 * UV[c, 1]
            tx = np.clip((u % 1.0) * (tw - 1), 0, tw - 1).astype(int)
            ty = np.clip((v % 1.0) * (th - 1), 0, th - 1).astype(int)
            col = t[ty, tx]
            region = img[y0:y1 + 1, x0:x1 + 1]
            region[m] = np.concatenate([col[m][:, :3], np.ones((m.sum(), 1), np.float32)], 1)
            zs[m] = z[m]
    out = Image.fromarray((img * 255).clip(0, 255).astype(np.uint8), 'RGBA')
    return out.resize((SIZE, SIZE), Image.LANCZOS)


def board_rot(parts):
    P = np.concatenate([np.asarray(p['positions'], np.float64) for p in parts])
    ext = P.max(0) - P.min(0)
    order = np.argsort(ext)  # thin, middle, long
    basis = np.zeros((3, 3))
    basis[1, order[2]] = 1   # long axis -> up
    basis[0, order[1]] = 1   # width -> x
    basis[2, order[0]] = 1   # thickness -> depth
    return rot_y(0.35) @ rot_x(-0.25) @ basis


def main():
    src = sys.argv[1]
    os.makedirs(OUT, exist_ok=True)
    for f in sorted(os.listdir(src)):
        if not f.endswith('.json'):
            continue
        d = json.load(open(os.path.join(src, f)))
        tex = Image.open(d['texture'])
        name = f[:-5]
        img = render(d['parts'], tex, board_rot(d['parts']), 0.9, False)
        img.save(os.path.join(OUT, f'{name}.png'))
        print('rendered', name)


if __name__ == '__main__':
    main()
