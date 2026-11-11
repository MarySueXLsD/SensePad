"""Split the DualSense body sprite into tintable layers for SensePad.

Input : assets/gv/fJIyBwn.png  (PS5 skin for GamepadViewer by CrazyGrape)
Output: assets/gv/layer_shell.png, layer_touch.png, layer_core.png, layer_light.png, layer_base.png

Shell/touchpad/core layers are white with the shading stored as brightness, so SensePad can
multiply them by any colour. layer_base keeps everything else (buttons, outlines, sticks wells).
Run with:  python tools/split_gv_skin.py
"""
from collections import deque
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent / "assets" / "gv"
body = Image.open(ROOT / "fJIyBwn.png").convert("RGBA")
W, H = body.size
px = body.load()


def lum(c):
    return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]


def dist(a, b):
    return sum((a[i] - b[i]) ** 2 for i in range(3)) ** 0.5


def flood(seeds, ref, tol):
    mask = bytearray(W * H)
    q = deque()
    for s in seeds:
        q.append(s)
        mask[s[1] * W + s[0]] = 1
    while q:
        x, y = q.popleft()
        for nx, ny in ((x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)):
            if 0 <= nx < W and 0 <= ny < H and not mask[ny * W + nx]:
                c = px[nx, ny]
                if c[3] > 0 and dist(c, ref) < tol:
                    mask[ny * W + nx] = 1
                    q.append((nx, ny))
    return mask


def dilate(mask, steps, claimed):
    m = bytearray(mask)
    for _ in range(steps):
        grow = []
        for y in range(H):
            row = y * W
            for x in range(W):
                i = row + x
                if m[i] or claimed[i] or px[x, y][3] == 0:
                    continue
                if (x > 0 and m[i - 1]) or (x < W - 1 and m[i + 1]) or (y > 0 and m[i - W]) or (y < H - 1 and m[i + W]):
                    grow.append(i)
        for i in grow:
            m[i] = 1
    return m


SHELL = (222, 226, 234)
CORE = (48, 48, 48)
shell = flood([(150, 300), (700, 450), (20, 500)], SHELL, 18)
# The bevel along the inner edge of the grips is a darker shade of the shell.
BEVEL = (165, 175, 195)
bevel_seeds = [(x, y) for (x, y) in ((150, 560), (657, 560), (90, 620), (717, 620)) if dist(px[x, y], BEVEL) < 14]
bevel = flood(bevel_seeds, BEVEL, 14)
shell = bytearray(1 if (shell[i] or bevel[i]) else 0 for i in range(W * H))
touch = flood([(403, 200)], SHELL, 18)
touch = bytearray(0 if shell[i] else touch[i] for i in range(W * H))
core = flood([(403, 420), (380, 322)], CORE, 14)
light = bytearray(W * H)
for y in range(H):
    for x in range(W):
        r, g, b, a = px[x, y]
        if a > 0 and b > r + 38 and b > 80:
            light[y * W + x] = 1

claimed = bytearray(W * H)
layers = {}
for name, mask, ref in (("light", light, None), ("shell", shell, SHELL), ("touch", touch, SHELL), ("core", core, CORE)):
    grown = dilate(mask, 2, claimed) if ref is not None else mask
    img = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    ip = img.load()
    count = 0
    for y in range(H):
        for x in range(W):
            i = y * W + x
            if not grown[i] or claimed[i]:
                continue
            c = px[x, y]
            if ref is None:
                v = min(255, int(c[2] / 217 * 255))
            else:
                v = min(255, int(lum(c) / lum(ref) * 255))
            ip[x, y] = (v, v, v, c[3])
            claimed[i] = 1
            count += 1
    layers[name] = img
    print(f"{name:6s} {count:7d} px")

base = body.copy()
bp = base.load()
for y in range(H):
    for x in range(W):
        if claimed[y * W + x]:
            bp[x, y] = (0, 0, 0, 0)
base.save(ROOT / "layer_base.png", optimize=True)
for name, img in layers.items():
    img.save(ROOT / f"layer_{name}.png", optimize=True)
print("done")
