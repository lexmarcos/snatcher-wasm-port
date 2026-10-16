"""GAME OVER picture (DATA_Y05 scene 5): metallic italic lettering over the gold noise, redrawn as "FIM DE JOGO".
Letters = a mask (outline included) filled with diagonal colour bands that run across the whole word, a pale highlight on the outer top-left edge
and a dark outline; bands and edges are measured on the original, the new mask comes from a condensed bold italic face."""
import sys, os
sys.path.insert(0, os.path.dirname(__file__))
from planes import *

def letter_mask(im):
    """True inside the lettering incl. its outline: everything the flood fill from the border through noise colours (6, 7, 8) cannot reach"""
    noise = (im == 6) | (im == 7) | (im == 8)
    h, w = im.shape
    reach = np.zeros_like(noise)
    stack = [(y, x) for y in range(h) for x in (0, w - 1)] + [(y, x) for x in range(w) for y in (0, h - 1)]
    while stack:
        y, x = stack.pop()
        if y < 0 or x < 0 or y >= h or x >= w or reach[y, x] or not noise[y, x]: continue
        reach[y, x] = True
        stack += [(y + 1, x), (y - 1, x), (y, x + 1), (y, x - 1)]
    return ~reach

if __name__ == '__main__':
    d = Data('DATA_Y05'); p = Plane(d, 5, plane_base=0xC000); im, pl = p.index_image(28)
    m = letter_mask(im)
    pw = d.palette(0x2f330); pal = pal_rgb(pw[:16]); pal[0] = (0, 0, 0)
    out = np.zeros(im.shape + (3,), dtype=np.uint8)
    for (y, x), v in np.ndenumerate(im): out[y, x] = pal[v] if m[y, x] else (40, 40, 40)
    Image.fromarray(out[30:110, 10:240]).resize((230 * 4, 80 * 4), Image.NEAREST).save('work/i18n/gfx/go_mask2.png')
    ys, xs = np.nonzero(m); print(m.sum(), xs.min(), xs.max(), ys.min(), ys.max())
