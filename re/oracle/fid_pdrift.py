#!/usr/bin/env python3
"""fid_pdrift.py NAME [min_jump]: drift (engine press frame - oracle press frame) per recorded press; prints where it changes by >= min_jump with the kinds
of the two presses (m = message, u = menu, e = entry) and the oracle frames between them."""
import json, re, sys
n = sys.argv[1]; mj = int(sys.argv[2]) if len(sys.argv) > 2 else 3
P = json.load(open('out/fid2/%s.json' % n))
ep = [int(m.group(1)) for m in (re.match(r'\s*(\d+) sync press', l) for l in open('out/fid/%s/engine/log.txt' % n)) if m]
k = min(len(P), len(ep)); prev = None; hist = {}
for i in range(k):
    d = ep[i] - P[i][0]
    if prev is not None and abs(d - prev) >= mj:
        key = (P[i - 1][2], P[i][2], P[i][3]); hist.setdefault(key, []).append(d - prev)
        if len(sys.argv) > 3: print('press %d o%d (%s since %s -> %s since %s): %+d' % (i, P[i][0], P[i-1][2], P[i-1][3], P[i][2], P[i][3], d - prev))
    prev = d
print('presses', k, 'of', len(P), 'final drift', ep[k - 1] - P[k - 1][0])
for key, v in sorted(hist.items(), key=lambda x: -abs(sum(x[1]))): print(' prev kind/kind/since', key, 'n', len(v), 'sum %+d' % sum(v), v[:8])
