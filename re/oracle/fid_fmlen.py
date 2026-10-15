#!/usr/bin/env python3
"""FIDELITY2: durations of FM sounds in the ORIGINAL: frames from the sequencer command (07 music / 1C sfx) until the done flag
`$980E` / `$980F` (S:0x1011E / 0x1013E, cleared by the main CPU driver's report, S:0xDF52) is 0 again.
usage (from re/oracle): .venv/bin/python -I fid_fmlen.py out/fid2/fmlen.json [ids...]
Compares with assets/audio/web/fm.json (engine `fm_len` = ceil(seconds * 60))."""
import json, os, sys
HERE = os.path.dirname(os.path.abspath(__file__)); sys.path.insert(0, HERE)
from machine import Machine
from fid_common import oracle_jump, patch_cdtime
patch_cdtime()
ROOT = os.path.join(HERE, '..', '..')
man = json.load(open(os.path.join(ROOT, 'assets', 'audio', 'web', 'fm.json')))['sounds']
m = Machine(); m.load_state(os.path.join(HERE, 'states', 'junker_hq.pkl'))
oracle_jump(m, 'SP01:0', 300)
for _ in range(500): m.step_frame(capture=False)       # debug menu idle
s = m.sub
ids = [int(x) for x in sys.argv[2:]] or list(range(1, 0x7B))
res = {}
def run(cmd, i, flag, limit):
    s.write(0x9940, bytes([0, cmd, 0, i, 0, 0]))
    s.w16(0x9B40, 0); s.w8(0x9B44, 1)
    n = 0; seen = False
    while n < limit:
        m.step_frame(capture=False); n += 1
        v = s.r8(flag)
        if v and not seen: seen = True; t0 = n
        if seen and v == 0: return n - t0 + 1, t0
    return None, (t0 if seen else None)
for i in ids:
    if i < 0x3F: cmd, flag, lim = 0x1C, 0x980F, 900
    else: cmd, flag, lim = 0x07, 0x980E, 2600
    mf = man.get(str(i), {})
    if cmd == 0x07 and mf.get('loop_start') is not None:
        res[i] = {'oracle': None, 'manifest_frames': None, 'loops': True}; continue
    d, t0 = run(cmd, i, flag, lim)
    res[i] = {'oracle': d, 'start_delay': t0, 'manifest_frames': int(-(-mf.get('seconds', 0) * 60 // 1)) if mf else None}
    print(hex(i), res[i], flush=True)
    if cmd == 0x07: run(0x07, 0xFF, 0x980E, 5)            # stop the music
    else: run(0x1C, 0x3D, 0x980F, 5)                      # stop the sfx (a looping one would block the next)
    for _ in range(30): m.step_frame(capture=False)
json.dump(res, open(sys.argv[1], 'w'), indent=1)
