#!/usr/bin/env python3
"""Write "bank id frames" lines (PCM effect lengths, as web/src/main.ts computes them) from assets/audio/web/pcm.json for the engine tracer:
python3 -I shoot_sfxlen.py > out/shoot/sfxlen.txt   (pcm.json is generated from the user's disc, never committed)"""
import json, math, os
d = json.load(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..', 'assets', 'audio', 'web', 'pcm.json')))
ln = lambda seg: math.ceil(seg['samples'] / seg['rate'] * 60)
for sid, s in d['sfx'].items():
    if s['src'] == 'drum':
        seg = d['drum'].get(s['offset'])
        if seg:
            for b in d['banks']:
                print(b, sid, ln(seg))
    else:
        for b, segs in d['banks'].items():
            seg = segs.get(s['offset'])
            if seg:
                print(b, sid, ln(seg))
