"""Pixel font of the videophone banners (DATA_M0 scenes 4 and 9, plane B): 9 row capitals, 1 px strokes with serifs, white (colour 2) with a
shadow of the letter shifted by (+1, +1) (colour 3) on the blue panel (colour 4)."""
import sys, os
sys.path.insert(0, os.path.dirname(__file__))
from planes import *

ENT = {4: [dict(src=0x2bcc8, dst=0x2000), dict(src=0x2f782, dst=0x6000), dict(src=0x31d12, dst=0xC000)]}
WHITE, SHADOW, PANEL = 2, 3, 4

def load(scene):
    d = Data('DATA_M0')
    # the three blobs of the picture are the same for the two banners except the nametable; find the entries from the tile channel trace
    return d

def glyphs_from(im, rows, text):
    """rows: (top, bottom) of the white mask of a text line; text: its letters"""
    y0, y1 = rows
    band = (im[y0:y1 + 1] == WHITE)
    cols = band.any(axis=0)
    runs = []; x = 0
    while x < len(cols):
        if cols[x]:
            s = x
            while x < len(cols) and cols[x]: x += 1
            runs.append((s, x - 1))
        else: x += 1
    letters = [c for c in text if c != ' ']
    return runs, letters, band


ENT[9] = [dict(src=0x2f310, dst=0x2000), dict(src=0x2f782, dst=0x6000), dict(src=0x327f6, dst=0xC000)]
LINES = {4: [((37, 45), 'NOT IN'), ((55, 63), 'SERVICE')], 9: [((57, 65), 'NO VIDEO'), ((78, 86), 'AVAILABLE')]}

def extract():
    d = Data('DATA_M0'); g = {}; info = []
    for sc, lines in LINES.items():
        p = Plane(d, sc, plane_base=0xC000, entries=ENT[sc]); im, pl = p.index_image(28)
        for (r, t) in lines:
            runs, letters, band = glyphs_from(im, r, t)
            runs = [x for x in runs if 70 < x[0] < 160]
            if len(runs) != len(letters): print('mismatch', sc, t, len(runs), len(letters)); continue
            for (s, e), ch in zip(runs, letters):
                g.setdefault(ch, []).append(band[:, s:e + 1].copy()); info.append((sc, t, ch, s, e - s + 1))
    return g, info
if __name__ == '__main__':
    g, info = extract()
    print(sorted(g))
    for ch in sorted(g):
        print(ch, g[ch][0].shape)
        for r in g[ch][0]: print(''.join('#' if v else '.' for v in r))


def _m(rows): return np.array([[c == '#' for c in r] for r in rows], dtype=bool)

class VP:
    GAP = 2; SPACE = 7
    def __init__(self):
        g, info = extract()
        self.g = {c: l[0] for c, l in g.items()}
        # letters that are not in the original banners, same 9 row / 1 px stroke / serif vocabulary
        self.g['A'] = _m(['....#....', '...#.#...', '...#.#...', '..#...#..', '..#...#..', '.#######.', '.#.....#.', '#.......#', '###...###'])
        self.g['L'] = _m(['###..', '.#...', '.#...', '.#...', '.#...', '.#...', '.#...', '.#..#', '#####'])
        self.g['F'] = _m(['######', '#....#', '#.....', '#...#.', '#####.', '#.....', '#.....', '#.....', '###...'])
        self.g['M'] = _m(['##.....##', '.#.....#.', '.##...##.', '.#.#.#.#.', '.#..#..#.', '.#.....#.', '.#.....#.', '.#.....#.', '###...###'])
        self.g['P'] = _m(['#####..', '#....#.', '#....#.', '#....#.', '#####..', '#......', '#......', '#......', '###....'])
        self.g['B'] = _m(['#####..', '#....#.', '#....#.', '#....#.', '#####..', '#....#.', '#.....#', '#....#.', '#####..'])
    def mask(self, ch):
        acc = {'Í': ('I', 'acute'), 'Ç': ('C', 'cedilla')}.get(ch)
        base = acc[0] if acc else ch
        m = self.g[base]
        if acc:
            h, w = m.shape
            if acc[1] == 'acute':
                n = np.zeros((h + 3, w), dtype=bool); n[3:] = m; c = w // 2
                n[0, c + 1] = True; n[1, c] = True; n[2, c - 1] = False
                m = n
            else:
                n = np.zeros((h + 2, w), dtype=bool); n[:h] = m; c = w // 2
                n[h, c] = True; n[h + 1, c - 1] = True; n[h + 1, c] = True
                m = n
        return m, (3 if acc and acc[1] == 'acute' else 0)
    def adv(self, ch):
        return self.SPACE if ch == ' ' else self.mask(ch)[0].shape[1] + self.GAP
    def width(self, text):
        return sum(self.adv(c) for c in text) - self.GAP

def draw_vp(font, im, text, cx, ytop, white=WHITE, shadow=SHADOW):
    """white glyphs with their (+1, +1) shadow; text centred on column cx, glyph row 0 at row ytop"""
    M = np.zeros(im.shape, dtype=bool)
    x = int(round(cx - font.width(text) / 2))
    for ch in text:
        if ch == ' ': x += font.SPACE; continue
        m, up = font.mask(ch)
        h, w = m.shape
        y0 = ytop - up
        for (yy, xx), v in np.ndenumerate(m):
            if v: M[y0 + yy, x + xx] = True
        x += w + font.GAP
    S = np.roll(np.roll(M, 1, 0), 1, 1) & ~M
    im[S] = shadow; im[M] = white
    return M
