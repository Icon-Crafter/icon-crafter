#!/usr/bin/env python3
"""Generate Icon Crafter brand assets (app icons + SVG mark).

The mark is an original geometric "crafted sparkle": a four-point concave
diamond with a teal anchor node on a dark, softly rounded app tile.

Run from the repository root:  python tools/gen_icons.py
Everything is produced from the same geometry so the icon set and the in-app
SVG mark stay identical. No third-party assets, no image libraries required.
"""

from __future__ import annotations

import math
import os
import struct
import zlib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ICON_DIR = os.path.join(ROOT, "apps", "desktop", "icons")
BRAND_DIR = os.path.join(ROOT, "brand")

BG_TOP = (23, 26, 33)
BG_BOTTOM = (11, 12, 16)
ACCENT_TOP = (255, 138, 102)
ACCENT_BOTTOM = (240, 74, 60)
ANCHOR = (45, 212, 191)
BORDER = (255, 255, 255, 26)


def lerp(a, b, t):
    return tuple(int(round(x + (y - x) * t)) for x, y in zip(a, b))


def rounded_rect_contains(px, py, size, inset, radius):
    x0, y0 = inset, inset
    x1, y1 = size - inset, size - inset
    r = min(radius, (x1 - x0) / 2, (y1 - y0) / 2)
    if px < x0 or py < y0 or px > x1 or py > y1:
        return False
    cx = min(max(px, x0 + r), x1 - r)
    cy = min(max(py, y0 + r), y1 - r)
    return (px - cx) ** 2 + (py - cy) ** 2 <= r * r


def star_contains(px, py, size):
    """Point-in-polygon for the concave four-point diamond."""
    cx = cy = size / 2
    outer = 0.302 * size
    inner = 0.086 * size
    pts = []
    for k in range(4):
        a = -math.pi / 2 + k * math.pi / 2
        pts.append((cx + outer * math.cos(a), cy + outer * math.sin(a)))
        b = a + math.pi / 4
        pts.append((cx + inner * math.cos(b), cy + inner * math.sin(b)))
    inside = False
    j = len(pts) - 1
    for i in range(len(pts)):
        xi, yi = pts[i]
        xj, yj = pts[j]
        if (yi > py) != (yj > py):
            x_cross = (xj - xi) * (py - yi) / ((yj - yi) or 1e-12) + xi
            if px < x_cross:
                inside = not inside
        j = i
    return inside


def circle_contains(px, py, size):
    r = 0.071 * size
    return (px - size / 2) ** 2 + (py - size / 2) ** 2 <= r * r


def render(size: int, ss: int = 3) -> list[bytearray]:
    rows = []
    tile_radius = 0.223 * size
    border_inset = 0.012 * size
    border_radius = tile_radius - border_inset * 0.5
    inner_inset = border_inset + 0.010 * size
    inv = 1.0 / (ss * ss)
    samples = range(ss)
    for y in range(size):
        row = bytearray(size * 4)
        for x in range(size):
            bg = [0.0, 0.0, 0.0, 0.0]
            star = [0.0, 0.0, 0.0, 0.0]
            border = 0.0
            anchor = 0.0
            for sy in samples:
                py = y + (sy + 0.5) / ss
                for sx in samples:
                    px = x + (sx + 0.5) / ss
                    if rounded_rect_contains(px, py, size, 0.0, tile_radius):
                        t = py / size
                        bg[0] += lerp(BG_TOP, BG_BOTTOM, t)[0]
                        bg[1] += lerp(BG_TOP, BG_BOTTOM, t)[1]
                        bg[2] += lerp(BG_TOP, BG_BOTTOM, t)[2]
                        bg[3] += 255
                    if rounded_rect_contains(px, py, size, border_inset, border_radius) and not rounded_rect_contains(
                        px, py, size, inner_inset, tile_radius - inner_inset * 0.5
                    ):
                        border += 1
                    if star_contains(px, py, size):
                        t = py / size
                        c = lerp(ACCENT_TOP, ACCENT_BOTTOM, t)
                        star[0] += c[0]
                        star[1] += c[1]
                        star[2] += c[2]
                        star[3] += 255
                    if circle_contains(px, py, size):
                        anchor += 1
            bg = [v * inv for v in bg]
            star = [v * inv for v in star]
            border *= inv
            anchor *= inv
            i = x * 4
            # composite: tile -> thin border -> sparkle -> anchor
            a = bg[3]
            r, g, b = (bg[0], bg[1], bg[2]) if a else (0.0, 0.0, 0.0)
            if border > 0:
                br, bgb, bb, ba = (255.0, 255.0, 255.0, border * BORDER[3])
                out_a = ba + a * (1 - ba / 255.0)
                if out_a > 0:
                    r = (br * ba + r * a * (1 - ba / 255.0)) / out_a
                    g = (bgb * ba + g * a * (1 - ba / 255.0)) / out_a
                    b = (bb * ba + b * a * (1 - ba / 255.0)) / out_a
                a = out_a
            sa = star[3]
            if sa > 0:
                out_a = sa + a * (1 - sa / 255.0)
                if out_a > 0:
                    r = (star[0] * sa + r * a * (1 - sa / 255.0)) / out_a
                    g = (star[1] * sa + g * a * (1 - sa / 255.0)) / out_a
                    b = (star[2] * sa + b * a * (1 - sa / 255.0)) / out_a
                a = out_a
            aa = anchor * 255.0
            if aa > 0:
                out_a = aa + a * (1 - aa / 255.0)
                if out_a > 0:
                    r = (ANCHOR[0] * aa + r * a * (1 - aa / 255.0)) / out_a
                    g = (ANCHOR[1] * aa + g * a * (1 - aa / 255.0)) / out_a
                    b = (ANCHOR[2] * aa + b * a * (1 - aa / 255.0)) / out_a
                a = out_a
            row[i] = max(0, min(255, int(round(r))))
            row[i + 1] = max(0, min(255, int(round(g))))
            row[i + 2] = max(0, min(255, int(round(b))))
            row[i + 3] = max(0, min(255, int(round(a))))
        rows.append(row)
    return rows


