#!/usr/bin/env python3
"""fid_sheet.py NAME FRAMES [y0 y1]: contact sheet of out/fid/NAME diff images (oracle | engine | diff) for rel FRAMES, cropped to rows y0..y1."""
import sys, os
from PIL import Image
n = sys.argv[1]; fr = [int(x) for x in sys.argv[2].split(',')]
y0, y1 = (int(sys.argv[3]), int(sys.argv[4])) if len(sys.argv) > 4 else (0, 224)
d = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'out', 'fid', n, 'diff')
ims = [Image.open(os.path.join(d, 'r%05d.png' % f)).crop((0, y0, 516, y1)) for f in fr]
cols = 1 if y1 - y0 > 120 else 2
rows = (len(ims) + cols - 1) // cols
W = Image.new('RGB', (516 * cols, (y1 - y0) * rows), (60, 0, 60))
for i, im in enumerate(ims): W.paste(im, ((i % cols) * 516, (i // cols) * (y1 - y0)))
out = os.path.join(d, '..', 'sheet.png'); W.save(out); print(out)
