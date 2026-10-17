"""OPTIONS screen (DATA_D2 scene 8, module state 11) -> pt-BR (docs/i18n-graphics.md).
Plane A holds the grey labels (fill 4, shadow 8) and the OPTIONS banner; the sprite objects 32..36 draw the yellow selected row (outline + label on top of the
grey one) and the values (white = row selected, blue = not selected): DEVICE value (frames 9 10 / 19 20), CONTROLS variants (11..16 / 21..26), SOUND MODE value
(17 18 / 27 28), GUN ADJUST enabled (29), selected labels (5 6 7 8 30).  Every variant is redrawn here from the glyphs of the originals."""
import sys, os, re
sys.path.insert(0, os.path.dirname(__file__))
from vrammap import *
from optfont import *
from textfont import *

FILE = 'DATA_D2.BIN'
ANIM = 0x3A790
BLOB_SPR = 0x31484          # sprite tiles of the options scene (VRAM tile 1.., 220 tiles)
BLOB_PLANE = 0x2E67C        # plane tiles of the panel (VRAM 0xD000)

def build(frames=1500):
    log, vram, cram = run_engine(frames, '1301:128', 'boot', os.path.join(ROOT, 'work/i18n/gfx/optrun'), env={})
    vm = VramMap(log)
    d = Data('DATA_D2')
    s = ScreenEdit(d, vram, cram, vm)
    return s, d, vram, cram

# ------------------------------------------------------------------ fonts
LABEL_SRC = {  # text -> (y0, [(x, fill width)] per letter, spaces skipped)
    'DEVICE': (52, [(51, 6), (58, 5), (64, 6), (71, 2), (74, 6), (81, 5)]),
    'CONTROLS': (88, [(51, 6), (58, 6), (65, 6), (72, 6), (79, 6), (86, 6), (93, 5), (99, 6)]),
    'SOUNDMODE': (124, [(51, 6), (58, 6), (65, 6), (72, 6), (79, 6), (91, 6), (98, 6), (105, 6), (112, 5)]),
    'GUNADJUST': (152, [(51, 6), (58, 6), (65, 6), (77, 6), (84, 6), (91, 6), (98, 6), (105, 6), (112, 6)]),
    'QUIT': (179, [(114, 7), (122, 6), (130, 2), (134, 6)]),
}

def label_font(im0):
    f = Font(space=5)
    for text, (y0, cols) in LABEL_SRC.items():
        for ch, (x, w) in zip(text, cols):
            if ch in f.g: continue
            f.add_mask(ch, im0[y0:y0 + 8, x:x + w + 1], 4)
    return f

def frame_image(d, tiles, i):
    """frame i of the option objects' animation table as an index image (0 transparent) + the (x, y) of its top left corner"""
    pcs = d.frame(ANIM, i)
    x0 = min(p[3] for p in pcs); y0 = min(p[0] for p in pcs)
    x1 = max(p[3] + 8 * (((p[1] >> 2) & 3) + 1) for p in pcs); y1 = max(p[0] + 8 * ((p[1] & 3) + 1) for p in pcs)
    g = np.zeros((y1 - y0, x1 - x0), dtype=np.uint8)
    for (y, size, attr, x) in pcs:
        grid, w, h = piece_tiles(size, attr)
        for r in range(h):
            for c in range(w):
                t = tiles[grid[r][c]]
                sub = g[y - y0 + r * 8:y - y0 + r * 8 + 8, x - x0 + c * 8:x - x0 + c * 8 + 8]
                np.copyto(sub, np.where(t > 0, t, sub))
    return g, (x0, y0)

VALUE_SRC = [   # frame, row offset, [(char, x, fill width)]
    (9, 0, [('J', 1, 5), ('O', 7, 5), ('Y', 13, 6), ('P', 20, 5), ('A', 26, 5), ('D', 32, 5), ('O', 44, 5), ('N', 50, 6)]),
    (11, 0, [('A', 4, 5), ('-', 12, 3), ('D', 19, 5), ('R', 25, 5), ('A', 31, 5), ('W', 37, 8), ('B', 50, 5), ('L', 56, 5), ('S', 68, 5), ('T', 74, 6), ('E', 81, 5)]),
    (11, 8, [('C', 38, 5)]),
    (11, 16, [('/', 55, 7)]),
    (17, 0, [('S', 0, 5), ('R', 19, 5)]),
    (18, 0, [('M', 0, 6)]),
]

