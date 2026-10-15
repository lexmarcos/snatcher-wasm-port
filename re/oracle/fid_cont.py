#!/usr/bin/env python3
"""FIDELITY2: CONTINUE round trip. The ORIGINAL (re/py/modules_scn.py, scenarios/fid2_continue.json: two save files written by the original's own
save routine, fid_save.py) and the ENGINE (`run ... boot`, SAVES) both boot, pick CONTINUE slot 2 and run on; prints the first passes after the
continue of both sides and whether the action queues are equal.
usage (from re/oracle): .venv/bin/python -I fid_cont.py [scenario] [n_passes]"""
import json, os, re, subprocess, sys
HERE = os.path.dirname(os.path.abspath(__file__)); ROOT = os.path.abspath(os.path.join(HERE, '..', '..'))
sc = sys.argv[1] if len(sys.argv) > 1 else 'scenarios/fid2_continue.json'; npass = int(sys.argv[2]) if len(sys.argv) > 2 else 12
d = json.load(open(os.path.join(HERE, sc)))
out = os.path.join(HERE, 'out', 'fid2', 'cont')
subprocess.run([os.path.join(HERE, '.venv', 'bin', 'python'), '-I', os.path.join(ROOT, 're', 'py', 'modules_scn.py'), os.path.join(HERE, sc), '--out', out],
               check=True, stdout=subprocess.DEVNULL)
ops = []; started = False
for l in open(os.path.join(out, 'trace.jsonl')):
    e = json.loads(l)
    if e['t'] == 'seq_start':
        hx = e['queue']; w = [int(hx[i:i + 4], 16) for i in range(0, len(hx), 4)]
        while w and w[-1] == 0: w.pop()
        ops.append((e['f'], w))
# the first oracle pass after the title: SP file load (11[1,A800,..]) -> take passes from the first one with cmd 0x11 to A800 after frame 1700
ops = [(f, w) for f, w in ops if f > 1700 and w and w[0] not in (0x22, 0x16, 0x05, 0x23) and w != [0x1C, 0x1D]]
# engine
t = 0; presses = []
for f, b in d['inputs']:
    t += f
    if b: presses.append('%d:%d' % (t, b))
saves = []
for i, k in enumerate(('SNATCHER_01', 'SNATCHER_02', 'SNATCHER_03', 'SNATCHER_04')):
    if k in d['bram']:
        p = os.path.join(out, 'save%d.hex' % i); open(p, 'w').write(d['bram'][k][:0x380]); saves.append('%d:%s' % (i, p))
env = dict(os.environ, SAVES=','.join(saves))
r = subprocess.run([os.path.join(ROOT, 'target', 'release', 'examples', 'run'), str(d['frames']), '100000', '/tmp/fid_cont_eng', ','.join(presses), 'boot'], cwd=ROOT, env=env, stdout=subprocess.PIPE, text=True)
eps = []
for l in r.stdout.splitlines():
    m = re.match(r'\s*(\d+) Log\("pass pc=(\w+) q=\[(.*)\]"\)', l)
    if m:
        words = []
        for c, ar in re.findall(r'\((\d+), \[([^\]]*)\]\)', m.group(3)):
            words.append(int(c)); words += [int(x) & 0xFFFF for x in ar.split(',') if x.strip()]
        while words and words[-1] == 0: words.pop()
        eps.append((int(m.group(1)), words[:64]))
print('oracle passes after continue:', len(ops), ' engine passes:', len(eps))
for i in range(max(min(npass, len(ops)), 0)):
    fo, wo = ops[i]; wo = wo[:64]
    fe, we = eps[i] if i < len(eps) else (None, [])
    print('%2d o%6d e%6s %s  %s' % (i, fo, fe, 'SAME' if wo == we else 'DIFF', ' '.join('%X' % x for x in wo)[:150]))
    if wo != we: print('            engine: ' + ' '.join('%X' % x for x in we)[:150])
