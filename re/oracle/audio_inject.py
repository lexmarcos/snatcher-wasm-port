#!/usr/bin/env python3
"""AUDIO-VERIFY: drive every FM sound through the ORIGINAL driver and record its YM writes.

usage: .venv/bin/python -I audio_inject.py <out.txt> [period] [first-frame]

Boots the game with no input (it idles on the title screen after the title music 0x3F),
then from <first-frame> on injects one FM request every <period> frames into the driver's
request queue exactly like fm_request M:0xFF5058 does (queue $FF7F00[$FF7F20++], wraps at
0x20; ids 0x32–0x3E also update the priority bytes $FF7F22/$FF7F23), so fm_update M:0xFF5106
starts it on the next VBlank. Order: each music id 0x3F–0x7A, and half a period later one
sfx id (0x01–0x3E cycling), then F2 (stop music) at the end. Output format = audio_ym.py.
"""
import os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from audio_ym import AudioMachine

def inject(m, d0):
    rd = m.main.r8
    if 0x32 <= d0 < 0x3F:
        prio = rd(0xFF6142 + d0 * 8 + 3)
        if prio < rd(0xFF7F22):
            return False
        m.main.w8(0xFF7F22, prio)
        m.main.w8(0xFF7F23, rd(0xFF7F20))
    i = rd(0xFF7F20)
    m.main.w8(0xFF7F00 + i, d0)
    m.main.w8(0xFF7F20, (i + 1) & 0x1F if i + 1 != 0x20 else 0)
    return True

def main():
    out = sys.argv[1]
    period = int(sys.argv[2]) if len(sys.argv) > 2 else 600
    first = int(sys.argv[3]) if len(sys.argv) > 3 else 1500
    plan = {}
    f = first
    sfx = list(range(0x01, 0x3F))
    for k, mus in enumerate(range(0x3F, 0x7B)):
        plan[f] = mus
        plan[f + period // 2] = sfx[k % len(sfx)]
        f += period
    for k in range(60, len(sfx)):          # remaining sfx, without music
        plan[f] = sfx[k]; f += period // 2
    plan[f] = 0xF2
    end = f + 120
    m = AudioMachine()
    with open(out, 'w') as fo:
        while m.frame < end:
            nf = m.frame + 1
            if nf in plan:
                ok = inject(m, plan[nf])
                m.ev.append('I %02x %d' % (plan[nf], ok))
            m.ev.append('F %d' % nf)
            m.step_frame(capture=False)
            if len(m.ev) > 20000:
                fo.write('\n'.join(m.ev) + '\n'); m.ev.clear()
        fo.write('\n'.join(m.ev) + '\n')
    print('done', m.frame)

if __name__ == '__main__':
    main()
