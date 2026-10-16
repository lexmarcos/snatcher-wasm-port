"""Bitmap font cut from the original disclaimer picture (DATA_D1 scene 0): white mask glyphs with a 1 px shadow on the right."""
import sys, os
sys.path.insert(0, os.path.dirname(__file__))
from planes import *

LINES = ['The story, characters,', 'and events, in this game', 'are entirely fictional.', 'Any similarities to', 'actual people, places', 'and events are entirely', 'coincidental.']
Y0 = [56 + 16 * k for k in range(7)]       # top row of the capitals of each line
BG = 5; WHITE = 1; SHADOW = 7

def extract():
    d = Data('DATA_D1'); p = Plane(d, 0); im, pl = p.index_image(28)
    white = (im == WHITE)
    glyphs = {}; info = []
    for k, text in enumerate(LINES):
        y0 = Y0[k] - 2; y1 = Y0[k] + 12
        band = white[y0:y1]
        cols = band.any(axis=0)
        # runs of columns
        runs = []; x = 0
        while x < 256:
            if cols[x]:
                s = x
                while x < 256 and cols[x]: x += 1
                runs.append((s, x - 1))
            else: x += 1
        letters = [c for c in text if c != ' ']
        info.append((k, len(runs), len(letters)))
        if len(runs) != len(letters): continue
        for (s, e), ch in zip(runs, letters):
            g = band[:, s:e + 1].copy()
            glyphs.setdefault(ch, []).append((g, s, e, k))
    return im, white, glyphs, info
if __name__ == '__main__':
    im, white, glyphs, info = extract()
    print(info)
    print(sorted(glyphs))


def _a(rows):
    return np.array([[c == '#' for c in r] for r in rows], dtype=bool)

def pad_rows(g, top=0, total=10):
    out = np.zeros((total, g.shape[1]), dtype=bool); out[top:top + g.shape[0]] = g; return out

