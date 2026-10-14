"""Resolve PCM SFX table (PCMLT_01 @0x4000, loaded S:0x54000) against PCMDRMDT (S:0x55000)
and type-1 banks (S:0x65000)."""
import struct, sys
D = sys.argv[1]
lt = open(D + '/PCMLT_01.BIN', 'rb').read()
ld = open(D + '/PCMLD_01.BIN', 'rb')
dr = open(D + '/PCMDRMDT.BIN', 'rb').read()
def segs(blk):
    o = 0; out = {}
    while o + 16 <= len(blk):
        cnt, ff, rate = struct.unpack('>IIH', blk[o:o+10])
        if cnt == 0xFFFFFFFF or ff != 0xFFFFFFFF: break
        out[o] = (rate, cnt); o += 16 + cnt
    return out
drs = segs(dr)
banks = {}
for i in range(1, 36):
    w0, w1, s, l = struct.unpack('>HHHH', lt[i*8:i*8+8])
    ld.seek(s*2048); banks[i] = segs(ld.read(l*2048))
n = lt[0x4000]
print('count', n)
for k in range(1, n):
    e = lt[0x4000+16*k:0x4010+16*k]
    prio, ch, env, pan, fd, addr = struct.unpack('>BBBBHI', e[:10])
    if addr < 0x65000:
        src = 'DRUM+%x %s' % (addr-0x55000, 'seg' if addr-0x55000 in drs else 'NOSEG')
    else:
        off = addr - 0x65000
        m = [b for b in banks if off in banks[b]]
        src = 'BANK+%x in banks %s' % (off, m)
    print('%02x prio=%d ch=%02x env=%02x pan=%02x fd=%04x (%.0f Hz) %s' % (k, prio, ch, env, pan, fd, fd*32552.08/2048, src))
