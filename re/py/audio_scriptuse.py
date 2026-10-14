"""Collect audio-related script operands (op 04 voice, 05 pcm sfx, 0a fm music, 0d cd track,
0f fm sfx) from all SPxx.BIN code regions (parser: re/py/sparse.py)."""
import sys, glob, collections
sys.path.insert(0, '/home/suel/Documents/snatcher/re/py')
from sparse import node
OPS = {0x04: 'voice', 0x05: 'pcmsfx', 0x0a: 'fm', 0x0d: 'cdda', 0x0f: 'fmsfx'}
use = collections.defaultdict(collections.Counter)
where = collections.defaultdict(set)
for f in sorted(glob.glob(sys.argv[1] + '/SP*.BIN')):
    b = open(f, 'rb').read()
    o = 0
    while o < 0x3800:
        out = []
        try:
            e = node(b, o, 0, out)
        except Exception:
            break
        for (no, dep, op, ptr, ops, end) in out:
            if op in OPS:
                v = ops[0][1]
                use[OPS[op]][v] += 1
                where[(OPS[op], v)].add(f.split('/')[-1][:4])
        o = e
for k, c in use.items():
    print(k, len(c), 'distinct; range', hex(min(c)), hex(max(c)))
    print('   ', ' '.join('%x' % v for v in sorted(c)))
if len(sys.argv) > 2:
    for (k, v), s in sorted(where.items()):
        if k in ('cdda', 'fm', 'fmsfx', 'pcmsfx'): print(k, hex(v), sorted(s))
