#!/usr/bin/env python3
"""fid_dsum.py NAME...: sum of the drift changes (engine - oracle frames) per type of the queue that preceded each matched pass (first commands)."""
import re, sys, collections
agg = collections.defaultdict(lambda: [0, 0])
for n in sys.argv[1:]:
    rows = []
    for l in open('out/fid/%s/report.txt' % n):
        m = re.match(r'\s+= o\s*(\d+) e\s*(\d+) \(d\s*([+-]?\d+)\)\s+(.*)', l)
        if m: rows.append((int(m.group(3)), m.group(4).split()))
    for i in range(1, len(rows)):
        dd = rows[i][0] - rows[i - 1][0]
        q = rows[i - 1][1]
        cmds = []; k = 0
        # crude signature: command words that are known command numbers at the start of each item
        sig = ' '.join(q[:1] + [w for w in q[1:] if w in ('0011', '001E', '000F', '0008', '0015', '0012', '0010', '000D', '000A', '001B', '000E', '000B', '0009', '0007', '001C')][:6])
        agg[sig][0] += dd; agg[sig][1] += 1
for sig, (s, c) in sorted(agg.items(), key=lambda x: -abs(x[1][0]))[:25]:
    print('%6d over %4d passes (%+.1f avg)  %s' % (s, c, s / c, sig))
