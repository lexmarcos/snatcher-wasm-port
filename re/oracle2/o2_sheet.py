#!/usr/bin/env python3
"""Contact sheet: o2_sheet.py out.png dirA dirB name1 name2 ...  (row 1 = dirA/name.png, row 2 = dirB/name.png; scaled to 160 px wide)"""
import sys, os
from PIL import Image, ImageDraw
out, da, db, *names = sys.argv[1:]
W, H = 256, 224
sheet = Image.new('RGB', (W * len(names), H * 2 + 14), (40, 40, 40))
d = ImageDraw.Draw(sheet)
for i, n in enumerate(names):
    for r, dd in enumerate((da, db)):
        p = os.path.join(dd, n + '.png')
        if os.path.exists(p):
            im = Image.open(p).convert('RGB')
            sheet.paste(im.crop((0, 0, min(im.width, W), min(im.height, H))), (i * W, r * H + 14 * (r)))
    d.text((i * W + 2, H), n, fill=(255, 255, 0))
sheet.save(out)
