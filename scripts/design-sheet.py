#!/usr/bin/env python3
"""Contact sheet of the design screenshots: scripts/design-sheet.py [dir]  (default work/shots/design) -> dir/sheet.png
Every png in dir (except sheet*.png) is scaled to fit a cell, in name order, with its file name under it."""
import sys, os, glob
from PIL import Image, ImageDraw, ImageFont
d = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(__file__), "..", "work/shots/design")
files = sorted(f for f in glob.glob(os.path.join(d, "*.png")) if not os.path.basename(f).startswith("sheet"))
if not files: sys.exit("no png in " + d)
cols = int(os.environ.get("COLS", 6)); cw, ch, lab, pad = 320, 300, 18, 8
rows = (len(files) + cols - 1) // cols
sheet = Image.new("RGB", (cols * (cw + pad) + pad, rows * (ch + lab + pad) + pad), (24, 24, 28))
dr = ImageDraw.Draw(sheet)
try: font = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf", 11)
except Exception: font = ImageFont.load_default()
for i, f in enumerate(files):
    im = Image.open(f).convert("RGB"); im.thumbnail((cw, ch), Image.NEAREST if max(im.size) < cw * 2 else Image.LANCZOS)
    x = pad + (i % cols) * (cw + pad); y = pad + (i // cols) * (ch + lab + pad)
    sheet.paste(im, (x, y)); dr.text((x, y + ch + 2), os.path.basename(f)[:-4][:44], fill=(220, 220, 220), font=font)
out = os.path.join(d, "sheet.png"); sheet.save(out); print(out, len(files), "images")
