#!/usr/bin/env python3
"""Real-BIOS check of what the screen shows while a voice clip plays (docs/voice-inventory.md).
Jumps to SPnn:pc on oracle2 (Genesis Plus GX + real Sega CD BIOS, as the other o2_* tools), presses C every 40 frames, and for every voice start
(pcm_play_voice S:0x154F8, id >= 36) saves a screenshot `v<id>.png` at `delay` frames after the request (the real BIOS needs ~38 frames to start the stream).
usage (from re/oracle): ../oracle2/o2_voicebox.py SPnn:pc FRAMES outdir [delay=90] [scene=HHII]
  scene: optional DATA scene (hex word: DATA file index << 8 | image) queued before the jump is not supported on the real BIOS path;
         give a pc whose root runs its own SCENE node before the voice (e.g. SP06:032e)."""
import json, os, sys
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE); sys.path.insert(0, os.path.join(HERE, '..', 'oracle'))
jump, frames, out = sys.argv[1], int(sys.argv[2]), sys.argv[3]
delay = int(sys.argv[4]) if len(sys.argv) > 4 else 90
os.environ['ORACLE'] = '2'
os.makedirs(out, exist_ok=True)
from o2_hooks import new_machine
from fid_common import oracle_jump, patch_cdtime
from PIL import Image
patch_cdtime()
m = new_machine('2')
oracle_jump(m, jump, 300)
m.add_hook('sub', 0x154F8)
s = m.sub
shots = []          # (frame to shoot, id)
log = []
for rel in range(1, frames + 1):
    s.write(0x786A, bytes(4))
    m.inputs = [[1, 0x20 if rel % 40 == 0 else 0]]
    m.step_frame(capture=False)
    for _, pc, r in m.hits:
        if pc == 0x154F8 and (r['d0'] & 0xFFFF) >= 36:
            vid = r['d0'] & 0xFFFF
            shots.append((rel + delay, vid)); log.append({'id': vid, 'start': rel})
    for sh in [x for x in shots if x[0] <= rel]:
        shots.remove(sh)
        Image.fromarray(m.vdp.frame_image()).save(os.path.join(out, 'v%d.png' % sh[1]))
json.dump(log, open(os.path.join(out, 'voices.json'), 'w'))
print('voices started:', [(x['id'], x['start']) for x in log])
