"""Print oracle VDP registers / scroll at given frames: modules_vdpregs.py F1,F2,... [inputs] [--load st]"""
import sys, os, json
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'oracle'))
from machine import Machine
fr = [int(x) for x in sys.argv[1].split(',')]
inputs = json.loads(sys.argv[2]) if len(sys.argv) > 2 and not sys.argv[2].startswith('--') else []
m = Machine(trace_path=None)
if '--load' in sys.argv: m.load_state(sys.argv[sys.argv.index('--load') + 1])
m.inputs = [list(x) for x in inputs]
while m.frame < max(fr):
    m.step_frame(capture=False)
    if m.frame in fr:
        v = m.vdp
        print(m.frame, 'regs', ' '.join('%02x' % r for r in v.reg[:24]))
        print('   vsram', v.vsram[:4], 'cram0', ['%03x' % c for c in v.cram[:16]])
