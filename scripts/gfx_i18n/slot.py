"""CONTINUE slot screen (DATA_D2 module state 10): header, PLAY / ACT / SAVES labels and NO DATA (docs/i18n-graphics.md).
Built on the finished screen of the English engine (VRAM dump + which blob wrote every byte), edited at pixel level, re-tiled into the tiles the
changed cells used (nothing grows) and written back into the blobs behind those tiles and nametable entries."""
import sys, os
sys.path.insert(0, os.path.dirname(__file__))
from vrammap import *
from textfont import *

def g(rows): return np.array([[c == '#' for c in r.replace('b', '#')] for r in rows], dtype=bool)
TINY = {   # 3 x 5 labels
    'A': ['###', '#.#', '###', '#.#', '#.#'], 'C': ['###', '#..', '#..', '#..', '###'], 'E': ['###', '#..', '###', '#..', '###'],
    'L': ['#..', '#..', '#..', '#..', '###'], 'M': ['#.#', '###', '###', '#.#', '#.#'], 'O': ['###', '#.#', '#.#', '#.#', '###'],
    'P': ['###', '#.#', '###', '#..', '#..'], 'S': ['###', '#..', '###', '..#', '###'], 'T': ['###', '.#.', '.#.', '.#.', '.#.'],
    'V': ['#.#', '#.#', '#.#', '#.#', '.#.'], 'Y': ['#.#', '#.#', '.#.', '.#.', '.#.'],
}
TINY = {k: g(v) for k, v in TINY.items()}
BOLD = {   # 4 x 7 (NO DATA font), M is 5 wide
    'S': ['.###', '#...', '#...', '.##.', '...#', '...#', '###.'], 'E': ['####', '#...', '#...', '###.', '#...', '#...', '####'],
    'M': ['#...#', '##.##', '#.#.#', '#...#', '#...#', '#...#', '#...#'], 'D': ['###.', '#..#', '#..#', '#..#', '#..#', '#..#', '###.'],
    'A': ['.##.', '#..#', '#..#', '#..#', '####', '#..#', '#..#'], 'O': ['.##.', '#..#', '#..#', '#..#', '#..#', '#..#', '.##.'],
}
BOLD = {k: g(v) for k, v in BOLD.items()}

def tiny_text(im, text, x, y, color):
    for ch in text:
        m = TINY[ch]
        for (yy, xx), v in np.ndenumerate(m):
            if v: im[y + yy, x + xx] = color
        x += 4
    return x

def bold_text(im, text, x, y, white=1, shadow=8):
    M = np.zeros(im.shape, dtype=bool)
    for ch in text:
        if ch == ' ': x += 3; continue
        m = BOLD[ch]
        for (yy, xx), v in np.ndenumerate(m):
            if v: M[y + yy, x + xx] = True
        x += m.shape[1] + 1
    S = np.roll(np.roll(M, 1, 0), 1, 1) & ~M
    im[S] = shadow; im[M] = white

SAVES = '0:re/oracle/out/fid2/cont/save0.hex,2:re/oracle/out/fid2/cont/save1.hex'

def build(frames=1500, saves=SAVES):
    log, vram, cram = run_engine(frames, '1300:128', 'boot', os.path.join(ROOT, 'work/i18n/gfx/slotrun'), env={'SPRTILES': '1'}, saves=saves)
    vm = VramMap(log)
    excl = set()
    for l in log:
        if l.startswith('sprite tiles:'):
            excl = set(int(x) for x in re.findall(r'\d+', l.split(':', 1)[1]))
    d = Data('DATA_D2')
    s = ScreenEdit(d, vram, cram, vm)
    s.exclude = excl | set(range(0, 256))
    return s

def edit(s):
    im = s.im
    for k in range(4):
        dy = 32 * k
        # PLAY -> TEMPO, ACT -> ATO, SAVES -> SALVOS (3 x 5 labels, colour 11 on 5)
        im[77 + dy:84 + dy, 66:88] = 5; tiny_text(im, 'TEMPO', 68, 78 + dy, 11)   # cells 8..10 only: cell 11 holds the NO DATA / play time text
        im[64 + dy:71 + dy, 167:184] = 5; tiny_text(im, 'ATO', 169, 65 + dy, 11)
        im[77 + dy:84 + dy, 150:178] = 5; tiny_text(im, 'SALVOS', 152, 78 + dy, 11)
    return s

NMASK = g(['#..#', '##.#', '##.#', '#.##', '#.##', '#..#', '#..#'])
def nodata(s, text='SEM DADOS'):
    """replace NO DATA in every slot that shows it in this state of the screen"""
    n = 0
    for k in range(4):
        y = 76 + 32 * k
        if ((s.im0[y:y + 7, 93:97] == 1) == NMASK).all():
            s.im[y - 1:y + 10, 91:139] = 5
            bold_text(s.im, text, 93, y)
            n += 1
    return n

def header(s, text='INVESTIGA\u00c7\u00c3O'):
    im = s.im
    # banner bar: colour 3 on rows 46..54, 2 around it; white strokes with a (+1, +1) shadow of colour 5 (the original lettering is serif bold, 10 rows of capitals)
    for y in range(43, 57): im[y, 58:201] = 3 if 46 <= y <= 54 else 2
    f0 = Face(SERIF_BOLD, 15, 1.0)
    xs = 141.0 / f0.layout('INVESTIGATION FILE')[1]
    f = Face(SERIF_BOLD, 15, xs)
    cov = np.zeros(im.shape, dtype=np.uint8)
    lay, total = f.layout(text)
    x0 = int(round(128.5 - total / 2)); base = 55
    for ch, px in lay:
        g = f.glyph(ch)
        if g is None: continue
        a, l, t = g
        y0 = base + t; xx = x0 + px + l
        sub = cov[y0:y0 + a.shape[0], xx:xx + a.shape[1]]
        np.maximum(sub, a[:sub.shape[0], :sub.shape[1]], out=sub)
    M = cov > 110
    S = np.roll(np.roll(M, 1, 0), 1, 1) & ~M
    im[S] = 5; im[M] = 1

def apply(P):
    s = edit(build())
    header(s)
    # NO DATA stays English: its cells are written at run time (the slot panel is drawn into Word RAM by the module code), a patch of the blobs
    # behind them leaked into the play time digits (tried: SEM DADOS)
    print('slot screen', s.commit(P, 'DATA_D2.BIN', 'CONTINUE slot screen', grow=(0x2e22e, 0xd000, 30)))
    ScreenEdit.flush(P, 'DATA_D2.BIN', 'CONTINUE slot screen: header INVESTIGACAO, labels TEMPO / ATO / SALVOS')
