#!/usr/bin/env python3
"""FIDELITY harness: play the ORIGINAL (oracle) and the ENGINE with the same sparse input script and
compare per VM pass the action queues and, at chosen frames, the screens.

usage (from re/oracle):
  .venv/bin/python -I fid_cmp.py --name walk1 --inputs '[[200,32],[150,32],...]' --shots 100,200 [--every 50]
inputs: list of [rel_frame, padbits] (pad held for ONE frame), rel_frame counted from the anchor.
Anchor: oracle = save state `states/junker_hq.pkl` (+ --wait frames, default 0: Mika dialogue in progress,
the command menu comes up by rel ~250); engine = new game run for --ebase frames (default 1530 =
"menu opened" log, see `fid_anchor`) with NO input.  To keep both on the same story point, the oracle
first plays --owait frames idle (default 300) and the engine starts its inputs at --ebase frames; the
rel frames of --inputs / --shots count from there.
--oracle-jump SPnn:pc / --engine-jump SPnn:pc[:item,..] start both from a jump instead (interact_jump).
Outputs in out/fid/<name>/: oracle/*.png, engine/*.png, diff/*.png (oracle | engine | |diff|), report.txt
Evidence of every difference must be traced in the disassembly (rule 1); this tool only finds them."""
import argparse, json, os, re, subprocess, sys, difflib
import numpy as np
from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, '..', '..'))
ap = argparse.ArgumentParser()
ap.add_argument('--name', default='fid')
ap.add_argument('--inputs', default='[]', help='[[rel_frame,bits],..] single presses')
ap.add_argument('--frames', type=int, default=0, help='rel frames to run (default: last input + 300)')
ap.add_argument('--shots', default='', help='comma list of rel frames')
ap.add_argument('--every', type=int, default=0)
ap.add_argument('--owait', type=int, default=300, help='oracle: idle frames after loading the state before rel 0')
ap.add_argument('--ebase', type=int, default=1590, help='engine: absolute frame of rel 0 (new game)')
ap.add_argument('--oracle-jump', default='', help='interact_jump args "--script SP04 --pc 0"')
ap.add_argument('--engine-jump', default='')
ap.add_argument('--eprelude', default='1540:32', help='engine: absolute-frame presses before rel 0 (dismiss the first Mika message)')
ap.add_argument('--load', default='junker_hq')
ap.add_argument('--skip-oracle', action='store_true')
ap.add_argument('--rng', action='store_true', help='fix the RANDOM script nodes on both sides (poke $786A..D = 0 each oracle frame, engine FIDRNG)')
ap.add_argument('--skip-engine', action='store_true')
a = ap.parse_args()
out = os.path.join(HERE, 'out', 'fid', a.name)
for d in ('oracle', 'engine', 'diff'):
    os.makedirs(os.path.join(out, d), exist_ok=True)
inputs = json.loads(a.inputs)
last = max([i[0] for i in inputs] + [0])
frames = a.frames or last + 300
shots = sorted(set([int(x) for x in a.shots.split(',') if x] + (list(range(0, frames + 1, a.every)) if a.every else [])))
shots = [s for s in shots if s > 0]
PY = os.path.join(HERE, '.venv', 'bin', 'python')

# ---------------- oracle (in-process, same loop as fid_play.py)
if not a.skip_oracle:
    sys.path.insert(0, HERE)
    from machine import Machine
    if a.oracle_jump:
        seq, t = [], 0
        for f, b in sorted(inputs):
            seq.append([f - t - 1, 0]); seq.append([1, b]); t = f
        cmd = [PY, '-I', 'interact_jump.py'] + a.oracle_jump.split() + ['--load', a.load, '--wait', str(a.owait), '--frames', str(frames),
               '--inputs', json.dumps(seq), '--shot-at', ','.join(map(str, shots)), '--shot-every', '0', '--out', os.path.join(out, 'oracle')]
        subprocess.run(cmd, cwd=HERE, check=True, stdout=subprocess.DEVNULL)
        for fn in os.listdir(os.path.join(out, 'oracle')):
            m_ = re.match(r'f(\d+)\.png', fn)
            if m_: os.replace(os.path.join(out, 'oracle', fn), os.path.join(out, 'oracle', 'r%05d.png' % int(m_.group(1))))
    else:
        m = Machine(trace_path=os.path.join(out, 'oracle', 'trace.jsonl'))
        m.load_state(os.path.join(HERE, 'states', a.load + '.pkl'))
        m.inputs = []
        for _ in range(a.owait): m.step_frame(capture=False)
        base0 = m.frame
        press = dict((f, b) for f, b in inputs)
        shotset = set(shots)
        for rel in range(1, frames + 1):
            if a.rng: m.sub.write(0x786A, bytes(4))
            m.inputs = [[1, press.get(rel, 0)]]
            m.step_frame(capture=rel in shotset)
            if rel in shotset:
                Image.fromarray(m.vdp.frame_image()).save(os.path.join(out, 'oracle', 'r%05d.png' % rel))
            if m.main.halted or m.sub.halted: print('HALT'); break

def oracle_passes():
    base = 4500 + a.owait if not a.oracle_jump else 0
    res = []
    p = os.path.join(out, 'oracle', 'trace.jsonl')
    for l in open(p):
        d = json.loads(l)
        if d['t'] == 'seq_start':
            hx = d['queue']
            words = [int(hx[i:i + 4], 16) for i in range(0, len(hx), 4)]
            res.append((d['f'] - base, words))
    return res

