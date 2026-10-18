import os, struct, sys
img = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'images', 'sub_prg.bin'),'rb').read()
T = 0x16cdc - 0x6000
TYPES = {}
for i in range(0x4b):
    a, b = struct.unpack('>HH', img[T+4*i:T+4*i+4])
    TYPES[i] = (a >> 8, (a >> 4) & 15, a & 15, 0x15400 + b)

def node(b, o, depth=0, out=None):
    op = b[o]
    if op not in TYPES:
        raise ValueError('bad op %02x at %x' % (op, o))
    t1, t2, t3, h = TYPES[op]
    s = o + 1
    ptr = None
    if (t2 | t3) & 8:
        ptr = struct.unpack('>H', b[s:s+2])[0]; s += 2
    ops = []
    for t in (t1, t2, t3):
        if t in (0, 7, 8):
            continue
        if t == 4:
            e = node(b, s, depth+1, out)
            ops.append(('n', s)); s = e
        else:
            ops.append((t, struct.unpack('>H', b[s:s+2])[0])); s += 2
    if out is not None:
        out.append((o, depth, op, ptr, ops, s))
    return s

if __name__ == '__main__':
    b = open(sys.argv[1], 'rb').read()
    o = int(sys.argv[2], 16) if len(sys.argv) > 2 else 0
    n = 0
    while o < len(b) and n < int(sys.argv[3] if len(sys.argv) > 3 else 20):
        out = []
        try:
            e = node(b, o, 0, out)
        except Exception as ex:
            print('ERR', ex); break
        for (no, dep, op, ptr, ops, end) in sorted(out):
            print('%04x %s%02x%s %s' % (no, '  '*dep, op, ' ->%04x' % ptr if ptr is not None else '', ' '.join(('%d:%04x' % x if x[0] != 'n' else '@%04x' % x[1]) for x in ops)))
        print('---- end %04x' % e)
        o = e; n += 1
