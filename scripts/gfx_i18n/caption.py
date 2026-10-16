"""Location caption of the first scene of Act 1 (DATA_H15, objects 21/22/29, anim table 0x2DEAC frames 8..43): "Konami Omni Building / Junker Headquarters"
-> "Edificio Konami Omni / Quartel-General Junker", typed letter by letter like the original.

The original draws the text with sprite pieces: 32x16 slices of the finished line for the words already typed and small per-letter pieces for the
letters of the slice being typed (16 sprites per line at most). Same scheme here: the text is Liberation Serif (Times metrics, squeezed to the original
advance widths, grey levels fitted to the original), slices and letter pieces get tiles in the caption's tile range plus free VRAM after the caption
blobs (the blob at VRAM 0x8000 grows by the tiles 1191..1279), the 36 new frames are appended to the DATA file and the frame offset table and the
typing delays of the script are patched."""
import sys, os, math
sys.path.insert(0, os.path.dirname(__file__))
import numpy as np
from lib import tiles_to_bytes
from data import Data, BASE
from textfont import Face, SERIF, quantize_caption

FILE = 'DATA_H15.BIN'
ANIM = 0x2DEAC
SCRIPT = 0x2823A          # first typing step of the script shared by objects 21, 22 and 29 (8 bytes per step: frame entry + sound op)
BLOB_8000 = 0x2D62A       # tiles 1024..1190 (+ new 1191..1279)
BLOB_A000 = 0x2E93C       # tiles 1280..1501 (+ new 1502..1510)
ATTR = 3 << 13            # palette line 3
BASE_Y = 11               # baseline row inside a 16 row line

LINES = [  # text, x0 (relative to the object), y of the line
    ('Edifício Konami Omni', 2, 0),
    ('Quartel-General Junker', 0, 16),
]
N_STEPS = 18              # typing steps per line (the script has 36 frame entries)

face = Face(SERIF, 16, 0.93)

def glyph_idx(ch):
    g = face.glyph(ch)
    if g is None: return None
    cov, left, top = g
    a = quantize_caption(cov)
    return a, left, top

def line_layout(text, x0):
    lay, total = face.layout(text)
    letters = []
    for ch, px in lay:
        if ch == ' ': continue
        g = glyph_idx(ch)
        if g is None:
            letters.append(dict(ch=ch, x=None)); continue
        a, left, top = g
        y = BASE_Y + top
        # clip to the 16 rows of the line
        if y < 0: a = a[-y:]; y = 0
        if y + a.shape[0] > 16: a = a[:16 - y]
        letters.append(dict(ch=ch, x=x0 + px + left, y=y, a=a, w=a.shape[1]))
    return letters, total

def cover_image(letters, xmin, width):
    cov = np.zeros((16, width), dtype=np.uint8)
    for L in letters:
        if L['x'] is None: continue
        # coverage-composite by max of the quantized grey order (7 brightest)
        rank = {0: 0, 11: 1, 10: 2, 9: 3, 7: 4}
        inv = {v: k for k, v in rank.items()}
        sub = cov[L['y']:L['y'] + L['a'].shape[0], L['x'] - xmin:L['x'] - xmin + L['w']]
        r = np.vectorize(rank.get)(L['a']); r0 = np.vectorize(rank.get)(sub)
        sub[:] = np.vectorize(inv.get)(np.maximum(r, r0))
    return cov

class Tiles:
    def __init__(self):
        free = [(1406, 1501), (1502, 1510), (1191, 1279)]
        self.free = [list(r) for r in free]
        self.mem = {}      # tile id -> 32 bytes
        self.dedupe = {}
    def alloc(self, n):
        for r in self.free:
            if r[1] - r[0] + 1 >= n:
                t = r[0]; r[0] += n; return t
        raise RuntimeError('out of tiles (%d wanted)' % n)
    def piece(self, img, cols):
        """img: (16, cols*8) colour indices -> first tile id of a cols x 2 piece (column-major numbering), deduplicated"""
        key = img.tobytes()
        if key in self.dedupe: return self.dedupe[key]
        t0 = self.alloc(cols * 2)
        for c in range(cols):
            for r in range(2):
                self.mem[t0 + c * 2 + r] = tiles_to_bytes(img[r * 8:(r + 1) * 8, c * 8:(c + 1) * 8][None])
        self.dedupe[key] = t0
        return t0

