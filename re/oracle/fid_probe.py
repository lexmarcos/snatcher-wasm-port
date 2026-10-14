#!/usr/bin/env python3
"""FIDELITY: dump the ORIGINAL's VDP state at rel frame N (same input convention as fid_cmp.py).
usage: .venv/bin/python -I fid_probe.py --inputs '[[rel,bits],..]' --frame N --out DIR [--wait 300]
writes DIR/vram.bin cram.txt sprites.txt (sprite attribute table decoded) and prints per-line register splits + object slots (sub RAM $7A00 + n*0x50)."""
import argparse, json, os, sys
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from machine import Machine
ap = argparse.ArgumentParser()
ap.add_argument('--inputs', default='[]'); ap.add_argument('--frame', type=int, required=True)
ap.add_argument('--wait', type=int, default=300); ap.add_argument('--load', default='junker_hq'); ap.add_argument('--out', required=True)
a = ap.parse_args()
os.makedirs(a.out, exist_ok=True)
m = Machine()
m.load_state(os.path.join(HERE, 'states', a.load + '.pkl'))
m.inputs = []
for _ in range(a.wait): m.step_frame(capture=False)
seq = []; t = 0
for f, b in sorted(json.loads(a.inputs)): seq.append([f - t - 1, 0]); seq.append([1, b]); t = f
m.inputs = seq
for _ in range(a.frame - 1): m.step_frame(capture=False)
rec = []
crams = {}
orig = m.vdp.render_line
def rl(y):
    r = m.vdp.reg
    rec.append((y, r[1] & 0x40, r[2], r[4], r[11], r[12], r[16], tuple(m.vdp.vsram[:4])))
    crams.setdefault(tuple(m.vdp.cram[:64]), []).append(y)
    orig(y)
m.vdp.render_line = rl
m.step_frame(capture=True)
prev = None
for t in rec:
    if t[1:] != (prev[1:] if prev else None):
        print('line %3d disp=%d r2=%02x r4=%02x r11=%02x r12=%02x r16=%02x vsram=%s' % t)
    prev = t
v = m.vdp
for i, (c, ys) in enumerate(crams.items()):
    print('CRAM set %d lines %d-%d (%d lines):' % (i, min(ys), max(ys), len(ys)), ' '.join('%03x' % x for x in c))
open(os.path.join(a.out, 'vram.bin'), 'wb').write(bytes(v.vram))
open(os.path.join(a.out, 'cram.txt'), 'w').write(' '.join('%03x' % x for x in v.cram[:64]))
sat = (v.reg[5] & 0x7F) << 9
print('regs', ' '.join('%02x' % x for x in v.reg[:24]), 'sat %04x' % sat)
lines = []
idx = 0
for _ in range(80):
    o = sat + idx * 8
    y = ((v.vram[o] << 8 | v.vram[o + 1]) & 0x3FF) - 128
    sz = v.vram[o + 2]; link = v.vram[o + 3] & 0x7F
    at = v.vram[o + 4] << 8 | v.vram[o + 5]
    x = ((v.vram[o + 6] << 8 | v.vram[o + 7]) & 0x1FF) - 128
    lines.append('spr %2d x %4d y %4d size %02x attr %04x link %d' % (idx, x, y, sz, at, link))
    idx = link
    if idx == 0: break
open(os.path.join(a.out, 'sprites.txt'), 'w').write('\n'.join(lines) + '\n')
print('\n'.join(lines))
s = m.sub
for n in range(64):
    b = 0x7A00 + n * 0x50
    if s.r16(b):
        print('obj %2d x %4d y %4d flags25 %02x idx %04x script %06x frame %06x' % (n, s.r16(b + 6), s.r16(b + 0xA), s.r8(b + 0x25), s.r16(b + 0x28), s.r32(b + 0x32), s.r32(b + 0x36)))
