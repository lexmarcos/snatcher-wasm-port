#!/usr/bin/env python3
"""AUDIO-VERIFY: drive FM sounds through the ORIGINAL driver and record its YM writes.

usage: .venv/bin/python -I audio_inject.py <out.txt> [period] [first-frame] [--vol] [--ids LO-HI]
                                           [--sfxonly] [--seed N]

Boots the game with no input (it idles on the title screen after the title music 0x3F),
then from <first-frame> on injects one FM request every <period> frames into the driver's
request queue exactly like fm_request M:0xFF5058 does (queue $FF7F00[$FF7F20++], wraps at
0x20; ids 0x32-0x3E also update the priority bytes $FF7F22/$FF7F23), so fm_update M:0xFF5106
starts it on the next VBlank. Order: each music id LO..HI (default 3F-7A), and half a period
later one sfx id (0x01-0x3E cycling), then the sfx not yet used (every half period), then F2
(stop music) at the end. Output format = audio_ym.py plus `I id accepted` lines.

  --vol      also drive the volume word M:$FF7F5E (normally COMSTAT1 from the sub CPU's ducking /
             fade curve, copied by M:0xFF4FB0) through ramps and pseudo-random values in
             300-frame segments; forced at each fm_tick entry (so it is constant during a tick,
             as in the game) and logged as `V` 
  --ids a-b  restrict the music ids (hex), e.g. 4f-4f
  --cmds     command mix: per music id (period frames): id, F3 (mono) at +20 %, sfx at +30 %, F4
             (stereo) at +50 %, F2 (stop music) at +70 %, id again at +80 %, F1 (stop all) at +95 %
  --burst    queue/priority test: groups of 2-4 requests injected in the SAME frame (priority sfx
             0x35 = 32, 0x3D = 17, 0x32/33/.. = 16 in all orders, plus music and F-commands),
             300 frames apart, repeated `period` times over several music ids
  --sfxonly  no music: sfx 01-3E one per half period, preceded by F2 (stop music)
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

def vol_word(nf, first):
    """Deterministic volume-word schedule (300-frame segments)."""
    seg, i = divmod(max(nf - first, 0), 300)
    kind = seg % 5
    if kind == 0: return 0xFFFF
    if kind == 1: return 0xFF00 | max(0x10, 0xFF - i)                 # low byte ramp down
    if kind == 2: return (max(0x10, 0xFF - i) << 8) | 0xFF           # high byte ramp down
    if kind == 3:
        v = max(0x10, 0xFF - i)
        return (v << 8) | v
    x = (nf * 2654435761) & 0xFFFFFFFF                                 # pseudo-random pairs
    return (((x >> 8) & 0xFE) << 8) | ((x >> 20) & 0xFE) | 0x0101

def main():
    argv = sys.argv[1:]
    vol = '--vol' in argv
    sfxonly = '--sfxonly' in argv
    ids = (0x3F, 0x7A)
    if '--ids' in argv:
        k = argv.index('--ids')
        lo, hi = argv[k + 1].split('-'); ids = (int(lo, 16), int(hi, 16))
        del argv[k + 1]
    args = [a for a in argv if not a.startswith('--')]
    out = args[0]
    period = int(args[1]) if len(args) > 1 else 600
    first = int(args[2]) if len(args) > 2 else 1500
    plan = {}
    f = first
    sfx = list(range(0x01, 0x3F))
    used = set()
    if '--burst' in argv:
        groups = [[0x35, 0x32], [0x32, 0x35], [0x32, 0x3D, 0x33], [0x3D, 0x32, 0x35, 0x33], [0x35, 0x3D, 0x3E],
                  [0x3E, 0x3D, 0x35], [0x36, 0x01, 0x3D, 0x02], [0x3D, 0x32], [0x35, 0x35, 0x3D], [0x32, 0x32]]
        mus = [0x4F, 0x40, 0x50, 0x6F]
        for k in range(period):          # `period` = number of groups
            if k % 10 == 0:
                plan[f] = [mus[(k // 10) % len(mus)]]; f += 200
            plan[f] = list(groups[k % len(groups)]); f += 150
        plan[f] = [0xF1]
        f += 100
    elif '--cmds' in argv:
        for k, mus in enumerate(range(ids[0], ids[1] + 1)):
            for frac, v in ((0, mus), (20, 0xF3), (30, sfx[k % len(sfx)]), (50, 0xF4), (70, 0xF2), (80, mus), (95, 0xF1)):
                plan[f + period * frac // 100] = v
            f += period
    elif not sfxonly:
        for k, mus in enumerate(range(ids[0], ids[1] + 1)):
            plan[f] = mus
            plan[f + period // 2] = sfx[k % len(sfx)]
            used.add(sfx[k % len(sfx)])
            f += period
    else:
        plan[f] = 0xF2; f += period // 2
    for s in sfx:                           # remaining sfx, without music
        if s not in used and '--cmds' not in argv and '--burst' not in argv:
            plan[f] = s; f += period // 2
    plan.setdefault(f, 0xF2)
    end = f + 120
    m = AudioMachine()
    if vol:
        m.vol_fn = lambda: vol_word(m.frame + 1, first) if m.frame + 1 >= first else None
    with open(out, 'w') as fo:
        while m.frame < end:
            nf = m.frame + 1
            if nf in plan:
                for v in (plan[nf] if isinstance(plan[nf], list) else [plan[nf]]):
                    ok = inject(m, v)
                    m.ev.append('I %02x %d' % (v, ok))
            m.ev.append('F %d' % nf)
            m.step_frame(capture=False)
            if len(m.ev) > 20000:
                fo.write('\n'.join(m.ev) + '\n'); m.ev.clear()
        fo.write('\n'.join(m.ev) + '\n')
    print('done', m.frame)

if __name__ == '__main__':
    main()
