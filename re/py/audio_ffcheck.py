"""Where is the first 0xFF in each raw voice entry (end marker)?"""
import struct, sys, collections
D = sys.argv[1]
lt = open(D + '/PCMLT_01.BIN', 'rb').read()
ld = open(D + '/PCMLD_01.BIN', 'rb')
tail = collections.Counter(); bad = []
for i in range(36, 1198):
    w0, w1, s, l = struct.unpack('>HHHH', lt[i*8:i*8+8])
    ld.seek(s * 2048); blk = ld.read(l * 2048)
    p = blk.find(b'\xff')
    if p < 0:
        bad.append((i, 'noff')); continue
    rest = blk[p:]
    tail[(len(blk) - p) // 2048] += 1
    nonpad = sum(1 for c in rest if c not in (0xff, 0x80, 0x00))
    if nonpad > 64: bad.append((i, p, len(blk), nonpad))
print('sectors after first FF:', sorted(tail.items())[:10])
print('suspicious', len(bad), bad[:10])