def build_frames():
    T = Tiles()
    frames = []   # list of piece lists (y, size4, attr, x)
    steps_words = []   # for each of the 36 steps: True if the step ends a word that is not the last of its line
    done_pieces = []   # final slice pieces of the lines already typed
    for li, (text, x0, ly) in enumerate(LINES):
        letters, total = line_layout(text, x0)
        ink = [L for L in letters if L['x'] is not None]
        xmin = min(L['x'] for L in ink); xmax = max(L['x'] + L['w'] for L in ink)
        width = ((xmax - xmin + 7) // 8) * 8 + 8
        cov = cover_image(ink, xmin, width)
        # slices of 32 px (the last one as wide as needed)
        slices = []
        sx = xmin
        while sx < xmax:
            sw = min(32, ((xmax - sx + 7) // 8) * 8)
            slices.append((sx, sw)); sx += sw
        slice_piece = []
        for (sx, sw) in slices:
            img = cov[:, sx - xmin:sx - xmin + sw]
            if img.shape[1] < sw: img = np.pad(img, ((0, 0), (0, sw - img.shape[1])))
            t0 = T.piece(img, sw // 8)
            slice_piece.append((ly, (((sw // 8) - 1) << 2) | 1, ATTR | t0, sx))
        # letter pieces (unique glyph bitmaps)
        glyph_piece = {}
        for i, L in enumerate(ink):
            cols = (L['w'] + 7) // 8
            img = np.zeros((16, cols * 8), dtype=np.uint8)
            img[L['y']:L['y'] + L['a'].shape[0], :L['w']] = L['a']
            t0 = T.piece(img, cols)
            glyph_piece[i] = (ly, ((cols - 1) << 2) | 1, ATTR | t0, L['x'])
        # steps: letters revealed per step
        n = len(ink)
        bounds = [(i * n) // N_STEPS for i in range(N_STEPS + 1)]
        # word ends (indices in `ink`) : a space follows
        word_end = set(); k = 0
        for ch in text:
            if ch == ' ': word_end.add(k - 1)
            else: k += 1
        for s in range(N_STEPS):
            last = bounds[s + 1] - 1          # last typed letter index
            typed = range(0, last + 1)
            pieces = list(done_pieces)
            complete = []
            for si, (sx, sw) in enumerate(slices):
                inter = [i for i, L in enumerate(ink) if L['x'] < sx + sw and L['x'] + L['w'] > sx]
                if inter and max(inter) <= last: complete.append(si)
            pieces += [slice_piece[si] for si in complete]
            for i in typed:
                L = ink[i]
                if all(not (L['x'] < slices[si][0] + slices[si][1] and L['x'] + L['w'] > slices[si][0]) or si not in complete for si in range(len(slices))) \
                        or any((L['x'] < slices[si][0] + slices[si][1] and L['x'] + L['w'] > slices[si][0]) and si not in complete for si in range(len(slices))):
                    pieces.append(glyph_piece[i])
            frames.append(pieces)
            steps_words.append(any(b in word_end for b in range(bounds[s], bounds[s + 1])) and s != N_STEPS - 1)
        done_pieces = done_pieces + slice_piece
    return frames, steps_words, T

def frame_bytes(pieces):
    b = (len(pieces) - 1).to_bytes(2, 'big')
    for (y, size4, attr, x) in pieces:
        b += (y & 0xFFFF).to_bytes(2, 'big') + (size4 << 8).to_bytes(2, 'big') + attr.to_bytes(2, 'big') + (x & 0xFFFF).to_bytes(2, 'big')
    return b

def check_limits(frames):
    worst = 0
    for pieces in frames:
        for y in range(0, 32):
            c = sum(1 for (py, s4, a, x) in pieces if py <= y < py + 16)
            worst = max(worst, c)
    return worst, max(len(p) for p in frames)

def apply(P):
    frames, word_steps, T = build_frames()
    assert len(frames) == 36
    worst, maxp = check_limits(frames)
    print('caption: tiles used', len(T.mem), 'max sprites per scanline', worst, 'max pieces per frame', maxp)
    assert worst <= 12
    d = P.data(FILE)
    # frame data appended to the file
    addrs = []
    for pieces in frames:
        addrs.append(P.append(FILE, frame_bytes(pieces), 'location caption: 36 new typing frames (Edificio Konami Omni / Quartel-General Junker)'))
    # offset words of frames 8..43
    for j, a in enumerate(addrs):
        off = ANIM - BASE + 2 * (8 + j)
        P.raw(FILE, off, ((a - ANIM) & 0xFFFF).to_bytes(2, 'big'), 'caption frame offset table')
    # typing delays: 8 ticks per letter, 16 after a word, the pauses at the end of the lines stay (48 and 128)
    for k in range(36):
        if k in (17, 35): continue
        P.raw(FILE, SCRIPT - BASE + 8 * k + 3, bytes([16 if word_steps[k] else 8]), 'caption typing delays')
    # tiles
    orig80 = d.blob(BLOB_8000); origA0 = d.blob(BLOB_A000)
    n80 = bytearray(orig80); nA0 = bytearray(origA0)
    for t, b in T.mem.items():
        if 1024 <= t < 1280:
            o = (t - 1024) * 32
            if o + 32 > len(n80): n80 += bytes(o + 32 - len(n80))
            n80[o:o + 32] = b
        else:
            o = (t - 1280) * 32
            if o + 32 > len(nA0): nA0 += bytes(o + 32 - len(nA0))
            nA0[o:o + 32] = b
    P.blob(FILE, BLOB_8000, bytes(n80), 'location caption: letter tiles 1191..1279 (the blob grows)')
    P.blob(FILE, BLOB_A000, bytes(nA0), 'location caption: slice tiles 1406..1510')
