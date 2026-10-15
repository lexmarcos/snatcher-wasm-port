#!/usr/bin/env python3
"""SHOOTING fidelity: screenshots of the ORIGINAL and the engine at the same frames of an oracle-driven session, pixel diff + side-by-side sheet.
usage (from re/oracle):  .venv/bin/python -I shoot_shots.py --group 0 --item 0 --seed 1 --frames 700 --shots 3,60,140,200,300,400,600
Output: out/shoot/shots_gG_iI/{oNNNNN,eNNNNN,dNNNNN}.png (oracle | engine | diff) and a summary of differing pixels per frame."""
import argparse, json, os, subprocess, sys
HERE = os.path.dirname(os.path.abspath(__file__))
from PIL import Image, ImageChops
ENG = os.path.join(HERE, '..', '..', 'target', 'release', 'examples', 'shoot_trace')
ap = argparse.ArgumentParser()
ap.add_argument('--group', default='0'); ap.add_argument('--item', default='0'); ap.add_argument('--seed', default='1')
ap.add_argument('--frames', type=int, default=700); ap.add_argument('--shots', default='3,60,140,200,300,400,600')
a = ap.parse_args()
d = os.path.join(HERE, 'out', 'shoot', 'shots_g%s_i%s' % (a.group, a.item)); os.makedirs(d, exist_ok=True)
base = os.path.join(d, 'run')
subprocess.run([sys.executable, '-I', os.path.join(HERE, 'shoot_rec.py'), '--group', a.group, '--item', a.item, '--auto', a.seed, '--frames', str(a.frames),
                '--shots', a.shots, '--out', base + '.o.jsonl'], check=True, stdout=subprocess.DEVNULL)
env = dict(os.environ, SCHED=base + '.o.jsonl.sched', SFXLEN=os.path.join(HERE, 'out', 'shoot', 'sfxlen.txt'))
subprocess.run([ENG, a.group, a.item, '@' + base + '.o.jsonl.script.json', str(a.frames), base + '.e.jsonl', a.shots], check=True, env=env, stderr=subprocess.DEVNULL)
for f in [int(x) for x in a.shots.split(',')]:
    o = Image.open(os.path.join(d, 'o%05d.png' % f)).convert('RGB')
    e = Image.open(base + '.e.e%05d.png' % f).convert('RGB')
    df = ImageChops.difference(o, e)
    n = sum(1 for p in df.getdata() if p != (0, 0, 0))
    sheet = Image.new('RGB', (o.width * 3, o.height))
    sheet.paste(o, (0, 0)); sheet.paste(e, (o.width, 0)); sheet.paste(df.point(lambda v: 255 if v else 0), (o.width * 2, 0))
    sheet.save(os.path.join(d, 'sheet%05d.png' % f))
    print('frame %4d: %5d of %d pixels differ' % (f, n, o.width * o.height))
