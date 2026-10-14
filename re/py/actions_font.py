"""Dump the sub-CPU 1bpp text font (S:0x14B7C, 8 bytes/glyph, glyph = code-0x20)
and the proportional width table (S:0x14EAC, 1 byte/glyph) as a PNG + text.
usage: python3 -I actions_font.py <sub_prg.bin> <out.png>"""
import sys, struct, zlib
img = open(sys.argv[1], 'rb').read()
B = 0x6000
FONT, WID = 0x14B7C, 0x14EAC
n = (WID - FONT) // 8
wid = img[WID - B: WID - B + n]
cols = 16; rows = (n + cols - 1) // cols
W, H = cols * 10, rows * 10
pix = bytearray(W * H)
for g in range(n):
    gx, gy = (g % cols) * 10, (g // cols) * 10
    for r in range(8):
        b = img[FONT - B + g * 8 + r]
        for c in range(8):
            if b & (0x80 >> c):
                pix[(gy + r) * W + gx + c] = 255
            elif c >= wid[g]:
                pix[(gy + r) * W + gx + c] = 60  # beyond advance width
def png(w, h, data):
    raw = b''.join(b'\0' + bytes(data[y * w:(y + 1) * w]) for y in range(h))
    def ch(t, d): return struct.pack('>I', len(d)) + t + d + struct.pack('>I', zlib.crc32(t + d) & 0xffffffff)
    return b'\x89PNG\r\n\x1a\n' + ch(b'IHDR', struct.pack('>IIBBBBB', w, h, 8, 0, 0, 0, 0)) + ch(b'IDAT', zlib.compress(raw)) + ch(b'IEND', b'')
open(sys.argv[2], 'wb').write(png(W, H, pix))
print('glyphs', n, 'widths', ' '.join('%02x:%d' % (g + 0x20, wid[g]) for g in range(n)))
print('after width table:', img[WID - B + n: WID - B + n + 32].hex())
