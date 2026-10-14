"""Experiment (superseded by core/src/gfx.rs + `snt gfx-export`): rebuild VRAM/CRAM for a DATA scene. NOTE: in-game planes are 32 cells wide (see docs/re/graphics.md §3); pass w=32."""
import sys, struct
sys.path.insert(0, '/home/suel/Documents/snatcher/re/py')
from gfx_lzkn import lzkn1
from gfx_scenes import scenes, chunks, be16, be32
from PIL import Image

def color(w):
    r = (w >> 1) & 7; g = (w >> 5) & 7; b = (w >> 9) & 7
    return (r * 255 // 7, g * 255 // 7, b * 255 // 7)

def build(d, n, vram=None, cram=None):
    vram = vram or bytearray(0x10000); cram = cram or [0] * 64
    t, offs = scenes(d)
    for ty, p, pl in chunks(d, t + offs[n]):
        if ty == 0:
            kind, _, off, cnt = pl[0], pl[1], pl[2], pl[3]
            if kind in (1, 2, 6, 9):
                ptr = be32(pl, 4) & 0xffffff
                src = ptr - 0x28000
                for i in range(cnt + 1):
                    if (off & 0x7f) // 2 + i < 64:
                        cram[(off & 0x7f) // 2 + i] = be16(d, src + 2 * i)
        elif ty == 1:
            for i in range(0, len(pl) - 1, 8):
                if pl[i:i+2] == b'\xff\xff': break
                src = be32(pl, i) & 0xffffff; dst = be16(pl, i + 6)
                out, sz, end = lzkn1(d, src - 0x28000)
                vram[dst:dst+len(out)] = out[:0x10000-dst]
    return vram, cram

def plane(vram, cram, base, w=64, rows=32, cols=32):
    im = Image.new('RGBA', (cols * 8, rows * 8), (0, 0, 0, 0))
    px = im.load()
    for cy in range(rows):
        for cx in range(cols):
            e = be16(vram, base + 2 * (cy * w + cx))
            tile = e & 0x7ff; pal = (e >> 13) & 3; vf = e & 0x1000; hf = e & 0x800
            for y in range(8):
                for x in range(8):
                    sx = 7 - x if hf else x; sy = 7 - y if vf else y
                    b = vram[(tile * 32 + sy * 4 + sx // 2) & 0xffff]
                    c = (b >> 4) if sx % 2 == 0 else (b & 15)
                    if c:
                        px[cx * 8 + x, cy * 8 + y] = color(cram[pal * 16 + c]) + (255,)
    return im

if __name__ == '__main__':
    d = open(sys.argv[1], 'rb').read(); n = int(sys.argv[2])
    vram, cram = build(d, n)
    b = plane(vram, cram, 0xC000); a = plane(vram, cram, 0xE000)
    bg = Image.new('RGBA', b.size, color(cram[0]) + (255,))
    bg.alpha_composite(b); bg.alpha_composite(a)
    bg.save(sys.argv[3])
    print('cram', ' '.join('%03x' % c for c in cram))

def objects(d, n):
    """Type-2 object spawns of scene n: (slot, flags, x, y, anim_table_addr, script_addr)."""
    t, offs = scenes(d)
    res = []
    for ty, p, pl in chunks(d, t + offs[n]):
        if ty != 2: continue
        q = 0
        while q + 12 <= len(pl):
            slot, fl = pl[q], pl[q+1]; x = be16(pl, q+2); y = be16(pl, q+4)
            at = be32(pl, q+6) & 0xffffff; rel = be16(pl, q+10)
            rel = rel - 0x10000 if rel & 0x8000 else rel
            script = 0x28000 + p + q + 12 + rel
            res.append((slot, fl, x, y, at, script)); q += 12
    return res

def first_frame(d, at, script):
    s = script - 0x28000
    while d[s] >= 0x80:
        if d[s] in (0xA1, 0xA7): s += 4; continue
        return None
    fr = d[s]
    tbl = at - 0x28000
    return tbl + be16(d, tbl + 2 * fr)

def draw_sprites(im, d, vram, cram, objs, xoff=0, yoff=0):
    px = im.load()
    for (slot, fl, x, y, at, script) in sorted(objs, key=lambda o: -o[0]):
        if fl & 2: continue
        f = first_frame(d, at, script)
        if f is None: continue
        cnt = be16(d, f) + 1; q = f + 2
        pieces = []
        for i in range(cnt):
            yo = struct.unpack('>h', d[q:q+2])[0]; sz = be16(d, q+2); attr = be16(d, q+4); xo = struct.unpack('>h', d[q+6:q+8])[0]
            q += 8; pieces.append((yo, sz, attr, xo))
        for (yo, sz, attr, xo) in reversed(pieces):
            w = ((sz >> 10) & 3) + 1; h = ((sz >> 8) & 3) + 1
            tile = attr & 0x7ff; pal = (attr >> 13) & 3; hf = attr & 0x800; vf = attr & 0x1000
            for cx in range(w):
                for cy in range(h):
                    tc = tile + cx * h + cy
                    for yy in range(8):
                        for xx in range(8):
                            b = vram[(tc * 32 + yy * 4 + xx // 2) & 0xffff]
                            c = (b >> 4) if xx % 2 == 0 else (b & 15)
                            if not c: continue
                            dx = (w - 1 - cx) * 8 + 7 - xx if hf else cx * 8 + xx
                            dy = (h - 1 - cy) * 8 + 7 - yy if vf else cy * 8 + yy
                            X = x + xo + dx + xoff; Y = y + yo + dy + yoff
                            if 0 <= X < im.width and 0 <= Y < im.height:
                                px[X, Y] = color(cram[pal * 16 + c]) + (255,)
