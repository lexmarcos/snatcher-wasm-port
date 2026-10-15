#!/usr/bin/env python3
"""PCM voice timing on either oracle while a recorded pad script is replayed (as fid_voicelen.py, but hook based and with the sub-CPU stall):
for every voice start (pcm_play_voice S:0x154F8, id >= 36) record the frame, the duration until `$1511E` bit 1 clears (voice stream playing),
and the frame-handler runs (hook S:0xD6EE) in the 12 frames from the start.  Compares with the manifest (assets/audio/web/pcm.json seconds * 60).
usage (from re/oracle): ../oracle2/o2_voice.py KIND SPnn:pc|"" pad.json FRAMES out.json"""
import json, os, sys
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE); sys.path.insert(0, os.path.join(HERE, '..', 'oracle'))
kind, jump, padf, frames, out = sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4]), sys.argv[5]
os.environ['ORACLE'] = '2' if kind == '2' else 'hle'
from o2_hooks import new_machine
from fid_common import oracle_jump, patch_cdtime
patch_cdtime()
m = new_machine(kind)
if jump: oracle_jump(m, jump, 300)
else:
    for _ in range(300): m.step_frame(capture=False)
for pc in (0x154F8, 0xD6EE): m.add_hook('sub', pc)
press = {e[0]: e[1] for e in json.load(open(padf))}
s = m.sub
pcm = json.load(open(os.path.join(HERE, '..', '..', 'assets', 'audio', 'web', 'pcm.json')))['voices']
hr = []; cur = None; res = []
for rel in range(1, frames + 1):
    s.write(0x786A, bytes(4)); m.inputs = [[1, press.get(rel, 0)]]
    m.step_frame(capture=False)
    hr.append(sum(1 for _, pc, _r in m.hits if pc == 0xD6EE))
    for _, pc, r in m.hits:
        if pc == 0x154F8 and (r['d0'] & 0xFFFF) >= 36:
            cur = {'id': r['d0'] & 0xFFFF, 'start': rel, 'end': None}; res.append(cur)
    if cur and cur['end'] is None and rel > cur['start'] + 3 and not (s.r8(0x1511E) & 2): cur['end'] = rel
for c in res:
    c['handler_runs_12'] = hr[c['start'] - 1:c['start'] + 11]
    c['stall12'] = sum(1 for x in c['handler_runs_12'] if x == 0)
    if c['end']:
        c['frames'] = c['end'] - c['start']
        mf = pcm.get(str(c['id']), {}).get('seconds'); c['manifest'] = mf and mf * 60
json.dump(res, open(out, 'w'))
done = [c for c in res if c.get('frames')]
import statistics
print('voices', len(res), 'complete', len(done))
if done:
    d = [c['frames'] - c['manifest'] for c in done if c.get('manifest')]
    print('duration - manifest (frames): mean %.1f min %.1f max %.1f' % (statistics.mean(d), min(d), max(d)))
    print('handler-less frames in the 12 frames after a voice start: mean %.2f min %d max %d' % (statistics.mean(c['stall12'] for c in done), min(c['stall12'] for c in done), max(c['stall12'] for c in done)))
    print('(handler-less frames of the whole run: %.3f)' % (hr.count(0) / len(hr)))
