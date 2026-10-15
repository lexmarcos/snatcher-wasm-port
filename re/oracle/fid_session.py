#!/usr/bin/env python3
"""FIDELITY2: one oracle-vs-engine session from a script entry point.
usage (from re/oracle): .venv/bin/python -I fid_session.py NAME SPnn:pc FRAMES [--every K] [--refine PX] [--cycle|--choices 0,1,..] [--skip-play]
 1. fid_play.py --jump SPnn:pc --rng records the adaptive pad script  -> out/fid2/NAME.json, story -> out/fid2/NAME.story.txt
 2. fid_cmp.py --jump ... --rng --sync replays it in both -> out/fid/NAME/{report.txt,diff/}
Prints the queue summary (matching blocks, oracle-only / engine-only passes) of the report."""
import argparse, os, subprocess, sys
HERE = os.path.dirname(os.path.abspath(__file__))
ap = argparse.ArgumentParser()
ap.add_argument('name'); ap.add_argument('jump'); ap.add_argument('frames', type=int)
ap.add_argument('--every', type=int, default=400); ap.add_argument('--refine', type=int, default=400)
ap.add_argument('--choices', default=''); ap.add_argument('--skip-play', action='store_true')
ap.add_argument('--settle', type=int, default=12)
ap.add_argument('--stop-script', default='')
ap.add_argument('--stall', type=int, default=12000)
a = ap.parse_args()
PY = [os.path.join(HERE, '.venv', 'bin', 'python'), '-I']
os.makedirs(os.path.join(HERE, 'out', 'fid2'), exist_ok=True)
js = os.path.join('out', 'fid2', a.name + '.json')
if not a.skip_play:
    cmd = PY + ['fid_play.py', '--jump', a.jump, '--rng', '--frames', str(a.frames), '--out', js, '--settle', str(a.settle), '--stall', str(a.stall)]
    cmd += ['--choices', a.choices] if a.choices else ['--cycle']
    if a.stop_script: cmd += ['--stop-script', a.stop_script]
    r = subprocess.run(cmd, cwd=HERE, stdout=subprocess.PIPE, text=True)
    open(os.path.join(HERE, 'out', 'fid2', a.name + '.story.txt'), 'w').write(r.stdout)
    print(r.stdout.strip().splitlines()[-1])
import json
last = max([e[0] for e in json.load(open(os.path.join(HERE, js)))] + [0])
cmd = PY + ['fid_cmp.py', '--name', a.name, '--jump', a.jump, '--rng', '--sync', '--inputs-file', js, '--frames', str(a.frames), '--every', str(a.every), '--refine', str(a.refine)]
r = subprocess.run(cmd, cwd=HERE, stdout=subprocess.PIPE, text=True)
rep = r.stdout.splitlines()
print(rep[0])
for l in rep:
    if 'ORACLE ONLY' in l or 'ENGINE ONLY' in l or '~' in l[:10]: print(l[:200])
bad = [l for l in rep if 'diff' in l and l.strip().startswith('r') and 'best' in l]
print('screens:', len([l for l in rep if l.strip().startswith('r0')]), 'flagged with best-offset:', len(bad))
