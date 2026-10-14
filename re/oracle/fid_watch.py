#!/usr/bin/env python3
"""FIDELITY: per-frame watch of ORIGINAL memory. usage (from re/oracle):
  .venv/bin/python -I fid_watch.py --inputs '[[rel,bits],..]' --frames N --watch s:7892,m:E061w,s:79F4 [--wait 300] [--from 40]
addresses are hex; prefix s: (sub CPU) or m: (main CPU, 0xFFxxxx implied for 4-digit addresses); suffix b/w/l = size (default b).
Frame numbers printed = frame index after the step (pad of rel frame r is applied in the frame printed as r), starting at rel 1."""
import argparse, json, os, sys
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from machine import Machine
ap = argparse.ArgumentParser()
ap.add_argument('--inputs', default='[]'); ap.add_argument('--frames', type=int, default=60)
ap.add_argument('--watch', default='s:7892'); ap.add_argument('--wait', type=int, default=300)
ap.add_argument('--from', dest='frm', type=int, default=1); ap.add_argument('--load', default='junker_hq')
ap.add_argument('--rng', action='store_true', help='poke $786A..D = 0 every frame (pair with fid_cmp --rng)')
ap.add_argument('--changes', action='store_true', help='print only frames where a value changed')
a = ap.parse_args()
m = Machine(trace_path=os.path.join(HERE, 'out', 'fid_watch_trace.jsonl'))
m.load_state(os.path.join(HERE, 'states', a.load + '.pkl'))
m.inputs = []
for _ in range(a.wait): m.step_frame(capture=False)
seq = []; t = 0
for f, b in sorted(json.loads(a.inputs)): seq.append([f - t - 1, 0]); seq.append([1, b]); t = f
m.inputs = seq
ws = []
for w in a.watch.split(','):
    cpu, ad = w.split(':'); sz = 'b'
    if ad[-1] in 'bwl': sz = ad[-1]; ad = ad[:-1]
    ad = int(ad, 16)
    if cpu == 'm' and ad < 0x10000: ad |= 0xFF0000
    ws.append((w, cpu, ad, sz))
last = None
for i in range(1, a.frames + 1):
    if a.rng: m.sub.write(0x786A, bytes(4))
    m.step_frame(capture=False)
    vals = []
    for w, cpu, ad, sz in ws:
        c = m.sub if cpu == 's' else m.main
        v = {'b': c.r8, 'w': c.r16, 'l': c.r32}[sz](ad)
        vals.append('%s=%0*x' % (w, {'b': 2, 'w': 4, 'l': 8}[sz], v))
    if i >= a.frm and (not a.changes or vals != last): print(i, ' '.join(vals))
    last = vals
