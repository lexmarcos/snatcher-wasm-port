"""DATA_xx.BIN reader for the picture work: LZKN1, scene chunks, objects, frames. Reads assets/raw/files (user's own data)."""
import os, struct
import numpy as np

RAW = os.path.join(os.path.dirname(__file__), '..', '..', 'assets', 'raw', 'files')
BASE = 0x28000

def lz_decode(src):
    size = (src[0] << 8) | src[1]
    i = 2; out = bytearray(); bits = 0; left = 0
    while True:
        if left == 0:
            bits = src[i]; i += 1; left = 8
        flag = bits & 1; bits >>= 1; left -= 1
        if flag == 0:
            out.append(src[i]); i += 1; continue
        c = src[i]; i += 1
        if c & 0x80 == 0:
            if c == 0x1F: return bytes(out), i
            lo = src[i]; i += 1
            dist = ((c & 0x60) << 3) | lo; cnt = (c & 0x1F) + 3
            for _ in range(cnt): out.append(out[-dist])
        elif c & 0x40 == 0:
            dist = c & 0x0F; cnt = (c >> 4) - 8 + 2
            for _ in range(cnt): out.append(out[-dist])
        else:
            cnt = c - 0xB8
            out += src[i:i + cnt]; i += cnt

def fnv64(b):
    h = 0xcbf29ce484222325
    for x in b:
        h ^= x; h = (h * 0x100000001b3) & 0xFFFFFFFFFFFFFFFF
    return h

class Data:
    def __init__(self, name, raw=None):
        self.name = name
        self.b = open(os.path.join(raw or RAW, name if name.endswith('.BIN') else name + '.BIN'), 'rb').read()
        self.table = self.u32(4)
    def u8(self, a): return self.b[a - BASE]
    def u16(self, a): return (self.b[a - BASE] << 8) | self.b[a - BASE + 1]
    def i16(self, a):
        v = self.u16(a); return v - 65536 if v >= 32768 else v
    def u32(self, off_or_addr):
        o = off_or_addr if off_or_addr < BASE else off_or_addr - BASE
        return struct.unpack('>I', self.b[o:o + 4])[0]
    def blob(self, addr):
        return lz_decode(self.b[addr - BASE:])[0]
    def nscenes(self):
        mn = 1 << 30; i = 0
        while 2 * i < mn and i < 1024:
            o = self.u16(self.table + 2 * i)
            if o < 2 * (i + 1): break
            mn = min(mn, o); i += 1
        return i
    def scene_addr(self, n): return self.table + self.u16(self.table + 2 * n)
    def chunks(self, n):
        p = self.scene_addr(n); out = []
        for _ in range(64):
            ty = self.u8(p)
            if ty & 0x80: break
            ln = self.u8(p + 1); pl = p + 2; end = pl + ln
            if ty == 0:
                frames = []; q = pl + 4
                while self.u16(q) != 0xFFFF and len(frames) < 256:
                    frames.append(self.u32(q) & 0xFFFFFF); q += 4
                out.append(('pal', dict(kind=self.u8(pl), delay=self.u8(pl + 1), off=self.u8(pl + 2), count=self.u8(pl + 3) + 1, frames=frames, addr=pl)))
            elif ty == 1:
                v = []; q = pl
                while self.u16(q) != 0xFFFF and len(v) < 256:
                    v.append(dict(src=self.u32(q) & 0xFFFFFF, delay=self.u16(q + 4), dst=self.u16(q + 6))); q += 8
                out.append(('vram', v))
            elif ty == 2:
                v = []; q = pl
                while q + 12 <= end:
                    rel = self.i16(q + 10)
                    v.append(dict(slot=self.u8(q), flags=self.u8(q + 1), x=self.i16(q + 2), y=self.i16(q + 4), anim=self.u32(q + 6) & 0xFFFFFF, script=q + 12 + rel)); q += 12
                out.append(('obj', v))
            else:
                out.append(('t%d' % ty, self.b[pl - BASE:end - BASE]))
            p = end
        return out
    def palette(self, addr):
        """frame pointer of a palette descriptor -> list of CRAM words"""
        n = 64
        return [self.u16(addr + 2 * i) for i in range(n)]
    def frame(self, anim, idx):
        """frame idx of an animation table -> list of pieces (y, size, attr, x)"""
        f = anim + self.u16(anim + 2 * idx)
        cnt = self.u16(f) + 1
        out = []
        for i in range(cnt):
            q = f + 2 + 8 * i
            out.append((self.i16(q), self.u16(q + 2) >> 8, self.u16(q + 4), self.i16(q + 6)))
        return out
    def script(self, addr, n=64):
        out = []
        for i in range(n):
            q = addr + 4 * i
            out.append(tuple(self.u8(q + k) for k in range(4)))
            if out[-1][0] == 0x80: break
        return out
