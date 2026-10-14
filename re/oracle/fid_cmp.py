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
import argparse, json, os, re, subprocess, sys, difflib, shutil
import numpy as np
from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, '..', '..'))
ap = argparse.ArgumentParser()
ap.add_argument('--name', default='fid')
ap.add_argument('--inputs', default='[]', help='[[rel_frame,bits],..] single presses')
ap.add_argument('--inputs-file', default='', help='read the input list from a JSON file (fid_play.py output)')
ap.add_argument('--sync', action='store_true', help='engine: press relative to its own message/menu readiness (SYNC, entries [rel,pad,kind,since]); screens are compared at drift-corrected frames')
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
ap.add_argument('--jump', default='', help='FIDELITY2: start both sides at SPnn:pc (hex pc of a SP01 CHOICE leaf etc.): oracle fid_common.oracle_jump after --owait idle frames (rel 0 = the jump), engine Engine::jump at frame 0')
ap.add_argument('--refine', type=int, default=0, help='for screens with more than N differing pixels also try engine frames +-12 around the mapped one and report the best offset (needs a second engine run)')
ap.add_argument('--refine-range', type=int, default=12)
ap.add_argument('--rng', action='store_true', help='fix the RANDOM script nodes on both sides (poke $786A..D = 0 each oracle frame, engine FIDRNG)')
ap.add_argument('--skip-engine', action='store_true')
a = ap.parse_args()
if a.jump: a.engine_jump = a.jump; a.ebase = 0; a.eprelude = ''
out = os.path.join(HERE, 'out', 'fid', a.name)
for d in ('oracle', 'engine', 'diff'):
    os.makedirs(os.path.join(out, d), exist_ok=True)
inputs = json.load(open(a.inputs_file)) if a.inputs_file else json.loads(a.inputs)
full_inputs = inputs
inputs = [[e[0], e[1]] for e in inputs]
last = max([i[0] for i in inputs] + [0])
frames = a.frames or last + 300
shots = sorted(set([int(x) for x in a.shots.split(',') if x] + (list(range(0, frames + 1, a.every)) if a.every else [])))
shots = [s for s in shots if s > 0]
PY = os.path.join(HERE, '.venv', 'bin', 'python')

# ---------------- oracle (in-process, same loop as fid_play.py)
if not a.skip_oracle:
    sys.path.insert(0, HERE)
    from machine import Machine
    from fid_common import patch_cdtime
    patch_cdtime()
    if a.oracle_jump and not a.jump:
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
        if a.jump:
            from fid_common import oracle_jump
            oracle_jump(m, a.jump, a.owait)
        else:
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
            while words and words[-1] == 0: words.pop()   # the trace stores the queue up to its zero terminator
            res.append((d['f'] - base, words))
    return res

# ---------------- engine
def parse_engine_passes(logfile, base):
    res = []
    for l in open(logfile):
        m = re.match(r'\s*(\d+) Log\("pass pc=(\w+) q=\[(.*)\]"\)', l)
        if m:
            f = int(m.group(1)) - base
            words = []
            for c, ar in re.findall(r'\((\d+), \[([^\]]*)\]\)', m.group(3)):
                words.append(int(c)); words += [int(x) & 0xFFFF for x in ar.split(',') if x.strip()]
            while words and words[-1] == 0: words.pop()
            res.append((f, words))
    return res

def run_engine(shot_abs):
    """run the engine; shot_abs = {engine absolute frame: name} -> engine/<name>.png"""
    sy = {}
    if a.sync:
        sp = os.path.join(out, 'engine', 'sync.json')
        json.dump(full_inputs, open(sp, 'w')); sy['SYNC'] = sp
    pads = ','.join(([a.eprelude] if a.eprelude and not a.engine_jump else []) + ([] if a.sync else ['%d:%d' % (f + a.ebase, b) for f, b in inputs]))
    env = dict(os.environ, SHOTAT=','.join(str(f) for f in sorted(shot_abs)) or '0', **sy)
    if a.rng: env['FIDRNG'] = '1'
    cmd = [os.path.join(ROOT, 'target', 'release', 'examples', 'run'), str(a.ebase + frames + (0 if not a.sync else 0)), '1000', os.path.join(out, 'engine'), pads]
    if a.engine_jump: cmd.append(a.engine_jump)
    r = subprocess.run(cmd, cwd=ROOT, env=env, check=True, stdout=subprocess.PIPE, text=True)
    open(os.path.join(out, 'engine', 'log.txt'), 'w').write(r.stdout)
    for fn in os.listdir(os.path.join(out, 'engine')):
        m = re.match(r'f(\d+)\.png', fn)
        if m:
            os.replace(os.path.join(out, 'engine', fn), os.path.join(out, 'engine', 'r%05d.png' % shot_abs[int(m.group(1))]) if False else os.path.join(out, 'engine', shot_abs.get(int(m.group(1)), 'x%d' % int(m.group(1))) + '.png'))

