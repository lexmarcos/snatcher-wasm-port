#!/usr/bin/env python3
"""fid_gap.py NAME: for every matched pass, frames from the last recorded press to the pass in the oracle and in the engine; lists those that differ by >= 3 frames
(`out/fid/NAME`, needs the report + engine log + the pad json out/fid2/NAME.json)."""
import json, re, sys, bisect
n = sys.argv[1]
presses = [e[0] for e in json.load(open('out/fid2/%s.json' % n))]
elog = [(int(m.group(1)), 1) for m in (re.match(r'\s*(\d+) sync press', l) for l in open('out/fid/%s/engine/log.txt' % n)) if m]
epress = [f for f, _ in elog]
rows = []
for l in open('out/fid/%s/report.txt' % n):
    m = re.match(r'\s+= o\s*(\d+) e\s*(\d+) \(d\s*([+-]?\d+)\)\s+(.*)', l)
    if m: rows.append((int(m.group(1)), int(m.group(2)), m.group(4)))
out = []
mode = sys.argv[2] if len(sys.argv) > 2 else 'after'
for fo, fe, q in rows:
    if mode == 'after':
        i = bisect.bisect_right(presses, fo) - 1; j = bisect.bisect_right(epress, fe) - 1
        if i < 0 or j < 0: continue
        go, ge = fo - presses[i], fe - epress[j]
    else:   # pass -> first press after it
        i = bisect.bisect_left(presses, fo); j = bisect.bisect_left(epress, fe)
        if i >= len(presses) or j >= len(epress): continue
        go, ge = presses[i] - fo, epress[j] - fe
    if abs(go - ge) >= 3 and go < 400: out.append((fo, go, ge, q[:80]))
for r in out[:40]: print('o%6d press->pass oracle %4d engine %4d (%+d)  %s' % (r[0], r[1], r[2], r[2] - r[1], r[3]))
print(len(out), 'of', len(rows))
