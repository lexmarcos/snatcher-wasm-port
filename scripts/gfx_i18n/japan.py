"""Prologue word JAPAN (DATA_D0 scene 1, plane A) -> JAPAO.  Each letter has its own colour pair (fill, outline) = (1,2) (3,4) (5,6) (7,8) (9,10), faded in one after the
other by palette channels: the new letters J A P A~ O keep the pair of their position.  Liberation Serif (Times metrics), size and spacing fitted to the original."""
import sys, os
sys.path.insert(0, os.path.dirname(__file__))
from planes import *
from textfont import *

def apply(P):
    d = Data('DATA_D0'); e = PlaneEdit(d, 1)
    e.im[76:122, 40:222] = 11
    f = Face(SERIF, 44, 1.0, track=3.0)
    text = 'JAPÃO'
    lay, total = f.layout(text)
    x0 = int(round(130 - total / 2)); base = 118
    for k, (ch, px) in enumerate(lay):
        g = f.glyph(ch)
        a, l, t = g
        cov = np.zeros(e.im.shape, dtype=np.uint8)
        y0 = base + t; xx = x0 + px + l
        cov[y0:y0 + a.shape[0], xx:xx + a.shape[1]] = a
        fill, edge = 1 + 2 * k, 2 + 2 * k
        e.im[(cov > 40) & (cov <= 150)] = edge
        e.im[cov > 150] = fill
    return e.commit(P, 'DATA_D0.BIN', 'prologue: JAPAO (tiles)', 'prologue: JAPAO nametable', grow_dst=0x2000)
