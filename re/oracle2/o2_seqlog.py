#!/usr/bin/env python3
"""Per-command timing of the sequencer (S:0xFFC4 seq_run, hooks) on either oracle while a recorded pad script is replayed
(fid_play.py output: [[rel, pad, kind, since], ...]; oracle_jump start like the fid tools).
usage (from re/oracle): .venv/bin/python -I ../oracle2/o2_seqlog.py KIND SPnn:pc pad.json FRAMES out.json
KIND = hle | 2.  Output rows: [rel_frame_of_first_run, queue_serial, cmd, args(hex words), frames_until_next_command, vm_script]
(a command is 'done' when seq_run first runs with a different (queue serial, index); the last command of a queue ends with the next queue).
The queue serial counts queues started (S:$9B44 0 -> FF edge while the sequencer runs)."""
import json, os, sys
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE); sys.path.insert(0, os.path.join(HERE, '..', 'oracle'))
from o2_hooks import new_machine
kind, jump, padf, frames, out = sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4]), sys.argv[5]
os.environ['ORACLE'] = '2' if kind == '2' else 'hle'
from fid_common import oracle_jump, patch_cdtime
patch_cdtime()
m = new_machine(kind)
oracle_jump(m, jump, 300)
m.add_hook('sub', 0xFFC4)
press = {e[0]: e[1] for e in json.load(open(padf))}
s = m.sub
rows = []; cur = None; serial = 0; prev_busy = 0
NARGS = {0x08: 2, 0x07: 1, 0x0D: 1, 0x0C: 1, 0x0E: 1, 0x10: 1, 0x12: 1, 0x1B: 3, 0x1C: 1, 0x20: 1, 0x11: 3, 0x09: 2, 0x01: 2, 0x03: 2, 0x22: 2, 0x05: 2, 0x15: 2}
for rel in range(1, frames + 1):
    s.write(0x786A, bytes(4))
    m.inputs = [[1, press.get(rel, 0)]]
    m.step_frame(capture=False)
    for _, pc, r in m.hits:
        idx = s.r16(0x9B40)
        cmd = s.r16(0x9940 + idx)
        key = (serial, idx)
        if cur is None or cur['key'] != key:
            if cur: cur['dur'] = rel - cur['rel']; rows.append(cur)
            n = NARGS.get(cmd, 0)
            cur = {'rel': rel, 'key': key, 'cmd': cmd, 'args': [s.r16(0x9940 + idx + 2 + 2 * k) for k in range(n)], 'script': s.r16(0x980C), 'dur': None}
    busy = s.r8(0x9B44)
    if busy and not prev_busy: serial += 1
    prev_busy = busy
if cur: cur['dur'] = None; rows.append(cur)
json.dump([{k: v for k, v in r.items() if k != 'key'} | {'serial': r['key'][0]} for r in rows], open(out, 'w'))
print(len(rows), 'commands')
