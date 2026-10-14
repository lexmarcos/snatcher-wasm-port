#!/usr/bin/env python3
"""SHOOTING fidelity: record per-frame shooting state of the ORIGINAL.
usage (from re/oracle):
  .venv/bin/python -I shoot_rec.py --group 0 --item 0 --script '[[100,64],[130,32]]' --frames 600 --out out/shoot/rec_a1_0.jsonl [--shots 200,300]
  --script  [[frame, bits(, len)],..]  pad bits for `len` frames (default 1) starting at frame `frame` after the stage start
  --gun     [[frame, x, y(, trig, aux)],..]  light-gun script (needs the gun model, see machine.py `gun`)
Frame 0 = the frame in which the shooting tick ran its first step ($78E8 = 0, $78EA = 1).  $786A..D is pinned to 0 before every frame
(the engine side pins the same, `Engine::fid_rng`).  Output: JSON lines {f, w:{addr:word}, o:{slot:[...]}}.
Disc data is read as data only."""
import argparse, json, os, sys
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from shoot_common import *

WORDS = list(range(0x78E0, 0x7950, 2)) + [0x7970, 0x7972, 0x7984, 0x7986, 0x7988, 0x798A, 0x7990, 0x7992]
SLOTS = list(range(21, 25)) + list(range(26, 32)) + list(range(32, 41)) + [48, 56, 57, 58, 59]


def snap(s):
    w = {'%04X' % a: s.r16(a) for a in WORDS}
    w['78AF'] = s.r8(0x78AF)
    o = {}
    for n in SLOTS:
        b = 0x7A00 + n * 0x50
        o[str(n)] = [s.r16(b), s.r8(b + 0x25), s.r16(b + 6), s.r16(b + 0xA), s.r16(b + 0x28), s.r8(b + 0x17), s.r8(b + 0x24), s.r16(b + 0x2C)]
    return {'w': w, 'o': o}


def expand(script):
    d = {}
    for it in script:
        f, b = it[0], it[1]
        n = it[2] if len(it) > 2 else 1
        for k in range(n):
            d[f + k] = b
    return d


class Auto:
    """Oracle-driven pad policy (decides from the state after the previous frame): draw the blaster when the play phase starts, aim at a
    hittable target cell (D-pad cell = held directions), fire after a short delay, sometimes shoot at empty cells / civilians, sometimes
    idle so that enemies attack.  The produced script is replayed in the engine."""
    def __init__(self, seed, acc=0.8, idle=0.1):
        import random
        self.r = random.Random(seed); self.acc = acc; self.idle = idle
        self.cell = None; self.wait = 0; self.fire_in = None; self.cool = 0

    def dirs(self, c):
        b = 0
        col, row = c % 3, c // 3
        if col == 0: b |= LEFT
        if col == 2: b |= RIGHT
        if row == 0: b |= UP
        if row == 2: b |= DOWN
        return b

    def __call__(self, s, f):
        w = lambda a: s.r16(a)
        ph = w(0x78E8)
        bits = 0
        if self.cell is not None:
            bits |= self.dirs(self.cell)
        if ph not in (2, 4):
            return bits
        if w(0x78F8) == 0 and w(0x78F8) != 1 and self.cool <= 0:
            self.cool = 6
            return A
        self.cool -= 1
        if w(0x78F8) & 1 and w(0x78F8) < 2:
            if self.fire_in is not None:
                self.fire_in -= 1
                if self.fire_in <= 0:
                    self.fire_in = None
                    bits |= C
            elif self.wait <= 0:
                # pick a new cell
                live = [k for k in range(9) if (s.r8(0x7A00 + (32 + k) * 0x50 + 0x24) & 0xF) in (2, 3, 4)]
                u = self.r.random()
                if u < self.idle:
                    self.cell = None; self.wait = self.r.randint(10, 60)
                elif live and u < self.idle + self.acc:
                    self.cell = self.r.choice(live); self.fire_in = self.r.randint(2, 9); self.wait = self.fire_in + self.r.randint(8, 20)
                else:
                    self.cell = self.r.randrange(9); self.fire_in = self.r.randint(2, 9); self.wait = self.fire_in + self.r.randint(8, 20)
            else:
                self.wait -= 1
        return bits | (self.dirs(self.cell) if self.cell is not None else 0)


