#!/usr/bin/env python3
"""Replay a recorded fid_play script on either oracle with the presses synchronised to the GAME state instead of absolute frames: each entry
[rel, pad, kind, since] is pressed when its kind (m = message waiting for confirm, u = menu key loop, e = text entry) has been ready for `since` frames
(same readiness tests as fid_play.py). Both oracles then receive the same inputs in game terms, so their sequencer queues can be compared with o2_cmpq.py.
usage (from re/oracle): ../oracle2/o2_syncplay.py KIND pad.json FRAMES trace_out.jsonl [--jump SPnn:pc]"""
import argparse, json, os, sys
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE); sys.path.insert(0, os.path.join(HERE, '..', 'oracle'))
ap = argparse.ArgumentParser()
ap.add_argument('kind'); ap.add_argument('pad'); ap.add_argument('frames', type=int); ap.add_argument('trace')
ap.add_argument('--jump', default=''); ap.add_argument('--load', default='junker_hq'); ap.add_argument('--wait', type=int, default=300)
a = ap.parse_args()
os.environ['ORACLE'] = '2' if a.kind == '2' else 'hle'
from oracle_sel import Machine
from fid_common import oracle_jump, patch_cdtime
patch_cdtime()
m = Machine(trace_path=a.trace)
m.load_state(os.path.join(HERE, '..', 'oracle', 'states', a.load + '.pkl'))
m.inputs = []
if a.jump: oracle_jump(m, a.jump, a.wait)
else:
    for _ in range(a.wait): m.step_frame(capture=False)
s = m.sub
script = json.load(open(a.pad)); k = 0
ready_since = {}; last_press = -100; ecount = 0; unpressed_since = 0
for rel in range(1, a.frames + 1):
    busy = s.r8(0x9B44)
    msg_ready = s.r8(0x9740) == 5 and s.r8(0x9720) == 0 and busy
    menu_ready = s.r8(0x9853) > 0 and s.r8(0x7A25) == 0 and s.r16(0x788C) != 0 and s.r8(0x79F4) == 0 and s.r8(0x9B44) == 0 and s.r8(0x7A00 + 1) == 1
    entry_ready = s.r8(0x79FF) in (2, 3) and s.r8(0x7892) == 0x0D + s.r8(0x79FF) and s.r8(0x9B44) == 0 and s.r8(0x79F4) == 0
    for key, v in (('m', msg_ready), ('u', menu_ready), ('e', entry_ready)):
        ready_since[key] = (ready_since.get(key) if ready_since.get(key) is not None else rel) if v else None
    pad = 0
    if k < len(script) and rel - last_press > 8:
        _, p, kind, since = script[k][:4]
        rs = ready_since.get(kind)
        if rs is not None and rel - rs >= since:
            pad = p; last_press = rel; k += 1
    m.inputs = [[1, pad]]
    s.write(0x786A, bytes(4))
    m.step_frame(capture=False)
print('presses applied', k, 'of', len(script))
