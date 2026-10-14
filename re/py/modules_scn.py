"""Oracle scenario runner with a CD relative-time model (MODULES workstream).

The stock oracle answers CDBSTAT with `$787A = 0xFFFFFFFF` (never updated), so every
CD-time-synchronised module (DATA_D1/D0 intro, DATA_Y16) stalls at its first timed entry
(`cmp.l $787A,d1`, S:0x29276 etc.).  This runner patches `Machine._cdbios` in-process
(no oracle file is modified) so that CDBSTAT+0x0C = relative track time in BCD
`MM SS FF 00` = frames since MSCPLAY * 75/60 (HYPOTHESIS, consistent with the timeline
tables: DATA_D1 last entry 02:12:40 ~ end of track 3).

usage: modules_scn.py scenario.json [--out DIR] [--log-regs]
scenario.json = same format as scenario.py (frames, inputs, shots, states, load).
Extra scenario keys: "force_display": true (apply the display-on hypothesis), "watch": [[cpu,addr,n,name]...] (print on change), "bram": {name: hex} (internal backup RAM files), "reglog": true -> print VDP reg 12 / reg 0x10 changes per frame.
"""
import argparse, json, os, struct, sys
HERE = os.path.dirname(os.path.abspath(__file__))
ORA = os.path.join(HERE, '..', 'oracle')
sys.path.insert(0, ORA)
from machine import Machine, bcd
from PIL import Image

ap = argparse.ArgumentParser()
ap.add_argument('scenario')
ap.add_argument('--out', default=os.path.join(ORA, 'out', 'modules'))
a = ap.parse_args()
sc = json.load(open(a.scenario))
os.makedirs(a.out, exist_ok=True)

orig = Machine._cdbios
def cdbios(self):
    orig(self)
    s = self.sub
    if (s.d(0) & 0xFFFF) == 0x81 and self.music:
        n = max(0, (self.frame - self.music['start_frame']) * 75 // 60)
        rel = (bcd(n // 4500) << 24) | (bcd((n // 75) % 60) << 16) | (bcd(n % 75) << 8)
        blk = s.a(0)
        s.write(blk + 0x0C, struct.pack('>I', rel))
Machine._cdbios = cdbios

m = Machine(trace_path=os.path.join(a.out, 'trace.jsonl'))
if sc.get('load'):
    m.load_state(os.path.join(ORA, 'states', sc['load'] + '.pkl'))
m.inputs = [list(x) for x in sc['inputs']]
# "bram": {"SNATCHER_01": "<hex>"} pre-populates the internal backup RAM (CONTINUE slots)
if sc.get('bram') and not hasattr(m, 'bram_files'): m.bram_files = {}
for name, hx in sc.get('bram', {}).items():
    m.bram_files[name] = bytes.fromhex(hx).ljust(896, b'\0')
shots = {}
for k, v in sc.get('shots', {}).items(): shots.setdefault(v, []).append(k)
every = sc.get('shot_every', 0)
states = {v: k for k, v in sc.get('states', {}).items()}
from_frame = sc.get('shot_from', 0)
last = None
force = sc.get('force_display', False)
# "watch": [["sub"|"main", "ADDR", bytes, "name"], ...] -> print on change (frame, name, hex value)
watch = [(w[0], int(w[1], 16), int(w[2]), w[3]) for w in sc.get('watch', [])]
wlast = {}
while m.frame < sc['frames']:
    f = m.frame + 1
    if force and m.main.r8(0xFFE06D) == 0 and m.main.r8(0xFFE06C) == 0 and not (m.main.r8(0xFFEF03) & 0x40) and m.main.r8(0xFFE023) == 3:
        # HYPOTHESIS (docs/re/modules.md 3.2): no H-INT mode ($E06C == 0) -> display on again
        m.main.w8(0xFFEF03, m.main.r8(0xFFEF03) | 0x40); m.vdp.reg[1] |= 0x40
    cap = f in shots or bool(every and f % every == 0 and f >= from_frame)
    m.step_frame(capture=cap)
    if cap:
        im = Image.fromarray(m.vdp.frame_image())
        for n in shots.get(f, ['f%05d' % f]): im.save(os.path.join(a.out, n + '.png'))
    if f in states: m.save_state(os.path.join(ORA, 'states', states[f] + '.pkl'))
    for cpu, wa, n, name in watch:
        c = m.sub if cpu == 'sub' else m.main
        v = int.from_bytes(bytes(c.r8(wa + i) for i in range(n)), 'big')
        if wlast.get(name) != v:
            print('frame', f, name, '%0*x' % (2 * n, v)); wlast[name] = v
    if sc.get('reglog'):
        cur = (m.vdp.reg[12], m.vdp.reg[16], m.vdp.reg[11])
        if cur != last:
            print('frame', f, 'reg12=%02x reg16=%02x reg11=%02x' % cur, 'music', m.music and m.music['track'])
            last = cur
    if m.main.halted or m.sub.halted:
        print('HALT', m.main.err, m.sub.err); break
print('done at frame', m.frame)
