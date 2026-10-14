"""Compare engine PNG frames with oracle PNG frames (MODULES workstream).
usage: modules_cmp.py ENGINE_DIR ORACLE_DIR OFFSET[,FROM:OFFSET2...] [step] [--sheet out.png] [--lo A] [--hi B]
engine frame = oracle frame - offset (offset list: "1639,9700:1654" = 1639 up to oracle frame 9700, then 1654).
Prints per oracle frame the mean absolute RGB difference (0..255) and the fraction of differing pixels.
Engine frames must exist (run the example with SHOTAT=...)."""
import sys, os, glob
from PIL import Image
import numpy as np
args = [a for a in sys.argv[1:] if not a.startswith('--')]
ed, od, offs = args[0], args[1], args[2]
sheet = sys.argv[sys.argv.index('--sheet') + 1] if '--sheet' in sys.argv else None
lo = int(sys.argv[sys.argv.index('--lo') + 1]) if '--lo' in sys.argv else 0
hi = int(sys.argv[sys.argv.index('--hi') + 1]) if '--hi' in sys.argv else 10**9
segs = []
for p in offs.split(','):
    if ':' in p: f, o = p.split(':'); segs.append((int(f), int(o)))
    else: segs.append((0, int(p)))
def off_for(o):
    r = segs[0][1]
    for f, v in segs:
        if o >= f: r = v
    return r
rows = []
for f in sorted(glob.glob(od + '/f*.png')):
    o = int(os.path.basename(f)[1:-4])
    if not lo <= o <= hi: continue
    e = o - off_for(o)
    ef = '%s/f%06d.png' % (ed, e)
    if not os.path.exists(ef): continue
    a = np.asarray(Image.open(f).convert('RGB'), dtype=np.int16)
    b = np.asarray(Image.open(ef).convert('RGB'), dtype=np.int16)
    if a.shape != b.shape: rows.append((o, e, -1, -1)); continue
    d = np.abs(a - b)
    rows.append((o, e, d.mean(), (d.sum(2) > 24).mean()))
bad = float(sys.argv[sys.argv.index('--bad') + 1]) if '--bad' in sys.argv else None
if bad is None:
    for o, e, m, fr in rows: print('oracle %6d engine %6d  mean %6.2f  differing %5.1f%%' % (o, e, m, fr * 100))
else:
    # compact: ranges of oracle frames with mean diff > bad
    rng = []
    for o, e, m, fr in rows:
        if m > bad:
            if rng and o - rng[-1][1] <= (rows[1][0] - rows[0][0] if len(rows) > 1 else 1): rng[-1][1] = o; rng[-1][2] = max(rng[-1][2], m)
            else: rng.append([o, o, m])
    print('frames over %.2f:' % bad, ', '.join('%d-%d(max %.1f)' % tuple(r) for r in rng) or 'none')
if rows: print('overall mean %.2f' % np.mean([r[2] for r in rows if r[2] >= 0]))
if sheet and rows:
    worst = sorted(rows, key=lambda r: -r[2])[:12]
    W = Image.new('RGB', (256 * 4, 224 * 6))
    for i, (o, e, m, fr) in enumerate(worst):
        W.paste(Image.open('%s/f%05d.png' % (od, o)).convert('RGB'), ((i % 2) * 512, (i // 2) * 224))
        W.paste(Image.open('%s/f%06d.png' % (ed, e)).convert('RGB'), ((i % 2) * 512 + 256, (i // 2) * 224))
    W.save(sheet)