shot_map = {}       # oracle rel frame -> engine rel frame (drift corrected with --sync)
if not a.skip_engine:
    subprocess.run(['cargo', 'build', '--release', '-p', 'snengine', '--example', 'run', '-q'], cwd=ROOT, check=True)
    if a.sync:
        run_engine({})      # pass 1: passes only
        ops = [(f, w) for f, w in oracle_passes() if not (len(w) == 0 or w[0] in (0x22, 0x16, 0x05) or w == [0x1C, 0x1D])]
        eps = [(f, w) for f, w in parse_engine_passes(os.path.join(out, 'engine', 'log.txt'), a.ebase) if len(w) > 0]
        key0 = lambda w: ' '.join('%04X' % x for x in w)
        sm0 = difflib.SequenceMatcher(None, [key0(w) for _, w in ops], [key0(w) for _, w in eps], autojunk=False)
        pairs = []
        for blk in sm0.get_matching_blocks():
            for k in range(blk.size): pairs.append((ops[blk.a + k][0] + 1, eps[blk.b + k][0]))   # oracle trace f = frame - 1
        pairs.sort()
        import bisect
        for r_ in shots:
            i = bisect.bisect_right([p_[0] for p_ in pairs], r_) - 1
            drift = (pairs[i][1] - pairs[i][0]) if i >= 0 else 0
            shot_map[r_] = r_ + drift
    else:
        for r_ in shots: shot_map[r_] = r_
    run_engine({v + a.ebase: 'r%05d' % k for k, v in shot_map.items()})

def engine_passes():
    return parse_engine_passes(os.path.join(out, 'engine', 'log.txt'), a.ebase if not a.engine_jump else 0)

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
        if i2 - i1 == j2 - j1:   # paired replacement: where do the two queues differ?
            for k in range(i2 - i1):
                wo, we = op_f[i1 + k][1], ep[j1 + k][1]
                n = next((x for x in range(min(len(wo), len(we))) if wo[x] != we[x]), min(len(wo), len(we)))
                rep.append('      ~ o%d vs e%d: common %d words, then oracle %s | engine %s' % (op_f[i1 + k][0], ep[j1 + k][0], n,
                           ' '.join('%04X' % x for x in wo[n:n + 6]) or '(end)', ' '.join('%04X' % x for x in we[n:n + 6]) or '(end)'))

# ---------------- screens
best = {}
if a.refine and not a.skip_engine:
    # first pass: which screens are bad?
    bad = []
    for sh in shots:
        o_, e_ = os.path.join(out, 'oracle', 'r%05d.png' % sh), os.path.join(out, 'engine', 'r%05d.png' % sh)
        if os.path.exists(o_) and os.path.exists(e_):
            oo, ee = np.asarray(Image.open(o_).convert('RGB')).astype(int), np.asarray(Image.open(e_).convert('RGB')).astype(int)
            if int((np.abs(oo - ee).sum(2) > 24).sum()) > a.refine: bad.append(sh)
    if bad:
        offs = list(range(-a.refine_range, a.refine_range + 1, 2))
        extra = {shot_map[sh] + a.ebase + o: 'x%05d_%+d' % (sh, o) for sh in bad for o in offs}
        keep = {v + a.ebase: 'r%05d' % k for k, v in shot_map.items()}
        shutil.rmtree(os.path.join(out, 'engine', 'keep'), ignore_errors=True)
        os.makedirs(os.path.join(out, 'engine', 'keep'))
        for fn in os.listdir(os.path.join(out, 'engine')):
            if fn.endswith('.png'): shutil.move(os.path.join(out, 'engine', fn), os.path.join(out, 'engine', 'keep', fn))
        run_engine(extra)
        for sh in bad:
            o_ = np.asarray(Image.open(os.path.join(out, 'oracle', 'r%05d.png' % sh)).convert('RGB')).astype(int)
            res = []
            for off in offs:
                fp = os.path.join(out, 'engine', 'x%05d_%+d.png' % (sh, off))
                if os.path.exists(fp):
                    ee = np.asarray(Image.open(fp).convert('RGB')).astype(int)
                    res.append((int((np.abs(o_ - ee).sum(2) > 24).sum()), off))
            if res: best[sh] = min(res)
        for fn in os.listdir(os.path.join(out, 'engine', 'keep')): shutil.move(os.path.join(out, 'engine', 'keep', fn), os.path.join(out, 'engine', fn))
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
    rb = ('  best of +-%d frames: %d px at %+d' % ((a.refine_range,) + best[s])) if s in best else ''
    rep.append('  r%05d diff %6d px (%.1f%%)%s%s' % (s, n, 100.0 * n / (h * w), bb, rb))
    dimg = np.clip(d, 0, 255).astype(np.uint8)
    both = np.concatenate([o[:h, :w].astype(np.uint8), np.full((h, 4, 3), 255, np.uint8), e[:h, :w].astype(np.uint8), np.full((h, 4, 3), 255, np.uint8), np.stack([dimg] * 3, 2)], 1)
    Image.fromarray(both).save(os.path.join(out, 'diff', 'r%05d.png' % s))
open(os.path.join(out, 'report.txt'), 'w').write('\n'.join(rep) + '\n')
print('\n'.join(rep))
