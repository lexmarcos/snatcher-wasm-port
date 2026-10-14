#!/usr/bin/env python3
"""AUDIO-VERIFY: record all reference logs of the ORIGINAL FM driver into assets/audio/ref/.

usage (from re/oracle, needs the oracle venv + core/build, see docs/re/oracle.md):
    .venv/bin/python -I audio_ref.py [outdir] [--quick]

Writes (format: audio_ym.py docstring; replayed by core/src/audio/verify.rs):
  intro_ym.txt     title screen -> options -> intro -> Junker HQ (music 3F, 38, 4F for ~2450
                   frames, menu sfx 1D) = the real game flow, scenarios/intro.json, 6600 frames
  inject600.txt    every music id 3F-7A (600 frames each) + one sfx between them, then the
                   remaining sfx (audio_inject.py 600)
  inject3000.txt   every music id for 3000 frames (50 s: past the first loop of most songs)
  inject_vol.txt   like inject600 with 1500 frames per song and the volume word M:$FF7F5E
                   swept through ramps/pseudo-random values (--vol)
  cmds.txt         music 3F-7A with the command ids F3 (mono), F4, F2, F1 and an sfx in between
  burst.txt        several requests in one frame (queue order and the priority gate of 0x32-0x3E)
  sfx_only.txt     every FM sfx 01-3E alone (no music), 150 frames apart
--long additionally records long_N.txt: every song for 6000 frames (100 s, covers the tail of the long songs), 4 parallel jobs (~10 min). --quick only records intro_ym.txt and inject600.txt (~2 min). Everything runs in parallel.
The logs are gitignored (they describe the original code's behaviour, not shipped).
"""
import os, subprocess, sys
here = os.path.dirname(os.path.abspath(__file__))
args = [a for a in sys.argv[1:] if not a.startswith('--')]
out = os.path.abspath(args[0]) if args else os.path.join(here, '..', '..', 'assets', 'audio', 'ref')
os.makedirs(out, exist_ok=True)
py = sys.executable
jobs = [
    [py, '-I', 'audio_ym.py', 'scenarios/intro.json', '6600', os.path.join(out, 'intro_ym.txt')],
    [py, '-I', 'audio_inject.py', os.path.join(out, 'inject600.txt'), '600', '1500'],
]
if '--quick' not in sys.argv:
    jobs += [
        [py, '-I', 'audio_inject.py', os.path.join(out, 'inject3000.txt'), '3000', '1500'],
        [py, '-I', 'audio_inject.py', os.path.join(out, 'inject_vol.txt'), '1500', '1500', '--vol'],
        [py, '-I', 'audio_inject.py', os.path.join(out, 'sfx_only.txt'), '300', '1500', '--sfxonly'],
        [py, '-I', 'audio_inject.py', os.path.join(out, 'cmds.txt'), '1000', '1500', '--cmds'],
        [py, '-I', 'audio_inject.py', os.path.join(out, 'burst.txt'), '60', '1500', '--burst'],
    ]
if '--long' in sys.argv:
    for n, (lo, hi) in enumerate([(0x3F, 0x4D), (0x4E, 0x5C), (0x5D, 0x6B), (0x6C, 0x7A)]):
        jobs.append([py, '-I', 'audio_inject.py', os.path.join(out, 'long_%d.txt' % n), '6000', '1500', '--ids', '%x-%x' % (lo, hi)])
procs = [subprocess.Popen(j, cwd=here) for j in jobs]
rc = [p.wait() for p in procs]
print('exit codes', rc)
sys.exit(1 if any(rc) else 0)
