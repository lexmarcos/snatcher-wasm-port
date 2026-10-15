#!/usr/bin/env python3
"""fid_drift.py NAME [min_jump]: drift (engine frame - oracle frame) of the matched passes of out/fid/NAME/report.txt; prints the passes where the drift
changed by at least min_jump (default 15) frames since the previous pass, with the queue that STARTED at the previous pass (its commands took
the extra / missing time)."""
import re, sys
n = sys.argv[1]; mj = int(sys.argv[2]) if len(sys.argv) > 2 else 15
prev = None; rows = []
for l in open('out/fid/%s/report.txt' % n):
    m = re.match(r'\s+= o\s*(\d+) e\s*(\d+) \(d\s*([+-]?\d+)\)\s+(.*)', l)
    if m: rows.append((int(m.group(1)), int(m.group(2)), int(m.group(3)), m.group(4)))
for i, (fo, fe, d, q) in enumerate(rows):
    if i and abs(d - rows[i - 1][2]) >= mj:
        print('o%6d e%6d drift %+5d (%+d)  prev queue: %s' % (fo, fe, d, d - rows[i - 1][2], rows[i - 1][3][:150]))
print('passes', len(rows), 'final drift', rows[-1][2] if rows else None)
