#!/usr/bin/env python3
"""Run a scenario: inputs + named screenshots + named save states.
usage: .venv/bin/python -I scenario.py scenarios/intro.json [--out out/refs] [--vm-trace]
Writes OUT/<name>.png, OUT/trace.jsonl and states/<name>.pkl."""
import argparse, json, os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from oracle_sel import Machine
from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
ap = argparse.ArgumentParser()
ap.add_argument('scenario')
ap.add_argument('--out', default=os.path.join(HERE, 'out', 'refs'))
ap.add_argument('--vm-trace', action='store_true')
a = ap.parse_args()
sc = json.load(open(a.scenario))
os.makedirs(a.out, exist_ok=True)
os.makedirs(os.path.join(HERE, 'states'), exist_ok=True)
m = Machine(trace_path=os.path.join(a.out, 'trace.jsonl'))
if sc.get('load'):
    m.load_state(os.path.join(HERE, 'states', sc['load'] + '.pkl'))
if a.vm_trace:
    m.enable_vm_trace()
m.inputs = [list(x) for x in sc['inputs']]
shots = {v: k for k, v in sc.get('shots', {}).items()}
states = {v: k for k, v in sc.get('states', {}).items()}
end = sc['frames']
while m.frame < end:
    f = m.frame + 1
    m.step_frame(capture=f in shots)
    if f in shots:
        Image.fromarray(m.vdp.frame_image()).save(os.path.join(a.out, shots[f] + '.png'))
    if f in states:
        m.save_state(os.path.join(HERE, 'states', states[f] + '.pkl'))
    if m.main.halted or m.sub.halted:
        print('HALT', m.main.err, m.sub.err)
        break
print('done at frame', m.frame)
