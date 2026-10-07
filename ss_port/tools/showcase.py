#!/usr/bin/env python3
"""Render the 1080p / 60 FPS showcase video (dist/showcase.mp4).

Each segment is a frame dump of the release binary (`--hd --dump-frames DIR
--from F --at T`: one simulation frame per video frame) of a menu screen, an
oracle trace replay or a scripted run; it is encoded with its caption and a
short fade, the segments are concatenated, and the Bali theme
(`site/bundles/bali/audio-basic/theme.ogg`) is laid under it. The frames are
deleted as soon as their segment is encoded.

    cargo build --release
    python3 tools/showcase.py [--out dist/showcase.mp4] [--keep]
"""
import argparse
import os
import shutil
import subprocess
import sys
import tempfile

from PIL import Image, ImageDraw, ImageFont

PORT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ROOT = os.path.dirname(PORT)
TRACES = os.path.join(ROOT, "oracle/traces")
THEME = os.path.join(ROOT, "oracle/site/bundles/bali/audio-basic/theme.ogg")
FONT = os.path.join(PORT, "assets/fonts/lilita-one.ttf")


def binary():
    out = subprocess.run(["cargo", "metadata", "--format-version", "1", "--no-deps"], cwd=PORT, capture_output=True, text=True, check=True)
    import json

    return os.path.join(json.loads(out.stdout)["target_directory"], "release", "ss_port")


def replay(name):
    return ["--replay", os.path.join(TRACES, name, "trace.jsonl")]


# (caption, extra args, first frame, last frame)
SEGMENTS = [
    ("Subway Surfers: a 1:1 Rust / Bevy port", ["--menu", "title"], 0, 180),
    ("Me panel: live 3D characters", ["--menu", "me:jake"], 0, 90),
    ("Tricky", ["--menu", "me:tricky"], 0, 80),
    ("Brody", ["--menu", "me:brody"], 0, 80),
    ("Hoverboards", ["--menu", "me-boards"], 0, 100),
    ("Bali: the Inspector and his dog give chase", replay("seed1-god"), 0, 300),
    ("Dodging trains, rolling under barriers", replay("seed1-god"), 600, 840),
    ("Power Jumper", replay("seed1-god"), 1080, 1200),
    ("Stumble: the pursuers close in", replay("seed1-god"), 1385, 1505),
    ("Super Sneakers", replay("seed1-god-sneakers-jump"), 1140, 1470),
    ("Coin Magnet", replay("seed1-god-magnet"), 1085, 1265),
    ("Jetpack and the sky coin ribbon", ["--god", "--power", "jetpack@400"], 400, 700),
    ("Hoverboard", replay("seed1-god-hoverboard"), 400, 640),
    ("Hoverboard crash shield", replay("seed1-hoverboard-shield"), 160, 280),
    ("Tricky on the run", replay("seed1-god") + ["--character", "tricky"], 600, 840),
    ("Crash... Save me!", [], 160, 360),
    ("New High Score!", [], 595, 790),
    ("Results", ["--press", "Space@800"], 800, 920),
]


def caption(text, path):
    """A 1920 x 1080 transparent overlay: a dark band with the caption."""
    im = Image.new("RGBA", (1920, 1080), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    font = ImageFont.truetype(FONT, 56)
    w = d.textlength(text, font=font)
    x, y = 60, 1080 - 140
    d.rounded_rectangle((x - 30, y - 18, x + w + 30, y + 78), radius=18, fill=(0, 0, 0, 120))
    d.text((x + 3, y + 3), text, font=font, fill=(0, 0, 0, 200))
    d.text((x, y), text, font=font, fill=(255, 255, 255, 255))
    im.save(path)


def run(cmd, **kw):
    r = subprocess.run(cmd, **kw)
    if r.returncode != 0:
        sys.exit(f"failed ({r.returncode}): {' '.join(cmd)}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default=os.path.join(PORT, "dist/showcase.mp4"))
    ap.add_argument("--keep", action="store_true", help="keep the work directory")
    a = ap.parse_args()
    exe = binary()
    work = tempfile.mkdtemp(prefix="ss_showcase_")
    parts = []
    for i, (cap, extra, first, last) in enumerate(SEGMENTS):
        frames = os.path.join(work, f"seg{i:02}")
        print(f"[{i + 1}/{len(SEGMENTS)}] {cap} ({last - first + 1} frames)", flush=True)
        run([exe, "--hd", "--dump-frames", frames, "--from", str(first), "--at", str(last)] + extra, cwd=PORT, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=900)
        n = len([f for f in os.listdir(frames) if f[0].isdigit()])
        dur = n / 60.0
        cap_png = os.path.join(work, f"cap{i:02}.png")
        caption(cap, cap_png)
        seg = os.path.join(work, f"seg{i:02}.mp4")
        fade = f"fade=t=in:st=0:d=0.15,fade=t=out:st={max(dur - 0.15, 0):.3f}:d=0.15"
        run(
            ["ffmpeg", "-v", "error", "-y", "-framerate", "60", "-i", os.path.join(frames, "%05d.png"), "-i", cap_png,
             "-filter_complex", f"[0:v][1:v]overlay=0:0,{fade},format=yuv420p",
             "-c:v", "libx264", "-preset", "slow", "-crf", "18", "-r", "60", seg]
        )
        shutil.rmtree(frames)
        parts.append(seg)
    listing = os.path.join(work, "parts.txt")
    with open(listing, "w") as f:
        f.writelines(f"file '{p}'\n" for p in parts)
    video = os.path.join(work, "video.mp4")
    run(["ffmpeg", "-v", "error", "-y", "-f", "concat", "-safe", "0", "-i", listing, "-c", "copy", video])
    total = float(subprocess.run(["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", video], capture_output=True, text=True).stdout)
    os.makedirs(os.path.dirname(a.out), exist_ok=True)
    run(
        ["ffmpeg", "-v", "error", "-y", "-i", video, "-i", THEME, "-map", "0:v", "-map", "1:a", "-c:v", "copy",
         "-af", f"afade=t=in:st=0:d=0.5,afade=t=out:st={max(total - 2.0, 0):.3f}:d=2", "-c:a", "aac", "-b:a", "192k", "-shortest",
         "-movflags", "+faststart", a.out]
    )
    if not a.keep:
        shutil.rmtree(work)
    print(f"wrote {a.out} ({total:.1f} s)")


if __name__ == "__main__":
    main()