def value_font(d, tiles):
    f = Font(space=6)
    for fr, r0, lst in VALUE_SRC:
        g, _ = frame_image(d, tiles, fr)
        for ch, x, w in lst:
            if ch in f.g: continue
            f.add_mask(ch, g[r0:r0 + 8, x:x + w + 1], 14)
    return f

ACUTE = ['..##', '.##.', '##..']
TILDE = ['.##..#', '#..##.']

def accented(font, base, acc, mark, ox):
    """glyph `base` with the mark art above it (mark rows + shadow, no gap), the mark `ox` columns from the left of the letter"""
    b = font.g[base]; a = art(mark)
    m = np.zeros((a.shape[0] + b.shape[0], b.shape[1]), dtype=np.uint8)
    m[a.shape[0]:, :] = b
    sub = m[:a.shape[0], ox:ox + a.shape[1]]
    sub[:] = np.where(a[:, :sub.shape[1]] > 0, a[:, :sub.shape[1]], sub)
    font.g[acc] = m


def add_drawn(lf, vf):
    """letters and marks the originals do not have, in the stroke vocabulary of each font"""
    lf.g['P'] = art(['#####', '##..#', '##..#', '#####', '##...', '##...', '##...'])
    lf.g['I'] = lf.g['I']
    vf.g['I'] = art(['##'] * 7)
    vf.g['.'] = art(['..'] * 5 + ['##', '##'])
    for base, acc in (('O', '\u00d3'), ('E', '\u00c9')):
        accented(vf, base, acc, ACUTE, max(0, (vf.fillw(base) - 3) // 2 + 1))
    lf.g['B'] = art(['#####', '##..#', '##..#', '#####', '##..#', '##..#', '#####'])
    accented(lf, 'O', '\u00d5', TILDE, 0)
    return lf, vf

# ------------------------------------------------------------------ texts (pt-BR)
T_TITLE = 'OP\u00c7\u00d5ES'
T_DEVICE, T_CONTROLS, T_SOUND, T_GUN, T_QUIT = 'ENTRADA', 'BOT\u00d5ES', 'SOM', 'AJUSTAR MIRA', 'SAIR'
V_DEVICE = ['S\u00d3 CONTROLE', 'CONTROLE E ARMA']      # JOYPAD ONLY, JOYPAD & GUN
V_SOUND = ['EST\u00c9REO', 'MONO']                       # STEREO, MONO
V_LINES = {'blaster': 'SACAR BLASTER', 'cancel': 'CANCELAR', 'select': 'SELEC./ATIRAR'}

# label rows on the plane: (text, x of the first fill pixel or None = centred on x_centre, y of the first row)
LABELS = [(T_DEVICE, 51, 52), (T_CONTROLS, 51, 88), (T_SOUND, 51, 124), (T_GUN, 51, 152), (T_QUIT, None, 179)]
OLD_BOX = {  # rectangle (x0, y0, x1, y1) of the original label text incl. shadow, erased before drawing
    T_DEVICE: (51, 52, 87, 60), T_CONTROLS: (51, 88, 106, 96), T_SOUND: (51, 124, 118, 132), T_GUN: (51, 152, 119, 160), T_QUIT: (113, 179, 142, 187),
}
QUIT_CENTRE = 127.0       # centre of the ink (fill + shadow) of QUIT in the button
BG = 5

def label_x(font, text, x):
    if x is not None: return x
    w = font.width(text) + 1          # incl. the shadow column
    return int(round(QUIT_CENTRE - w / 2.0))

def edit_labels(s, lf):
    im = s.im
    out = {}
    for text, x, y in LABELS:
        x0, y0, x1, y1 = OLD_BOX[text]
        im[y0:y1, x0:x1] = BG
        m, pos = lf.render(text)
        x = label_x(lf, text, x)
        ya = y - (m.shape[0] - 8)                       # accent rows above the 8 text rows
        sub = im[ya:ya + m.shape[0], x:x + m.shape[1]]
        sub[m == 2] = 8; sub[m == 1] = 4
        out[text] = (x, y, m)
    return out

def banner_extent(im0):
    """ink (colour 1) extent of the original OPTIONS lettering"""
    ys, xs = np.nonzero(im0[26:42, 40:216] == 1)
    return xs.min() + 40, xs.max() + 40, ys.min() + 26, ys.max() + 26

def header(s, text=T_TITLE):
    """banner: colour 3 bar on rows 30..38 (2 around), letters white (1) with a (+1, +1) shadow of colour 5, serif bold squeezed to the original lettering"""
    im = s.im
    x0, x1, y0, y1 = banner_extent(s.im0)
    f0 = Face(SERIF_BOLD, 15, 1.0)
    xs = (x1 - x0 + 1) / f0.layout('OPTIONS')[1]
    f = Face(SERIF_BOLD, 15, xs)
    # erase the old lettering: the bar (rows 30..38) and the rows above / below it that held serifs and shadow
    im[26:43, x0 - 3:x1 + 4] = 2
    im[30:39, x0 - 3:x1 + 4] = 3
    cov = np.zeros(im.shape, dtype=np.uint8)
    lay, total = f.layout(text)
    xc = (x0 + x1 + 1) / 2.0 + 0.5
    xl = int(round(xc - total / 2)); base = 39
    for ch, px in lay:
        g = f.glyph(ch)
        if g is None: continue
        a, l, t = g
        yy = base + t; xx = xl + px + l
        sub = cov[yy:yy + a.shape[0], xx:xx + a.shape[1]]
        np.maximum(sub, a[:sub.shape[0], :sub.shape[1]], out=sub)
    M = cov > 110
    S = np.roll(np.roll(M, 1, 0), 1, 1) & ~M
    im[S] = 5; im[M] = 1
    return (x0, x1, y0, y1), (xl, total)

def preview(s, path, scale=4):
    from PIL import Image
    rgb = s.rgb()
    Image.fromarray(rgb).resize((rgb.shape[1] * scale, rgb.shape[0] * scale), Image.NEAREST).save(path)

def show(f, text):
    m, pos = f.render(text)
    print('%-18s width %d' % (text, f.width(text)))
    for r in m: print(''.join('.#s'[v] for v in r))

# ------------------------------------------------------------------ sprites
YELLOW, WHITE, BLUE, SHADOW = 12, 14, 11, 8
ATTR = 2 << 13                                   # palette line 2
FIRST_NEW_TILE = 221                             # the sprite blob holds VRAM tiles 1..220

# screen position of the object behind each frame (script ops 0x84 / 0x85 of the objects 32..36 and the scene's object table)
OBJ_POS = {5: (40, 40), 6: (40, 72), 8: (40, 112), 30: (40, 136), 7: (96, 168), 29: (48, 152)}
for _f in (9, 10, 19, 20): OBJ_POS[_f] = (128, 48)
for _f in list(range(11, 17)) + list(range(21, 27)): OBJ_POS[_f] = (112, 80)
for _f in (17, 18, 27, 28): OBJ_POS[_f] = (168, 120)

class Sheet:
    """new sprite tiles: they take the VRAM tile ids the redrawn pieces freed (tiles 1..220 of the original sheet), then ids appended after tile 220"""
    def __init__(self, free):
        self.free = sorted(free); self.tiles = {}; self.next = FIRST_NEW_TILE
    def alloc(self, k):
        f = self.free
        for i in range(len(f) - k + 1):
            if f[i + k - 1] - f[i] == k - 1:
                run = f[i:i + k]; del f[i:i + k]; return run[0]
        t = self.next; self.next += k; return t
    def strip(self, img, px, py):
        """img: 8-row index image; returns sprite pieces (y, size, attr, x) at (px, py) of the frame (columns of at most 4 tiles)"""
        w = img.shape[1]; n = -(-w // 8)
        pad = np.zeros((8, n * 8), dtype=np.uint8); pad[:, :w] = img
        pcs = []; c = 0
        while c < n:
            k = min(4, n - c)
            t0 = self.alloc(k)
            for j in range(k): self.tiles[t0 + j] = pad[:, (c + j) * 8:(c + j) * 8 + 8].copy()
            pcs.append((py, (k - 1) << 2, ATTR | t0, px + 8 * c)); c += k
        return pcs

def colour(mask, fill, shadow=SHADOW, with_shadow=True):
    out = np.zeros(mask.shape, dtype=np.uint8)
    if with_shadow: out[mask == 2] = shadow
    out[mask == 1] = fill
    return out

def text_pieces(sheet, font, text, x_screen, y_screen, obj, fill, with_shadow=True):
    """pieces of one line of text whose first fill pixel is at (x_screen, y_screen) (first row of the 8-row text); accents go into a piece above"""
    m, pos = font.render(text)
    ox, oy = obj
    img = colour(m, fill, with_shadow=with_shadow)
    ar = img.shape[0] - 8                       # rows of accent above the text rows
    pcs = sheet.strip(img[ar:], x_screen - ox, y_screen - oy)
    if ar:
        top = img[:ar]
        # accent rows go to the bottom of the 8-row tiles above the text row
        full = np.zeros((8, img.shape[1]), dtype=np.uint8); full[8 - ar:] = top
        for c in range(-(-img.shape[1] // 8)):
            tile = full[:, c * 8:c * 8 + 8]
            if tile.any(): pcs += sheet.strip(tile, x_screen - ox + 8 * c, y_screen - oy - 8)
    return pcs

# tile ids (attr & 0x7FF) of the first piece of each controls line in the originals -> kind of line (white frames 11..16, blue frames 21..26)
LINE_KIND = {a & 0x7FF: k for a, k in ((16494, 'blaster'), (16504, 'cancel'), (16509, 'select'), (16553, 'blaster'), (16563, 'cancel'), (16568, 'select'))}
DEVICE_X, SOUND_X_END, LINE_X = 114, 205, 112 + 18      # first fill pixel of the device values / last of the sound values / first of the control lines

def is_text(fr, p, ty):
    y, size, attr, x = p
    return y == ty and (size & 3) == 0 and (size >> 2) <= 3

def kept_pieces(orig):
    """pieces of the originals that stay: box outlines of the selected rows, the A - / B - / C - prefixes of the controls frames"""
    keep = {}
    for fr, ty, ntext in ((5, 12, 2), (6, 16, 2), (8, 12, 3), (30, 16, 3), (7, 11, 1)):
        k = [p for p in orig[fr] if not is_text(fr, p, ty)]
        assert len(k) == len(orig[fr]) - ntext, (fr, len(k))
        keep[fr] = k
    for fr in list(range(11, 17)) + list(range(21, 27)):
        k = [p for p in orig[fr] if p[3] == 0]
        assert len(k) == 3, (fr, k)
        keep[fr] = k
    return keep

def tile_ids(p):
    grid, w, h = piece_tiles(p[1], p[2])
    return {t for row in grid for t in row}

def build_frames(d, lf, vf, label_pos):
    """-> {frame index: new piece list}, the sheet of new tiles"""
    orig = {i: d.frame(ANIM, i) for i in range(31)}
    keep = kept_pieces(orig)
    used = set()
    for pcs in keep.values():
        for p in pcs: used |= tile_ids(p)
    sh = Sheet(set(range(1, 221)) - used)
    new = {}
    # yellow selected rows: keep the outline pieces, redraw the label (frame 30 keeps the shadow of the original)
    for fr, text in ((5, T_DEVICE), (6, T_CONTROLS), (8, T_SOUND), (30, T_GUN), (7, T_QUIT)):
        x, y, _ = label_pos[text]
        new[fr] = list(keep[fr]) + text_pieces(sh, lf, text, x, y, OBJ_POS[fr], YELLOW, with_shadow=(fr == 30))
    # frame 29: GUN ADJUST enabled (white)
    x, y, _ = label_pos[T_GUN]
    new[29] = text_pieces(sh, lf, T_GUN, x, y, OBJ_POS[29], WHITE)
    # device values (white 9 10 / blue 19 20), text rows 52..59
    for fr, text, col in ((9, V_DEVICE[0], WHITE), (10, V_DEVICE[1], WHITE), (19, V_DEVICE[0], BLUE), (20, V_DEVICE[1], BLUE)):
        new[fr] = text_pieces(sh, vf, text, DEVICE_X, 52, OBJ_POS[fr], col)
    # sound values (white 17 18 / blue 27 28), text rows 124..131
    for fr, text, col in ((17, V_SOUND[0], WHITE), (18, V_SOUND[1], WHITE), (27, V_SOUND[0], BLUE), (28, V_SOUND[1], BLUE)):
        x = SOUND_X_END - vf.width(V_SOUND[0])
        new[fr] = text_pieces(sh, vf, text, x, 124, OBJ_POS[fr], col)
    # controls: the prefix pieces stay, the lines are redrawn; one set of tiles per (kind, colour) shared by the frames
    cache = {}
    for fr in list(range(11, 17)) + list(range(21, 27)):
        col = WHITE if fr < 20 else BLUE
        pcs = list(keep[fr])
        for y in (0, 8, 16):
            line = [p for p in orig[fr] if p[0] == y and p[3] >= 16]
            kind = LINE_KIND[[p for p in line if p[3] == 16][0][2] & 0x7FF]
            key = (kind, col)
            if key not in cache:
                cache[key] = text_pieces(sh, vf, V_LINES[kind], LINE_X, 80, OBJ_POS[fr], col)   # first row at piece y 0, moved to y below
            pcs += [(y + p[0], p[1], p[2], p[3]) for p in cache[key]]
        new[fr] = pcs
    return new, sh

def frame_bytes(pieces):
    b = (len(pieces) - 1).to_bytes(2, 'big')
    for (y, size4, attr, x) in pieces:
        b += (y & 0xFFFF).to_bytes(2, 'big') + (size4 << 8).to_bytes(2, 'big') + attr.to_bytes(2, 'big') + (x & 0xFFFF).to_bytes(2, 'big')
    return b

def prepare():
    s, d, vram, cram = build()
    lf = label_font(s.im0); vf = value_font(d, s.tiles); add_drawn(lf, vf)
    s.exclude = set(range(0, 512))
    label_pos = edit_labels(s, lf)
    header(s)
    frames, sh = build_frames(d, lf, vf, label_pos)
    return s, d, frames, sh

def apply(P):
    s, d, frames, sh = prepare()
    print('options plane', s.commit(P, FILE, 'OPTIONS screen', grow=(BLOB_PLANE, 0xD000, 17)))
    ScreenEdit.flush(P, FILE, 'OPTIONS screen (plane): OPCOES banner, ENTRADA / CONTROLES / SOM / AJUSTAR MIRA / SAIR labels')
    orig = d.blob(BLOB_SPR)
    assert len(orig) == 220 * 32
    top = max(list(sh.tiles) + [220])
    new = bytearray(orig) + bytes(32 * (top - 220))
    assert len(new) <= 0x2000, 'sprite tiles: %d tiles' % (len(new) // 32)
    for t, im in sh.tiles.items(): new[(t - 1) * 32:t * 32] = tiles_to_bytes(im[None])
    new = bytes(new)
    P.blob(FILE, BLOB_SPR, new, 'OPTIONS screen sprites: %d redrawn tiles (labels, values, controls lines), sheet grown to %d tiles' % (len(sh.tiles), top))
    tilech_len(P, FILE, BLOB_SPR, len(new))
    note = 'OPTIONS screen: redrawn frames of the option objects'
    for fr, pcs in sorted(frames.items()):
        addr = P.append(FILE, frame_bytes(pcs), note)
        P.raw(FILE, ANIM - BASE + 2 * fr, ((addr - ANIM) & 0xFFFF).to_bytes(2, 'big'), 'OPTIONS screen: frame offset table')
    print('options sprites: %d tiles, sheet %d, %d frames' % (len(sh.tiles), top, len(frames)))

if __name__ == '__main__':
    s, d, frames, sh = prepare()
    preview(s, os.path.join(ROOT, 'work/i18n/gfx/shots/options/plane_new.png'))
    print('tiles', len(sh.tiles), 'max id', max(sh.tiles), 'frames', sorted(frames))
