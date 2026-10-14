#!/usr/bin/env python3
"""SHOOTING fidelity: run every stage in the ORIGINAL (oracle-driven pad policy) and replay the pad script in the engine, then compare.
usage (from re/oracle):  .venv/bin/python -I shoot_batch.py [--seeds 1,2] [--frames 2500] [--stages 0:0,0:1,...] [--acc 0.8 --idle 0.1]
Stage items (group:item -> $78E0): 0:0 -> 0, 0:1 -> 0B, 0:2 -> 1, 0:3 -> 2, 0:4 -> 3, 0:5 -> 4, 1:0 -> 5, 1:1 -> 6, 1:2 -> 8, 1:3 -> 7, 1:4 -> 9, 1:5 -> 0A, 2:0 -> 0C, 2:1 -> 0D, 2:2 -> 0E."""
import argparse, os, subprocess, sys
HERE = os.path.dirname(os.path.abspath(__file__))
ENG = os.path.join(HERE, '..', '..', 'target', 'release', 'examples', 'shoot_trace')
ALL = ['0:0', '0:1', '0:2', '0:3', '0:4', '0:5', '1:0', '1:1', '1:2', '1:3', '1:4', '1:5', '2:0', '2:1', '2:2']
ap = argparse.ArgumentParser()
ap.add_argument('--seeds', default='1'); ap.add_argument('--frames', type=int, default=2500)
ap.add_argument('--stages', default=','.join(ALL)); ap.add_argument('--acc', default='0.8'); ap.add_argument('--idle', default='0.1')
ap.add_argument('--min-run', default='4'); ap.add_argument('--objs', action='store_true'); ap.add_argument('--tag', default='')
a = ap.parse_args()
os.makedirs(os.path.join(HERE, 'out', 'shoot'), exist_ok=True)
for st in a.stages.split(','):
    g, i = st.split(':')
    for sd in a.seeds.split(','):
        base = os.path.join(HERE, 'out', 'shoot', 'b%s_g%s_i%s_s%s' % (a.tag, g, i, sd))
        subprocess.run([sys.executable, '-I', os.path.join(HERE, 'shoot_rec.py'), '--group', g, '--item', i, '--auto', sd, '--frames', str(a.frames),
                        '--acc', a.acc, '--idle', a.idle, '--out', base + '.o.jsonl'], check=True, stdout=subprocess.DEVNULL)
        env = dict(os.environ, SCHED=base + '.o.jsonl.sched', SFXLEN=os.path.join(HERE, 'out', 'shoot', 'sfxlen.txt'))
        subprocess.run([ENG, g, i, '@' + base + '.o.jsonl.script.json', str(a.frames), base + '.e.jsonl'], check=True, env=env, stderr=subprocess.DEVNULL)
        r = subprocess.run([sys.executable, '-I', os.path.join(HERE, 'shoot_cmp.py'), base + '.o.jsonl', base + '.e.jsonl', '--min-run', a.min_run] + (['--objs'] if a.objs else []),
                           capture_output=True, text=True)
        print('== stage', st, 'seed', sd); print(r.stdout.strip(), flush=True)
