"""Ferrodock final icon: a steel hex nut holding a container, on a rust tile.

usage: icon_final.py OUT.png [full|small]
'small' is the simplified drawing used for 16-32 px.
"""
import math, sys
from PIL import Image, ImageDraw

out = sys.argv[1]
small = len(sys.argv) > 2 and sys.argv[2] == "small"
S, SS = 1024, 2
W = S * SS

RUST_TOP = (0xF0, 0x6E, 0x40)
RUST_BOTTOM = (0xB0, 0x36, 0x20)
STEEL = (0x17, 0x19, 0x1F)
STEEL_FACE = (0x34, 0x3A, 0x46)
CREAM = (0xF7, 0xF0, 0xE8)
CREAM_RAIL = (0xFF, 0xFB, 0xF6)
CREAM_RIB = (0xCF, 0xBF, 0xAE)

def lerp(a, b, t):
    return tuple(int(x + (y - x) * t) for x, y in zip(a, b))

def rounded_poly(corners, radius, steps=12):
    pts, n = [], len(corners)
    for i in range(n):
        p0, p1, p2 = corners[i - 1], corners[i], corners[(i + 1) % n]
        v1 = (p0[0] - p1[0], p0[1] - p1[1]); l1 = math.hypot(*v1)
        v2 = (p2[0] - p1[0], p2[1] - p1[1]); l2 = math.hypot(*v2)
        u1, u2 = (v1[0] / l1, v1[1] / l1), (v2[0] / l2, v2[1] / l2)
        ang = math.acos(max(-1, min(1, u1[0] * u2[0] + u1[1] * u2[1])))
        dist = radius / math.tan(ang / 2)
        a = (p1[0] + u1[0] * dist, p1[1] + u1[1] * dist)
        b = (p1[0] + u2[0] * dist, p1[1] + u2[1] * dist)
        for s in range(steps + 1):
            t = s / steps
            pts.append(((1 - t) ** 2 * a[0] + 2 * (1 - t) * t * p1[0] + t ** 2 * b[0],
                        (1 - t) ** 2 * a[1] + 2 * (1 - t) * t * p1[1] + t ** 2 * b[1]))
    return pts

def hexagon(d, cx, cy, r, fill, round_frac=0.13):
    corners = [(cx + r * math.cos(math.radians(60 * i - 90)),
                cy + r * math.sin(math.radians(60 * i - 90))) for i in range(6)]
    d.polygon(rounded_poly(corners, r * round_frac), fill=fill)

# Rust tile with a soft vertical gradient.
img = Image.new("RGBA", (W, W), (0, 0, 0, 0))
grad = Image.new("RGBA", (W, W))
gd = ImageDraw.Draw(grad)
for y in range(W):
    gd.line([(0, y), (W, y)], fill=lerp(RUST_TOP, RUST_BOTTOM, y / (W - 1)) + (255,))
mask = Image.new("L", (W, W), 0)
m = int(W * (0.06 if small else 0.09))
ImageDraw.Draw(mask).rounded_rectangle([m, m, W - m, W - m], radius=int(W * 0.21), fill=255)
img.paste(grad, (0, 0), mask)
d = ImageDraw.Draw(img)

cx, cy = W / 2, W / 2
R = W * (0.40 if small else 0.355)
hexagon(d, cx, cy + W * 0.012, R, (0x7E, 0x24, 0x14))   # soft shadow under the nut
hexagon(d, cx, cy, R, STEEL)
if not small:
    hexagon(d, cx, cy, R * 0.8, STEEL_FACE, round_frac=0.11)

# The container.
cw, ch = (R * 1.14, R * 0.66) if small else (R * 1.0, R * 0.56)
x0, y0, x1, y1 = cx - cw / 2, cy - ch / 2, cx + cw / 2, cy + ch / 2
d.rounded_rectangle([x0, y0, x1, y1], radius=int(ch * 0.12), fill=CREAM)
ribs = 3 if small else 4
lw = int(cw * (0.07 if small else 0.045))
pad = ch * 0.2
for i in range(1, ribs + 1):
    x = x0 + i * cw / (ribs + 1)
    d.rounded_rectangle([x - lw / 2, y0 + pad, x + lw / 2, y1 - pad], radius=lw // 2, fill=CREAM_RIB)
if not small:
    d.rounded_rectangle([x0, y0, x1, y0 + ch * 0.13], radius=int(ch * 0.065), fill=CREAM_RAIL)

img.resize((S, S), Image.LANCZOS).save(out)
