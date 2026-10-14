import sys, glob
sys.path.insert(0, '/home/suel/Documents/snatcher/re/py')
from sparse import node
for f in sorted(glob.glob('/home/suel/Documents/snatcher/assets/raw/files/SP*.BIN')):
    b = open(f, 'rb').read()
    o = 0; n = 0; err = None
    while o < len(b):
        try:
            o = node(b, o); n += 1
        except Exception as ex:
            err = str(ex); break
    print(f.split('/')[-1], len(b), 'stmts', n, 'stopped at %x' % o, err or '', b[o:o+16].hex() if err else '')
