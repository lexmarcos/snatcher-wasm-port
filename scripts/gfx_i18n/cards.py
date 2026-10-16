"""Act cards: ACT n -> ATO n, CURE -> CURA (DATA_H01 scene 1 = ACT 1 SNATCH, DATA_H15 scene 1 = ACT 2 CURE, DATA_H15 scene 2 = ACT 3 JUNK).
The letters are 2-colour-ramp sprites: fill colour = F(y + 0.1 x) (a symmetric yellow-to-white-to-yellow ramp, measured on the originals), dark edge
pixels (colours 1..6) around them. Glyphs are cut from the originals, re-coloured at their new position; the O is built from the left half of the C and
its 180 degree rotation (an italic O is point-symmetric)."""
import sys, os
sys.path.insert(0, os.path.dirname(__file__))
from lib import *; from data import *
from cards_probe import card_grid

FILL_IDX = range(7, 12)
# fill colour by phase (row inside the line + 0.1 * x), measured on the three cards
def ramp(ph):
    ph = int(round(ph))
    if ph <= 3: return 11
    if ph <= 6: return 10
    if ph <= 9: return 9
    if ph <= 12: return 8
    if ph <= 17: return 7
    if ph <= 20: return 8
    if ph <= 22: return 9
    if ph <= 24: return 10
    return 11
FILL = 255

def glyph(g, x0, x1, y0, y1):
    """cut columns x0..x1 rows y0..y1 of grid g; fill pixels -> FILL, edge pixels keep their colour; returns array (rows, cols)"""
    a = g[y0:y1 + 1, x0:x1 + 1].copy()
    out = np.zeros_like(a)
    for (y, x), v in np.ndenumerate(a):
        if v == 0: continue
        out[y, x] = FILL if 7 <= v <= 11 else v
    return out

def place(canvas, gl, x, y, line_top):
    """draw glyph with its top-left at (x, y) (canvas coordinates); fill re-coloured with the ramp of the line starting at row line_top"""
    h, w = gl.shape
    for (yy, xx), v in np.ndenumerate(gl):
        if v == 0: continue
        cy, cx = y + yy, x + xx
        canvas[cy, cx] = ramp(cy - line_top + 1 + 0.1 * cx) if v == FILL else v

def erase(canvas, x0, x1, y0, y1):
    canvas[y0:y1 + 1, x0:x1 + 1] = 0

def make_O(C):
    """C: glyph of the letter C (rows 1..22 -> array 22 rows); O = left half + 180 degree rotation of the left half"""
    h, w = C.shape
    half = C[:, :7]
    rot = half[::-1, ::-1]
    O = np.zeros((h, 14), dtype=np.uint8)
    O[:, :7] = half
    sub = O[:, 7:14]
    sub[rot > 0] = rot[rot > 0]
    return O

class Cards:
    def __init__(self):
        self.d15 = Data('DATA_H15'); self.d01 = Data('DATA_H01')
        g2, self.pcs2 = card_grid(self.d15, 1, 0x28e96, 0)
        self.g2 = g2
        # glyphs of card 2 (line 1 rows 1..22, line 2 rows 49..70)
        self.A = glyph(g2, 5, 17, 1, 22); self.C = glyph(g2, 20, 33, 1, 22); self.T = glyph(g2, 34, 47, 1, 22)
        self.U = glyph(g2, 16, 30, 49, 70); self.R = glyph(g2, 31, 47, 49, 70)

def build_line1(cards, canvas, x_a=5):
    """ATO in place of ACT (columns 5..47 of line 1, rows 1..22)"""
    erase(canvas, 0, 48, 0, 23)
    place(canvas, cards.A, 5, 1, 1)
    place(canvas, cards.T, 20, 1, 1)
    O = make_O(cards.C)
    place(canvas, O, 34, 1, 1)
    return canvas

CARDS = [  # (file, scene, anim table, blob address, x shift of line 1 relative to card 2, CURE -> CURA)
    ('DATA_H01.BIN', 1, 0x2AD26, 0x2A312, 16, False),   # ACT 1 SNATCH
    ('DATA_H15.BIN', 1, 0x28E96, 0x2873A, 0, True),     # ACT 2 CURE
    ('DATA_H15.BIN', 2, 0x28EC2, 0x28AD6, 0, False),    # ACT 3 JUNK
]

def write_grid(d, scene, anim, g):
    """tile bytes (VRAM image) after drawing grid g back into the tiles of the frame-0 pieces"""
    vram, _ = vram_from(d, scene)
    vram = bytearray(vram)
    for y, s, a, x in d.frame(anim, 0):
        grid, w, h = piece_tiles(s, a)
        for r in range(h):
            for c in range(w):
                tn = grid[r][c]
                vram[tn * 32:tn * 32 + 32] = tiles_to_bytes(g[y + r * 8:y + r * 8 + 8, x + c * 8:x + c * 8 + 8][None])
    return vram

def apply(P):
    cards = Cards()
    for (file, scene, anim, addr, dx, cura) in CARDS:
        d = Data(file)
        g, pcs = card_grid(d, scene, anim, 0)
        g = g.copy()
        # ACT -> ATO (columns 5+dx .. 47+dx)
        erase(g, dx, dx + 48, 0, 23)
        place(g, cards.A, 3 + dx, 1, 1); place(g, cards.T, 18 + dx, 1, 1)
        place(g, make_O(cards.C), 32 + dx, 1, 1)
        if cura:   # CURE -> CURA: the E (cols 47..62 of line 2) becomes an A
            erase(g, 47, 63, 48, 71)
            place(g, cards.A, 48, 49, 49)
        vram = write_grid(d, scene, anim, g)
        # the blob uploaded at VRAM 0x2000
        orig = d.blob(addr)
        new = bytes(vram[0x2000:0x2000 + len(orig)])
        P.blob(file, addr, new, 'act card %s scene %d: ACT -> ATO%s' % (file, scene, ', CURE -> CURA' if cura else ''))
