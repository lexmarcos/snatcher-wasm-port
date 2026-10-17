"""Pixel fonts of the OPTIONS screen (DATA_D2, docs/i18n-graphics.md), cut from the English screen: the label font of the grey / yellow labels
(plane tiles, fill 4 + shadow 8) and the small value font of the blue / white sprite texts (fill 14 or 11 + shadow 8).  Glyphs are masks
(0 background, 1 fill, 2 shadow) with width = fill width + 1 shadow column; the advance of a glyph is its fill width + 1 (measured on the originals).
Missing letters and accents are drawn here in the same stroke vocabulary (2 px stems, shadow = fill shifted by (+1, +1))."""
import numpy as np

def art(rows):
    """ASCII art: '#' fill; the shadow (fill shifted by (+1,+1), not on fill) is added; returns (rows + 1) x (width + 1) mask"""
    h = len(rows); w = max(len(r) for r in rows)
    m = np.zeros((h + 1, w + 1), dtype=np.uint8)
    f = np.zeros((h + 1, w + 1), dtype=bool)
    for y, r in enumerate(rows):
        for x, ch in enumerate(r):
            if ch == '#': f[y, x] = True
    sh = np.zeros_like(f); sh[1:, 1:] = f[:-1, :-1]; sh &= ~f
    m[sh] = 2; m[f] = 1
    return m

class Font:
    def __init__(self, space, height=8):
        self.g = {}; self.space = space; self.h = height
    def add_mask(self, ch, region, fill, shadow=8):
        """region: 2-D array of palette indices (rows = font height)"""
        m = np.zeros(region.shape, dtype=np.uint8)
        m[region == shadow] = 2
        m[region == fill] = 1
        # trim the empty columns on the right (a shadow column belongs to the glyph, anything beyond does not)
        cols = np.nonzero(m.any(axis=0))[0]
        m = m[:, :cols.max() + 1]
        self.g[ch] = m
    def adv(self, ch):
        if ch == ' ': return self.space
        m = self.g[ch]
        return int((m == 1).any(axis=0).nonzero()[0].max() + 1) + 1 if ch not in self.fixed_adv else self.fixed_adv[ch]
    fixed_adv = {}
    def fillw(self, ch):
        m = self.g[ch]; return int((m == 1).any(axis=0).nonzero()[0].max() + 1)
    def width(self, text):
        """ink width in pixels of the fill (first pixel to the last fill pixel)"""
        w = 0
        for i, ch in enumerate(text):
            w += self.adv(ch) if i < len(text) - 1 else (self.fillw(ch) if ch != ' ' else 0)
        return w
    def render(self, text):
        """returns (mask, x offsets of every char); glyphs are bottom aligned (accented ones are taller than the font height),
        columns = ink width + 1 shadow column"""
        W = self.width(text) + 2
        H = max([self.h] + [self.g[c].shape[0] for c in text if c != ' '])
        m = np.zeros((H, W), dtype=np.uint8)
        pos = []
        x = 0
        for ch in text:
            pos.append(x)
            if ch != ' ':
                g = self.g[ch]
                sub = m[H - g.shape[0]:, x:x + g.shape[1]]
                gm = g[:, :sub.shape[1]]
                sub[(gm == 2) & (sub == 0)] = 2
                sub[gm == 1] = 1
            x += self.adv(ch)
        return m, pos
