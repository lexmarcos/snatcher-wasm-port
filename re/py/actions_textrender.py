"""Prototype of the message box (sequencer command 08) and glyph renderer, used to verify
docs/re/actions.md §2 against real data. Renders every page of one message into PNGs.

usage: python3 -I actions_textrender.py <sub_prg.bin> <SPxx.BIN> <text_off_hex> <speaker_word_hex> <outprefix>
  speaker word lo byte: 0 = no name, 1 = narration, n>=2 = name[n-1]
Window = 256x64 4bpp bitmap (32x8 tiles), palette = defaults set by subfn_17 (S:0xF600).
"""
import sys, struct, zlib

B = 0x6000
FONT, WID, NAMES = 0x14B7C, 0x14EAC, 0x13552
# CRAM words written by subfn_17 to palette set A entries 0..7 (S:0xF600..0xF638)
CRAM = [0x000, 0xEEE, 0x0EE, 0x86E, 0xEE0, 0xE42, 0x4E0, 0x000]


def cram_rgb(w):
    r, g, b = (w >> 1) & 7, (w >> 5) & 7, (w >> 9) & 7
    return (r * 36, g * 36, b * 36)


class Window:
    def __init__(self, img):
        self.img = img
        self.pix = bytearray(256 * 64)
        self.x = self.y = 0
        self.col = 1
        self.fixed = 0

    def glyph(self, c):
        g = c - 0x20
        if g < 0 or g >= (WID - FONT) // 8:
            return
        x0 = self.x + (1 if self.fixed and g in (0x11, 0x29) else 0)
        for r in range(8):
            bits = self.img[FONT - B + g * 8 + r]
            for k in range(8):
                if bits & (0x80 >> k):
                    px, py = x0 + k, self.y + r
                    if 0 <= px < 256 and 0 <= py < 64:
                        self.pix[py * 256 + px] = self.col
        self.x += 9 if self.fixed else self.img[WID - B + g]

    def stream(self, s):
        """Render a render-stream (instantly; delays ignored)."""
        i = 0
        while i < len(s):
            c = s[i]; i += 1
            if c == 0xFF: break
            elif c == 0xFE: self.col = s[i]; i += 1
            elif c == 0xFD: self.y += 9; self.x = 0x18; i += 1
            elif c == 0xFC: i += 1
            elif c == 0xFB: self.x, self.y = s[i], s[i + 1]; i += 2
            elif c == 0xFA: i += 2
            elif c == 0xF9: i += 1
            elif c == 0xF8: i += 2
            elif c == 0xF7: self.fixed = s[i]; i += 1
            elif c < 0x20: i += 1
            else: self.glyph(c)

    def png(self, path, scale=3):
        W, H = 256 * scale, 64 * scale
        rows = []
        for y in range(H):
            row = bytearray([0])
            for x in range(W):
                row += bytes(cram_rgb(CRAM[self.pix[(y // scale) * 256 + x // scale] & 7]))
            rows.append(bytes(row))
        def ch(t, d): return struct.pack('>I', len(d)) + t + d + struct.pack('>I', zlib.crc32(t + d) & 0xffffffff)
        open(path, 'wb').write(b'\x89PNG\r\n\x1a\n' + ch(b'IHDR', struct.pack('>IIBBBBB', W, H, 8, 2, 0, 0, 0)) +
                               ch(b'IDAT', zlib.compress(b''.join(rows))) + ch(b'IEND', b''))


def name_stream(img, spk):
    o = struct.unpack('>h', img[NAMES - B + 2 * spk:NAMES - B + 2 * spk + 2])[0]
    p = NAMES + o - B
    c = img[p]
    col = 3 if c & 0x80 else c & 7
    if not c & 0x80:
        p += 1
    txt = img[p:p + 0x19].split(b'\xff')[0]
    return bytes([0xFC, 0, 0xF9, 1, 0xFB, 0x10, 0, 0xFE, col]) + txt + b'\xff'


def message(img, sp, off, spk_word, outprefix):
    text = sp[0x3800:]
    spk = (spk_word & 0xFF) - 1   # $9744
    pos = off
    col = 1
    done = False
    page = 0
    while not done:
        w = Window(img)
        if spk not in (0, -1):
            w.stream(name_stream(img, spk))
            y = 9
        else:
            y = 0
        while True:
            line = bytearray([0xFC, 1, 0xF9, 1, 0xFE, col, 0xFB, 0x28 if spk == 0 else 0x18, y])
            n = 0; endpage = False
            while True:
                c = text[pos]; pos += 1
                if c == 0xFF: done = True; break
                if c == 0xF2: break
                if c == 0xF6: endpage = True; break
                if c == 0xF4: continue
                if c in (0xEC, 0xEE, 0xF0):   # button letters, config 0: A / B / C
                    c = {0xEC: 0x41, 0xEE: 0x42, 0xF0: 0x43}[c]
                elif c >= 0xF8:
                    col = c & 7; line += bytes([0xFE, col]); continue
                line.append(c); n += 1
                if n >= 0x32: break
            line.append(0xFF)
            w.stream(bytes(line))
            if done or endpage: break
            y += 9
            if y >= 0x39: break
        w.png('%s_p%d.png' % (outprefix, page))
        print('page', page, 'ends at text offset %#x' % pos)
        page += 1


if __name__ == '__main__':
    img = open(sys.argv[1], 'rb').read()
    sp = open(sys.argv[2], 'rb').read()
    message(img, sp, int(sys.argv[3], 16), int(sys.argv[4], 16), sys.argv[5])
