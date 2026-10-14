"""Print the speaker-name table S:0x13552 (word offsets relative to the table)."""
import sys, struct
img = open(sys.argv[1], 'rb').read(); B = 0x6000; T = 0x13552
n = (0x135B2 - T) // 2  # table ends where loc_0135b2 code starts

for i in range(n):
    o = struct.unpack('>h', img[T - B + 2 * i:T - B + 2 * i + 2])[0]
    p = T + o - B
    c = img[p]
    col = 3 if c & 0x80 else c & 7
    s = img[p + (0 if c & 0x80 else 1):].split(b'\xff')[0]
    print(i, 'colour', col, s.decode('latin1'))
