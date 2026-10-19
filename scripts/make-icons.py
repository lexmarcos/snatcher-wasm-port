#!/usr/bin/env python3
"""Draw the home-screen icons of the web app (web/icons/icon-180.png, icon-192.png, icon-512.png) from the pixel "S" of the page's favicon.
Our own shape on the design system's two colours (--desk, --ink): no game art. Usage: python3 -I scripts/make-icons.py"""
import os
from PIL import Image

DESK, INK = (0x07, 0x0B, 0x24), (0xE8, 0xEC, 0xF8)
# the favicon's 16x16 grid: rectangles (x, y, w, h) of the S
S = [(3, 3, 10, 2), (3, 5, 2, 2), (3, 7, 10, 2), (11, 9, 2, 2), (3, 11, 10, 2)]
out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "web", "icons")
os.makedirs(out, exist_ok=True)
for size in (180, 192, 512):
    im = Image.new("RGB", (16, 16), DESK)
    for x, y, w, h in S:
        for i in range(x, x + w):
            for j in range(y, y + h):
                im.putpixel((i, j), INK)
    im.resize((size, size), Image.NEAREST).save(os.path.join(out, f"icon-{size}.png"), optimize=True)
print("icons written to", os.path.normpath(out))
