"""Text pictures of the prologue (DATA_D1 / DATA_D0 plane pictures): date line, "50 years later", ... (docs/i18n-graphics.md).
Text is Liberation Serif Bold (Times New Roman metrics) squeezed to the widths of the original lettering, 5 grey levels like the original."""
import sys, os
sys.path.insert(0, os.path.dirname(__file__))
from planes import *
from textfont import *

LV = ((1, .14), (2, .29), (3, .57), (4, .71), (5, 1.0))     # palette 0 of scenes 3 and 6: 0x222 0x444 0x888 0xAAA 0xEEE
F_BIG = lambda: Face(SERIF_BOLD, 30, 0.957)
F_MID = lambda: Face(SERIF_BOLD, 24, 1.04)

def moscow(P):
    d = Data('DATA_D1'); e = PlaneEdit(d, 3)
    e.im[:] = 0
    draw_text(e.im, F_BIG(), 'Moscou:', 129.5, 100, LV, align='center')
    draw_text(e.im, F_MID(), '6 de junho de 1996', 128, 129, LV, align='center')
    return e.commit(P, 'DATA_D1.BIN', 'prologue: "Moscou: 6 de junho de 1996" (tiles)', 'prologue: Moscow caption nametable')

def years_later(P):
    d = Data('DATA_D1'); e = PlaneEdit(d, 6)
    e.im[:] = 0
    draw_text(e.im, F_BIG(), '50', 52, 120, LV)
    draw_text(e.im, F_MID(), 'anos depois', 92, 120, LV)
    return e.commit(P, 'DATA_D1.BIN', 'prologue: "50 anos depois" (tiles)', 'prologue: 50 years later nametable')

def apply(P):
    print('moscow', moscow(P)); print('50 years', years_later(P))
