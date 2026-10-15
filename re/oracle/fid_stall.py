#!/usr/bin/env python3
"""FIDELITY2: frames in which the ORIGINAL's sub frame handler (S:0xD6EE) does not run while a PCM voice streams: inject [0D n 0A] from the idle SP01 menu and
list the handler-less frames relative to the command. usage: fid_stall.py n [n..]"""
import sys, os
HERE = os.path.dirname(os.path.abspath(__file__)); sys.path.insert(0, HERE)
import machine as M
from fid_common import oracle_jump, patch_cdtime
patch_cdtime()
class HM(M.Machine):
    def __init__(self, *a, **k):
        super().__init__(*a, **k); self.lib.core_add_hook(M.SUB, 0xD6EE); self.run = set()
    def _sub_bios(self, pc):
        if pc == 0xD6EE: self.run.add(self.frame); return
        return super()._sub_bios(pc)
m = HM(); m.load_state(os.path.join(HERE, 'states', 'junker_hq.pkl'))
oracle_jump(m, 'SP01:0', 300)
for _ in range(500): m.step_frame(capture=False)
s = m.sub
for n in [int(x) for x in sys.argv[1:]]:
    for _ in range(120): m.step_frame(capture=False)
    s.write(0x9940, b''.join(x.to_bytes(2, 'big') for x in (0x0D, n, 0x0A, 0)))
    s.w16(0x9B40, 0); s.w8(0x9B44, 1)
    t0 = m.frame; k = 0
    while s.r8(0x9B44) and k < 2500: m.step_frame(capture=False); k += 1
    miss = [f - t0 for f in range(t0 + 1, m.frame + 1) if f not in m.run]
    print('voice', n, 'queue frames', k, 'handler-less frames (rel to start):', miss)
