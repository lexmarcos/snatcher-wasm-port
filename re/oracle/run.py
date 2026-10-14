#!/usr/bin/env python3
"""Run the Snatcher test oracle.
usage: re/oracle/.venv/bin/python -I re/oracle/run.py --frames N [--inputs JSON] [--shot-every K] [--out DIR]
inputs: [[frames, padbits], ...] with pad bits UP=1 DOWN=2 LEFT=4 RIGHT=8 B=16 C=32 A=64 START=128
"""
import argparse, json, os, sys, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from machine import Machine
from PIL import Image

ap = argparse.ArgumentParser()
ap.add_argument('--frames', type=int, default=300)
ap.add_argument('--inputs', default='[]')
ap.add_argument('--shot-every', type=int, default=60)
ap.add_argument('--shot-at', default='', help='comma list of frames')
ap.add_argument('--out', default=os.path.join(os.path.dirname(os.path.abspath(__file__)), 'out', 'run'))
ap.add_argument('-v', '--verbose', action='store_true')
ap.add_argument('--load', help='start from a save state (.pkl)')
ap.add_argument('--save', help='write a save state (.pkl) at the end')
a = ap.parse_args()
os.makedirs(a.out, exist_ok=True)
m = Machine(trace_path=os.path.join(a.out, 'trace.jsonl'), verbose=a.verbose)
if a.load:
    m.load_state(a.load)
m.inputs = [list(x) for x in json.loads(a.inputs)]
shots = set(int(x) for x in a.shot_at.split(',') if x)
t0 = time.time()
rec = {'vm_pc': 0, 'script': 0, 'state': 0}
for i in range(a.frames):
    f = m.frame + 1
    cap = bool((a.shot_every and f % a.shot_every == 0) or f in shots)
    rec = m.step_frame(capture=cap)
    if cap:
        Image.fromarray(m.vdp.frame_image()).save(os.path.join(a.out, 'f%05d.png' % f))
    if m.main.halted or m.sub.halted:
        print('HALT', m.main.err, m.sub.err)
        break
if a.save:
    m.save_state(a.save)
print('frames', m.frame, 'time %.1fs' % (time.time() - t0), 'main pc %06x sub pc %06x' % (m.main.pc(), m.sub.pc()),
      'vm_pc %04x script %04x state %d' % (rec['vm_pc'], rec['script'], rec['state']))
