"""GAME OVER picture (DATA_Y05 scene 5) -> "FIM DE JOGO".  Big stylised metallic lettering: pixel redraw was tried first (flat diagonal colour bands measured on
the original, letter mask from a TTF face) but the bands, highlight and outline did not reconstruct well, so this item used the codex-image fallback
(gpt-6.1-sol, effort high; reference = the original lettering crop, text FIM DE JOGO, landscape 256:88).  The generated art (work/i18n/gfx/codex/gameover_pt.png,
not committed) is scaled to 256x88, quantised to the picture palette (colour 1..11 of line 1) and written over rows 32..111; the committed patch holds only the
resulting tiles (docs/i18n-graphics.md)."""
import sys, os
sys.path.insert(0, os.path.dirname(__file__))
from planes import *

SRC = os.path.join(os.path.dirname(__file__), '..', '..', 'work', 'i18n', 'gfx', 'codex', 'gameover_pt.png')

def quantize_to(img, pal, first=1, last=11):
    """RGB image array (h,w,3) -> palette indices first..last by the nearest colour (weighted RGB)"""
    P = np.array(pal[first:last + 1], dtype=float)
    out = np.zeros(img.shape[:2], dtype=np.uint8)
    flat = img.reshape(-1, 3).astype(float)
    w = np.array([0.30, 0.59, 0.11])
    d = (((flat[:, None, :] - P[None, :, :]) ** 2) * w).sum(axis=2)
    return (d.argmin(axis=1) + first).reshape(img.shape[:2]).astype(np.uint8)

def apply(P):
    d = Data('DATA_Y05')
    e = PlaneEdit(d, 5, plane_base=0xC000)
    pal = pal_rgb(d.palette(0x2f330)[:16])
    g = Image.open(SRC).convert('RGB').resize((256, 88), Image.BOX)       # rows 30..117 of the picture
    q = quantize_to(np.array(g), pal)
    y0, y1, x0, x1 = 32, 112, 8, 248
    e.im[y0:y1, x0:x1] = q[y0 - 30:y1 - 30, x0:x1]
    return e.commit(P, 'DATA_Y05.BIN', 'GAME OVER -> FIM DE JOGO (codex-image art, quantised) tiles', 'GAME OVER -> FIM DE JOGO nametable')
