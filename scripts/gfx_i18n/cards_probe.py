import sys, os
sys.path.insert(0, os.path.dirname(__file__))
from lib import *; from data import *
def card_grid(d, scene, anim, frame=0):
    vram, _ = vram_from(d, scene)
    tiles = tiles_from_bytes(vram)
    pcs = d.frame(anim, frame)
    x1 = max(x + 8 * (((s >> 2) & 3) + 1) for y, s, a, x in pcs); y1 = max(y + 8 * ((s & 3) + 1) for y, s, a, x in pcs)
    g = np.zeros((y1, x1), dtype=np.uint8)
    for y, s, a, x in pcs:
        grid, w, h = piece_tiles(s, a)
        for r in range(h):
            for c in range(w):
                tn = grid[r][c]
                g[y + r * 8:y + r * 8 + 8, x + c * 8:x + c * 8 + 8] = tiles[tn]
    return g, pcs
if __name__ == '__main__':
    d = Data('DATA_H15')
    g, pcs = card_grid(d, 1, 0x28e96)
    ch = ' 123456789abcdef'
    for y, row in enumerate(g):
        print('%2d ' % y + ''.join(ch[v] if v else '.' for v in row))
