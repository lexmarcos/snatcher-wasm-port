#!/usr/bin/env python3
"""Compare two fid_play stories (HLE vs oracle2 or any two) event by event: aligned on the event text (MSG/MENU/queue lines), prints the frame of
each side, the delta and the change of the delta since the previous aligned event (= extra time spent in that segment).
usage: o2_storycmp.py story_a.txt story_b.txt [--min N] [--summary]"""
import difflib, re, sys
def load(p):
    ev = []
    for l in open(p):
        m = re.match(r'(\d+) (.*)', l.rstrip('\n'))
        if m and not l.startswith('presses') and 'STALL' not in l: ev.append((int(m.group(1)), m.group(2)))
    return ev
a, b = load(sys.argv[1]), load(sys.argv[2])
mn = int(sys.argv[sys.argv.index('--min') + 1]) if '--min' in sys.argv else 0
sm = difflib.SequenceMatcher(None, [t for _, t in a], [t for _, t in b], autojunk=False)
pairs = []
for tag, i1, i2, j1, j2 in sm.get_opcodes():
    if tag == 'equal':
        for k in range(i2 - i1): pairs.append((a[i1 + k], b[j1 + k]))
    else:
        for k in a[i1:i2]: print('only A', k[0], k[1][:100])
        for k in b[j1:j2]: print('only B', k[0], k[1][:100])
prev = 0; tot = 0
print('%7s %7s %6s %6s  event' % ('A', 'B', 'delta', 'seg'))
rows = []
for (fa, ta), (fb, tb) in pairs:
    d = fb - fa
    rows.append((fa, fb, d, d - prev, ta)); prev = d
if '--summary' in sys.argv:
    import statistics
    segs = [r[3] for r in rows]
    print('aligned events', len(rows), 'final delta', rows[-1][2] if rows else None, 'mean seg', round(statistics.mean(segs), 2), 'median', statistics.median(segs))
else:
    for fa, fb, d, sg, ta in rows:
        if abs(sg) >= mn: print('%7d %7d %6d %6d  %s' % (fa, fb, d, sg, ta[:90]))
