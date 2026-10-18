#!/usr/bin/env python3
"""Probe per-scanline VDP state of the original game at a given frame.
usage: .venv/bin/python -I probe.py <scenario.json> <frame> [--load state]
Prints line ranges where display/VSRAM/regs/CRAM change, plus plane bases."""
import json, os, sys, hashlib
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from oracle_sel import Machine
HERE = os.path.dirname(os.path.abspath(__file__))
sc = json.load(open(sys.argv[1])); target = int(sys.argv[2])
m = Machine()
if len(sys.argv) > 4 and sys.argv[3] == '--load':
    m.load_state(os.path.join(HERE, 'states', sys.argv[4] + '.pkl'))
m.inputs = [list(x) for x in sc['inputs']]
while m.frame < target - 1:
    m.step_frame(capture=False)
rec = []
orig = m.vdp.render_line
def rl(y):
    r = m.vdp.reg
    rec.append((y, r[1] & 0x40, r[2], r[4], r[11], r[12], r[16], tuple(m.vdp.vsram[:4]), hashlib.md5(bytes(str(list(m.vdp.cram)), 'ascii')).hexdigest()[:6]))
    orig(y)
m.vdp.render_line = rl
m.step_frame(capture=True)
prev = None
for t in rec:
    if t[1:] != (prev[1:] if prev else None):
        print('line %3d disp=%d r2=%02x r4=%02x r11=%02x r12=%02x r16=%02x vsram=%s cram=%s' % t)
    prev = t
print('cram', [hex(x) for x in m.vdp.cram[:64]])
from PIL import Image
os.makedirs(os.path.join(HERE, 'out'), exist_ok=True)
Image.fromarray(m.vdp.frame_image()).save(os.path.join(HERE, 'out', 'probe.png'))
open(os.path.join(HERE, 'out', 'oracle_vram.bin'),'wb').write(bytes(m.vdp.vram))
