"""Contact sheet of PNG frames: modules_sheet.py <dir> <out.png> [cols] [glob] [scale]"""
import sys, glob, os
from PIL import Image, ImageDraw
d, out = sys.argv[1], sys.argv[2]
cols = int(sys.argv[3]) if len(sys.argv) > 3 else 6
pat = sys.argv[4] if len(sys.argv) > 4 else 'f*.png'
scale = float(sys.argv[5]) if len(sys.argv) > 5 else 0.5
fs = sorted(glob.glob(os.path.join(d, pat)))
ims = [Image.open(f).convert('RGB') for f in fs]
w = int(max(i.width for i in ims) * scale); h = int(max(i.height for i in ims) * scale)
rows = (len(ims) + cols - 1) // cols
S = Image.new('RGB', (cols * w, rows * (h + 10)), (40, 0, 40))
dr = ImageDraw.Draw(S)
for k, (f, im) in enumerate(zip(fs, ims)):
    x, y = (k % cols) * w, (k // cols) * (h + 10)
    S.paste(im.resize((int(im.width * scale), int(im.height * scale))), (x, y + 10))
    dr.text((x + 2, y), os.path.basename(f), fill=(255, 255, 0))
S.save(out)
