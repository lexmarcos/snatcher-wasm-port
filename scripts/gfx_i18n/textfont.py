"""Anti-aliased serif text for the pictures (Times New Roman look; Liberation Serif is metric compatible).
Glyphs are drawn big (S times) and box-filtered down, so the grey levels are true area coverage; `xscale` squeezes the face horizontally
to the advance widths of the original pictures. Pen positions are the rounded sums of the unrounded advances."""
import numpy as np
from PIL import Image, ImageDraw, ImageFont

SERIF = '/usr/share/fonts/truetype/liberation/LiberationSerif-Regular.ttf'
SERIF_BOLD = '/usr/share/fonts/truetype/liberation/LiberationSerif-Bold.ttf'
S = 8

class Face:
    def __init__(self, path, size, xscale=1.0, baseline=None):
        self.size = size; self.xscale = xscale
        self.font = ImageFont.truetype(path, size * S)
        self.baseline = baseline
        self._cache = {}
    def adv(self, ch):
        return self.font.getlength(ch) * self.xscale / S
    def glyph(self, ch, subx=0.0):
        """coverage array (0..255) of ch with its pen origin at pixel column PAD + subx; returns (array, left, top): left/top relative to the pen/baseline"""
        key = (ch, round(subx * 4))
        if key in self._cache: return self._cache[key]
        PAD = 4
        wpx = int(self.font.getlength(ch) * self.xscale / S) + 2 * PAD + 6
        hpx = self.size * 2
        base = self.size + 2
        big = Image.new('L', (int(wpx * S / self.xscale) + S * 8, hpx * S), 0)
        ImageDraw.Draw(big).text((int((PAD + subx) * S / self.xscale), base * S), ch, font=self.font, fill=255, anchor='ls')
        small = big.resize((int(big.width * self.xscale / S), hpx), Image.BOX)
        a = np.array(small)
        ys, xs = np.nonzero(a > 3)
        if len(xs) == 0:
            self._cache[key] = None; return None
        r = (a[ys.min():ys.max() + 1, xs.min():xs.max() + 1], int(xs.min()) - PAD, int(ys.min()) - base)
        self._cache[key] = r
        return r
    def layout(self, text):
        """[(char, integer pen x)], total width"""
        x = 0.0; out = []
        for ch in text:
            out.append((ch, int(round(x)))); x += self.adv(ch)
        return out, x

def quantize(cov, levels=((0, 0.0), (11, 0.29), (10, 0.43), (9, 0.71), (7, 1.0)), floor=0.14):
    """coverage 0..255 -> palette index by the nearest brightness (white text on black: grey = coverage)"""
    c = cov.astype(float) / 255.0
    vals = np.array([l[1] for l in levels]); idx = np.array([l[0] for l in levels], dtype=np.uint8)
    out = idx[np.argmin(np.abs(c[..., None] - vals[None, None, :]), axis=-1)]
    out[c < floor] = 0
    return out.astype(np.uint8)

def quantize_caption(cov):
    """coverage 0..255 -> palette index (white text, grey levels 9/10/11), cut points fitted so that the grey-level mix equals the original caption's"""
    out = np.zeros(cov.shape, dtype=np.uint8)
    out[(cov > 24) & (cov <= 60)] = 11
    out[(cov > 60) & (cov <= 105)] = 10
    out[(cov > 105) & (cov <= 150)] = 9
    out[cov > 150] = 7
    return out
