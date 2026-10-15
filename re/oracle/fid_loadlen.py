#!/usr/bin/env python3
"""FIDELITY2: frames the ORIGINAL's sequencer command 11 (file load, S:0x102C8) takes, injected from the idle SP01 debug menu:
queue [11, hi, lo, file] -> frames until the sequencer is idle ($9B44 == 0). usage: fid_loadlen.py out.json [files hex...]"""
import json, os, sys
HERE = os.path.dirname(os.path.abspath(__file__)); sys.path.insert(0, HERE)
from machine import Machine
from fid_common import oracle_jump, patch_cdtime
patch_cdtime()
RAW = os.path.join(HERE, '..', '..', 'assets', 'raw')
sizes = {}
for line in open(os.path.join(RAW, 'filelist.tsv')).read().splitlines()[1:]:
    n, lba, size, _ = line.split('\t'); sizes[n] = int(size)
m = Machine(); m.load_state(os.path.join(HERE, 'states', 'junker_hq.pkl'))
oracle_jump(m, 'SP01:0', 300)
for _ in range(500): m.step_frame(capture=False)
s = m.sub
files = [int(x, 16) for x in sys.argv[2:]] or list(range(0x39, 0x5F)) + list(range(0x01, 0x39))
names = [l.split('\t')[0] for l in open(os.path.join(RAW, 'filelist.tsv')).read().splitlines()[1:]]
res = {}
RAWF = os.path.join(RAW, 'files'); heads = {}
for fn in os.listdir(RAWF):
    if fn.upper().endswith('.BIN'):
        with open(os.path.join(RAWF, fn), 'rb') as fh: heads[fn] = fh.read(64)
for f in files:
    dst = (1, 0xA800) if f >= 0x39 and f <= 0x5E else (2, 0x8000)
    s.write(0x9940, b''.join(x.to_bytes(2, 'big') for x in (0x11, dst[0], dst[1], f, 0)))
    s.w16(0x9B40, 0); s.w8(0x9B44, 1)
    n = 0
    while s.r8(0x9B44) and n < 3000: m.step_frame(capture=False); n += 1
    base = dst[1] if dst[0] == 1 else 0x28000
    head = bytes(s.read(base, 64)); nm = next((k for k, v in heads.items() if v == head), '?')
    res[f] = {'frames': n, 'name': nm, 'size': sizes.get(nm)}
    print(hex(f), res[f], flush=True)
json.dump(res, open(sys.argv[1], 'w'), indent=1)
