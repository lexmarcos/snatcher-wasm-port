"""Experiment: parse DATA_xx scene tables ($28004 -> word offsets -> chunk lists). See docs/re/graphics.md."""
import struct, sys
BASE = 0x28000
def be16(d, o): return struct.unpack('>H', d[o:o+2])[0]
def be32(d, o): return struct.unpack('>I', d[o:o+4])[0]

def scenes(d):
    t = be32(d, 4) - BASE
    # scene table: word offsets relative to table start; count = until first offset target
    offs = []
    i = 0
    first = None
    while True:
        o = be16(d, t + 2*i)
        if first is None: first = o
        if t + 2*i >= t + first: break
        offs.append(o); i += 1
    return t, offs

def chunks(d, p):
    out = []
    while True:
        ty = d[p]
        if ty & 0x80: break
        ln = d[p+1]
        out.append((ty, p+2, d[p+2:p+2+ln]))
        p += 2 + ln
    return out

if __name__ == '__main__':
    d = open(sys.argv[1], 'rb').read()
    t, offs = scenes(d)
    print('scene table @%x, %d scenes' % (t, len(offs)))
    for n, o in enumerate(offs[:int(sys.argv[2]) if len(sys.argv) > 2 else 999]):
        print('scene %d @%x' % (n, t+o))
        for ty, p, pl in chunks(d, t + o):
            print('   type %d @%x len %d: %s' % (ty, p, len(pl), pl[:48].hex()))
