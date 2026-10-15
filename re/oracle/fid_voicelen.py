#!/usr/bin/env python3
"""FIDELITY2: PCM voice durations in the ORIGINAL vs the manifest: replays a recorded session (pad json), hooks pcm_play_voice (S:0x154F8, d0 = id) and watches
`$1511E` bit 1 (voice stream playing). usage: fid_voicelen.py SPnn:pc pad.json frames out.json"""
import sys, os, json
HERE = os.path.dirname(os.path.abspath(__file__)); sys.path.insert(0, HERE)
import machine as M
from fid_common import oracle_jump, patch_cdtime
patch_cdtime()
class HM(M.Machine):
    def __init__(self, *a, **k):
        super().__init__(*a, **k); self.lib.core_add_hook(M.SUB, 0x154F8); self.starts = []
    def _sub_bios(self, pc):
        if pc == 0x154F8: self.starts.append((self.frame, self.sub.d(0) & 0xFFFF)); return
        return super()._sub_bios(pc)
m = HM(); m.load_state(os.path.join(HERE, 'states', 'junker_hq.pkl'))
oracle_jump(m, sys.argv[1], 300)
press = {e[0]: e[1] for e in json.load(open(sys.argv[2]))}
base = m.frame; res = []; cur = None
for rel in range(1, int(sys.argv[3]) + 1):
    m.sub.write(0x786A, bytes(4)); m.inputs = [[1, press.get(rel, 0)]]
    n0 = len(m.starts)
    m.step_frame(capture=False)
    if len(m.starts) > n0:
        f, i = m.starts[-1]
        if i >= 36: cur = [i, rel, None]; res.append(cur)
    if cur and cur[2] is None and rel > cur[1] + 3 and not (m.sub.r8(0x1511E) & 2): cur[2] = rel
pcm = json.load(open(os.path.join(HERE, '..', '..', 'assets', 'audio', 'web', 'pcm.json')))['voices']
out = []
for i, a, b in res:
    if b is None: continue
    mf = pcm.get(str(i), {}).get('seconds')
    out.append({'id': i, 'start': a, 'frames': b - a, 'manifest': mf and mf * 60})
json.dump(out, open(sys.argv[4], 'w'))
for o in out[:60]: print(o['id'], o['frames'], round(o['manifest'] or 0, 1), round(o['frames'] - (o['manifest'] or 0), 1))