def record(m, script, frames, rng=True, shots=(), outdir=None, gun=None, log=None, auto=None):
    from PIL import Image
    pad = expand(script)
    s = m.sub
    # sub-CPU timing: S:0xD6EE (frame handler) entries per frame (a PCM sample copy S:0x1530E/0x15C32 can overrun a frame: 0 or 2 entries)
    # and the sound calls subfn_06 S:0x1530E (PCM effect, d0) / subfn_12 S:0x15EB4 (PCM/voice with done flag, d0)
    hooks = {0x1530E: '06', 0x15EB4: '12', 0xD6EE: 'h', 0xD644: 'rng'}
    ev = []
    sched = ['0 1 0 0 0']
    cnt = [0]   # handler runs since the recording started (engine: Engine::ix.rng_seq)
    orig = m._sub_bios

    def hk(pc):
        if pc == 0xD644:
            # S:0xD644 = the RNG step at the start of every frame handler run: pin the state before it (a handler overrun by a PCM sample copy
            # S:0x15C32 straddles frames, so a per-frame poke would not equal the engine's per-tick pin)
            if rng:
                cnt[0] += 1
                s.write(0x786A, ((cnt[0] * 2654435761) & 0xFFFFFFFF).to_bytes(4, 'big'))
        elif pc == 0xD6EE:
            # logical pad bytes this handler run works with (sampled earlier in the handler: S:0xD664 / S:0xDFD6)
            ev.append(('h', s.r8(0x7990), s.r8(0x7991), s.r8(0x7992)))
        elif pc in hooks:
            ev.append((hooks[pc], s.d(0) & 0xFFFF))
        else:
            orig(pc)
    m._sub_bios = hk
    for a in hooks:
        m.lib.core_add_hook(1, a)
    recs = [dict(f=0, h=1, s=[], **snap(s))]
    for f in range(1, frames + 1):
        if gun is not None:
            gun(m, f)
        if auto is not None:
            pad[f] = auto(s, f)
        m.inputs = [[1, pad.get(f, 0)]]
        ev.clear()
        m.step_frame(capture=(f in shots))
        h = sum(1 for x in ev if x[0] == 'h')
        snd = [list(x) for x in ev if x[0] != 'h']
        sched.append('%d %d %s' % (f, h, ' '.join('%d %d %d' % x[1:] for x in ev if x[0] == 'h')))
        if f in shots:
            Image.fromarray(m.vdp.frame_image()).save(os.path.join(outdir, 'o%05d.png' % f))
        recs.append(dict(f=f, h=h, s=snd, **snap(s)))
        if m.main.halted or m.sub.halted:
            print('HALT', m.main.err, m.sub.err); break
    m._sub_bios = orig
    record.pad = pad
    record.sched = sched
    return recs


if __name__ == '__main__':
    ap = argparse.ArgumentParser()
    ap.add_argument('--group', type=int, default=0)
    ap.add_argument('--item', type=int, default=0)
    ap.add_argument('--state', default='', help='stage-start state saved with shoot_common.py (default: navigate)')
    ap.add_argument('--script', default='[]')
    ap.add_argument('--frames', type=int, default=600)
    ap.add_argument('--out', required=True)
    ap.add_argument('--shots', default='')
    ap.add_argument('--norng', action='store_true')
    ap.add_argument('--auto', type=int, default=None, help='seed of the oracle-driven pad policy (script written to OUT.script.json)')
    ap.add_argument('--acc', type=float, default=0.8)
    ap.add_argument('--idle', type=float, default=0.1)
    a = ap.parse_args()
    m = new_machine()
    if a.state:
        m.load_state(a.state)
    else:
        sp = os.path.join(HERE, 'out', 'shoot', 'st_g%d_i%d.pkl' % (a.group, a.item))
        if os.path.exists(sp):
            m.load_state(sp)
        else:
            start_stage(m, a.group, a.item)
            os.makedirs(os.path.dirname(sp), exist_ok=True)
            m.save_state(sp)
    outdir = os.path.dirname(os.path.abspath(a.out)); os.makedirs(outdir, exist_ok=True)
    auto = Auto(a.auto, a.acc, a.idle) if a.auto is not None else None
    recs = record(m, json.loads(a.script), a.frames, rng=not a.norng, shots=set(int(x) for x in a.shots.split(',') if x), outdir=outdir, auto=auto)
    open(a.out + '.sched', 'w').write('\n'.join(record.sched) + '\n')
    if auto is not None:
        json.dump([[f, b] for f, b in sorted(record.pad.items()) if b], open(a.out + '.script.json', 'w'))
    with open(a.out, 'w') as fh:
        for r in recs:
            fh.write(json.dumps(r) + '\n')
    s = m.sub
    print('frames', len(recs) - 1, 'E8=%x hits=%d miss=%d life=%d' % (s.r16(0x78E8), s.r16(0x790C), s.r16(0x790E), s.r16(0x7910)))