# ---------------- engine
if not a.skip_engine:
    subprocess.run(['cargo', 'build', '--release', '-p', 'snengine', '--example', 'run', '-q'], cwd=ROOT, check=True)
    pads = ','.join(([a.eprelude] if a.eprelude and not a.engine_jump else []) + ['%d:%d' % (f + a.ebase, b) for f, b in inputs])
    env = dict(os.environ, SHOTAT=','.join(str(s + a.ebase) for s in shots) or '0')
    if a.rng: env['FIDRNG'] = '1'
    cmd = [os.path.join(ROOT, 'target', 'release', 'examples', 'run'), str(a.ebase + frames), '1000', os.path.join(out, 'engine'), pads]
    if a.engine_jump: cmd.append(a.engine_jump)
    r = subprocess.run(cmd, cwd=ROOT, env=env, check=True, stdout=subprocess.PIPE, text=True)
    open(os.path.join(out, 'engine', 'log.txt'), 'w').write(r.stdout)
    for fn in os.listdir(os.path.join(out, 'engine')):
        m = re.match(r'f(\d+)\.png', fn)
        if m:
            os.replace(os.path.join(out, 'engine', fn), os.path.join(out, 'engine', 'r%05d.png' % (int(m.group(1)) - a.ebase)))

def engine_passes():
    base = a.ebase if not a.engine_jump else 0
    res = []
    for l in open(os.path.join(out, 'engine', 'log.txt')):
        m = re.match(r'\s*(\d+) Log\("pass pc=(\w+) q=\[(.*)\]"\)', l)
        if m:
            f = int(m.group(1)) - base
            words = []
            for c, ar in re.findall(r'\((\d+), \[([^\]]*)\]\)', m.group(3)):
                words.append(int(c)); words += [int(x) & 0xFFFF for x in ar.split(',') if x.strip()]
            res.append((f, words))
    return res

def fmt(w): return ' '.join('%X' % x if i == 0 else '%X' % x for i, x in enumerate(w))
rep = []
op, ep = oracle_passes(), engine_passes()
# the engine does not log menu-driven commands (1C) in the queue; drop single-command 1C passes from the oracle list
def menu_noise(w):   # menu-bar / window redraw passes: the engine draws these natively (menus.rs), not via the queue
    cmds = []; i = 0
    return len(w) == 0 or w[0] in (0x22, 0x16, 0x05) or w == [0x1C, 0x1D]
op_f = [(f, w) for f, w in op if not menu_noise(w)]
ep = [(f, w) for f, w in ep if len(w) > 0]
key = lambda w: ' '.join('%04X' % x for x in w)
sm = difflib.SequenceMatcher(None, [key(w) for _, w in op_f], [key(w) for _, w in ep], autojunk=False)
rep.append('pass queues: oracle %d, engine %d, matching blocks %d' % (len(op_f), len(ep), sum(b.size for b in sm.get_matching_blocks())))
for tag, i1, i2, j1, j2 in sm.get_opcodes():
    if tag == 'equal':
        for k in range(i2 - i1):
            fo, fe = op_f[i1 + k][0], ep[j1 + k][0]
            rep.append('  = o%6d e%6d (d %+5d)  %s' % (fo, fe, fe - fo, key(op_f[i1 + k][1])[:100]))
    else:
        for k in range(i1, i2): rep.append('  - o%6d              ORACLE ONLY  %s' % (op_f[k][0], key(op_f[k][1])[:140]))
        for k in range(j1, j2): rep.append('  + e%6d              ENGINE ONLY  %s' % (ep[k][0], key(ep[k][1])[:140]))

# ---------------- screens
def load(p): return np.asarray(Image.open(p).convert('RGB')).astype(int) if os.path.exists(p) else None
rep.append('screens (differing pixels of 57344; mean abs diff)')
for s in shots:
    o, e = load(os.path.join(out, 'oracle', 'r%05d.png' % s)), load(os.path.join(out, 'engine', 'r%05d.png' % s))
    if o is None or e is None:
        rep.append('  r%05d missing %s' % (s, 'oracle' if o is None else 'engine')); continue
    h = min(o.shape[0], e.shape[0]); w = min(o.shape[1], e.shape[1])
    d = np.abs(o[:h, :w] - e[:h, :w]).sum(2)
    n = int((d > 24).sum())
    ys, xs = np.nonzero(d > 24)
    bb = ' bbox x%d-%d y%d-%d (scene %d, window %d)' % (xs.min(), xs.max(), ys.min(), ys.max(), int((ys < 136).sum()), int((ys >= 136).sum())) if n else ''
    rep.append('  r%05d diff %6d px (%.1f%%)%s' % (s, n, 100.0 * n / (h * w), bb))
    dimg = np.clip(d, 0, 255).astype(np.uint8)
    both = np.concatenate([o[:h, :w].astype(np.uint8), np.full((h, 4, 3), 255, np.uint8), e[:h, :w].astype(np.uint8), np.full((h, 4, 3), 255, np.uint8), np.stack([dimg] * 3, 2)], 1)
    Image.fromarray(both).save(os.path.join(out, 'diff', 'r%05d.png' % s))
open(os.path.join(out, 'report.txt'), 'w').write('\n'.join(rep) + '\n')
print('\n'.join(rep))