class Font:
    """glyph masks with row 0 = top row of the capitals (10 rows: 7 cap rows + descender rows)"""
    def __init__(self):
        im, white, glyphs, info = extract()
        self.g = {}
        for ch, lst in glyphs.items():
            g = lst[0][0][2:12]            # the band started 2 rows above the cap top
            g = np.pad(g, ((0, 10 - g.shape[0]), (0, 0)))
            self.g[ch] = g.astype(bool)
        self.adv_extra = {'i': 1, 'l': 1}
        # synthesized letters, same 2 px stroke vocabulary
        p = self.g['p']; self.g['q'] = p[:, ::-1].copy()
        self.g['j'] = pad_rows(_a(['..##', '....', '..##', '..##', '..##', '..##', '###.']), top=1)
        self.g['Q'] = pad_rows(_a(['.####.', '##..##', '##..##', '##..##', '##.###', '##..##', '.#####', '....##']), top=0)
        self.g['b'] = self.g['d'][:, ::-1].copy() if 'd' in self.g else None
        self.g['x'] = pad_rows(_a(['##..##', '.####.', '..##..', '.####.', '##..##']), top=2)
        self.g['z'] = pad_rows(_a(['######', '...##.', '..##..', '.##...', '######']), top=2)
        self.g['E'] = pad_rows(_a(['######', '##....', '##....', '#####.', '##....', '##....', '######']), top=0)
        self.g['O'] = pad_rows(_a(['.####.', '##..##', '##..##', '##..##', '##..##', '##..##', '.####.']), top=0)
        self.g['H'] = pad_rows(_a(['##..##', '##..##', '##..##', '######', '##..##', '##..##', '##..##']), top=0)
        self.g['P'] = pad_rows(_a(['#####.', '##..##', '##..##', '#####.', '##....', '##....', '##....']), top=0)
        self.g['S'] = pad_rows(_a(['.#####', '##....', '##....', '.####.', '....##', '....##', '#####.']), top=0)
        self.g['C'] = pad_rows(_a(['.#####', '##....', '##....', '##....', '##....', '##....', '.#####']), top=0)
        self.g['N'] = pad_rows(_a(['##..##', '###.##', '######', '######', '##.###', '##..##', '##..##']), top=0)
        self.g['U'] = pad_rows(_a(['##..##', '##..##', '##..##', '##..##', '##..##', '##..##', '.####.']), top=0)
        self.g['R'] = pad_rows(_a(['#####.', '##..##', '##..##', '#####.', '##.##.', '##..##', '##..##']), top=0)
        self.g['I'] = pad_rows(_a(['####', '.##.', '.##.', '.##.', '.##.', '.##.', '####']), top=0)
        self.g['M'] = pad_rows(_a(['##...##', '###.###', '#######', '##.#.##', '##...##', '##...##', '##...##']), top=0)
        self.g['F'] = pad_rows(_a(['#####.', '##....', '##....', '####..', '##....', '##....', '##....']), top=0)
        self.g['D'] = pad_rows(_a(['####..', '##.##.', '##..##', '##..##', '##..##', '##.##.', '####..']), top=0)
        self.g['B'] = pad_rows(_a(['#####.', '##..##', '##..##', '#####.', '##..##', '##..##', '#####.']), top=0)
        self.g['G'] = pad_rows(_a(['.#####', '##....', '##....', '##.###', '##..##', '##..##', '.#####']), top=0)
        self.g['L'] = pad_rows(_a(['##....', '##....', '##....', '##....', '##....', '##....', '######']), top=0)
        self.g['V'] = pad_rows(_a(['##..##', '##..##', '##..##', '##..##', '##..##', '.####.', '..##..']), top=0)
        self.g['J'] = pad_rows(_a(['..####', '...##.', '...##.', '...##.', '##.##.', '##.##.', '.###..']), top=0)
        self.g['W'] = pad_rows(_a(['##...##', '##...##', '##...##', '##.#.##', '#######', '###.###', '##...##']), top=0)
        self.g['k'] = pad_rows(_a(['##....', '##....', '##..##', '##.##.', '####..', '##.##.', '##..##']), top=0)
        self.g['w'] = pad_rows(_a(['##...##', '##...##', '##.#.##', '#######', '.##.##.']), top=2)
        self.g['ç'] = np.vstack([self.g['c'][:8], pad_rows(_a(['..##..']), top=0, total=2)])[:10]
        self.g['ç'][7] = False; self.g['ç'][7, 2:4] = True
        self.g['-'] = pad_rows(_a(['####']), top=4)
        self.g[' '] = np.zeros((10, 0), dtype=bool)
        for accent, name in (('á', 'a'), ('é', 'e'), ('ó', 'o'), ('ú', 'u'), ('à', 'a')):
            self.g[accent] = self.accent(name, 'acute' if accent != 'à' else 'grave')
        self.g['ã'] = self.accent('a', 'tilde'); self.g['õ'] = self.accent('o', 'tilde')
        self.g['ê'] = self.accent('e', 'circ'); self.g['ô'] = self.accent('o', 'circ'); self.g['â'] = self.accent('a', 'circ')
        base = np.zeros((10, 3), dtype=bool); base[3:7, 0:2] = True
        ii = base.copy(); ii[0, 2] = True; ii[1, 1] = True      # i-acute: the dot is replaced by the accent
        self.g['í'] = ii
        for up, lo in (('Á', 'A'), ('É', 'E'), ('Ó', 'O')):
            pass
    def accent(self, base, kind):
        g = self.g[base].copy()
        w = g.shape[1]
        g[0:2] = False
        if kind == 'acute': g[0, w - 3:w - 1] = True; g[1, w - 4:w - 2] = True
        elif kind == 'grave': g[0, 2:4] = True; g[1, 3:5] = True
        elif kind == 'circ': g[0, 2:4] = True; g[1, 1:2] = True; g[1, 4:5] = True
        elif kind == 'tilde': g[0, 1:3] = True; g[0, 4] = True; g[1, 0] = True; g[1, 3] = True
        return g
    def width(self, ch):
        return self.g[ch].shape[1]
    def advance(self, ch):
        if ch == ' ': return 8
        return self.width(ch) + 2 + self.adv_extra.get(ch, 0)
    def text_width(self, text):
        return sum(self.advance(c) for c in text) - 2

def draw_bm(font, im, text, x, ytop, white=WHITE, shadow=SHADOW, bg=BG):
    """draw into index image `im`: glyph row 0 at row ytop; returns the end x"""
    for ch in text:
        g = font.g[ch]
        h, w = g.shape
        for (yy, xx), v in np.ndenumerate(g):
            if v and 0 <= ytop + yy < im.shape[0] and 0 <= x + xx < im.shape[1]:
                im[ytop + yy, x + xx] = white
        for (yy, xx), v in np.ndenumerate(g):
            if v and not (xx + 1 < w and g[yy, xx + 1]):
                if 0 <= ytop + yy < im.shape[0] and 0 <= x + xx + 1 < im.shape[1] and im[ytop + yy, x + xx + 1] != white:
                    im[ytop + yy, x + xx + 1] = shadow
        x += font.advance(ch)
    return x
