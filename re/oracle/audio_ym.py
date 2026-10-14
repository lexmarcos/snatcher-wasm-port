#!/usr/bin/env python3
"""AUDIO-VERIFY: record the ORIGINAL FM driver's activity in the oracle.

usage: .venv/bin/python -I audio_ym.py <scenario.json> <frames> <out.txt> [--load state]

Writes one line per event, in execution order:
  F <frame>                      start of a main VBlank frame (oracle frame counter)
  S <id>                         fm_start entry M:0xFF5306 (d0 = sound id)
  C <id>                         command dispatch M:0xFF518C (d0 = 0xF1..)
  T                              fm_tick entry M:0xFF408A
  W <port> <reg> <val> <ppc>     YM2612 data write (ppc = PC of the writing instruction)
  V <word>                       value of the volume word M:$FF7F5E at each tick (diagnostic)
Only writes from driver code (ppc in M:0xFF4000..0xFF6142) are meaningful for kdriver; others
(e.g. boot-time chip init) are kept with their ppc so the comparer can filter them.
"""
import json, os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import machine as M

HOOKS = {0xFF5306: 'S', 0xFF518C: 'C', 0xFF408A: 'T'}

class AudioMachine(M.Machine):
    def __init__(self, *a, **k):
        self.ev = []
        # optional callable() -> int|None: volume word to force at each fm_tick entry (the
        # driver reads M:$FF7F5E during the tick; writing it only at tick entry keeps it
        # constant for the whole tick like the real vint-time COMSTAT1 copy, M:0xFF4FB0)
        self.vol_fn = None
        super().__init__(*a, **k)
        for pc in HOOKS:
            self.lib.core_add_hook(M.MAIN, pc)

    def _main_bios(self, pc):
        t = HOOKS.get(pc)
        if t is None:
            return super()._main_bios(pc)
        if t == 'T':
            if self.vol_fn is not None:
                w = self.vol_fn()
                if w is not None:
                    self.main.w16(0xFF7F5E, w)
            self.ev.append('T')
            self.ev.append('V %04x' % self.main.r16(0xFF7F5E))
        else:
            self.ev.append('%s %02x' % (t, self.main.d(0) & 0xFF))

    def _main_w(self, a, size, v):
        if size == 1 and a in (0xA04001, 0xA04003):
            self.ev.append('W %d %02x %02x %06x' % (self.ym_addr[0], self.ym_addr[1], v & 0xFF,
                                                     self.main.reg(M.R_PPC) & 0xFFFFFF))
        return super()._main_w(a, size, v)

def main():
    sc = json.load(open(sys.argv[1]))
    end = int(sys.argv[2])
    m = AudioMachine()
    if len(sys.argv) > 5 and sys.argv[4] == '--load':
        m.load_state(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'states', sys.argv[5] + '.pkl'))
    m.inputs = [list(x) for x in sc['inputs']]
    with open(sys.argv[3], 'w') as out:
        while m.frame < end:
            m.ev.append('F %d' % (m.frame + 1))
            m.step_frame(capture=False)
            if len(m.ev) > 10000:
                out.write('\n'.join(m.ev) + '\n'); m.ev.clear()
        out.write('\n'.join(m.ev) + '\n')
    print('done', m.frame)

if __name__ == '__main__':
    main()
