#!/usr/bin/env python3
"""fid_summary.py NAME [threshold]: screens of out/fid/NAME/report.txt whose best (phase-refined) difference exceeds threshold px."""
import re, sys
n = sys.argv[1]; th = int(sys.argv[2]) if len(sys.argv) > 2 else 300
rows = []
for l in open('out/fid/%s/report.txt' % n):
    m = re.match(r'\s+r(\d+) diff\s+(\d+) px.*?(?:best of \+-\d+ frames: (\d+) px at ([+-]\d+))?\s*$', l)
    if m:
        f, d = int(m.group(1)), int(m.group(2)); b = int(m.group(3)) if m.group(3) else d
        rows.append((f, d, b, m.group(4)))
print('%s: %d screens, %d exact (<=%d px), %d > %d px after phase search' % (n, len(rows), sum(1 for r in rows if r[2] <= 60), 60, sum(1 for r in rows if r[2] > th), th))
for f, d, b, o in rows:
    if b > th: print('  r%05d raw %6d best %6d at %s' % (f, d, b, o))
