"""Screens built by tile channels: which blob and offset every VRAM byte of the picture came from (from the engine's GFXTRACE log), a VRAM / CRAM dump of the
finished screen, and an editor that patches the blobs behind the cells (tiles and nametable entries).  Used for the CONTINUE slot screen and the options screen."""
import sys, os, re, subprocess, collections
sys.path.insert(0, os.path.dirname(__file__))
from lib import *; from data import *

ROOT = os.path.join(os.path.dirname(__file__), '..', '..')
RUN = os.path.join(ROOT, 'target', 'release', 'examples', 'run')

def run_engine(frames, inputs, jump, out, env=None, saves=None):
    """run the engine natively (English), return (log lines, vram bytes, cram words)"""
    e = dict(os.environ); e.update(env or {})
    vr = os.path.join(out, 'vram.bin'); cr = os.path.join(out, 'cram.bin')
    os.makedirs(out, exist_ok=True)
    e.update(GFXTRACE='1', VRAMDUMP=vr, CRAMDUMP=cr)
    if saves: e['SAVES'] = saves
    r = subprocess.run([RUN, str(frames), '100000', out, inputs, jump], env=e, capture_output=True, text=True, cwd=ROOT)
    return r.stdout.splitlines(), open(vr, 'rb').read(), load_cram(cr)

class VramMap:
    def __init__(self, log, until=None):
        self.src = {}                       # vram byte address -> (blob src, offset in the decoded blob)
        self.blobs = {}                     # blob src -> length
        pending = None; buf_blob = {}
        for l in log:
            m = re.match(r'\s*(\d+) gfx (Blob|VramUp|TileSend) \{(.*)\}', l)
            if not m: continue
            fr = int(m.group(1))
            if until is not None and fr > until: break
            kind, body = m.group(2), m.group(3)
            f = dict(re.findall(r'(\w+): (\w+)', body))
            if kind == 'Blob':
                pending = (int(f['src'], 16), int(f['len'], 16)); self.blobs[pending[0]] = pending[1]
            elif kind == 'VramUp':
                s, n = pending; dst = int(f['dst'], 16)
                for i in range(n): self.src[dst + i] = (s, i)
                for b in range(4): buf_blob[b] = pending if b == 0 else buf_blob.get(b)
            else:
                b = int(f['buf']); 
                if pending is not None: buf_blob[b] = pending; pending = None
                if buf_blob.get(b) is None: continue
                s, n = buf_blob[b]
                v = int(f['vram'], 16); off = int(f['off'], 16); ln = int(f['len'], 16)
                for i in range(ln): self.src[v + i] = (s, off + i)

class Screen:
    """plane A (and B) nametables + tiles of a finished screen, editable"""
    def __init__(self, d, vram, cram, vmap, plane_a=0xE000, plane_b=None, w=32, rows=28):
        self.d = d; self.vram = bytearray(vram); self.vmap = vmap; self.w = w; self.rows = rows
        self.pa = plane_a; self.pb = plane_b
        self.tiles = tiles_from_bytes(bytes(vram[:0x10000]))
        self.cram = cram[0]
        self.pal = [pal_slot(cram[0], i) for i in range(4)]
        self.im, self.pl, self.prio = self.compose()
        self.im0 = self.im.copy()
    def entry(self, base, cx, cy):
        o = base + (cy * self.w + cx) * 2
        return (self.vram[o] << 8) | self.vram[o + 1]
    def compose(self):
        im = np.zeros((self.rows * 8, self.w * 8), dtype=np.uint8); pl = np.zeros_like(im); pr = np.zeros(im.shape, dtype=bool)
        for base in [b for b in (self.pb, self.pa) if b is not None]:
            for cy in range(self.rows):
                for cx in range(self.w):
                    e = self.entry(base, cx, cy)
                    t = self.tiles[e & 0x7FF].copy()
                    if e & 0x800: t = t[:, ::-1]
                    if e & 0x1000: t = t[::-1, :]
                    sl = (slice(cy * 8, cy * 8 + 8), slice(cx * 8, cx * 8 + 8))
                    m = t > 0
                    im[sl][m] = t[m]; pl[sl][m] = (e >> 13) & 3
        return im, pl, pr
    def rgb(self, im=None):
        im = self.im if im is None else im
        out = np.zeros(im.shape + (3,), dtype=np.uint8)
        for (y, x), v in np.ndenumerate(im): out[y, x] = self.pal[self.pl[y, x]][v] if v else (0, 0, 0)
        return out


