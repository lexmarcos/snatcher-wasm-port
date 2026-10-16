"""Skeleton font of the dedication picture (DATA_D1 scene 2): 1 px bright strokes (colour 11, 10 at ends and corners) with a 1 px dark outline (9)."""
import sys, os
sys.path.insert(0, os.path.dirname(__file__))
from planes import *

TEXT = ['THIS STORY IS DEDICATED', 'TO ALL THOSE CYBERPUNKS', 'WHO FIGHT AGAINST INJUSTICE', 'AND CORRUPTION EVERY DAY', 'OF THEIR LIVES.']
BG, DARK, MID, BRIGHT = 8, 9, 10, 11
TOPS = [51, 75, 99, 123, 147]      # first outline row of each line

def sh(S, dy, dx): return np.roll(np.roll(S, dy, 0), dx, 1)

def render_skeleton(S, im=None):
    """colour image from a skeleton mask: outline = 4-neighbours, bright on straight runs and junctions, mid on ends and corners"""
    L = sh(S, 0, 1); R = sh(S, 0, -1); U = sh(S, 1, 0); D = sh(S, -1, 0)
    n = L.astype(int) + R + U + D
    bright = ((L & R) | (U & D) | (n >= 3))
    out = np.full(S.shape, BG, dtype=np.uint8)
    out[S | L | R | U | D] = DARK
    out[S] = np.where(bright[S], BRIGHT, MID)
    return out

def extract():
    d = Data('DATA_D1'); p = Plane(d, 2); im, pl = p.index_image(28)
    S = im >= MID
    S[223:] = False
    glyphs = {}; starts = []
    for k, text in enumerate(TEXT):
        y0 = TOPS[k] + 1; band = S[y0:y0 + 10]
        cols = band.any(axis=0)
        runs = []; x = 0
        while x < 256:
            if cols[x]:
                s = x
                while x < 256 and cols[x]: x += 1
                runs.append((s, x - 1))
            else: x += 1
        letters = [c for c in text if c != ' ']
        if len(runs) != len(letters):
            print('line', k, 'runs', len(runs), 'letters', len(letters)); continue
        for (s, e), ch in zip(runs, letters):
            glyphs.setdefault(ch, []).append(band[:, s:e + 1].copy())
        starts.append(runs)
    return im, S, glyphs, starts
if __name__ == '__main__':
    im, S, glyphs, starts = extract()
    print(sorted(glyphs))
    for k, r in enumerate(starts): print(k, [(s, e - s + 1) for s, e in r][:12])


def _m(rows):
    return np.array([[c == '#' for c in r] for r in rows], dtype=bool)

class Skel:
    GAP = 1; WORD = 8
    def __init__(self):
        im, S, glyphs, starts = extract()
        self.g = {c: l[0] for c, l in glyphs.items() if c != '.'}
        self.g['.'] = glyphs['.'][0]
        # letters not in the original sentence, drawn in the same 1 px stroke vocabulary (10 rows, row 9 empty)
        self.g['D'] = _m(['#####..', '.#...#.', '.#....#', '.#....#', '.#....#', '.#....#', '.#....#', '.#...#.', '#####..', '.......'])
        self.g['M'] = _m(['##.....##', '.##...##.', '.#.#.#.#.', '.#..#..#.', '.#.....#.', '.#.....#.', '.#.....#.', '.#.....#.', '##.....##', '.........'])
        q = np.zeros((10, 8), dtype=bool); q[:9, :7] = self.g['O'][:9]; q[7, 4] = True; q[8, 5] = True; q[9, 5:8] = True
        self.g['Q'] = q
        self.g['Z'] = _m(['#######', '.....#.', '....#..', '...#...', '..#....', '.#.....', '#......', '#.....#', '#######', '.......'])
        self.g['X'] = _m(['##...##', '.#...#.', '..#.#..', '...#...', '..#.#..', '.#...#.', '##...##', '.......', '.......', '.......'])
        self.g['M'] = self.g['M']
        self.g['-'] = _m(['.....', '.....', '.....', '.....', '#####', '.....', '.....', '.....', '.....', '.....'])
    def accented(self, base, kind):
        return ('acute', base) if kind else None
    def width(self, ch): return self.g[ch].shape[1] if ch in self.g else 0
    def adv(self, ch):
        return self.WORD if ch == ' ' else self.glyph(ch).shape[1] + self.GAP
    def glyph(self, ch):
        """mask (rows -3..11 relative to the top skeleton row are allowed through `extra`): returns the 10-row mask of the base letter"""
        base, acc = BASE.get(ch, (ch, None))
        return self.g[base]
    def text_width(self, text):
        return sum(self.adv(c) for c in text) - self.GAP

BASE = {'Ó': ('O', 'acute'), 'É': ('E', 'acute'), 'Ã': ('A', 'tilde'), 'Ç': ('C', 'cedilla'), 'Á': ('A', 'acute'), 'Ê': ('E', 'circ'), 'Õ': ('O', 'tilde')}

def put_text(S, font, text, x, ytop):
    """set the skeleton pixels of `text` (glyph row 0 at row ytop) in the boolean image S"""
    for ch in text:
        if ch == ' ':
            x += font.WORD; continue
        base, acc = BASE.get(ch, (ch, None))
        g = font.g[base]; h, w = g.shape
        for (yy, xx), v in np.ndenumerate(g):
            if v: S[ytop + yy, x + xx] = True
        c = x + w // 2
        if acc == 'acute': S[ytop - 3, c + 1] = True; S[ytop - 2, c] = True
        elif acc == 'tilde': S[ytop - 2, c - 2] = True; S[ytop - 3, c - 1] = True; S[ytop - 2, c] = True; S[ytop - 3, c + 1] = True
        elif acc == 'circ': S[ytop - 3, c] = True; S[ytop - 2, c - 1] = True; S[ytop - 2, c + 1] = True
        elif acc == 'cedilla': S[ytop + 9, c] = True; S[ytop + 10, c - 1] = True; S[ytop + 10, c] = True
        x += w + font.GAP
    return x
