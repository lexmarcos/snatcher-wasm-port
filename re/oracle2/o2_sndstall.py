#!/usr/bin/env python3
"""Sub-CPU stalls at sound starts, measured on oracle2 (real BIOS): frame-handler runs per frame (hook S:0xD6EE) around every PCM effect start
(subfn_06 S:0x1530E, d0 = id) and FM/voice call (subfn_12 S:0x15EB4) while a shooting stage is played with the oracle-driven pad policy of shoot_rec.py.
usage (from re/oracle): ../oracle2/o2_sndstall.py GROUP ITEM FRAMES SEED [--kind hle]   (stage = SP01 debug menu group:item, see docs/re/shooting.md)"""
import os, sys, collections, statistics
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE); sys.path.insert(0, os.path.join(HERE, '..', 'oracle'))
KIND = sys.argv[sys.argv.index('--kind') + 1] if '--kind' in sys.argv else '2'
os.environ['ORACLE'] = KIND if KIND == '2' else 'hle'
from shoot_common import start_stage
from o2_hooks import new_machine as _nm
import shoot_rec
g, it, frames, seed = [int(x) for x in sys.argv[1:5]]
m = _nm(KIND)
for _ in range(300): m.step_frame()
start_stage(m, g, it, log=lambda *a: None)
for pc in (0xD6EE, 0x1530E, 0x15EB4): m.add_hook('sub', pc)
auto = shoot_rec.Auto(seed, 0.8, 0.1)
s = m.sub
hruns = []; sounds = []
for f in range(1, frames + 1):
    pad = auto(s, f)
    m.inputs = [[1, pad]]
    m.step_frame()
    h = sum(1 for _, pc, _r in m.hits if pc == 0xD6EE)
    hruns.append(h)
    for _, pc, r in m.hits:
        if pc == 0x1530E: sounds.append((f, 'pcm', r['d0'] & 0xFFFF))
        elif pc == 0x15EB4: sounds.append((f, 'fm/voice', r['d0'] & 0xFFFF))
print('frames', frames, 'handler runs per frame histogram', dict(collections.Counter(hruns)))
base = collections.Counter(hruns)
# handler-less frames within 8 frames after each sound start
for kind in ('pcm', 'fm/voice'):
    ev = [x for x in sounds if x[1] == kind]
    miss = []
    for f, _, i in ev:
        w = hruns[f - 1:f + 7]
        miss.append(sum(1 for x in w if x == 0))
    if ev: print('%-9s starts %3d  handler-less frames in the 8 frames from the start: mean %.2f  max %d  (all frames: %.3f)' % (kind, len(ev), statistics.mean(miss), max(miss), hruns.count(0) / len(hruns)))
for f, k, i in sounds[:12]: print('  f%4d %s id %x  handler runs f..f+5: %s' % (f, k, i, hruns[f - 1:f + 5]))
