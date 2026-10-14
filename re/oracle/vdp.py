"""Mega Drive VDP model for the test oracle (ports, DMA, VRAM/CRAM/VSRAM, renderer).

Semantics follow public documentation (Sega Genesis Software Manual / Plutiedev VDP pages,
Genesis Plus GX vdp_ctrl.c for edge cases). Rendering is a whole-frame approximation using
the register state at the time of the screenshot (no mid-frame raster effects).
"""
import numpy as np


class VDP:
    def __init__(self, read16):
        self.read16 = read16            # callback(addr) -> word from 68k bus (for DMA)
        self.vram = bytearray(0x10000)
        self.cram = [0] * 64
        self.vsram = [0] * 40
        self.reg = [0] * 24
        self.addr = 0
        self.code = 0
        self.pending = False
        self.fill_pending = False
        self.vblank = False
        self.line = 0
        self.log = []                   # DMA log (for trace)
        self.read_buf = 0

    # ---------------- ports ----------------
    def status(self):
        s = 0x3400 | 0x0200           # FIFO empty, upper bits as seen on hardware
        if self.vblank:
            s |= 0x0008
        if self.line >= 224 or self.line < 0:
            s |= 0x0004               # hblank-ish (approximation)
        return s

    def hv(self):
        v = self.line & 0xFF
        return (v << 8) | 0x80

    def ctrl_write(self, w):
        if self.pending:
            self.pending = False
            self.addr = (self.addr & 0x3FFF) | ((w & 3) << 14)
            self.code = (self.code & 3) | ((w >> 2) & 0x3C)
            if self.code & 0x20 and self.reg[1] & 0x10:
                self.dma()
            return
        if (w & 0xC000) == 0x8000:
            r = (w >> 8) & 0x1F
            if r < 24:
                self.reg[r] = w & 0xFF
            self.code = 0
            return
        self.pending = True
        self.addr = (self.addr & 0xC000) | (w & 0x3FFF)
        self.code = (self.code & 0x3C) | ((w >> 14) & 3)

    def data_write(self, w):
        self.pending = False
        if self.fill_pending:
            self.fill_pending = False
            self.dma_fill(w)
            return
        self._write(w)
        self.addr = (self.addr + self.reg[15]) & 0xFFFF

    def _write(self, w):
        c = self.code & 0xF
        if c == 1:      # VRAM
            a = self.addr & 0xFFFE
            if self.addr & 1:
                w = ((w & 0xFF) << 8) | (w >> 8)
            self.vram[a] = w >> 8
            self.vram[a + 1] = w & 0xFF
        elif c == 3:    # CRAM
            self.cram[(self.addr >> 1) & 0x3F] = w & 0x0EEE
        elif c == 5:    # VSRAM
            i = (self.addr >> 1) & 0x3F
            if i < 40:
                self.vsram[i] = w & 0x7FF

    def data_read(self):
        self.pending = False
        c = self.code & 0xF
        if c == 0:
            a = self.addr & 0xFFFE
            v = (self.vram[a] << 8) | self.vram[a + 1]
        elif c == 8:
            v = self.cram[(self.addr >> 1) & 0x3F]
        elif c == 4:
            i = (self.addr >> 1) & 0x3F
            v = self.vsram[i] if i < 40 else 0
        else:
            v = 0
        self.addr = (self.addr + self.reg[15]) & 0xFFFF
        return v

    # ---------------- DMA ----------------
    def dma(self):
        mode = self.reg[23] >> 6
        length = self.reg[19] | (self.reg[20] << 8)
        if length == 0:
            length = 0x10000
        if mode < 2:   # 68k -> VDP
            src = ((self.reg[21] | (self.reg[22] << 8) | ((self.reg[23] & 0x7F) << 16)) << 1) & 0xFFFFFF
            self.log.append(('dma68k', src, self.addr, length, self.code & 0xF))
            # Sega CD quirk (HYPOTHESIS, per GPGX/Sega docs): DMA from Word RAM lags by one word,
            # games compensate by adding 2 to the source; emulate by reading from src-2.
            lag = 0x200000 <= src < 0x240000
            for i in range(length):
                a = src + 2 * i - (2 if lag else 0)
                self._write(self.read16(a & 0xFFFFFE))
                self.addr = (self.addr + self.reg[15]) & 0xFFFF
            self.reg[19] = self.reg[20] = 0
            src2 = (src >> 1) + length
            self.reg[21] = src2 & 0xFF
            self.reg[22] = (src2 >> 8) & 0xFF
        elif mode == 2:  # fill on next data write
            self.fill_pending = True
            self.fill_len = length
        else:            # VRAM copy
            src = self.reg[21] | (self.reg[22] << 8)
            self.log.append(('copy', src, self.addr, length))
            for i in range(length):
                self.vram[self.addr & 0xFFFF] = self.vram[(src + i) & 0xFFFF]
                self.addr = (self.addr + self.reg[15]) & 0xFFFF
            self.reg[19] = self.reg[20] = 0

    def dma_fill(self, w):
        length = self.fill_len
        self.log.append(('fill', self.addr, length, w))
        c = self.code & 0xF
        if c == 1:
            # first write is a normal word write, then high byte fills
            self._write(w)
            self.addr = (self.addr + self.reg[15]) & 0xFFFF
            for _ in range(length - 1):
                self.vram[(self.addr ^ 1) & 0xFFFF] = w >> 8
                self.addr = (self.addr + self.reg[15]) & 0xFFFF
        else:
            for _ in range(length):
                self._write(w)
                self.addr = (self.addr + self.reg[15]) & 0xFFFF
        self.reg[19] = self.reg[20] = 0

    # ---------------- rendering ----------------
    def palette_rgb(self):
        c = np.array(self.cram, dtype=np.int32)
        r = ((c >> 1) & 7) * 36
        g = ((c >> 5) & 7) * 36
        b = ((c >> 9) & 7) * 36
        return np.stack([r, g, b], 1).astype(np.uint8)

    # ---------------- per-line renderer ----------------
    def begin_frame(self):
        self.fb = np.zeros((240, 320, 3), np.uint8)
        self.fb_w = 256

    def _plane_line(self, vram, base, y, W, is_a, pw, ph):
        r = self.reg
        hs_base = (r[13] & 0x3F) << 10
        hmode = r[11] & 3
        if hmode == 0:
            hl = hs_base
        elif hmode == 2:
            hl = hs_base + (y & ~7) * 4
        else:
            hl = hs_base + y * 4
        hl &= 0xFFFF
        o = hl + (0 if is_a else 2)
        hsc = ((int(vram[o]) << 8) | int(vram[o + 1])) & 0x3FF
        xs = np.arange(W)
        px = (xs - hsc) % (pw * 8)
        if (r[11] >> 2) & 1:
            cell = ((xs // 16).clip(0, 19))
            vs = np.array([self.vsram[c * 2 + (0 if is_a else 1)] for c in range(20)])[cell] & 0x3FF
            py = (y + vs) % (ph * 8)
        else:
            py = np.full(W, (y + (self.vsram[0 if is_a else 1] & 0x3FF)) % (ph * 8))
        na = base + ((py // 8) * pw + px // 8) * 2
        ent = (vram[na & 0xFFFF].astype(np.int32) << 8) | vram[(na + 1) & 0xFFFF]
        return self._tile_pixels(vram, ent, px % 8, py % 8)

    def _tile_pixels(self, vram, ent, tx, ty):
        tile = ent & 0x7FF
        tx = np.where(ent & 0x800, 7 - tx, tx)
        ty = np.where(ent & 0x1000, 7 - ty, ty)
        addr = tile * 32 + ty * 4 + (tx >> 1)
        b = vram[addr & 0xFFFF]
        ci = np.where(tx & 1, b & 15, b >> 4)
        col = np.where(ci != 0, ((ent >> 13) & 3) * 16 + ci, 0)
        pri = ((ent >> 15) & 1).astype(bool) & (ci != 0)
        return col, pri

    def render_line(self, y):
        """Render scanline y into self.fb with the CURRENT register/VRAM state."""
        r = self.reg
        h40 = bool(r[12] & 0x81)
        W = 320 if h40 else 256
        self.fb_w = W
        if y >= 240:
            return
        pal = self.palette_rgb()
        bg = (r[7] >> 4 & 3) * 16 + (r[7] & 15)
        if not (r[1] & 0x40):
            self.fb[y, :W] = pal[bg]
            return
        vram = np.frombuffer(bytes(self.vram), np.uint8)
        sizes = {0: 32, 1: 64, 3: 128}
        pw = sizes.get(r[16] & 3, 32)
        ph = sizes.get((r[16] >> 4) & 3, 32)
        a_col, a_pri = self._plane_line(vram, (r[2] & 0x38) << 10, y, W, True, pw, ph)
        b_col, b_pri = self._plane_line(vram, (r[4] & 7) << 13, y, W, False, pw, ph)
        # window plane
        wx, wy = r[17], r[18]
        wyc = (wy & 0x1F) * 8
        in_y = (y >= wyc) if wy & 0x80 else (y < wyc)
        wxc = (wx & 0x1F) * 16
        xs = np.arange(W)
        in_x = (xs >= wxc) if wx & 0x80 else (xs < wxc)
        if (wx & 0x9F) or (wy & 0x9F):
            win = in_x | in_y
            if win.any():
                ww = 64 if h40 else 32
                wbase = ((r[3] & 0x3E) << 10) if h40 else ((r[3] & 0x3F) << 10)
                na = wbase + (((y // 8) & 31) * ww + (xs // 8) % ww) * 2
                ent = (vram[na & 0xFFFF].astype(np.int32) << 8) | vram[(na + 1) & 0xFFFF]
                wc, wp = self._tile_pixels(vram, ent, xs % 8, np.full(W, y % 8))
                a_col = np.where(win, wc, a_col)
                a_pri = np.where(win, wp, a_pri)
        # sprites on this line
        s_col = np.zeros(W, np.int32)
        s_pri = np.zeros(W, bool)
        sat = ((r[5] & 0x7E) << 9) if h40 else ((r[5] & 0x7F) << 9)
        order, i = [], 0
        for _ in range(80 if h40 else 64):
            o = (sat + i * 8) & 0xFFFF
            order.append(o)
            link = self.vram[o + 3] & 0x7F
            if link == 0:
                break
            i = link
        for o in reversed(order):
            sy = (((self.vram[o] << 8) | self.vram[o + 1]) & 0x3FF) - 128
            sz = self.vram[o + 2]
            hh = (sz & 3) + 1
            if not (sy <= y < sy + hh * 8):
                continue
            hw = ((sz >> 2) & 3) + 1
            attr = (self.vram[o + 4] << 8) | self.vram[o + 5]
            sx = (((self.vram[o + 6] << 8) | self.vram[o + 7]) & 0x1FF) - 128
            row = y - sy
            if attr & 0x1000:
                row = hh * 8 - 1 - row
            cy, ty = row // 8, row % 8
            for cx in range(hw):
                tcx = hw - 1 - cx if attr & 0x800 else cx
                t = ((attr & 0x7FF) + tcx * hh + cy) & 0x7FF
                for xx in range(8):
                    X = sx + cx * 8 + xx
                    if not 0 <= X < W:
                        continue
                    txx = 7 - xx if attr & 0x800 else xx
                    bb = self.vram[(t * 32 + ty * 4 + (txx >> 1)) & 0xFFFF]
                    c = (bb & 15) if txx & 1 else (bb >> 4)
                    if c:
                        s_col[X] = ((attr >> 13) & 3) * 16 + c
                        s_pri[X] = bool(attr >> 15)
        out = np.full(W, bg, np.int32)
        for col, pri, hi in ((b_col, b_pri, False), (a_col, a_pri, False), (s_col, s_pri, False),
                             (b_col, b_pri, True), (a_col, a_pri, True), (s_col, s_pri, True)):
            m = (col != 0) & (pri == hi)
            out[m] = col[m]
        self.fb[y, :W] = pal[out]

    def frame_image(self):
        H = 240 if self.reg[1] & 0x08 else 224
        return self.fb[:H, :self.fb_w].copy()

    def render(self):
        """Whole-frame render from the current state (no raster effects)."""
        self.begin_frame()
        for y in range(224):
            self.render_line(y)
        return self.frame_image()
