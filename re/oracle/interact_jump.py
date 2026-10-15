#!/usr/bin/env python3
"""INTERACT helper: jump the ORIGINAL game into any script position and play it with inputs.
usage: .venv/bin/python -I interact_jump.py --script SP04 --pc 0 --out out/interact_jordan \
          --inputs '[[200,0],[1,32],...]' --shot-every 30 [--load junker_hq] [--wait 300]
Runs `--wait` frames from the save state (the Junker HQ command menu is up after 300 frames), then
writes the script file (assets/raw/files/SPnn.BIN) to S:0x1A800, sets script id $980C, a pending jump
$9802 = pc, clears the menu path ($9893, mark stack $9841) and the menu result $79F4 = $FF so the VM
thread leaves the menu and takes the jump (S:0x16892).  Disc data is read as data only."""
import argparse, json, os, sys
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from oracle_sel import Machine
from PIL import Image

ap = argparse.ArgumentParser()
ap.add_argument('--script', default='', help='SPnn to jump into; empty = stay in the loaded state (use with --poke)')
ap.add_argument('--pc', default='0')
ap.add_argument('--load', default='junker_hq')
ap.add_argument('--wait', type=int, default=300)
ap.add_argument('--frames', type=int, default=600)
ap.add_argument('--inputs', default='[]', help='[[frames,bits],...] applied after the jump')
ap.add_argument('--shot-every', type=int, default=30)
ap.add_argument('--shot-at', default='')
ap.add_argument('--out', default=os.path.join(HERE, 'out', 'interact'))
ap.add_argument('--pre', default='[]', help='[[frames,bits],...] played right after the jump, before --until (menu navigation)')
ap.add_argument('--until', default='', help='"ADDR=VAL[,ADDR=VAL..]" (sub words) – press C every 60 frames until it holds, then start --inputs and count frames from there')
ap.add_argument('--watch', default='', help='comma list of sub addresses (word) printed every frame of the --inputs phase')
ap.add_argument('--poke', default='', help='extra pokes after the jump, "sub:ADDR=HEXBYTES;..."')
a = ap.parse_args()
os.makedirs(a.out, exist_ok=True)
m = Machine(trace_path=os.path.join(a.out, 'trace.jsonl'))
m.load_state(os.path.join(HERE, 'states', a.load + '.pkl'))
m.inputs = []
for _ in range(a.wait):
    m.step_frame(capture=False)
s = m.sub
if a.script:
    n = int(a.script[2:])
    data = open(os.path.join(HERE, '..', '..', 'assets', 'raw', 'files', 'SP%02d.BIN' % n), 'rb').read()
    s.write(0x1A800, data[:0xD000])
    s.w16(0x980C, 57 + n)
    s.w16(0x9800, int(a.pc, 16)); s.w16(0x9802, 0xFFFF)   # next pass starts at pc
    s.w8(0x9893, 0); s.w8(0x9841, 0)
    s.w8(0x79F4, 0xFF)
for p in filter(None, a.poke.split(';')):
    ad, hx = p.split(':')[1].split('=')
    s.write(int(ad, 16), bytes.fromhex(hx))
m.inputs = [list(x) for x in json.loads(a.pre)]
while m.inputs:
    m.step_frame(capture=False)
for cond in filter(None, a.until.split(',')):     # several conditions are waited for in turn
    ad, val = cond.split('=')
    ad, val = int(ad, 16), int(val, 16)
    k = 0
    while s.r16(ad) != val and k < 6000:
        m.pad = 0
        m.inputs = [[1, 32 if k % 60 == 30 else 0]]
        m.step_frame(capture=False); k += 1
    print('until', cond, 'reached after', k, 'frames')
m.inputs = [list(x) for x in json.loads(a.inputs)]
shots = set(int(x) for x in a.shot_at.split(',') if x)
t0 = m.frame   # frames in --shot-at/--shot-every are relative to this (after --until)
for i in range(a.frames):
    f = m.frame + 1
    cap = bool((a.shot_every and (f - t0) % a.shot_every == 0) or (f - t0) in shots)
    rec = m.step_frame(capture=cap)
    if a.watch:
        print(f - t0, ' '.join('%s=%04x' % (w, s.r16(int(w, 16))) for w in a.watch.split(',')))
    if cap:
        Image.fromarray(m.vdp.frame_image()).save(os.path.join(a.out, 'f%05d.png' % (f - t0)))
    if m.main.halted or m.sub.halted:
        print('HALT', m.main.err, m.sub.err); print('sub pc ring', [hex(x) for x in m.sub.pc_ring()[-24:]]); break
print('frames', m.frame, 'vm_pc %04x script %04x state %d' % (rec['vm_pc'], rec['script'], rec['state']))
if os.environ.get('SAVE'):
    m.save_state(os.environ['SAVE'])