class ScreenEdit(Screen):
    exclude = set()     # tile ids used by sprites or other states: never reused
    STATE = {'blobs': {}, 'grown': {}}   # shared by every state of a screen that is edited: patched blob bytes, tiles appended per blob
    def cell_content(self, im, cx, cy): return im[cy * 8:cy * 8 + 8, cx * 8:cx * 8 + 8]
    def commit(self, P, file, note='', grow=None):
        """grow = (blob src, vram dst of that blob, max tiles): new tiles that fit no freed id are appended to this blob (its tile-channel length is patched)"""
        vm = self.vmap
        changed = [(cx, cy) for cy in range(self.rows) for cx in range(self.w)
                   if not (self.cell_content(self.im, cx, cy) == self.cell_content(self.im0, cx, cy)).all()]
        ch_set = set(changed)
        used_unch = set(self.exclude); used_ch = set()
        for cy in range(self.rows):
            for cx in range(self.w):
                t = self.entry(self.pa, cx, cy) & 0x7FF
                (used_ch if (cx, cy) in ch_set else used_unch).add(t)
                if self.pb is not None: used_unch.add(self.entry(self.pb, cx, cy) & 0x7FF)
        free = sorted(used_ch - used_unch)
        content = {}
        for t in sorted(used_unch): content.setdefault(bytes(self.vram[t * 32:t * 32 + 32]), t)
        new_tiles = {}; entries = {}
        if grow:
            gsrc, gdst, gmax = grow; gend = gdst // 32 + vm.blobs[gsrc] // 32; gnext = gend + self.STATE['grown'].get(gsrc, 0); gend0 = gend; gend = gnext
        for (cx, cy) in changed:
            data = tiles_to_bytes(self.cell_content(self.im, cx, cy)[None])
            tid = content.get(data)
            if tid is None:
                if free: tid = free.pop(0)
                else:
                    assert grow and gnext < gend + gmax, 'out of tiles: %d changed cells, no free tile left' % len(changed)
                    tid = gnext; gnext += 1
                new_tiles[tid] = data; content[data] = tid
            e = self.entry(self.pa, cx, cy)
            entries[(cx, cy)] = (e & ~0x7FF & ~0x1800 & 0xFFFF) | tid
        blobs = self.STATE['blobs']
        if grow and gnext > gend:
            b = blobs.setdefault(gsrc, bytearray(self.d.blob(gsrc))); b += bytes(max(0, (gnext - gend0) * 32 - (len(b) - vm.blobs[gsrc])))
            for k in range(gend0, gnext):
                for i in range(32): vm.src[k * 32 + i] = (gsrc, (k - gdst // 32) * 32 + i)
            self.STATE['grown'][gsrc] = gnext - gend0
        def patch(addr, data):
            for i, b in enumerate(data):
                s = vm.src.get(addr + i)
                assert s is not None, 'VRAM %#x is not written by a blob' % (addr + i)
                blobs.setdefault(s[0], bytearray(self.d.blob(s[0])))[s[1]] = b
        for t, data in new_tiles.items(): patch(t * 32, data)
        for (cx, cy), e in entries.items(): patch(self.pa + (cy * self.w + cx) * 2, bytes([e >> 8, e & 0xFF]))
        return dict(changed=len(changed), new=len(new_tiles), free_left=len(free))

    @classmethod
    def flush(cls, P, file, note=''):
        for src, b in cls.STATE['blobs'].items(): P.blob(file, src, bytes(b), note)
        for src, n in cls.STATE['grown'].items(): tilech_len(P, file, src, P.data(file).blob(src).__len__() + 32 * n)
        cls.STATE['blobs'].clear(); cls.STATE['grown'].clear()


def tilech_len(P, file, src, new_len):
    """tile-channel entries [src:4][vram:2][off:2][len:2] that send a blob: the length is the number of bytes sent, so a grown blob needs a longer entry"""
    d = P.data(file); pat = (src).to_bytes(4, 'big'); i = 0; n = 0
    while True:
        i = d.b.find(pat, i)
        if i < 0: break
        ln = (d.b[i + 8] << 8) | d.b[i + 9]
        if ln == len(d.blob(src)):          # sends exactly the original blob
            P.raw(file, i + 8, new_len.to_bytes(2, 'big'), 'tile channel length of blob %#x grown to %d' % (src, new_len)); n += 1
        i += 1
    assert n, 'no tile-channel entry of blob %#x found' % src
    return n
