#!/usr/bin/env python3
"""Replay a recorded pad script (fid_play json) from an oracle_jump start on oracle2 (or the HLE oracle) and take screenshots / print state words.
usage (from re/oracle): ../oracle2/o2_replay.py KIND SPnn:pc pad.json FRAMES --shots 100,200 --out DIR [--watch s16:9808,s16:980C] [--from-rel N]
Prints one line per change of the watched words (frame = rel, like fid_play)."""
import argparse, json, os, sys
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE); sys.path.insert(0, os.path.join(HERE, '..', 'oracle'))
ap = argparse.ArgumentParser()
ap.add_argument('kind'); ap.add_argument('jump'); ap.add_argument('pad'); ap.add_argument('frames', type=int)
ap.add_argument('--shots', default=''); ap.add_argument('--every', type=int, default=0); ap.add_argument('--out', default='out/replay')
ap.add_argument('--watch', default='s16:9808,s16:980C,s16:7840,s16:7842,s16:7896')
a = ap.parse_args()
os.environ['ORACLE'] = '2' if a.kind == '2' else 'hle'
from o2_hooks import new_machine
from fid_common import oracle_jump, patch_cdtime
from PIL import Image
patch_cdtime()
os.makedirs(a.out, exist_ok=True)
m = new_machine(a.kind)
oracle_jump(m, a.jump, 300)
press = {e[0]: e[1] for e in json.load(open(a.pad))}
shots = set(int(x) for x in a.shots.split(',') if x)
watch = []
for w in a.watch.split(','):
    k, ad = w.split(':'); watch.append((w, k, int(ad, 16)))
def val(k, ad):
    cpu = m.sub if k[0] == 's' else m.main
    return {'8': cpu.r8, '16': cpu.r16, '32': cpu.r32}[k[1:]](ad)
prev = None
for rel in range(1, a.frames + 1):
    m.sub.write(0x786A, bytes(4)); m.inputs = [[1, press.get(rel, 0)]]
    cap = rel in shots or (a.every and rel % a.every == 0)
    m.step_frame(capture=cap)
    cur = tuple(val(k, ad) for _, k, ad in watch)
    if cur != prev:
        print(rel, ' '.join('%s=%x' % (w, v) for (w, _, _), v in zip(watch, cur)), flush=True); prev = cur
    if cap:
        Image.fromarray(m.vdp.frame_image()).save(os.path.join(a.out, 'r%05d.png' % rel))
