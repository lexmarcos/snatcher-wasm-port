"""Watch memory values in the oracle per frame (prints on change).
usage: re/oracle/.venv/bin/python -I re/py/modules_watch.py FRAMES 'S:7842:w,S:7896:w,M:E022:w' [inputs-json] [--load state.pkl]"""
import sys, os, json
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'oracle'))
from machine import Machine
n = int(sys.argv[1]); specs = sys.argv[2].split(',')
inputs = json.loads(sys.argv[3]) if len(sys.argv) > 3 and not sys.argv[3].startswith('--') else []
m = Machine(trace_path=None)
if '--load' in sys.argv: m.load_state(sys.argv[sys.argv.index('--load') + 1])
m.inputs = [list(x) for x in inputs]
def rd(spec):
    cpu, a, sz = spec.split(':'); a = int(a, 16)
    if cpu == 'S': buf, o = m.prg, a
    elif cpu == 'W': buf, o = m.wram, a
    else: buf, o = m.mram, a & 0xFFFF
    if sz == 'b': return buf[o]
    if sz == 'w': return buf[o] << 8 | buf[o + 1]
    return buf[o] << 24 | buf[o + 1] << 16 | buf[o + 2] << 8 | buf[o + 3]
last = None
for i in range(n):
    m.step_frame(capture=False)
    v = tuple(rd(s) for s in specs)
    if v != last:
        print(m.frame, ' '.join('%s=%x' % (s.rsplit(':', 1)[0], x) for s, x in zip(specs, v)))
        last = v
