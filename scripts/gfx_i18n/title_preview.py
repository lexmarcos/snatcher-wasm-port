import sys, os
sys.path.insert(0, os.path.dirname(__file__))
from title import *
a, b = load_cram('work/i18n/gfx/t0/cram.bin')
pal = pal_slot(a, 1); pal[0] = (0, 60, 0)
def img(g, scale=8):
    im = np.zeros((g.shape[0], g.shape[1], 3), dtype=np.uint8)
    for y in range(g.shape[0]):
        for x in range(g.shape[1]): im[y, x] = pal[g[y, x]]
    return Image.fromarray(im).resize((g.shape[1] * scale, g.shape[0] * scale), Image.NEAREST)
blob = D.blob(BLOB_ADDR); tiles = strips(blob)
t2, out = build(tiles)
rows = [read_strip(tiles, 'new_b'), out['new_b'], read_strip(tiles, 'cont_b'), out['cont_b']]
ims = [img(g) for g in rows]
W = max(i.width for i in ims); H = sum(i.height for i in ims)
sheet_ = Image.new('RGB', (W, H), (0, 60, 0)); y = 0
for i in ims: sheet_.paste(i, (0, y)); y += i.height
sheet_.save(sys.argv[1] if len(sys.argv) > 1 else 'work/i18n/gfx/t0/title_cmp.png')
