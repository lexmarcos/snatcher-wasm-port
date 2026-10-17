#!/usr/bin/env python3
"""Cycle through every state of the OPTIONS screen in the engine (English and pt-BR pack) and save 3x crops: work/i18n/gfx/shots/options/{en,pt}_NN.png,
cmp_NN.png (side by side).  A light gun is faked (GUN) so that the device row can toggle.  usage: optshots.py [outdir]"""
import os, subprocess, sys
from PIL import Image
ROOT = os.path.join(os.path.dirname(__file__), '..', '..')
RUN = os.path.join(ROOT, 'target', 'release', 'examples', 'run')
OUT = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, 'work/i18n/gfx/shots/options')
R, L, D, U = 8, 4, 2, 1
# (frame of the press, bits, name of the screenshot taken 12 frames later or None)
steps = [(1340, R, 'dev_gun_sel0'), (1364, D, 'cfg0_sel1_gun')]
f = 1388
for k in range(1, 6):
    steps.append((f, R, 'cfg%d_sel1_gun' % k)); f += 24
steps += [(f, D, 'stereo_sel2_gun'), (f + 24, R, 'mono_sel2_gun'), (f + 48, D, 'gunadj_sel3'), (f + 72, D, 'quit_sel4_gun')]
f += 96
for k in range(4): steps.append((f + 12 * k, U, None))
f += 48
steps.append((f, L, 'dev_pad_sel0'))      # JOYPAD ONLY selected
f += 24
steps += [(f, D, 'cfg5_sel1_pad'), (f + 24, D, 'stereo_sel2_pad'), (f + 48, D, 'quit_sel4_pad')]
f += 72
# blue (unselected) controls variants: set a config on row 1, step down to row 2, look, come back
steps.append((f, U, None)); steps.append((f + 12, U, None)); f += 36
for k in range(6):
    steps.append((f, R, None)); steps.append((f + 12, D, 'blue_cfg%d' % ((5 + k + 1) % 6))); steps.append((f + 36, U, None)); f += 60
inputs = '1301:128,' + ','.join('%d:%d' % (fr, b) for fr, b, _ in steps)
shots = [(fr + 12, n) for fr, b, n in steps if n]
total = max(s[0] for s in shots) + 4
def run(pack, d):
    env = dict(os.environ, SHOTAT=','.join(str(s[0]) for s in shots), GUN='100:100:1')
    if pack: env['I18N_PACK'] = os.path.join(ROOT, 'i18n/build/pt-BR.pack')
    os.makedirs(d, exist_ok=True)
    subprocess.run([RUN, str(total), '100000', d, inputs, 'boot'], env=env, cwd=ROOT, capture_output=True, check=True)
tmp = os.path.join(ROOT, 'work/i18n/gfx/optshots_tmp')
run(False, os.path.join(tmp, 'en')); run(True, os.path.join(tmp, 'pt'))
os.makedirs(OUT, exist_ok=True)
for i, (fr, n) in enumerate(shots):
    a = Image.open(os.path.join(tmp, 'en', 'f%06d.png' % fr)).convert('RGB').crop((28, 20, 228, 205))
    b = Image.open(os.path.join(tmp, 'pt', 'f%06d.png' % fr)).convert('RGB').crop((28, 20, 228, 205))
    a = a.resize((a.width * 3, a.height * 3), Image.NEAREST); b = b.resize((b.width * 3, b.height * 3), Image.NEAREST)
    a.save(os.path.join(OUT, 'en_%02d_%s.png' % (i, n))); b.save(os.path.join(OUT, 'pt_%02d_%s.png' % (i, n)))
    c = Image.new('RGB', (a.width * 2 + 6, a.height), (255, 0, 255)); c.paste(a, (0, 0)); c.paste(b, (a.width + 6, 0))
    c.save(os.path.join(OUT, 'cmp_%02d_%s.png' % (i, n)))
print(len(shots), 'shots in', OUT)