def png_bytes(size: int, rows: list[bytearray]) -> bytes:
    raw = b"".join(b"\x00" + bytes(r) for r in rows)

    def chunk(tag: bytes, data: bytes) -> bytes:
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    ihdr = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", ihdr)
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


def ico_bytes(images: list[tuple[int, bytes]]) -> bytes:
    count = len(images)
    header = struct.pack("<HHH", 0, 1, count)
    offset = 6 + 16 * count
    entries = b""
    blobs = b""
    for size, data in images:
        w = 0 if size >= 256 else size
        h = 0 if size >= 256 else size
        entries += struct.pack("<BBBBHHII", w, h, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
        blobs += data
    return header + entries + blobs


def write_svg() -> None:
    cx = cy = 512
    outer = 0.302 * 1024
    inner = 0.086 * 1024
    pts = []
    for k in range(4):
        a = -math.pi / 2 + k * math.pi / 2
        pts.append((cx + outer * math.cos(a), cy + outer * math.sin(a)))
        b = a + math.pi / 4
        pts.append((cx + inner * math.cos(b), cy + inner * math.sin(b)))
    star = " ".join(f"{x:.2f},{y:.2f}" for x, y in pts)
    svg = f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" width="1024" height="1024" role="img" aria-label="Icon Crafter">
  <defs>
    <linearGradient id="tile" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#171a21"/>
      <stop offset="1" stop-color="#0b0c10"/>
    </linearGradient>
    <linearGradient id="accent" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#ff8a66"/>
      <stop offset="1" stop-color="#f04a3c"/>
    </linearGradient>
  </defs>
  <rect x="0" y="0" width="1024" height="1024" rx="228" fill="url(#tile)"/>
  <rect x="12" y="12" width="1000" height="1000" rx="222" fill="none" stroke="#ffffff" stroke-opacity="0.10" stroke-width="10"/>
  <polygon points="{star}" fill="url(#accent)"/>
  <circle cx="512" cy="512" r="73" fill="#2dd4bf"/>
</svg>
"""
    os.makedirs(BRAND_DIR, exist_ok=True)
    with open(os.path.join(BRAND_DIR, "brand-mark.svg"), "w", encoding="utf-8") as f:
        f.write(svg)


def main() -> None:
    os.makedirs(ICON_DIR, exist_ok=True)
    sizes = {16: 3, 32: 3, 48: 3, 128: 3, 256: 2, 512: 2, 1024: 1}
    rendered: dict[int, bytes] = {}
    for size, ss in sizes.items():
        data = png_bytes(size, render(size, ss))
        rendered[size] = data
        print(f"rendered {size}x{size}")
    names = {32: "32x32.png", 128: "128x128.png", 256: "128x128@2x.png", 512: "icon.png"}
    for size, name in names.items():
        with open(os.path.join(ICON_DIR, name), "wb") as f:
            f.write(rendered[size])
    with open(os.path.join(ICON_DIR, "icon.ico"), "wb") as f:
        f.write(ico_bytes([(s, rendered[s]) for s in (16, 32, 48, 128, 256)]))
    write_svg()
    print("wrote icon set and brand/brand-mark.svg")


if __name__ == "__main__":
    main()
