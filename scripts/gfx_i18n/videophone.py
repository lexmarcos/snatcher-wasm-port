"""Videophone banners (DATA_M0 scenes 4 and 9): NOT IN SERVICE -> FORA DE SERVICO, NO VIDEO AVAILABLE -> SEM SINAL DE VIDEO."""
import sys, os
sys.path.insert(0, os.path.dirname(__file__))
from vpfont import *

def banner(P, sc, lines, clear, cx, name, x0=68, x1=164):
    d = Data('DATA_M0')
    e = PlaneEdit(d, sc, plane_base=0xC000, entries=ENT[sc])
    for (y0, y1) in clear: e.im[y0:y1, x0:x1] = PANEL
    f = VP()
    for text, ytop in lines: draw_vp(f, e.im, text, cx, ytop)
    return e.commit(P, 'DATA_M0.BIN', 'videophone banner %s (tiles)' % name, 'videophone banner %s (nametable)' % name, grow_dst=0x2000)

def apply(P):
    print('vp not in service', banner(P, 4, [('FORA DE', 37), ('SERVIÇO', 55)], [(32, 49), (52, 69)], 115.5, 'FORA DE SERVICO'))
    print('vp no video', banner(P, 9, [('SEM SINAL', 57), ('DE VÍDEO', 78)], [(52, 70), (74, 92)], 115.5, 'SEM SINAL DE VIDEO', 74, 158))
