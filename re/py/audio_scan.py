"""Scan PCMLT_01 voice table + PCMLD_01 segment chains. Usage: python3 -I audio_scan.py <files dir>"""
import struct, sys, collections
D = sys.argv[1]
lt = open(D + '/PCMLT_01.BIN', 'rb').read()
ld = open(D + '/PCMLD_01.BIN', 'rb')
stats = collections.Counter(); rates = collections.Counter()
for i in range(1, 1198):
    w0, w1, s, l = struct.unpack('>HHHH', lt[i*8:i*8+8])
    ld.seek(s * 2048); blk = ld.read(l * 2048)
    o = 0; segs = []
    while True:
        cnt, ff, rate = struct.unpack('>IIH', blk[o:o+10])
        if cnt == 0xFFFFFFFF:
            break
        if ff != 0xFFFFFFFF or blk[o+10:o+16] != bytes(6):
            segs.append(('BADHDR', o, blk[o:o+16].hex())); break
        segs.append((rate, cnt - 4))
        o += 16 + cnt
        if o + 16 > len(blk):
            segs.append(('OVER', o)); break
    stats[len(segs)] += 1
    for sg in segs:
        rates[sg[0]] += 1
    if len(sys.argv) > 2 or i < 40 or any(isinstance(x[0], str) for x in segs):
        print(i, '%04x %04x' % (w0, w1), s, l, 'end@%x/%x' % (o, len(blk)), segs[:6])
print('segments per entry', sorted(stats.items()))
print('rates', rates.most_common())
