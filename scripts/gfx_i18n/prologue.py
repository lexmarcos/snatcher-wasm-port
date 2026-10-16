"""Text pictures of the prologue (DATA_D1 / DATA_D0 plane pictures): date line, "50 years later", ... (docs/i18n-graphics.md).
Text is Liberation Serif Bold (Times New Roman metrics) squeezed to the widths of the original lettering, 5 grey levels like the original."""
import sys, os
sys.path.insert(0, os.path.dirname(__file__))
from planes import *
from textfont import *

MONO = '/usr/share/fonts/opentype/urw-base35/NimbusMonoPS-Regular.otf'
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


# ---- RSS credit (DATA_D1 scene 1): the logo and the RSS (Roland Sound Space) line stay, the sentence around them is translated
THR_RSS = ((25, 2), (60, 3), (110, 7))      # palette 0xEEE = 7, thin AA 3 / 2
def rss(P):
    d = Data('DATA_D1'); e = PlaneEdit(d, 1)
    f = Face(MONO, 20, 0.7, track=1.0)   # Courier-like, 9 px pitch as the original
    e.im[118:140, :] = 0; e.im[166:188, :] = 0
    draw_text_thr(e.im, f, 'Este jogo utiliza o', 126.5, 134, THR_RSS, align='center')
    draw_text_thr(e.im, f, 'em algumas partes.', 129, 182, THR_RSS, align='center')
    return e.commit(P, 'DATA_D1.BIN', 'prologue: RSS credit sentence (tiles)', 'prologue: RSS credit nametable')

# ---- disclaimer (DATA_D1 scene 0) and dedication (scene 2): bitmap fonts cut from the originals (bmfont.py, skelfont.py)
import bmfont, skelfont
DISCLAIMER = ['A história, os personagens', 'e os eventos deste jogo', 'são totalmente fictícios.', 'Qualquer semelhança com', 'pessoas, lugares ou fatos', 'reais é mera coincidência.']
DEDICATION = ['ESTA HISTÓRIA É DEDICADA', 'A TODOS OS CYBERPUNKS QUE', 'LUTAM CONTRA A INJUSTIÇA', 'E A CORRUPÇÃO TODOS OS', 'DIAS DE SUAS VIDAS.']

def disclaimer(P):
    d = Data('DATA_D1'); e = PlaneEdit(d, 0)
    f = bmfont.Font()
    e.im[:] = bmfont.BG
    wmax = max(f.text_width(t) for t in DISCLAIMER)
    x0 = int(round(130 - wmax / 2)); top = 64
    for k, t in enumerate(DISCLAIMER):
        bmfont.draw_bm(f, e.im, t, x0, top + 16 * k)
    return e.commit(P, 'DATA_D1.BIN', 'prologue: disclaimer text (tiles)', 'prologue: disclaimer nametable')

def dedication(P):
    d = Data('DATA_D1'); e = PlaneEdit(d, 2)
    f = skelfont.Skel()
    S = np.zeros(e.im.shape, dtype=bool)
    for k, t in enumerate(DEDICATION):
        skelfont.put_text(S, f, t, int(round(128 - f.text_width(t) / 2)), skelfont.TOPS[k] + 1)
    out = skelfont.render_skeleton(S)
    e.im[40:176, :] = out[40:176, :]
    return e.commit(P, 'DATA_D1.BIN', 'prologue: dedication text (tiles)', 'prologue: dedication nametable')

def apply(P):
    print('moscow', moscow(P)); print('50 years', years_later(P)); print('rss', rss(P))
    print('disclaimer', disclaimer(P)); print('dedication', dedication(P))
